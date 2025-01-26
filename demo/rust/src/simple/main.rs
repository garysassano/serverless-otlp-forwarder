use lambda_otel_lite::{init_telemetry, traced_handler, TelemetryConfig, TracedHandlerOptions};
use lambda_runtime::{service_fn, tracing, Error, LambdaEvent, Runtime};
use serde_json::json;
use serde_json::Value;
use tracing::instrument;
use aws_lambda_events::event::apigw::ApiGatewayV2httpRequest;

#[instrument(skip_all)]
async fn nested_function() {
    tracing::info!("Nested function called");
}

async fn function_handler(event: LambdaEvent<ApiGatewayV2httpRequest>) -> Result<Value, Error> {
    let who = event
        .payload
        .query_string_parameters.first("name")
        .unwrap_or("world");

    nested_function().await;

    Ok(json!({
        "statusCode": 200,
        "body": json!({
            "message": format!("Hello {who}, this is an AWS Lambda HTTP request")
        }).to_string()
    }))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize telemetry with async mode
    let completion_handler = init_telemetry(TelemetryConfig::default()).await?;

    // Create the Lambda service with tracing
    let func = service_fn(move |event| {
        traced_handler(
            TracedHandlerOptions::default()
                .with_name("simple-handler")
                .with_event(event),
            completion_handler.clone(),
            function_handler,
        )
    });

    Runtime::new(func).run().await
}
