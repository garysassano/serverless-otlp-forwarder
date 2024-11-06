//! Lambda LightWeight Router (lambda-lw-router) is a lightweight routing library for AWS Lambda HTTP events.
//! 
//! It provides a simple and efficient way to define routes and handlers for AWS Lambda functions
//! that process HTTP events from API Gateway, Application Load Balancer, and WebSocket APIs.
//! 
//! # Features
//! 
//! * Support for multiple AWS event types (API Gateway v2, v1, ALB, WebSocket)
//! * Path parameter extraction
//! * Type-safe route handlers
//! * Compile-time route registration
//! * Minimal runtime overhead
//! 
//! # Quick Start
//! 
//! ```rust
//! use lambda_lw_router::{define_router, route};
//! use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
//! 
//! // Define your application state
//! #[derive(Clone)]
//! struct AppState {
//!     // your state fields here
//! }
//! 
//! // Set up the router
//! define_router!(event = ApiGatewayV2httpRequest);
//! 
//! // Define a route handler
//! #[route(path = "/hello/{name}")]
//! async fn handle_hello(ctx: RouteContext) -> Result<Value, Error> {
//!     let name = ctx.params.get("name").unwrap_or("World");
//!     Ok(json!({
//!         "message": format!("Hello, {}!", name)
//!     }))
//! }
//! ```

pub use lambda_lw_router_core::*;
pub use lambda_lw_router_macro::route;

/// Default module name used when no custom name is provided in `define_router!`.
pub const DEFAULT_ROUTER_MODULE: &str = "__lambda_lw_router_core_default_router";

/// Defines a router module with the necessary type aliases for your Lambda application.
/// 
/// This macro creates a module containing type aliases for the router components,
/// making them easily accessible throughout your application. It's typically used
/// at the beginning of your Lambda function to set up the routing infrastructure.
/// 
/// # Type Aliases
/// 
/// The macro creates the following type aliases:
/// * `Event` - The Lambda HTTP event type (e.g., ApiGatewayV2httpRequest)
/// * `Router` - The router instance type for your application
/// * `RouterBuilder` - The builder type for constructing routers
/// * `RouteContext` - The context type passed to route handlers
/// 
/// # Arguments
/// 
/// * `event` - The Lambda HTTP event type (required). Supported types:
///   * `ApiGatewayV2httpRequest` - API Gateway HTTP API v2
///   * `ApiGatewayProxyRequest` - API Gateway REST API v1
///   * `AlbTargetGroupRequest` - Application Load Balancer
///   * `ApiGatewayWebsocketProxyRequest` - API Gateway WebSocket
/// * `name` - The module name (optional, defaults to an internal name)
/// 
/// # Examples
/// 
/// Basic usage with default module name:
/// ```rust
/// use lambda_lw_router::define_router;
/// use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
/// 
/// define_router!(event = ApiGatewayV2httpRequest);
/// 
/// // This creates a module with the following types:
/// // Router - Router<AppState, ApiGatewayV2httpRequest>
/// // RouterBuilder - RouterBuilder<AppState, ApiGatewayV2httpRequest>
/// // RouteContext - RouteContext<AppState, ApiGatewayV2httpRequest>
/// ```
/// 
/// Custom module name for better readability or multiple routers:
/// ```rust
/// use lambda_lw_router::define_router;
/// use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
/// use aws_lambda_events::alb::AlbTargetGroupRequest;
/// 
/// // Define an API Gateway router
/// define_router!(event = ApiGatewayV2httpRequest, name = api_router);
/// 
/// // Define an ALB router in the same application
/// define_router!(event = AlbTargetGroupRequest, name = alb_router);
/// 
/// // Now you can use specific types for each router:
/// // api_router::Router
/// // api_router::RouterBuilder
/// // api_router::RouteContext
/// // 
/// // alb_router::Router
/// // alb_router::RouterBuilder
/// // alb_router::RouteContext
/// ```
/// 
/// # Usage with Route Handlers
/// 
/// The module name defined here should match the `module` parameter in your route handlers:
/// 
/// ```rust
/// define_router!(event = ApiGatewayV2httpRequest, name = api_router);
/// 
/// #[route(path = "/hello", module = "api_router")]
/// async fn handle_hello(ctx: api_router::RouteContext) -> Result<Value, Error> {
///     Ok(json!({ "message": "Hello, World!" }))
/// }
/// ```
#[macro_export]
macro_rules! define_router {
    (event = $event_type:ty, name = $name:ident) => {
        pub mod $name {
            use super::*;
            
            pub type Event = $event_type;
            pub type Router = ::lambda_lw_router::Router<AppState, Event>;
            pub type RouterBuilder = ::lambda_lw_router::RouterBuilder<AppState, Event>;
            pub type RouteContext = ::lambda_lw_router::RouteContext<AppState, Event>;
        }
        pub use $name::*;
    };
    
    (event = $event_type:ty) => {
        mod __lambda_lw_router_core_default_router {
            use super::*;
            
            pub type Event = $event_type;
            pub type Router = ::lambda_lw_router::Router<AppState, Event>;
            pub type RouterBuilder = ::lambda_lw_router::RouterBuilder<AppState, Event>;
            pub type RouteContext = ::lambda_lw_router::RouteContext<AppState, Event>;
        }
        pub use __lambda_lw_router_core_default_router::*;
    };
}
