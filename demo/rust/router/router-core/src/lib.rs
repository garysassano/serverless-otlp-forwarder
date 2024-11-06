use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use regex::Regex;
use serde_json::{json, Value};
use lambda_runtime::Error;
use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
use lazy_static::lazy_static;
use std::sync::Mutex;
use std::any::{TypeId, Any};
use aws_lambda_events::{
    alb::AlbTargetGroupRequest,
    apigw::{ApiGatewayProxyRequest, ApiGatewayWebsocketProxyRequest},
};
pub use ctor;  // Re-export ctor for use by the macro


pub const DEFAULT_ROUTER_MODULE: &str = "__lambda_lw_router_core_default_router";

pub trait LambdaHttpEvent: Send + Sync + Clone + 'static {
    fn raw_path(&self) -> Option<String>;
    fn http_method(&self) -> String;
}

impl LambdaHttpEvent for ApiGatewayV2httpRequest {
    fn raw_path(&self) -> Option<String> {
        self.raw_path.clone()
    }

    fn http_method(&self) -> String {
        self.request_context.http.method.to_string()
    }
}

impl LambdaHttpEvent for ApiGatewayProxyRequest {
    fn raw_path(&self) -> Option<String> {
        self.path.clone()
    }

    fn http_method(&self) -> String {
        self.http_method.to_string()
    }
}

impl LambdaHttpEvent for AlbTargetGroupRequest {
    fn raw_path(&self) -> Option<String> {
        self.path.clone()
    }

    fn http_method(&self) -> String {
        self.http_method.to_string()
    }
}

impl LambdaHttpEvent for ApiGatewayWebsocketProxyRequest {
    fn raw_path(&self) -> Option<String> {
        self.path.clone()
    }

    fn http_method(&self) -> String {
        self.http_method.clone().map(|m| m.to_string()).unwrap_or_else(|| "GET".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct RouteContext<State: Clone, E> {
    pub path: String,
    pub method: String,
    pub params: HashMap<String, String>,
    pub state: Arc<State>,
    pub event: E,
    pub lambda_context: lambda_runtime::Context,
}

pub struct Router<State, E> 
where
    State: Send + Sync + Clone + 'static,
    E: LambdaHttpEvent,
{
    routes: HashMap<
        String,
        (
            Arc<dyn Fn(RouteContext<State, E>) -> Pin<Box<dyn Future<Output = Result<Value, Error>> + Send>> + Send + Sync>,
            Regex,
        ),
    >,
}

impl<State, E: LambdaHttpEvent> Router<State, E> 
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
        Fut: Future<Output = Result<Value, Error>> + Send + 'static,
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
            Box::pin(handler(ctx)) as Pin<Box<dyn Future<Output = Result<Value, Error>> + Send>>
        });

        let key = format!("{} {}", method.to_uppercase(), path);
        self.routes.insert(key, (handler, regex));
    }

    pub async fn handle_request(
        &self,
        event: E,
        lambda_context: lambda_runtime::Context,
        state: Arc<State>,
    ) -> Result<Value, Error> {
        let raw_path = event.raw_path();
        let path = raw_path.as_deref().unwrap_or("/").to_string();
        let method = event.http_method();

        let ctx = RouteContext {
            path: path.clone(),
            method: method.to_uppercase(),
            params: HashMap::new(),
            state,
            event: event.clone(),
            lambda_context,
        };

        for (_route_key, (handler_fn, regex)) in &self.routes {
            if let Some(captures) = regex.captures(&path) {
                let mut params = HashMap::new();
                for name in regex.capture_names().flatten() {
                    if let Some(value) = captures.name(name) {
                        params.insert(name.to_string(), value.as_str().to_string());
                    }
                }
                
                let new_ctx = RouteContext {
                    params,
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

pub struct RouterBuilder<State, E: LambdaHttpEvent> 
where
    State: Send + Sync + Clone + 'static,
{
    routes: Vec<(
        String, 
        String, 
        Box<dyn Fn(RouteContext<State, E>) -> Pin<Box<dyn Future<Output = Result<Value, Error>> + Send>> + Send + Sync>
    )>,
}

impl<State, E: LambdaHttpEvent> RouterBuilder<State, E> 
where
    State: Send + Sync + Clone + 'static,
{
    pub fn new() -> Self {
        Self { routes: Vec::new() }
    }
    
    pub fn route<F, Fut>(mut self, method: &str, path: &str, handler: F) -> Self 
    where
        F: Fn(RouteContext<State, E>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, Error>> + Send + 'static,
    {
        let handler = Box::new(move |ctx| {
            Box::pin(handler(ctx)) as Pin<Box<dyn Future<Output = Result<Value, Error>> + Send>>
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

type BoxedHandler<State, E> = dyn Fn(RouteContext<State, E>) -> Pin<Box<dyn Future<Output = Result<Value, Error>> + Send>>
    + Send
    + Sync;

#[derive(Clone)]
struct RouteEntry<State: Clone, E: LambdaHttpEvent> {
    method: &'static str,
    path: &'static str,
    handler: Arc<BoxedHandler<State, E>>,
}

lazy_static! {
    static ref ROUTE_REGISTRY: Mutex<HashMap<(TypeId, TypeId), Box<dyn Any + Send + Sync>>> = 
        Mutex::new(HashMap::new());
}

pub fn register_route<State, E: LambdaHttpEvent>(
    method: &'static str,
    path: &'static str,
    handler: impl Fn(RouteContext<State, E>) -> Pin<Box<dyn Future<Output = Result<Value, Error>> + Send>>
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

