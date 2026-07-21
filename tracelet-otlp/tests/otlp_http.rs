//! No local Jaeger/Tempo available in this environment, so this stands in
//! for "spans show up in a real OTLP collector": a minimal HTTP/1.1 receiver
//! captures the raw bytes tracelet-otlp POSTs and decodes them with the same
//! protobuf schema a real collector would use.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

use prost::Message;
use tracelet_core::{generate_trace_id, RingBuffer, SpanRecord};
use tracelet_otlp::proto::any_value::Value;
use tracelet_otlp::proto::ExportTraceServiceRequest;
use tracelet_otlp::{spawn_exporter, OtlpExporterConfig};

#[test]
fn exports_captured_span_via_otlp_http() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock receiver");
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let body = read_http_request_body(&mut stream);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n");
            let _ = tx.send(body);
        }
    });

    let buffer = Arc::new(RingBuffer::new(16));
    buffer.push(sample_record());

    spawn_exporter(
        buffer,
        OtlpExporterConfig {
            endpoint: format!("http://{addr}/v1/traces"),
            service_name: "otlp-test-service".to_string(),
            batch_max_spans: 512,
            flush_interval: Duration::from_millis(50),
        },
    );

    let body = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("no export request received from tracelet-otlp");
    let decoded =
        ExportTraceServiceRequest::decode(body.as_slice()).expect("response body was not valid OTLP protobuf");

    let resource_spans = &decoded.resource_spans[0];
    let service_name_attr = &resource_spans.resource.as_ref().unwrap().attributes[0];
    assert_eq!(service_name_attr.key, "service.name");
    assert!(matches!(
        &service_name_attr.value.as_ref().unwrap().value,
        Some(Value::StringValue(v)) if v == "otlp-test-service"
    ));

    let span = &resource_spans.scope_spans[0].spans[0];
    assert_eq!(span.name, "do_work");
    assert_eq!(span.trace_id.len(), 16, "trace_id must be the OTLP-mandated 16 bytes");
    assert_eq!(span.span_id.len(), 8, "span_id must be the OTLP-mandated 8 bytes");
    assert!(span.parent_span_id.is_empty(), "root span must have an empty parent_span_id");
    assert_eq!(span.attributes[0].key, "iteration");
}

fn sample_record() -> SpanRecord {
    SpanRecord {
        trace_id: generate_trace_id(),
        span_id: 42,
        parent_id: None,
        name: "do_work".to_string(),
        start: SystemTime::now(),
        end: Some(SystemTime::now()),
        attributes: vec![("iteration".to_string(), "0".to_string())],
    }
}

fn read_http_request_body(stream: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];

    let header_end = loop {
        let n = stream.read(&mut chunk).expect("read headers");
        assert!(n > 0, "connection closed before headers completed");
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
    };

    let headers = String::from_utf8_lossy(&buf[..header_end]);
    let content_length: usize = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().to_string())
        })
        .and_then(|v| v.parse().ok())
        .expect("missing content-length header");

    while buf.len() < header_end + content_length {
        let n = stream.read(&mut chunk).expect("read body");
        assert!(n > 0, "connection closed before body completed");
        buf.extend_from_slice(&chunk[..n]);
    }

    buf[header_end..header_end + content_length].to_vec()
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
