from lambda_otel_lite import init_telemetry, create_traced_handler
from otlp_stdout_span_exporter import OTLPStdoutSpanExporter
import json

# Initialize telemetry once at module load time
tracer, completion_handler = init_telemetry()

DEFAULT_DEPTH = 2
DEFAULT_ITERATIONS = 4

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
            span.set_attributes({"depth": depth, "iteration": i, "payload": "x" * 256})
            process_level(depth - 1, iterations)


# Create a traced handler with configuration
traced = create_traced_handler(
    name="benchmark-execution",
    completion_handler=completion_handler,
    # Using default extractor as we don't need specific HTTP attribute extraction
)


@traced
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
    depth = event.get("depth", DEFAULT_DEPTH)
    iterations = event.get("iterations", DEFAULT_ITERATIONS)

    # Now that we're using the decorator, we don't need the context manager here
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
