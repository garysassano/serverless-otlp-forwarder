"""Tests for the traced_handler implementation."""

import os
from collections.abc import Generator
from dataclasses import dataclass
from typing import Any, cast
from unittest.mock import Mock, patch

import pytest
from opentelemetry import context as context_api
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.trace import (
    NonRecordingSpan,
    SpanContext,
    SpanKind,
    TraceFlags,
    set_span_in_context,
)
from opentelemetry.trace.propagation.tracecontext import TraceContextTextMapPropagator

from lambda_otel_lite import ProcessorMode
from lambda_otel_lite.handler import _extract_span_attributes, traced_handler


@dataclass
class MockLambdaContext:
    """Mock AWS Lambda context."""

    invoked_function_arn: str = "arn:aws:lambda:us-west-2:123456789012:function:test-function"
    aws_request_id: str = "test-request-id"


@pytest.fixture
def mock_tracer() -> trace.Tracer:
    """Create a mock tracer."""
    tracer = Mock(spec=trace.Tracer)
    span = Mock()
    context_manager = Mock()
    context_manager.__enter__ = Mock(return_value=span)
    context_manager.__exit__ = Mock(return_value=None)
    tracer.start_as_current_span.return_value = context_manager
    return tracer


@pytest.fixture
def mock_provider() -> TracerProvider:
    """Create a mock tracer provider."""
    provider = Mock(spec=TracerProvider)
    provider.force_flush.return_value = None
    return provider


@pytest.fixture
def mock_env() -> Generator[dict[str, str], None, None]:
    """Set up mock environment variables."""
    env_vars = {
        "AWS_LAMBDA_REQUEST_ID": "test-request-id",
    }
    with patch.dict(os.environ, env_vars):
        yield env_vars


@pytest.fixture
def mock_context() -> MockLambdaContext:
    """Create a mock Lambda context."""
    return MockLambdaContext()


def test_traced_handler_sync_mode(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
    mock_context: MockLambdaContext,
) -> None:
    """Test traced_handler in sync mode."""
    with patch("lambda_otel_lite.processor_mode", ProcessorMode.SYNC):
        with traced_handler(mock_tracer, mock_provider, "test_handler", context=mock_context):
            pass

        # Verify span creation with basic attributes
        mock_tracer = cast(Mock, mock_tracer)
        mock_tracer.start_as_current_span.assert_called_once_with(
            "test_handler",
            context=None,
            kind=SpanKind.SERVER,
            attributes={
                "faas.invocation_id": mock_env["AWS_LAMBDA_REQUEST_ID"],
                "cloud.resource_id": mock_context.invoked_function_arn,
                "cloud.account.id": "123456789012",
                "faas.trigger": "other",
            },
            links=None,
            start_time=None,
            record_exception=True,
            set_status_on_exception=True,
            end_on_exit=True,
        )

        # Verify force flush in sync mode
        mock_provider = cast(Mock, mock_provider)
        mock_provider.force_flush.assert_called_once()


def test_traced_handler_async_mode(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
) -> None:
    """Test traced_handler in async mode."""
    with (
        patch("lambda_otel_lite.handler.processor_mode", ProcessorMode.ASYNC),
        patch("lambda_otel_lite.handler.handler_complete_event") as mock_complete,
    ):
        with traced_handler(mock_tracer, mock_provider, "test_handler"):
            # Handler should not have signaled completion yet
            mock_complete = cast(Mock, mock_complete)
            mock_complete.set.assert_not_called()

        # Verify completion signal
        mock_complete.set.assert_called_once()
        # No force flush in async mode
        mock_provider = cast(Mock, mock_provider)
        mock_provider.force_flush.assert_not_called()


def test_traced_handler_finalize_mode(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
) -> None:
    """Test traced_handler in finalize mode."""
    with (
        patch("lambda_otel_lite.handler.processor_mode", ProcessorMode.FINALIZE),
        patch("lambda_otel_lite.handler.handler_complete_event") as mock_complete,
    ):
        with traced_handler(mock_tracer, mock_provider, "test_handler"):
            # No completion signal during execution
            mock_complete = cast(Mock, mock_complete)
            mock_complete.set.assert_not_called()

        # No completion signal in finalize mode
        mock_complete.set.assert_not_called()
        # No force flush in finalize mode
        mock_provider = cast(Mock, mock_provider)
        mock_provider.force_flush.assert_not_called()


def test_traced_handler_cold_start(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
) -> None:
    """Test cold start attribute setting."""
    with (
        patch("lambda_otel_lite.processor_mode", ProcessorMode.SYNC),
        patch("lambda_otel_lite.handler._is_cold_start", True),
    ):
        with traced_handler(mock_tracer, mock_provider, "test_handler"):
            mock_tracer = cast(Mock, mock_tracer)
            span = mock_tracer.start_as_current_span.return_value.__enter__.return_value
            span = cast(Mock, span)
            span.set_attribute.assert_called_once_with("faas.cold_start", True)


def test_traced_handler_not_cold_start(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
) -> None:
    """Test no cold start attribute after first invocation."""
    with (
        patch("lambda_otel_lite.processor_mode", ProcessorMode.SYNC),
        patch("lambda_otel_lite.handler._is_cold_start", False),
    ):
        with traced_handler(mock_tracer, mock_provider, "test_handler"):
            mock_tracer = cast(Mock, mock_tracer)
            span = mock_tracer.start_as_current_span.return_value.__enter__.return_value
            span = cast(Mock, span)
            span.set_attribute.assert_not_called()


def test_traced_handler_with_attributes(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
    mock_context: MockLambdaContext,
) -> None:
    """Test traced_handler with custom attributes."""
    attributes = {"test.key": "test.value"}

    with patch("lambda_otel_lite.processor_mode", ProcessorMode.SYNC):
        with traced_handler(
            mock_tracer, mock_provider, "test_handler", context=mock_context, attributes=attributes
        ):
            pass

        # Verify custom attributes are merged with span attributes
        expected_attributes = {
            "faas.invocation_id": mock_env["AWS_LAMBDA_REQUEST_ID"],
            "cloud.resource_id": mock_context.invoked_function_arn,
            "cloud.account.id": "123456789012",
            "test.key": "test.value",
            "faas.trigger": "other",
        }

        mock_tracer = cast(Mock, mock_tracer)
        mock_tracer.start_as_current_span.assert_called_once_with(
            "test_handler",
            context=None,
            kind=SpanKind.SERVER,
            attributes=expected_attributes,
            links=None,
            start_time=None,
            record_exception=True,
            set_status_on_exception=True,
            end_on_exit=True,
        )


def test_traced_handler_with_http_trigger(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
    mock_context: MockLambdaContext,
) -> None:
    """Test traced_handler with HTTP event."""
    event: dict[str, Any] = {
        "httpMethod": "POST",
    }

    with patch("lambda_otel_lite.processor_mode", ProcessorMode.SYNC):
        with traced_handler(mock_tracer, mock_provider, "test_handler", event, mock_context):
            pass

        expected_attributes = {
            "faas.invocation_id": mock_env["AWS_LAMBDA_REQUEST_ID"],
            "cloud.resource_id": mock_context.invoked_function_arn,
            "cloud.account.id": "123456789012",
            "faas.trigger": "http",
            "http.method": "POST",
            "http.route": "",
            "http.target": "",
        }

        mock_tracer = cast(Mock, mock_tracer)
        mock_tracer.start_as_current_span.assert_called_once_with(
            "test_handler",
            context=None,
            kind=SpanKind.SERVER,
            attributes=expected_attributes,
            links=None,
            start_time=None,
            record_exception=True,
            set_status_on_exception=True,
            end_on_exit=True,
        )


def test_traced_handler_with_invalid_arn(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
) -> None:
    """Test traced_handler with invalid function ARN."""
    context = MockLambdaContext(invoked_function_arn="invalid:arn")

    with patch("lambda_otel_lite.processor_mode", ProcessorMode.SYNC):
        with traced_handler(mock_tracer, mock_provider, "test_handler", context=context):
            pass

        # Should only set invocation_id and faas.trigger when ARN is invalid
        mock_tracer = cast(Mock, mock_tracer)
        mock_tracer.start_as_current_span.assert_called_once_with(
            "test_handler",
            context=None,
            kind=SpanKind.SERVER,
            attributes={
                "faas.invocation_id": mock_env["AWS_LAMBDA_REQUEST_ID"],
                "faas.trigger": "other",
            },
            links=None,
            start_time=None,
            record_exception=True,
            set_status_on_exception=True,
            end_on_exit=True,
        )


def test_traced_handler_with_http_headers_context(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
) -> None:
    """Test traced_handler with context in HTTP headers."""
    # Create a parent context
    parent_context = SpanContext(
        trace_id=0x12345678901234567890123456789012,
        span_id=0x1234567890123456,
        is_remote=True,
        trace_flags=TraceFlags(TraceFlags.SAMPLED),
    )
    parent_span = NonRecordingSpan(parent_context)
    parent_carrier: dict[str, str] = {}
    TraceContextTextMapPropagator().inject(parent_carrier, set_span_in_context(parent_span))

    event = {
        "headers": parent_carrier,
    }

    with patch("lambda_otel_lite.processor_mode", ProcessorMode.SYNC):
        with traced_handler(mock_tracer, mock_provider, "test_handler", event):
            pass

        # Verify span creation with parent context
        mock_tracer = cast(Mock, mock_tracer)
        mock_tracer.start_as_current_span.assert_called_once()
        call_args = mock_tracer.start_as_current_span.call_args
        assert call_args[1]["context"] is not None


def test_traced_handler_with_invalid_carrier(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
    mock_context: MockLambdaContext,
) -> None:
    """Test traced_handler with invalid context carrier."""

    def invalid_extractor(event: dict[str, Any]) -> dict[str, str]:
        carrier: dict[str, str] = {}
        return carrier

    with patch("lambda_otel_lite.processor_mode", ProcessorMode.SYNC):
        with traced_handler(
            mock_tracer,
            mock_provider,
            "test_handler",
            {"headers": {}},
            context=mock_context,
            get_carrier=invalid_extractor,
        ):
            pass

        # Verify span creation without parent context
        mock_tracer = cast(Mock, mock_tracer)
        mock_tracer.start_as_current_span.assert_called_once_with(
            "test_handler",
            context=None,
            kind=SpanKind.SERVER,
            attributes={
                "faas.invocation_id": mock_env["AWS_LAMBDA_REQUEST_ID"],
                "faas.trigger": "other",
                "cloud.resource_id": mock_context.invoked_function_arn,
                "cloud.account.id": "123456789012",
            },
            links=None,
            start_time=None,
            record_exception=True,
            set_status_on_exception=True,
            end_on_exit=True,
        )


def test_traced_handler_parent_context_precedence(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
) -> None:
    """Test that explicit parent context takes precedence over extracted context."""
    # Create a parent context
    parent_context = SpanContext(
        trace_id=0x12345678901234567890123456789012,
        span_id=0x1234567890123456,
        is_remote=True,
        trace_flags=TraceFlags(TraceFlags.SAMPLED),
    )
    parent_span = NonRecordingSpan(parent_context)
    parent_carrier: dict[str, str] = {}
    TraceContextTextMapPropagator().inject(parent_carrier, set_span_in_context(parent_span))

    # Create a different context in the event
    event_context = SpanContext(
        trace_id=0x98765432109876543210987654321098,
        span_id=0x9876543210987654,
        is_remote=True,
        trace_flags=TraceFlags(TraceFlags.SAMPLED),
    )
    event_span = NonRecordingSpan(event_context)
    event: dict[str, Any] = {"headers": {}}
    TraceContextTextMapPropagator().inject(event["headers"], set_span_in_context(event_span))

    with patch("lambda_otel_lite.processor_mode", ProcessorMode.SYNC):
        with traced_handler(
            mock_tracer,
            mock_provider,
            "test_handler",
            event,
            parent_context=context_api.Context(),
        ):
            pass

        # Verify span creation with explicit parent context
        mock_tracer = cast(Mock, mock_tracer)
        mock_tracer.start_as_current_span.assert_called_once()
        call_args = mock_tracer.start_as_current_span.call_args
        assert call_args[1]["context"] is not None


def test_traced_handler_no_context_extraction(
    mock_tracer: trace.Tracer,
    mock_provider: TracerProvider,
    mock_env: dict[str, str],
) -> None:
    """Test traced_handler with context extraction disabled."""
    # Create a parent context that should be extracted
    event = {
        "headers": {
            "traceparent": "00-12345678901234567890123456789012-1234567890123456-01",
        },
    }

    with patch("lambda_otel_lite.processor_mode", ProcessorMode.SYNC):
        with traced_handler(
            mock_tracer,
            mock_provider,
            "test_handler",
            event,
            get_carrier=None,  # Should fall back to headers extraction
        ):
            pass

        # Verify span creation with extracted parent context
        mock_tracer = cast(Mock, mock_tracer)
        call_args = mock_tracer.start_as_current_span.call_args

        # Context should be extracted from headers
        assert call_args.kwargs["context"] is not None
        assert call_args.kwargs["kind"] == SpanKind.SERVER
        assert call_args.kwargs["attributes"]["faas.trigger"] == "other"
        assert call_args.kwargs["links"] is None
        assert call_args.kwargs["start_time"] is None
        assert call_args.kwargs["record_exception"] is True
        assert call_args.kwargs["set_status_on_exception"] is True
        assert call_args.kwargs["end_on_exit"] is True


def test_extract_span_attributes_with_context(mock_context: MockLambdaContext) -> None:
    """Test span attribute extraction with context."""
    attributes = _extract_span_attributes(None, mock_context)

    assert attributes["faas.invocation_id"] == mock_context.aws_request_id
    assert attributes["cloud.resource_id"] == mock_context.invoked_function_arn
    assert attributes["cloud.account.id"] == "123456789012"
    assert attributes["faas.trigger"] == "other"


def test_extract_span_attributes_without_context() -> None:
    """Test span attribute extraction without context."""
    attributes = _extract_span_attributes(None, None)

    assert "faas.invocation_id" not in attributes
    assert "cloud.resource_id" not in attributes
    assert "cloud.account.id" not in attributes
    assert attributes["faas.trigger"] == "other"


def test_extract_span_attributes_with_partial_context(mock_context: MockLambdaContext) -> None:
    """Test span attribute extraction with partial context."""
    mock_context.invoked_function_arn = "invalid:arn"
    attributes = _extract_span_attributes(None, mock_context)

    assert attributes["faas.invocation_id"] == mock_context.aws_request_id
    assert "cloud.resource_id" not in attributes
    assert "cloud.account.id" not in attributes
    assert attributes["faas.trigger"] == "other"


def test_extract_span_attributes_without_event() -> None:
    """Test span attribute extraction without event."""
    attributes = _extract_span_attributes(None, None)
    assert attributes["faas.trigger"] == "other"


def test_extract_span_attributes_with_simple_event() -> None:
    """Test span attribute extraction with simple event."""
    event = {"key": "value"}
    attributes = _extract_span_attributes(event, None)
    assert attributes["faas.trigger"] == "other"
