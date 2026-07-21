use std::fmt;
use std::io::Write as _;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tracelet_core::RingBuffer;
use tracelet_layer::CaptureLayer;
use tracelet_otlp::OtlpExporterConfig;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

const RING_BUFFER_CAPACITY: usize = 1024;
const FLUSH_INTERVAL: Duration = Duration::from_secs(2);

pub struct TracerConfig {
    pub service_name: String,
    /// OTLP/HTTP traces endpoint, e.g. "http://localhost:4318/v1/traces".
    /// When None, captured spans are printed to stdout instead.
    pub otlp_endpoint: Option<String>,
    /// Unused until head sampling lands (M3). Every span is captured.
    pub sample_ratio: f64,
}

impl Default for TracerConfig {
    fn default() -> Self {
        Self {
            service_name: "unnamed-service".to_string(),
            otlp_endpoint: None,
            sample_ratio: 1.0,
        }
    }
}

#[derive(Debug)]
pub enum InitError {
    SetGlobalDefault(tracing::subscriber::SetGlobalDefaultError),
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InitError::SetGlobalDefault(e) => write!(f, "failed to install tracing subscriber: {e}"),
        }
    }
}

impl std::error::Error for InitError {}

pub fn init(config: TracerConfig) -> Result<(), InitError> {
    let buffer = Arc::new(RingBuffer::new(RING_BUFFER_CAPACITY));
    let layer = CaptureLayer::new(buffer.clone());
    let subscriber = Registry::default().with(layer);

    tracing::subscriber::set_global_default(subscriber).map_err(InitError::SetGlobalDefault)?;

    match config.otlp_endpoint {
        Some(endpoint) => tracelet_otlp::spawn_exporter(
            buffer,
            OtlpExporterConfig {
                endpoint,
                service_name: config.service_name,
                ..Default::default()
            },
        ),
        None => spawn_stdout_flusher(buffer, config.service_name),
    }

    Ok(())
}

fn spawn_stdout_flusher(buffer: Arc<RingBuffer>, service_name: String) {
    thread::spawn(move || loop {
        thread::sleep(FLUSH_INTERVAL);
        for record in buffer.drain() {
            let duration = record.end.and_then(|end| end.duration_since(record.start).ok());
            println!(
                "[{service_name}] span={} parent={:?} duration={:?} attrs={:?}",
                record.name, record.parent_id, duration, record.attributes,
            );
        }
        let _ = std::io::stdout().flush();
    });
}
