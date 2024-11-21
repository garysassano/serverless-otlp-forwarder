# Lambda OTLP Forwarder

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![OpenTelemetry](https://img.shields.io/badge/OpenTelemetry-enabled-blue.svg)](https://opentelemetry.io)
![AWS Lambda](https://img.shields.io/badge/AWS-Lambda-orange?logo=amazon-aws)
[![Rust](https://img.shields.io/badge/Rust-1.70%2B-orange.svg)](https://www.rust-lang.org)
[![Python](https://img.shields.io/badge/Python-3.12%2B-blue.svg)](https://www.python.org)
[![Node.js](https://img.shields.io/badge/Node.js-18.x-green.svg)](https://nodejs.org)

![diagram](https://github.com/user-attachments/assets/aa9c2b02-5e66-4829-af08-8ceb509472ff)

## Overview

The Lambda OTLP Forwarder enables serverless applications to send OpenTelemetry data to collectors without the overhead of direct connections or sidecars. It works by:

1. Capturing telemetry data through CloudWatch Logs
2. Processing and forwarding to your OTLP collector
3. Supporting multiple programming languages and frameworks

### Why Use Lambda OTLP Forwarder?

- 📉 **Lower Costs**: Eliminates need for VPC connectivity or sidecars
- 🔒 **Enhanced Security**: Keeps telemetry data within AWS infrastructure
- 🚀 **Reduced Latency**: Minimal impact on Lambda execution and cold start times
- 💰 **Cost Optimization**: Supports compression and efficient protocols to reduce the ingestion costs

### Why not use the OTEL/ADOT Lambda Layer extension?

This project was created to address the challenges of efficiently sending telemetry data from serverless applications to OTLP collectors without adding to cold start times. The current approaches using the OTEL/ADOT Lambda Layer extension deploys a sidecar agent, which increases resource usage, slows cold starts, and drives up costs. This becomes particularly problematic when running Lambda functions with limited memory, as the [overhead of initializing and running the ADOT/OTEL layer](https://github.com/aws-observability/aws-otel-lambda/issues/228) can negate any cost savings from memory optimization. This solution provides a streamlined approach that maintains full telemetry capabilities while keeping resource consumption and costs minimal.

As a side benefit, if you're running an OTEL collector in your VPC to benefit from the advanced filtering and sampling capabilities, you don't need to expose it to the internet or connect all your lambda functions to your VPC. Since the transport for OLTP is CloudWatch logs, you are keeping all your telemetry data internal.

## Supported Languages
While the inital proof of concept was written in Rust, and the Rust OTEL SDK provided a convenient "hook" to replace the HTTP client with a custom implementation that would instead write to stdout, and a similar approach could also be used with the Python SDK, the Node.js/Typescript SDK didn't seem to provide a similar way to hook into the HTTP client, and required creating a custom provider.


### Rust
[code](packages/rust/otlp-stdout-client) | [docs](packages/rust/otlp-stdout-client/README.md) | [crates.io](https://crates.io/crates/otlp-stdout-client) | [examples](packages/rust/otlp-stdout-client/examples)

```rust
use aws_lambda_events::event::apigw::ApiGatewayProxyRequest;
use lambda_otel_utils::{HttpOtelLayer, HttpTracerProviderBuilder, Layer};
use lambda_runtime::{service_fn, Error, LambdaEvent};
use serde_json::Value;

async fn function_handler(_event: LambdaEvent<ApiGatewayProxyRequest>) -> Result<Value, Error> {
    Ok(serde_json::json!({"message": "Hello from Lambda!"}))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize tracer provider
    let tracer_provider = HttpTracerProviderBuilder::default()
        .with_stdout_client()
        .with_tracer_name("example-lambda-function")
        .build()?;
    
    // Create a service with a tracing layer
    let service = HttpOtelLayer::new(|| {
        tracer_provider.force_flush();
    })
    .layer(service_fn(function_handler));

    // Run the Lambda runtime
    lambda_runtime::run(service).await?;
    Ok(())
}
```

### Python
[code](packages/python/adapter) | [docs](packages/python/adapter/README.md) | [pypi](https://pypi.org/project/otlp-stdout-adapter/) | [examples](packages/python/adapter/examples)

```python
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
from otlp_stdout_adapter import StdoutAdapter, get_lambda_resource
from opentelemetry.trace import SpanKind
from contextlib import contextmanager

def init_telemetry(service_name: str = __name__) -> tuple[trace.Tracer, TracerProvider]:
    """Initialize OpenTelemetry with AWS Lambda-specific configuration"""
    provider = TracerProvider(resource=get_lambda_resource())
    
    provider.add_span_processor(BatchSpanProcessor(
        OTLPSpanExporter(
            session=StdoutAdapter().get_session(),
            timeout=5
        )
    ))

    trace.set_tracer_provider(provider)
    return trace.get_tracer(service_name), provider

# Initialize tracer
tracer, tracer_provider = init_telemetry()

@contextmanager
def force_flush(tracer_provider):
    """Ensure traces are flushed even if Lambda freezes"""
    try:
        yield
    finally:
        tracer_provider.force_flush()

def lambda_handler(event, context):
    with force_flush(tracer_provider), tracer.start_as_current_span(
        "lambda-invocation",
        kind=SpanKind.SERVER
    ) as span:
        try:
            result = {"message": "Hello from Lambda!"}
            return {
                "statusCode": 200,
                "body": json.dumps(result)
            }
        except Exception as e:
            span.record_exception(e)
            span.set_status(trace.StatusCode.ERROR, str(e))
            raise
```

### Node
[code](packages/node/exporter) | [docs](packages/node/exporter/README.md) | [npm](https://www.npmjs.com/package/@dev7a/otlp-stdout-exporter) | [examples](packages/node/exporter/examples)
```javascript
const { NodeTracerProvider } = require('@opentelemetry/sdk-trace-node');
const { BatchSpanProcessor } = require('@opentelemetry/sdk-trace-base');
const { Resource } = require('@opentelemetry/resources');
const { trace, SpanKind, context, propagation } = require('@opentelemetry/api');
const { StdoutOTLPExporterNode } = require('@dev7a/otlp-stdout-exporter');
const { AwsLambdaDetectorSync } = require('@opentelemetry/resource-detector-aws');
const { W3CTraceContextPropagator } = require('@opentelemetry/core');

// Set up W3C Trace Context propagator
propagation.setGlobalPropagator(new W3CTraceContextPropagator());

const createProvider = () => {
  const awsResource = new AwsLambdaDetectorSync().detect();
  const resource = new Resource({
    ["service.name"]: process.env.AWS_LAMBDA_FUNCTION_NAME || 'demo-function',
  }).merge(awsResource);

  const provider = new NodeTracerProvider({ resource });
  provider.addSpanProcessor(new BatchSpanProcessor(new StdoutOTLPExporterNode()));
  return provider;
};

const provider = createProvider();
provider.register();
const tracer = trace.getTracer('demo-function');

exports.handler = async (event, context) => {
  const parentSpan = tracer.startSpan('lambda-invocation', {
    kind: SpanKind.SERVER
  });

  return await context.with(trace.setSpan(context.active(), parentSpan), async () => {
    try {
      const result = { message: 'Hello from Lambda!' };
      return {
        statusCode: 200,
        body: JSON.stringify(result)
      };
    } catch (error) {
      parentSpan.recordException(error);
      parentSpan.setStatus({ code: 1 });
      throw error;
    } finally {
      parentSpan.end();
      await provider.forceFlush();
    }
  });
};
```

## Architecture

### Components
1. **Application Instrumentation**: Language-specific libraries that format telemetry data and write to stdout/CloudWatch Logs
2. **CloudWatch Logs**: Transport layer for telemetry data
3. **Forwarder Lambda**: Processes and forwards data to collectors
4. **OTLP Collector**: Your chosen observability platform

### Configuring the Forwarder
Each application needs to be instrumented with the appropriate Opentelemetry SDK for the application platform, andmust be configured to write to stdout using the [client](packages/rust/otlp-stdout-client) in Rust, the [adapter](packages/python/adapter) in Python, or the [exporter](packages/node/exporter) in Node. 

Additionally, each application must also define a collector endpoint, protocol, and optional compression in the environment variables.
For instance, this is an example configuration for a SAM template:

```yaml
  InstrumentedFunction:
    Type: AWS::Serverless::Function
    Properties:
      FunctionName: !Sub '${NestedStackName}-example-function'
      CodeUri: ./src
      Handler: main.lambda_handler
      Runtime: python3.12
      Description: 'Example instrumented Lambda function'
      Environment:
        Variables:
          OTEL_EXPORTER_OTLP_ENDPOINT: https://localhost:4318
          OTEL_EXPORTER_OTLP_PROTOCOL: http/protobuf
          OTEL_EXPORTER_OTLP_COMPRESSION: gzip
          OTEL_SERVICE_NAME: !Sub '${NestedStackName}-example-function'
```
See the [demo/template.yaml](demo/template.yaml) for a complete example with multiple functions.

Note that the `OTEL_EXPORTER_OTLP_ENDPOINT` can just be set to localhost, as the actual endpoint will be determined by the forwarder, based on its own configuration, but it's useful to set it to a known value as some SDKs or libraries may not work otherwise.

> [!IMPORTANT]
> If you're using an observability vendor that requires authentication, you should not set the `OTEL_EXPORTER_OTLP_HEADERS` environment variable to include your credentials in your instrumented lambda functions as they would be sent in the logs (and in any case, ignored by the forwarder). The authentication headers should be added to the collector configuration instead (see [Configuring the Collector](#configuring-the-collector) below).

### Configuring the Collector
The configuration for the collector is done through a secret in AWS secret manager. 
By default, the forwarder service looks into a key defined as: `lambda-otlp-forwarder/keys/default`, set by the template parameter `CollectorsSecretsKeyPrefix` in the [SAM template](template.yaml).

To create the secret, you can just use the AWS CLI (or the AWS console as you prefer):

```bash
aws secretsmanager create-secret \
  --name "lambda-otlp-forwarder/keys/default" \
  --secret-string '{
    "name": "my-collector",
    "endpoint": "https://collector.example.com",
    "auth": "x-api-key=your-api-key"
  }'
```

where:
- `--name` is the AWS secret manager key for the default collector.
- `name` is a friendly name for the collector, for instance `selfhosted`, `honeycomb`, or `datadog`, etc.
- `endpoint` is the URL of the collector endpoint for http/protobuf or http/json.
- `auth` is the optional authentication header to use. If omitted, the forwarder will not add any authentication headers to the requests.

The default collector configuration serves two purposes:
1. It receives and forwards telemetry data from all instrumented services in the AWS account
2. It handles the forwarder service's own telemetry data, ensuring the forwarder itself is properly monitored

[!TIP] You can add multiple configurations secrets under the same prefix, if for whatever reason you want to forward to multiple collectors. The forwarder will load all the collectors and send the telemetry data to all of them, in parallel. For instance, you could create a `lambda-otlp-forwarder/keys/honeycomb` and a `lambda-otlp-forwarder/keys/datadog` secret, each with the appropriate endpoint and authentication header. All the telemetry data will be sent to both collectors.


### Data Flow
1. Your application emits telemetry data to stdout
2. CloudWatch Logs captures the output
3. Forwarder Lambda processes matching log entries
4. Data is forwarded to your OTLP collector

## Quick Start

1. Install prerequisites:
   - [AWS SAM CLI](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/install-sam-cli.html)
   - [Rust](https://www.rust-lang.org/tools/install)
   - [Cargo Lambda](https://www.cargo-lambda.info/guide/installation.html)

2. Deploy the forwarder in your aws account:
   ```bash
   git clone https://github.com/dev7a/lambda-otlp-forwarder
   cd lambda-otlp-forwarder
   sam build && sam deploy --guided
   ```

3. Instrument your application to emit telemetry data using the otel SDK for your language:
   [Rust](#rust) | [Python](#python) | [Node.js](#nodejs)

## Configuration
The configuration in the `samconfig.toml` file can be used to override the default parameters for the forwarder service in your aws account.
By default, the forwarded is configured to subscribe to all log groups in the account, and a simple demo application is deployed to validate the telemetry ingestion.

### Environment Variables

The following environment variables can be set in the instrumented lambda function to override the default parameters for the forwarder service.
| Variable | Description | Default |
|----------|-------------|---------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | Collector endpoint | `http://localhost:4318` |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `http/protobuf` or `http/json` | `http/protobuf` |
| `OTEL_EXPORTER_OTLP_COMPRESSION` | `gzip` or `none` | `gzip` |

### Best Practices

1. **Protocol Selection**
   - Use `http/protobuf` for smaller payloads
   - Enable GZIP compression for further size reduction

2. **Multi-Account Setup**
   - Deploy one forwarder per AWS account
   - Consider using AWS Organizations for management

## Development

- [Contributing Guidelines](CONTRIBUTING.md)
- [Security Policy](SECURITY.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.