# lambda-otel-lite Examples

This directory contains examples demonstrating how to use `lambda-otel-lite` in AWS Lambda functions.

## Examples Overview

### 1. Hello World (`hello_world/`)

A minimal example showing basic usage of `lambda-otel-lite`. Perfect for getting started.

```python
from lambda_otel_lite import init_telemetry, traced_handler

def handler(event, context):
    tracer, provider = init_telemetry(name="hello-world-demo")
    
    with traced_handler(
        tracer=tracer,
        tracer_provider=provider,
        name="process_request",
        event=event,
        context=context,
    ) as span:
        name = event.get("body", {}).get("name", "World")
        return {"message": f"Hello, {name}!"}
```

This example demonstrates:
- Basic telemetry initialization
- Using the traced handler decorator
- Standard OTLP output format

### 2. Custom Processors (`custom_processors/`)

A more advanced example showing how to use custom span processors for telemetry enrichment.

```python
class SystemMetricsProcessor(SpanProcessor):
    """Processor that enriches spans with system metrics at start time."""
    def on_start(self, span, parent_context=None):
        cpu_percent = psutil.cpu_percent(interval=None)
        span.set_attribute("system.cpu.usage_percent", cpu_percent)
        # ... more system metrics ...

def handler(event, context):
    tracer, provider = init_telemetry(
        name="custom-processors-demo",
        span_processors=[
            SystemMetricsProcessor(),        # First add system metrics
            BatchSpanProcessor(              # Then export to console
                ConsoleSpanExporter()
            ),
        ],
    )
    # ... rest of handler code
```

This example demonstrates:
- Creating custom processors for span enrichment
- Chaining processors in the right order
- Adding system metrics to spans at start time

## Running the Examples

The examples can be run locally using `uv` (Python package manager):

```bash
# Run hello world example
uv run examples/hello_world/app.py

# Run custom processors example
uv run examples/custom_processors/app.py
```

Each example includes a `if __name__ == "__main__":` block that simulates a Lambda invocation locally.

### Understanding the Output

Both examples use the `OtlpStdoutSpanExporter`, which outputs spans in a format that will be processed by the serverless-otlp-forwarder. The custom processors example also includes a `DebugProcessor` that prints spans in human-readable JSON format:

```json
{
    "name": "process_request",
    "attributes": {
        "system.cpu.usage_percent": 15.3,
        "system.memory.used_percent": 68.7,
        "process.cpu.usage_percent": 0.0,
        "calculation.result": 499999500000
        // ... more attributes ...
    }
    // ... more span fields ...
}
```

## Setup

1. Install the package:
```bash
pip install lambda-otel-lite
```

2. Deploy either example:
   - Create a new Lambda function
   - Use Python 3.8 or later
   - Set the handler to `app.handler`
   - Upload the corresponding `app.py` file

3. Configure environment variables:
```
OTEL_SERVICE_NAME=your-service-name
``` 