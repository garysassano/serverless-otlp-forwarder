use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose, Engine};
use flate2::read::GzDecoder;
use lambda_runtime::{service_fn, Error as LambdaError, LambdaEvent};
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client,
};
use serde_json::Value;
use std::env;
use std::io::Read;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

// Constants for content types
const CONTENT_TYPE_JSON: &str = "application/json";

// Constants for headers
const CONTENT_TYPE_HEADER: &str = "content-type";
const CONTENT_ENCODING_HEADER: &str = "content-encoding";
const KEY_HEADERS: &str = "headers";

// Constants for JSON keys
const KEY_OTEL: &str = "_otel";
const KEY_ENDPOINT: &str = "endpoint";
const KEY_PAYLOAD: &str = "payload";
const KEY_BASE64: &str = "base64";

// Constant for GZIP encoding
const ENCODING_GZIP: &str = "gzip";

struct EnvHeaders(HeaderMap);

impl EnvHeaders {
    fn new() -> Result<Self, anyhow::Error> {
        let mut headers = HeaderMap::new();
        if let Ok(env_headers) = env::var("OTEL_EXPORTER_OTLP_HEADERS") {
            for header_pair in env_headers.split(',') {
                let parts: Vec<&str> = header_pair.split('=').collect();
                if parts.len() != 2 {
                    warn!("Invalid header pair: {}", header_pair);
                    continue;
                }

                let (name, value) = (parts[0].trim(), parts[1].trim());

                if let Ok(header_name) = HeaderName::from_str(name) {
                    if let Ok(header_value) = HeaderValue::from_str(value) {
                        headers.insert(header_name, header_value);
                        debug!("Added header from env: {}={}", name, value);
                    } else {
                        warn!("Invalid header value from env: {}", value);
                    }
                } else {
                    warn!("Invalid header name from env: {}", name);
                }
            }
        }
        Ok(EnvHeaders(headers))
    }
}

/// Handles the Lambda function invocation.
///
/// This function processes the incoming CloudWatch Logs event, decodes and decompresses
/// the log data, and processes each log event.
///
/// # Arguments
///
/// * `event` - The Lambda event containing the CloudWatch Logs data
/// * `client` - An HTTP client for making outbound requests
/// * `env_headers` - Pre-processed environment headers
///
/// # Returns
///``
/// Returns `Ok(())` if processing is successful, or an error if any step fails.
#[instrument(skip(event, client, env_headers))]
async fn function_handler(
    event: LambdaEvent<Value>,
    client: Client,
    env_headers: Arc<EnvHeaders>,
) -> Result<(), LambdaError> {
    debug!("Function handler started");

    // Extract and process the CloudWatch Logs data
    let awslogs = event
        .payload
        .get("awslogs")
        .context("No 'awslogs' object found in the payload")?;
    debug!("Extracted awslogs from payload");

    let data = awslogs
        .get("data")
        .and_then(|d| d.as_str())
        .context("No 'data' field found in awslogs or it's not a string")?;
    debug!("Extracted data from awslogs");

    // Decode the base64 encoded log data
    let decoded = general_purpose::STANDARD
        .decode(data)
        .context("Failed to decode base64")?;
    debug!("Decoded base64 data");

    // Decompress the gzipped log data
    let mut decoder = GzDecoder::new(&decoded[..]);
    let mut decompressed = String::new();
    decoder
        .read_to_string(&mut decompressed)
        .context("Failed to decompress data")?;
    debug!("Decompressed gzip data");

    // Parse the JSON log data
    let log_data: Value =
        serde_json::from_str(&decompressed).context("Failed to parse decompressed data as JSON")?;
    debug!("Parsed decompressed data as JSON");

    // Process each log event
    let log_events = log_data["logEvents"]
        .as_array()
        .context("No logEvents array found in the data")?;
    info!("Processing {} log events", log_events.len());

    for (index, event) in log_events.iter().enumerate() {
        debug!("Processing log event {}/{}", index + 1, log_events.len());
        if let Some(message) = event["message"].as_str() {
            match serde_json::from_str::<Value>(message) {
                Ok(message_json) => {
                    debug!("Successfully parsed message as JSON");
                    process_message(&client, message_json, &env_headers).await?;
                }
                Err(e) => warn!("Failed to parse message as JSON: {}", e),
            }
        } else {
            warn!("Log event does not contain a 'message' field or it's not a string");
        }
    }

    info!("Function handler completed successfully");
    Ok(())
}

/// Processes a single log message.
///
/// This function extracts the payload from the message, decodes it if necessary,
/// prepares headers, and sends the data to the specified endpoint.
///
/// # Arguments
///
/// * `client` - An HTTP client for making outbound requests
/// * `message` - The JSON message to process
/// * `env_headers` - Pre-processed environment headers
///
/// # Returns
///
/// Returns `Ok(())` if processing is successful, or an error if any step fails.
#[instrument(skip(client, message, env_headers))]
async fn process_message(
    client: &Client,
    message: Value,
    env_headers: &EnvHeaders,
) -> Result<(), anyhow::Error> {
    info!("Starting to process message");

    // Check if the message contains the OpenTelemetry field
    if message.get(KEY_OTEL).is_none() {
        info!("Skipping record without _otel field");
        return Ok(());
    }

    info!("Message has _otel field, continuing processing");
    debug!("Full message structure: {:?}", message);

    // Extract and process the payload
    let payload = extract_payload(&message)?;
    info!("Extracted payload from message");

    // Decode the payload if it's base64 encoded
    let decoded_payload = decode_payload(&message, payload)?;
    debug!("Decoded payload (length: {} bytes)", decoded_payload.len());

    // Process the payload based on content type and encoding
    let final_payload = process_payload(&message, decoded_payload)?;
    debug!("Processed payload (length: {} bytes)", final_payload.len());

    // Prepare headers for the outbound request
    let mut headers = HeaderMap::new();
    extract_headers(&message, &mut headers)?;
    add_content_headers(&message, &mut headers)?;
    headers.extend(env_headers.0.clone()); // Add environment headers

    // Extract the endpoint and send the request
    let endpoint = message
        .get(KEY_ENDPOINT)
        .and_then(|e| e.as_str())
        .context("Endpoint is missing or not a string")?;
    debug!("Sending POST request to endpoint: {}", endpoint);

    send_request(client, endpoint, headers, final_payload).await
}

/// Extracts the payload from the message.
fn extract_payload(message: &Value) -> Result<String, anyhow::Error> {
    match message.get(KEY_PAYLOAD) {
        Some(p) => match p {
            Value::String(s) => Ok(s.clone()),
            Value::Object(o) => {
                serde_json::to_string(o).context("Failed to serialize payload object to string")
            }
            _ => Err(anyhow::anyhow!("Payload is neither a string nor an object")),
        },
        None => Err(anyhow::anyhow!("Payload field is missing from the message")),
    }
}

/// Decodes the payload if it's base64 encoded.
fn decode_payload(message: &Value, payload: String) -> Result<Vec<u8>, anyhow::Error> {
    if message.get(KEY_BASE64).and_then(|v| v.as_bool()) == Some(true) {
        general_purpose::STANDARD
            .decode(&payload)
            .context("Failed to decode base64 payload")
    } else {
        Ok(payload.into_bytes())
    }
}

/// Processes the payload based on content type and encoding.
fn process_payload(message: &Value, decoded_payload: Vec<u8>) -> Result<Vec<u8>, anyhow::Error> {
    if message.get(CONTENT_ENCODING_HEADER) == Some(&Value::String(ENCODING_GZIP.to_string())) {
        debug!("Payload is gzip-encoded, keeping as bytes");
        Ok(decoded_payload)
    } else if message.get(CONTENT_TYPE_HEADER)
        == Some(&Value::String(CONTENT_TYPE_JSON.to_string()))
    {
        let json_str = String::from_utf8(decoded_payload)
            .context("Failed to convert payload to UTF-8 string")?;
        let json_value: Value =
            serde_json::from_str(&json_str).context("Failed to parse payload as JSON")?;
        serde_json::to_vec(&json_value).context("Failed to convert JSON to bytes")
    } else {
        Ok(decoded_payload)
    }
}

/// Extracts headers from the message and adds them to the HeaderMap.
fn extract_headers(message: &Value, headers: &mut HeaderMap) -> Result<(), anyhow::Error> {
    let message_headers = match message.get(KEY_HEADERS).and_then(|h| h.as_object()) {
        Some(headers) => headers,
        None => return Ok(()), // No headers to process
    };

    for (key, value) in message_headers {
        let value_str = match value.as_str() {
            Some(s) => s,
            None => {
                warn!("Header value is not a string for key: {}", key);
                continue;
            }
        };

        let normalized_key = key.to_lowercase();
        let header_name = match HeaderName::from_str(&normalized_key) {
            Ok(name) => name,
            Err(_) => {
                warn!("Invalid header name: {}", normalized_key);
                continue;
            }
        };

        let header_value = match HeaderValue::from_str(value_str) {
            Ok(value) => value,
            Err(_) => {
                warn!("Invalid header value for {}: {}", normalized_key, value_str);
                continue;
            }
        };

        headers.insert(header_name, header_value);
        debug!(
            "Added header from message: {}={}",
            normalized_key, value_str
        );
    }

    Ok(())
}

/// Adds content-type and content-encoding headers if present in the message.
fn add_content_headers(message: &Value, headers: &mut HeaderMap) -> Result<(), anyhow::Error> {
    if let Some(content_type) = message.get(CONTENT_TYPE_HEADER).and_then(|ct| ct.as_str()) {
        headers.insert(
            HeaderName::from_static(CONTENT_TYPE_HEADER),
            HeaderValue::from_str(content_type)?,
        );
        debug!("Added/Updated content-type header: {}", content_type);
    }
    if let Some(content_encoding) = message
        .get(CONTENT_ENCODING_HEADER)
        .and_then(|ce| ce.as_str())
    {
        headers.insert(
            HeaderName::from_static(CONTENT_ENCODING_HEADER),
            HeaderValue::from_str(content_encoding)?,
        );
        debug!(
            "Added/Updated content-encoding header: {}",
            content_encoding
        );
    }
    Ok(())
}

/// Sends the HTTP request with the processed payload and headers.
async fn send_request(
    client: &Client,
    endpoint: &str,
    headers: HeaderMap,
    payload: Vec<u8>,
) -> Result<(), anyhow::Error> {
    let response = client
        .post(endpoint)
        .headers(headers)
        .body(payload)
        .send()
        .await
        .context("Failed to send POST request")?;

    if response.status().is_success() {
        info!(
            "Successfully posted message. Endpoint: {}, Status: {}, Headers: {:?}",
            endpoint,
            response.status(),
            response.headers()
        );
        Ok(())
    } else {
        let error_message = format!(
            "Failed to post message. Status: {}, Body: {:?}",
            response.status(),
            response.text().await
        );
        error!("{}", error_message);
        Err(anyhow!(error_message))
    }
}

#[tokio::main]
async fn main() -> Result<(), LambdaError> {
    // Initialize the logger with a specific log level
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .init();

    info!("Lambda function started");
    let client = Client::new();
    let env_headers = Arc::new(EnvHeaders::new()?);
    lambda_runtime::run(service_fn(|event| {
        function_handler(event, client.clone(), env_headers.clone())
    }))
    .await
}
