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
use reqwest::{header::HeaderMap, Client, ClientBuilder};
use serde_json::Value;
use std::io::Read;
use std::sync::Arc;
use tracing::instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use opentelemetry::trace::TraceContextExt;
use opentelemetry::trace::Status as OTelStatus;
use lambda_runtime::{
    layers::{OpenTelemetryFaasTrigger, OpenTelemetryLayer as OtelLayer},
    Error as LambdaError, LambdaEvent, Runtime,
};

mod collectors;
mod headers;

use lambda_otel_utils::HttpTracerProviderBuilder;
use otlp_stdout_client::LogRecord;
use tracing::Instrument;
use crate::collectors::Collectors;


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
    forwarder.endpoint = endpoint, 
    lambda_otlp_forwarder.send_request.status
))]
async fn send_request(
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
    current_span.record(
        "lambda_otlp_forwarder.send_request.status",
        status.to_string(),
    );
    if !status.is_success() {
        current_span.context().span().set_status(OTelStatus::error("Failed to post log record"));
        tracing::warn!(
            name = "error posting log record",
            endpoint = endpoint,
            status = status.as_u16(),
        );
    }
    Ok(())
}

/// Processes a single log record by:
/// 1. Decoding its payload
/// 2. Finding a matching collector
/// 3. Building request headers
/// 4. Sending the request to the collector
#[instrument(skip(client, log_record, env_headers), fields(
    source = log_record.source,
))]
async fn process_record(
    client: &Client,
    log_record: LogRecord,
    env_headers: &headers::EnvHeaders,
) -> Result<(), anyhow::Error> {
    let decoded_payload = decode_payload(&log_record)?;

    let collector = match Collectors::find_matching(&log_record.endpoint) {
        Some(c) => c,
        None => {
            tracing::warn!(
                "No matching collector found for endpoint: {}. Skipping this record.",
                log_record.endpoint
            );
            return Ok(());
        }
    };

    let headers = headers::LogRecordHeaders::new()
        .with_env_headers(env_headers)
        .with_log_record(&log_record)?
        .with_collector_auth(&collector.auth)?
        .build();

    send_request(client, &log_record.endpoint, headers, decoded_payload).await
}

/// Processes a single CloudWatch log event by:
/// 1. Extracting the log record from the event message
/// 2. Parsing it into a structured format
/// 3. Processing the resulting log record
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

    tracing::debug!("Processing log event");
    process_record(client, log_record, env_headers).await
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
            .instrument(tracing::info_span!("process_log_event"))
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
        async move {
            function_handler(event, client, env_headers).await
        }
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
