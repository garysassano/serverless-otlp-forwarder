use lambda_extension::{tracing, Error};
use opentelemetry::trace::{SpanId, TraceId};

/// Holds information parsed from the X-Ray tracing header.
#[derive(Debug, Clone)]
pub struct ParsedXrayTraceContext {
    pub trace_id: TraceId,
    pub parent_id: Option<SpanId>,
    pub sampled: bool,
    pub platform_span_id: Option<SpanId>,
}

/// Parses the X-Ray trace header string (e.g., "Root=1-xxx;Parent=yyy;Sampled=1")
pub fn parse_xray_header_value(value: &str) -> Result<ParsedXrayTraceContext, Error> {
    let mut trace_id = None;
    let mut parent_id = None;
    let mut sampled = None;

    for part in value.split(';') {
        let mut kv = part.splitn(2, '=');
        if let (Some(key), Some(val)) = (kv.next(), kv.next()) {
            match key {
                "Root" => {
                    let parts: Vec<&str> = val.splitn(3, '-').collect();
                    if parts.len() == 3 {
                        let epoch_hex = parts[1];
                        let random_hex = parts[2];
                        let trace_id_str = format!("{}{}", epoch_hex, &random_hex[..24]);
                        if trace_id_str.len() == 32 {
                            trace_id = TraceId::from_hex(&trace_id_str).ok();
                        } else {
                            tracing::warn!(
                                "Could not construct 32-char Trace ID from X-Ray Root: {}",
                                val
                            );
                        }
                    }
                }
                "Parent" => {
                    if val.len() == 16 {
                        parent_id = SpanId::from_hex(val).ok();
                    }
                }
                "Sampled" => match val {
                    "1" => sampled = Some(true),
                    "0" => sampled = Some(false),
                    _ => sampled = Some(false),
                },
                _ => {}
            }
        }
    }

    Ok(ParsedXrayTraceContext {
        trace_id: trace_id
            .ok_or_else(|| Error::from("Missing or invalid Root in X-Ray trace context"))?,
        parent_id,
        sampled: sampled.unwrap_or(false),
        platform_span_id: None,
    })
}
