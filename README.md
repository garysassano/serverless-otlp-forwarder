# Lambda OTLP Forwarder

![diagram](https://github.com/user-attachments/assets/f3b5f009-e4bf-4bb8-bbe2-b585235f20c7)

The Lambda OTLP Forwarder is a serverless solution designed to forward OpenTelemetry Protocol (OTLP) formatted logs from AWS CloudWatch to an OTLP collector. This project consists of two main components:

1. A Rust-based AWS Lambda function that processes and forwards logs
2. A custom OpenTelemetry exporter library ([otlp-stdout-client](otlp-stdout-client/rust/client)) that formats logs for the forwarder

## Motivation

The main motivation for this project is to provide a simple and cost-effective solution for serverless applications to send telemetry data to an OTLP collector. It eliminates the need for a sidecar agent, reducing resource consumption, cold starts, and costs. As a side benefit, if you're running an OTEL collector in your VPC, you don't need to expose it to the internet or connect all your lambda functions to your VPC. The transport for OLTP is CloudWatch logs, keeping all your telemetry data internal.

To minimize CloudWatch ingestioncosts, consider configuring your applications to use the HTTP/protobuf protocol instead of the default HTTP/JSON, as the payload is much smaller. Enabling gzip compression is also recommended. The resulting data will still be base64 encoded, but it will be much smaller than the JSON format.

## Quick start

1. Install the AWS SAM CLI
2. Clone this repository
3. Run `sam build && sam deploy --guided`
4. Configure your applications to use the `otlp-stdout-client` library

## Features

- Processes CloudWatch logs and forwards OTLP-formatted data to a specified endpoint
- Option to subscribe to all logs in the AWS account
- Filters logs to process only OTLP-formatted records
- Supports both tracing and metrics data
- Configurable through environment variables and AWS Secrets Manager

## Architecture

The project deploys a Rust-based Lambda function using AWS SAM (Serverless Application Model). The function is configured by default (with the RouteAllLogs parameter set to true) to subscribe to all logs in the AWS account, using a subscription filter that selects only OTLP-formatted records sent using the `otlp-stdout-client` library. The `otlp-stdout-client` library is used in your applications to format and send logs in a way that can be processed by the forwarder.

## Deployment

To deploy the Lambda OTLP Forwarder:

1. Ensure you have the AWS SAM CLI installed and configured.
2. Clone this repository.
3. Navigate to the project root directory.
4. Run the following command:

```
sam build && sam deploy --guided
```

Follow the prompts to configure your deployment.

### Multiaccount setup with AWS Organizations

By default, the stack will create a subscription filter for the entire AWS account. This means you only need one instance of the forwarder deployed, but you will need to do that for each account in your organization that hosts OTEL-enabled resources. It may be possible to create a stack set to deploy the forwarder to all accounts, but this has not been tested or fully evaluated.

## Configuration

The LogProcessor Lambda function can be configured using the following environment variables, set in the template.yaml file:

- `RUST_LOG`: Sets the log level for the Rust application (e.g., `log_processor=info`)
- `OTEL_EXPORTER_OTLP_HEADERS`: Specifies headers for the OTLP exporter (stored in AWS Secrets Manager under the name `keys/collector`). Typically, the value of the secret is a string with the following format: `Authorization=secret-api-key`, or `x-some-header=some-value`. Please refer to your observability platform documentation for the correct headers to use.

Since the LogProcessor function is also instrumented with OpenTelemetry, you can also set the following environment variables to configure the OTLP exporter for this function:

- `OTEL_EXPORTER_OTLP_ENDPOINT`: Specifies the endpoint for the OTLP exporter. This can be a hostname or IP address.
- `OTEL_EXPORTER_OTLP_PROTOCOL`: Specifies the protocol for the OTLP exporter. This can be `http/protobuf` or `http/json`.
- `OTEL_EXPORTER_OTLP_COMPRESSION`: Specifies the compression for the OTLP exporter. This can be `gzip` or `none`.

Please note that the variables above are only used for the LogProcessor function, and not your applications. You will need to configure your applications separately, depending on the platform you are sending the telemetry data to and the features that you want to use.

## Configuring your own applications

At this stage, only Rust lambda functions are supported, but the plan is to add support for other runtimes in the future. All you need to do is to add the `otlp-stdout-client` library to your project and initialize the tracer provider with the correct configuration.

> [!NOTE]
> The `otlp-stdout-client` library currently includes a local implementation of the `LambdaResourceDetector` from the `opentelemetry-aws` crate. This is a temporary measure while waiting for the `opentelemetry-aws` crate to be updated to version 0.13.0. Once the update is available, this local implementation will be removed in favor of the official crate dependency.

### Sending logs

To send logs that can be processed by the forwarder, use the [otlp-stdout-client](https://crates.io/crates/otlp-stdout-client) library in your Rust applications. Add it to your `Cargo.toml`:

```toml
[dependencies]
otlp-stdout-client = "0.1.1"
```

Then, use it in your code:

```rust
use otlp-stdout-client::init_tracer_provider;
use opentelemetry::trace::TracerProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tracer_provider = init_tracer_provider()?;
    let tracer = tracer_provider.tracer("my-service");
    
    // Use the tracer for instrumenting your code
    // ...

    Ok(())
}
```

## Viewing forwarded data

The forwarded data will be sent to the OTLP collector specified in your configuration. You can view this data in your observability platform of choice that supports OpenTelemetry and OTLP.


## Development

### Lambda function

The Lambda function code is located in the `forwarder` directory. To make changes:

1. Navigate to the `forwarder` directory
2. Make your changes in `src/main.rs`
3. Build and deploy using SAM CLI

### otlp-stdout-client

The `otlp-stdout-client` library is located in the [otlp-stdout-client/rust/client](otlp-stdout-client/rust/client) directory. To make changes:

1. Navigate to the `otlp-stdout-client/rust/client` directory
2. Make your changes
3. Update the version in `Cargo.toml` if necessary

## License

This project is licensed under the Apache License, Version 2.0. See the [LICENSE](LICENSE) file for details.