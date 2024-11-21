# lambda-otel-utils

`lambda-otel-utils` is a Rust library that simplifies the integration of OpenTelemetry tracing and metrics with AWS Lambda functions. It provides utilities for setting up and configuring OpenTelemetry in serverless environments, making it easier to implement distributed tracing and metrics collection in your Lambda-based applications.

## Features

- Easy setup of OpenTelemetry TracerProvider and MeterProvider for AWS Lambda
- Customizable tracing and metrics configuration
- Support for context propagation in HTTP requests
- Integration with various OpenTelemetry exporters
- Compatible with the `lambda_runtime` crate

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
lambda-otel-utils = "0.1.0" 
```

## Usage

There are two ways to use OpenTelemetry with this library:

### 1. Using the HTTP OpenTelemetry Layer

Here's an example using our custom HTTP OpenTelemetry layer, which provides enhanced HTTP-specific tracing:

```rust
use lambda_otel_utils::{
    HttpTracerProviderBuilder,
    http_otel_layer::HttpOtelLayer
};
use lambda_runtime::{service_fn, Error, LambdaEvent};
use aws_lambda_events::event::apigw::ApiGatewayProxyRequest;
use serde_json::Value;
use lambda_runtime::tower::ServiceBuilder;

async fn function_handler(event: LambdaEvent<ApiGatewayProxyRequest>) -> Result<Value, Error> {
    // Your Lambda function logic here
    Ok(serde_json::json!({
        "statusCode": 200,
        "body": json!({ "message": "Hello, World!" })
    }))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Set up tracing
    let tracer_provider = HttpTracerProviderBuilder::default()
        .with_stdout_client()
        .with_tracer_name("my-lambda-function")
        .with_default_text_map_propagator()
        .enable_global(true)
        .enable_fmt_layer(true)
        .build()?;

    let provider_clone = tracer_provider.clone();

    // Build the Lambda service with the HTTP OpenTelemetry layer
    let func = ServiceBuilder::new()
        .layer(HttpOtelLayer::new(move || {
            provider_clone.force_flush();
        }))
        .service(service_fn(function_handler));

    lambda_runtime::run(func).await?;
    Ok(())
}
```

This implementation provides:
- Automatic HTTP context propagation
- HTTP-specific span attributes (method, route, status code)
- Cold start tracking
- Proper span lifecycle management
- Automatic error tracking and status code recording

### 2. Using AWS Lambda Runtime OpenTelemetry Layer

For more general use cases, you can use the AWS Lambda Runtime OpenTelemetry layer:

```rust
use lambda_otel_utils::{HttpTracerProviderBuilder, HttpMeterProviderBuilder, HttpPropagationLayer};
use lambda_runtime::{service_fn, Error, LambdaEvent, Runtime};
use lambda_runtime::layers::{OpenTelemetryLayer, OpenTelemetryFaasTrigger};
use aws_lambda_events::event::apigw::ApiGatewayProxyRequest;
use serde_json::Value;
use std::time::Duration;

async fn function_handler(event: LambdaEvent<ApiGatewayProxyRequest>) -> Result<Value, Error> {
    // Your Lambda function logic here
    Ok(serde_json::json!({"message": "Hello, World!"}))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Set up tracing
    let tracer_provider = HttpTracerProviderBuilder::default()
        .with_stdout_client()
        .with_tracer_name("my-lambda-function")
        .with_default_text_map_propagator()
        .enable_global(true)
        .enable_fmt_layer(true)
        .build()?;

    // Set up metrics
    let meter_provider = HttpMeterProviderBuilder::default()
        .with_stdout_client()
        .with_meter_name("my-lambda-function")
        .with_export_interval(Duration::from_secs(60))
        .build()?;

    let service = service_fn(function_handler);

    Runtime::new(service)
        .layer(
            OpenTelemetryLayer::new(move || {
                tracer_provider.force_flush();
                meter_provider.force_flush();
            })
            .with_trigger(OpenTelemetryFaasTrigger::Http),
        )
        .layer(HttpPropagationLayer)
        .run()
        .await
}
```

This example demonstrates how to:
1. Set up both the `HttpTracerProviderBuilder` and `HttpMeterProviderBuilder` with various options
2. Create a Lambda runtime with a service function
3. Add the `OpenTelemetryLayer` with a flush callback for both tracing and metrics
4. Include the `HttpPropagationLayer` for context propagation

## Main Components

### HttpTracerProviderBuilder

The `HttpTracerProviderBuilder` allows you to configure and build a custom TracerProvider tailored for Lambda environments. It supports various options such as:

- Custom HTTP clients for exporting traces
- Enabling/disabling logging layers
- Setting custom tracer names
- Configuring propagators and ID generators
- Choosing between simple and batch exporters

It also support the use of the `otlp-stdout-client` to export the traces to a stdout-like sink, for easy integration with AWS Lambda and forwarding to collectors.

### HttpMeterProviderBuilder

The `HttpMeterProviderBuilder` allows you to configure and build a custom MeterProvider tailored for Lambda environments. It supports various options such as:

- Custom HTTP clients for exporting metrics
- Setting custom meter names
- Configuring export intervals and timeouts
- Integration with Lambda resource attributes

It also supports the use of the `otlp-stdout-client` to export the metrics to a stdout-like sink, for easy integration with AWS Lambda and forwarding to collectors.

### HttpPropagationLayer

The `HttpPropagationLayer` is used for propagating context across HTTP boundaries, ensuring that trace context is maintained throughout your distributed system.

## Environment Variables

The crate respects standard OpenTelemetry environment variables for configuration, but the `HttpTracerProviderBuilder` is restricted to only `http/protobuf` and `http/json` protocols for the `OTEL_EXPORTER_OTLP_PROTOCOL` variable.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under MIT. See the LICENSE file for details.
