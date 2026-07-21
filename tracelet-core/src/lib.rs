use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct SpanRecord {
    pub span_id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
    pub start: SystemTime,
    pub end: Option<SystemTime>,
    pub attributes: Vec<(String, String)>,
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
