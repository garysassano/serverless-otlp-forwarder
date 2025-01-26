"""Tests for the telemetry module."""

import os
from unittest.mock import patch

import pytest
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.trace import Tracer

from lambda_otel_lite.telemetry import get_lambda_resource, init_telemetry


@pytest.fixture
def mock_env():
    """Fixture to provide a clean environment for each test."""
    from opentelemetry import trace
    from opentelemetry.util._once import Once

    # Clear any existing tracer provider
    trace._TRACER_PROVIDER = None
    trace._TRACER_PROVIDER_SET_ONCE = Once()
    with patch.dict(os.environ, {}, clear=True):
        yield


def test_get_lambda_resource_basic(mock_env):
    """Test basic Lambda resource creation with minimal environment."""
    os.environ.update(
        {
            "AWS_LAMBDA_FUNCTION_NAME": "test-function",
            "AWS_REGION": "us-west-2",
        }
    )

    resource = get_lambda_resource()
    assert isinstance(resource, Resource)
    attrs = resource.attributes

    assert attrs["service.name"] == "test-function"
    assert attrs["faas.name"] == "test-function"
    assert attrs["cloud.provider"] == "aws"
    assert attrs["cloud.region"] == "us-west-2"


def test_get_lambda_resource_with_otel_service_name(mock_env):
    """Test that OTEL_SERVICE_NAME overrides AWS_LAMBDA_FUNCTION_NAME for service.name."""
    os.environ.update(
        {
            "AWS_LAMBDA_FUNCTION_NAME": "test-function",
            "OTEL_SERVICE_NAME": "custom-service",
        }
    )

    resource = get_lambda_resource()
    assert resource.attributes["service.name"] == "custom-service"
    assert resource.attributes["faas.name"] == "test-function"


def test_get_lambda_resource_with_resource_attributes(mock_env):
    """Test that OTEL_RESOURCE_ATTRIBUTES are properly parsed."""
    os.environ.update(
        {
            "AWS_LAMBDA_FUNCTION_NAME": "test-function",
            "OTEL_RESOURCE_ATTRIBUTES": "key1=value1,key2=value2,custom.provider=gcp",
        }
    )

    resource = get_lambda_resource()
    attrs = resource.attributes

    assert attrs["key1"] == "value1"
    assert attrs["key2"] == "value2"
    assert attrs["custom.provider"] == "gcp"
    # Lambda attributes should remain unchanged
    assert attrs["cloud.provider"] == "aws"


def test_get_lambda_resource_with_url_encoded_attributes(mock_env):
    """Test that URL-encoded values in OTEL_RESOURCE_ATTRIBUTES are properly decoded."""
    os.environ.update(
        {
            "AWS_LAMBDA_FUNCTION_NAME": "test-function",
            "OTEL_RESOURCE_ATTRIBUTES": "deployment.env=prod%20env,custom.name=my%20service",
        }
    )

    resource = get_lambda_resource()
    attrs = resource.attributes

    assert attrs["deployment.env"] == "prod env"
    assert attrs["custom.name"] == "my service"
    assert attrs["service.name"] == "test-function"


def test_init_telemetry_basic(mock_env):
    """Test basic telemetry initialization."""
    os.environ["AWS_LAMBDA_FUNCTION_NAME"] = "test-function"

    tracer, provider = init_telemetry("test-service")

    assert isinstance(tracer, Tracer)
    assert isinstance(provider, TracerProvider)
    assert provider.resource.attributes["service.name"] == "test-function"


def test_init_telemetry_with_custom_resource():
    """Test telemetry initialization with a custom resource."""
    custom_resource = Resource.create({"custom.attr": "value"})

    tracer, provider = init_telemetry("test-service", resource=custom_resource)

    assert provider.resource.attributes["custom.attr"] == "value"


def test_get_lambda_resource_no_service_name(mock_env):
    """Test that service.name defaults to 'unknown_service' when no name is provided."""
    resource = get_lambda_resource()
    attrs = resource.attributes

    assert attrs["service.name"] == "unknown_service"
    assert "faas.name" not in attrs
    assert attrs["cloud.provider"] == "aws"


def test_get_lambda_resource_only_otel_service_name(mock_env):
    """Test that when only OTEL_SERVICE_NAME is set, faas.name is not set."""
    os.environ["OTEL_SERVICE_NAME"] = "custom-service"

    resource = get_lambda_resource()
    attrs = resource.attributes

    assert attrs["service.name"] == "custom-service"
    assert "faas.name" not in attrs
    assert attrs["cloud.provider"] == "aws"


def test_init_telemetry_with_custom_processor(mock_env):
    """Test telemetry initialization with a custom processor."""
    from opentelemetry.sdk.trace import SpanProcessor

    class CustomProcessor(SpanProcessor):
        def __init__(self):
            self.was_used = False

        def on_start(self, span, parent_context):
            self.was_used = True

        def on_end(self, span):
            pass

        def shutdown(self):
            pass

        def force_flush(self, timeout_millis=30000):
            pass

    custom_processor = CustomProcessor()
    tracer, provider = init_telemetry("test-service", span_processors=[custom_processor])

    # Create a span to trigger the processor
    with tracer.start_span("test"):
        pass

    # Verify the custom processor was used
    assert custom_processor.was_used
