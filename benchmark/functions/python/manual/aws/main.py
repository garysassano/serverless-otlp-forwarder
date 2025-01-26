from opentelemetry.instrumentation.botocore import BotocoreInstrumentor
from opentelemetry.propagate import extract
from lambda_otel_lite import init_telemetry, traced_handler
import json
import boto3

tracer, provider = init_telemetry("benchmark-test")
BotocoreInstrumentor().instrument()

# Initialize clients
s3_client = boto3.client("s3")
dynamodb = boto3.client("dynamodb")


def test_aws_operations() -> None:
    """
    Executes AWS SDK operations (S3 and DynamoDB) with OpenTelemetry instrumentation.
    """
    with tracer.start_as_current_span("aws-operations"):
        # Test S3 operations
        with tracer.start_as_current_span("s3-operations") as s3_span:
            s3_client.list_buckets()
            s3_span.set_attribute("aws.operation", "list_buckets")

        # Test DynamoDB operations
        with tracer.start_as_current_span("dynamodb-operations") as dynamo_span:
            dynamodb.list_tables(Limit=10)
            dynamo_span.set_attribute("aws.operation", "list_tables")


def handler(event, context):
    """
    Lambda handler that exercises AWS SDK operations with OpenTelemetry instrumentation.

    This handler executes S3 and DynamoDB operations to generate telemetry spans. It uses
    the BotocoreInstrumentor to automatically instrument AWS SDK calls.

    Args:
        event: Lambda event (not used)
        context: Lambda context (not used)

    Returns:
        dict: Response containing status code and completion message
    """

    with traced_handler(
        tracer=tracer,
        tracer_provider=provider,
        name="benchmark-execution",
        event=event,
        context=context
    ):
        test_aws_operations()
        return {
            "statusCode": 200,
            "body": json.dumps({"message": "Benchmark complete"}),
        }
