use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
use lambda_lw_http_router::{define_router, route};
use lambda_otel_lite::{init_telemetry, TelemetryConfig};
use lambda_otel_utils::{OpenTelemetryFaasTrigger, OpenTelemetryLayer};
use lambda_runtime::{service_fn, Error, LambdaEvent, Runtime};
use serde_json::{json, Value};
use std::sync::Arc;

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
    let name = ctx.params.get("name").map(|s| s.as_str()).unwrap();
    Ok(json!({
        "message": format!("Hello, {}!", name)
    }))
}

// Lambda function entry point
#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize telemetry first (without logging)
    let completion_handler = init_telemetry(TelemetryConfig::default()).await?;

    let state = Arc::new(AppState {});
    let router = Arc::new(RouterBuilder::from_registry().build());

    let lambda = move |event: LambdaEvent<ApiGatewayV2httpRequest>| {
        let state = state.clone();
        let router = router.clone();
        async move { router.handle_request(event, state).await }
    };

    // Use OpenTelemetryLayer for creating the initial span
    // and signal completion to the internal extension
    let runtime = Runtime::new(service_fn(lambda)).layer(
        OpenTelemetryLayer::new(move || completion_handler.complete())
            .with_trigger(OpenTelemetryFaasTrigger::Http),
    );

    runtime.run().await
}
