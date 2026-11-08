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

impl<V: super::Venue> super::Engine<V> {
    #[inline]
    pub(super) fn emit_engine_unknown(
        &mut self,
        now: f64,
        request: RequestId,
        owner: crate::request_context::Owner,
        status: Option<u16>,
        first_action: usize,
    ) {
        let Some(telemetry) = V::unknown_outcome_telemetry() else {
            return;
        };
        // Only new sealed notices are observed; returned telemetry cannot be
        // revisited, nor can a late response re-emit an earlier completion.
        let end = self.io.core.actions.len();
        for index in first_action..end {
            let super::Action::Notice(super::Notice::CommandFailed { command, error }) =
                &self.io.core.actions[index]
            else {
                continue;
            };
            if error.outcome != Some(crate::Outcome::Unknown)
                || matches!(owner, crate::request_context::Owner::Command { id, .. } if id != *command)
            {
                continue;
            }
            let event = telemetry(&EngineUnknownOutcome {
                now,
                command: *command,
                request,
                status,
                error,
            });
            super::push_event(
                &mut self.io.core.actions,
                &mut self.io.core.health,
                self.io.core.min_level,
                event,
            );
        }
    }
}
