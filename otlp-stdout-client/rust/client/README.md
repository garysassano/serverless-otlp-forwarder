# otlp-stdout-client

The `otlp-stdout-client` library is designed to export OpenTelemetry data to stdout in a formatted JSON structure, suitable for serverless environments like AWS Lambda. It is specifically designed to work with the lambda-otlp-forwarder, which processes these stdout logs and forwards them to an OTLP collector.

## Features

This library supports both tracing and metrics functionality. By default, both features are enabled.

- `trace`: Enables tracing functionality (enabled by default)
- `metrics`: Enables metrics functionality (enabled by default)

## Important Note

> [!NOTE]
> The `otlp-stdout-client` library currently includes a local implementation of the `LambdaResourceDetector` from the `opentelemetry-aws` crate. This is a temporary measure while waiting for the `opentelemetry-aws` crate to be updated to version 0.13.0. Once the update is available, this local implementation will be removed in favor of the official crate dependency.

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
otlp-stdout-client = "0.1.1"
```

This will include both tracing and metrics functionality by default.

### Opting out of metrics

If you want to use only the tracing functionality and opt out of metrics, you can disable the default features and explicitly enable only the `trace` feature:

```toml
[dependencies]
otlp-stdout-client = { version = "0.1.1", default-features = false, features = ["trace"] }
```

## Examples

### Using tracing

```rust
use otlp_stdout_client::init_tracer_provider;
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

### Using metrics (when enabled)

```rust
use otlp_stdout_client::init_meter_provider;
use opentelemetry::metrics::MeterProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let meter_provider = init_meter_provider()?;
    let meter = meter_provider.meter("my-service");
    
    // Use the meter for creating instruments and recording metrics
    // ...

    Ok(())
}
```

## Record Structure

The `otlp-stdout-client` writes OTLP data to stdout in a JSON format that can be easily parsed by the lambda-otlp-forwarder. Each log record includes:

- `_otel`: A version identifier for the otlp-stdout-client
- `content-type`: The content type of the payload (JSON or Protobuf)
- `endpoint`: The configured OTLP endpoint
- `method`: The HTTP method (always "POST")
- `payload`: The OTLP data (may be base64 encoded if compressed)
- `headers`: Any configured headers
- `content-encoding`: The compression method (if used)
- `base64`: A boolean indicating if the payload is base64 encoded

## Configuration

The exporter can be configured using standard OpenTelemetry environment variables:

- `OTEL_EXPORTER_OTLP_ENDPOINT`: Sets the endpoint for the OTLP exporter
- `OTEL_EXPORTER_OTLP_PROTOCOL`: Specifies the protocol (http/protobuf or http/json)
- `OTEL_EXPORTER_OTLP_HEADERS`: Sets additional headers for the OTLP exporter
- `OTEL_EXPORTER_OTLP_COMPRESSION`: Specifies the compression algorithm (gzip or none)
- `OTEL_SERVICE_NAME`: Sets the service name for the Lambda function

For detailed information on these variables and their effects, please refer to the opentelemetry documentation.

## License

This project is licensed under

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)

at your option.