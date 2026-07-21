//! Hand-written subset of the public OTLP protobuf schema
//! (opentelemetry-proto: common/v1, resource/v1, trace/v1), covering only the
//! fields tracelet emits. Field tags match the official .proto definitions
//! exactly, so the wire bytes are compatible with any real OTLP/HTTP
//! collector -- this is not a custom schema, just a trimmed one.

use prost::Message;

#[derive(Clone, PartialEq, Message)]
pub struct ExportTraceServiceRequest {
    #[prost(message, repeated, tag = "1")]
    pub resource_spans: Vec<ResourceSpans>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ResourceSpans {
    #[prost(message, optional, tag = "1")]
    pub resource: Option<Resource>,
    #[prost(message, repeated, tag = "2")]
    pub scope_spans: Vec<ScopeSpans>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ScopeSpans {
    #[prost(message, optional, tag = "1")]
    pub scope: Option<InstrumentationScope>,
    #[prost(message, repeated, tag = "2")]
    pub spans: Vec<Span>,
}

#[derive(Clone, PartialEq, Message)]
pub struct InstrumentationScope {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub version: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct Resource {
    #[prost(message, repeated, tag = "1")]
    pub attributes: Vec<KeyValue>,
}

#[derive(Clone, PartialEq, Message)]
pub struct KeyValue {
    #[prost(string, tag = "1")]
    pub key: String,
    #[prost(message, optional, tag = "2")]
    pub value: Option<AnyValue>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AnyValue {
    #[prost(oneof = "any_value::Value", tags = "1")]
    pub value: Option<any_value::Value>,
}

pub mod any_value {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        #[prost(string, tag = "1")]
        StringValue(String),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct Span {
    #[prost(bytes = "vec", tag = "1")]
    pub trace_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub span_id: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    pub parent_span_id: Vec<u8>,
    #[prost(string, tag = "5")]
    pub name: String,
    #[prost(fixed64, tag = "7")]
    pub start_time_unix_nano: u64,
    #[prost(fixed64, tag = "8")]
    pub end_time_unix_nano: u64,
    #[prost(message, repeated, tag = "9")]
    pub attributes: Vec<KeyValue>,
}
