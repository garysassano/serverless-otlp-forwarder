//! # otlp-stdout-client
//!
//! `otlp-stdout-client` is a Rust library that provides an OpenTelemetry exporter
//! designed for serverless environments, particularly AWS Lambda functions.
//!
//! This exporter implements the `opentelemetry_http::HttpClient` interface and can be used
//! in an OpenTelemetry OTLP pipeline to send OTLP data (both JSON and Protobuf formats)
//! to stdout. This allows the data to be easily ingested and forwarded to an OTLP collector.
//!
//! ## Note on Dependencies
//!
//! This crate currently includes a local implementation of the `LambdaResourceDetector`
//! from the `opentelemetry-aws` crate. This is a temporary measure while waiting for the
//! `opentelemetry-aws` crate to be updated to version 0.13.0. Once the update is available,
//! this local implementation will be removed in favor of the official crate dependency.
//!
//! ## Key Features
//!
//! - Implements `opentelemetry_http::HttpClient` for use in OTLP pipelines
//! - Exports OpenTelemetry data to stdout in a structured format
//! - Supports both HTTP/JSON and HTTP/Protobuf OTLP records
//! - Designed for serverless environments, especially AWS Lambda
//! - Configurable through environment variables
//! - Includes Lambda-specific resource detection
//! - Optional GZIP compression of payloads
//!
//! ## Usage
//!
//! This library can be integrated into OpenTelemetry OTLP pipelines to redirect
//! telemetry data to stdout. It's particularly useful in serverless environments
//! where direct network access to a collector might not be available or desired.
//!
//! ```rust
//! use otlp_stdout_client::{StdoutClient, init_tracer_provider};
//! use opentelemetry::trace::TracerProvider;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize a tracer provider with the StdoutClient
//!     let tracer_provider = init_tracer_provider()?;
//!     
//!     // Use the tracer for instrumenting your code
//!     let tracer = tracer_provider.tracer("my-service");
//!     
//!     // Your instrumented code here...
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Configuration
//!
//! The exporter can be configured using the following environment variables.
//!
//! - `OTEL_EXPORTER_OTLP_PROTOCOL`: Specifies the protocol to use for the OTLP exporter.
//!   Valid values are:
//!   - "http/json" (default): Uses HTTP with JSON payload
//!   - "http/protobuf": Uses HTTP with Protobuf payload
//! 
//!     If not set or set to an unsupported value, defaults to "http/json".
//!     The output will be written to stdout in the format specified by the protocol.
//!
//! - `OTEL_EXPORTER_OTLP_ENDPOINT`: Sets the endpoint for the OTLP exporter.
//!   Specify the endpoint of your OTLP collector.
//!   A field `endpoint` will be added to the output, and can be used by the forwarder to actually send the data.
//!
//! - `OTEL_EXPORTER_OTLP_HEADERS`: Sets additional headers for the OTLP exporter.
//!   Format: "key1=value1,key2=value2"
//!   These headers will be added to the output under the `headers` field. If your collector requires
//!   authentication, it's probably better to not add them here, as they will be visible in the logs.
//!   You should add the authentication information in the forwarder, and not here.
//!
//! - `OTEL_EXPORTER_OTLP_TIMEOUT`: Sets the timeout for the OTLP exporter in milliseconds.
//!   If not set, a default timeout is used.
//!   Since the output is written to stdout, this setting is not used.
//!
//! - `OTEL_EXPORTER_OTLP_COMPRESSION`: Specifies the compression algorithm to use.
//!   Valid values are:
//!   - "gzip": Compresses the payload using GZIP
//!   - If not set or any other value, no compression is applied
//! 
//!     If you specify a compression algorithm, the payload will be compressed before being written to stdout, and encoded as base64.
//!     The `content-encoding` header will be also added as a field that can be used by the forwarder to determine the compression algorithm to both decode the payload and set the correct `Content-Encoding` header in the request to the collector.
//!   
//! - `OTEL_SERVICE_NAME`: Sets the service name for the Lambda function.
//!   If not set, falls back to the name of the lambda function.
//!
//! ## Usage
//!
//! This crate provides functions to initialize tracer and meter providers
//! that use the stdout exporter. These can be easily integrated into your
//! Lambda functions or other serverless applications. Please refer to the main README.md for the lambda-otlp-forwarder project for more information.
//!
//! For detailed usage instructions, see the documentation for `init_tracer_provider`
//! and `init_meter_provider` functions.

use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use flate2::{write::GzEncoder, Compression};
use http::{Request, Response};
use opentelemetry::trace::TraceError;
use opentelemetry::KeyValue;
use opentelemetry_http::HttpClient;
use opentelemetry_otlp::Protocol;
use opentelemetry_sdk::{
    resource::{Resource, ResourceDetector},
    trace::{self as sdktrace, Config},
};
use serde_json::{json, Value};
use std::env;
use std::time::Duration;
use std::{
    error::Error as StdError,
    fmt::Debug,
    io::{self, Write},
};
use tracing::warn;

mod lambda_detector;
use lambda_detector::LambdaResourceDetector;

// Constants for content types
const CONTENT_TYPE_JSON: &str = "application/json";
const CONTENT_TYPE_PROTOBUF: &str = "application/x-protobuf";

// Constants for headers
const CONTENT_TYPE_HEADER: &str = "content-type";
const CONTENT_ENCODING_HEADER: &str = "content-encoding";

// Constants for JSON keys
const KEY_OTEL: &str = "_otel";
const KEY_ENDPOINT: &str = "endpoint";
const KEY_METHOD: &str = "method";
const KEY_PAYLOAD: &str = "payload";
const KEY_BASE64: &str = "base64";
const KEY_HEADERS: &str = "headers";

// Constant for OTEL version prefix
const OTEL_VERSION_PREFIX: &str = "oltp-stdout-";

// Constant for GZIP encoding
const ENCODING_GZIP: &str = "gzip";

#[derive(Debug)]
pub struct StdoutClient {
    use_gzip: bool,
}

impl StdoutClient {
    pub fn new() -> Self {
        let use_gzip = env::var("OTEL_EXPORTER_OTLP_COMPRESSION")
            .map(|v| v.to_lowercase() == "gzip")
            .unwrap_or(false);

        StdoutClient { use_gzip }
    }
}

impl Default for StdoutClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Implements the `HttpClient` trait for `StdoutClient`.
///
/// This implementation allows the `StdoutClient` to be used as an HTTP client
/// for sending OTLP (OpenTelemetry Protocol) data. It processes the request body
/// and writes it to stdout in a JSON format suitable for log parsing.
///
/// The `send` method handles both JSON and non-JSON payloads, formatting them
/// appropriately for stdout output.
#[async_trait]
impl HttpClient for StdoutClient {
    async fn send(
        &self,
        request: Request<Vec<u8>>,
    ) -> Result<Response<Bytes>, Box<dyn StdError + Send + Sync>> {
        let content_type = request
            .headers()
            .get(CONTENT_TYPE_HEADER)
            .and_then(|ct| ct.to_str().ok());

        match content_type {
            Some(CONTENT_TYPE_JSON) => {
                self.process_payload(request, CONTENT_TYPE_JSON, false)?;
            }
            Some(CONTENT_TYPE_PROTOBUF) => {
                self.process_payload(request, CONTENT_TYPE_PROTOBUF, true)?;
            }
            _ => {
                let message = match content_type {
                    Some(ct) => format!("Content type '{}' is not supported", ct),
                    None => "Content type not specified".to_string(),
                };
                warn!("{message}. Skipping processing.");
                return Ok(Response::builder().status(200).body(Bytes::new()).unwrap());
            }
        }

        Ok(Response::builder().status(200).body(Bytes::new()).unwrap())
    }
}

impl StdoutClient {
    fn process_payload(
        &self,
        request: Request<Vec<u8>>,
        content_type: &str,
        is_binary: bool,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        let (processed_payload, mut is_binary) = if !is_binary {
            let json_value: Value = serde_json::from_slice(request.body())?;
            (serde_json::to_vec(&json_value)?, false)
        } else {
            (request.body().to_vec(), true)
        };

        let final_payload = if self.use_gzip {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&processed_payload)?;
            is_binary = true;
            encoder.finish()?
        } else {
            processed_payload
        };

        let payload_value = if is_binary {
            Value::String(general_purpose::STANDARD.encode(&final_payload))
        } else {
            serde_json::from_slice(&final_payload)?
        };

        let headers_json = self.headers_to_json(request.headers());

        let mut output = json!({
            KEY_OTEL: format!("{}{}-{}", OTEL_VERSION_PREFIX, env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
            CONTENT_TYPE_HEADER: content_type,
            KEY_ENDPOINT: request.uri().to_string(),
            KEY_METHOD: request.method().to_string(),
            KEY_PAYLOAD: payload_value,
            KEY_HEADERS: headers_json
        });

        if self.use_gzip {
            output[CONTENT_ENCODING_HEADER] = json!(ENCODING_GZIP);
        }
        if is_binary {
            output[KEY_BASE64] = json!(true);
        }
        let compact_json = serde_json::to_string(&output)?;
        writeln!(io::stdout(), "{}", compact_json)?;

        Ok(())
    }

    fn headers_to_json(&self, headers: &http::HeaderMap) -> Value {
        let mut headers_map = serde_json::Map::new();
        for (name, value) in headers.iter() {
            if let Ok(v) = value.to_str() {
                // Convert the header name to lowercase
                headers_map.insert(name.as_str().to_lowercase(), Value::String(v.to_string()));
            }
        }
        Value::Object(headers_map)
    }
}

/// Initializes and returns a new TracerProvider configured for stdout output.
///
/// # Examples
///
/// ```
/// use otlp_stdout_client::init_tracer_provider;
/// use opentelemetry::trace::TracerProvider;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let tracer_provider = init_tracer_provider()?;
///     let tracer = tracer_provider.tracer("my-service");
///     
///     // Use the tracer for instrumenting your code
///     // ...
///
///     Ok(())
/// }
/// ```
///
/// # Returns
///
/// Returns a `Result` containing the initialized `TracerProvider` on success,
/// or a `TraceError` if initialization fails.

pub fn init_tracer_provider() -> Result<sdktrace::TracerProvider, TraceError> {
    let protocol = match env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "http/protobuf" => Protocol::HttpBinary,
        "http/json" | "" => Protocol::HttpJson,
        unsupported => {
            eprintln!(
                "Warning: OTEL_EXPORTER_OTLP_PROTOCOL value '{}' is not supported. Defaulting to HTTP JSON.",
                unsupported
            );
            Protocol::HttpJson
        }
    };

    let exporter = opentelemetry_otlp::new_exporter()
        .http()
        .with_protocol(protocol)
        .with_http_client(StdoutClient::new());

    let lambda_resource = get_lambda_resource();

    let tracer_provider = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(Config::default().with_resource(lambda_resource))
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    opentelemetry::global::set_tracer_provider(tracer_provider.clone());

    Ok(tracer_provider)
}

/// Initializes and returns a new MeterProvider configured for stdout output.
///
/// This function sets up a MeterProvider that uses the StdoutClient for exporting
/// metrics data. It's configured to use the HTTP JSON protocol and includes
/// Lambda-specific resource information.
///
/// # Returns
///
/// Returns a `Result` containing the initialized `SdkMeterProvider` on success,
/// or a `MetricsError` if initialization fails.
///
/// # Examples
///
/// ```
/// use otlp_stdout_client::init_meter_provider;
/// use opentelemetry::metrics::MeterProvider;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let meter_provider = init_meter_provider()?;
///     let meter = meter_provider.meter("my-service");
///     
///     // Use the meter for creating instruments and recording metrics
///     // ...
///
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// This function will return an error if:
/// * The OTLP exporter cannot be created
/// * The meter provider cannot be built
#[cfg(feature = "metrics")]
use opentelemetry::metrics::MetricsError;
use opentelemetry_sdk::metrics;

#[cfg(feature = "metrics")]
pub fn init_meter_provider() -> Result<metrics::SdkMeterProvider, MetricsError> {
    let exporter = opentelemetry_otlp::new_exporter()
        .http()
        .with_protocol(Protocol::HttpJson)
        .with_http_client(StdoutClient::new());

    let lambda_resource = get_lambda_resource();
    let meter_provider = opentelemetry_otlp::new_pipeline()
        .metrics(opentelemetry_sdk::runtime::Tokio)
        .with_exporter(exporter)
        .with_resource(lambda_resource)
        .build()?;

    opentelemetry::global::set_meter_provider(meter_provider.clone());

    Ok(meter_provider)
}

/// Retrieves the Lambda resource information for OpenTelemetry.
///
/// This function creates a `Resource` object that includes Lambda-specific
/// information. It attempts to set the service name from environment variables,
/// prioritizing `OTEL_SERVICE_NAME` over `AWS_LAMBDA_FUNCTION_NAME`.
///
/// # Returns
///
/// Returns a `Resource` object containing Lambda-specific information and the service name.
///
/// # Examples
///
/// ```
/// use otlp_stdout_client::get_lambda_resource;
/// use opentelemetry_sdk::resource::Resource;
///
/// let lambda_resource = get_lambda_resource();
/// // Use lambda_resource in your OpenTelemetry configuration
/// ```
pub fn get_lambda_resource() -> Resource {
    let service_name =
        match env::var("OTEL_SERVICE_NAME").or_else(|_| env::var("AWS_LAMBDA_FUNCTION_NAME")) {
            Ok(name) => name,
            Err(_) => "unknown-function".to_string(),
        };

    LambdaResourceDetector
        .detect(Duration::default())
        .merge(&Resource::new(vec![KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            service_name,
        )]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{HeaderMap, HeaderValue};

    #[tokio::test]
    async fn test_stdout_client_new() {
        let client = StdoutClient::new();
        assert!(!client.use_gzip); // Assuming OTEL_EXPORTER_OTLP_COMPRESSION is not set
    }

    #[tokio::test]
    async fn test_stdout_client_send_json() {
        let client = StdoutClient::new();
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE_HEADER,
            HeaderValue::from_static(CONTENT_TYPE_JSON),
        );

        let body = json!({
            "test": "data"
        })
        .to_string()
        .into_bytes();

        let request = Request::builder()
            .method("POST")
            .uri("http://example.com/v1/metrics")
            .header(CONTENT_TYPE_HEADER, CONTENT_TYPE_JSON)
            .body(body)
            .unwrap();

        let result = client.send(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_init_tracer_provider() {
        let result = init_tracer_provider();
        assert!(result.is_ok());
    }

    #[cfg(feature = "metrics")]
    #[tokio::test]
    async fn test_init_meter_provider() {
        let result = init_meter_provider();
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_lambda_resource() {
        let resource = get_lambda_resource();
        assert!(resource
            .iter()
            .any(|(key, _)| key.as_str()
                == opentelemetry_semantic_conventions::resource::SERVICE_NAME));
    }
}