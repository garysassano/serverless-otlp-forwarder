# Lambda LightWeight HTTP Router

A lightweight, type-safe HTTP router for AWS Lambda functions with support for API Gateway, Application Load Balancer, and WebSocket APIs.

## Features

- 🚀 Zero runtime overhead with compile-time route registration
- 🔒 Type-safe route handlers and application state
- 🎯 Path parameter extraction
- 🔄 Support for multiple AWS event types:
  - API Gateway HTTP API (v2)
  - API Gateway REST API (v1)
  - Application Load Balancer
  - WebSocket API
- 📊 OpenTelemetry integration for tracing
- 🏗️ Builder pattern for easy router construction
- 🧩 Modular design with separate core and macro crates

## Installation

Run `cargo add lambda-lw-http-router` to add the crate to your project or add the following to your `Cargo.toml`:

```toml
[dependencies]
lambda-lw-http-router = "0.1.0"
```

## Quick Start

```rust
use lambda_lw_http_router::{define_router, route};
use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
use serde_json::{json, Value};
use lambda_runtime::Error;

// Define your application state
#[derive(Clone)]
struct AppState {
    // your state fields here
}

// Set up the router
define_router!(event = ApiGatewayV2httpRequest, state = AppState);

// Define route handlers
#[route(path = "/hello/{name}")]
async fn handle_hello(ctx: RouteContext) -> Result<Value, Error> {
    let name = ctx.params.get("name").map(|s| s.as_str()).unwrap_or("World");
    Ok(json!({
        "message": format!("Hello, {}!", name)
    }))
}

// Lambda function entry point
#[tokio::main]
async fn main() -> Result<(), Error> {
    let state = Arc::new(AppState {});
    let router = RouterBuilder::from_registry().build();
    
    lambda_runtime::run(handler(router, state)).await?;
    Ok(())
}
```

## Advanced Usage

### Multiple Routers

You can define multiple routers for different event types:

```rust
use aws_lambda_events::alb::AlbTargetGroupRequest;

// Define an API Gateway router
define_router!(event = ApiGatewayV2httpRequest, module = api_router, state = AppState);

// Define an ALB router
define_router!(event = AlbTargetGroupRequest, module = alb_router, state = AppState);

// Use specific types for each router
#[route(path = "/api/hello", module = "api_router")]
async fn api_hello(ctx: api_router::RouteContext) -> Result<Value, Error> {
    // ...
}

#[route(path = "/alb/hello", module = "alb_router")]
async fn alb_hello(ctx: alb_router::RouteContext) -> Result<Value, Error> {
    // ...
}
```

### OpenTelemetry Integration

The router automatically integrates with OpenTelemetry for tracing:

```rust
#[route(path = "/users/{id}", set_span_name = true)]
async fn get_user(ctx: RouteContext) -> Result<Value, Error> {
    // Span name will be "GET /users/{id}"
    ctx.set_otel_attribute("user.id", ctx.params.get("id").unwrap());
    // ...
}
```

### Path Parameters

Support for various path parameter patterns:

```rust
// Basic parameters
#[route(path = "/users/{id}")]

// Multi-segment parameters
#[route(path = "/files/{path+}")]  // Matches /files/docs/2024/report.pdf

// Multiple parameters
#[route(path = "/users/{user_id}/posts/{post_id}")]
```

## API Documentation

For detailed API documentation, run:

```bash
cargo doc --open
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request