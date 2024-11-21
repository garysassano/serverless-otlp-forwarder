//! # lambda-otel-utils
//!
//! `lambda-otel-utils` is a Rust library that provides utilities for integrating
//! OpenTelemetry tracing and metrics with AWS Lambda functions. It simplifies the process of
//! setting up and configuring OpenTelemetry for use in serverless environments.
//!
//! ## Main Components
//!
//! This crate consists of three main modules:
//!
//! - `http_otel_layer`: Provides the `HttpOtelLayer` for propagating
//!   context across HTTP boundaries.
//! - `http_tracer_provider`: Offers the `HttpTracerProviderBuilder` for configuring and
//!   building a custom TracerProvider tailored for Lambda environments.
//! - `http_meter_provider`: Provides the `HttpMeterProviderBuilder` for configuring and
//!   building a custom MeterProvider tailored for Lambda environments.
//!
//! ## Example
//!
//! ```rust,no_run
//! use lambda_runtime::{service_fn, Error, LambdaEvent};
//! use aws_lambda_events::event::apigw::ApiGatewayProxyRequest;
//! use lambda_otel_utils::{HttpOtelLayer, HttpTracerProviderBuilder, HttpMeterProviderBuilder};
//! use serde_json::Value;
//! use lambda_runtime::tower::Layer;
//! use std::time::Duration;
//!
//! async fn function_handler(event: LambdaEvent<ApiGatewayProxyRequest>) -> Result<Value, Error> {
//!     Ok(serde_json::json!({"message": "Hello from Lambda!"}))
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     // Initialize tracer provider
//!     let tracer_provider = HttpTracerProviderBuilder::default()
//!         .with_stdout_client()
//!         .with_tracer_name("my-lambda-function")
//!         .build()?;
//!     
//!     // Initialize meter provider
//!     let meter_provider = HttpMeterProviderBuilder::default()
//!         .with_stdout_client()
//!         .with_meter_name("my-lambda-function")
//!         .with_export_interval(Duration::from_secs(60))
//!         .build()?;
//!     
//!     // Clone providers for the flush function
//!     let tracer_provider_clone = tracer_provider.clone();
//!     let meter_provider_clone = meter_provider.clone();
//!     
//!     // Create service with tracing layer
//!     let service = HttpOtelLayer::new(move || {
//!         tracer_provider_clone.force_flush();
//!         meter_provider_clone.force_flush();
//!     })
//!     .layer(service_fn(function_handler));
//!
//!     // Run the Lambda runtime
//!     lambda_runtime::run(service).await?;
//!     Ok(())
//! }
//! ```
//!
//! This example demonstrates how to set up a comprehensive tracing and metrics configuration using
//! `lambda-otel-utils` in a Lambda function, including both the TracerProvider, MeterProvider, and
//! the context propagation layer.

pub mod http_meter_provider;
pub mod http_otel_layer;
pub mod http_tracer_provider;

pub use http_meter_provider::HttpMeterProviderBuilder;
pub use http_otel_layer::HttpOtelLayer;
pub use http_tracer_provider::HttpTracerProviderBuilder;
pub use lambda_runtime::tower::Layer;
