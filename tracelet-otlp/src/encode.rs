use std::time::{SystemTime, UNIX_EPOCH};

use prost::Message;
use tracelet_core::SpanRecord;

use crate::proto::{
    any_value, AnyValue, ExportTraceServiceRequest, InstrumentationScope, KeyValue, Resource,
    ResourceSpans, ScopeSpans, Span,
};

pub fn encode_export_request(service_name: &str, records: &[SpanRecord]) -> Vec<u8> {
    let spans = records.iter().map(span_from_record).collect();

    let request = ExportTraceServiceRequest {
        resource_spans: vec![ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_attribute("service.name", service_name)],
            }),
            scope_spans: vec![ScopeSpans {
                scope: Some(InstrumentationScope {
                    name: "tracelet".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                }),
                spans,
            }],
        }],
    };

    request.encode_to_vec()
}

fn span_from_record(record: &SpanRecord) -> Span {
    Span {
        trace_id: record.trace_id.to_be_bytes().to_vec(),
        span_id: record.span_id.to_be_bytes().to_vec(),
        parent_span_id: record
            .parent_id
            .map(|p| p.to_be_bytes().to_vec())
            .unwrap_or_default(),
        name: record.name.clone(),
        start_time_unix_nano: unix_nanos(record.start),
        end_time_unix_nano: record.end.map(unix_nanos).unwrap_or(0),
        attributes: record
            .attributes
            .iter()
            .map(|(k, v)| string_attribute(k, v))
            .collect(),
    }
}

fn string_attribute(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

fn unix_nanos(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::any_value::Value;

    fn record() -> SpanRecord {
        SpanRecord {
            trace_id: 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10,
            span_id: 0x1112_1314_1516_1718,
            parent_id: Some(0x2122_2324_2526_2728),
            name: "do_work".to_string(),
            start: UNIX_EPOCH + std::time::Duration::from_nanos(1_000),
            end: Some(UNIX_EPOCH + std::time::Duration::from_nanos(2_000)),
            attributes: vec![("iteration".to_string(), "3".to_string())],
        }
    }

    #[test]
    fn round_trips_through_the_wire_format() {
        let bytes = encode_export_request("test-service", &[record()]);
        let decoded = ExportTraceServiceRequest::decode(bytes.as_slice()).unwrap();

        let span = &decoded.resource_spans[0].scope_spans[0].spans[0];
        assert_eq!(span.trace_id, record().trace_id.to_be_bytes().to_vec());
        assert_eq!(span.span_id, record().span_id.to_be_bytes().to_vec());
        assert_eq!(span.parent_span_id, 0x2122_2324_2526_2728u64.to_be_bytes().to_vec());
        assert_eq!(span.name, "do_work");
        assert_eq!(span.start_time_unix_nano, 1_000);
        assert_eq!(span.end_time_unix_nano, 2_000);
        assert_eq!(span.attributes[0].key, "iteration");
        assert!(matches!(
            &span.attributes[0].value.as_ref().unwrap().value,
            Some(Value::StringValue(v)) if v == "3"
        ));

        let resource_attrs = &decoded.resource_spans[0].resource.as_ref().unwrap().attributes;
        assert_eq!(resource_attrs[0].key, "service.name");
    }
}
