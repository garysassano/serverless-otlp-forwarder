//! AWS Lambda function that forwards CloudWatch logs to OpenTelemetry collectors.
//!
//! This Lambda function:
//! 1. Receives CloudWatch log events
//! 2. Decodes and decompresses the log data
//! 3. Matches each log record with an appropriate collector
//! 4. Forwards the logs to their respective collectors with proper headers
//!
//! The function supports:
//! - Multiple collectors with different endpoints
//! - Custom headers and authentication
//! - Base64 encoded payloads
//! - Gzip compressed data
//! - OpenTelemetry instrumentation

use anyhow::{Context, Result};
use aws_sdk_secretsmanager::Client as SecretsManagerClient;
use base64::{engine::general_purpose, Engine};
use flate2::read::GzDecoder;
use futures::future::join_all;
use lambda_runtime::{
    layers::{OpenTelemetryFaasTrigger, OpenTelemetryLayer as OtelLayer},
    Error as LambdaError, LambdaEvent, Runtime,
};
use opentelemetry::trace::SpanKind;
use reqwest::{header::HeaderMap, Client, ClientBuilder};
use serde_json::Value;
use std::io::Read;
use std::sync::Arc;
use tracing::instrument;

mod collectors;
mod headers;

use crate::collectors::Collectors;
use lambda_otel_utils::HttpTracerProviderBuilder;
use otlp_stdout_client::LogRecord;
use tracing::Instrument;

/// Decodes a payload from a log record.
///
/// # Arguments
/// * `log_record` - The log record containing the payload
///
/// # Returns
/// * `Result<Vec<u8>>` - The decoded payload as bytes
///
/// # Details
/// - If payload is a string: decodes from base64 if base64 flag is true
/// - If payload is a JSON value: serializes to string first
#[instrument(skip_all)]
fn decode_payload(log_record: &LogRecord) -> Result<Vec<u8>, anyhow::Error> {
    let payload = match &log_record.payload {
        Value::String(s) => s.to_string(),
        _ => serde_json::to_string(&log_record.payload)
            .context("Failed to serialize JSON payload")?,
    };

    match log_record.base64 {
        Some(true) => general_purpose::STANDARD
            .decode(&payload)
            .context("Failed to decode base64 payload"),
        _ => Ok(payload.as_bytes().to_vec()),
    }
}

/// Sends an HTTP POST request to the specified endpoint with the given headers and payload.
/// Includes OpenTelemetry instrumentation for request tracking.
#[instrument(skip_all, fields(
    otel.kind = ?SpanKind::Client,
    otel.status_code,
    http.method = "POST",
    http.url = endpoint,
    http.status_code,
))]
async fn post(
    client: &Client,
    endpoint: &str,
    headers: HeaderMap,
    payload: Vec<u8>,
) -> Result<(), anyhow::Error> {
    let current_span = tracing::Span::current();
    let response = client
        .post(endpoint)
        .headers(headers.clone())
        .body(payload.clone())
        .send()
        .await
        .context("Failed to send POST request")?;

    let status = response.status();

    // Record the HTTP status code
    current_span.record("http.status_code", &status.as_u16());

    if !status.is_success() {
        current_span.record("otel.status_code", "ERROR");
        tracing::warn!(
            name = "error posting log record",
            endpoint = endpoint,
            status = status.as_u16(),
        );
    }

    Ok(())
}

/// Processes a single CloudWatch log event by:
/// 1. Extracting and parsing the log record from the event message
/// 2. Decoding its payload
/// 3. Getting all configured collectors
/// 4. Building request headers for each collector
/// 5. Sending the request to all collectors in parallel
async fn process_log_event(
    event: &Value,
    client: &Client,
    env_headers: &headers::EnvHeaders,
) -> Result<()> {
    let record = event["message"]
        .as_str()
        .context("Log event 'message' field is not a string")?;

    let log_record: LogRecord = serde_json::from_str(record)
        .with_context(|| format!("Failed to parse log record: {}", record))?;

    // Record the source field for tracing
    tracing::Span::current().record("event.source", &log_record.source);

    tracing::debug!("Processing log event");

    let decoded_payload = decode_payload(&log_record)?;

    // Get all collectors with proper signal paths
    let collectors = Collectors::get_signal_endpoints(&log_record.endpoint)?;

    // Create futures for sending to each collector
    let futures: Vec<_> = collectors
        .into_iter()
        .map(|collector| {
            let client = client.clone();
            let decoded_payload = decoded_payload.clone();
            let headers = headers::LogRecordHeaders::new()
                .with_env_headers(env_headers)
                .with_log_record(&log_record)
                .unwrap()
                .with_collector_auth(&collector.auth)
                .unwrap()
                .build();

            async move {
                match post(&client, &collector.endpoint, headers, decoded_payload).await {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        tracing::warn!("Failed to send to collector {}: {}", collector.name, e);
                        Err(e)
                    }
                }
            }
        })
        .collect();

    // Execute all requests in parallel
    let results = join_all(futures).await;

    // Check if any requests succeeded
    let mut any_success = false;
    let mut last_error = None;

    for result in results {
        match result {
            Ok(()) => {
                any_success = true;
            }
            Err(e) => {
                tracing::error!("Task error: {:?}", e);
                last_error = Some(e);
            }
        }
    }

    if !any_success {
        if let Some(e) = last_error {
            Err(anyhow::anyhow!("All collectors failed. Last error: {}", e))
        } else {
            Err(anyhow::anyhow!(
                "All collectors failed with no error details"
            ))
        }
    } else {
        Ok(())
    }
}

/// Main Lambda function handler that:
/// 1. Decodes base64 CloudWatch log data
/// 2. Decompresses gzipped content
/// 3. Spawns concurrent tasks to process each log event
/// 4. Aggregates results and handles errors
#[instrument(skip_all, name = "function_handler")]
async fn function_handler(
    event: LambdaEvent<Value>,
    client: Client,
    env_headers: Arc<headers::EnvHeaders>,
) -> Result<(), LambdaError> {
    tracing::debug!("Function handler started");

    // Extract and process the CloudWatch Logs data
    let decoded = general_purpose::STANDARD
        .decode(
            event
                .payload
                .get("awslogs")
                .context("No 'awslogs' object found in the payload")?
                .get("data")
                .and_then(|d| d.as_str())
                .context("No 'data' field found in awslogs or it's not a string")?,
        )
        .context("Failed to decode base64")?;

    // Decompress the gzipped log data
    let mut decoder = GzDecoder::new(&decoded[..]);
    let mut decompressed = String::new();
    decoder
        .read_to_string(&mut decompressed)
        .context("Failed to decompress data")?;

    // Parse the JSON log data
    let log_data: Value =
        serde_json::from_str(&decompressed).context("Failed to parse decompressed data as JSON")?;

    // Process each log event
    let log_events = log_data["logEvents"]
        .as_array()
        .context("No logEvents array found in the data")?;
    tracing::debug!("Processing {} log events", log_events.len());

    // Create an Arc<Vec<Value>> to share ownership of log_events
    let log_events = Arc::new(log_events.clone());

    let tasks: Vec<_> = log_events
        .iter()
        .map(|event| {
            let client = client.clone();
            let env_headers = Arc::clone(&env_headers);
            let event = event.clone();

            async move {
                let result = process_log_event(&event, &client, &env_headers).await;
                if let Err(e) = &result {
                    tracing::warn!("Error processing log event: {}", e);
                }
                result
            }
            .instrument(tracing::info_span!(
                "process_log_event",
                event.source = tracing::field::Empty
            ))
        })
        .map(|future| tokio::spawn(future.in_current_span()))
        .collect();

    let results = join_all(tasks).await;
    for result in results {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::error!("Task error: {:?}", e),
            Err(e) => tracing::error!("Task panicked: {:?}", e),
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), LambdaError> {
    let tracer_provider = HttpTracerProviderBuilder::default()
        .enable_global(true)
        .enable_fmt_layer(true)
        .with_tracer_name("lambda-otlp-forwarder")
        .with_batch_exporter()
        .build()?;

    // Initialize AWS clients and collectors
    let config = aws_config::load_from_env().await;
    let secrets_client = SecretsManagerClient::new(&config);
    Collectors::init(&secrets_client).await?;

    let http_client = ClientBuilder::new().build()?;
    let env_headers = headers::EnvHeaders::from_env()?;

    Runtime::new(lambda_runtime::service_fn(|event| {
        let client = http_client.clone();
        let env_headers = Arc::clone(&env_headers);
        async move { function_handler(event, client, env_headers).await }
    }))
    .layer(
        OtelLayer::new(|| {
            tracer_provider.force_flush();
        })
        .with_trigger(OpenTelemetryFaasTrigger::PubSub),
    )
    .run()
    .await
}
