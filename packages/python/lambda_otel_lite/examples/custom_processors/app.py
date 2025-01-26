# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "opentelemetry-sdk",
#     "otlp-stdout-span-exporter",
#     "psutil",
#     "lambda-otel-lite",
# ]
#
# [tool.uv.sources]
# lambda-otel-lite = { path = "../../" }
# ///
"""
Example Lambda function demonstrating custom processor usage with lambda-otel-lite.

This example shows how to:
1. Create a custom processor for span enrichment with system metrics
2. Use the standard OTLP exporter with a custom processor
3. Chain processors in the right order
"""

import json

import psutil
from opentelemetry.sdk.trace import SpanProcessor
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.trace import SpanKind
from otlp_stdout_span_exporter import OTLPStdoutSpanExporter

from lambda_otel_lite import init_telemetry, traced_handler


class SystemMetricsProcessor(SpanProcessor):
    """Processor that enriches spans with system metrics at start time."""

    def on_start(self, span, parent_context=None):
        """Add system metrics when span starts."""
        # CPU usage as percentage
        cpu_percent = psutil.cpu_percent(interval=None)
        span.set_attribute("system.cpu.usage_percent", cpu_percent)

        # Memory usage
        memory = psutil.virtual_memory()
        span.set_attribute("system.memory.used_percent", memory.percent)
        span.set_attribute("system.memory.available_bytes", memory.available)

        # Process-specific metrics
        process = psutil.Process()
        span.set_attribute("process.cpu.usage_percent", process.cpu_percent(interval=None))
        span.set_attribute("process.memory.rss_bytes", process.memory_info().rss)
        span.set_attribute("process.num_threads", process.num_threads())


class DebugProcessor(SpanProcessor):
    """Processor that prints all spans as json."""

    def on_end(self, span):
        print(span.to_json())


def handler(event, context):
    """Lambda handler showing custom processor usage."""
    # Initialize with custom processors:
    # 1. SystemMetricsProcessor to add system metrics at span start
    # 2. BatchSpanProcessor with OTLPStdoutSpanExporter for standard OTLP output
    tracer, provider = init_telemetry(
        name="custom-processors-demo",
        span_processors=[
            SystemMetricsProcessor(),  # First add system metrics
            DebugProcessor(),  # Then print all spans as json
            BatchSpanProcessor(OTLPStdoutSpanExporter()),  # Then export in OTLP format
        ],
    )

    with traced_handler(
        tracer=tracer,
        tracer_provider=provider,
        name="process_request",
        event=event,
        context=context,
        kind=SpanKind.SERVER,
    ) as span:
        # Add some work to measure
        result = 0
        for i in range(1000000):
            result += i
        span.set_attribute("calculation.result", result)

        return {
            "statusCode": 200,
            "body": json.dumps({"result": result}),
        }


if __name__ == "__main__":
    import os

    os.environ["AWS_LAMBDA_FUNCTION_NAME"] = "custom-processors-example"
    handler({}, None)
