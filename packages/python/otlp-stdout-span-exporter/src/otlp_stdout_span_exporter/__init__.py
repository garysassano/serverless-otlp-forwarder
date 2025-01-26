"""
OpenTelemetry Stdout Span Exporter

A span exporter that writes OpenTelemetry spans to stdout in OTLP format.
"""

from .exporter import OTLPStdoutSpanExporter
from .version import VERSION

__version__ = VERSION
__all__ = ["OTLPStdoutSpanExporter"]
