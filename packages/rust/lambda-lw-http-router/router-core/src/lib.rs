use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use regex::Regex;
use serde_json::{json, Value as JsonValue};
use lambda_runtime::{Error, LambdaEvent};
use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
use lazy_static::lazy_static;
use std::sync::Mutex;
use std::any::{TypeId, Any};
use aws_lambda_events::{
    alb::AlbTargetGroupRequest,
    apigw::{ApiGatewayProxyRequest, ApiGatewayWebsocketProxyRequest},
};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use opentelemetry::{Key as OtelKey, Value as OtelValue};
pub use ctor;  // Re-export ctor for use by the macro


/// A trait for HTTP events that can be routed by the router.
/// 
/// This trait defines the minimum requirements for an HTTP event to be
/// usable with the routing system. Any event type that implements this
/// trait can be used with the router.
/// 
/// # Examples
/// 
/// ```rust
/// use lambda_lw_http_router_core::RoutableHttpEvent;
/// 
/// #[derive(Clone)]  // Required by RoutableHttpEvent
/// struct CustomHttpEvent {
///     path: String,
///     method: String,
/// }
/// 
/// impl RoutableHttpEvent for CustomHttpEvent {
///     fn raw_path(&self) -> Option<String> {
///         Some(self.path.clone())
///     }
///     
///     fn http_method(&self) -> String {
///         self.method.clone()
///     }
/// }
/// ```
pub trait RoutableHttpEvent: Send + Sync + Clone + 'static {
    /// Returns the raw path of the HTTP request
    fn raw_path(&self) -> Option<String>;
    
    /// Returns the HTTP method of the request
    fn http_method(&self) -> String;

    /// Returns the API Gateway resource pattern if available
    fn resource(&self) -> Option<String> {
        None
    }

    /// Returns pre-parsed path parameters if available
    fn path_parameters(&self) -> Option<&HashMap<String, String>> {
        None
    }
}

impl RoutableHttpEvent for ApiGatewayV2httpRequest {
    fn raw_path(&self) -> Option<String> {
        self.raw_path.clone()
    }

    fn http_method(&self) -> String {
        self.request_context.http.method.to_string()
    }
}

impl RoutableHttpEvent for ApiGatewayProxyRequest {
    fn raw_path(&self) -> Option<String> {
        self.path.clone()
    }

    fn http_method(&self) -> String {
        self.http_method.to_string()
    }

    fn resource(&self) -> Option<String> {
        self.resource.clone()
    }

    fn path_parameters(&self) -> Option<&HashMap<String, String>> {
        Some(&self.path_parameters)
    }
}

impl RoutableHttpEvent for AlbTargetGroupRequest {
    fn raw_path(&self) -> Option<String> {
        self.path.clone()
    }

    fn http_method(&self) -> String {
        self.http_method.to_string()
    }
}

impl RoutableHttpEvent for ApiGatewayWebsocketProxyRequest {
    fn raw_path(&self) -> Option<String> {
        self.path.clone()
    }

    fn http_method(&self) -> String {
        self.http_method.clone().map(|m| m.to_string()).unwrap_or_else(|| "GET".to_string())
    }
}

/// Context passed to route handlers containing request information and application state.
/// 
/// This struct provides access to:
/// - The request path and method
/// - Path parameters extracted from the URL
/// - Application state
/// - The original Lambda event
/// - Lambda execution context
#[derive(Debug, Clone)]
pub struct RouteContext<State: Clone, E> {
    /// The full request path
    pub path: String,
    /// The HTTP method (GET, POST, etc.)
    pub method: String,
    /// Path parameters extracted from the URL (e.g., {id} -> "123")
    pub params: HashMap<String, String>,
    /// Application state shared across all requests
    pub state: Arc<State>,
    /// The original Lambda event
    pub event: E,
    /// Lambda execution context
    pub lambda_context: lambda_runtime::Context,
    /// The route template pattern (e.g., "/quote/{id}")
    pub route_pattern: String,
}

impl<State: Clone, E> RouteContext<State, E> {
    /// Helper method to set the OpenTelemetry span name using the route pattern
    pub fn set_otel_span_name(&self) -> &Self {
        let span = Span::current();
        let span_name = format!("{} {}", self.method, self.route_pattern);
        span.record("otel.name", &span_name);
        self
    }

    /// Helper method to set a single attribute on the current OpenTelemetry span
    /// 
    /// # Examples
    /// ```
    /// ctx.set_otel_attribute("string_attr", "value");
    /// ctx.set_otel_attribute("int_attr", 42);
    /// ctx.set_otel_attribute("bool_attr", true);
    /// ```
    pub fn set_otel_attribute(&self, key: impl Into<OtelKey>, value: impl Into<OtelValue>) -> &Self {
        let span = Span::current();
        span.set_attribute(key, value);
        self
    }
}

/// The main router type that handles HTTP requests and dispatches them to handlers.
/// 
/// The router matches incoming requests against registered routes and executes
/// the corresponding handlers. It supports path parameters and different HTTP methods.
/// 
/// # Type Parameters
/// 
/// * `State` - The type of the application state shared across handlers
/// * `E` - The Lambda event type (must implement `RoutableHttpEvent`)
/// 
/// # Examples
/// 
/// ```rust
/// use lambda_lw_http_router_core::{Router, RouteContext};
/// use serde_json::{json, Value};
/// use lambda_runtime::Error;
/// use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
/// 
/// #[derive(Clone)]
/// struct State {}
/// 
/// let mut router: Router<State, ApiGatewayV2httpRequest> = Router::new();
/// router.register_route("GET", "/hello/{name}", |ctx| async move {
///     let name = ctx.params.get("name").map_or("World", String::as_str);
///     Ok(json!({ "message": format!("Hello, {}!", name) }))
/// });
/// ```
pub struct Router<State, E> 
where
    State: Send + Sync + Clone + 'static,
    E: RoutableHttpEvent,
{
    routes: HashMap<
        String,
        (
            Arc<dyn Fn(RouteContext<State, E>) -> Pin<Box<dyn Future<Output = Result<JsonValue, Error>> + Send>> + Send + Sync>,
            Regex,
        ),
    >,
}

impl<State, E: RoutableHttpEvent> Router<State, E> 
where
    State: Send + Sync + Clone + 'static,
{
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    pub fn register_route<F, Fut>(&mut self, method: &str, path: &str, handler: F)
    where
        F: Fn(RouteContext<State, E>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<JsonValue, Error>> + Send + 'static,
    {
        let regex_pattern = path
            .split('/')
            .map(|segment| {
                if segment.starts_with('{') && segment.ends_with('}') {
                    let param_name = segment[1..segment.len()-1].trim_end_matches('+');
                    if segment.ends_with("+}") {
                        // Greedy match for proxy+ style parameters
                        format!("(?P<{}>.*)", param_name)
                    } else {
                        // Normal parameter match (non-greedy, no slashes)
                        format!("(?P<{}>[^/]+)", param_name)
                    }
                } else {
                    regex::escape(segment)  // Escape regular segments
                }
            })
            .collect::<Vec<_>>()
            .join("/");
                
        let regex = Regex::new(&format!("^{}$", regex_pattern))
            .expect("Invalid route pattern");
        
        let handler = Arc::new(move |ctx| {
            Box::pin(handler(ctx)) as Pin<Box<dyn Future<Output = Result<JsonValue, Error>> + Send>>
        });

        let key = format!("{} {}", method.to_uppercase(), path);
        self.routes.insert(key, (handler, regex));
    }

    pub async fn handle_request(
        &self,
        event: LambdaEvent<E>,
        state: Arc<State>,
    ) -> Result<JsonValue, Error> {
        let (payload, lambda_context) = event.into_parts();
        let raw_path = payload.raw_path();
        let path = raw_path.as_deref().unwrap_or("/").to_string();
        let method = payload.http_method().to_uppercase();

        let ctx = RouteContext {
            path: path.clone(),
            method: method.clone(),
            params: HashMap::new(),
            state,
            event: payload.clone(),
            lambda_context,
            route_pattern: String::new(),
        };

        // Check if we have a matching route
        for (route_key, (handler_fn, regex)) in &self.routes {
            // Extract method and path from route_key
            let parts: Vec<&str> = route_key.split_whitespace().collect();
            let (route_method, route_path) = match parts.as_slice() {
                [method, path] => (method.to_uppercase(), path),
                _ => continue, // Invalid route key format
            };

            // Check if methods match
            if method != route_method {
                continue;
            }

            // If we have both resource and path_parameters, validate against API Gateway configuration
            if let (Some(resource), Some(path_params)) = (payload.resource(), payload.path_parameters()) {
                if resource == route_path.to_string() {
                    let new_ctx = RouteContext {
                        params: path_params.clone(),
                        route_pattern: route_path.to_string(),
                        ..ctx.clone()
                    };
                    return handler_fn(new_ctx).await;
                }
            }
            
            // Fall back to our own path parameter extraction
            if let Some(captures) = regex.captures(&path) {
                let mut params = HashMap::new();
                for name in regex.capture_names().flatten() {
                    if let Some(value) = captures.name(name) {
                        params.insert(name.to_string(), value.as_str().to_string());
                    }
                }
                
                let new_ctx = RouteContext {
                    params,
                    route_pattern: route_path.to_string(),
                    ..ctx.clone()
                };
                
                return handler_fn(new_ctx).await;
            }
        }

        Ok(json!({
            "statusCode": 404,
            "headers": {"Content-Type": "text/plain"},
            "body": "Not Found"
        }))
    }
}

/// A builder for constructing routers with a fluent API.
/// 
/// This type provides a more ergonomic way to create and configure routers
/// compared to using `Router` directly. It supports method chaining and
/// handles all the type complexity internally.
/// 
/// # Examples
/// 
/// ```rust
/// use lambda_lw_http_router_core::{RouterBuilder, RouteContext};
/// use serde_json::{json, Value};
/// use lambda_runtime::Error;
/// use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
/// 
/// #[derive(Clone)]
/// struct State {}
/// 
/// async fn get_users(ctx: RouteContext<State, ApiGatewayV2httpRequest>) -> Result<Value, Error> {
///     Ok(json!({ "users": [] }))
/// }
/// 
/// async fn create_user(ctx: RouteContext<State, ApiGatewayV2httpRequest>) -> Result<Value, Error> {
///     Ok(json!({ "status": "created" }))
/// }
/// 
/// let router = RouterBuilder::<State, ApiGatewayV2httpRequest>::new()
///     .route("GET", "/users", get_users)
///     .route("POST", "/users", create_user)
///     .build();
/// ```
pub struct RouterBuilder<State, E: RoutableHttpEvent> 
where
    State: Send + Sync + Clone + 'static,
{
    routes: Vec<(
        String, 
        String, 
        Box<dyn Fn(RouteContext<State, E>) -> Pin<Box<dyn Future<Output = Result<JsonValue, Error>> + Send>> + Send + Sync>
    )>,
}

impl<State, E: RoutableHttpEvent> RouterBuilder<State, E> 
where
    State: Send + Sync + Clone + 'static,
{
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }
    
    pub fn route<F, Fut>(mut self, method: &str, path: &str, handler: F) -> Self 
    where
        F: Fn(RouteContext<State, E>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<JsonValue, Error>> + Send + 'static,
    {
        let handler = Box::new(move |ctx| {
            Box::pin(handler(ctx)) as Pin<Box<dyn Future<Output = Result<JsonValue, Error>> + Send>>
        });
        self.routes.push((method.to_string(), path.to_string(), handler));
        self
    }
    
    pub fn build(self) -> Router<State, E> {
        let mut router = Router::new();
        for (method, path, handler) in self.routes {
            let handler = move |ctx: RouteContext<State, E>| {
                (handler)(ctx)
            };
            router.register_route(&method, &path, handler);
        }
        router
    }

    pub fn from_registry() -> Self {
        let mut builder = Self::new();
        
        let routes = {
            let registry = ROUTE_REGISTRY.lock().unwrap();
            registry.get(&(TypeId::of::<State>(), TypeId::of::<E>()))
                .and_then(|routes| routes.downcast_ref::<Vec<RouteEntry<State, E>>>())
                .map(|routes| routes.iter().map(|entry| {
                    (
                        entry.method,
                        entry.path,
                        Arc::clone(&entry.handler)
                    )
                }).collect::<Vec<_>>())
                .unwrap_or_default()
        };

        for (method, path, handler) in routes {
            let handler = Box::new(move |ctx: RouteContext<State, E>| {
                (handler)(ctx)
            });
            builder.routes.push((
                method.to_string(),
                path.to_string(),
                handler
            ));
        }
        
        builder
    }
}

type BoxedHandler<State, E> = dyn Fn(RouteContext<State, E>) -> Pin<Box<dyn Future<Output = Result<JsonValue, Error>> + Send>>
    + Send
    + Sync;

#[derive(Clone)]
struct RouteEntry<State: Clone, E: RoutableHttpEvent> {
    method: &'static str,
    path: &'static str,
    handler: Arc<BoxedHandler<State, E>>,
}

lazy_static! {
    static ref ROUTE_REGISTRY: Mutex<HashMap<(TypeId, TypeId), Box<dyn Any + Send + Sync>>> = 
        Mutex::new(HashMap::new());
}

pub fn register_route<State, E: RoutableHttpEvent>(
    method: &'static str,
    path: &'static str,
    handler: impl Fn(RouteContext<State, E>) -> Pin<Box<dyn Future<Output = Result<JsonValue, Error>> + Send>>
        + Send
        + Sync
        + 'static,
) where
    State: Send + Sync + Clone + 'static,
{
    let state_type_id = TypeId::of::<State>();
    let event_type_id = TypeId::of::<E>();
    let handler = Arc::new(handler) as Arc<BoxedHandler<State, E>>;
    let entry = RouteEntry {
        method,
        path,
        handler,
    };

    let mut registry = ROUTE_REGISTRY.lock().unwrap();
    let routes = registry
        .entry((state_type_id, event_type_id))
        .or_insert_with(|| Box::new(Vec::<RouteEntry<State, E>>::new()));
    
    let routes = routes.downcast_mut::<Vec<RouteEntry<State, E>>>()
        .expect("Registry type mismatch");
    routes.push(entry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use aws_lambda_events::http::Method;

    /// Test event struct that implements RoutableHttpEvent
    #[derive(Clone)]
    struct TestHttpEvent {
        path: String,
        method: String,
    }

    impl RoutableHttpEvent for TestHttpEvent {
        fn raw_path(&self) -> Option<String> {
            Some(self.path.clone())
        }

        fn http_method(&self) -> String {
            self.method.clone()
        }
    }

    /// Simple state struct for testing
    #[derive(Clone)]
    struct TestState {}

    #[tokio::test]
    async fn test_path_parameter_extraction() {
        let mut router = Router::<TestState, TestHttpEvent>::new();
        
        // Register a route with path parameters
        router.register_route("GET", "/users/{id}/posts/{post_id}", |ctx| async move {
            Ok(json!({
                "user_id": ctx.params.get("id"),
                "post_id": ctx.params.get("post_id"),
            }))
        });

        // Create a test event
        let event = TestHttpEvent {
            path: "/users/123/posts/456".to_string(),
            method: "GET".to_string(),
        };
        let lambda_context = lambda_runtime::Context::default();
        let lambda_event = LambdaEvent::new(event, lambda_context);

        // Handle the request
        let result = router.handle_request(lambda_event, Arc::new(TestState {})).await.unwrap();

        // Verify the extracted parameters
        assert_eq!(result["user_id"], "123");
        assert_eq!(result["post_id"], "456");
    }

    #[tokio::test]
    async fn test_greedy_path_parameter() {
        let mut router = Router::<TestState, TestHttpEvent>::new();
        
        // Register a route with a greedy path parameter
        router.register_route("GET", "/files/{path+}", |ctx| async move {
            Ok(json!({
                "path": ctx.params.get("path"),
            }))
        });

        // Create a test event with a nested path
        let event = TestHttpEvent {
            path: "/files/documents/2024/report.pdf".to_string(),
            method: "GET".to_string(),
        };
        let lambda_context = lambda_runtime::Context::default();
        let lambda_event = LambdaEvent::new(event, lambda_context);

        // Handle the request
        let result = router.handle_request(lambda_event, Arc::new(TestState {})).await.unwrap();

        // Verify the extracted parameter captures the full path
        assert_eq!(result["path"], "documents/2024/report.pdf");
    }

    #[tokio::test]
    async fn test_no_match_returns_404() {
        let router = Router::<TestState, TestHttpEvent>::new();
        
        // Create a test event with a path that doesn't match any routes
        let event = TestHttpEvent {
            path: "/nonexistent".to_string(),
            method: "GET".to_string(),
        };
        let lambda_context = lambda_runtime::Context::default();
        let lambda_event = LambdaEvent::new(event, lambda_context);

        // Handle the request
        let result = router.handle_request(lambda_event, Arc::new(TestState {})).await.unwrap();

        // Verify we get a 404 response
        assert_eq!(result["statusCode"], 404);
    }

    #[tokio::test]
    async fn test_apigw_resource_path_parameters() {
        let mut router = Router::<TestState, ApiGatewayProxyRequest>::new();
        
        router.register_route("GET", "/users/{id}/posts/{post_id}", |ctx| async move {
            Ok(json!({
                "params": ctx.params,
            }))
        });

        let mut path_parameters = HashMap::new();
        path_parameters.insert("id".to_string(), "123".to_string());
        path_parameters.insert("post_id".to_string(), "456".to_string());

        let event = ApiGatewayProxyRequest {
            path: Some("/users/123/posts/456".to_string()),
            http_method: Method::GET,
            resource: Some("/users/{id}/posts/{post_id}".to_string()),
            path_parameters,
            ..Default::default()
        };

        let lambda_context = lambda_runtime::Context::default();
        let lambda_event = LambdaEvent::new(event, lambda_context);

        let result = router.handle_request(lambda_event, Arc::new(TestState {})).await.unwrap();

        assert_eq!(result["params"]["id"], "123");
        assert_eq!(result["params"]["post_id"], "456");
    }

    #[tokio::test]
    async fn test_method_matching_with_apigw() {
        let mut router = Router::<TestState, ApiGatewayProxyRequest>::new();
        
        // Register both GET and POST handlers for the same path
        router.register_route("GET", "/quotes", |_| async move {
            Ok(json!({ "method": "GET" }))
        });
        
        router.register_route("POST", "/quotes", |_| async move {
            Ok(json!({ "method": "POST" }))
        });

        // Create a POST request
        let post_event = ApiGatewayProxyRequest {
            path: Some("/quotes".to_string()),
            http_method: Method::POST,
            resource: Some("/quotes".to_string()),
            path_parameters: HashMap::new(),
            ..Default::default()
        };

        let lambda_context = lambda_runtime::Context::default();
        let lambda_event = LambdaEvent::new(post_event, lambda_context);

        // Handle the POST request
        let result = router.handle_request(lambda_event, Arc::new(TestState {})).await.unwrap();
        assert_eq!(result["method"], "POST", "POST request should be handled by POST handler");

        // Create a GET request to the same path
        let get_event = ApiGatewayProxyRequest {
            path: Some("/quotes".to_string()),
            http_method: Method::GET,
            resource: Some("/quotes".to_string()),
            path_parameters: HashMap::new(),
            ..Default::default()
        };

        let lambda_context = lambda_runtime::Context::default();
        let lambda_event = LambdaEvent::new(get_event, lambda_context);

        // Handle the GET request
        let result = router.handle_request(lambda_event, Arc::new(TestState {})).await.unwrap();
        assert_eq!(result["method"], "GET", "GET request should be handled by GET handler");
    }
}

