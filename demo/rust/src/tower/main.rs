use lambda_otel_lite::{init_telemetry, OtelTracingLayer, TelemetryConfig, SpanAttributes};
use lambda_runtime::tower::ServiceBuilder;
use lambda_runtime::{Error, LambdaEvent, Runtime};
use serde_json::{json, Value};
use tracing::instrument;
use aws_lambda_events::event::apigw::ApiGatewayV2httpRequest;
use std::collections::HashMap;

#[instrument(skip_all)]
async fn nested_function() {
    tracing::debug!("Nested function called");
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

    // Build service with OpenTelemetry tracing middleware
    let service = ServiceBuilder::new()
        .layer(OtelTracingLayer::new(completion_handler)
                .with_name("tower-handler")
                .with_extractor_fn(|event: &LambdaEvent<ApiGatewayV2httpRequest>| {
                    let mut attributes = HashMap::new();
                    if let Some(name) = event.payload.query_string_parameters.first("name") {
                        attributes.insert("user.name".to_string(), name.to_string());
                    }
                    attributes.insert(
                        "function.name".to_string(),
                        event.context.invoked_function_arn.split(':').last()
                            .unwrap_or("unknown").to_string()
                    );
                    
                    SpanAttributes {
                        kind: None,
                        attributes,
                        links: vec![],
                        carrier: None,
                    }
                })
        )
        .service_fn(function_handler);

    Runtime::new(service).run().await
}
