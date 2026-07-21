use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use tracelet::TracerConfig;

#[derive(Clone)]
struct AppState {
    http: reqwest::Client,
}

#[tokio::main]
async fn main() {
    tracelet::init(TracerConfig {
        service_name: "edge-service".to_string(),
        otlp_endpoint: std::env::var("TRACELET_OTLP_ENDPOINT").ok(),
        ..Default::default()
    })
    .expect("failed to init tracelet");

    let state = AppState {
        http: reqwest::Client::new(),
    };
    let app = Router::new().route("/edge", get(handle_edge)).with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:4000").await.unwrap();
    println!("edge-service listening on http://127.0.0.1:4000");
    axum::serve(listener, app).await.unwrap();
}

#[tracing::instrument(skip(state))]
async fn handle_edge(State(state): State<AppState>) -> impl IntoResponse {
    let mut request = state.http.get("http://127.0.0.1:4001/work");
    if let Some(traceparent) = tracelet::current_traceparent() {
        request = request.header("traceparent", traceparent);
    }

    match request.send().await {
        Ok(response) => {
            let body = response.text().await.unwrap_or_default();
            format!("edge ok; downstream said: {body}")
        }
        Err(err) => format!("edge ok; downstream call failed: {err}"),
    }
}
