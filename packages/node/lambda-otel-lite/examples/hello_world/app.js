/**
 * Simple Hello World Lambda function using lambda-otel-lite.
 */

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

// Allow running the example locally
if (require.main === module) {
  exports.handler({}, {}).catch(console.error);
} 