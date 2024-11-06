# lambda-otel-utils

`lambda-otel-utils` is a Rust library that simplifies the integration of OpenTelemetry tracing with AWS Lambda functions. It provides utilities for setting up and configuring OpenTelemetry in serverless environments, making it easier to implement distributed tracing in your Lambda-based applications.

## Features

- Easy setup of OpenTelemetry TracerProvider for AWS Lambda
- Customizable tracing configuration
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

Here's a comprehensive example of how to use `lambda-otel-utils` to set up tracing in a Lambda function:

```rust
use lambda_otel_utils::{HttpTracerProviderBuilder, HttpPropagationLayer};
use lambda_runtime::{service_fn, Error, LambdaEvent, Runtime};
use lambda_runtime::layers::{OpenTelemetryLayer, OpenTelemetryFaasTrigger};
use aws_lambda_events::event::apigw::ApiGatewayProxyRequest;
use serde_json::Value;

async fn function_handler(event: LambdaEvent<ApiGatewayProxyRequest>) -> Result<Value, Error> {
    // Your Lambda function logic here
    Ok(serde_json::json!({"message": "Hello, World!"}))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let tracer_provider = HttpTracerProviderBuilder::default()
        .with_stdout_client()
        .with_tracer_name("my-lambda-function")
        .with_default_text_map_propagator()
        .enable_global(true)
        .enable_fmt_layer(true)
        .build()?;

    let service = service_fn(function_handler);

    Runtime::new(service)
        .layer(
            OpenTelemetryLayer::new(move || {
                tracer_provider.force_flush();
            })
            .with_trigger(OpenTelemetryFaasTrigger::Http),
        )
        .layer(HttpPropagationLayer)
        .run()
        .await
}
```

This example demonstrates how to:
1. Set up the `HttpTracerProviderBuilder` with various options.
2. Create a Lambda runtime with a service function.
3. Add the `OpenTelemetryLayer` with a flush callback.
4. Include the `HttpPropagationLayer` for context propagation.

## Main Components

### HttpTracerProviderBuilder

The `HttpTracerProviderBuilder` allows you to configure and build a custom TracerProvider tailored for Lambda environments. It supports various options such as:

- Custom HTTP clients for exporting traces
- Enabling/disabling logging layers
- Setting custom tracer names
- Configuring propagators and ID generators
- Choosing between simple and batch exporters

It also support the use of the `otlp-stdout-client` to export the traces to a stdout-like sink, for easy integration with AWS Lambda and forwarding to collectors.

### HttpPropagationLayer

The `HttpPropagationLayer` is used for propagating context across HTTP boundaries, ensuring that trace context is maintained throughout your distributed system.

## Environment Variables

The crate respects standard OpenTelemetry environment variables for configuration, but the `HttpTracerProviderBuilder` is restricted to only `http/protobuf` and `http/json` protocols for the `OTEL_EXPORTER_OTLP_PROTOCOL` variable.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under MIT. See the LICENSE file for details.
