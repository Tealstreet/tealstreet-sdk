//! Checked identity policy shared by engine allocation and host number codecs.
use std::ops::Range;

/// Largest integer that all supported JavaScript hosts carry exactly.
pub const MAX_RESOURCE_ID: u64 = (1_u64 << 53) - 1;

/// Validate before converting a JS Number; zero is reserved for existing shared
/// public socket ingress and is never returned by the allocator.
pub fn from_host_number(value: f64) -> Option<u64> {
    if !value.is_finite() || value < 0. || value.fract() != 0. || value > MAX_RESOURCE_ID as f64 {
        return None;
    }
    Some(value as u64)
}

/// An exhausted allocator never rewinds or emits its exclusive-end sentinel.
/// Typed admission can reserve a whole range without making exhaustion fatal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceIds {
    next: u64,
    fatal: bool,
    ceiling: u64,
}
impl Default for ResourceIds {
    fn default() -> Self {
        Self {
            next: 1,
            fatal: false,
            ceiling: MAX_RESOURCE_ID,
        }
    }
}
impl ResourceIds {
    /// Pure preflight permits coordinated admission across separate namespaces.
    pub fn available(&self, count: u64) -> bool {
        !self.fatal
            && count != 0
            && self
                .next
                .checked_add(count)
                .is_some_and(|end| end <= self.ceiling + 1)
    }
    pub fn reserve(&mut self, count: u64) -> Option<Range<u64>> {
        if !self.available(count) {
            return None;
        }
        let start = self.next;
        self.next += count;
        Some(start..self.next)
    }
    /// Compatibility backstop for infallible legacy engine APIs. Hosts must
    /// retire the failed engine; subsequent engine entry/drain is fenced.
    pub fn next_or_fatal(&mut self) -> u64 {
        self.reserve_or_fatal(1).start
    }
    pub fn reserve_or_fatal(&mut self, count: u64) -> Range<u64> {
        if let Some(range) = self.reserve(count) {
            return range;
        }
        self.invalidate();
    }
    pub(crate) fn invalidate(&mut self) -> ! {
        self.fatal = true;
        panic!("engine resource identities exhausted or invalid");
    }
    pub fn failed(&self) -> bool {
        self.fatal
    }

    #[cfg(test)]
    pub(crate) fn starting_at(next: u64) -> Self {
        assert!((1..=MAX_RESOURCE_ID + 1).contains(&next));
        Self {
            next,
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(crate) fn with_ceiling(ceiling: u64) -> Self {
        assert!((1..=MAX_RESOURCE_ID).contains(&ceiling));
        Self {
            ceiling,
            ..Self::default()
        }
    }
}

/// Rust tuple construction cannot bypass the shared host-safe wire boundary.
pub(crate) fn action_ids_safe(mut action: &crate::Action) -> bool {
    use crate::{Action, request_context::HttpAction};
    let safe = |id: u64| id <= MAX_RESOURCE_ID;
    loop {
        return match action {
            Action::WsConnect { socket, .. }
            | Action::WsSend { socket, .. }
            | Action::WsClose { socket } => safe(socket.0),
            Action::Http(request) => safe(request.id.0),
            Action::Sign { attempt, .. } => safe(attempt.0),
            Action::RetireAddressSetup { task, .. } => safe(task.0),
            Action::SetTimer { timer, .. } | Action::ClearTimer { timer } => safe(timer.0),
            Action::SignedOutbound { attempt, outbound } => {
                if !safe(attempt.0) {
                    return false;
                }
                action = outbound;
                continue;
            }
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => safe(preparation.attempt().request.0) && safe(request.id.0),
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    safe(preparation.attempt().request.0)
                        && safe(permit.attempt().request.0)
                        && safe(request.id.0)
                }
                HttpAction::BindGroup { permit, members } => {
                    permit
                        .members()
                        .iter()
                        .all(|permit| safe(permit.attempt().request.0))
                        && members.iter().all(|member| {
                            safe(member.preparation.attempt().request.0)
                                && member
                                    .request
                                    .as_ref()
                                    .is_none_or(|request| safe(request.id.0))
                        })
                }
                HttpAction::Cancel {
                    preparation,
                    signing,
                } => safe(preparation.attempt().request.0) && signing.is_none_or(|id| safe(id.0)),
            },
            Action::Notice(_) => true,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crosses_old_u32_boundary_without_aliasing() {
        let mut ids = ResourceIds {
            next: u64::from(u32::MAX),
            ..ResourceIds::default()
        };
        let values: Vec<_> = ids.reserve(3).unwrap().collect();
        assert_eq!(
            values,
            [u64::from(u32::MAX), 1_u64 << 32, (1_u64 << 32) + 1]
        );
        for value in values {
            assert_eq!(from_host_number(value as f64), Some(value));
        }
    }
    #[test]
    fn complete_range_is_reserved_atomically_and_last_id_is_issued_once() {
        let mut ids = ResourceIds {
            next: MAX_RESOURCE_ID - 1,
            ..ResourceIds::default()
        };
        assert_eq!(ids.reserve(3), None);
        assert_eq!(ids.next, MAX_RESOURCE_ID - 1);
        assert_eq!(ids.reserve(0), None);
        assert_eq!(ids.reserve(u64::MAX), None);
        assert_eq!(
            ids.reserve(2),
            Some(MAX_RESOURCE_ID - 1..MAX_RESOURCE_ID + 1)
        );
        assert_eq!(ids.reserve(1), None);
        assert!(
            !ids.failed(),
            "typed admission failure does not poison the engine"
        );
    }
    #[test]
    fn legacy_backstop_latches_before_trapping_and_all_future_ranges_refuse() {
        let mut ids = ResourceIds::with_ceiling(2);
        assert_eq!(ids.next_or_fatal(), 1);
        assert_eq!(ids.next_or_fatal(), 2);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ids.next_or_fatal()));
        assert!(result.is_err());
        assert!(ids.failed());
        assert_eq!(ids.reserve(1), None);
        assert!(!ids.available(1));
        assert_eq!(ids.next, 3);
    }
    #[test]
    fn host_numbers_never_truncate_or_saturate() {
        for value in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.,
            1.5,
            (MAX_RESOURCE_ID + 1) as f64,
        ] {
            assert_eq!(from_host_number(value), None);
        }
        assert_eq!(from_host_number(0.), Some(0));
        assert_eq!(
            from_host_number(MAX_RESOURCE_ID as f64),
            Some(MAX_RESOURCE_ID)
        );
    }
}
