//! Shared admission checks through a temporary native private account.
use safe_cex_engine::ErrorKind;
use safe_cex_native::{AccountHandle, Outgoing, ServerConfig, Transport};
use std::sync::Arc;
use std::time::Duration;

// Explicit staging entries. Adding one requires the matching offline suite;
// this catalog does not enable a signing mode or change a rollout declaration.
pub(crate) const PRIVATE_STARTUP_VENUES: &[&str] =
    &["bybit", "binance", "okx", "bitget", "blofin", "kucoin"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProbeOutcome {
    Accepted,
    Invalid,
    Transient,
}

/// Venue-independent classification of the engine's existing read result.
/// Missing classification is uncertainty, never successful admission.
pub(crate) fn classify(result: Result<(), Option<ErrorKind>>) -> ProbeOutcome {
    match result {
        Ok(()) => ProbeOutcome::Accepted,
        Err(Some(
            ErrorKind::AuthInvalid
            | ErrorKind::AuthExpired
            | ErrorKind::AuthPermission
            | ErrorKind::InvalidInput
            | ErrorKind::Unsupported
            | ErrorKind::Rejected
            | ErrorKind::InsufficientBalance,
        )) => ProbeOutcome::Invalid,
        Err(
            None
            | Some(
                ErrorKind::RateLimited
                | ErrorKind::Network
                | ErrorKind::Timeout
                | ErrorKind::VenueUnavailable
                | ErrorKind::Internal,
            ),
        ) => ProbeOutcome::Transient,
    }
}

impl ProbeOutcome {
    fn into_result(self) -> Result<(), ProbeRefusal> {
        match self {
            Self::Accepted => Ok(()),
            Self::Invalid => Err(ProbeRefusal::INVALID),
            Self::Transient => Err(ProbeRefusal::TRANSIENT),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProbeRefusal {
    pub transient: bool,
}
impl ProbeRefusal {
    pub const INVALID: Self = Self { transient: false };
    pub const TRANSIENT: Self = Self { transient: true };

    fn classified(kind: ErrorKind) -> Self {
        Self {
            transient: classify(Err(Some(kind))) != ProbeOutcome::Invalid,
        }
    }
}

struct ProbeActor {
    handle: AccountHandle,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}
impl Drop for ProbeActor {
    fn drop(&mut self) {
        self.handle.dispose();
        self.task.abort();
    }
}

pub(crate) async fn run<T: Transport + 'static>(
    venue: &str,
    options_json: &str,
    provider: safe_cex_engine::signing::Provider,
    transport: T,
    config: ServerConfig,
    allowed: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<(), ProbeRefusal> {
    if !PRIVATE_STARTUP_VENUES.contains(&venue) {
        return Err(ProbeRefusal::INVALID);
    }
    let options: safe_cex_venues::EngineOptions =
        serde_json::from_str(options_json).map_err(|_| ProbeRefusal::INVALID)?;
    if options.frame_fed
        || options
            .construction(venue)
            .public_data_only
            .unwrap_or(options.public_data_only)
        || options.signing.as_ref().is_none_or(|identities| {
            identities
                .primary
                .as_ref()
                .is_none_or(|identity| identity.signing_ref().is_err())
        })
    {
        return Err(ProbeRefusal::INVALID);
    }
    if !allowed() {
        return Err(ProbeRefusal::TRANSIENT);
    }
    let (handle, task) = safe_cex_native::spawn_venue_account(
        venue,
        options_json,
        transport,
        Some(provider),
        config,
    )
    .map_err(|_| ProbeRefusal::INVALID)?;
    let mut actor = ProbeActor { handle, task };
    let result = async {
        let mut link = actor
            .handle
            .join(0, true)
            .await
            .map_err(|_| ProbeRefusal::TRANSIENT)?;
        let mut tick = tokio::time::interval(Duration::from_millis(20));
        loop {
            tokio::select! {
                biased;
                frame = link.outgoing.recv() => {
                    let Some(frame) = frame else { return Err(ProbeRefusal::TRANSIENT); };
                    if let Outgoing::Text(text) = frame
                        && let Some(refusal) = notice_refusal(&text)
                    { return Err(refusal); }
                }
                _ = tick.tick() => {
                    if !allowed() { return Err(ProbeRefusal::TRANSIENT); }
                    let loaded = actor.handle.read(|store| {
                        let loaded = store.loaded();
                        loaded.markets && loaded.balance && loaded.orders && loaded.positions
                    }).await;
                    match loaded {
                        Some(true) => return classify(Ok(())).into_result(),
                        Some(false) => {},
                        None => return Err(ProbeRefusal::TRANSIENT),
                    }
                }
                _ = link.canceled.changed() => return Err(ProbeRefusal::TRANSIENT),
            }
        }
    }
    .await;
    actor.handle.dispose();
    let _ = (&mut actor.task).await;
    result
}

fn notice_refusal(text: &str) -> Option<ProbeRefusal> {
    let notice: serde_json::Value = serde_json::from_str(text).ok()?;
    let kind = match notice["type"].as_str()? {
        "startupFailed" => &notice["detail"]["kind"],
        "event" => &notice["fields"]["kind"],
        _ => return None,
    };
    let classified = [
        ErrorKind::AuthInvalid,
        ErrorKind::AuthExpired,
        ErrorKind::AuthPermission,
        ErrorKind::RateLimited,
        ErrorKind::Network,
        ErrorKind::Timeout,
        ErrorKind::VenueUnavailable,
        ErrorKind::InvalidInput,
        ErrorKind::Unsupported,
        ErrorKind::Internal,
        ErrorKind::Rejected,
        ErrorKind::InsufficientBalance,
    ]
    .into_iter()
    .find(|value| kind.as_str() == Some(value.as_str()));
    match classified {
        Some(kind) => Some(ProbeRefusal::classified(kind)),
        None if notice["type"] == "startupFailed" => Some(ProbeRefusal::TRANSIENT),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classifier_requires_success_and_refuses_unknown_results() {
        assert_eq!(classify(Ok(())), ProbeOutcome::Accepted);
        assert_eq!(classify(Err(None)), ProbeOutcome::Transient);
        for kind in [
            ErrorKind::AuthInvalid,
            ErrorKind::AuthExpired,
            ErrorKind::AuthPermission,
            ErrorKind::InvalidInput,
            ErrorKind::Unsupported,
            ErrorKind::Rejected,
            ErrorKind::InsufficientBalance,
        ] {
            assert_eq!(classify(Err(Some(kind))), ProbeOutcome::Invalid);
        }
        for kind in [
            ErrorKind::RateLimited,
            ErrorKind::Network,
            ErrorKind::Timeout,
            ErrorKind::VenueUnavailable,
            ErrorKind::Internal,
        ] {
            assert_eq!(classify(Err(Some(kind))), ProbeOutcome::Transient);
        }
        for notice in [
            r#"{"type":"startupFailed","detail":{"message":"success"}}"#,
            r#"{"type":"startupFailed","detail":{"kind":"futureKind"}}"#,
        ] {
            assert!(notice_refusal(notice).unwrap().transient);
        }
    }

    #[test]
    fn probe_uses_error_kinds_and_never_error_text() {
        for kind in [
            ErrorKind::AuthInvalid,
            ErrorKind::AuthExpired,
            ErrorKind::AuthPermission,
        ] {
            assert!(!ProbeRefusal::classified(kind).transient);
        }
        for kind in [
            ErrorKind::RateLimited,
            ErrorKind::Network,
            ErrorKind::Timeout,
            ErrorKind::VenueUnavailable,
        ] {
            assert!(ProbeRefusal::classified(kind).transient);
        }
        assert!(
            notice_refusal(
                r#"{"type":"startupFailed","detail":{"kind":"network","message":"authInvalid"}}"#
            )
            .unwrap()
            .transient
        );
        assert!(
            !notice_refusal(
                r#"{"type":"event","fields":{"kind":"authPermission"},"message":"timeout"}"#
            )
            .unwrap()
            .transient
        );
        assert!(notice_refusal(r#"{"type":"event","message":"authInvalid"}"#).is_none());
    }
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod venue_tests;
