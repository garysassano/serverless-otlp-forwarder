# Serverless OTLP Forwarder Packages

This directory contains various packages for OpenTelemetry integration with serverless environments across multiple programming languages.

## Active Packages

### OTLP Stdout Span Exporter

A span exporter that writes OpenTelemetry spans to stdout, using a custom serialization format that embeds the spans serialized as OTLP protobuf in the `payload` field. Outputting telemetry data in this format directly to stdout makes the library easily usable in network constrained environments, or in environments that are particularly sensitive to the overhead of HTTP connections, such as AWS Lambda.

**Features:**
- Uses OTLP Protobuf serialization for efficient encoding
- Applies GZIP compression with configurable levels
- Detects service name from environment variables
- Supports custom headers via environment variables
- Zero external HTTP dependencies
- Lightweight and fast

**Available Implementations:**

| Language | Package | Source |
|----------|---------|--------|
| Node.js | [![npm](https://img.shields.io/npm/v/@dev7a/otlp-stdout-span-exporter.svg)](https://www.npmjs.com/package/@dev7a/otlp-stdout-span-exporter) | [Source](https://github.com/dev7a/serverless-otlp-forwarder/tree/main/packages/node/otlp-stdout-span-exporter) |
| Python | [![PyPI](https://img.shields.io/pypi/v/otlp-stdout-span-exporter.svg)](https://pypi.org/project/otlp-stdout-span-exporter/) | [Source](https://github.com/dev7a/serverless-otlp-forwarder/tree/main/packages/python/otlp-stdout-span-exporter) |
| Rust | [![Crates.io](https://img.shields.io/crates/v/otlp-stdout-span-exporter.svg)](https://crates.io/crates/otlp-stdout-span-exporter) | [Source](https://github.com/dev7a/serverless-otlp-forwarder/tree/main/packages/rust/otlp-stdout-span-exporter) |

### Lambda OTel Lite

A lightweight, efficient OpenTelemetry implementation specifically designed for AWS Lambda environments. It features a custom span processor and internal extension mechanism that optimizes telemetry collection for Lambda's unique execution model.

**Features:**
- Flexible Processing Modes: Support for synchronous, asynchronous, and custom export strategies
- Automatic Resource Detection: Automatic extraction of Lambda environment attributes
- Lambda Extension Integration: Built-in extension for efficient telemetry export
- Efficient Memory Usage: Fixed-size queue to prevent memory growth
- AWS Event Support: Automatic extraction of attributes from common AWS event types
- Flexible Context Propagation: Support for W3C Trace Context

**Available Implementations:**

| Language | Package | Source |
|----------|---------|--------|
| Node.js | [![npm](https://img.shields.io/npm/v/@dev7a/lambda-otel-lite.svg)](https://www.npmjs.com/package/@dev7a/lambda-otel-lite) | [Source](https://github.com/dev7a/serverless-otlp-forwarder/tree/main/packages/node/lambda-otel-lite) |
| Python | [![PyPI](https://img.shields.io/pypi/v/lambda-otel-lite.svg)](https://pypi.org/project/lambda-otel-lite/) | [Source](https://github.com/dev7a/serverless-otlp-forwarder/tree/main/packages/python/lambda-otel-lite) |
| Rust | [![Crates.io](https://img.shields.io/crates/v/lambda-otel-lite.svg)](https://crates.io/crates/lambda-otel-lite) | [Source](https://github.com/dev7a/serverless-otlp-forwarder/tree/main/packages/rust/lambda-otel-lite) |

### OTLP SigV4 Client (Rust only)

[![Crates.io](https://img.shields.io/crates/v/otlp-sigv4-client.svg)](https://crates.io/crates/otlp-sigv4-client)

A SigV4-compatible HTTP client wrapper for OpenTelemetry OTLP exporters, enabling AWS authentication for sending telemetry data to the CloudWatch OTLP endpoint.

**Features:**
- AWS SigV4 authentication for OpenTelemetry OTLP exporters
- Support for both reqwest and hyper HTTP clients
- Automatic AWS region detection from environment
- Configurable AWS service name
- Compatible with AWS credentials provider chain

**Links:** [Source](https://github.com/dev7a/serverless-otlp-forwarder/tree/main/packages/rust/otlp-sigv4-client) | [crates.io](https://crates.io/crates/otlp-sigv4-client)

## Deprecated Packages

The following packages are deprecated and should not be used for new projects:

### Node.js

- **exporter**: Legacy exporter package, replaced by otlp-stdout-span-exporter

### Python

- **adapter**: Legacy adapter package, replaced by lambda-otel-lite

### Rust

- **lambda-otel-utils**: Utility functions for OpenTelemetry in AWS Lambda (superseded by lambda-otel-lite)
- **otlp-stdout-client**: Legacy stdout client, replaced by otlp-stdout-span-exporter
- **lambda-lw-http-router**: Lightweight HTTP router for AWS Lambda (temporarily in the backburner) 