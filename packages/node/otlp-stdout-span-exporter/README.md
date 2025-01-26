# @dev7a/otlp-stdout-span-exporter

An OpenTelemetry span exporter that writes spans to stdout in OTLP format with Protobuf serialization and GZIP compression. Designed for use in serverless environments where writing to stdout is preferred over direct HTTP export.

>[!IMPORTANT]
>This package is part of the serverless-otlp-forwarder project and is designed for AWS Lambda environments. While it can be used in other contexts, it's primarily tested with AWS Lambda.

## Features

- Writes spans in OTLP format to stdout
- Uses Protobuf serialization for efficient encoding
- Applies GZIP compression with configurable level
- Supports automatic service name detection from environment variables
- Follows standard OTLP format for compatibility
- Allows custom headers via environment variables
- Zero external HTTP dependencies
- Lightweight and fast

## Installation

```bash
npm install @dev7a/otlp-stdout-span-exporter
```

## Usage

Basic usage with default configuration:

```typescript
import { OTLPStdoutSpanExporter } from '@dev7a/otlp-stdout-span-exporter';
import { NodeTracerProvider } from '@opentelemetry/sdk-trace-node';
import { BatchSpanProcessor } from '@opentelemetry/sdk-trace-base';

// Create a tracer provider
const provider = new NodeTracerProvider();

// Create and configure the exporter
const exporter = new OTLPStdoutSpanExporter();

// Use BatchSpanProcessor for efficient processing
provider.addSpanProcessor(new BatchSpanProcessor(exporter));

// Register the provider
provider.register();
```

Advanced usage with custom configuration:

```typescript
import { OTLPStdoutSpanExporter } from '@dev7a/otlp-stdout-span-exporter';
import { NodeTracerProvider } from '@opentelemetry/sdk-trace-node';
import { BatchSpanProcessor } from '@opentelemetry/sdk-trace-base';
import * as zlib from 'zlib';

// Create a tracer provider with custom configuration
const provider = new NodeTracerProvider({
  resource: new Resource({
    [SemanticResourceAttributes.SERVICE_NAME]: 'my-service',
  }),
});

// Configure the exporter with maximum compression
const exporter = new OTLPStdoutSpanExporter({
  gzipLevel: zlib.constants.Z_BEST_COMPRESSION, // Level 9
});

// Use BatchSpanProcessor with custom configuration
provider.addSpanProcessor(new BatchSpanProcessor(exporter, {
  maxQueueSize: 2048,
  scheduledDelayMillis: 1000,
}));

provider.register();
```

## Configuration

### Constructor Options

```typescript
interface OTLPStdoutSpanExporterOptions {
  gzipLevel?: number; // Optional, defaults to 6 (0-9, where 9 is maximum compression)
}
```

### Environment Variables

The exporter respects standard OpenTelemetry environment variables:

- `OTEL_SERVICE_NAME`: Service name for span source
- `AWS_LAMBDA_FUNCTION_NAME`: Fallback service name (if OTEL_SERVICE_NAME not set)
- `OTEL_EXPORTER_OTLP_HEADERS`: Global headers for all OTLP exporters
- `OTEL_EXPORTER_OTLP_TRACES_HEADERS`: Trace-specific headers (takes precedence over global headers)

Header format examples:
```bash
# Single header
export OTEL_EXPORTER_OTLP_HEADERS="api-key=secret123"

# Multiple headers
export OTEL_EXPORTER_OTLP_HEADERS="api-key=secret123,custom-header=value"

# Headers with special characters
export OTEL_EXPORTER_OTLP_HEADERS="authorization=Basic dXNlcjpwYXNzd29yZA=="
```

### Output Format

The exporter writes JSON objects to stdout with the following structure:

```json
{
  "__otel_otlp_stdout": "@dev7a/otlp-stdout-span-exporter@0.1.0",
  "source": "service-name",
  "endpoint": "http://localhost:4318/v1/traces",
  "method": "POST",
  "content-type": "application/x-protobuf",
  "content-encoding": "gzip",
  "headers": {
    "custom-header": "value"
  },
  "base64": true,
  "payload": "<base64-encoded-gzipped-protobuf-data>"
}
```

The `payload` field contains the base64-encoded, gzipped, Protobuf-serialized span data in OTLP format.

## Error Handling

The exporter handles various error conditions:
- Failed compression attempts
- Stdout write errors
- Invalid environment variables
- Malformed headers

All errors are properly propagated through the OpenTelemetry error handling system.

## License

MIT 