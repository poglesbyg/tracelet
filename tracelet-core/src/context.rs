//! Tracks the (trace_id, span_id) of the span currently entered on this
//! thread, so client code can call `current()` to inject a `traceparent`
//! into an outgoing request without needing access to the tracing registry.
//!
//! Each entry is a shared cell rather than a plain value: with
//! `#[instrument(fields(otel_remote_trace_id = Empty))]`, a span is entered
//! (and pushed here) *before* its handler body runs `.record()` to fill in
//! the remote context. A plain snapshot taken at push time would miss that
//! later update; routing both the push and the update through the same cell
//! keeps `current()` correct regardless of ordering.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

pub type ContextCell = Arc<Mutex<(u128, u64)>>;

pub fn new_cell(trace_id: u128, span_id: u64) -> ContextCell {
    Arc::new(Mutex::new((trace_id, span_id)))
}

pub fn set_trace_id(cell: &ContextCell, trace_id: u128) {
    cell.lock().unwrap().0 = trace_id;
}

thread_local! {
    static STACK: RefCell<Vec<ContextCell>> = const { RefCell::new(Vec::new()) };
}

pub fn push_current(cell: ContextCell) {
    STACK.with(|stack| stack.borrow_mut().push(cell));
}

pub fn pop_current() {
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
}

/// The (trace_id, span_id) of the innermost currently-entered span on this
/// thread, if any.
pub fn current() -> Option<(u128, u64)> {
    STACK.with(|stack| stack.borrow().last().map(|cell| *cell.lock().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflects_updates_made_after_push() {
        let cell = new_cell(1, 2);
        push_current(cell.clone());
        assert_eq!(current(), Some((1, 2)));

        set_trace_id(&cell, 99);
        assert_eq!(current(), Some((99, 2)), "current() must see the update, not a stale snapshot");

        pop_current();
        assert_eq!(current(), None);
    }

    #[test]
    fn nests_correctly() {
        push_current(new_cell(1, 1));
        push_current(new_cell(1, 2));
        assert_eq!(current(), Some((1, 2)));
        pop_current();
        assert_eq!(current(), Some((1, 1)));
        pop_current();
        assert_eq!(current(), None);
    }
}
