use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct SpanRecord {
    pub trace_id: u128,
    pub span_id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub start: SystemTime,
    pub end: Option<SystemTime>,
    pub attributes: Vec<(String, String)>,
}

static TRACE_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generates a 128-bit trace id for a root span. Not cryptographically
/// random -- mixes wall-clock time, a process-local counter, and the process
/// id, which is enough entropy to avoid collisions without pulling in a
/// `rand` dependency. Child spans should inherit their parent's trace id
/// instead of calling this.
pub fn generate_trace_id() -> u128 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = TRACE_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id() as u64;

    let high = mix((nanos >> 64) as u64, pid);
    let low = mix(nanos as u64, counter);
    ((high as u128) << 64) | low as u128
}

fn mix(a: u64, b: u64) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    a.hash(&mut hasher);
    b.hash(&mut hasher);
    hasher.finish()
}

/// Bounded, thread-safe span buffer. Drops the oldest record on overflow so a
/// slow or unreachable exporter can never apply backpressure to the app.
pub struct RingBuffer {
    capacity: usize,
    inner: Mutex<VecDeque<SpanRecord>>,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    pub fn push(&self, record: SpanRecord) {
        let mut guard = self.inner.lock().unwrap();
        if guard.len() == self.capacity {
            guard.pop_front();
        }
        guard.push_back(record);
    }

    /// Removes and returns every buffered record.
    pub fn drain(&self) -> Vec<SpanRecord> {
        let mut guard = self.inner.lock().unwrap();
        guard.drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(span_id: u64) -> SpanRecord {
        SpanRecord {
            trace_id: generate_trace_id(),
            span_id,
            parent_id: None,
            name: "test".to_string(),
            start: SystemTime::now(),
            end: None,
            attributes: Vec::new(),
        }
    }

    #[test]
    fn drops_oldest_on_overflow() {
        let buf = RingBuffer::new(2);
        buf.push(record(1));
        buf.push(record(2));
        buf.push(record(3));
        let drained = buf.drain();
        assert_eq!(drained.iter().map(|r| r.span_id).collect::<Vec<_>>(), vec![2, 3]);
    }

    #[test]
    fn drain_empties_buffer() {
        let buf = RingBuffer::new(4);
        buf.push(record(1));
        assert_eq!(buf.len(), 1);
        buf.drain();
        assert!(buf.is_empty());
    }
}
