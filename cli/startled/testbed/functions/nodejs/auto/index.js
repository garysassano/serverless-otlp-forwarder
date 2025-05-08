const { trace } = require('@opentelemetry/api');
const tracer = trace.getTracer('benchmark-test');

const DEFAULT_DEPTH = 2;
const DEFAULT_ITERATIONS = 4;

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
        // Create and wait for each span (and its children) to complete before moving to next iteration
        const spanPromise = new Promise(resolve => {
            tracer.startActiveSpan(`operation_depth_${depth}_iter_${i}`, {
                attributes: {
                    depth,
                    iteration: i,
                    payload: "x".repeat(256)
                }
            }, async (span) => {
                try {
                    await processLevel(depth - 1, iterations);
                } finally {
                    span.end();
                    resolve();
                }
            });
        });
        
        // Wait for this span and all its children to complete before continuing to next iteration
        await spanPromise;
    }
}

/**
 * Lambda handler that creates a tree of spans based on input parameters.
 * 
 * @param {Object} event - Lambda event containing depth and iterations parameters
 * @returns {Object} Response containing the benchmark parameters
 */
exports.handler = async (event) => {
    const depth = event.depth ?? DEFAULT_DEPTH;
    const iterations = event.iterations ?? DEFAULT_ITERATIONS;

    // Get the current active span from the tracer
    const currentSpan = trace.getActiveSpan();
    currentSpan.addEvent('Event payload', {
        event: JSON.stringify(event)
    });

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