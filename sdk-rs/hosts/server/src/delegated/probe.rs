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
    venue: &str,
    prepared: super::PreparedDelegated,
    transport: T,
    config: ServerConfig,
    allowed: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<(), ProbeRefusal> {
    let (handle, task) = safe_cex_native::spawn_venue_account(
        venue,
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
    #[derive(Clone, Default)]
    struct Requests(std::sync::Arc<std::sync::Mutex<Vec<String>>>);

    impl Transport for Requests {
        fn prepare_http(
            &self,
            request: safe_cex_engine::HttpRequest,
            _: safe_cex_engine::request_context::RoutingIntent,
        ) -> safe_cex_native::HttpPrepareFuture {
            self.0.lock().unwrap().push(request.url);
            Box::pin(async { Err(safe_cex_native::HttpPrepareError::Unsupported) })
        }
        fn connect(
            &self,
            _: safe_cex_engine::SocketId,
            url: String,
            _: safe_cex_native::Events,
        ) -> tokio::sync::mpsc::UnboundedSender<String> {
            self.0.lock().unwrap().push(url);
            tokio::sync::mpsc::unbounded_channel().0
        }
        fn request(&self, request: safe_cex_engine::HttpRequest, events: safe_cex_native::Events) {
            // Record and refuse legacy requests as well as managed preparations; no network IO.
            self.0.lock().unwrap().push(request.url);
            let _ = events.send(safe_cex_native::HostEvent::HttpFailed(
                request.id,
                "synthetic transport refusal".into(),
            ));
        }
    }

    #[tokio::test]
    async fn venue_declaration_probe_uses_the_requested_engine() {
        tokio::task::LocalSet::new()
            .run_until(async {
                crate::delegated::with_test_declaration(
                    serde_json::json!({"venues": {
                        "bybit": {"delegated": {"enabled": true}},
                        "binance": {"delegated": {"enabled": true}}
                    }}),
                    async {
                        let init = crate::envelope::InitPayload {
                            account_name: "fixture".into(),
                            exchange_name: "binance".into(),
                            key: "synthetic-key".into(),
                            ..Default::default()
                        };
                        let pair = crate::delegated::DelegatedPair::new(
                            init.key.clone(),
                            safe_cex_crypto::signer::Secret::new("synthetic".into()),
                            None,
                        );
                        let prepared = crate::delegated::prepare_delegated(&init, pair)
                            .unwrap_or_else(|_| panic!("enabled fixture venue prepares"));
                        let requests = Requests::default();
                        let _result = tokio::time::timeout(
                            Duration::from_secs(2),
                            run(
                                &init.exchange_name,
                                prepared,
                                requests.clone(),
                                ServerConfig::default(),
                                Arc::new(|| true),
                            ),
                        )
                        .await;
                        let requests = requests.0.lock().unwrap();
                        assert!(!requests.is_empty());
                        assert!(
                            requests.iter().all(|url| url.contains("binance")),
                            "{requests:?}"
                        );
                    },
                )
                .await;
            })
            .await;
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
