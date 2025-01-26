"""
Telemetry initialization for lambda-otel-lite.

This module provides the initialization function for OpenTelemetry in AWS Lambda.
"""

import os
from urllib import parse

from opentelemetry import trace
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import SpanProcessor, TracerProvider
from otlp_stdout_span_exporter import OTLPStdoutSpanExporter

from . import ProcessorMode, processor_mode
from .extension import init_extension
from .processor import LambdaSpanProcessor

# Global state
_tracer_provider: TracerProvider | None = None


def get_lambda_resource() -> Resource:
    """Create a Resource instance with AWS Lambda attributes and OTEL environment variables.

    This function combines AWS Lambda environment attributes with any OTEL resource attributes
    specified via environment variables (OTEL_RESOURCE_ATTRIBUTES and OTEL_SERVICE_NAME).

    Returns:
        Resource instance with AWS Lambda and OTEL environment attributes
    """
    # Start with Lambda attributes
    # Create base attributes with cloud provider (this is always AWS in Lambda)
    attributes = {"cloud.provider": "aws"}

    # Map environment variables to attribute names
    env_mappings = {
        "AWS_REGION": "cloud.region",
        "AWS_LAMBDA_FUNCTION_NAME": "faas.name",
        "AWS_LAMBDA_FUNCTION_VERSION": "faas.version",
        "AWS_LAMBDA_LOG_STREAM_NAME": "faas.instance",
        "AWS_LAMBDA_FUNCTION_MEMORY_SIZE": "faas.max_memory",
    }

    # Add attributes only if they exist in environment
    for env_var, attr_name in env_mappings.items():
        if value := os.environ.get(env_var):
            attributes[attr_name] = value

    # Add service name (guaranteed to have a value)
    service_name = os.environ.get(
        "OTEL_SERVICE_NAME", os.environ.get("AWS_LAMBDA_FUNCTION_NAME", "unknown_service")
    )
    attributes["service.name"] = service_name

    # Add OTEL environment resource attributes if present
    env_resources_items = os.environ.get("OTEL_RESOURCE_ATTRIBUTES")
    if env_resources_items:
        for item in env_resources_items.split(","):
            try:
                key, value = item.split("=", maxsplit=1)
            except ValueError:
                continue
            if value := value.strip():
                value_url_decoded = parse.unquote(value)
                attributes[key.strip()] = value_url_decoded

    # Create resource and merge with default resource
    resource = Resource(attributes)
    return Resource.create().merge(resource)


def init_telemetry(
    name: str,  # Name to identify the tracer instance
    resource: Resource | None = None,
    span_processors: list[SpanProcessor] | None = None,
) -> tuple[trace.Tracer, TracerProvider]:
    """Initialize OpenTelemetry with manual OTLP stdout configuration.

    This function provides a flexible way to initialize OpenTelemetry for AWS Lambda,
    with sensible defaults that work well in most cases but allowing customization
    where needed.

    Args:
        name: Name to identify the tracer instance. This is not the service name, which
            should be set via OTEL_SERVICE_NAME environment variable or custom resource.
        resource: Optional custom Resource. Defaults to Lambda resource detection
        span_processors: Optional list of SpanProcessors. If None, a default LambdaSpanProcessor
            with OTLPStdoutSpanExporter will be used. If provided, these processors will be
            the only ones used, in the order provided.

    Returns:
        tuple: (tracer, provider) instances
    """
    global _tracer_provider

    # Setup resource
    resource = resource or get_lambda_resource()
    _tracer_provider = TracerProvider(resource=resource)

    if span_processors is None:
        # Default case: Add LambdaSpanProcessor with OTLPStdoutSpanExporter
        compression_level = int(os.environ.get("OTLP_STDOUT_SPAN_EXPORTER_COMPRESSION_LEVEL", "6"))
        exporter = OTLPStdoutSpanExporter(gzip_level=compression_level)
        processor = LambdaSpanProcessor(
            exporter, max_queue_size=int(os.getenv("LAMBDA_SPAN_PROCESSOR_QUEUE_SIZE", "2048"))
        )
        _tracer_provider.add_span_processor(processor)
    else:
        # Custom case: Add user-provided processors in order
        for processor in span_processors:
            _tracer_provider.add_span_processor(processor)

    trace.set_tracer_provider(_tracer_provider)

    # Initialize extension for async and finalize modes
    if processor_mode in [ProcessorMode.ASYNC, ProcessorMode.FINALIZE]:
        init_extension(processor_mode, _tracer_provider)

    return trace.get_tracer(name), _tracer_provider
