use std::thread;
use std::time::Duration;

use tracelet::TracerConfig;

#[tracing::instrument]
fn do_work(iteration: u32) {
    thread::sleep(Duration::from_millis(50));
    tracing::info!(iteration, "did some work");
}

fn main() {
    // Set TRACELET_OTLP_ENDPOINT (e.g. http://localhost:4318/v1/traces) to
    // export to a real collector instead of printing to stdout.
    let otlp_endpoint = std::env::var("TRACELET_OTLP_ENDPOINT").ok();

    tracelet::init(TracerConfig {
        service_name: "minimal-example".to_string(),
        otlp_endpoint,
        ..Default::default()
    })
    .expect("failed to init tracelet");

    for i in 0..5 {
        do_work(i);
    }

    // Give the background flusher time to drain and print before exit.
    thread::sleep(Duration::from_secs(3));
}
