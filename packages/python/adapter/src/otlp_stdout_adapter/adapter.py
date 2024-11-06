import json
import os
import base64
import gzip
from requests.adapters import HTTPAdapter
from requests import Session as RequestsSession
from requests.models import Response
from importlib.metadata import version, PackageNotFoundError
from opentelemetry.sdk.resources import Resource
from opentelemetry.semconv.resource import ResourceAttributes


class StdoutAdapter(HTTPAdapter):
    """
    A custom HTTP adapter that writes OpenTelemetry OTLP data to stdout.

    Note: While the OpenTelemetry specification supports both JSON and Protobuf over HTTP,
    the Python SDK currently only supports Protobuf. The environment variable
    OTEL_EXPORTER_OTLP_TRACES_PROTOCOL is recognized but JSON format is not yet implemented.
    All exports will use application/x-protobuf content-type.
    """

    _session = None
    _service_name = None
    _package_identifier = None

    @classmethod
    def get_package_identifier(cls):
        """
        Get the package identifier, initializing it if it hasn't been set yet.

        Returns:
            str: The package identifier in the format 'package-name@version'.
        """
        if cls._package_identifier is None:
            package_name = __package__.replace("_", "-")
            try:
                cls._package_identifier = f"{package_name}@{version(package_name)}"
            except PackageNotFoundError:
                cls._package_identifier = f"{package_name}@unknown"
        return cls._package_identifier

    @classmethod
    def get_session(cls):
        """
        Get the session, initializing it if it hasn't been set yet.

        Returns:
            RequestsSession: The session.
        """
        if cls._session is None:
            cls._session = RequestsSession()
            cls._session.mount("http://", cls())
            cls._session.mount("https://", cls())
        return cls._session

    @classmethod
    def get_service_name(cls):
        """
        Get the service name, initializing it if it hasn't been set yet.

        Returns:
            str: The service name.
        """
        if cls._service_name is None:
            cls._service_name = os.environ.get("OTEL_SERVICE_NAME") or os.environ.get(
                "AWS_LAMBDA_FUNCTION_NAME", "unknown-service"
            )
        return cls._service_name

    def send(self, request, **kwargs):
        """
        Send a request to the stdout adapter.

        This method processes the request and writes the telemetry data to stdout in a
        structured JSON format. It handles both JSON and binary payloads, with support
        for GZIP compression.

        Logic flow:
        1. For JSON payloads:
           - If input is gzipped, decompress it
           - Always parse the JSON
           - Only base64 encode if we're going to compress it for output

        2. For non-JSON payloads (protobuf):
           - Always base64 encode (since it's binary)
           - Keep the original payload as-is
           - If output compression is enabled, compress it

        3. For all payloads:
           - If output compression is enabled, compress and mark for base64 encoding
           - Base64 encode if either:
             - It's a binary payload (protobuf)
             - We compressed it for output

        Args:
            request: The request to send.
            **kwargs: Additional keyword arguments.

        Returns:
            Response: A Response object with status code 200.

        Raises:
            ValueError: If the content type is not supported.
        """
        output = {
            "__otel_otlp_stdout": self.get_package_identifier(),
            "source": self.get_service_name(),
            "endpoint": request.url,
            "method": request.method,
        }

        is_gzip = False
        compress = (
            os.environ.get("OTEL_EXPORTER_OTLP_COMPRESSION", "").lower() == "gzip"
        )

        normalized_headers = {
            header.lower(): value for header, value in request.headers.items()
        }
        content_type = normalized_headers.get("content-type")
        content_encoding = normalized_headers.get("content-encoding")
        output["headers"] = normalized_headers

        if content_encoding:
            content_encoding = content_encoding.lower()
            if content_encoding == "gzip":
                output["content-encoding"] = "gzip"
                is_gzip = True

        if not content_type:
            raise ValueError("Content-Type header is required")

        content_type = content_type.lower()
        output["content-type"] = content_type

        if content_type == "application/json":
            if request.body:
                payload = (
                    gzip.decompress(request.body).decode("utf-8")
                    if is_gzip
                    else request.body.decode("utf-8")
                )
                payload = json.loads(payload)

                if compress:
                    compressed_payload = gzip.compress(
                        json.dumps(payload).encode("utf-8")
                    )
                    output["payload"] = base64.b64encode(compressed_payload).decode(
                        "utf-8"
                    )
                    output["base64"] = True
                    output["content-encoding"] = "gzip"
                else:
                    output["payload"] = payload
                    output["base64"] = False
        elif content_type == "application/x-protobuf":
            payload = request.body

            if compress:
                if not is_gzip:
                    payload = gzip.compress(payload)
                output["content-encoding"] = "gzip"

            output["payload"] = base64.b64encode(payload).decode("utf-8")
            output["base64"] = True
        else:
            raise ValueError(f"Unsupported content type: {content_type}")

        print(json.dumps(output))

        response = Response()
        response.status_code = 200
        response._content = b'{"message": "success"}'
        response.headers = {"Content-Type": "application/json"}
        return response


def get_lambda_resource() -> Resource:
    """
    Creates a Resource instance with AWS Lambda attributes using OpenTelemetry semantic conventions.
    
    Returns:
        Resource: Resource instance with AWS Lambda environment attributes
    """
    # Extract account ID from log group ARN if available
    log_group = os.environ.get("AWS_LAMBDA_LOG_GROUP_NAME", "")
    account_id = log_group.split(":")[3] if log_group.startswith("arn:") else ""
    
    attributes = {
        ResourceAttributes.CLOUD_PROVIDER: "aws",
        ResourceAttributes.CLOUD_ACCOUNT_ID: account_id,
        **{
            attr: os.environ.get(env_var, "")
            for attr, env_var in {
                ResourceAttributes.CLOUD_REGION: "AWS_REGION",
                ResourceAttributes.FAAS_NAME: "AWS_LAMBDA_FUNCTION_NAME",
                ResourceAttributes.FAAS_VERSION: "AWS_LAMBDA_FUNCTION_VERSION",
                ResourceAttributes.FAAS_INSTANCE: "AWS_LAMBDA_LOG_STREAM_NAME",
                ResourceAttributes.FAAS_MAX_MEMORY: "AWS_LAMBDA_FUNCTION_MEMORY_SIZE",
            }.items()
        }
    }
    
    return Resource.create(attributes)
