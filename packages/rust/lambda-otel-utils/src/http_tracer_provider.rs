//! This module provides utilities for configuring and building an OpenTelemetry TracerProvider
//! specifically tailored for use in AWS Lambda environments.
//!
//! It includes:
//! - `HttpTracerProviderBuilder`: A builder struct for configuring and initializing a TracerProvider.
//! - `get_lambda_resource`: A function to create a Resource with Lambda-specific attributes.
//!
//! The module supports various configuration options, including:
//! - Custom HTTP clients for exporting traces
//! - Enabling/disabling logging layers
//! - Setting custom tracer names
//! - Configuring propagators and ID generators
//! - Choosing between simple and batch exporters
//!
//! It also respects environment variables for certain configurations, such as the span processor type
//! and the OTLP exporter protocol.
//!
//! # Examples
//!
//! ```
//! use lambda_otel_utils::HttpTracerProviderBuilder;
//! use opentelemetry_sdk::trace::{TracerProvider, Tracer};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let tracer_provider: TracerProvider = HttpTracerProviderBuilder::default()
//!     .with_stdout_client()
//!     .enable_fmt_layer(true)
//!     .with_tracer_name("my-service")
//!     .with_default_text_map_propagator()
//!     .with_default_id_generator()
//!     .enable_global(true)
//!     .build()?;
//! # Ok(())
//! # }
//! ```
//!
//! This example demonstrates how to use the `HttpTracerProviderBuilder` to configure and build
//! a TracerProvider with custom settings for use in a Lambda function.

use std::time::Duration;

use delegate::delegate;
use opentelemetry::propagation::text_map_propagator::FieldIter;
use opentelemetry::propagation::TextMapPropagator;
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::{
    trace::{SpanId, TraceError, TraceId, TracerProvider},
    Context, KeyValue,
};
use opentelemetry_aws::{
    detector::LambdaResourceDetector,
    trace::{XrayIdGenerator, XrayPropagator},
};
use opentelemetry_http::HttpClient;
use opentelemetry_otlp::Protocol;
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    resource::Resource,
    resource::ResourceDetector,
    trace::{self, IdGenerator, RandomIdGenerator},
};
use otlp_stdout_client::StdoutClient;
use std::borrow::Cow;
use std::{env, fmt::Debug};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Debug)]
enum ExporterType {
    Simple,
    Batch,
}

#[derive(Debug)]
struct PropagatorWrapper(Box<dyn TextMapPropagator + Send + Sync>);

impl TextMapPropagator for PropagatorWrapper {
    delegate! {
        to self.0 {
            fn inject_context(&self, cx: &Context, carrier: &mut dyn Injector);
            fn extract_with_context(&self, cx: &Context, carrier: &dyn Extractor) -> Context;
            fn fields(&self) -> FieldIter<'_>;
        }
    }
}

#[derive(Debug)]
struct IdGeneratorWrapper(Box<dyn IdGenerator + Send + Sync>);

impl IdGenerator for IdGeneratorWrapper {
    delegate! {
        to self.0 {
            fn new_trace_id(&self) -> TraceId;
            fn new_span_id(&self) -> SpanId;
        }
    }
}

/// Builder for configuring and initializing a TracerProvider.
///
/// This struct provides a fluent interface for configuring various aspects of the
/// OpenTelemetry tracing setup, including the exporter type, propagators, and ID generators.
///
/// # Examples
///
/// ```
/// use lambda_otel_utils::HttpTracerProviderBuilder;
/// use opentelemetry_sdk::trace::{TracerProvider, Tracer};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let tracer_provider: TracerProvider = HttpTracerProviderBuilder::default()
///     .with_stdout_client()
///     .enable_fmt_layer(true)
///     .with_tracer_name("my-service")
///     .with_default_text_map_propagator()
///     .with_default_id_generator()
///     .enable_global(true)
///     .build()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct HttpTracerProviderBuilder<C: HttpClient + 'static = StdoutClient> {
    client: Option<C>,
    enable_fmt_layer: bool,
    install_global: bool,
    tracer_name: Option<Cow<'static, str>>,
    propagators: Vec<PropagatorWrapper>,
    id_generator: Option<IdGeneratorWrapper>,
    exporter_type: ExporterType,
}

/// Provides a default implementation for `HttpTracerProviderBuilder`.
///
/// This implementation creates a new `HttpTracerProviderBuilder` with default settings
/// by calling the `new()` method.
///
/// # Examples
///
/// ```
/// use lambda_otel_utils::HttpTracerProviderBuilder;
///
/// let default_builder = HttpTracerProviderBuilder::default();
/// ```
impl Default for HttpTracerProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTracerProviderBuilder {
    /// Creates a new `HttpTracerProviderBuilder` with default settings.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::new();
    /// ```
    pub fn new() -> Self {
        let exporter_type = match env::var("LAMBDA_OTEL_SPAN_PROCESSOR")
            .unwrap_or_else(|_| "simple".to_string())
            .to_lowercase()
            .as_str()
        {
            "batch" => ExporterType::Batch,
            "simple" => ExporterType::Simple,
            invalid => {
                eprintln!(
                    "Warning: Invalid LAMBDA_OTEL_SPAN_PROCESSOR value '{}'. Defaulting to Simple.",
                    invalid
                );
                ExporterType::Simple
            }
        };

        Self {
            client: None,
            install_global: false,
            enable_fmt_layer: false,
            tracer_name: None,
            propagators: Vec::new(),
            id_generator: None,
            exporter_type,
        }
    }

    /// Configures the builder to use a stdout client for exporting traces.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::default().with_stdout_client();
    /// ```
    pub fn with_stdout_client(mut self) -> Self {
        self.client = Some(StdoutClient::new());
        self
    }

    /// Enables or disables the fmt layer for logging.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::default().enable_fmt_layer(true);
    /// ```
    pub fn enable_fmt_layer(mut self, enabled: bool) -> Self {
        self.enable_fmt_layer = enabled;
        self
    }

    /// Sets the tracer name.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::default().with_tracer_name("my-service");
    /// ```
    pub fn with_tracer_name(mut self, tracer_name: impl Into<Cow<'static, str>>) -> Self {
        self.tracer_name = Some(tracer_name.into());
        self
    }

    /// Adds a custom text map propagator.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    /// use opentelemetry_sdk::propagation::TraceContextPropagator;
    ///
    /// let builder = HttpTracerProviderBuilder::default()
    ///     .with_text_map_propagator(TraceContextPropagator::new());
    /// ```
    pub fn with_text_map_propagator<P>(mut self, propagator: P) -> Self
    where
        P: TextMapPropagator + Send + Sync + 'static,
    {
        self.propagators
            .push(PropagatorWrapper(Box::new(propagator)));
        self
    }

    /// Adds the default text map propagator (TraceContextPropagator).
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::default().with_default_text_map_propagator();
    /// ```
    pub fn with_default_text_map_propagator(mut self) -> Self {
        self.propagators
            .push(PropagatorWrapper(Box::new(TraceContextPropagator::new())));
        self
    }

    /// Adds the XRay text map propagator.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::default().with_xray_text_map_propagator();
    /// ```
    pub fn with_xray_text_map_propagator(mut self) -> Self {
        self.propagators
            .push(PropagatorWrapper(Box::new(XrayPropagator::new())));
        self
    }

    /// Sets a custom ID generator.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    /// use opentelemetry_sdk::trace::RandomIdGenerator;
    ///
    /// let builder = HttpTracerProviderBuilder::default()
    ///     .with_id_generator(RandomIdGenerator::default());
    /// ```
    pub fn with_id_generator<I>(mut self, id_generator: I) -> Self
    where
        I: IdGenerator + Send + Sync + 'static,
    {
        self.id_generator = Some(IdGeneratorWrapper(Box::new(id_generator)));
        self
    }

    /// Sets the default ID generator (RandomIdGenerator).
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::default().with_default_id_generator();
    /// ```
    pub fn with_default_id_generator(mut self) -> Self {
        self.id_generator = Some(IdGeneratorWrapper(Box::new(RandomIdGenerator::default())));
        self
    }

    /// Adds the XRay ID generator.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::default().with_xray_id_generator();
    /// ```
    pub fn with_xray_id_generator(mut self) -> Self {
        self.id_generator = Some(IdGeneratorWrapper(Box::new(XrayIdGenerator::default())));
        self
    }

    /// Enables or disables global installation of the tracer provider.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::default().enable_global(true);
    /// ```
    pub fn enable_global(mut self, set_global: bool) -> Self {
        self.install_global = set_global;
        self
    }

    /// Configures the builder to use a simple exporter.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::default().with_simple_exporter();
    /// ```
    pub fn with_simple_exporter(mut self) -> Self {
        self.exporter_type = ExporterType::Simple;
        self
    }

    /// Configures the builder to use a batch exporter.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    ///
    /// let builder = HttpTracerProviderBuilder::default().with_batch_exporter();
    /// ```
    pub fn with_batch_exporter(mut self) -> Self {
        self.exporter_type = ExporterType::Batch;
        self
    }

    /// Builds the `TracerProvider` with the configured settings.
    ///
    /// # Examples
    ///
    /// ```
    /// use lambda_otel_utils::HttpTracerProviderBuilder;
    /// use opentelemetry_sdk::trace::{TracerProvider, Tracer};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let tracer_provider: TracerProvider = HttpTracerProviderBuilder::default()
    ///     .with_stdout_client()
    ///     .enable_fmt_layer(true)
    ///     .with_tracer_name("my-service")
    ///     .with_default_text_map_propagator()
    ///     .with_default_id_generator()
    ///     .enable_global(true)
    ///     .build()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn build(self) -> Result<opentelemetry_sdk::trace::TracerProvider, TraceError> {
        let protocol = match env::var("OTEL_EXPORTER_OTLP_PROTOCOL")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "http/protobuf" => Protocol::HttpBinary,
            "http/json" | "" => Protocol::HttpJson,
            unsupported => {
                eprintln!("Warning: OTEL_EXPORTER_OTLP_PROTOCOL value '{}' is not supported. Defaulting to HTTP JSON.", unsupported);
                Protocol::HttpJson
            }
        };

        let mut exporter = opentelemetry_otlp::new_exporter()
            .http()
            .with_protocol(protocol);

        if self.client.is_some() {
            exporter = exporter.with_http_client(self.client.unwrap());
        }

        let mut trace_config = trace::Config::default().with_resource(get_lambda_resource());

        if self.id_generator.is_some() {
            trace_config = trace_config.with_id_generator(self.id_generator.unwrap());
        }

        let pipeline = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(exporter)
            .with_trace_config(trace_config);

        let tracer_provider = match self.exporter_type {
            ExporterType::Simple => pipeline.install_simple()?,
            ExporterType::Batch => pipeline.install_batch(opentelemetry_sdk::runtime::Tokio)?,
        };

        let tracer = if let Some(tracer_name) = self.tracer_name {
            tracer_provider.tracer(tracer_name)
        } else {
            tracer_provider.tracer("default")
        };

        let registry = tracing_subscriber::Registry::default()
            .with(EnvFilter::from_default_env())
            .with(tracing_opentelemetry::OpenTelemetryLayer::new(tracer));

        if self.enable_fmt_layer {
            let log_format: String = std::env::var("AWS_LAMBDA_LOG_FORMAT").unwrap_or_default();
            if log_format.eq_ignore_ascii_case("json") {
                registry
                    .with(fmt::layer().with_target(false).without_time().json())
                    .init();
            } else {
                registry
                    .with(
                        fmt::layer()
                            .with_target(false)
                            .without_time()
                            .with_ansi(false),
                    )
                    .init();
            }
        } else {
            registry.init();
        }

        if !self.propagators.is_empty() {
            let composite_propagator = opentelemetry::propagation::TextMapCompositePropagator::new(
                self.propagators.into_iter().map(|p| p.0).collect(),
            );
            opentelemetry::global::set_text_map_propagator(composite_propagator);
        }

        if self.install_global {
            opentelemetry::global::set_tracer_provider(tracer_provider.clone());
        }

        Ok(tracer_provider)
    }
}

/// Retrieves the Lambda resource with the service name.
///
/// This function attempts to retrieve the service name from the `OTEL_SERVICE_NAME` environment variable.
/// If that variable is not set, it falls back to the `AWS_LAMBDA_FUNCTION_NAME` environment variable.
/// If neither variable is set, it defaults to "unknown-service".
///
/// The function then creates a new `Resource` with the detected Lambda resource information
/// and merges it with a new `Resource` containing the service name key-value pair.
///
/// # Returns
///
/// A `Resource` representing the Lambda resource with the service name.
pub fn get_lambda_resource() -> Resource {
    let service_name =
        match env::var("OTEL_SERVICE_NAME").or_else(|_| env::var("AWS_LAMBDA_FUNCTION_NAME")) {
            Ok(name) => name,
            Err(_) => "unknown-service".to_string(),
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
    use sealed_test::prelude::*;
    use std::env;

    #[test]
    fn test_http_tracer_provider_builder_default() {
        let builder = HttpTracerProviderBuilder::default();
        assert!(matches!(builder.exporter_type, ExporterType::Simple));
        assert!(builder.client.is_none());
        assert!(!builder.enable_fmt_layer);
        assert!(!builder.install_global);
        assert!(builder.tracer_name.is_none());
        assert!(builder.propagators.is_empty());
        assert!(builder.id_generator.is_none());
    }

    #[test]
    fn test_http_tracer_provider_builder_customization() {
        let builder = HttpTracerProviderBuilder::new()
            .with_stdout_client()
            .enable_fmt_layer(true)
            .with_tracer_name("test-tracer")
            .with_default_text_map_propagator()
            .with_default_id_generator()
            .enable_global(true)
            .with_batch_exporter();

        assert!(builder.client.is_some());
        assert!(builder.enable_fmt_layer);
        assert!(builder.install_global);
        assert_eq!(builder.tracer_name, Some(Cow::Borrowed("test-tracer")));
        assert_eq!(builder.propagators.len(), 1);
        assert!(builder.id_generator.is_some());
        assert!(matches!(builder.exporter_type, ExporterType::Batch));
    }

    #[sealed_test(env = [
        ("OTEL_SERVICE_NAME", "test-service"),
        ("AWS_LAMBDA_FUNCTION_NAME", "test-function"),
    ])]
    fn test_get_lambda_resource_with_otel_service_name() {
        let resource = get_lambda_resource();
        assert_eq!(
            resource.get(opentelemetry_semantic_conventions::resource::SERVICE_NAME.into()),
            Some("test-service".into())
        );
    }

    #[sealed_test(env = [
        ("AWS_LAMBDA_FUNCTION_NAME", "test-function"),
    ])]
    fn test_get_lambda_resource_with_aws_lambda_function_name() {
        env::set_var("AWS_LAMBDA_FUNCTION_NAME", "test-function");
        let resource = get_lambda_resource();
        assert_eq!(
            resource.get(opentelemetry_semantic_conventions::resource::SERVICE_NAME.into()),
            Some("test-function".into())
        );
    }

    #[sealed_test]
    fn test_get_lambda_resource_without_env_vars() {
        let resource = get_lambda_resource();
        assert_eq!(
            resource.get(opentelemetry_semantic_conventions::resource::SERVICE_NAME.into()),
            Some("unknown-service".into())
        );
    }
}
