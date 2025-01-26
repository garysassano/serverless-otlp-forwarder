use lambda_runtime::{run, service_fn, tracing, Error, LambdaEvent};
use aws_sdk_lambda::Client as LambdaClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;
use aws_sdk_lambda::primitives::Blob;
/// Request payload for the proxy function
#[derive(Deserialize)]
struct ProxyRequest {
    /// Target Lambda function to invoke
    target: String,
    /// Payload to send to the target function
    payload: Value,
}

/// Response with timing measurements
#[derive(Serialize)]
struct ProxyResponse {
    /// Time taken for the invocation in milliseconds
    invocation_time_ms: f64,
    /// Response from the target function
    response: Value,
}

/// Main handler for the proxy function
async fn function_handler(
    event: LambdaEvent<ProxyRequest>,
    lambda_client: &LambdaClient,
) -> Result<ProxyResponse, Error> {
    let request = event.payload;
    
    // Start timing
    let start = Instant::now();
    
    // Invoke target function
    let invoke_result = lambda_client
        .invoke()
        .function_name(&request.target)
        .payload(Blob::new(
            serde_json::to_vec(&request.payload)?
        ))
        .send()
        .await?;
    
    // Calculate elapsed time
    let elapsed = start.elapsed();
    let invocation_time_ms = elapsed.as_secs_f64() * 1000.0;
    
    // Parse response payload
    let response = if let Some(payload) = invoke_result.payload() {
        serde_json::from_slice(payload.as_ref())?
    } else {
        Value::Null
    };
    
    Ok(ProxyResponse {
        invocation_time_ms,
        response,
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing::init_default_subscriber();

    // Initialize AWS Lambda client
    let config = aws_config::load_from_env().await;
    let lambda_client = LambdaClient::new(&config);
    
    // Create a closure that clones the Lambda client
    let handler_func = move |event| {
        let client = lambda_client.clone();
        async move { function_handler(event, &client).await }
    };
    
    run(service_fn(handler_func)).await
}
