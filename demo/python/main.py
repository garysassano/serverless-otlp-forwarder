import os
import json
from requests import Session
from lambda_otel_lite import init_telemetry, traced_handler
from opentelemetry.instrumentation.requests import RequestsInstrumentor
from opentelemetry.trace import Status, StatusCode, get_current_span, SpanKind, Link
from opentelemetry import context, propagate, trace
import logging
from opentelemetry.propagate import get_global_textmap
from opentelemetry.propagators.textmap import Getter
from typing import Optional, List, Mapping

logger = logging.getLogger(__name__)

tracer, provider = init_telemetry("demo-function")

# instrument requests library
RequestsInstrumentor().instrument()

http_session = Session()
target_url = os.environ.get("TARGET_URL")


class Boto3SQSGetter(Getter[dict]):
    """
    Custom getter for extracting trace context from SQS message attributes.
    Implements the OpenTelemetry Getter protocol for SQS message attributes format.

    The SQS message attributes have the format:
    {
        "key": {
            "stringValue": "value",
            "dataType": "String"
        }
    }
    """

    def get(self, carrier: dict, key: str) -> Optional[List[str]]:
        """Get value from SQS message attributes for a given key."""
        msg_attr = carrier.get(key)
        if not isinstance(msg_attr, Mapping):
            return None

        value = msg_attr.get("stringValue")
        return [value] if value is not None else None

    def keys(self, carrier: dict) -> List[str]:
        """Get all keys from the carrier."""
        return list(carrier.keys())


# Create a single instance of the getter
boto3sqs_getter = Boto3SQSGetter()


def create_span_link_from_message_attributes(message_attributes: dict) -> Link | None:
    """
    Create a span link from SQS message attributes for distributed tracing.

    Args:
        message_attributes: SQS message attributes dictionary containing the trace context

    Returns:
        Link: OpenTelemetry span link if valid trace context is found, None otherwise
    """
    try:
        ctx = context.Context()
        carrier_ctx = propagate.extract(
            message_attributes, context=ctx, getter=boto3sqs_getter
        )
        span_context = trace.get_current_span(carrier_ctx).get_span_context()
        return Link(context=span_context) if span_context.is_valid else None
    except Exception as e:
        logger.warning(f"Failed to extract trace context from message attributes: {e}")
        return None


@tracer.start_as_current_span("save_quote")
def save_quote(quote: dict):
    """
    Save a quote to the backend service.
    """
    span = get_current_span()
    span.add_event(
        "Saving quote",
        {
            "log.severity": "info",
            "log.message": "Sending quote to backend service",
            "quote.text": quote.get("quote", ""),
            "quote.author": quote.get("author", ""),
        },
    )

    response = http_session.post(
        target_url,
        json=quote,
        headers={
            "content-type": "application/json",
        },
    )
    response.raise_for_status()

    span.add_event(
        "Quote saved",
        {
            "log.severity": "info",
            "log.message": "Successfully saved quote to backend",
            "quote.id": str(quote.get("id", "")),
            "http.status_code": str(response.status_code),
        },
    )

    return response.json()


def lambda_handler(event, lambda_context):
    """
    Lambda handler that processes SQS events containing quotes.
    Creates a parent span for the batch and individual child spans for each message,
    with each child span linked to its original trace context.

    Args:
        event: Lambda event containing SQS records
        lambda_context: Lambda context

    Returns:
        dict: Response with status code 200 and processing results
    """
    with traced_handler(
        tracer=tracer,
        tracer_provider=provider,
        name="process-quotes",
        event=event,
        context=lambda_context,
        kind=SpanKind.CONSUMER,
    ) as parent_span:
        parent_span.add_event(
            "Processing started",
            {
                "log.severity": "info",
                "log.message": "Started processing SQS messages",
                "batch.size": len(event.get("Records", [])),
            },
        )

        results = []
        for record in event.get("Records", []):
            message_attributes = record.get("messageAttributes", {})
            message_body = record.get("body", "{}")
            quote = json.loads(message_body)
            message_id = record.get("messageId")

            # Extract trace context and create link for this specific message
            carrier_ctx = propagate.extract(message_attributes, getter=boto3sqs_getter)
            span_context = trace.get_current_span(carrier_ctx).get_span_context()

            links = [Link(context=span_context)] if span_context.is_valid else []
            with tracer.start_as_current_span(
                "process-quote", kind=SpanKind.CONSUMER, links=links
            ) as record_span:
                record_span.set_attribute("messaging.message_id", message_id)
                record_span.add_event(
                    "Processing quote",
                    {
                        "log.severity": "info",
                        "log.message": "Processing individual quote from batch",
                        "quote.text": quote.get("quote", ""),
                        "quote.author": quote.get("author", ""),
                    },
                )

                # Save the quote
                result = save_quote(quote)
                results.append(result)

        parent_span.add_event(
            "Processing completed",
            {
                "log.severity": "info",
                "log.message": f"Successfully processed {len(results)} quotes",
            },
        )

        return {
            "statusCode": 200,
            "body": json.dumps(
                {"message": "Quote processing complete", "processed": len(results)}
            ),
        }
