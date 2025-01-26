# OTLP Stdout Kinesis Processor

AWS Lambda function that forwards Kinesis wrapped OTLP records to OpenTelemetry collectors.

## Overview

This Lambda function:
1. Receives Kinesis events containing otlp-stdout format records
2. Decodes and decompresses the data
3. Converts records to TelemetryData
4. Forwards the data to collectors in parallel

## Features

- Multiple collectors with different endpoints
- Custom headers and authentication
- Base64 encoded payloads
- Gzip compressed data
- OpenTelemetry instrumentation

## Prerequisites

- Rust 1.70 or later
- AWS SAM CLI
- AWS credentials configured
- Kinesis stream set up with appropriate permissions

## Building

```bash
# Build the Lambda function
sam build

# Run tests
cargo test
```

## Deployment

1. First time deployment:
```bash
sam deploy --guided
```

2. Subsequent deployments:
```bash
sam deploy
```

