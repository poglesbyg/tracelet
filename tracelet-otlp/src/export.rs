use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tracelet_core::RingBuffer;

use crate::encode::encode_export_request;

pub struct OtlpExporterConfig {
    /// Full OTLP/HTTP traces URL, e.g. "http://localhost:4318/v1/traces".
    pub endpoint: String,
    pub service_name: String,
    pub batch_max_spans: usize,
    pub flush_interval: Duration,
}

impl Default for OtlpExporterConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4318/v1/traces".to_string(),
            service_name: "unnamed-service".to_string(),
            batch_max_spans: 512,
            flush_interval: Duration::from_secs(2),
        }
    }
}

/// Spawns a background thread that drains the ring buffer on an interval and
/// POSTs batches to the configured OTLP/HTTP endpoint. Export failures are
/// logged to stderr and dropped -- never applied as backpressure to the app.
pub fn spawn_exporter(buffer: Arc<RingBuffer>, config: OtlpExporterConfig) {
    thread::spawn(move || loop {
        thread::sleep(config.flush_interval);
        let records = buffer.drain();
        if records.is_empty() {
            continue;
        }

        for chunk in records.chunks(config.batch_max_spans.max(1)) {
            let body = encode_export_request(&config.service_name, chunk);
            if let Err(err) = post(&config.endpoint, body) {
                eprintln!("tracelet-otlp: export to {} failed: {err}", config.endpoint);
            }
        }
    });
}

fn post(endpoint: &str, body: Vec<u8>) -> Result<(), Box<ureq::Error>> {
    ureq::post(endpoint)
        .header("Content-Type", "application/x-protobuf")
        .send(&body)
        .map_err(Box::new)?;
    Ok(())
}
