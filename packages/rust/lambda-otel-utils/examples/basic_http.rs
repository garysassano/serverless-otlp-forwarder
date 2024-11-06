use aws_lambda_events::event::apigw::ApiGatewayProxyRequest;
use lambda_otel_utils::{HttpOtelLayer, HttpTracerProviderBuilder, Layer};
use lambda_runtime::{service_fn, Error, LambdaEvent};
use serde_json::Value;

async fn function_handler(_event: LambdaEvent<ApiGatewayProxyRequest>) -> Result<Value, Error> {
    Ok(serde_json::json!({"message": "Hello from Lambda!"}))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize tracer provider
    let tracer_provider = HttpTracerProviderBuilder::default()
        .with_stdout_client()
        .with_tracer_name("example-lambda-function")
        .build()?;
    
    // Create service with tracing layer
    let service = HttpOtelLayer::new(|| {
        tracer_provider.force_flush();
    })
    .layer(service_fn(function_handler));

    // Run the Lambda runtime
    lambda_runtime::run(service).await?;
    Ok(())
}
