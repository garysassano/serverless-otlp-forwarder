# Lambda OpenTelemetry Benchmark

A CLI tool to benchmark different OpenTelemetry instrumentation approaches for AWS Lambda functions. It helps you understand the performance impact of various instrumentation methods by measuring cold starts, warm starts, and execution times.

We developed this benchmark primarily to validate our OTLP-to-stdout implementation against other approaches. While we've tried to make the tests as fair as possible, benchmarking is hard, and results can vary based on many factors. Don't take the results as absolute truth - they're meant to give you a general idea of the performance characteristics of each approach. Your mileage may vary depending on your specific use case, region, and other factors.

## Quick Start

1. Deploy the benchmark stack:
```bash
# Clone the repo
git clone https://github.com/your-org/serverless-otlp-forwarder
cd serverless-otlp-forwarder/benchmark

# Deploy with SAM
sam build
sam deploy --guided
```

During the guided deployment, you'll be asked to configure telemetry collection for each instrumentation method:
- For OpenTelemetry SDK, you'll set up the OTLP endpoint and related parameters
- For ADOT, you'll need to have prepared your `functions/config/collector.yaml` based on the example file [functions/config/collector.example.yaml](functions/config/collector.example.yaml)
- For AppSignals, you'll need to have enabled it in your AWS account


2. Run some benchmarks:

The following examples assume that you have deployed the stack with the name `benchmark`.
```bash
# Test a single function
cargo run --release -- function benchmark-basic-python-stdout -m 128 -c 5 -r 10 -d /tmp/test

Starting benchmark for: benchmark-basic-python-stdout
Configuration:
  Memory: 128 MB
  Runtime: python3.13
  Architecture: arm64
  Concurrency: 5
  Rounds: 10

Telemetry:
  OpenTelemetry is not configured (OTEL_EXPORTER_OTLP_ENDPOINT and OTEL_SERVICE_NAME are required)

Updating function configuration...
✓ Function configuration updated

Collecting server metrics...
✓ Server metrics collected

Collecting client metrics...
✓ Client metrics collected

🥶 Cold Start Metrics (5 invocations) | Memory Size: 128 MB

Metric                   Min          P50          P95          Max         Mean
---------------------------------------------------------------------------------------
Init Duration     732.626 ms    754.639 ms    794.957 ms    794.957 ms    762.785 ms
Server Duration    42.584 ms     59.358 ms     80.916 ms     80.916 ms     63.366 ms
Net Duration       20.307 ms     36.635 ms     75.046 ms     75.046 ms     42.481 ms
Billed Duration    43.000 ms     60.000 ms     81.000 ms     81.000 ms     63.800 ms
Memory Used          68.0 MB       68.0 MB       68.0 MB       68.0 MB       68.0 MB

🔥 Warm Start Metrics (50 invocations) | Memory Size: 128 MB

Metric                   Min          P50          P95          Max         Mean
---------------------------------------------------------------------------------------
Server Duration     5.531 ms     33.242 ms     52.394 ms     59.496 ms     30.306 ms
Net Duration        2.357 ms     14.570 ms     37.241 ms     39.320 ms     16.317 ms
Billed Duration     6.000 ms     34.000 ms     53.000 ms     60.000 ms     30.800 ms
Memory Used          68.0 MB       68.0 MB       69.0 MB       69.0 MB       68.2 MB

⏱️ Client Metrics (50 invocations) | Memory Size: 128 MB

Metric                   Min          P50          P95          Max         Mean
---------------------------------------------------------------------------------------
Client Duration    37.783 ms     78.459 ms     98.839 ms    110.333 ms     69.658 ms

Report saved to: /tmp/test/128mb/benchmark-basic-python-stdout.json

Restoring function configuration...
✓ Function configuration restored

```
You can also test all functions in a stack, or only those matching a substring in their names.
```bash
# Test all functions in a stack matching the substring "basic-node" in their names
cargo run --release -- stack benchmark -s basic-node -m 128 -c 5 -r 10 -d /tmp/test-stack
```

Finally, you can run a batch of tests from a config file:
```bash
# Run a batch of tests from batch config file
cargo run --release -- batch -c benchmark-config.yaml -d /tmp/test-batch
```

3. Generate a browsable report:
```bash
cargo run --release -- report -d /tmp/test-batch -d /tmp/test-batch -o /tmp/test-report
```

4. Or generate a set of Jekyll "just the docs" theme pages:

```bash
cargo run --release -- jekyll -c benchmark-config.yaml -d /tmp/test-batch -o /tmp/test-jekyll
```

## What's Being Tested?

We're comparing four different instrumentation approaches:
- Manual instrumentation with OTLP-to-stdout (our reference implementation)
  - Uses our lightweight libraries ([`@dev7a/lambda-otel-lite`](https://www.npmjs.com/package/@dev7a/lambda-otel-lite) for Node.js and [`lambda-otel-lite`](https://pypi.org/project/lambda-otel-lite/) for Python)
  - Minimal dependencies and optimized for Lambda environment
  - Direct stdout output without buffering or complex processing
  - Writes encoded OTLP traces to CloudWatch logs
  - Requires the serverless-otlp-forwarder (main project in this repo) to be deployed
- [OpenTelemetry Lambda Layers](https://github.com/open-telemetry/opentelemetry-lambda) (standard auto-instrumentation)
  - Configuration is passed through stack parameters during `sam deploy --guided`
  - Only the language-specific layers are used, not the collector
- [AWS Distro for OpenTelemetry (ADOT)](https://github.com/aws-observability/aws-otel-lambda)
  - Uses both the OpenTelemetry Layer and a stripped-down collector
  - Requires configuration via `/benchmark/functions/config/collector.yaml`
  - Use `/benchmark/functions/config/collector.example.yaml` as a template to configure your own collector
- [AWS AppSignals Lambda Layer](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/CloudWatch-Application-Signals-Enable-Lambda.html#Enable-Lambda-Manually)
  - Requires AWS Application Signals to be enabled in your account
  - Will send traces to CloudWatch Application Signals

Each approach is tested across different scenarios:
- Basic operations (span creation overhead)
- HTTP client calls
- AWS SDK operations
- Stream processing

> [!NOTE]
> For all tests, but the Application Signals test, X-Ray Tracing is disabled, and the OTEL_LAMBDA_DISABLE_AWS_CONTEXT_PROPAGATION is set to true, in order to prevent the creation of a phantom root span.

## Project Structure

The benchmark functions are organized by runtime and instrumentation type:
```
functions/
├── nodejs/
│   ├── auto/        # Zero-code instrumentation (OTel SDK, ADOT, AppSignals)
│   │   ├── basic/
│   │   ├── http/
│   │   ├── aws/
│   │   └── stream/
│   └── manual/      # Manual OTLP-to-stdout implementation for Node.js
│       ├── basic/
│       ├── http/
│       ├── aws/
│       └── stream/
└── python/
    ├── auto/        # Zero-code instrumentation (OTel SDK, ADOT, AppSignals)
    │   ├── basic/
    │   ├── http/
    │   ├── aws/
    │   └── stream/
    └── manual/      # Manual OTLP-to-stdout implementation for Python
        ├── basic/
        ├── http/
        ├── aws/
        └── stream/
```

The `manual/` directories contain our reference implementation using OTLP-to-stdout, which provides a baseline for comparison.

The `auto/` directories contain identical code that can be instrumented using the different zero-code approaches (OTel Layer, ADOT, AppSignals) just by changing the Lambda layers and environment variables. This ensures we're comparing the instrumentation overhead rather than different implementations.


## CLI Commands

### Test a Single Function
```
Usage: otel-benchmark-cli function [OPTIONS] --dir <OUTPUT_DIR> <FUNCTION_NAME>

Arguments:
  <FUNCTION_NAME>  Lambda function ARN or name

Options:
  -m, --memory <MEMORY>              Memory size in MB
  -c, --concurrent <CONCURRENT>      Number of concurrent invocations [default: 1]
  -r, --rounds <ROUNDS>              Number of rounds for warm starts [default: 1]
  -d, --dir <OUTPUT_DIR>             Directory to save the benchmark results
      --payload <PAYLOAD>            JSON payload to send with each invocation
      --payload-file <PAYLOAD_FILE>  JSON file containing the payload to send with each invocation
  -e, --env <ENVIRONMENT>            Environment variables to set (can be specified multiple times)
  -h, --help                         Print help
```

### Test a Stack
```
Usage: otel-benchmark-cli stack [OPTIONS] --dir <OUTPUT_DIR> <STACK_NAME>

Arguments:
  <STACK_NAME>  CloudFormation stack name

Options:
  -s, --select <SELECT>              Select functions by name pattern
  -m, --memory <MEMORY>              Memory size in MB
  -c, --concurrent <CONCURRENT>      Number of concurrent invocations [default: 1]
  -r, --rounds <ROUNDS>              Number of rounds for warm starts [default: 1]
  -d, --dir <OUTPUT_DIR>             Directory to save the benchmark results
      --payload <PAYLOAD>            JSON payload to send with each invocation
      --payload-file <PAYLOAD_FILE>  JSON file containing the payload to send with each invocation
  -p, --parallel                     Run function tests in parallel
  -e, --env <ENVIRONMENT>            Environment variables to set (can be specified multiple times)
  -h, --help                         Print help
```

### Run Batch Tests
```
Usage: otel-benchmark-cli batch [OPTIONS] --dir <INPUT_DIR>

Options:
  -c, --config <CONFIG>      Path to the configuration file [default: benchmark-config.yaml]
  -d, --dir <INPUT_DIR>      Directory to save the benchmark results
  -o, --output <OUTPUT_DIR>  Output directory for generated files
      --report               Generate report after benchmarking
      --jekyll               Generate Jekyll documentation after benchmarking
  -h, --help                 Print help
```

### Generate Report
```
Usage: otel-benchmark-cli report [OPTIONS] --dir <INPUT_DIR> --output <OUTPUT_DIR>

Options:
  -d, --dir <INPUT_DIR>      Directory containing benchmark results
  -o, --output <OUTPUT_DIR>  Output directory for report files
      --screenshot <THEME>   Generate screenshots with specified theme [possible values: light, dark]
  -h, --help                 Print help
```

### Generate Jekyll Documentation
```
Usage: otel-benchmark-cli jekyll [OPTIONS] --dir <INPUT_DIR> --output <OUTPUT_DIR>

Options:
  -d, --dir <INPUT_DIR>      Directory containing benchmark results
  -o, --output <OUTPUT_DIR>  Output directory for Jekyll files
  -c, --config <CONFIG>      Path to the configuration file [default: benchmark-config.yaml]
  -h, --help                 Print help
```

The report and Jekyll documentation commands generate different types of visualizations:
- The report command generates standalone HTML files with interactive charts
- The Jekyll command generates documentation that can be integrated into a Jekyll site

Note: The `--report` and `--jekyll` options in the batch command are mutually exclusive as they generate different types of output that shouldn't be mixed in the same directory.

The report command generates a browsable website with the following structure:
```
html/
└── benchmark/                                # Stack name
    ├── basic-node/                           # Function selector
    │   └── basic/                            # Test name
    │       ├── 128mb/                        # Memory configuration
    │       │   ├── client_duration.html      # Client-side latency
    │       │   ├── cold_start_init.html      # Cold start initialization
    │       │   ├── cold_start_server.html    # Server-side cold start
    │       │   ├── memory_usage.html         # Memory utilization
    │       │   ├── net_duration.html         # Network duration
    │       │   ├── net_duration_time.html    # Network duration over time
    │       │   ├── server_duration.html      # Server-side processing
    │       │   └── index.html                # Overview page
    │       ├── 256mb/                        # Another memory configuration
    │       │   └── [same structure as 128mb]
    │       └── index.html                    # Test overview
    └── index.html                            # Main overview page
```

Each test result includes visualizations for different metrics, making it easy to analyze and compare performance across different configurations and test scenarios.

## Configuration

For batch testing, create a `benchmark-config.yaml`:
```yaml
global:
  title: "Lambda OpenTelemetry Benchmarks"  # Title for the benchmark suite
  description: |                            # Overall description of the benchmark suite
    This benchmark suite compares the performance characteristics of different
    OpenTelemetry instrumentation approaches for AWS Lambda functions.
    
  memory_sizes: [128, 256]  # Test with different memory sizes
  concurrent: 5             # Number of concurrent invocations
  rounds: 50                # Number of warm start rounds
  stack_name: "benchmark"   # Default stack name
  environment:              # Global environment variables
    LOG_LEVEL: "info"
    COMMON_SETTING: "value"

tests:
  - title: "Node.js Basic Function"         # Title for this specific test
    description: "Testing basic Node.js Lambda function with different instrumentation approaches"
    name: "basic"
    selector: "basic-node"
    payload:
      depth: 2
      iterations: 3
    environment:           # Test-specific environment (merges with global)
      FUNCTION_SPECIFIC: "value"
      LOG_LEVEL: "debug"   # Overrides global LOG_LEVEL

  - title: "Python Basic Function"
    description: "Testing basic Python Lambda function with different instrumentation approaches"
    name: "basic"
    selector: "basic-python"
    payload:
      depth: 2
      iterations: 3
```

The configuration file supports:
- Global settings that apply to all tests
  - `title`: Main title for the benchmark suite (used in reports and documentation)
  - `description`: Overall description of what the benchmark suite measures
  - `memory_sizes`: List of memory configurations to test
  - `concurrent`: Number of concurrent invocations
  - `rounds`: Number of warm start rounds
  - `stack_name`: Default CloudFormation stack name
  - `environment`: Global environment variables
- Individual test configurations
  - `title`: Human-readable test title
  - `description`: Detailed description of what the test measures
  - `name`: Unique identifier for the test
  - `selector`: Pattern to match function names in the stack
  - `payload`: Test-specific payload (inline or file path)
  - `environment`: Test-specific environment variables (overrides globals)

## Technical Details

The benchmark measures several key metrics:
- Cold start duration (including initialization time)
- Warm start duration
- Maximum memory used
- Billed duration
- Client-side latency

Results are saved as JSON files and can be visualized using the report command, which generates interactive HTML reports with charts and comparisons.

## OpenTelemetry Configuration

The CLI itself is instrumented with OpenTelemetry and can send spans and traces to an OTLP collector. To enable this, configure the following environment variables:

```bash
# Required: Set the OTLP protocol (http/protobuf is recommended)
export OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf

# Required: Your collector endpoint
export OTEL_EXPORTER_OTLP_ENDPOINT=https://your.otlp.endpoint.com

# Optional: Headers for authentication
export OTEL_EXPORTER_OTLP_HEADERS=x-api-key=your-api-key
```

This allows you to monitor the benchmark execution itself using your preferred observability platform.

### AWS Permissions

The CLI needs at least these AWS permissions:
```json
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "lambda:InvokeFunction",
                "lambda:UpdateFunctionConfiguration",
                "lambda:GetFunction",
                "cloudformation:ListStackResources"
            ],
            "Resource": "*"
        }
    ]
}
```

## Contributing

Found a bug? Have an idea for improvement? Feel free to open an issue or submit a PR!

## License

This project is open-sourced under the MIT License - see the [LICENSE](LICENSE) file for details.
