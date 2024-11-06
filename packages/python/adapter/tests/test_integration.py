import unittest
from unittest.mock import patch
import json
from opentelemetry import trace
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
from otlp_stdout_adapter import StdoutAdapter, get_lambda_resource


class TestOTLPPipeline(unittest.TestCase):
    def setUp(self):
        """Reset any cached values before each test."""
        StdoutAdapter._package_identifier = None
        StdoutAdapter._service_name = None

    def test_complete_pipeline(self):
        """
        Test the complete OpenTelemetry pipeline as shown in the README.

        Note: While the OpenTelemetry specification supports both JSON and Protobuf over HTTP,
        the Python SDK currently only supports Protobuf (see opentelemetry-python#1003).
        The environment variable OTEL_EXPORTER_OTLP_TRACES_PROTOCOL is recognized but JSON format
        is not yet implemented. All exports will use application/x-protobuf content-type.

        See: https://github.com/open-telemetry/opentelemetry-python/issues/1003
        """
        # Initialize the StdoutAdapter
        adapter = StdoutAdapter()
        session = adapter.get_session()

        captured_output = []

        # Mock print to capture the output
        with patch("builtins.print") as mock_print, patch.dict(
            "os.environ",
            {
                "OTEL_SERVICE_NAME": "test-service",
                "OTEL_EXPORTER_OTLP_ENDPOINT": "http://collector:4318/v1/traces",
                "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL": "http/protobuf",
            },
        ):
            # Set up the mock to capture printed JSON
            def capture_output(output):
                captured_output.append(json.loads(output))

            mock_print.side_effect = capture_output

            # Create OTLP exporter with custom session
            exporter = OTLPSpanExporter(
                endpoint="http://collector:4318/v1/traces", session=session
            )

            # Set up the trace provider
            provider = TracerProvider(resource=get_lambda_resource())
            processor = BatchSpanProcessor(exporter)
            provider.add_span_processor(processor)
            trace.set_tracer_provider(provider)

            # Get a tracer
            tracer = trace.get_tracer(__name__)

            # Create a test span
            with tracer.start_as_current_span("test_span") as span:
                span.set_attribute("test.attribute", "test_value")

            # Force the BatchSpanProcessor to export
            processor.force_flush()

        # Verify we captured output
        self.assertTrue(len(captured_output) > 0, "No output was captured")

        # Verify the structure of the captured output
        output = captured_output[0]
        print(
            "\nComplete output:", json.dumps(output, indent=2)
        )  # Print the entire output

        self.assertIn("__otel_otlp_stdout", output)

        # Debug print
        print(f"\nActual __otel_otlp_stdout value: {output['__otel_otlp_stdout']}")

        self.assertTrue(
            output["__otel_otlp_stdout"].startswith("otlp-stdout-adapter@"),
            f"Expected __otel_otlp_stdout to start with 'otlp-stdout-adapter@', but got: {output['__otel_otlp_stdout']}",
        )

        self.assertEqual(output["source"], "test-service")
        self.assertEqual(output["endpoint"], "http://collector:4318/v1/traces")
        self.assertEqual(output["method"], "POST")
        self.assertEqual(
            output["content-type"],
            "application/x-protobuf",
            "Python SDK currently only supports protobuf format",
        )

        # Verify the payload contains our test span
        self.assertIn("payload", output)
        payload = output["payload"]

        # Verify payload is base64 encoded
        self.assertTrue(
            output.get("base64", False), "Expected payload to be base64 encoded"
        )
        self.assertTrue(isinstance(payload, str), "Expected payload to be a string")

        # Note: We can't decode the payload as it's in protobuf format
        # If you need to verify the span data, you would need to use the protobuf library
        # to decode the payload. For this test, we'll just verify the payload exists
        self.assertTrue(len(payload) > 0, "Expected non-empty payload")
