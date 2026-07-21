# tracelet

`#[tracing::instrument]` you already use, minus the OpenTelemetry dependency tree.

`tracelet` is a minimal, embeddable tracer for Rust services. It's a `tracing_subscriber::Layer`
that batches spans and exports them as OTLP — built to be a fraction of the dependency footprint
of `opentelemetry` + `opentelemetry-otlp` + `tracing-opentelemetry`, with no forced async runtime.

Status: **early, pre-release.** M0 (capture spans, no export yet) is done. See Roadmap.

## Why

Existing observability agents (Datadog Agent, Grafana Alloy, Vector) are separate processes you
deploy and configure. The OpenTelemetry Rust SDK is embeddable but heavy: a large dependency tree,
multiple builder APIs to learn (TracerProvider, Resource, SpanProcessor, exporters), and real
binary-size/compile-time cost. `tracelet` targets the gap between those two: something you
`cargo add` directly into a service, with an API surface small enough to hold in your head.

## Non-goals (v1)

These are deliberate scope cuts, not oversights:

- **No metrics or logs.** Metrics is already well-served by the [`metrics`](https://docs.rs/metrics)
  crate. Logs ride on your existing `tracing` subscriber stack.
- **No tail sampling.** Requires a collector-side buffer — out of scope for something living
  in-process.
- **Head-based probabilistic sampling only.**
- **W3C `traceparent` propagation only** — no B3, no vendor-specific headers.
- **OTLP/HTTP+protobuf only** — no gRPC OTLP variant.
- **No pipeline/plugin configuration.** One hardcoded path: instrument → buffer → batch → export.
  This is exactly the configuration surface that makes Vector/Alloy heavy.

## Design

- **Sync-first.** The capture path and background flush use `std::thread`, not tokio. An app
  doesn't have to adopt an async runtime just to get traces out. Async HTTP export is a future
  opt-in feature, not the default.
- **Bounded, drop-oldest buffering.** A slow or unreachable OTLP endpoint can never apply
  backpressure to the instrumented application.
- **Workspace split** so the export protocol is swappable later without touching instrumentation
  code:
  - `tracelet-core` — span record types and the ring buffer. No I/O, no async.
  - `tracelet-layer` — the `tracing_subscriber::Layer` implementation.
  - `tracelet-otlp` — protobuf encoding + HTTP export.
  - `tracelet` — the public facade crate: `TracerConfig` + `init()`.

## Roadmap

- [x] **M0 — Skeleton.** Workspace scaffold, `TracerConfig`, `init()` installs the layer.
      Spans are captured into the ring buffer and printed to stdout (stand-in for export).
- [x] **M1 — OTLP export.** `tracelet-otlp` encodes OTLP/HTTP trace protobuf (hand-written
      structs matching the official field tags, via `prost`) and a background thread batches
      and POSTs them with `ureq`. Verified two ways: an integration test decoding the wire
      bytes against a mock HTTP receiver, and end-to-end against a real local Jaeger instance
      (spans confirmed via Jaeger's query API — correct names, durations, and attributes).
- [ ] **M2 — Propagation + real service.** W3C `traceparent` inject/extract helpers; an `axum`
      example with two services sharing one trace across a network hop.
- [ ] **M3 — Sampling + overhead validation.** Probabilistic head sampling; a published
      benchmark of per-span overhead and dependency-tree size versus the standard OTel Rust stack.

## Example

```rust
tracelet::init(tracelet::TracerConfig {
    service_name: "my-service".to_string(),
    ..Default::default()
})?;

#[tracing::instrument]
fn do_work(iteration: u32) {
    tracing::info!(iteration, "did some work");
}
```

See [`examples/minimal`](examples/minimal) for a runnable version.

### Testing against a local collector

```sh
docker run -d --name tracelet-jaeger \
  -p 16686:16686 -p 4317:4317 -p 4318:4318 \
  -e COLLECTOR_OTLP_ENABLED=true \
  jaegertracing/all-in-one:latest

TRACELET_OTLP_ENDPOINT=http://localhost:4318/v1/traces cargo run -p minimal
```

Then check [localhost:16686](http://localhost:16686) for the `minimal-example` service.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at your option.
