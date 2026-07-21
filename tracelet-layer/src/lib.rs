use std::fmt;
use std::sync::Arc;
use std::time::SystemTime;

use tracelet_core::{RingBuffer, SpanRecord};
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

// Held in each span's extensions between on_new_span and on_close.
struct SpanState {
    trace_id: u128,
    parent_id: Option<u64>,
    name: String,
    start: SystemTime,
    attributes: Vec<(String, String)>,
}

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span must exist in on_new_span");

        let mut visitor = AttributeVisitor::default();
        attrs.record(&mut visitor);

        let parent = span.parent();
        let parent_id = parent.as_ref().map(|p| p.id().into_u64());
        let trace_id = parent
            .as_ref()
            .and_then(|p| p.extensions().get::<SpanState>().map(|s| s.trace_id))
            .unwrap_or_else(tracelet_core::generate_trace_id);

        span.extensions_mut().insert(SpanState {
            trace_id,
            parent_id,
            name: attrs.metadata().name().to_string(),
            start: SystemTime::now(),
            attributes: visitor.0,
        });
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span must exist in on_record");
        let mut extensions = span.extensions_mut();
        if let Some(state) = extensions.get_mut::<SpanState>() {
            let mut visitor = AttributeVisitor::default();
            values.record(&mut visitor);
            state.attributes.extend(visitor.0);
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let span = ctx.span(&id).expect("span must exist in on_close");
        let state = span.extensions_mut().remove::<SpanState>();

        if let Some(state) = state {
            self.buffer.push(SpanRecord {
                trace_id: state.trace_id,
                span_id: id.into_u64(),
                parent_id: state.parent_id,
                name: state.name,
                start: state.start,
                end: Some(SystemTime::now()),
                attributes: state.attributes,
            });
        }
    }
}
