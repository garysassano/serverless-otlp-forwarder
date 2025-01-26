#!/usr/bin/env python3
"""
Example showing how to use the OTLPStdoutSpanExporter.

This example demonstrates:
1. Basic setup and configuration
2. Creating spans with attributes and events
3. Nested spans and context propagation
4. Using environment variables for configuration

Run with:
    python -m otlp_stdout_span_exporter.examples.basic_usage
"""

import os
import time
from random import randint

from opentelemetry import trace
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.trace.status import Status, StatusCode

from otlp_stdout_span_exporter import OTLPStdoutSpanExporter


def setup_tracing() -> None:
    """Configure the tracer with our exporter."""
    # Set service name via environment variable
    os.environ["OTEL_SERVICE_NAME"] = "example-service"

    # Set custom headers for the exporter
    os.environ["OTEL_EXPORTER_OTLP_HEADERS"] = "api-key=secret123,custom-header=value"

    # Create and configure the exporter
    exporter = OTLPStdoutSpanExporter(gzip_level=6)

    # Create a TracerProvider with a resource
    resource = Resource.create({"environment": "demo"})
    provider = TracerProvider(resource=resource)

    # Add the exporter to the provider
    processor = BatchSpanProcessor(exporter)
    provider.add_span_processor(processor)

    # Set the provider as the global default
    trace.set_tracer_provider(provider)


def process_item(item_id: int) -> None:
    """Simulate processing an item with nested spans."""
    tracer = trace.get_tracer(__name__)

    with tracer.start_as_current_span("process_item") as span:
        span.set_attribute("item.id", item_id)

        # Simulate some work
        time.sleep(0.1)

        # Add an event
        span.add_event("processing.started", {"timestamp": time.time()})

        # Create a nested span for validation
        with tracer.start_as_current_span("validate_item") as validate_span:
            validate_span.set_attribute("validation.level", "basic")
            time.sleep(0.05)

            # Simulate a validation error
            if item_id % 3 == 0:
                validate_span.set_status(Status(StatusCode.ERROR))
                validate_span.record_exception(
                    ValueError(f"Invalid item: {item_id}"), attributes={"error.type": "validation"}
                )
            else:
                validate_span.set_status(Status(StatusCode.OK))

        # Create a nested span for saving
        with tracer.start_as_current_span("save_item") as save_span:
            save_span.set_attribute("storage.type", "database")
            time.sleep(0.05)
            save_span.add_event("item.saved", {"timestamp": time.time()})


def main() -> None:
    """Run the example."""
    setup_tracing()

    tracer = trace.get_tracer(__name__)

    # Create a parent span for the batch operation
    with tracer.start_as_current_span("process_batch") as batch_span:
        batch_span.set_attribute("batch.size", 5)

        # Process multiple items
        for item_id in range(5):
            process_item(item_id)

            # Simulate random delays between items
            time.sleep(randint(1, 3) / 10)

        batch_span.add_event("batch.completed", {"timestamp": time.time(), "successful_items": 5})


if __name__ == "__main__":
    main()
