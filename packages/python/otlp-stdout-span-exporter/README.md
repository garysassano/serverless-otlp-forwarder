# OpenTelemetry Stdout Span Exporter

A Python span exporter that writes OpenTelemetry spans to stdout in OTLP format. Part of the [serverless-otlp-forwarder](https://github.com/dev7a/serverless-otlp-forwarder) project.

This exporter is particularly useful in serverless environments like AWS Lambda where writing to stdout is a common pattern for exporting telemetry data.

## Features

- Uses OTLP Protobuf serialization for efficient encoding
- Applies GZIP compression with configurable levels
- Detects service name from environment variables
- Supports custom headers via environment variables
- Consistent JSON output format

## Installation

```bash
pip install otlp-stdout-span-exporter
```

## Usage

Basic usage:

```python
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from otlp_stdout_span_exporter import OTLPStdoutSpanExporter

# Create and set the tracer provider
provider = TracerProvider()
trace.set_tracer_provider(provider)

# Create and register the exporter with optional GZIP compression level
exporter = OTLPStdoutSpanExporter(gzip_level=6)
provider.add_span_processor(BatchSpanProcessor(exporter))

# Your instrumentation code here
tracer = trace.get_tracer(__name__)
with tracer.start_as_current_span("my-operation") as span:
    span.set_attribute("my.attribute", "value")
```

For a more comprehensive example including nested spans, custom attributes, events, and environment variable configuration:

```bash
# Clone the repository
git clone https://github.com/dev7a/serverless-otlp-forwarder
cd serverless-otlp-forwarder/packages/python/otlp-stdout-span-exporter

# Install the package
pip install -e .

# Run the example
python examples/basic_usage.py
```

## Environment Variables

The exporter respects the following environment variables:

- `OTEL_SERVICE_NAME`: Service name to use in output
- `AWS_LAMBDA_FUNCTION_NAME`: Fallback service name (if `OTEL_SERVICE_NAME` not set)
- `OTEL_EXPORTER_OTLP_HEADERS`: Global headers for OTLP export
- `OTEL_EXPORTER_OTLP_TRACES_HEADERS`: Trace-specific headers (takes precedence)

## Output Format

The exporter writes each batch of spans as a JSON object to stdout:

```json
{
  "__otel_otlp_stdout": "0.1.0",
  "source": "my-service",
  "endpoint": "http://localhost:4318/v1/traces",
  "method": "POST",
  "content-type": "application/x-protobuf",
  "content-encoding": "gzip",
  "headers": {
    "api-key": "secret123",
    "custom-header": "value"
  },
  "payload": "<base64-encoded-gzipped-protobuf>",
  "base64": true
}
```

## Development

1. Create a virtual environment:
```bash
uv venv && source .venv/bin/activate
```

2. Install development dependencies:
```bash
uv pip install -e ".[dev]"
```

3. Run tests:
```bash
pytest
```

4. Run linting:
```bash
ruff check .
ruff format .
```

## License

Apache License 2.0

## See Also

- [serverless-otlp-forwarder](https://github.com/dev7a/serverless-otlp-forwarder) - The main project repository
- [TypeScript Span Exporter](https://github.com/dev7a/serverless-otlp-forwarder/tree/main/packages/node/otlp-stdout-span-exporter) - The TypeScript version of this exporter
