use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use tracelet_core::context::ContextCell;
use tracelet_core::{RingBuffer, SpanRecord, REMOTE_PARENT_SPAN_ID_FIELD, REMOTE_TRACE_ID_FIELD};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

pub struct CaptureLayer {
    buffer: Arc<RingBuffer>,
}

impl CaptureLayer {
    pub fn new(buffer: Arc<RingBuffer>) -> Self {
        Self { buffer }
    }
}

#[derive(Default)]
struct AttributeVisitor(Vec<(String, String)>);

impl Visit for AttributeVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0.push((field.name().to_string(), format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
}

/// Separates the reserved remote-context fields out of a freshly-visited
/// field list. `Span::record()` sets one field per call, so the two reserved
/// fields will usually arrive in separate calls -- each is applied
/// independently rather than requiring both together. Whatever's found is
/// removed from the returned attribute list so it never gets exported as a
/// regular span attribute.
fn partition_remote_fields(fields: Vec<(String, String)>) -> (Option<u128>, Option<u64>, Vec<(String, String)>) {
    let mut trace_id = None;
    let mut parent_id = None;
    let mut attributes = Vec::with_capacity(fields.len());

    for (key, value) in fields {
        if key == REMOTE_TRACE_ID_FIELD {
            trace_id = u128::from_str_radix(&value, 16).ok();
        } else if key == REMOTE_PARENT_SPAN_ID_FIELD {
            parent_id = u64::from_str_radix(&value, 16).ok();
        } else {
            attributes.push((key, value));
        }
    }

    (trace_id, parent_id, attributes)
}

// Held in each span's extensions between on_new_span and on_close.
struct SpanState {
    trace_id: u128,
    // Our own id, not tracing::Id::into_u64() -- see generate_span_id's docs
    // for why that can't be reused as the OTLP-facing span id.
    span_id: u64,
    parent_id: Option<u64>,
    name: String,
    start: SystemTime,
    attributes: Vec<(String, String)>,
    // Shared with tracelet_core::context's thread-local stack so a remote
    // context recorded after this span was entered (the
    // #[instrument(fields(x = Empty))] + .record() pattern) is still visible
    // to current_traceparent() calls made later in the same span.
    context: ContextCell,
}

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span must exist in on_new_span");

        let mut visitor = AttributeVisitor::default();
        attrs.record(&mut visitor);
        let (remote_trace_id, remote_parent_id, attributes) = partition_remote_fields(visitor.0);

        let parent = span.parent();
        let local_parent = parent
            .as_ref()
            .and_then(|p| p.extensions().get::<SpanState>().map(|s| (s.trace_id, s.span_id)));

        let span_id = tracelet_core::generate_span_id();
        let trace_id = remote_trace_id
            .unwrap_or_else(|| local_parent.map(|(tid, _)| tid).unwrap_or_else(tracelet_core::generate_trace_id));
        let parent_id = remote_parent_id.or(local_parent.map(|(_, sid)| sid));

        span.extensions_mut().insert(SpanState {
            trace_id,
            span_id,
            parent_id,
            name: attrs.metadata().name().to_string(),
            start: SystemTime::now(),
            attributes,
            context: tracelet_core::context::new_cell(trace_id, span_id),
        });
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span must exist in on_record");
        let mut extensions = span.extensions_mut();
        if let Some(state) = extensions.get_mut::<SpanState>() {
            let mut visitor = AttributeVisitor::default();
            values.record(&mut visitor);
            let (remote_trace_id, remote_parent_id, attributes) = partition_remote_fields(visitor.0);

            if let Some(trace_id) = remote_trace_id {
                state.trace_id = trace_id;
                tracelet_core::context::set_trace_id(&state.context, trace_id);
            }
            if let Some(parent_id) = remote_parent_id {
                state.parent_id = Some(parent_id);
            }

            state.attributes.extend(attributes);
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(state) = span.extensions().get::<SpanState>() {
                tracelet_core::context::push_current(state.context.clone());
            }
        }
    }

    fn on_exit(&self, _id: &Id, _ctx: Context<'_, S>) {
        tracelet_core::context::pop_current();
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let span = ctx.span(&id).expect("span must exist in on_close");
        let state = span.extensions_mut().remove::<SpanState>();

        if let Some(state) = state {
            self.buffer.push(SpanRecord {
                trace_id: state.trace_id,
                span_id: state.span_id,
                parent_id: state.parent_id,
                name: state.name,
                start: state.start,
                end: Some(SystemTime::now()),
                attributes: state.attributes,
            });
        }
    }
}
