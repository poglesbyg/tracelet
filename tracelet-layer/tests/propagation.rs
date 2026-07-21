//! Proves the two propagation building blocks work together: a client-side
//! span's context can be formatted into a `traceparent`, and a server-side
//! span seeded from that header (via the #[instrument(fields(x = Empty))] +
//! .record() pattern real axum handlers use) ends up sharing the same trace,
//! correctly parented, with the reserved fields stripped from its exported
//! attributes.

use std::sync::Arc;

use tracelet_core::{format_traceparent, parse_traceparent, RingBuffer};
use tracelet_layer::CaptureLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

#[test]
fn propagates_trace_id_across_a_simulated_network_hop() {
    let buffer = Arc::new(RingBuffer::new(16));
    let layer = CaptureLayer::new(buffer.clone());
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let client_span = tracing::info_span!("client_handler");
        let header = {
            let _guard = client_span.enter();
            tracelet_core::context::current()
                .map(|(trace_id, span_id)| format_traceparent(trace_id, span_id))
                .expect("context::current() should see the entered client span")
        };
        drop(client_span);

        let (remote_trace_id, remote_parent_id) = parse_traceparent(&header).expect("valid header");

        let server_span = tracing::info_span!(
            "server_handler",
            otel_remote_trace_id = tracing::field::Empty,
            otel_remote_parent_span_id = tracing::field::Empty,
        );
        {
            let _guard = server_span.enter();
            server_span.record("otel_remote_trace_id", format!("{remote_trace_id:032x}").as_str());
            server_span.record(
                "otel_remote_parent_span_id",
                format!("{remote_parent_id:016x}").as_str(),
            );

            let (live_trace_id, _) = tracelet_core::context::current().unwrap();
            assert_eq!(
                live_trace_id, remote_trace_id,
                "current() must reflect the .record() call made after this span was entered"
            );
        }
        drop(server_span);
    });

    let records = buffer.drain();
    assert_eq!(records.len(), 2);

    let client = records.iter().find(|r| r.name == "client_handler").unwrap();
    let server = records.iter().find(|r| r.name == "server_handler").unwrap();

    assert_eq!(client.trace_id, server.trace_id, "trace id must propagate across the hop");
    assert_eq!(
        server.parent_id,
        Some(client.span_id),
        "server span's parent must be the client span"
    );
    assert!(
        server
            .attributes
            .iter()
            .all(|(k, _)| k != "otel_remote_trace_id" && k != "otel_remote_parent_span_id"),
        "reserved propagation fields must not leak into exported attributes: {:?}",
        server.attributes
    );
}
