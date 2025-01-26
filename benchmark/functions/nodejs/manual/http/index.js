const { initTelemetry, tracedHandler } = require('@dev7a/lambda-otel-lite');
const { registerInstrumentations } = require('@opentelemetry/instrumentation');
const { HttpInstrumentation } = require('@opentelemetry/instrumentation-http');

const { provider, tracer } = initTelemetry('benchmark-http');

registerInstrumentations({
    tracerProvider: provider,
    instrumentations: [
      new HttpInstrumentation(),
    ]
  });

const https = require('https');

const exampleUrl = 'https://example.com';

/**
 * Makes a HEAD request to example.com and traces it with OpenTelemetry spans.
 */
async function testHttpOperations() {
    return await tracer.startActiveSpan('http-operations', async (parentSpan) => {
        try {
            await tracer.startActiveSpan('http-head', async (span) => {
                try {
                    await new Promise((resolve, reject) => {
                        const req = https.request(exampleUrl, {
                            method: 'HEAD',
                        }, (res) => {
                            res.resume(); // Consume response to free memory
                            resolve();
                        });
                        req.on('error', reject);
                        req.end();
                    });
                } catch (error) {
                    span.setStatus({ code: SpanStatusCode.ERROR, message: error.message });
                    span.recordException(error);
                    throw error;
                } finally {
                    span.end();
                }
            });
        } finally {
            parentSpan.end();
        }
    });
}

/**
 * Lambda handler that exercises HTTP operations with OpenTelemetry instrumentation.
 * 
 * This handler executes HTTP operations to generate telemetry spans. It uses
 * the HttpInstrumentation to automatically instrument HTTP calls.
 */
exports.handler = async (event, context) => {
    return await tracedHandler({
        tracer,
        provider,
        name: 'lambda-invocation',
        event,
        context,
        fn: async (span) => {
            await testHttpOperations();
            return {
                statusCode: 200,
                body: JSON.stringify({
                    message: 'Benchmark complete'
                })
            };
        }
    });
}; 
