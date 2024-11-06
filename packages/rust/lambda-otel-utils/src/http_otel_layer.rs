//! This module provides an OpenTelemetry layer for HTTP-based AWS Lambda functions.
//!
//! It includes utilities for tracing HTTP requests in Lambda functions triggered by
//! API Gateway, ALB, or similar HTTP-based event sources. The module implements
//! automatic context propagation and span creation for incoming HTTP requests.
//!
//! # Examples
//!
//! Here's a complete example of how to use this module in an AWS Lambda function:
//!
//! ```rust,no_run
//! use lambda_runtime::{service_fn, Error, LambdaEvent};
//! use aws_lambda_events::event::apigw::ApiGatewayProxyRequest;
//! use serde_json::json;
//! use lambda_runtime::tower::ServiceBuilder;
//! use tracing::info;
//! use opentelemetry::global;
//! use opentelemetry_stdout::SpanExporter;
//! use opentelemetry::trace::TracerProvider;
//!
//! use lambda_otel_utils::http_otel_layer::HttpOtelLayer;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Error> {
//!     // Initialize OpenTelemetry
//!     let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
//!         .with_simple_exporter(SpanExporter::default())
//!         .build();
//!
//!     let tracer = tracer_provider.tracer("lambda-http-example");
//!     global::set_tracer_provider(tracer_provider.clone());
//!
//!     // Build the Lambda service with OpenTelemetry layer
//!     let func = ServiceBuilder::new()
//!         .layer(HttpOtelLayer::new(move || {
//!             tracer_provider.force_flush();
//!         }))
//!         .service(service_fn(handler));
//!
//!     lambda_runtime::run(func).await?;
//!     Ok(())
//! }
//!
//! async fn handler(event: LambdaEvent<ApiGatewayProxyRequest>) -> Result<serde_json::Value, Error> {
//!     // Your handler logic here
//!     info!("Received request: {:?}", event.payload.path);
//!
//!     Ok(json!({
//!         "statusCode": 200,
//!         "body": json!({ "message": "Hello from Lambda!" })
//!     }))
//! }
//! ```
//!
//! This example demonstrates:
//! 1. Setting up OpenTelemetry with a stdout exporter
//! 2. Configuring the Lambda runtime with the `HttpOtelLayer`
//! 3. Implementing a simple handler function that will be automatically instrumented

use aws_lambda_events::event::{
    alb::AlbTargetGroupRequest,
    apigw::{ApiGatewayProxyRequest, ApiGatewayV2httpRequest},
};
use http::{HeaderMap, Method};
use lambda_runtime::tower::{Layer, Service};
use lambda_runtime::{Error, LambdaEvent};
use opentelemetry::trace::TraceContextExt;
use opentelemetry::{global, trace::Status};
use opentelemetry_http::HeaderExtractor;
use opentelemetry_semantic_conventions::trace as traceconv;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, ready},
};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use pin_project::pin_project;
use tracing::{instrument::Instrumented, Instrument, Span};

/// A trait to extract HTTP information from Lambda event payloads.
///
/// This trait should be implemented for various Lambda event types that represent
/// HTTP requests, such as API Gateway events or Application Load Balancer events.
///
/// # Examples
///
/// ```rust
/// use http::{HeaderMap, Method};
/// use lambda_otel_utils::http_otel_layer::HttpEvent;
///
/// struct CustomHttpEvent {
///     method: Method,
///     path: String,
///     headers: HeaderMap,
/// }
///
/// impl HttpEvent for CustomHttpEvent {
///     fn method(&self) -> Method {
///         self.method.clone()
///     }
///
///     fn target(&self) -> String {
///         self.path.clone()
///     }
///
///     fn headers(&self) -> &HeaderMap {
///         &self.headers
///     }
///
///     fn route(&self) -> String {
///         self.path.clone()
///     }
/// }
/// ```
pub trait HttpEvent {
    fn method(&self) -> Method;
    fn target(&self) -> String;
    fn headers(&self) -> &HeaderMap;
    fn route(&self) -> String;
}

/// A Tower layer that adds OpenTelemetry tracing to AWS Lambda functions handling HTTP events.
///
/// This layer creates spans for incoming HTTP requests and ensures proper context propagation.
/// It also manages span lifecycle to ensure all telemetry is properly flushed within the same
/// Lambda invocation.
///
/// # Examples
///
/// ```rust,no_run
/// use lambda_runtime::{service_fn, Error, LambdaEvent};
/// use aws_lambda_events::event::apigw::ApiGatewayProxyRequest;
/// use lambda_runtime::tower::ServiceBuilder;
/// use opentelemetry::global;
/// use opentelemetry_sdk::trace::TracerProvider;
/// use lambda_otel_utils::http_otel_layer::HttpOtelLayer;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let tracer_provider = TracerProvider::builder().build();
/// let provider_clone = tracer_provider.clone();
///
/// let func = ServiceBuilder::new()
///     .layer(HttpOtelLayer::new(move || {
///         provider_clone.force_flush();
///     }))
///     .service(service_fn(handler));
/// #     Ok(())
/// # }
/// # async fn handler(_: LambdaEvent<ApiGatewayProxyRequest>) -> Result<serde_json::Value, Error> {
/// #     Ok(serde_json::json!({"statusCode": 200}))
/// # }
/// ```
#[derive(Clone)]
pub struct HttpOtelLayer<F> {
    flush_fn: F,
    coldstart: bool,
}

impl<F> HttpOtelLayer<F>
where
    F: Fn() + Clone,
{
    pub fn new(flush_fn: F) -> Self {
        Self {
            flush_fn,
            coldstart: true,
        }
    }
}

impl<S, F> Layer<S> for HttpOtelLayer<F>
where
    F: Fn() + Clone,
{
    type Service = HttpOtelService<S, F>;

    fn layer(&self, inner: S) -> Self::Service {
        HttpOtelService {
            inner,
            flush_fn: self.flush_fn.clone(),
            coldstart: self.coldstart,
        }
    }
}

/// The service implementation for the HttpOtelLayer.
///
/// This struct wraps the inner service and adds OpenTelemetry instrumentation including:
/// - HTTP request tracing
/// - Context propagation
/// - Cold start tracking
/// - Trace flushing
pub struct HttpOtelService<S, F> {
    inner: S,
    flush_fn: F,
    coldstart: bool,
}

/// A future that manages the lifecycle of OpenTelemetry spans in Lambda functions.
/// 
/// This implementation ensures that spans are properly closed and flushed within the same
/// Lambda invocation they were created in. It follows the AWS Lambda runtime pattern of
/// explicitly dropping spans before flushing to prevent spans from being carried over
/// to subsequent invocations.
#[pin_project]
pub struct HttpOtelFuture<Fut, F> {
    /// The underlying future, wrapped in Option to allow explicit dropping
    #[pin]
    future: Option<Fut>,
    /// Function to flush OpenTelemetry data
    flush_fn: F,
    /// The root span for this invocation, wrapped in Option to allow explicit closing
    span: Option<Span>,
}

impl<Fut, F> Future for HttpOtelFuture<Fut, F>
where
    Fut: Future<Output = Result<serde_json::Value, Error>>,
    F: Fn(),
{
    type Output = Fut::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Wait for the underlying future to complete
        let ready = ready!(self
            .as_mut()
            .project()
            .future
            .as_pin_mut()
            .expect("future polled after completion")
            .poll(cx));

        // Record response status and any errors before closing the span
        if let Some(span) = self.as_mut().project().span.as_ref() {
            match &ready {
                Ok(response) => {
                    if let Some(status_code) = response["statusCode"].as_i64() {
                        span.record("http.response.status_code", status_code);
                        if (200..300).contains(&status_code) {
                            span.context().span().set_status(Status::Ok);
                        } else {
                            span.context()
                                .span()
                                .set_status(Status::error(format!("HTTP error {}", status_code)));
                        }
                    }
                }
                Err(e) => {
                    span.record("http.response.status_code", 500);
                    span.context().span().record_error(e.as_ref());
                    span.context()
                        .span()
                        .set_status(Status::error(e.to_string()));
                }
            }
        }

        // Explicitly close spans and futures before flushing to ensure all telemetry
        // is captured within the current invocation. This prevents spans from being
        // carried over to subsequent invocations.
        self.as_mut().project().span.take();  // Close span
        Pin::set(&mut self.as_mut().project().future, None);  // Drop future
        
        // Flush only after everything is closed to ensure complete trace data
        (self.project().flush_fn)();

        Poll::Ready(ready)
    }
}

impl<S, F, T> Service<LambdaEvent<T>> for HttpOtelService<S, F>
where
    S: Service<LambdaEvent<T>, Response = serde_json::Value, Error = Error> + Send + 'static,
    S::Future: Send + 'static,
    T: HttpEvent + Send + Sync + 'static,
    F: Fn() + Clone + Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = HttpOtelFuture<Instrumented<S::Future>, F>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, event: LambdaEvent<T>) -> Self::Future {
        let method = event.payload.method();
        let uri = event.payload.target();
        let route = event.payload.route();
        let headers = event.payload.headers();

        println!("Incoming headers: {:?}", headers);

        // Extract parent context from headers
        let parent_cx = global::get_text_map_propagator(|propagator| {
            propagator.extract(&HeaderExtractor(headers))
        });

        println!(
            "Creating span - coldstart: {}, method: {}, route: {}, parent context: {:?}", 
            self.coldstart, 
            method,
            route,
            parent_cx.span().span_context()
        );

        let span = create_span(
            &method,
            &uri,
            &route,
            headers,
            &event.context.request_id,
            self.coldstart,
        );
        span.set_parent(parent_cx);
        println!("Span created: {:?}", span.context().span().span_context());

        self.coldstart = false;

        let fut = self.inner.call(event).instrument(span.clone());

        HttpOtelFuture {
            future: Some(fut),
            flush_fn: self.flush_fn.clone(),
            span: Some(span),
        }
    }
}

/// Creates a new span for an HTTP request.
///
/// This function is used internally to create a span with appropriate attributes
/// for an incoming HTTP request.
///
/// # Arguments
///
/// * `method` - The HTTP method of the request
/// * `uri` - The target URI of the request
/// * `route` - The route or resource path of the request
/// * `headers` - The HTTP headers of the request
/// * `request_id` - The unique identifier for this request
/// * `is_coldstart` - Boolean indicating if this is a cold start invocation
///
/// # Returns
///
/// A new `Span` with HTTP request attributes and Lambda-specific fields.
fn create_span(
    method: &Method,
    uri: &str,
    route: &str,
    headers: &HeaderMap,
    request_id: &str,
    is_coldstart: bool,
) -> Span {
    tracing::info_span!(
        "lambda-invocation",
        name = format!("{} {}", method, route),
        "otel.kind" = "server",
        "http.method" = %method,
        "http.route" = %route,
        "http.target" = %uri,
        "http.user_agent" = %headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or(""),
        { traceconv::FAAS_TRIGGER } = "http",
        { traceconv::FAAS_INVOCATION_ID } = request_id,
        { traceconv::FAAS_COLDSTART } = is_coldstart,
        "http.response.status_code" = tracing::field::Empty,
    )
}

// Implement HttpEvent for ApiGatewayProxyRequest, ApiGatewayV2httpRequest, and AlbTargetGroupRequest

/// Implementation of HttpEvent for API Gateway Proxy requests.
impl HttpEvent for ApiGatewayProxyRequest {
    fn method(&self) -> Method {
        self.http_method.clone()
    }

    fn target(&self) -> String {
        let path = self.path.as_deref().unwrap_or("/");
        path.to_string()
    }

    fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    fn route(&self) -> String {
        self.resource.clone().unwrap_or_default()
    }
}

/// Implementation of HttpEvent for API Gateway V2 HTTP requests.
impl HttpEvent for ApiGatewayV2httpRequest {
    fn method(&self) -> Method {
        self.http_method.clone()
    }

    fn target(&self) -> String {
        self.raw_path.as_deref().unwrap_or("/").to_string()
    }

    fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    fn route(&self) -> String {
        self.raw_path
            .as_deref()
            .unwrap_or("/")
            .to_string()
    }
}

/// Implementation of HttpEvent for Application Load Balancer Target Group requests.
impl HttpEvent for AlbTargetGroupRequest {
    fn method(&self) -> Method {
        self.http_method.clone()
    }

    fn target(&self) -> String {
        self.path.as_deref().unwrap_or("/").to_string()
    }

    fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    fn route(&self) -> String {
        self.path.as_deref().unwrap_or("/").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lambda_events::event::alb::AlbTargetGroupRequest;
    use aws_lambda_events::event::apigw::{ApiGatewayProxyRequest, ApiGatewayV2httpRequest};
    use http::header::HeaderMap;
    use http::Method;
    use lambda_runtime::Error;
    use lambda_runtime::LambdaEvent;
    use opentelemetry::trace::SpanContext;
    use opentelemetry::trace::TracerProvider;
    use opentelemetry_http::HeaderInjector;
    use opentelemetry_sdk::propagation::TraceContextPropagator;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::prelude::*;

    #[test]
    fn test_http_event_api_gateway_proxy_request() {
        let request = ApiGatewayProxyRequest {
            http_method: Method::POST,
            path: Some("/api/test".to_string()),
            headers: HeaderMap::new(),
            resource: Some("/api/resource".to_string()),
            ..Default::default()
        };

        let http_event: &dyn HttpEvent = &request;
        assert_eq!(http_event.method(), Method::POST);
        assert_eq!(http_event.target(), "/api/test");
        assert_eq!(http_event.route(), "/api/resource");
    }

    #[test]
    fn test_http_event_api_gateway_v2_http_request() {
        let request = ApiGatewayV2httpRequest {
            http_method: Method::PUT,
            raw_path: Some("/v2/test".to_string()),
            headers: HeaderMap::new(),
            resource: Some("/v2/resource".to_string()),
            ..Default::default()
        };

        let http_event: &dyn HttpEvent = &request;
        assert_eq!(http_event.method(), Method::PUT);
        assert_eq!(http_event.target(), "/v2/test");
        assert_eq!(http_event.route(), "/v2/test");
    }

    #[test]
    fn test_http_event_alb_target_group_request() {
        let request = AlbTargetGroupRequest {
            http_method: Method::DELETE,
            path: Some("/alb/test".to_string()),
            headers: HeaderMap::new(),
            ..Default::default()
        };

        let http_event: &dyn HttpEvent = &request;
        assert_eq!(http_event.method(), Method::DELETE);
        assert_eq!(http_event.target(), "/alb/test");
        assert_eq!(http_event.route(), "/alb/test");
    }

    /// Mock service for testing `HttpOtelService`.
    ///
    /// This struct mocks the inner service and records whether it received the propagated context.
    #[derive(Clone)]
    struct MockService {
        received_context: Arc<Mutex<Option<SpanContext>>>,
    }

    impl MockService {
        fn new() -> Self {
            MockService {
                received_context: Arc::new(Mutex::new(None)),
            }
        }
    }

    impl Service<LambdaEvent<ApiGatewayProxyRequest>> for MockService {
        type Response = serde_json::Value;
        type Error = Error;
        type Future = futures::future::Ready<Result<Self::Response, Self::Error>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _event: LambdaEvent<ApiGatewayProxyRequest>) -> Self::Future {
            // Capture the current OpenTelemetry span
            let span = tracing::Span::current();
            let span_context = span.context().span().span_context().clone();
            let mut lock = self.received_context.lock().unwrap();
            *lock = Some(span_context);

            // Return a dummy response
            futures::future::ready(Ok(serde_json::json!({
                "statusCode": 200,
                "body": "Test response"
            })))
        }
    }

    #[tokio::test]
    async fn test_http_otel_service_context_propagation() {
        // Set the global propagator
        global::set_text_map_propagator(TraceContextPropagator::new());

        // Initialize OpenTelemetry tracer
        let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
            .with_simple_exporter(opentelemetry_stdout::SpanExporter::default())
            .build();
        let provider_clone = tracer_provider.clone();
        let tracer = tracer_provider.tracer("test_tracer");
        global::set_tracer_provider(tracer_provider);

        // Set up tracing subscriber with OpenTelemetry layer
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
        let subscriber = tracing_subscriber::registry().with(telemetry);
        tracing::subscriber::set_global_default(subscriber).expect("Failed to set subscriber");

        // Create a parent span using `tracing`
        let parent_span = tracing::info_span!("parent_span");
        let parent_context = parent_span.context();

        // Create a mock HTTP event with headers containing trace context
        let mut headers = HeaderMap::new();
        global::get_text_map_propagator(|propagator| {
            propagator.inject_context(&parent_context, &mut HeaderInjector(&mut headers))
        });

        let request = ApiGatewayProxyRequest {
            http_method: Method::GET,
            path: Some("/test".to_string()),
            headers,
            resource: Some("/test".to_string()),
            ..Default::default()
        };

        let mock_service = MockService::new();

        // Wrap the mock service with the HttpOtelService
        let mut otel_service = HttpOtelLayer::new(|| {
            provider_clone.force_flush();
        })
        .layer(mock_service.clone());

        // Enter the parent span's context
        let _guard = parent_span.enter();

        // Invoke the service
        let _response = otel_service
            .call(LambdaEvent::new(request, Default::default()))
            .await
            .expect("Service call should succeed");

        // Verify that the inner service received the propagated context
        let received_span_context = mock_service
            .received_context
            .lock()
            .unwrap()
            .take()
            .expect("Inner service did not receive any context");

        let parent_context = parent_span.context();
        let parent_span = parent_context.span();
        let parent_span_context = parent_span.span_context();

        println!("Received span context: {:?}", received_span_context);
        println!("Parent trace ID: {:?}", parent_span_context.trace_id());
        println!("Received trace ID: {:?}", received_span_context.trace_id());

        assert_eq!(
            received_span_context.trace_id(),
            parent_span_context.trace_id(),
            "Trace ID should be the same as parent"
        );
        assert!(
            received_span_context.is_valid(),
            "Received span context should be valid"
        );

        // After the test
        global::shutdown_tracer_provider();
    }
}
