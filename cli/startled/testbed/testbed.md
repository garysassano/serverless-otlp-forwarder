# Serverless OTLP Forwarder Benchmark Results

This report contains performance benchmarks for different implementations of the serverless-otlp-forwarder across multiple languages and memory configurations.

## Overview

The benchmarks compare several metrics:

- **Cold Start Performance**: Initialization time, server duration, and total cold start times
- **Warm Start Performance**: Client latency, server processing time, and extension overhead
- **Resource Usage**: Memory consumption across different configurations

## Key Findings

- Rust implementations consistently show the best performance in cold starts
- Node.js performs well for warm invocations with minimal overhead
- Python demonstrates good balance between performance and ease of instrumentation

## Test Configuration

All tests were run with the following parameters:
- 100 invocations per function
- 10 concurrent requests
- AWS Lambda arm64 architecture
- Same payload size for all tests

## Implementations Tested

| Language | Instrumentation Types |
|----------|------------------------|
| Rust     | stdout, otel, adot, rotel |
| Node.js  | stdout, otel, adot, rotel, appsignals |
| Python   | stdout, otel, adot, rotel, appsignals | 