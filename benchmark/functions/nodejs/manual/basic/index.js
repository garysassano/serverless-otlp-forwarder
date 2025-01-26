const { initTelemetry, tracedHandler } = require('@dev7a/lambda-otel-lite');
const { provider, tracer } = initTelemetry('benchmark-basic');

/**
 * Recursively create a tree of spans to measure OpenTelemetry overhead.
 * 
 * @param {number} depth - Current depth level (decrements towards 0)
 * @param {number} iterations - Number of spans to create at each level
 */
async function processLevel(depth, iterations) {
    if (depth <= 0) {
        return;
    }
    
    for (let i = 0; i < iterations; i++) {
        await tracer.startActiveSpan(`operation_depth_${depth}_iter_${i}`, {
            attributes: {
                depth,
                iteration: i
            }
        }, async (span) => {
            try {
                await processLevel(depth - 1, iterations);
                span.addEvent('process-level-complete');
            } finally {
                span.end();
            }
        });
    }
}

/**
 * Lambda handler that creates a tree of spans based on input parameters.
 */
exports.handler = async (event, context) => {
    const depth = event.depth ?? 2;
    const iterations = event.iterations ?? 2;
    
    return await tracedHandler({
        tracer,
        provider,
        name: 'lambda-invocation',
        event,
        context,
        fn: async (span) => {
            await processLevel(depth, iterations);
            return {
                statusCode: 200,
                body: JSON.stringify({
                    message: 'Benchmark complete',
                    depth,
                    iterations
                })
            };
        }
    });
}; 