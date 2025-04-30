use crate::events::ParsedPlatformEvent;

/// Enum representing the different types of input the main processor loop can receive.
#[derive(Debug)]
pub(crate) enum ProcessorInput {
    /// An OTLP JSON string read directly from the named pipe.
    OtlpJson(String),
    /// A parsed platform event received from the Telemetry API.
    PlatformTelemetry(ParsedPlatformEvent),
} 