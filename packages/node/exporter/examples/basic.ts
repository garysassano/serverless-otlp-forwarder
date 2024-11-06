import { StdoutOTLPExporterNode } from '@dev7a/otlp-stdout-exporter';
import { CompressionAlgorithm } from '@opentelemetry/otlp-exporter-base';
import { NodeTracerProvider } from '@opentelemetry/sdk-trace-node';
import { SimpleSpanProcessor } from '@opentelemetry/sdk-trace-base';
import { Resource } from '@opentelemetry/resources';
import { ATTR_SERVICE_NAME } from '@opentelemetry/semantic-conventions';
import { SpanStatusCode, Exception } from '@opentelemetry/api';

// Simulate some async work
async function someAsyncWork(): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, 100));
}

// Configure the tracer provider
const provider = new NodeTracerProvider({
  resource: new Resource({
    [ATTR_SERVICE_NAME]: 'example-service',
  }),
});

// Create and configure the exporter
const exporter = new StdoutOTLPExporterNode({
  compression: CompressionAlgorithm.GZIP,
  timeoutMillis: 5000,
  url: 'http://localhost:4318/v1/traces'
});

// Add the exporter to the provider
provider.addSpanProcessor(new SimpleSpanProcessor(exporter));

// Register the provider
provider.register();

// Get a tracer
const tracer = provider.getTracer('example-tracer');

// Create spans
async function doWork() {
  const span = tracer.startSpan('main-operation');
  
  try {
    // Add attributes
    span.setAttribute('operation.value', 123);
    
    // Create a child span
    const childSpan = tracer.startSpan('child-operation');
    await someAsyncWork();
    childSpan.end();
    
  } catch (error) {
    // Properly type the error for recordException
    const exception: Exception = {
      message: error instanceof Error ? error.message : String(error),
      name: error instanceof Error ? error.name : 'UnknownError',
    };
    span.recordException(exception);
    span.setStatus({ code: SpanStatusCode.ERROR });
  } finally {
    span.end();
  }
}

// Run the example
doWork().then(() => {
  // Shutdown the provider after a short delay to ensure spans are exported
  setTimeout(() => {
    provider.shutdown();
  }, 1000);
}); 