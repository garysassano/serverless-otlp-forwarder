//! Lambda function handler wrapper with OpenTelemetry tracing.
//!
//! This module provides a wrapper function that automatically creates OpenTelemetry spans
//! for Lambda function invocations. It supports:
//! - Automatic span creation with configurable names and attributes
//! - Built-in support for common AWS event types (API Gateway v1/v2)
//! - Automatic context propagation from HTTP headers
//! - Response status code tracking
//! - Custom attribute extraction
//!
//! # Architecture
//!
//! The handler wrapper operates by:
//! 1. Creating a span for each invocation
//! 2. Extracting attributes from the event
//! 3. Running the handler function within the span
//! 4. Capturing response attributes (e.g., status code)
//! 5. Signaling completion for span export
//!
//! # Example
//!
//! ```no_run
//! use lambda_otel_lite::{init_telemetry, traced_handler, TracedHandlerOptions, TelemetryConfig};
//! use lambda_runtime::{service_fn, Error, LambdaEvent};
//! use serde_json::Value;
//!
//! async fn function_handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
//!     Ok(serde_json::json!({ "statusCode": 200 }))
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
//!     
//!     let func = service_fn(|event| {
//!         traced_handler(
//!             TracedHandlerOptions::default()
//!                 .with_name("my-handler")
//!                 .with_event(event),
//!             completion_handler.clone(),
//!             function_handler,
//!         )
//!     });
//!
//!     lambda_runtime::run(func).await
//! }
//! ```

use crate::TelemetryCompletionHandler;
use lambda_runtime::{Error, LambdaEvent};
use opentelemetry::{global, trace::Link, trace::Status, Context as OtelContext};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{collections::HashMap, future::Future, sync::Arc};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Function type for extracting carrier headers from an event.
///
/// This type represents a function that can extract context propagation headers
/// from a Lambda event. It's used to support distributed tracing across service boundaries.
///
/// The function takes a JSON Value representing the event and returns a map of header
/// names to values that can be used for context propagation.
pub type CarrierExtractor = Arc<dyn Fn(&Value) -> HashMap<String, String> + Send + Sync>;

/// Options for configuring traced Lambda handlers.
///
/// This struct provides a builder-style interface for configuring how spans are created
/// and attributed for Lambda function invocations. All fields are optional and have
/// sensible defaults.
///
/// # Example
///
/// ```no_run
/// use lambda_otel_lite::TracedHandlerOptions;
/// use lambda_runtime::LambdaEvent;
/// use serde_json::Value;
///
/// let event = LambdaEvent::new(Value::Null, Default::default());
/// let options = TracedHandlerOptions::default()
///     .with_name("my-handler")
///     .with_event(event)
///     .with_kind("SERVER".to_string());
/// ```
#[derive(Clone)]
pub struct TracedHandlerOptions<T> {
    /// Name of the span
    pub name: Option<&'static str>,
    /// Lambda event containing both payload and context
    pub event: Option<LambdaEvent<T>>,
    /// Optional span kind. Defaults to SERVER
    pub kind: Option<String>,
    /// Optional custom attributes to add to the span
    pub attributes: Option<HashMap<String, String>>,
    /// Optional span links
    pub links: Option<Vec<Link>>,
    /// Optional parent context for trace propagation
    pub parent_context: Option<OtelContext>,
    /// Optional function to extract carrier from event for context propagation
    pub get_carrier: Option<CarrierExtractor>,
}

impl<T> Default for TracedHandlerOptions<T> {
    fn default() -> Self {
        Self {
            name: None,
            event: None,
            kind: None,
            attributes: None,
            links: None,
            parent_context: None,
            get_carrier: None,
        }
    }
}

impl<T> TracedHandlerOptions<T> {
    /// Create new options with required fields
    pub fn new(name: &'static str, event: LambdaEvent<T>) -> Self {
        Self {
            name: Some(name),
            event: Some(event),
            kind: None,
            attributes: None,
            links: None,
            parent_context: None,
            get_carrier: None,
        }
    }

    /// Set the span name
    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    /// Set the event
    pub fn with_event(mut self, event: LambdaEvent<T>) -> Self {
        self.event = Some(event);
        self
    }

    /// Set the span kind
    pub fn with_kind(mut self, kind: String) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Set custom attributes
    pub fn with_attributes(mut self, attributes: HashMap<String, String>) -> Self {
        self.attributes = Some(attributes);
        self
    }

    /// Set span links
    pub fn with_links(mut self, links: Vec<Link>) -> Self {
        self.links = Some(links);
        self
    }

    /// Set parent context
    pub fn with_parent_context(mut self, context: OtelContext) -> Self {
        self.parent_context = Some(context);
        self
    }

    /// Set carrier extractor
    pub fn with_carrier_extractor(mut self, extractor: CarrierExtractor) -> Self {
        self.get_carrier = Some(extractor);
        self
    }
}

/// Extract headers from an event, either using a custom carrier extractor or from the headers field.
///
/// This function attempts to extract context propagation headers in two ways:
/// 1. Using a custom carrier extractor if provided
/// 2. Looking for a 'headers' field in the event JSON if no extractor is provided
///
/// The function returns None if:
/// - No headers were found
/// - The headers were empty
/// - The extraction failed
pub(crate) fn extract_headers(
    event: &Value,
    get_carrier: Option<&CarrierExtractor>,
) -> Option<HashMap<String, String>> {
    // Try custom carrier extractor first
    if let Some(extractor) = get_carrier {
        let carrier = extractor(event);
        if !carrier.is_empty() {
            return Some(carrier);
        }
    }

    // Fall back to headers field
    event
        .get("headers")
        .and_then(|headers| headers.as_object())
        .map(|headers| {
            headers
                .iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                .collect()
        })
        .filter(|headers: &HashMap<String, String>| !headers.is_empty())
}

/// Extract HTTP attributes from API Gateway v2 events.
///
/// This function extracts standard HTTP attributes from API Gateway v2 events:
/// - http.method: The HTTP method used
/// - http.target: The request path
/// - http.route: The route key
/// - http.scheme: The protocol used
fn extract_apigw_v2_attributes(span: &tracing::Span, payload: &Value) {
    if let Some(route_key) = payload.get("routeKey").and_then(|v| v.as_str()) {
        span.set_attribute("http.route", route_key.to_string());
    }

    if let Some(http) = payload.get("requestContext").and_then(|v| v.get("http")) {
        if let Some(method) = http.get("method").and_then(|v| v.as_str()) {
            span.set_attribute("http.method", method.to_string());
        }
        if let Some(path) = http.get("path").and_then(|v| v.as_str()) {
            span.set_attribute("http.target", path.to_string());
        }
        if let Some(protocol) = http.get("protocol").and_then(|v| v.as_str()) {
            span.set_attribute("http.scheme", protocol.to_lowercase());
        }
    }
}

/// Extract HTTP attributes from API Gateway v1 events.
///
/// This function extracts standard HTTP attributes from API Gateway v1 events:
/// - http.method: The HTTP method used
/// - http.target: The request path
/// - http.route: The resource path
/// - http.scheme: The protocol used
fn extract_apigw_v1_attributes(span: &tracing::Span, payload: &Value) {
    if let Some(resource) = payload.get("resource").and_then(|v| v.as_str()) {
        span.set_attribute("http.route", resource.to_string());
    }
    if let Some(method) = payload.get("httpMethod").and_then(|v| v.as_str()) {
        span.set_attribute("http.method", method.to_string());
    }
    if let Some(path) = payload.get("path").and_then(|v| v.as_str()) {
        span.set_attribute("http.target", path.to_string());
    }
    if let Some(protocol) = payload
        .get("requestContext")
        .and_then(|ctx| ctx.get("protocol"))
        .and_then(|v| v.as_str())
    {
        span.set_attribute("http.scheme", protocol.to_lowercase());
    }
}

/// Wraps a Lambda handler function with OpenTelemetry tracing.
///
/// This function wraps a Lambda handler function to automatically:
/// - Create spans for each invocation
/// - Extract and set span attributes from the event
/// - Propagate context from incoming requests
/// - Track response status codes
/// - Signal completion for span export
///
/// The wrapper supports both synchronous and asynchronous span processing modes
/// through the `completion_handler`.
///
/// # Type Parameters
///
/// * `T` - The event payload type that must be deserializable and serializable
/// * `R` - The response type that must be serializable
/// * `F` - The handler function type
/// * `Fut` - The future returned by the handler function
///
/// # Arguments
///
/// * `options` - Configuration options for the traced handler
/// * `completion_handler` - Handler for managing span export
/// * `handler_fn` - The actual Lambda handler function to wrap
///
/// # Returns
///
/// Returns the result from the handler function
///
/// # Example
///
/// ```no_run
/// use lambda_otel_lite::{init_telemetry, traced_handler, TracedHandlerOptions, TelemetryConfig};
/// use lambda_runtime::{service_fn, Error, LambdaEvent};
/// use serde_json::Value;
///
/// async fn function_handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
///     Ok(serde_json::json!({ "statusCode": 200 }))
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Error> {
///     let completion_handler = init_telemetry(TelemetryConfig::default()).await?;
///     
///     let func = service_fn(|event| {
///         traced_handler(
///             TracedHandlerOptions::default()
///                 .with_name("my-handler")
///                 .with_event(event),
///             completion_handler.clone(),
///             function_handler,
///         )
///     });
///
///     lambda_runtime::run(func).await
/// }
/// ```
pub async fn traced_handler<T, R, F, Fut>(
    options: TracedHandlerOptions<T>,
    completion_handler: TelemetryCompletionHandler,
    handler_fn: F,
) -> Result<R, Error>
where
    T: DeserializeOwned + Serialize + Send + 'static,
    R: Serialize + Send + 'static,
    F: FnOnce(LambdaEvent<T>) -> Fut,
    Fut: Future<Output = Result<R, Error>> + Send,
{
    let result = {
        let span = tracing::info_span!(
            parent: None,
            "handler",
            otel.kind=options.kind.unwrap_or("SERVER".to_string()),
            otel.name=options.name.unwrap_or("lambda-invocation"),
        );

        // Extract span attributes and get event for handler
        let event = if let Some(event) = options.event {
            span.set_attribute("faas.invocation_id", event.context.request_id.clone());
            span.set_attribute(
                "cloud.resource_id",
                event.context.invoked_function_arn.clone(),
            );

            // Extract account ID from ARN
            if let Some(account_id) = event.context.invoked_function_arn.split(':').nth(4) {
                span.set_attribute("cloud.account.id", account_id.to_string());
            }

            // Try to extract HTTP attributes if available
            if let Ok(payload) = serde_json::to_value(&event.payload) {
                // Try to extract context from headers first
                if let Some(headers) = extract_headers(&payload, options.get_carrier.as_ref()) {
                    let ctx =
                        global::get_text_map_propagator(|propagator| propagator.extract(&headers));
                    span.set_parent(ctx);
                }

                // Then handle HTTP-specific attributes
                if payload.get("requestContext").is_some() || payload.get("httpMethod").is_some() {
                    span.set_attribute("faas.trigger", "http");

                    if payload.get("version").and_then(|v| v.as_str()) == Some("2.0") {
                        extract_apigw_v2_attributes(&span, &payload);
                    } else {
                        extract_apigw_v1_attributes(&span, &payload);
                    }
                } else {
                    span.set_attribute("faas.trigger", "other");
                }
            }
            event
        } else {
            return Err(Error::from("No event provided"));
        };

        // Add links if provided
        if let Some(links) = options.links {
            for link in links {
                span.add_link_with_attributes(link.span_context, link.attributes);
            }
        }

        // Add custom attributes if provided
        if let Some(attributes) = options.attributes {
            for (key, value) in attributes {
                span.set_attribute(key, value);
            }
        }

        // Create a tracing span that will be linked to the OTel span
        let result = handler_fn(event).instrument(span.clone()).await;

        // Handle HTTP response status if available
        if let Ok(response) = &result {
            if let Ok(value) = serde_json::to_value(response) {
                if let Some(status_code) = get_status_code(&value) {
                    span.set_attribute("http.status_code", status_code.to_string());
                    if status_code >= 500 {
                        span.set_status(Status::error(format!("HTTP {} response", status_code)));
                    } else {
                        span.set_status(Status::Ok);
                    }
                }
            }
        }
        result
    };
    // Signal completion
    completion_handler.complete();
    result
}

/// Extract status code from response if it's an HTTP response
pub(crate) fn get_status_code(response: &Value) -> Option<i64> {
    response
        .as_object()
        .and_then(|obj| obj.get("statusCode"))
        .and_then(|v| v.as_i64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processor::ProcessorMode;
    use futures_util::future::BoxFuture;
    use lambda_runtime::Context;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry::trace::{Status, TraceContextExt, TraceResult};
    use opentelemetry_sdk::{
        export::trace::{SpanData, SpanExporter},
        trace::TracerProvider,
        Resource,
    };
    use serial_test::serial;
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    use std::time::Duration;
    use tracing_subscriber::prelude::*;

    // Test exporter that captures spans and their attributes
    #[derive(Debug, Default, Clone)]
    struct TestExporter {
        export_count: Arc<AtomicUsize>,
        spans: Arc<Mutex<Vec<SpanData>>>,
    }

    impl TestExporter {
        fn new() -> Self {
            Self {
                export_count: Arc::new(AtomicUsize::new(0)),
                spans: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_spans(&self) -> Vec<SpanData> {
            self.spans.lock().unwrap().clone()
        }

        fn find_attribute(span: &SpanData, key: &str) -> Option<String> {
            span.attributes
                .iter()
                .find(|kv| kv.key.as_str() == key)
                .map(|kv| kv.value.to_string())
        }
    }

    impl SpanExporter for TestExporter {
        fn export(&mut self, spans: Vec<SpanData>) -> BoxFuture<'static, TraceResult<()>> {
            self.export_count.fetch_add(spans.len(), Ordering::SeqCst);
            self.spans.lock().unwrap().extend(spans);
            Box::pin(futures_util::future::ready(Ok(())))
        }
    }

    fn setup_test_provider() -> (
        Arc<TracerProvider>,
        Arc<TestExporter>,
        tracing::dispatcher::DefaultGuard,
    ) {
        let exporter = Arc::new(TestExporter::new());
        let provider = TracerProvider::builder()
            .with_simple_exporter(exporter.as_ref().clone())
            .with_resource(Resource::empty())
            .build();
        let subscriber = tracing_subscriber::registry::Registry::default()
            .with(tracing_opentelemetry::OpenTelemetryLayer::new(
                provider.tracer("test"),
            ))
            .set_default();
        (Arc::new(provider), exporter, subscriber)
    }

    #[tokio::test]
    #[serial]
    async fn test_basic_handler_wrapping() -> Result<(), Error> {
        let (provider, exporter, _subscriber_guard) = setup_test_provider();

        let completion_handler =
            TelemetryCompletionHandler::new(provider.clone(), None, ProcessorMode::Sync);

        let handler_fn =
            |_event: LambdaEvent<Value>| async move { Ok(serde_json::json!({"statusCode": 200})) };

        let event = LambdaEvent::new(serde_json::json!({}), Context::default());

        let options = TracedHandlerOptions::default()
            .with_name("test-handler")
            .with_event(event);

        let result = traced_handler(options, completion_handler, handler_fn).await?;

        // Wait a bit longer for spans to be exported
        tokio::time::sleep(Duration::from_millis(500)).await;

        let spans = exporter.get_spans();
        assert!(!spans.is_empty());
        assert_eq!(result["statusCode"], 200);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_apigw_v2_attributes() -> Result<(), Error> {
        let (provider, exporter, _subscriber_guard) = setup_test_provider();

        let completion_handler =
            TelemetryCompletionHandler::new(provider.clone(), None, ProcessorMode::Sync);

        let event = LambdaEvent::new(
            serde_json::json!({
                "version": "2.0",
                "routeKey": "GET /items",
                "requestContext": {
                    "http": {
                        "method": "GET",
                        "path": "/items",
                        "protocol": "HTTP/1.1"
                    }
                }
            }),
            Context::default(),
        );

        let options = TracedHandlerOptions::default()
            .with_name("test-handler")
            .with_event(event);

        let handler_fn =
            |_event: LambdaEvent<Value>| async move { Ok(serde_json::json!({"statusCode": 200})) };

        traced_handler(options, completion_handler, handler_fn).await?;

        // Wait a bit for spans to be exported
        tokio::time::sleep(Duration::from_millis(100)).await;

        let spans = exporter.get_spans();
        assert!(!spans.is_empty());
        let span = &spans[0];

        assert_eq!(
            TestExporter::find_attribute(span, "http.method"),
            Some("GET".to_string())
        );
        assert_eq!(
            TestExporter::find_attribute(span, "http.target"),
            Some("/items".to_string())
        );
        assert_eq!(
            TestExporter::find_attribute(span, "http.route"),
            Some("GET /items".to_string())
        );
        assert_eq!(
            TestExporter::find_attribute(span, "http.scheme"),
            Some("http/1.1".to_string())
        );
        assert_eq!(
            TestExporter::find_attribute(span, "faas.trigger"),
            Some("http".to_string())
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_apigw_v1_attributes() -> Result<(), Error> {
        let (provider, exporter, _subscriber_guard) = setup_test_provider();

        let completion_handler =
            TelemetryCompletionHandler::new(provider.clone(), None, ProcessorMode::Sync);

        let event = LambdaEvent::new(
            serde_json::json!({
                "resource": "/items",
                "httpMethod": "GET",
                "path": "/items",
                "requestContext": {
                    "protocol": "HTTP/1.1"
                }
            }),
            Context::default(),
        );

        let options = TracedHandlerOptions::default()
            .with_name("test-handler")
            .with_event(event);

        let handler_fn =
            |_event: LambdaEvent<Value>| async move { Ok(serde_json::json!({"statusCode": 200})) };

        traced_handler(options, completion_handler, handler_fn).await?;

        // Wait a bit for spans to be exported
        tokio::time::sleep(Duration::from_millis(100)).await;

        let spans = exporter.get_spans();
        assert!(!spans.is_empty());
        let span = &spans[0];

        assert_eq!(
            TestExporter::find_attribute(span, "http.method"),
            Some("GET".to_string())
        );
        assert_eq!(
            TestExporter::find_attribute(span, "http.target"),
            Some("/items".to_string())
        );
        assert_eq!(
            TestExporter::find_attribute(span, "http.route"),
            Some("/items".to_string())
        );
        assert_eq!(
            TestExporter::find_attribute(span, "http.scheme"),
            Some("http/1.1".to_string())
        );
        assert_eq!(
            TestExporter::find_attribute(span, "faas.trigger"),
            Some("http".to_string())
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_custom_carrier_extractor() -> Result<(), Error> {
        // Set up the propagator
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );

        let (provider, _, _subscriber_guard) = setup_test_provider();

        let completion_handler =
            TelemetryCompletionHandler::new(provider.clone(), None, ProcessorMode::Sync);

        let event = LambdaEvent::new(
            serde_json::json!({
                "custom_headers": {
                    "traceparent": "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
                }
            }),
            Context::default(),
        );

        let get_carrier: CarrierExtractor = Arc::new(|event| {
            let mut headers = HashMap::new();
            if let Some(custom_headers) = event.get("custom_headers").and_then(|h| h.as_object()) {
                for (k, v) in custom_headers {
                    if let Some(value) = v.as_str() {
                        headers.insert(k.clone(), value.to_string());
                    }
                }
            }
            headers
        });

        let options = TracedHandlerOptions::default()
            .with_name("test-handler")
            .with_event(event)
            .with_carrier_extractor(get_carrier.clone());
        let handler_fn = |_event: LambdaEvent<Value>| async move {
            let span = tracing::Span::current();
            let ctx = span.context();
            let trace_id = ctx.span().span_context().trace_id();
            assert_eq!(trace_id.to_string(), "0af7651916cd43dd8448eb211c80319c");
            Ok(serde_json::json!({"statusCode": 200}))
        };

        traced_handler(options, completion_handler, handler_fn).await?;
        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_custom_attributes() -> Result<(), Error> {
        let (provider, exporter, _subscriber_guard) = setup_test_provider();

        let completion_handler =
            TelemetryCompletionHandler::new(provider.clone(), None, ProcessorMode::Sync);

        let event = LambdaEvent::new(serde_json::json!({}), Context::default());

        let mut attributes = HashMap::new();
        attributes.insert("custom.attr1".to_string(), "value1".to_string());
        attributes.insert("custom.attr2".to_string(), "value2".to_string());

        let options = TracedHandlerOptions::default()
            .with_name("test-handler")
            .with_event(event)
            .with_attributes(attributes);

        let handler_fn =
            |_event: LambdaEvent<Value>| async move { Ok(serde_json::json!({"statusCode": 200})) };

        traced_handler(options, completion_handler, handler_fn).await?;

        // Wait a bit for spans to be exported
        tokio::time::sleep(Duration::from_millis(100)).await;

        let spans = exporter.get_spans();
        assert!(!spans.is_empty());
        let span = &spans[0];

        assert_eq!(
            TestExporter::find_attribute(span, "custom.attr1"),
            Some("value1".to_string())
        );
        assert_eq!(
            TestExporter::find_attribute(span, "custom.attr2"),
            Some("value2".to_string())
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_error_response() -> Result<(), Error> {
        let (provider, exporter, _subscriber_guard) = setup_test_provider();

        let completion_handler =
            TelemetryCompletionHandler::new(provider.clone(), None, ProcessorMode::Sync);

        let event = LambdaEvent::new(serde_json::json!({}), Context::default());

        let options = TracedHandlerOptions::default()
            .with_name("test-handler")
            .with_event(event);

        let handler_fn = |_event: LambdaEvent<Value>| async move {
            Ok(serde_json::json!({
                "statusCode": 500,
                "body": "Internal Server Error"
            }))
        };

        let result = traced_handler(options, completion_handler, handler_fn).await?;

        assert_eq!(result["statusCode"], 500);

        // Wait a bit for spans to be exported
        tokio::time::sleep(Duration::from_millis(100)).await;

        let spans = exporter.get_spans();
        assert!(!spans.is_empty());
        let span = &spans[0];

        assert_eq!(
            TestExporter::find_attribute(span, "http.status_code"),
            Some("500".to_string())
        );
        assert!(matches!(span.status, Status::Error { .. }));

        Ok(())
    }
}
