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
Simple Hello World Lambda function using lambda-otel-lite.

This example shows basic usage with the default processor setup.
"""

import json

from opentelemetry.trace import SpanKind

from lambda_otel_lite import init_telemetry, traced_handler


def handler(event, context):
    """Simple Lambda handler that returns Hello World."""
    # Initialize with default processor (optimized for Lambda)
    tracer, provider = init_telemetry(name="hello-world")

    with traced_handler(
        tracer=tracer,
        tracer_provider=provider,
        name="hello_world",
        event=event,
        context=context,
        kind=SpanKind.SERVER,
    ) as span:
        # Extract name from event or use default
        body = event.get("body", "{}")
        if isinstance(body, str):
            try:
                body = json.loads(body)
            except json.JSONDecodeError:
                body = {}

        name = body.get("name", "World")
        message = f"Hello {name}!"

        # Add custom attribute to span
        span.set_attribute("greeting.name", name)

        return {
            "statusCode": 200,
            "body": json.dumps({"message": message}),
        }


if __name__ == "__main__":
    import os

    os.environ["AWS_LAMBDA_FUNCTION_NAME"] = "hello-world-example"
    handler({}, None)
