//! W3C `traceparent` formatting/parsing (https://www.w3.org/TR/trace-context/)
//! and the reserved span-field names used to seed a span with a remote trace
//! context. tracelet-layer looks for these two field names on every span and,
//! when present, uses them instead of the local in-process parent -- this is
//! how a server-side handler picks up the trace_id/parent_span_id extracted
//! from an inbound request instead of starting a new trace.

/// Set to the 32-hex-digit trace id extracted from an inbound `traceparent`.
pub const REMOTE_TRACE_ID_FIELD: &str = "otel_remote_trace_id";
/// Set to the 16-hex-digit parent span id extracted from an inbound `traceparent`.
pub const REMOTE_PARENT_SPAN_ID_FIELD: &str = "otel_remote_parent_span_id";

pub fn format_traceparent(trace_id: u128, span_id: u64) -> String {
    format!("00-{trace_id:032x}-{span_id:016x}-01")
}

/// Parses a `traceparent` header value. Returns `None` on any malformed or
/// all-zero id, per the W3C spec's validity rules -- callers should treat
/// that the same as "no incoming trace context" and start a fresh trace.
pub fn parse_traceparent(header: &str) -> Option<(u128, u64)> {
    let mut parts = header.trim().split('-');
    let version = parts.next()?;
    let trace_id = parts.next()?;
    let span_id = parts.next()?;
    let _flags = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if version.len() != 2 || trace_id.len() != 32 || span_id.len() != 16 {
        return None;
    }

    let trace_id = u128::from_str_radix(trace_id, 16).ok()?;
    let span_id = u64::from_str_radix(span_id, 16).ok()?;
    if trace_id == 0 || span_id == 0 {
        return None;
    }
    Some((trace_id, span_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let (trace_id, span_id) = (0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10, 0x1112_1314_1516_1718);
        let header = format_traceparent(trace_id, span_id);
        assert_eq!(header, "00-0102030405060708090a0b0c0d0e0f10-1112131415161718-01");
        assert_eq!(parse_traceparent(&header), Some((trace_id, span_id)));
    }

    #[test]
    fn rejects_malformed_headers() {
        assert_eq!(parse_traceparent(""), None);
        assert_eq!(parse_traceparent("00-short-1112131415161718-01"), None);
        assert_eq!(parse_traceparent("not-even-close"), None);
        assert_eq!(
            parse_traceparent("00-00000000000000000000000000000000-1112131415161718-01"),
            None,
            "all-zero trace_id is invalid per spec"
        );
        assert_eq!(
            parse_traceparent("00-0102030405060708090a0b0c0d0e0f10-0000000000000000-01"),
            None,
            "all-zero span_id is invalid per spec"
        );
    }
}
