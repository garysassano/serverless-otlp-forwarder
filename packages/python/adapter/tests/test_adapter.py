import unittest
from unittest.mock import patch, MagicMock
import json
import gzip
import base64
from otlp_stdout_adapter import StdoutAdapter, get_lambda_resource


class TestStdoutAdapter(unittest.TestCase):
    def setUp(self):
        """Set up test fixtures."""
        self.adapter = StdoutAdapter()
        self.base_request = MagicMock()
        self.base_request.url = "http://example.com"
        self.base_request.method = "POST"
        # Reset the cached service name
        StdoutAdapter._service_name = None

    def test_get_session(self):
        """Test session singleton behavior."""
        session1 = StdoutAdapter.get_session()
        session2 = StdoutAdapter.get_session()
        self.assertIsNotNone(session1)
        self.assertIs(session1, session2)

    def test_get_service_name_from_env(self):
        """Test service name resolution from environment variables."""
        with patch.dict("os.environ", {"OTEL_SERVICE_NAME": "test-service"}):
            self.assertEqual(StdoutAdapter.get_service_name(), "test-service")

        # Reset cached value before testing next case
        StdoutAdapter._service_name = None

        with patch.dict("os.environ", {"AWS_LAMBDA_FUNCTION_NAME": "lambda-function"}):
            self.assertEqual(StdoutAdapter.get_service_name(), "lambda-function")

        # Reset cached value before testing next case
        StdoutAdapter._service_name = None

        with patch.dict("os.environ", clear=True):
            self.assertEqual(StdoutAdapter.get_service_name(), "unknown-service")

    @patch("builtins.print")
    def test_send_json_uncompressed(self, mock_print):
        """Test sending uncompressed JSON payload."""
        test_data = {"test": "data"}
        self.base_request.headers = {"Content-Type": "application/json"}
        self.base_request.body = json.dumps(test_data).encode("utf-8")

        response = self.adapter.send(self.base_request)

        self.assertEqual(response.status_code, 200)
        printed_data = json.loads(mock_print.call_args[0][0])
        self.assertEqual(printed_data["payload"], test_data)
        self.assertFalse(printed_data["base64"])

    @patch("builtins.print")
    def test_send_json_compressed_input(self, mock_print):
        """Test sending compressed JSON payload."""
        test_data = {"test": "data"}
        compressed_data = gzip.compress(json.dumps(test_data).encode("utf-8"))
        self.base_request.headers = {
            "Content-Type": "application/json",
            "Content-Encoding": "gzip",
        }
        self.base_request.body = compressed_data

        response = self.adapter.send(self.base_request)

        self.assertEqual(response.status_code, 200)
        printed_data = json.loads(mock_print.call_args[0][0])
        self.assertEqual(printed_data["payload"], test_data)
        self.assertFalse(printed_data["base64"])

    @patch.dict("os.environ", {"OTEL_EXPORTER_OTLP_COMPRESSION": "gzip"})
    @patch("builtins.print")
    def test_send_json_compressed_output(self, mock_print):
        """Test sending JSON with compressed output."""
        test_data = {"test": "data"}
        self.base_request.headers = {"Content-Type": "application/json"}
        self.base_request.body = json.dumps(test_data).encode("utf-8")

        response = self.adapter.send(self.base_request)

        self.assertEqual(response.status_code, 200)
        printed_data = json.loads(mock_print.call_args[0][0])
        self.assertTrue(printed_data["base64"])
        self.assertEqual(printed_data["content-encoding"], "gzip")

        # Verify we can decode and decompress the payload back to original
        decoded = base64.b64decode(printed_data["payload"])
        decompressed = json.loads(gzip.decompress(decoded))
        self.assertEqual(decompressed, test_data)

    @patch("builtins.print")
    def test_send_protobuf(self, mock_print):
        """Test sending protobuf payload."""
        test_data = b"\x08\x96\x01\x12\x09test data"
        self.base_request.headers = {"Content-Type": "application/x-protobuf"}
        self.base_request.body = test_data

        response = self.adapter.send(self.base_request)

        self.assertEqual(response.status_code, 200)
        printed_data = json.loads(mock_print.call_args[0][0])
        self.assertTrue(printed_data["base64"])
        decoded = base64.b64decode(printed_data["payload"])
        self.assertEqual(decoded, test_data)

    @patch.dict("os.environ", {"OTEL_EXPORTER_OTLP_COMPRESSION": "gzip"})
    @patch("builtins.print")
    def test_send_protobuf_compressed(self, mock_print):
        """Test sending protobuf with compression."""
        test_data = b"\x08\x96\x01\x12\x09test data"
        self.base_request.headers = {"Content-Type": "application/x-protobuf"}
        self.base_request.body = test_data

        _response = self.adapter.send(self.base_request)

        printed_data = json.loads(mock_print.call_args[0][0])
        self.assertTrue(printed_data["base64"])
        self.assertEqual(printed_data["content-encoding"], "gzip")
        decoded = base64.b64decode(printed_data["payload"])
        decompressed = gzip.decompress(decoded)
        self.assertEqual(decompressed, test_data)

    def test_send_invalid_content_type(self):
        """Test sending unsupported content type."""
        self.base_request.headers = {"Content-Type": "text/plain"}
        self.base_request.body = b"test"

        with self.assertRaises(ValueError) as context:
            self.adapter.send(self.base_request)
        self.assertIn("Unsupported content type", str(context.exception))

    def test_send_missing_content_type(self):
        """Test sending request without content type."""
        self.base_request.headers = {}
        self.base_request.body = b"test"

        with self.assertRaises(ValueError) as context:
            self.adapter.send(self.base_request)
        self.assertIn("Content-Type header is required", str(context.exception))

    def test_output_structure(self):
        """Test the structure of the JSON output record."""
        # Reset cached values
        StdoutAdapter._package_identifier = None
        StdoutAdapter._service_name = None

        # Create a basic request with JSON content
        request = self.base_request
        request.headers = {"Content-Type": "application/json"}
        request.body = b'{"test": "data"}'

        # Capture stdout to verify the JSON structure
        with patch("builtins.print") as mock_print:
            self.adapter.send(request)

            # Get the JSON that would have been printed
            output_call = mock_print.call_args[0][0]
            output = json.loads(output_call)

            # Verify the structure
            self.assertIn("__otel_otlp_stdout", output)
            self.assertTrue(
                output["__otel_otlp_stdout"].startswith("otlp-stdout-adapter@")
            )

            self.assertIn("source", output)
            self.assertEqual(output["source"], "unknown-service")

            self.assertIn("endpoint", output)
            self.assertEqual(output["endpoint"], "http://example.com")

            self.assertIn("method", output)
            self.assertEqual(output["method"], "POST")

            # Verify other required fields
            self.assertIn("payload", output)
            self.assertIn("headers", output)
            self.assertIn("content-type", output)
            self.assertIn("base64", output)

    def test_content_type_from_protocol(self):
        """Test content type is set correctly based on OTEL_EXPORTER_OTLP_PROTOCOL."""
        # Reset cached values
        StdoutAdapter._package_identifier = None
        StdoutAdapter._service_name = None

        request = self.base_request
        request.body = b'{"test": "data"}'

        test_cases = [
            ("http/json", "application/json"),
            ("http/protobuf", "application/x-protobuf"),
        ]

        for protocol, expected_content_type in test_cases:
            with self.subTest(protocol=protocol):
                with patch.dict(
                    "os.environ", {"OTEL_EXPORTER_OTLP_PROTOCOL": protocol}
                ), patch("builtins.print") as mock_print:
                    request.headers = {"Content-Type": expected_content_type}
                    self.adapter.send(request)

                    output = json.loads(mock_print.call_args[0][0])
                    self.assertEqual(output["content-type"], expected_content_type)


class TestGetLambdaResource(unittest.TestCase):
    @patch.dict(
        "os.environ",
        {
            "AWS_REGION": "us-west-2",
            "AWS_LAMBDA_FUNCTION_NAME": "test-function",
            "AWS_LAMBDA_FUNCTION_VERSION": "1",
            "AWS_LAMBDA_LOG_STREAM_NAME": "test-stream",
        },
    )
    def test_get_lambda_resource(self):
        """Test Lambda resource attributes."""
        resource = get_lambda_resource()
        self.assertIsNotNone(resource)
        attributes = dict(resource.attributes)
        self.assertEqual(attributes["cloud.region"], "us-west-2")
        self.assertEqual(attributes["cloud.provider"], "aws")
        self.assertEqual(attributes["faas.name"], "test-function")
        self.assertEqual(attributes["faas.version"], "1")
        self.assertEqual(attributes["faas.instance"], "test-stream")

    @patch.dict("os.environ", clear=True)
    def test_get_lambda_resource_missing_env(self):
        """Test Lambda resource with missing environment variables."""
        resource = get_lambda_resource()
        attributes = dict(resource.attributes)
        self.assertEqual(attributes["cloud.region"], "")
        self.assertEqual(attributes["cloud.provider"], "aws")
        self.assertEqual(attributes["faas.name"], "")
        self.assertEqual(attributes["faas.version"], "")
        self.assertEqual(attributes["faas.instance"], "")


if __name__ == "__main__":
    unittest.main()
