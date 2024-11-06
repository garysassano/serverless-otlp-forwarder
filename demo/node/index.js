const { NodeTracerProvider } = require('@opentelemetry/sdk-trace-node');
const { BatchSpanProcessor } = require('@opentelemetry/sdk-trace-base');
const { Resource } = require('@opentelemetry/resources');
const { ATTR_SERVICE_NAME } = require('@opentelemetry/semantic-conventions');
const { trace, SpanKind, context, propagation } = require('@opentelemetry/api');
const { StdoutOTLPExporterNode } = require('@dev7a/otlp-stdout-exporter');
const { AwsLambdaDetectorSync } = require('@opentelemetry/resource-detector-aws');
const axios = require('axios');
const { W3CTraceContextPropagator } = require('@opentelemetry/core');

// Configure axios with OpenTelemetry
const { registerInstrumentations } = require('@opentelemetry/instrumentation');
const { HttpInstrumentation } = require('@opentelemetry/instrumentation-http');

registerInstrumentations({
  instrumentations: [
    new HttpInstrumentation(),
  ],
});

const QUOTES_URL = 'https://dummyjson.com/quotes/random';
const TARGET_URL = process.env.TARGET_URL;

// Set up W3C Trace Context propagator
propagation.setGlobalPropagator(new W3CTraceContextPropagator());

const createProvider = () => {
  // Detect AWS Lambda resources synchronously
  const awsResource = new AwsLambdaDetectorSync().detect();
  
  // Merge AWS resource with service name
  const resource = new Resource({
    [ATTR_SERVICE_NAME]: process.env.AWS_LAMBDA_FUNCTION_NAME || 'demo-function',
  }).merge(awsResource);

  const provider = new NodeTracerProvider({
    resource
  });

  // Configure the stdout exporter with env based configuration
  const exporter = new StdoutOTLPExporterNode();

  // Add the exporter to the provider with batch processing
  provider.addSpanProcessor(new BatchSpanProcessor(exporter));

  return provider;
};

const provider = createProvider();
provider.register();
const tracer = trace.getTracer('demo-function');

// Helper function to get random quote
async function getRandomQuote() {
  // Get the parent context
  const parentContext = context.active();
  
  const span = tracer.startSpan('get_random_quote', {
    kind: SpanKind.CLIENT,
  }, parentContext);  // Pass the parent context

  // Set the context with the new span
  return await context.with(trace.setSpan(parentContext, span), async () => {
    try {
      const response = await axios.get(QUOTES_URL);
      return response.data;
    } catch (error) {
      span.recordException(error);
      span.setStatus({ code: 1 });
      throw error;
    } finally {
      span.end();
    }
  });
}

// Helper function to save quote
async function saveQuote(quote) {
  const parentContext = context.active();
  
  const span = tracer.startSpan('save_quote', {
    kind: SpanKind.CLIENT,
  }, parentContext);

  return await context.with(trace.setSpan(parentContext, span), async () => {
    try {
      const headers = {
        'content-type': 'application/json',
      };
      
      // Inject trace context into headers
      propagation.inject(context.active(), headers);
      
      const response = await axios.post(TARGET_URL, quote, { headers });
      return response.data;
    } catch (error) {
      span.recordException(error);
      span.setStatus({ code: 1 });
      throw error;
    } finally {
      span.end();
    }
  });
}

exports.handler = async (event, lambdaContext) => {
  const parentSpan = tracer.startSpan('lambda-otlp-forwarder-demo-nodejs-client', {
    kind: SpanKind.SERVER
  });

  // Set the root span as active
  return await context.with(trace.setSpan(context.active(), parentSpan), async () => {
    try {
      parentSpan.addEvent('Lambda Invocation Started');

      // Get random quote
      const quote = await getRandomQuote();
      parentSpan.addEvent('Random Quote Fetched', {
        attributes: {
          severity: 'info',
          quote: quote.quote
        }
      });

      // Save the quote
      const response = await saveQuote(quote);
      parentSpan.addEvent('Quote Saved', {
        attributes: {
          severity: 'info',
          quote: quote.quote
        }
      });

      parentSpan.addEvent('Lambda Execution Completed');

      return {
        statusCode: 200,
        body: JSON.stringify({
          message: 'Hello from demo Lambda!',
          input: event,
          quote: quote,
          response: response
        })
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
