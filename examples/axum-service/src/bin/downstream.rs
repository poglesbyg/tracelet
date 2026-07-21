use std::time::Duration;

use axum::extract::Request;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use tracelet::TracerConfig;

#[tokio::main]
async fn main() {
    tracelet::init(TracerConfig {
        service_name: "downstream-service".to_string(),
        otlp_endpoint: std::env::var("TRACELET_OTLP_ENDPOINT").ok(),
        ..Default::default()
    })
    .expect("failed to init tracelet");

    let app = Router::new().route("/work", get(handle_work));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:4001").await.unwrap();
    println!("downstream-service listening on http://127.0.0.1:4001");
    axum::serve(listener, app).await.unwrap();
}

// otel_remote_trace_id / otel_remote_parent_span_id must be spelled out as
// literal identifiers here -- #[instrument]'s `fields(...)` needs them at
// compile time, so they can't be substituted from the
// tracelet::REMOTE_*_FIELD constants. They must match those constants'
// string values exactly (checked at runtime by `.record()` below).
#[tracing::instrument(
    skip(request),
    fields(otel_remote_trace_id = tracing::field::Empty, otel_remote_parent_span_id = tracing::field::Empty)
)]
async fn handle_work(request: Request) -> impl IntoResponse {
    let remote_context = request
        .headers()
        .get("traceparent")
        .and_then(|v| v.to_str().ok())
        .and_then(tracelet::parse_traceparent);

    if let Some((trace_id, parent_span_id)) = remote_context {
        let span = tracing::Span::current();
        span.record(tracelet::REMOTE_TRACE_ID_FIELD, format!("{trace_id:032x}").as_str());
        span.record(
            tracelet::REMOTE_PARENT_SPAN_ID_FIELD,
            format!("{parent_span_id:016x}").as_str(),
        );
    }

    tokio::time::sleep(Duration::from_millis(30)).await;
    "downstream work done"
}
