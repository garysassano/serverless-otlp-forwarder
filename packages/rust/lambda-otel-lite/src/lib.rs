//! OpenTelemetry instrumentation optimized for AWS Lambda functions.
//!
//! This crate provides a lightweight, efficient implementation of OpenTelemetry tracing
//! specifically designed for AWS Lambda functions. It offers flexible processing modes,
//! automatic resource detection, and integration with the Lambda Extensions API.
//!
//! # Features
//!
//! - **Flexible Processing Modes**: Support for synchronous, asynchronous, and custom export strategies
//! - **Automatic Resource Detection**: Automatic extraction of Lambda environment attributes
//! - **Lambda Extension Integration**: Built-in extension for efficient telemetry export
//! - **Efficient Memory Usage**: Fixed-size ring buffer to prevent memory growth
//! - **AWS Event Support**: Automatic extraction of attributes from common AWS event types
//!
//! # Architecture
//!
//! The crate is organized into several modules, each handling a specific aspect of telemetry:
//!
//! - [`telemetry`]: Core initialization and configuration
//! - [`processor`]: Span processing and export strategies
//! - [`extension`]: Lambda extension implementation
//! - [`layer`]: Tower middleware for automatic instrumentation
//! - [`handler`]: Function wrapper for manual instrumentation
//!
//! # Quick Start
//!
//! ```no_run
//! use lambda_otel_lite::{init_telemetry, TelemetryConfig};
//! use lambda_runtime::{service_fn, Error, LambdaEvent};
//! use serde_json::Value;
//!
//! async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
//!     Ok(event.payload)
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     // Initialize telemetry with default configuration
//!     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
//!
//!     // Run the Lambda function
//!     lambda_runtime::run(service_fn(|event| async {
//!         let result = handler(event).await;
//!         completion_handler.complete();
//!         result
//!     })).await
//! }
//! ```
//!
//! # Processing Modes
//!
//! The crate supports three processing modes for telemetry data:
//!
//! - **Sync Mode**: Direct export in handler thread
//!   - Simple execution path with no IPC overhead
//!   - Efficient for small payloads and low resource environments
//!   - Guarantees span delivery before response
//!
//! - **Async Mode**: Export via Lambda extension
//!   - Requires coordination with extension process
//!   - Additional overhead from IPC
//!   - Best when advanced export features are needed
//!   - Provides retry capabilities through extension
//!
//! - **Finalize Mode**: Custom export strategy
//!   - Full control over export timing and behavior
//!   - Compatible with BatchSpanProcessor
//!   - Best for specialized export requirements
//!
//! See [`processor`] module for detailed documentation on processing modes.
//!
//! # Configuration
//!
//! Configuration is handled through environment variables:
//!
//! - `LAMBDA_EXTENSION_SPAN_PROCESSOR_MODE`: Controls processing mode
//!   - "sync" for Sync mode (default)
//!   - "async" for Async mode
//!   - "finalize" for Finalize mode
//!
//! - `LAMBDA_SPAN_PROCESSOR_QUEUE_SIZE`: Controls buffer size
//!   - Defaults to 2048 spans
//!   - Should be tuned based on span volume
//!
//! - `OTEL_SERVICE_NAME`: Service name for spans
//!   - Falls back to AWS_LAMBDA_FUNCTION_NAME
//!   - Required for proper service identification
//!
//! See [`telemetry`] module for detailed configuration options.
//!
//! # Examples
//!
//! ## Using the Tower Layer
//!
//! ```no_run
//! use lambda_otel_lite::{init_telemetry, TelemetryConfig, OtelTracingLayer};
//! use lambda_runtime::{service_fn, Error, LambdaEvent};
//! use tower::ServiceBuilder;
//! use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
//!
//! async fn handler(event: LambdaEvent<ApiGatewayV2httpRequest>) -> Result<serde_json::Value, Error> {
//!     Ok(serde_json::json!({"status": "ok"}))
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
//!
//!     let service = ServiceBuilder::new()
//!         .layer(OtelTracingLayer::new(completion_handler.clone()))
//!         .service_fn(handler);
//!
//!     lambda_runtime::run(service).await
//! }
//! ```
//!
//! See [`layer`] module for more examples of automatic instrumentation.
//!
//! ## Manual Instrumentation
//!
//! ```no_run
//! use lambda_otel_lite::{init_telemetry, TelemetryConfig, traced_handler, TracedHandlerOptions};
//! use lambda_runtime::{service_fn, Error, LambdaEvent};
//! use serde_json::Value;
//!
//! async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
//!     Ok(event.payload)
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
//!
//!     lambda_runtime::run(service_fn(|event: LambdaEvent<Value>| {
//!         traced_handler(
//!             TracedHandlerOptions::default().with_name("my-handler"),
//!             completion_handler.clone(),
//!             handler,
//!         )
//!     })).await
//! }
//! ```
//!
//! See [`handler`] module for more examples of manual instrumentation.

pub use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;

mod extension;
mod handler;
mod layer;
mod processor;
mod telemetry;

pub use extension::{register_extension, OtelInternalExtension};
pub use handler::{traced_handler, TracedHandlerOptions};
pub use layer::{OtelTracingLayer, SpanAttributes, SpanAttributesExtractor};
pub use processor::{LambdaSpanProcessor, ProcessorConfig, ProcessorMode};
pub use telemetry::{
    init_telemetry, TelemetryCompletionHandler, TelemetryConfig, TelemetryConfigBuilder,
};

#[cfg(doctest)]
#[macro_use]
extern crate doc_comment;

#[cfg(doctest)]
use doc_comment::doctest;

#[cfg(doctest)]
doctest!("../README.md", readme);
