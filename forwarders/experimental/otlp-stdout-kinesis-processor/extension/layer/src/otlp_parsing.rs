use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine};
use flate2::read::GzDecoder;
use opentelemetry::trace::{SpanId, TraceId};
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use prost::Message;
use serde::Deserialize;
use std::io::Read;

// A simplified struct matching the relevant fields of otlp_stdout_span_exporter::ExporterOutput
#[derive(Deserialize, Debug)]
struct OtlpStdoutJsonLine {
    payload: String,
    #[serde(default)]
    base64: bool,
    #[serde(rename = "content-encoding", default)]
    content_encoding: String,
    #[serde(rename = "content-type", default)]
    content_type: String,
    // Other fields like source, endpoint, version are ignored for now
}

/// Parses an OTLP/stdout JSON line, decodes/decompresses the payload,
/// and extracts the TraceId and SpanId of the *first* span found.
///
/// Returns `Ok(None)` if the line isn't valid JSON, the payload is empty,
/// or no spans are found. Returns `Err` for decoding/decompression issues.
pub fn extract_trace_info_from_json_line(
    line: &str,
) -> Result<Option<(TraceId, SpanId)>> {
    let parsed_line: OtlpStdoutJsonLine = match serde_json::from_str(line) {
        Ok(p) => p,
        Err(_) => {
            // Not the JSON format we expect, ignore silently
            return Ok(None);
        }
    };

    if parsed_line.payload.is_empty() {
        return Ok(None); // No payload to process
    }

    // 1. Decode Base64 if necessary
    let raw_payload = if parsed_line.base64 {
        general_purpose::STANDARD
            .decode(&parsed_line.payload)
            .context("Failed to decode base64 payload")?
    } else {
        parsed_line.payload.into_bytes()
    };

    // 2. Decompress Gzip if necessary
    let decompressed_payload = if parsed_line.content_encoding == "gzip" {
        let mut decoder = GzDecoder::new(&raw_payload[..]);
        let mut decompressed_data = Vec::new();
        decoder
            .read_to_end(&mut decompressed_data)
            .context("Failed to decompress Gzip payload")?;
        decompressed_data
    } else {
        raw_payload
    };

    // 3. Decode Protobuf if necessary
    let trace_request = if parsed_line.content_type == "application/x-protobuf" {
        ExportTraceServiceRequest::decode(decompressed_payload.as_slice())
            .context("Failed to decode OTLP protobuf payload")?
    } else {
        // TODO: Handle application/json content-type if needed
        // For now, assume only protobuf or skip
        return Ok(None);
    };

    // 4. Find the first span and extract its IDs
    for resource_span in trace_request.resource_spans {
        for scope_span in resource_span.scope_spans {
            if let Some(span) = scope_span.spans.first() {
                let trace_id_bytes: [u8; 16] = span
                    .trace_id
                    .as_slice()
                    .try_into()
                    .context("Invalid trace_id length")?;
                let span_id_bytes: [u8; 8] = span
                    .span_id
                    .as_slice()
                    .try_into()
                    .context("Invalid span_id length")?;

                let trace_id = TraceId::from_bytes(trace_id_bytes);
                let span_id = SpanId::from_bytes(span_id_bytes);

                // Check for invalid IDs (all zeros)
                if trace_id != TraceId::INVALID && span_id != SpanId::INVALID {
                    tracing::debug!(%trace_id, %span_id, "Extracted trace info from first span in pipe message");
                    return Ok(Some((trace_id, span_id)));
                } else {
                    tracing::warn!("Found invalid trace_id or span_id in first span, skipping payload.");
                    return Ok(None);
                }
            }
        }
    }

    // No spans found in the payload
    Ok(None)
}

#[cfg(test)]
mod tests {
    // TODO: Add unit tests for extract_trace_info_from_json_line
} 