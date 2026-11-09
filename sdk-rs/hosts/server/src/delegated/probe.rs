//! A temporary native account observes the venue's existing private startup.
use safe_cex_engine::ErrorKind;
use safe_cex_native::{AccountHandle, Outgoing, ServerConfig, Transport};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProbeRefusal {
    pub transient: bool,
}
impl ProbeRefusal {
    pub const INVALID: Self = Self { transient: false };
    pub const TRANSIENT: Self = Self { transient: true };

    fn classified(kind: ErrorKind) -> Self {
        Self {
            transient: matches!(
                kind,
                ErrorKind::RateLimited
                    | ErrorKind::Network
                    | ErrorKind::Timeout
                    | ErrorKind::VenueUnavailable
                    | ErrorKind::Internal
            ),
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
    prepared: super::PreparedDelegated,
    transport: T,
    config: ServerConfig,
    allowed: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<(), ProbeRefusal> {
    let (handle, task) = safe_cex_native::spawn_venue_account(
        "bybit",
        &prepared.options,
        transport,
        Some(prepared.resident.provider()),
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
                        Some(true) => return Ok(()),
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
