from lambda_otel_lite import init_telemetry, traced_handler
from otlp_stdout_span_exporter import OTLPStdoutSpanExporter
import json

tracer, provider = init_telemetry(
    name="benchmark-test",
    exporter=OTLPStdoutSpanExporter(),
)


def process_level(depth: int, iterations: int) -> None:
    """
    Recursively create a tree of spans to measure OpenTelemetry overhead.

    Args:
        depth: Current depth level (decrements towards 0)
        iterations: Number of spans to create at each level
    """
    if depth <= 0:
        return

    for i in range(iterations):
        with tracer.start_as_current_span(
            f"operation_depth_{depth}_iter_{i}",
            set_status_on_exception=True,
            record_exception=True,
        ) as span:
            span.set_attributes({"depth": depth, "iteration": i})
            process_level(depth - 1, iterations)
            span.add_event("process-level-complete")


def handler(event, context):
    """
    Lambda handler that creates a tree of spans based on input parameters.

    This handler generates a tree of OpenTelemetry spans to measure overhead
    and performance characteristics. It also handles trace context propagation
    from the incoming event.

    Args:
        event: Lambda event containing depth, iterations parameters and trace context
        context: Lambda context (not used)

    Returns:
        dict: Response containing the benchmark parameters and completion status
    """
    depth = event.get("depth", 2)
    iterations = event.get("iterations", 2)

    with traced_handler(
        tracer=tracer,
        tracer_provider=provider,
        name="benchmark-execution",
        event=event,
        context=context,
    ):
        process_level(depth, iterations)
        return {
            "statusCode": 200,
            "body": json.dumps(
                {
                    "message": "Benchmark complete",
                    "depth": depth,
                    "iterations": iterations,
                }
            ),
        }
