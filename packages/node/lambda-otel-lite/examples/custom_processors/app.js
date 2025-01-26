/**
 * Example Lambda function demonstrating custom processor usage with lambda-otel-lite.
 * 
 * This example shows how to:
 * 1. Create a custom processor for span enrichment with system metrics
 * 2. Use the standard OTLP exporter with a custom processor
 * 3. Chain processors in the right order
 */

const { SpanKind } = require('@opentelemetry/api');
const { BatchSpanProcessor, NoopSpanProcessor } = require('@opentelemetry/sdk-trace-base');
const { OTLPStdoutSpanExporter } = require('@dev7a/otlp-stdout-span-exporter');
const { initTelemetry, tracedHandler } = require('@dev7a/lambda-otel-lite');
const os = require('os');

/**
 * Processor that enriches spans with system metrics at start time.
 */
class SystemMetricsProcessor extends NoopSpanProcessor {
  onStart(span) {
    // CPU usage
    const cpus = os.cpus();
    const totalIdle = cpus.reduce((acc, cpu) => acc + cpu.times.idle, 0);
    const totalTick = cpus.reduce(
      (acc, cpu) =>
        acc + cpu.times.idle + cpu.times.user + cpu.times.sys + cpu.times.nice + cpu.times.irq,
      0
    );
    const cpuUsage = ((totalTick - totalIdle) / totalTick) * 100;
    span.setAttribute('system.cpu.usage_percent', cpuUsage);

    // Memory usage
    const totalMem = os.totalmem();
    const freeMem = os.freemem();
    const usedMem = totalMem - freeMem;
    const memoryUsage = (usedMem / totalMem) * 100;

    span.setAttribute('system.memory.used_percent', memoryUsage);
    span.setAttribute('system.memory.available_bytes', freeMem);

    // Process metrics
    const processMemory = process.memoryUsage();
    span.setAttribute('process.memory.rss_bytes', processMemory.rss);
    span.setAttribute('process.memory.heap_used_bytes', processMemory.heapUsed);
    span.setAttribute('process.memory.heap_total_bytes', processMemory.heapTotal);
  }
}

/**
 * Processor that prints all spans as json.
 */
class DebugProcessor extends NoopSpanProcessor {

  _nsToIsoString(hrTime) {
    if (!hrTime) return null;
    // Convert hrtime [seconds, nanoseconds] to milliseconds
    const milliseconds = (hrTime[0] * 1e3) + (hrTime[1] / 1e6);
    return new Date(milliseconds).toISOString();
  }

  _formatAttributes(attributes) {
    if (!attributes) return {};
    if (attributes instanceof Map) {
      const result = {};
      attributes.forEach((value, key) => {
        result[key] = value;
      });
      return result;
    }
    return attributes;
  }

  onEnd(span) {
    const parentId = span.parentSpanId ? `0x${span.parentSpanId}` : null;
    const context = span.spanContext();

    const spanJson = {
      name: span.name,
      context: {
        trace_id: `0x${context.traceId}`,
        span_id: `0x${context.spanId}`,
        trace_flags: context.traceFlags,
        trace_state: context.traceState?.serialize() || null,
      },
      kind: String(span.kind),
      parent_id: parentId,
      start_time: this._nsToIsoString(span.startTime),
      end_time: this._nsToIsoString(span.endTime),
      status: {
        status_code: String(span.status.code),
        description: span.status.message || null
      },
      attributes: this._formatAttributes(span.attributes),
      events: span.events.map(event => ({
        name: event.name,
        timestamp: this._nsToIsoString(event.time),
        attributes: this._formatAttributes(event.attributes)
      })),
      links: span.links.map(link => ({
        context: {
          trace_id: `0x${link.context.traceId}`,
          span_id: `0x${link.context.spanId}`,
          trace_flags: link.context.traceFlags,
          trace_state: link.context.traceState?.serialize() || null,
        },
        attributes: this._formatAttributes(link.attributes)
      })),
      resource: span.resource?.attributes ? this._formatAttributes(span.resource.attributes) : null,
    };

    console.log(JSON.stringify(spanJson, null, 4));
  }
}

// Initialize with custom processors:
// 1. SystemMetricsProcessor to add system metrics at span start
// 2. DebugProcessor to print all spans as json
// 3. OTLPStdoutSpanExporter for standard OTLP output
const { tracer, provider } = initTelemetry('custom-processors-demo', {
  spanProcessors: [
    new SystemMetricsProcessor(),  // First add system metrics
    new DebugProcessor(),          // Then print all spans as json
    new BatchSpanProcessor(        // Then export in OTLP format
      new OTLPStdoutSpanExporter({
        gzipLevel: parseInt(process.env.OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL || '6', 10)
      })
    )
  ]
});

exports.handler = async (event, context) => {
  return tracedHandler(
    {
      tracer,
      provider,
      name: 'process_request',
      event,
      context,
      kind: SpanKind.SERVER,
    },
    async (span) => {
      // Add some work to measure
      let result = 0;
      for (let i = 0; i < 1000000; i++) {
        result += i;
      }
      span.setAttribute('calculation.result', result);

      return {
        statusCode: 200,
        body: JSON.stringify({ result }),
      };
    }
  );
};

// Allow running the example locally
if (require.main === module) {
  exports.handler({}, {}).catch(console.error);
} 