//! Telemetry-only projection after engine-owned mutation uncertainty sealing.
//! Venues opt in through `Venue::unknown_outcome_telemetry`; the default is inert.
//! The callback sees no request body, credentials or engine capabilities.
use super::{CommandId, LogEvent, RequestId, VenueError};

/// Canonical venue-authored Unknown guidance; presentation only.
pub const UNKNOWN_OUTCOME_TEXT: &str = "The outcome is unknown. Check Open Orders";
/// The engine's existing unproven HTTP-envelope message.
pub const UNKNOWN_HTTP_OUTCOME_TEXT: &str =
    "HTTP response does not prove the write outcome. The outcome is unknown. Check Open Orders";

/// Immutable facts after the engine sealed a released mutation Unknown.
/// No request, completion, classification, Store or retry authority is provided.
#[derive(Debug)]
pub struct EngineUnknownOutcome<'a> {
    pub now: f64,
    pub command: CommandId,
    pub request: RequestId,
    /// None for a transport failure without a complete HTTP response.
    pub status: Option<u16>,
    pub error: &'a VenueError,
}

/// Static telemetry projection: no mutable venue receiver or engine capability.
pub type UnknownOutcomeTelemetry = fn(&EngineUnknownOutcome<'_>) -> LogEvent;

/// First engine-owned uncertainty for one bounded owner generation.
#[derive(Clone, Copy)]
pub(super) struct UnknownOutcomeSource {
    pub request: RequestId,
    pub status: Option<u16>,
}
