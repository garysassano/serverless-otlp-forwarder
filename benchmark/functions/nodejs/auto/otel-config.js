// Not used in this benchmark, but kept for reference
// as an example of how to customize the configuration of the JS OpenTelemetry SDK for AWS Lambda when using zero-code instrumentation
// this file needs to be required as NODE_OPTIONS='--require ./otel-config.js'

const { propagation, context: otelContext } = require('@opentelemetry/api');

// Export the configuration function
exports.configureLambdaInstrumentation = (config) => {
    console.log('Lambda instrumentation configuration called');
    return {
        ...config,
        eventContextExtractor: (event, lambdaContext) => {
            // Extract context from our custom header field
            if (event.headers) {
                return propagation.extract(otelContext.active(), event.headers);
            }
            return otelContext.active();
        }
    };
};

// Also set it on global for backwards compatibility
global.configureLambdaInstrumentation = exports.configureLambdaInstrumentation; 