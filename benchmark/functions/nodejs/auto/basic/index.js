const { trace } = require('@opentelemetry/api');

const tracer = trace.getTracer('benchmark-test');

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
 * 
 * @param {Object} event - Lambda event containing depth and iterations parameters
 * @returns {Object} Response containing the benchmark parameters
 */
exports.handler = async (event) => {
    const depth = event.depth ?? 2;
    const iterations = event.iterations ?? 2;
    
    try {
        await processLevel(depth, iterations);
        return {
            statusCode: 200,
            body: JSON.stringify({
                message: 'Benchmark complete',
                depth,
                iterations
            })
        };
    } catch (error) {
        console.error('Error in handler:', error);
        throw error;
    }
}; 