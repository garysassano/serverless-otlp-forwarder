# lambda-otel-lite Examples

This directory contains examples demonstrating how to use `lambda-otel-lite` in AWS Lambda functions.

## Examples Overview

### 1. Hello World (`hello_world/app.js`)

A minimal example showing basic usage of `lambda-otel-lite`. Perfect for getting started.

```javascript
const { SpanKind } = require('@opentelemetry/api');
const { initTelemetry, tracedHandler } = require('@dev7a/lambda-otel-lite');

// Initialize telemetry once at module load
const { tracer, provider } = initTelemetry('hello-world');

exports.handler = async (event, context) => {
  return tracedHandler(
    {
      tracer,
      provider,
      name: 'hello_world',
      event,
      context,
      kind: SpanKind.SERVER,
    },
    async (span) => {
      span.setAttribute('greeting.name', 'World');
      return {
        statusCode: 200,
        body: JSON.stringify({ message: 'Hello World!' }),
      };
    }
  );
};
```

This example demonstrates:
- Basic telemetry initialization
- Using the traced handler wrapper
- Standard OTLP output format

### 2. Custom Processors (`custom_processors/app.js`)

A more advanced example showing how to use custom span processors for telemetry enrichment.

```javascript
const { SpanKind } = require('@opentelemetry/api');
const { BatchSpanProcessor } = require('@opentelemetry/sdk-trace-base');
const { OTLPStdoutSpanExporter } = require('@dev7a/otlp-stdout-span-exporter');
const { initTelemetry, tracedHandler } = require('@dev7a/lambda-otel-lite');

// Initialize with custom processors
const { tracer, provider } = initTelemetry('custom-processors-demo', {
  spanProcessors: [
    new SystemMetricsProcessor(),  // First add system metrics
    new DebugProcessor(),          // Then print all spans as json
    new BatchSpanProcessor(        // Then export in OTLP format
      new OTLPStdoutSpanExporter({
        gzipLevel: parseInt(process.env.OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL || '6', 10)
      })
    )
  ]
});

exports.handler = async (event, context) => {
  return tracedHandler(
    {
      tracer,
      provider,
      name: 'process_request',
      event,
      context,
      kind: SpanKind.SERVER,
    },
    async (span) => {
      // Your handler code here
      span.setAttribute('custom.attribute', 'value');
      return {
        statusCode: 200,
        body: JSON.stringify({ success: true }),
      };
    }
  );
};
```

This example demonstrates:
- Creating custom processors for span enrichment
- Using compression with the OTLP exporter
- Adding system metrics to spans at start time

## Setup

1. Install the package:
```bash
npm install @dev7a/lambda-otel-lite
```

2. Deploy either example:
   - Create a new Lambda function
   - Use Node.js 18.x or later
   - Set the handler to `app.handler`
   - Upload the corresponding `app.js` file

3. Configure environment variables:
```
OTEL_SERVICE_NAME=your-service-name
``` 