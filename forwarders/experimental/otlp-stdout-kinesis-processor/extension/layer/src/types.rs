use crate::events::ParsedPlatformEvent;
use std::fmt::Debug;

/// Enum representing the different types of input the main processor loop can receive.
#[derive(Debug)]
pub(crate) enum ProcessorInput {
    /// A parsed platform event received from the Telemetry API.
    PlatformTelemetry(ParsedPlatformEvent),
} 