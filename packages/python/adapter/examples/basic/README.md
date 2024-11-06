# Basic OpenTelemetry Example

This example demonstrates how to use the `otlp-stdout-adapter` to instrument a Python application with OpenTelemetry using the StdoutAdapter.

## Features Demonstrated

- Setting up the OTLP exporter with stdout adapter
- Creating and configuring a tracer provider
- Creating spans and adding attributes
- Using context propagation with nested spans
- Simulating a batch processing workflow

## Running the Example

1. Install the required packages:

```bash
pip install otlp-stdout-adapter opentelemetry-api opentelemetry-sdk opentelemetry-exporter-otlp-proto-http requests
```

# Or install from requirements.txt:
```bash
pip install -r requirements.txt
```

2. Run the example:

```bash
python app.py
```

## Expected Output

The example will output multiple JSON records to stdout, one for each batch of spans. The spans created include:

- A root span named "batch_process"
- Multiple "process_item" spans as children
- "validate_item" spans nested under each "process_item"
- "transform_item" spans for processing

Each record will contain:
- A marker identifying the adapter (`__otel_otlp_stdout`)
- Source service name
- Endpoint information
- Base64-encoded protobuf payload containing the spans

Example output:

```json
{
    "__otel_otlp_stdout": "otlp-stdout-adapter@0.1.0",
    "source": "example-service",
    "endpoint": "http://localhost:4318/v1/traces",
    "method": "POST",
    "content-type": "application/x-protobuf",
    "payload": "<base64-encoded-content>",
    "base64": true,
    "content-encoding": "gzip"
}
```

## Environment Variables

You can customize the behavior using standard OpenTelemetry environment variables:

```bash
export OTEL_SERVICE_NAME="example-service"
export OTEL_EXPORTER_OTLP_ENDPOINT="http://localhost:4318/v1/traces"
export OTEL_EXPORTER_OTLP_COMPRESSION="gzip"  # Optional: Enable compression
```

## Code Walkthrough

The example demonstrates:

1. **Setup**: Configuring the OpenTelemetry SDK with the stdout adapter
2. **Resource Attribution**: Using Lambda resource attributes
3. **Span Creation**: Creating parent and child spans
4. **Context Propagation**: Maintaining the trace context across operations
5. **Attributes**: Adding custom attributes to spans
6. **Batch Processing**: Simulating a real-world batch processing scenario

## Note on Protobuf Format

While the OpenTelemetry specification supports both JSON and Protobuf over HTTP, the Python SDK currently only supports Protobuf (see [opentelemetry-python#1003](https://github.com/open-telemetry/opentelemetry-python/issues/1003)). All exports will use application/x-protobuf content-type.