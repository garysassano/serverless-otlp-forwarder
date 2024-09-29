# Lambda OTLP Forwarder

![diagram](https://github.com/user-attachments/assets/ed033397-cb35-4ad8-a577-e1f5f62dd3c1)
The Lambda OTLP Forwarder is a serverless solution designed to forward OpenTelemetry Protocol (OTLP) formatted logs from AWS CloudWatch to an OTLP collector. This project consists of two main components:

1. A Rust-based AWS Lambda function that processes and forwards logs
2. A custom OpenTelemetry exporter library (`otlp-stdout-client`) that formats logs for the forwarder


## Features

- Processes CloudWatch logs and forwards OTLP-formatted data to a specified endpoint
- Option to subscribe to all logs in the AWS account
- Filters logs to process only OTLP-formatted records
- Supports both tracing and metrics data
- Configurable through environment variables and AWS Secrets Manager

## Architecture

The project deploys a Rust-based Lambda function using AWS SAM (Serverless Application Model). The function can be configured to subscribe to all logs in the AWS account, using a subscription filter that selects only OTLP-formatted records sent using the `otlp-stdout-client` library.

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

## Configuration

The Lambda function can be configured using the following environment variables:

- `RUST_LOG`: Sets the log level for the Rust application (e.g., `log_processor=info`)
- `OTEL_EXPORTER_OTLP_HEADERS`: Specifies headers for the OTLP exporter (stored in AWS Secrets Manager)

These can be set in the `template.yaml` file or through the AWS Console.

## Usage

### Sending logs

To send logs that can be processed by the forwarder, use the `otlp-stdout-client` library in your Rust applications. Add it to your `Cargo.toml`:

```
[dependencies]
otlp-stdout-client = "0.1.0"
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

### Viewing forwarded data

The forwarded data will be sent to the OTLP collector specified in your configuration. You can view this data in your observability platform of choice that supports OTLP.

## Development

### Lambda Function

The Lambda function code is located in the `forwarder` directory. To make changes:

1. Navigate to the `forwarder` directory
2. Make your changes in `src/main.rs`
3. Build and deploy using SAM CLI

### otlp-stdout-client

The `otlp-stdout-client` library is located in the `otlp-stdout-client` directory. To make changes:

1. Navigate to the `otlp-stdout-client/rust/client` directory
2. Make your changes
3. Update the version in `Cargo.toml` if necessary
4. Publish the new version to crates.io

## License

This project is licensed under the Apache License, Version 2.0. See the [LICENSE](LICENSE) file for details.
