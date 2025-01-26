from lambda_otel_lite import init_telemetry, traced_handler
import json
import base64

tracer, provider = init_telemetry("benchmark-test")


def process_record(record: dict) -> None:
    """
    Process a single Kinesis record with OpenTelemetry instrumentation.

    Args:
        record: A Kinesis record containing sequence number, partition key, and base64 encoded data
    """
    with tracer.start_as_current_span("process-record") as span:
        # Extract record metadata
        sequence_number = record.get("kinesis", {}).get("sequenceNumber", "")
        partition_key = record.get("kinesis", {}).get("partitionKey", "")

        # Set span attributes for record context
        span.set_attributes(
            {
                "stream.sequence_number": sequence_number,
                "stream.partition_key": partition_key,
            }
        )

        # Decode and process the record data
        encoded_data = record.get("kinesis", {}).get("data", "")
        with tracer.start_as_current_span("decode-record") as decode_span:
            data = base64.b64decode(encoded_data)
            decode_span.set_attribute("stream.data.size", len(data))

            # Parse JSON data
            with tracer.start_as_current_span("parse-record") as parse_span:
                json_data = json.loads(data)

                # Set business-specific attributes
                parse_span.set_attributes(
                    {
                        "stock.ticker": json_data["ticker"],
                        "stock.price": json_data["price"],
                        "stock.event_time": json_data["event_time"],
                    }
                )


def test_stream_operations(records: list) -> None:
    """
    Process a batch of Kinesis records with OpenTelemetry instrumentation.

    Args:
        records: List of Kinesis records to process
    """
    with tracer.start_as_current_span("stream-operations") as span:
        span.set_attribute("stream.batch.size", len(records))
        for record in records:
            process_record(record)


def handler(event, context):
    """
    Lambda handler that processes Kinesis records with OpenTelemetry instrumentation.

    This handler processes each record in the Kinesis event batch, creating spans
    for batch processing and individual record processing operations. Each record
    contains stock ticker data with price information.

    Args:
        event: Lambda event containing Kinesis records
        context: Lambda context (not used)

    Returns:
        dict: Response containing the number of records processed and completion status
    """
    records = event.get("Records", [])
    with traced_handler(
        tracer=tracer,
        tracer_provider=provider,
        name="benchmark-execution",
        event=event,
        context=context,
    ):
        test_stream_operations(records)
        return {
            "statusCode": 200,
            "body": json.dumps(
                {"message": "Benchmark complete", "records_processed": len(records)}
            ),
        }
