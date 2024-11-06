"""
Basic example demonstrating OpenTelemetry instrumentation using otlp-stdout-adapter.

This example shows how to:
1. Set up the OTLP exporter with stdout adapter
2. Create and configure a tracer provider
3. Create spans and add attributes
4. Use context propagation
"""

import time
import random
import os
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
from otlp_stdout_adapter import StdoutAdapter, get_lambda_resource


def setup_telemetry():
    """Configure OpenTelemetry with the stdout adapter."""
    # Initialize the StdoutAdapter
    adapter = StdoutAdapter()
    session = adapter.get_session()

    # Create OTLP exporter with custom session
    exporter = OTLPSpanExporter(
        endpoint=os.environ.get(
            "OTEL_EXPORTER_OTLP_ENDPOINT", 
            "http://localhost:4318/v1/traces"
        ),
        session=session
    )

    # Set up the trace provider with Lambda resource attributes
    provider = TracerProvider(resource=get_lambda_resource())
    processor = BatchSpanProcessor(exporter)
    provider.add_span_processor(processor)
    trace.set_tracer_provider(provider)

    return trace.get_tracer(__name__)


def process_item(tracer, item):
    """Simulate processing an item with nested spans."""
    with tracer.start_as_current_span("process_item") as span:
        span.set_attribute("item.id", item)
        span.set_attribute("item.type", "test")
        
        # Simulate some work
        time.sleep(random.uniform(0.1, 0.3))
        
        # Create a nested span for validation
        with tracer.start_span("validate_item") as validate_span:
            validate_span.set_attribute("validation.level", "basic")
            time.sleep(random.uniform(0.1, 0.2))

        # Create a nested span for processing
        with tracer.start_span("transform_item") as transform_span:
            transform_span.set_attribute("transform.type", "normalize")
            time.sleep(random.uniform(0.1, 0.2))


def main():
    """Main function demonstrating span creation and attributes."""
    # Set up OpenTelemetry
    tracer = setup_telemetry()

    # Create a parent span for the batch process
    with tracer.start_as_current_span("batch_process") as batch_span:
        batch_span.set_attribute("batch.size", 3)
        batch_span.set_attribute("batch.type", "test")
        
        # Process multiple items
        for item_id in range(3):
            process_item(tracer, item_id)

    # Force flush to ensure all spans are exported
    provider = trace.get_tracer_provider()
    provider.force_flush()


if __name__ == "__main__":
    # Set service name for identification
    os.environ.setdefault("OTEL_SERVICE_NAME", "example-service")
    
    # Enable compression (optional)
    os.environ.setdefault("OTEL_EXPORTER_OTLP_COMPRESSION", "gzip")
    
    main()