import { trace, Tracer } from '@opentelemetry/api';
import { NodeTracerProvider } from '@opentelemetry/sdk-trace-node';
import { Resource, detectResourcesSync, envDetectorSync } from '@opentelemetry/resources';
import { SpanProcessor } from '@opentelemetry/sdk-trace-base';
import { OTLPStdoutSpanExporter } from '@dev7a/otlp-stdout-span-exporter';
import { state } from '../state';
import { LambdaSpanProcessor } from './processor';

// Track cold start
let _isColdStart = true;

export function isColdStart(): boolean {
  return _isColdStart;
}

export function setColdStart(value: boolean): void {
  _isColdStart = value;
}

/**
 * Create a Resource instance with AWS Lambda attributes and OTEL environment variables.
 * 
 * This function combines AWS Lambda environment attributes with any OTEL resource attributes
 * specified via environment variables (OTEL_RESOURCE_ATTRIBUTES and OTEL_SERVICE_NAME).
 * 
 * @param name - Name of the service to use if not overridden by environment variables
 * @returns Resource instance with AWS Lambda and OTEL environment attributes
 */
function getLambdaResource(name: string): Resource {
  // Start with Lambda attributes
  const attributes: Record<string, string> = {
    'cloud.provider': 'aws'
  };

  // Map environment variables to attribute names
  const envMappings: Record<string, string> = {
    AWS_REGION: 'cloud.region',
    AWS_LAMBDA_FUNCTION_NAME: 'faas.name',
    AWS_LAMBDA_FUNCTION_VERSION: 'faas.version',
    AWS_LAMBDA_LOG_STREAM_NAME: 'faas.instance',
    AWS_LAMBDA_FUNCTION_MEMORY_SIZE: 'faas.max_memory'
  };

  // Add attributes only if they exist in environment
  for (const [envVar, attrName] of Object.entries(envMappings)) {
    const value = process.env[envVar];
    if (value) {
      attributes[attrName] = value;
    }
  }

  // Add service name (guaranteed to have a value)
  const serviceName = process.env.OTEL_SERVICE_NAME || process.env.AWS_LAMBDA_FUNCTION_NAME || 'unknown_service';
  attributes['service.name'] = serviceName;

  // Add OTEL environment resource attributes if present
  const envResourcesItems = process.env.OTEL_RESOURCE_ATTRIBUTES;
  if (envResourcesItems) {
    for (const item of envResourcesItems.split(',')) {
      try {
        const [key, value] = item.split('=', 2);
        if (value?.trim()) {
          const valueUrlDecoded = decodeURIComponent(value.trim());
          attributes[key.trim()] = valueUrlDecoded;
        }
      } catch {
        // Skip malformed items
        continue;
      }
    }
  }

  // Create resource and merge with default resource
  const resource = new Resource(attributes);
  return Resource.default().merge(resource);
}

/**
 * Initializes OpenTelemetry telemetry for a Lambda function.
 * 
 * @param name - Name of the tracer to create
 * @param options - Optional configuration options
 * @param options.resource - Custom Resource to use instead of auto-detected resources
 * @param options.spanProcessors - Array of SpanProcessor implementations to use. If not provided,
 *                                defaults to LambdaSpanProcessor with OTLPStdoutSpanExporter
 * @returns Object containing the NodeTracerProvider and Tracer instances
 * 
 */
export function initTelemetry(
  name: string,
  options?: {
    resource?: Resource,
    spanProcessors?: SpanProcessor[]
  }
): { provider: NodeTracerProvider; tracer: Tracer } {
  if (!name) {
    throw new Error('Tracer name must be provided to initTelemetry');
  }

  // Setup resource
  const baseResource = options?.resource || getLambdaResource(name);

  // Setup processors with compression level support
  const processors = options?.spanProcessors || [
    new LambdaSpanProcessor(
      new OTLPStdoutSpanExporter({
        gzipLevel: parseInt(process.env.OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL || '6', 10)
      }),
      { maxQueueSize: parseInt(process.env.LAMBDA_SPAN_PROCESSOR_QUEUE_SIZE || '2048', 10) }
    )
  ];

  // Create provider with resources
  const provider = new NodeTracerProvider({
    resource: baseResource,
    spanProcessors: processors
  });

  // Store in shared state for extension
  state.provider = provider;

  // Register as global tracer
  provider.register();

  return { provider, tracer: trace.getTracer(name) };
}

export function getTracerProvider() {
  return state.provider;
} 