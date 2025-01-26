//! Tower middleware for OpenTelemetry tracing in AWS Lambda functions.
//!
//! This module provides a Tower middleware layer that automatically creates OpenTelemetry spans
//! for Lambda invocations. It supports automatic extraction of span attributes from common AWS
//! event types and allows for custom attribute extraction through a flexible trait system.
//!
//! # Architecture
//!
//! The layer operates by wrapping a Lambda service with OpenTelemetry instrumentation:
//! 1. Creates a span for each Lambda invocation
//! 2. Extracts attributes from the event using either:
//!    - Built-in implementations of `SpanAttributesExtractor` for supported event types
//!    - Custom implementations of `SpanAttributesExtractor` for user-defined types
//!    - A closure-based extractor for one-off customizations
//! 3. Propagates context from incoming requests via headers
//! 4. Tracks response status codes
//! 5. Signals completion for span export through the `TelemetryCompletionHandler`
//!
//! # Features
//!
//! - Automatic span creation for Lambda invocations
//! - Built-in support for common AWS event types:
//!   - API Gateway v1/v2 (HTTP method, path, route, protocol)
//!   - Application Load Balancer (HTTP method, path, target group ARN)
//! - Extensible attribute extraction through the `SpanAttributesExtractor` trait
//! - Custom attribute extraction through closure-based extractors
//! - Automatic context propagation from HTTP headers
//! - Response status code tracking
//!
//! # Basic Usage
//!
//! ```no_run
//! use lambda_otel_lite::{init_telemetry, OtelTracingLayer, TelemetryConfig};
//! use lambda_runtime::{service_fn, Error, LambdaEvent, Runtime};
//! use aws_lambda_events::event::apigw::ApiGatewayV2httpRequest;
//! use tower::ServiceBuilder;
//!
//! async fn function_handler(event: LambdaEvent<ApiGatewayV2httpRequest>) -> Result<serde_json::Value, Error> {
//!     Ok(serde_json::json!({"statusCode": 200}))
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
//!     
//!     let service = ServiceBuilder::new()
//!         .layer(OtelTracingLayer::new(completion_handler)
//!             .with_name("my-handler"))
//!         .service_fn(function_handler);
//!
//!     Runtime::new(service).run().await
//! }
//! ```
//!
//! # Automatic Attribute Extraction
//!
//! The layer automatically extracts attributes from supported event types through the
//! `SpanAttributesExtractor` trait. Built-in implementations include:
//!
//! - API Gateway v2:
//!   - `http.method`: The HTTP method used
//!   - `http.target`: The request path
//!   - `http.route`: The route key
//!   - `http.scheme`: The protocol used
//!
//! - API Gateway v1:
//!   - `http.method`: The HTTP method used
//!   - `http.target`: The request path
//!   - `http.route`: The resource path
//!   - `http.scheme`: The protocol used
//!
//! - Application Load Balancer:
//!   - `http.method`: The HTTP method used
//!   - `http.target`: The request path
//!   - `alb.target_group_arn`: The ALB target group ARN
//!
//! # Custom Attribute Extraction
//!
//! You can implement the `SpanAttributesExtractor` trait for your own event types:
//!
//! ```no_run
//! use lambda_otel_lite::{SpanAttributes, SpanAttributesExtractor};
//! use std::collections::HashMap;
//!
//! struct MyEvent {
//!     user_id: String,
//! }
//!
//! impl SpanAttributesExtractor for MyEvent {
//!     fn extract_span_attributes(&self) -> SpanAttributes {
//!         let mut attributes = HashMap::new();
//!         attributes.insert("user.id".to_string(), self.user_id.clone());
//!         SpanAttributes {
//!             attributes,
//!             ..SpanAttributes::default()
//!         }
//!     }
//! }
//! ```
//!
//! Or use the closure-based extractor for one-off customizations:
//!
//! ```no_run
//! use lambda_otel_lite::{init_telemetry, OtelTracingLayer, TelemetryConfig, SpanAttributes};
//! use std::collections::HashMap;
//! use lambda_runtime::Error;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
//!     let layer: OtelTracingLayer<serde_json::Value> = OtelTracingLayer::new(completion_handler)
//!         .with_name("my-handler")
//!         .with_extractor_fn(|event| {
//!         let mut attributes = HashMap::new();
//!         attributes.insert("custom.field".to_string(), "value".to_string());
//!         SpanAttributes {
//!             attributes,
//!             ..SpanAttributes::default()
//!         }
//!     });
//!     Ok(())
//! }
//! ```
//! # Context Propagation
//!
//! The layer automatically extracts and propagates tracing context from HTTP headers
//! in supported event types. This enables distributed tracing across service boundaries.
//! The W3C Trace Context format is used for propagation.
//!
//! # Response Tracking
//!
//! For HTTP responses, the layer automatically:
//! - Sets `http.status_code` from the response statusCode
//! - Sets span status to ERROR for 5xx responses
//! - Sets span status to OK for all other responses

use aws_lambda_events::event::alb::AlbTargetGroupRequest;
use aws_lambda_events::event::apigw::{ApiGatewayProxyRequest, ApiGatewayV2httpRequest};
use futures_util::ready;
use lambda_runtime::{Error, LambdaEvent};
use opentelemetry::trace::{Link, Status};
use pin_project::pin_project;
use serde::{de::DeserializeOwned, Serialize};
use std::task::Poll;
use std::{collections::HashMap, sync::Arc};
use std::{future::Future, pin::Pin, task};
use tower::{Layer, Service};
use tracing::field::Empty;
use tracing::instrument::Instrumented;
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use crate::TelemetryCompletionHandler;

/// Data extracted from a Lambda event for span creation.
///
/// This struct contains all the information needed to create and configure an OpenTelemetry span,
/// including custom attributes, span kind, links, and context propagation headers.
///
/// # Example
///
/// ```no_run
/// use lambda_otel_lite::SpanAttributes;
/// use std::collections::HashMap;
///
/// let mut attributes = HashMap::new();
/// attributes.insert("custom.field".to_string(), "value".to_string());
///
/// let span_attrs = SpanAttributes {
///     kind: Some("CLIENT".to_string()),
///     attributes,
///     ..SpanAttributes::default()
/// };
/// ```
#[derive(Default)]
pub struct SpanAttributes {
    /// Optional span kind (defaults to SERVER if not provided)
    pub kind: Option<String>,
    /// Custom attributes to add to the span
    pub attributes: HashMap<String, String>,
    /// Optional span links for connecting related traces
    pub links: Vec<Link>,
    /// Optional carrier headers for context propagation (W3C Trace Context format)
    pub carrier: Option<HashMap<String, String>>,
}

/// Function type for extracting span attributes from a Lambda event.
///
/// This type represents a closure that can extract span attributes from a Lambda event.
/// It's used by the `OtelTracingLayer` when custom attribute extraction is needed
/// beyond what's provided by the `SpanAttributesExtractor` trait.
///
/// The function takes a reference to a `LambdaEvent<T>` and returns a `SpanAttributes`
/// struct containing the extracted information.
///
/// See [`OtelTracingLayer::with_extractor_fn`] for usage examples.
pub type EventExtractor<T> = Arc<dyn Fn(&LambdaEvent<T>) -> SpanAttributes + Send + Sync>;

/// Trait for types that can provide span attributes.
///
/// Implement this trait for your event types to enable automatic attribute extraction.
/// The layer will automatically detect and use this implementation when processing events.
///
/// # Implementation Guidelines
///
/// When implementing this trait:
/// 1. Extract relevant attributes that describe the event
/// 2. Set appropriate span kind if different from SERVER
/// 3. Include any headers needed for context propagation
/// 4. Add span links if the event is related to other traces
///
/// # Example
///
/// ```no_run
/// use lambda_otel_lite::{SpanAttributes, SpanAttributesExtractor};
/// use std::collections::HashMap;
///
/// struct CustomEvent {
///     operation: String,
///     trace_parent: Option<String>,
/// }
///
/// impl SpanAttributesExtractor for CustomEvent {
///     fn extract_span_attributes(&self) -> SpanAttributes {
///         let mut attributes = HashMap::new();
///         attributes.insert("operation".to_string(), self.operation.clone());
///
///         // Add trace context if available
///         let carrier = self.trace_parent.as_ref().map(|header| {
///             let mut headers = HashMap::new();
///             headers.insert("traceparent".to_string(), header.clone());
///             headers
///         });
///
///         SpanAttributes {
///             attributes,
///             carrier,
///             ..SpanAttributes::default()
///         }
///     }
/// }
/// ```
pub trait SpanAttributesExtractor {
    /// Extract span attributes from this type.
    ///
    /// This method should extract any relevant information from the implementing type
    /// that should be included in the OpenTelemetry span. This includes:
    /// - Custom attributes describing the event
    /// - Span kind if different from SERVER
    /// - Headers for context propagation
    /// - Links to related traces
    fn extract_span_attributes(&self) -> SpanAttributes;
}

// Implementation for API Gateway V2 events
impl SpanAttributesExtractor for ApiGatewayV2httpRequest {
    fn extract_span_attributes(&self) -> SpanAttributes {
        let mut attributes = HashMap::new();

        // Add HTTP attributes
        attributes.insert(
            "http.method".to_string(),
            self.request_context.http.method.to_string(),
        );
        if let Some(path) = &self.request_context.http.path {
            attributes.insert("http.target".to_string(), path.to_string());
        }
        if let Some(protocol) = &self.request_context.http.protocol {
            attributes.insert("http.scheme".to_string(), protocol.to_lowercase());
        }

        // Add route
        if let Some(route) = &self.route_key {
            attributes.insert("http.route".to_string(), route.to_string());
        }

        // Extract headers for context propagation
        let carrier = self
            .headers
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
            .collect();

        SpanAttributes {
            attributes,
            carrier: Some(carrier),
            ..SpanAttributes::default()
        }
    }
}

// Implementation for API Gateway V1 events
impl SpanAttributesExtractor for ApiGatewayProxyRequest {
    fn extract_span_attributes(&self) -> SpanAttributes {
        let mut attributes = HashMap::new();

        // Add HTTP attributes
        attributes.insert("http.method".to_string(), self.http_method.to_string());
        if let Some(path) = &self.path {
            attributes.insert("http.target".to_string(), path.to_string());
        }
        if let Some(protocol) = &self.request_context.protocol {
            attributes.insert("http.scheme".to_string(), protocol.to_lowercase());
        }

        // Add route
        if let Some(resource) = &self.resource {
            attributes.insert("http.route".to_string(), resource.to_string());
        }

        // Extract headers for context propagation
        let carrier = self
            .headers
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
            .collect();

        SpanAttributes {
            attributes,
            carrier: Some(carrier),
            ..SpanAttributes::default()
        }
    }
}

// Implementation for ALB Target Group events
impl SpanAttributesExtractor for AlbTargetGroupRequest {
    fn extract_span_attributes(&self) -> SpanAttributes {
        let mut attributes = HashMap::new();

        // Add HTTP attributes
        attributes.insert("http.method".to_string(), self.http_method.to_string());
        if let Some(path) = &self.path {
            attributes.insert("http.target".to_string(), path.to_string());
        }

        // Add ALB specific attributes
        if let Some(target_group_arn) = &self.request_context.elb.target_group_arn {
            attributes.insert(
                "alb.target_group_arn".to_string(),
                target_group_arn.to_string(),
            );
        }

        // Extract headers for context propagation
        let carrier = self
            .headers
            .iter()
            .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
            .collect();

        SpanAttributes {
            attributes,
            carrier: Some(carrier),
            ..SpanAttributes::default()
        }
    }
}

/// Future that calls complete() on the completion handler when the inner future completes.
///
/// This future wraps the inner service future to ensure that spans are properly completed
/// and exported. It:
/// 1. Polls the inner future to completion
/// 2. Extracts response attributes (e.g., status code)
/// 3. Sets span status based on response
/// 4. Signals completion through the completion handler
///
/// This type is created automatically by `OtelTracingService` - you shouldn't need to
/// construct it directly.
#[pin_project]
pub struct CompletionFuture<Fut> {
    #[pin]
    future: Option<Fut>,
    completion_handler: Option<TelemetryCompletionHandler>,
    span: Option<tracing::Span>,
}

impl<Fut, R> Future for CompletionFuture<Fut>
where
    Fut: Future<Output = Result<R, Error>>,
    R: Serialize + Send + 'static,
{
    type Output = Result<R, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let ready = ready!(self
            .as_mut()
            .project()
            .future
            .as_pin_mut()
            .expect("future polled after completion")
            .poll(cx));

        // Extract response attributes if it's a successful response
        if let Ok(response) = &ready {
            // Try to convert response to Value to extract attributes
            if let Ok(value) = serde_json::to_value(response) {
                if let Some(span) = self.span.as_ref() {
                    // Extract status code and set span status
                    if let Some(status_code) = value.get("statusCode").and_then(|s| s.as_i64()) {
                        span.set_attribute("http.status_code", status_code);
                        if status_code >= 500 {
                            span.set_status(Status::error(format!(
                                "HTTP {} response",
                                status_code
                            )));
                        } else {
                            span.set_status(Status::Ok);
                        }
                    }
                }
            }
        }

        // Drop the future and span before calling complete
        Pin::set(&mut self.as_mut().project().future, None);
        let this = self.project();
        this.span.take(); // Take ownership and drop the span

        // Now that the span is closed, complete telemetry
        if let Some(handler) = this.completion_handler.take() {
            handler.complete();
        }

        Poll::Ready(ready)
    }
}

/// Tower middleware to create an OpenTelemetry tracing span for Lambda invocations.
///
/// This layer wraps a Lambda service to automatically create and configure OpenTelemetry
/// spans for each invocation. It supports:
/// - Automatic span creation with configurable names
/// - Automatic attribute extraction from supported event types
/// - Custom attribute extraction through closures
/// - Context propagation from HTTP headers
/// - Response status tracking
///
/// # Example
///
/// ```no_run
/// use lambda_otel_lite::{init_telemetry, OtelTracingLayer, TelemetryConfig, SpanAttributes};
/// use lambda_runtime::{service_fn, Error, LambdaEvent, Runtime};
/// use aws_lambda_events::event::apigw::ApiGatewayV2httpRequest;
/// use tower::ServiceBuilder;
///
/// async fn handler(event: LambdaEvent<ApiGatewayV2httpRequest>) -> Result<serde_json::Value, Error> {
///     Ok(serde_json::json!({ "statusCode": 200 }))
/// }
///
/// # async fn example() -> Result<(), Error> {
/// let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
///
/// // Create a layer with custom name and attribute extraction
/// let layer = OtelTracingLayer::new(completion_handler)
///     .with_name("api-handler")
///     .with_extractor_fn(|event| {
///         let mut attributes = std::collections::HashMap::new();
///         attributes.insert("custom.field".to_string(), "value".to_string());
///         SpanAttributes {
///             attributes,
///             ..SpanAttributes::default()
///         }
///     });
///
/// // Apply the layer to your handler
/// let service = ServiceBuilder::new()
///     .layer(layer)
///     .service_fn(handler);
///
/// Runtime::new(service).run().await
/// # }
/// ```
#[derive(Clone)]
pub struct OtelTracingLayer<T> {
    completion_handler: TelemetryCompletionHandler,
    name: String,
    event_extractor: Option<EventExtractor<T>>,
}

impl<T> OtelTracingLayer<T> {
    /// Create a new OpenTelemetry tracing layer with the required completion handler.
    ///
    /// The completion handler is used to signal when spans should be exported. It's typically
    /// obtained from [`init_telemetry`](crate::init_telemetry).
    ///
    /// # Arguments
    ///
    /// * `completion_handler` - Handler for managing span export timing
    pub fn new(completion_handler: TelemetryCompletionHandler) -> Self {
        Self {
            completion_handler,
            name: "lambda-invocation".to_string(),
            event_extractor: None,
        }
    }

    /// Set the span name.
    ///
    /// This name will be used for all spans created by this layer. It should describe
    /// the purpose of the Lambda function (e.g., "process-order", "api-handler").
    ///
    /// # Arguments
    ///
    /// * `name` - The name to use for spans
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the event extractor function from a closure.
    ///
    /// This function will be called for each invocation to extract custom attributes
    /// for the span. It's useful when you need to:
    /// - Extract attributes from unsupported event types
    /// - Add custom attributes beyond what's provided by `SpanAttributesExtractor`
    /// - Override the default attribute extraction
    ///
    /// # Arguments
    ///
    /// * `f` - Closure that takes a reference to the Lambda event and returns span attributes
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use lambda_otel_lite::{OtelTracingLayer, SpanAttributes, TelemetryCompletionHandler};
    /// # use lambda_runtime::LambdaEvent;
    /// # use serde_json::Value;
    /// # let completion_handler: TelemetryCompletionHandler = unimplemented!();
    /// let layer = OtelTracingLayer::new(completion_handler)
    ///     .with_name("my-handler")
    ///     .with_extractor_fn(|event: &LambdaEvent<Value>| {
    ///         let mut attributes = std::collections::HashMap::new();
    ///         
    ///         // Extract custom fields from the event
    ///         if let Ok(payload) = serde_json::to_value(&event.payload) {
    ///             if let Some(user_id) = payload.get("userId").and_then(|v| v.as_str()) {
    ///                 attributes.insert("user.id".to_string(), user_id.to_string());
    ///             }
    ///         }
    ///         
    ///         SpanAttributes {
    ///             attributes,
    ///             ..SpanAttributes::default()
    ///         }
    ///     });
    /// ```
    pub fn with_extractor_fn(
        mut self,
        f: impl Fn(&LambdaEvent<T>) -> SpanAttributes + Send + Sync + 'static,
    ) -> Self {
        self.event_extractor = Some(Arc::new(f));
        self
    }
}

impl<S, T> Layer<S> for OtelTracingLayer<T>
where
    T: Clone,
{
    type Service = OtelTracingService<S, T>;

    fn layer(&self, inner: S) -> Self::Service {
        OtelTracingService {
            inner,
            completion_handler: self.completion_handler.clone(),
            name: self.name.clone(),
            event_extractor: self.event_extractor.clone(),
        }
    }
}

/// Tower service returned by [OtelTracingLayer].
///
/// This service wraps the inner Lambda service to:
/// 1. Create a span for each invocation
/// 2. Extract and set span attributes
/// 3. Propagate context from headers
/// 4. Track response status
/// 5. Signal completion for span export
///
/// The service is created automatically by the layer - you shouldn't need to
/// construct it directly.
#[derive(Clone)]
pub struct OtelTracingService<S, T> {
    inner: S,
    completion_handler: TelemetryCompletionHandler,
    name: String,
    event_extractor: Option<EventExtractor<T>>,
}

impl<S, F, T, R> Service<LambdaEvent<T>> for OtelTracingService<S, T>
where
    S: Service<LambdaEvent<T>, Response = R, Error = Error, Future = F> + Send,
    F: Future<Output = Result<R, Error>> + Send + 'static,
    T: DeserializeOwned + Serialize + Send + 'static,
    R: Serialize + Send + 'static,
{
    type Response = R;
    type Error = Error;
    type Future = CompletionFuture<Instrumented<S::Future>>;

    fn poll_ready(&mut self, cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, event: LambdaEvent<T>) -> Self::Future {
        let span = tracing::info_span!(
            parent: None,
            "handler",
            otel.name=Empty,
            otel.kind=Empty,
            requestId=%event.context.request_id,
            faas.invocation_id=%event.context.request_id,
            cloud.resource_id=%event.context.invoked_function_arn,
        );

        // Set the span name
        span.set_attribute("otel.name", self.name.clone());

        // Add cloud account ID if available
        if let Some(account_id) = event.context.invoked_function_arn.split(':').nth(4) {
            span.set_attribute("cloud.account.id", account_id.to_string());
        }

        // Default span kind
        span.set_attribute("otel.kind", "SERVER");

        // Try to extract HTTP attributes if available
        if serde_json::to_value(&event.payload).is_ok() {
            span.set_attribute("faas.trigger", "http");
        } else {
            span.set_attribute("faas.trigger", "other");
        }

        // Extract attributes if type implements SpanAttributesExtractor
        if let Some(extractor) =
            (&event.payload as &dyn std::any::Any).downcast_ref::<&dyn SpanAttributesExtractor>()
        {
            let attrs = extractor.extract_span_attributes();

            // Apply extracted attributes
            if let Some(kind) = attrs.kind {
                span.set_attribute("otel.kind", kind);
            }

            for (key, value) in attrs.attributes {
                span.set_attribute(key, value);
            }

            for link in attrs.links {
                span.add_link_with_attributes(link.span_context, link.attributes);
            }

            if let Some(carrier) = attrs.carrier {
                let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
                    propagator.extract(&carrier)
                });
                span.set_parent(parent_context);
            }
        }

        // Apply custom attributes if extractor is provided
        if let Some(extractor) = &self.event_extractor {
            let attrs = extractor(&event);

            // Override span kind if provided
            if let Some(kind) = attrs.kind {
                span.set_attribute("otel.kind", kind);
            }

            // Add custom attributes
            for (key, value) in attrs.attributes {
                span.set_attribute(key, value);
            }

            // Add links
            for link in attrs.links {
                span.add_link_with_attributes(link.span_context, link.attributes);
            }

            // Override parent context if carrier provided
            if let Some(carrier) = attrs.carrier {
                let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
                    propagator.extract(&carrier)
                });
                span.set_parent(parent_context);
            }
        }

        let future = {
            let _guard = span.enter();
            self.inner.call(event)
        };

        CompletionFuture {
            future: Some(future.instrument(span.clone())),
            completion_handler: Some(self.completion_handler.clone()),
            span: Some(span),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProcessorMode;
    use aws_lambda_events::event::apigw::ApiGatewayV2httpRequest;
    use futures_util::future::BoxFuture;
    use lambda_runtime::Context;
    use opentelemetry::trace::TraceResult;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_sdk::{
        export::trace::{SpanData, SpanExporter},
        trace::TracerProvider,
        Resource,
    };
    use serial_test::serial;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tower::ServiceExt;
    use tracing_subscriber::prelude::*;

    // Mock exporter that counts exports
    #[derive(Debug)]
    struct CountingExporter {
        export_count: Arc<AtomicUsize>,
    }

    impl CountingExporter {
        fn new() -> Self {
            Self {
                export_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl SpanExporter for CountingExporter {
        fn export(&mut self, batch: Vec<SpanData>) -> BoxFuture<'static, TraceResult<()>> {
            self.export_count.fetch_add(batch.len(), Ordering::SeqCst);
            Box::pin(futures_util::future::ready(Ok(())))
        }

        fn shutdown(&mut self) {}
    }

    #[tokio::test]
    #[serial]
    async fn test_basic_layer() -> Result<(), Error> {
        let exporter = CountingExporter::new();
        let export_count = exporter.export_count.clone();

        let provider = TracerProvider::builder()
            .with_simple_exporter(exporter)
            .with_resource(Resource::empty())
            .build();
        let provider = Arc::new(provider);

        // Set up tracing subscriber
        let _subscriber = tracing_subscriber::registry::Registry::default()
            .with(tracing_opentelemetry::OpenTelemetryLayer::new(
                provider.tracer("test"),
            ))
            .set_default();

        let completion_handler =
            TelemetryCompletionHandler::new(provider.clone(), None, ProcessorMode::Sync);

        let handler = |_: LambdaEvent<serde_json::Value>| async {
            // Create a span to ensure it's captured
            let _span = tracing::info_span!("test_span");
            Ok::<_, Error>(serde_json::json!({"status": "ok"}))
        };

        let layer = OtelTracingLayer::new(completion_handler).with_name("test-handler");

        let mut svc = tower::ServiceBuilder::new()
            .layer(layer)
            .service_fn(handler);

        let event = LambdaEvent::new(serde_json::json!({}), Context::default());

        let _ = svc.ready().await?.call(event).await?;

        // Wait a bit longer for spans to be exported
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert!(export_count.load(Ordering::SeqCst) > 0);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_automatic_attribute_extraction() -> Result<(), Error> {
        let exporter = CountingExporter::new();

        let provider = TracerProvider::builder()
            .with_simple_exporter(exporter)
            .with_resource(Resource::empty())
            .build();
        let provider = Arc::new(provider);

        let completion_handler =
            TelemetryCompletionHandler::new(provider.clone(), None, ProcessorMode::Sync);

        let handler = |_: LambdaEvent<ApiGatewayV2httpRequest>| async {
            Ok::<_, Error>(serde_json::json!({"status": "ok"}))
        };

        let layer = OtelTracingLayer::new(completion_handler).with_name("test-handler");

        let mut svc = tower::ServiceBuilder::new()
            .layer(layer)
            .service_fn(handler);

        let request = ApiGatewayV2httpRequest {
            raw_path: Some("/test".to_string()),
            request_context: aws_lambda_events::apigw::ApiGatewayV2httpRequestContext::default(),
            ..Default::default()
        };

        let event = LambdaEvent::new(request, Context::default());

        let _ = svc.ready().await?.call(event).await?;

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_custom_attribute_extraction() -> Result<(), Error> {
        let exporter = CountingExporter::new();

        let provider = TracerProvider::builder()
            .with_simple_exporter(exporter)
            .with_resource(Resource::empty())
            .build();
        let provider = Arc::new(provider);

        let completion_handler =
            TelemetryCompletionHandler::new(provider.clone(), None, ProcessorMode::Sync);

        let handler = |_: LambdaEvent<serde_json::Value>| async {
            Ok::<_, Error>(serde_json::json!({"status": "ok"}))
        };

        let layer = OtelTracingLayer::new(completion_handler)
            .with_name("test-handler")
            .with_extractor_fn(|_event: &LambdaEvent<serde_json::Value>| {
                let mut attributes = HashMap::new();
                attributes.insert("custom.attribute".to_string(), "test-value".to_string());
                SpanAttributes {
                    attributes,
                    ..SpanAttributes::default()
                }
            });

        let mut svc = tower::ServiceBuilder::new()
            .layer(layer)
            .service_fn(handler);

        let event = LambdaEvent::new(serde_json::json!({}), Context::default());

        let _ = svc.ready().await?.call(event).await?;

        Ok(())
    }
}
