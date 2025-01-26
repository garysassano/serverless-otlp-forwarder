const { trace, SpanStatusCode } = require('@opentelemetry/api');
const https = require('https');

const tracer = trace.getTracer('benchmark-test');

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
 * This handler executes HTTP operations to generate telemetry spans. HTTP instrumentation
 * is handled automatically by the Lambda layer.
 */
exports.handler = async (event, context) => {
    await testHttpOperations();
    return {
        statusCode: 200,
        body: JSON.stringify({
            message: 'Benchmark complete'
        })
    };
}; 