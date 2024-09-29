# otlp-stdout-client

The `otlp-stdout-client` library is designed to export OpenTelemetry data to stdout in a formatted JSON structure, suitable for serverless environments like AWS Lambda.

## Features

This library supports both tracing and metrics functionality. By default, both features are enabled.

- `trace`: Enables tracing functionality (enabled by default)
- `metrics`: Enables metrics functionality (enabled by default)

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
cw_otlp_exporter = "0.1.0"
```

This will include both tracing and metrics functionality by default.

### Opting out of metrics

If you want to use only the tracing functionality and opt out of metrics, you can disable the default features and explicitly enable only the `trace` feature:

```toml
[dependencies]
cw_otlp_exporter = { version = "0.1.0", default-features = false, features = ["trace"] }
```

## Examples

### Using tracing

```rust
use cw_otlp_exporter::init_tracer_provider;
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
use cw_otlp_exporter::init_meter_provider;
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

## Configuration

The exporter can be configured using environment variables. For details on available configuration options, please refer to the library documentation.

## License

This project is licensed under

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)

at your option.