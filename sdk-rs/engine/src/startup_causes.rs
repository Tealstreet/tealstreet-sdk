//! Bounded startup diagnostics; no request text, URLs, identities or rows are retained.
use crate::{ErrorKind, VenueError, log::Value};

/// Native request membership, declared before issuing a startup read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sections(u8);
impl Sections {
    pub const NONE: Self = Self(0);
    pub const MARKETS: Self = Self(1);
    pub const TICKERS: Self = Self(2);
    pub const BALANCE: Self = Self(4);
    pub const ORDERS: Self = Self(8);
    pub const POSITIONS: Self = Self(16);
    pub const PUBLIC: Self = Self(3);
    pub const ACCOUNT: Self = Self(28);
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Clone)]
struct Failure {
    state: &'static str,
    kind: Option<ErrorKind>,
    venue_code: Option<String>,
    status: Option<u16>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Delivery {
    pub sections: Sections,
    pub status: Option<u16>,
    recorded: bool,
}

impl Delivery {
    pub fn new(sections: Sections, status: Option<u16>) -> Self {
        Self {
            sections,
            status,
            recorded: false,
        }
    }
}

#[inline(never)]
pub(crate) fn error_kind(value: &str) -> Option<ErrorKind> {
    Some(match value {
        "rejected" => ErrorKind::Rejected,
        "invalidInput" => ErrorKind::InvalidInput,
        "insufficientBalance" => ErrorKind::InsufficientBalance,
        "authInvalid" => ErrorKind::AuthInvalid,
        "authExpired" => ErrorKind::AuthExpired,
        "authPermission" => ErrorKind::AuthPermission,
        "rateLimited" => ErrorKind::RateLimited,
        "network" => ErrorKind::Network,
        "timeout" => ErrorKind::Timeout,
        "venueUnavailable" => ErrorKind::VenueUnavailable,
        "unsupported" => ErrorKind::Unsupported,
        "internal" => ErrorKind::Internal,
        _ => return None,
    })
}

#[derive(Default)]
pub(crate) struct Causes {
    started: u8,
    failures: [Option<Failure>; 5],
}

const KEYS: [[&str; 4]; 5] = [
    [
        "startupMarketsState",
        "startupMarketsKind",
        "startupMarketsVenueCode",
        "startupMarketsHttpStatus",
    ],
    [
        "startupTickersState",
        "startupTickersKind",
        "startupTickersVenueCode",
        "startupTickersHttpStatus",
    ],
    [
        "startupBalanceState",
        "startupBalanceKind",
        "startupBalanceVenueCode",
        "startupBalanceHttpStatus",
    ],
    [
        "startupOrdersState",
        "startupOrdersKind",
        "startupOrdersVenueCode",
        "startupOrdersHttpStatus",
    ],
    [
        "startupPositionsState",
        "startupPositionsKind",
        "startupPositionsVenueCode",
        "startupPositionsHttpStatus",
    ],
];

pub(crate) fn venue_code(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_:-.".contains(&b)))
    .then(|| value.to_owned())
}

impl Causes {
    pub fn started(&mut self, sections: Sections) {
        self.started |= sections.0;
    }

    // Shared across every Pending type; retries never erase the last failure.
    #[inline(never)]
    pub fn failed(&mut self, delivery: &mut Delivery, error: &VenueError, refused: bool) {
        if delivery.sections == Sections::NONE {
            return;
        }
        if delivery.recorded && error.kind.is_none() && error.venue_code.is_none() {
            return;
        }
        delivery.recorded = true;
        self.started(delivery.sections);
        let failure = Failure {
            state: if error.kind == Some(ErrorKind::Timeout) {
                "timedOut"
            } else if refused
                || matches!(
                    error.kind,
                    Some(
                        ErrorKind::AuthInvalid
                            | ErrorKind::AuthExpired
                            | ErrorKind::AuthPermission
                            | ErrorKind::Rejected
                    )
                )
            {
                "refused"
            } else {
                "failed"
            },
            kind: error.kind,
            venue_code: error.venue_code.as_deref().and_then(venue_code),
            status: delivery.status.filter(|s| (100..=599).contains(s)),
        };
        for (i, slot) in self.failures.iter_mut().enumerate() {
            if delivery.sections.0 & (1 << i) != 0 {
                *slot = Some(failure.clone());
            }
        }
    }

    #[inline(never)]
    pub fn fields(&self, missing: &[&str], fields: &mut Vec<(&'static str, Value)>) {
        for (i, name) in ["markets", "tickers", "balance", "orders", "positions"]
            .iter()
            .enumerate()
        {
            if !missing.contains(name) {
                continue;
            }
            let [state, kind, code, status] = KEYS[i];
            let failure = self.failures[i].as_ref();
            fields.push((
                state,
                Value::StaticStr(failure.map_or(
                    if self.started & (1 << i) == 0 {
                        "neverStarted"
                    } else {
                        "started"
                    },
                    |f| f.state,
                )),
            ));
            if let Some(failure) = failure {
                if let Some(value) = failure.kind {
                    fields.push((kind, Value::StaticStr(value.as_str())));
                }
                if let Some(value) = &failure.venue_code {
                    fields.push((code, value.as_str().into()));
                }
                if let Some(value) = failure.status {
                    fields.push((status, value.into()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_error_kind_parser_round_trips_without_accepting_message_text() {
        for kind in [
            ErrorKind::Rejected,
            ErrorKind::InvalidInput,
            ErrorKind::InsufficientBalance,
            ErrorKind::AuthInvalid,
            ErrorKind::AuthExpired,
            ErrorKind::AuthPermission,
            ErrorKind::RateLimited,
            ErrorKind::Network,
            ErrorKind::Timeout,
            ErrorKind::VenueUnavailable,
            ErrorKind::Unsupported,
            ErrorKind::Internal,
        ] {
            assert_eq!(error_kind(kind.as_str()), Some(kind));
        }
        for text in ["", "authPermission: private account", "TIMEOUT"] {
            assert_eq!(error_kind(text), None);
        }
    }
}
