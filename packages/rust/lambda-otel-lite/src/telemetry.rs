//! Telemetry initialization and configuration for AWS Lambda functions.
//!
//! This module provides the core functionality for setting up OpenTelemetry in Lambda functions:
//! - Configurable telemetry initialization
//! - Resource attribute management
//! - Span processor configuration
//! - Completion handling for different processing modes
//!
//! # Architecture
//!
//! The telemetry system consists of three main components:
//! 1. Configuration builder for customizing telemetry setup
//! 2. Completion handler for managing span export lifecycle
//! 3. Resource management for Lambda-specific attributes
//!
//! # Processing Modes
//!
//! Telemetry can be processed in three modes:
//! - `Sync`: Direct export in handler thread
//!   - Simple execution path with direct export
//!   - No IPC overhead with extension
//!   - Efficient for small payloads and low resource environments
//!   - Guarantees span delivery before response
//!
//! - `Async`: Export via Lambda extension
//!   - Requires coordination with extension process
//!   - Additional overhead from IPC
//!   - Best when advanced export features are needed
//!   - Provides retry capabilities through extension
//!
//! - `Finalize`: Custom export strategy
//!   - Full control over export timing and behavior
//!   - Compatible with BatchSpanProcessor
//!   - Best for specialized export requirements
//!
//! # Examples
//!
//! Basic usage with default configuration:
//! ```no_run
//! use lambda_otel_lite::{init_telemetry, TelemetryConfig};
//! use lambda_runtime::Error;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
//!     Ok(())
//! }
//! ```
//!
//! Custom configuration with resource attributes:
//! ```no_run
//! use lambda_otel_lite::{init_telemetry, TelemetryConfigBuilder};
//! use opentelemetry::KeyValue;
//! use opentelemetry_sdk::Resource;
//! use lambda_runtime::Error;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let resource = Resource::new(vec![
//!         KeyValue::new("service.version", "1.0.0"),
//!         KeyValue::new("deployment.environment", "production"),
//!     ]);
//!
//!     let config = TelemetryConfigBuilder::default()
//!         .with_resource(resource)
//!         .build();
//!
//!     let completion_handler = init_telemetry(config).await?;
//!     Ok(())
//! }
//! ```
//!
//! Custom configuration with span processor:
//! ```no_run
//! use lambda_otel_lite::{init_telemetry, TelemetryConfigBuilder};
//! use opentelemetry_sdk::trace::SimpleSpanProcessor;
//! use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;
//! use lambda_runtime::Error;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let config = TelemetryConfigBuilder::default()
//!         // Add a custom span processor (disables default Lambda processor)
//!         .with_span_processor(SimpleSpanProcessor::new(
//!             Box::new(OtlpStdoutSpanExporter::default())
//!         ))
//!         // Set custom library name (defaults to "lambda-otel-lite")
//!         .with_library_name("my-service")
//!         // Enable trace data in application logs (defaults to false)
//!         .with_fmt_layer(true)
//!         .build();
//!
//!     let completion_handler = init_telemetry(config).await?;
//!     Ok(())
//! }
//! ```
//!
//! # Environment Variables
//!
//! The following environment variables affect the configuration:
//! - `OTEL_SERVICE_NAME`: Service name for spans
//! - `OTEL_RESOURCE_ATTRIBUTES`: Additional resource attributes
//! - `LAMBDA_SPAN_PROCESSOR_QUEUE_SIZE`: Span buffer size
//! - `OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL`: Export compression
//! - `LAMBDA_TRACING_ENABLE_FMT_LAYER`: Enable formatting layer
//! - `LAMBDA_EXTENSION_SPAN_PROCESSOR_MODE`: Processing mode (sync/async/finalize)

use crate::{register_extension, LambdaSpanProcessor, ProcessorConfig, ProcessorMode};
use lambda_extension::Error;
use opentelemetry::{global::set_tracer_provider, trace::TracerProvider as _, KeyValue};
use opentelemetry::{otel_debug, otel_warn};
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    trace::{Builder as TracerProviderBuilder, SpanProcessor, TracerProvider as SdkTracerProvider},
    Resource,
};
use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;
use std::{borrow::Cow, collections::HashMap, env, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;
use tracing_subscriber::layer::SubscriberExt;
use urlencoding::decode;

/// A handler that ensures telemetry data is properly exported upon Lambda function completion.
///
/// This type manages the completion of telemetry data export based on the configured mode:
/// - In `Sync` mode, directly exports the data in the handler thread
///   - Simple execution path with no IPC overhead
///   - Efficient for small payloads and low resource environments
///   - Guarantees span delivery before response
///
/// - In `Async` mode, notifies the internal extension to export the data
///   - Requires coordination with extension process
///   - Additional overhead from IPC
///   - Provides retry capabilities through extension
///
/// - In `Finalize` mode, lets the processor handle the export strategy
///   - Full control over export timing and behavior
///   - Compatible with BatchSpanProcessor
///   - Best for specialized export requirements
///
/// # Thread Safety
///
/// The handler is thread-safe and can be cloned and shared between threads.
///
/// # Example
///
/// ```no_run
/// use lambda_otel_lite::{init_telemetry, TelemetryConfig};
/// use lambda_runtime::{service_fn, Error, LambdaEvent};
///
/// async fn handler(event: LambdaEvent<serde_json::Value>) -> Result<serde_json::Value, Error> {
///     // ... handler logic ...
///     Ok(event.payload)
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Error> {
///     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
///     lambda_runtime::run(service_fn(|event| async {
///         let result = handler(event).await;
///         completion_handler.complete();
///         result
///     })).await
/// }
/// ```
#[derive(Clone)]
pub struct TelemetryCompletionHandler {
    provider: Arc<SdkTracerProvider>,
    sender: Option<UnboundedSender<()>>,
    mode: ProcessorMode,
}

impl TelemetryCompletionHandler {
    /// Creates a new TelemetryCompletionHandler
    pub(crate) fn new(
        provider: Arc<SdkTracerProvider>,
        sender: Option<UnboundedSender<()>>,
        mode: ProcessorMode,
    ) -> Self {
        Self {
            provider,
            sender,
            mode,
        }
    }

    /// Returns the current processor mode
    pub fn mode(&self) -> ProcessorMode {
        self.mode.clone()
    }

    /// Get a reference to the tracer provider
    pub fn provider(&self) -> &Arc<SdkTracerProvider> {
        &self.provider
    }

    /// Signals completion of the current Lambda invocation, triggering telemetry export.
    ///
    /// The behavior depends on the processor mode:
    ///
    /// - In `Sync` mode:
    ///   - Directly exports spans in the handler thread
    ///   - Simple execution path with no IPC overhead
    ///   - Blocks until export completes
    ///   - Best for small payloads or when immediate delivery is required
    ///
    /// - In `Async` mode:
    ///   - Notifies the extension to handle export
    ///   - Non-blocking operation in handler thread
    ///   - Requires coordination with extension process
    ///   - Provides retry capabilities through extension
    ///
    /// - In `Finalize` mode:
    ///   - Lets the configured processor handle export
    ///   - Behavior depends on processor implementation
    ///   - Useful for custom export strategies
    ///   - No additional action taken by handler
    pub fn complete(&self) {
        match self.mode {
            ProcessorMode::Async => {
                if let Some(sender) = &self.sender {
                    if let Err(err) = sender.send(()) {
                        otel_warn!(
                            name: "TelemetryCompletionHandler.complete",
                            message = "error signaling completion",
                            reason = format!("{}", err)
                        );
                    }
                }
            }
            ProcessorMode::Sync => {
                for result in self.provider.force_flush() {
                    if let Err(err) = result {
                        otel_warn!(
                            name: "TelemetryCompletionHandler.complete",
                            message = "error exporting telemetry",
                            reason = format!("{}", err)
                        );
                    }
                }
            }
            ProcessorMode::Finalize => {
                // Let the processor handle it
                otel_debug!(
                    name: "TelemetryCompletionHandler.complete",
                    message = "letting processor handle telemetry export"
                );
            }
        }
    }

    /// Forces immediate export of all telemetry data, regardless of mode.
    ///
    /// This is useful when you need to ensure all telemetry data is exported
    /// immediately, such as during shutdown or error handling.
    pub fn force_export(&self) {
        for result in self.provider.force_flush() {
            if let Err(err) = result {
                tracing::warn!("[otel] error force exporting telemetry: {}", err);
            }
        }
    }
}

/// Get the default Lambda resource with AWS Lambda attributes and OTEL environment variables.
fn get_lambda_resource() -> Resource {
    let mut attributes = HashMap::new();

    // Add AWS Lambda attributes
    attributes.insert("cloud.provider".to_string(), "aws".to_string());

    // Map environment variables to attribute names
    let env_mappings = [
        ("AWS_REGION", "cloud.region"),
        ("AWS_LAMBDA_FUNCTION_NAME", "faas.name"),
        ("AWS_LAMBDA_FUNCTION_VERSION", "faas.version"),
        ("AWS_LAMBDA_LOG_STREAM_NAME", "faas.instance"),
        ("AWS_LAMBDA_FUNCTION_MEMORY_SIZE", "faas.max_memory"),
    ];

    // Add attributes only if they exist in environment
    for (env_var, attr_name) in env_mappings {
        if let Ok(value) = env::var(env_var) {
            attributes.insert(attr_name.to_string(), value);
        }
    }

    // Add service name (guaranteed to have a value)
    let service_name = env::var("OTEL_SERVICE_NAME")
        .or_else(|_| env::var("AWS_LAMBDA_FUNCTION_NAME"))
        .unwrap_or_else(|_| "unknown_service".to_string());
    attributes.insert("service.name".to_string(), service_name);

    // Add OTEL environment resource attributes if present
    if let Ok(env_resources) = env::var("OTEL_RESOURCE_ATTRIBUTES") {
        for item in env_resources.split(',') {
            if let Some((key, value)) = item.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                if !value.is_empty() {
                    if let Ok(decoded_value) = decode(value) {
                        attributes.insert(key.to_string(), decoded_value.into_owned());
                    }
                }
            }
        }
    }

    // Convert to KeyValue pairs
    let attributes: Vec<KeyValue> = attributes
        .into_iter()
        .map(|(k, v)| KeyValue::new(k, v))
        .collect();

    // Create resource and merge with default resource
    let resource = Resource::new(attributes);
    Resource::default().merge(&resource)
}

/// Configuration for OpenTelemetry telemetry initialization.
///
/// This struct holds the complete configuration for initializing OpenTelemetry:
/// - Resource attributes for identifying the Lambda function
/// - Tracer provider configuration
/// - Library name for instrumentation
/// - Formatting layer configuration
///
/// Rather than constructing directly, use [`TelemetryConfigBuilder`] for a more
/// flexible configuration experience.
///
/// # Default Configuration
///
/// The default configuration includes:
/// - Lambda resource attributes from environment variables
/// - LambdaSpanProcessor with OtlpStdoutSpanExporter
/// - Default library name from package name
/// - Formatting layer disabled (no duplicate traces in logs)
pub struct TelemetryConfig {
    resource: Resource,
    provider_builder: TracerProviderBuilder,
    library_name: Cow<'static, str>,
    enable_fmt_layer: bool,
}

impl Default for TelemetryConfig {
    /// Creates a standard Lambda telemetry configuration with:
    /// - Lambda resource attributes from environment variables
    /// - LambdaSpanProcessor with OtlpStdoutSpanExporter
    fn default() -> Self {
        TelemetryConfigBuilder::default().build()
    }
}

/// Builder for configuring OpenTelemetry telemetry initialization.
///
/// This builder provides a fluent interface for customizing telemetry configuration:
/// - Custom resources for additional attributes
/// - Custom span processors for different export strategies
/// - Custom library name for instrumentation
/// - Control over formatting layer for logs
///
/// # Environment Variables
///
/// Several environment variables affect the configuration:
/// - `OTEL_SERVICE_NAME`: Service name for spans
/// - `AWS_LAMBDA_FUNCTION_NAME`: Fallback service name
/// - `OTEL_RESOURCE_ATTRIBUTES`: Additional resource attributes
/// - `LAMBDA_SPAN_PROCESSOR_QUEUE_SIZE`: Span buffer size
/// - `OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL`: Export compression
/// - `LAMBDA_TRACING_ENABLE_FMT_LAYER`: Enable formatting layer (default: false)
///
/// # Example
///
/// ```no_run
/// use lambda_otel_lite::TelemetryConfigBuilder;
/// use opentelemetry_sdk::trace::SimpleSpanProcessor;
/// use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;
///
/// let config = TelemetryConfigBuilder::default()
///     .with_library_name("my-lambda")
///     .with_fmt_layer(true)
///     .with_span_processor(SimpleSpanProcessor::new(
///         Box::new(OtlpStdoutSpanExporter::default())
///     ))
///     .build();
/// ```
#[derive(Default)]
pub struct TelemetryConfigBuilder {
    resource: Option<Resource>,
    provider_builder: TracerProviderBuilder,
    has_custom_processors_pipeline: bool,
    library_name: Option<Cow<'static, str>>,
    enable_fmt_layer: Option<bool>,
}

impl TelemetryConfigBuilder {
    /// Add a custom resource to the tracer provider.
    ///
    /// If not set, the default Lambda resource will be used, which includes:
    /// - AWS Lambda environment attributes (region, function name, etc.)
    /// - OTEL_SERVICE_NAME or AWS_LAMBDA_FUNCTION_NAME as service.name
    /// - Any attributes from OTEL_RESOURCE_ATTRIBUTES environment variable
    pub fn with_resource(mut self, resource: Resource) -> Self {
        self.resource = Some(resource);
        self
    }

    /// Add a span processor to the tracer provider.
    ///
    /// Note: Adding any processor will disable the default Lambda processor pipeline.
    /// If you need multiple processors, call this method multiple times.
    ///
    /// # Example
    /// ```no_run
    /// use lambda_otel_lite::TelemetryConfigBuilder;
    /// use opentelemetry_sdk::trace::SimpleSpanProcessor;
    /// use otlp_stdout_span_exporter::OtlpStdoutSpanExporter;
    ///
    /// let config = TelemetryConfigBuilder::default()
    ///     .with_span_processor(SimpleSpanProcessor::new(Box::new(OtlpStdoutSpanExporter::default())))
    ///     .build();
    /// ```
    pub fn with_span_processor<T>(mut self, processor: T) -> Self
    where
        T: SpanProcessor + 'static,
    {
        self.has_custom_processors_pipeline = true;
        self.provider_builder = self.provider_builder.with_span_processor(processor);
        self
    }

    /// Set a custom library name for the tracer.
    ///
    /// If not set, defaults to the package name ("lambda-otel-lite").
    /// This value is used as the `library.name` attribute in spans.
    pub fn with_library_name<S>(mut self, name: S) -> Self
    where
        S: Into<Cow<'static, str>>,
    {
        self.library_name = Some(name.into());
        self
    }

    /// Enable or disable the formatting layer for logs.
    ///
    /// When enabled, trace data will also appear in application logs.
    /// Defaults to false if not set, can be overridden by LAMBDA_TRACING_ENABLE_FMT_LAYER.
    pub fn with_fmt_layer(mut self, enable: bool) -> Self {
        self.enable_fmt_layer = Some(enable);
        self
    }

    /// Build the final telemetry configuration.
    ///
    /// If no resource was set, uses the default Lambda resource.
    /// If no processors were added, uses the default Lambda processor pipeline.
    /// If fmt_layer not set, checks LAMBDA_TRACING_ENABLE_FMT_LAYER environment variable.
    pub fn build(self) -> TelemetryConfig {
        let resource = self.resource.unwrap_or_else(get_lambda_resource);
        let mut builder = self.provider_builder;

        // If no processors were added, use the default Lambda processor
        if !self.has_custom_processors_pipeline {
            let compression_level = env::var("OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(6);
            let exporter = Box::new(OtlpStdoutSpanExporter::with_gzip_level(compression_level));
            let max_queue_size = env::var("LAMBDA_SPAN_PROCESSOR_QUEUE_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2048);
            let processor = LambdaSpanProcessor::new(exporter, ProcessorConfig { max_queue_size });
            builder = builder.with_span_processor(processor);
        }

        let enable_fmt_layer = self.enable_fmt_layer.unwrap_or_else(|| {
            env::var("LAMBDA_TRACING_ENABLE_FMT_LAYER")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false)
        });

        TelemetryConfig {
            resource,
            provider_builder: builder,
            library_name: self
                .library_name
                .unwrap_or_else(|| Cow::Owned(env!("CARGO_PKG_NAME").to_string())),
            enable_fmt_layer,
        }
    }
}

impl TelemetryConfig {
    /// Build a tracer provider with this configuration
    pub fn build_provider(self) -> Result<SdkTracerProvider, Error> {
        Ok(self.provider_builder.with_resource(self.resource).build())
    }
}

/// Initialize telemetry base functionality without setting global state.
///
/// This function contains the core initialization logic without setting any global state:
/// 1. Creates a tracer provider with the given configuration
/// 2. Sets up the tracer provider
/// 3. Registers the internal extension if running in async/finalize mode
/// 4. Configures the tracing subscriber with appropriate formatting
///
/// Returns both the handler and subscriber for flexible usage in different contexts.
async fn init_telemetry_base(
    config: TelemetryConfig,
) -> Result<
    (
        TelemetryCompletionHandler,
        tracing_subscriber::layer::Layered<
            tracing_subscriber::EnvFilter,
            tracing_subscriber::layer::Layered<
                tracing_opentelemetry::OpenTelemetryLayer<
                    tracing_subscriber::Registry,
                    opentelemetry_sdk::trace::Tracer,
                >,
                tracing_subscriber::Registry,
            >,
        >,
    ),
    Error,
> {
    let mode = ProcessorMode::from_env();
    let library_name = config.library_name.clone();
    let provider = config.build_provider()?;
    let provider = Arc::new(provider);
    let tracer = provider.tracer(library_name);

    // Register the extension if in async or finalize mode
    let sender = match mode {
        ProcessorMode::Async | ProcessorMode::Finalize => {
            Some(register_extension(provider.clone(), mode.clone()).await?)
        }
        _ => None,
    };

    // Set the provider as global
    set_tracer_provider(provider.as_ref().clone());

    let handler = TelemetryCompletionHandler::new(provider.clone(), sender, mode);

    // initialize tracing subscriber
    let env_var_name = if env::var("RUST_LOG").is_ok() {
        "RUST_LOG"
    } else {
        "AWS_LAMBDA_LOG_LEVEL"
    };

    let env_filter = tracing_subscriber::EnvFilter::builder()
        .with_env_var(env_var_name)
        .from_env_lossy();

    let subscriber = tracing_subscriber::registry::Registry::default()
        .with(tracing_opentelemetry::OpenTelemetryLayer::new(
            tracer.clone(),
        ))
        .with(env_filter);

    Ok((handler, subscriber))
}

/// Initialize telemetry for a Lambda function.
///
/// This function sets up OpenTelemetry for use in a Lambda function:
/// 1. Creates a tracer provider with the given configuration
/// 2. Sets it as the global tracer provider
/// 3. Registers the internal extension if running in async/finalize mode
/// 4. Configures the tracing subscriber with appropriate formatting
///
/// The function automatically handles:
/// - Propagation context setup
/// - Resource attribute management
/// - Extension registration based on mode
/// - Logging format configuration
///
/// # Examples
///
/// Using with Tower middleware:
/// ```no_run
/// use lambda_otel_lite::{init_telemetry, TelemetryConfig, OtelTracingLayer};
/// use lambda_runtime::{service_fn, Error, LambdaEvent, Runtime};
/// use tower::ServiceBuilder;
/// use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
/// use serde_json::json;
///
/// async fn function_handler(event: LambdaEvent<ApiGatewayV2httpRequest>) -> Result<serde_json::Value, Error> {
///     Ok(json!({ "statusCode": 200 }))
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Error> {
///     // Initialize telemetry with default configuration
///     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
///     
///     // Build service with OpenTelemetry tracing middleware
///     let service = ServiceBuilder::new()
///         .layer(OtelTracingLayer::new(completion_handler)
///             .with_name("my-handler"))
///         .service_fn(function_handler);
///     
///     Runtime::new(service).run().await
/// }
/// ```
///
/// Using with traced handler:
/// ```no_run
/// use lambda_otel_lite::{init_telemetry, traced_handler, TelemetryConfig, TracedHandlerOptions};
/// use lambda_runtime::{service_fn, Error, LambdaEvent, Runtime};
/// use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
/// use serde_json::json;
///
/// async fn function_handler(event: LambdaEvent<ApiGatewayV2httpRequest>) -> Result<serde_json::Value, Error> {
///     Ok(json!({ "statusCode": 200 }))
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Error> {
///     // Initialize telemetry with default configuration
///     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
///
///     // Create the Lambda service with tracing
///     let func = service_fn(move |event| {
///         traced_handler(
///             TracedHandlerOptions::default()
///                 .with_name("my-handler")
///                 .with_event(event),
///             completion_handler.clone(),
///             function_handler,
///         )
///     });
///
///     Runtime::new(func).run().await
/// }
/// ```
///
/// # Environment Variables
///
/// * `RUST_LOG` or `AWS_LAMBDA_LOG_LEVEL`: Controls log filtering
/// * `AWS_LAMBDA_LOG_FORMAT`: Set to "JSON" for JSON formatted logs
///
/// # Arguments
///
/// * `config` - Configuration for telemetry initialization
///
/// # Returns
///
/// Returns a [`TelemetryCompletionHandler`] for managing telemetry completion
///
/// # Errors
///
/// Returns an error if:
/// - Failed to build tracer provider
/// - Failed to register extension
/// - Failed to set up tracing subscriber
pub async fn init_telemetry(config: TelemetryConfig) -> Result<TelemetryCompletionHandler, Error> {
    let enable_fmt_layer = config.enable_fmt_layer;
    let (handler, subscriber) = init_telemetry_base(config).await?;

    // Only add fmt layer if enabled in config
    if enable_fmt_layer {
        let is_json = env::var("AWS_LAMBDA_LOG_FORMAT")
            .unwrap_or_default()
            .to_uppercase()
            == "JSON";

        if is_json {
            let subscriber = subscriber.with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .without_time()
                    .json(),
            );
            tracing::subscriber::set_global_default(subscriber)?;
        } else {
            let subscriber = subscriber.with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                    .without_time()
                    .with_ansi(false),
            );
            tracing::subscriber::set_global_default(subscriber)?;
        }
    }
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
    Ok(handler)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::future::BoxFuture;
    use opentelemetry::trace::TraceResult;
    use opentelemetry::trace::Tracer;
    use opentelemetry_sdk::export::trace::{SpanData, SpanExporter};
    use serial_test::serial;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use std::thread;
    use std::time::Duration;
    use tracing_subscriber::util::SubscriberInitExt;

    // Test exporter that counts exports
    #[derive(Debug)]
    struct CountingExporter {
        export_count: Arc<AtomicUsize>,
        spans: Arc<Mutex<Vec<SpanData>>>,
    }

    impl CountingExporter {
        fn new() -> Self {
            Self {
                export_count: Arc::new(AtomicUsize::new(0)),
                spans: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl SpanExporter for CountingExporter {
        fn export(&mut self, spans: Vec<SpanData>) -> BoxFuture<'static, TraceResult<()>> {
            self.export_count.fetch_add(spans.len(), Ordering::SeqCst);
            self.spans.lock().unwrap().extend(spans);
            Box::pin(futures_util::future::ready(Ok(())))
        }
    }

    // Helper function to start a mock Lambda extension API server
    fn start_mock_server() -> String {
        // Find an available port
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        // Start a mock server in a separate thread
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                // Read the request
                let mut buffer = [0; 1024];
                if stream.read(&mut buffer).is_ok() {
                    // Send a response with extension ID header and proper JSON body
                    let body =
                        r#"{"functionName":"test","functionVersion":"1","handler":"test.handler"}"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\n\
                        Lambda-Extension-Identifier: test-extension-id\r\n\
                        Content-Type: application/json\r\n\
                        Content-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        });

        format!("http://{}", addr)
    }

    #[tokio::test]
    #[serial]
    async fn test_completion_handler_sync_mode() {
        // Start mock server
        let mock_endpoint = start_mock_server();
        std::env::set_var("AWS_LAMBDA_RUNTIME_API", mock_endpoint);

        let exporter = CountingExporter::new();
        let export_count = exporter.export_count.clone();

        let config = TelemetryConfigBuilder::default()
            .with_span_processor(opentelemetry_sdk::trace::SimpleSpanProcessor::new(
                Box::new(exporter),
            ))
            .build();

        let (handler, subscriber) = init_telemetry_base(config).await.unwrap();
        let _guard = subscriber.set_default();

        // Set some spans
        let tracer = opentelemetry::global::tracer("test");
        tracer.in_span("test_span", |_cx| {
            // Span will be exported on completion
        });

        // Complete should trigger export in sync mode
        handler.complete();

        // Give a moment for the export to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert!(export_count.load(Ordering::SeqCst) > 0);

        // Clean up
        std::env::remove_var("AWS_LAMBDA_RUNTIME_API");
    }

    #[tokio::test]
    #[serial]
    async fn test_completion_handler_async_mode() {
        // Start mock server
        let mock_endpoint = start_mock_server();
        std::env::set_var("AWS_LAMBDA_RUNTIME_API", mock_endpoint);
        // Set async mode
        std::env::set_var("LAMBDA_EXTENSION_SPAN_PROCESSOR_MODE", "async");

        let exporter = CountingExporter::new();

        let config = TelemetryConfigBuilder::default()
            .with_span_processor(opentelemetry_sdk::trace::SimpleSpanProcessor::new(
                Box::new(exporter),
            ))
            .build();

        let (handler, subscriber) = init_telemetry_base(config).await.unwrap();
        let _guard = subscriber.set_default();
        // Set some spans
        let tracer = opentelemetry::global::tracer("test");
        tracer.in_span("test_span", |_cx| {
            // Span will be exported via extension
        });

        // Complete should send message to extension
        handler.complete();

        // Give a moment for the export to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // In async mode, the extension would handle the export
        // We can't easily test the actual export here
        assert!(handler.sender.is_some());

        // Clean up
        std::env::remove_var("AWS_LAMBDA_RUNTIME_API");
        std::env::remove_var("LAMBDA_EXTENSION_SPAN_PROCESSOR_MODE");
    }

    #[test]
    fn test_lambda_resource_extraction() {
        // Set test environment variables
        std::env::set_var("AWS_REGION", "us-west-2");
        std::env::set_var("AWS_LAMBDA_FUNCTION_NAME", "test-function");
        std::env::set_var("AWS_LAMBDA_FUNCTION_VERSION", "1");
        std::env::set_var("AWS_LAMBDA_LOG_STREAM_NAME", "2023/01/01/[$LATEST]123");
        std::env::set_var("AWS_LAMBDA_FUNCTION_MEMORY_SIZE", "128");

        let resource = get_lambda_resource();
        let attrs: HashMap<String, String> = resource
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        assert_eq!(attrs.get("cloud.provider").unwrap(), "aws");
        assert_eq!(attrs.get("cloud.region").unwrap(), "us-west-2");
        assert_eq!(attrs.get("faas.name").unwrap(), "test-function");
        assert_eq!(attrs.get("faas.version").unwrap(), "1");
        assert_eq!(
            attrs.get("faas.instance").unwrap(),
            "2023/01/01/[$LATEST]123"
        );
        assert_eq!(attrs.get("faas.max_memory").unwrap(), "128");
    }

    #[tokio::test]
    async fn test_telemetry_config_builder() {
        let resource = Resource::new(vec![KeyValue::new("service.name", "test-service")]);

        let config = TelemetryConfigBuilder::default()
            .with_resource(resource.clone())
            .with_library_name("test-lib")
            .with_fmt_layer(false)
            .build();

        assert_eq!(config.library_name, "test-lib");
        assert!(!config.enable_fmt_layer);

        // Verify resource was properly set
        assert!(config
            .resource
            .iter()
            .any(|(k, v)| k.as_str() == "service.name" && v.as_str() == "test-service"));

        // Verify we can build a provider with this config
        let _provider = config.build_provider().unwrap();
    }
}
