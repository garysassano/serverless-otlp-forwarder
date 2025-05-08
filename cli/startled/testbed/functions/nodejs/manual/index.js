const { trace } = require('@opentelemetry/api');
const { initTelemetry, createTracedHandler, apiGatewayV2Extractor } = require('@dev7a/lambda-otel-lite');

// Initialize telemetry once at module load
const { tracer, completionHandler } = initTelemetry();

const DEFAULT_DEPTH = 2;
const DEFAULT_ITERATIONS = 4;

// Create a traced handler with configuration
const traced = createTracedHandler(
    'handler',
    completionHandler
  );
  
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
 */
exports.handler = traced(async (event, context) => {
    const depth = event.depth ?? DEFAULT_DEPTH;
    const iterations = event.iterations ?? DEFAULT_ITERATIONS;
    context.callbackWaitsForEmptyEventLoop = true;
    // Get the current active span from the tracer
    const currentSpan = trace.getActiveSpan();
    currentSpan.addEvent('Event payload', {
        event: JSON.stringify(event)
    });

    await processLevel(depth, iterations);
    return {
        statusCode: 200,
        body: JSON.stringify({
            message: 'Benchmark complete',
            depth,
            iterations
        })
    };
}); 