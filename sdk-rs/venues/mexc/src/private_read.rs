//! Exact selected read ownership and deferred native account successors.
use crate::{MexcFutures, Pending, sign};
use safe_cex_engine::credential_selection::{SlotPolicy, Slots};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::{Context, Error, RetryPolicy};
use safe_cex_engine::selected_read::{
    AdmittedRead, LocalRefusal, ReadLeg, ReadLegPolicy, ReadPolicy, ReadRequest, ReadRole,
    ReadRoot, SelectedRead,
};
use safe_cex_engine::{Cx, Method, VenueError};

pub enum Owner {
    Background(AdmittedRead),
    Caller { root: ReadRoot, leg: ReadLeg },
}
impl Owner {
    pub fn context(&self) -> &Context {
        match self {
            Self::Background(read) => read.context(),
            Self::Caller { leg, .. } => leg.context(),
        }
    }
    pub fn root(&self) -> Option<&Context> {
        match self {
            Self::Caller { root, .. } => Some(root.context()),
            _ => None,
        }
    }
    pub fn current(&self, cx: &Cx<'_, Pending>) -> bool {
        match self {
            Self::Background(read) => cx
                .validate_background_read_http_delivery(read.context())
                .is_ok(),
            Self::Caller { root, leg } => root.validate_delivery(cx, leg).is_ok(),
        }
    }
}
pub struct Deferred {
    pub method: Method,
    pub path: String,
    pub payload: String,
    pub streamed: bool,
    pub pending: Pending,
}
impl MexcFutures {
    pub(crate) fn read_selection(
        &self,
        primary: bool,
        scope: Option<safe_cex_engine::AccountReadScope>,
    ) -> Result<SelectedRead, safe_cex_engine::credential_selection::CredentialUnavailable> {
        let enabled = !self.options.public_data_only
            && self.options.read_credentials_valid != Some(false)
            && !(self.options.read_credentials_valid == Some(true)
                && self.options.identities.secondary.is_none());
        let slots = Slots::new(
            self.credentials.clone().filter(|_| enabled),
            self.options
                .identities
                .secondary
                .clone()
                .filter(|_| enabled),
            SlotPolicy::default(),
        );
        SelectedRead::select(
            &slots,
            if primary {
                ReadRole::Primary {
                    reason: safe_cex_engine::credential_selection::PrimaryReason::History,
                    scope,
                }
            } else {
                ReadRole::Read { scope }
            },
        )
    }
    pub(crate) fn signed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        payload: String,
        streamed: bool,
        pending: Pending,
    ) {
        if self.defer_private {
            self.deferred_private.push(Deferred {
                method,
                path: path.into(),
                payload,
                streamed,
                pending,
            });
            return;
        }
        if method != Method::Get {
            self.queue_trade(cx, method, path, payload, pending);
            return;
        }
        if matches!(pending, Pending::Trade(_)) {
            self.queue_trade(cx, method, path, payload, pending);
            return;
        }
        let scope = crate::account::auth_scope(&pending);
        let selected = self.read_selection(false, scope);
        let recipe_path = path.to_owned();
        let recipe_payload = payload.clone();
        let url = if payload.is_empty() {
            format!("{}{path}", crate::REST)
        } else {
            format!("{}{path}?{payload}", crate::REST)
        };
        let request = ReadRequest {
            method,
            url,
            body: None,
            stream: streamed.then_some(ArrayAt::EnvelopeField("data")),
        };
        let window = self.options.recv_window;
        let recipe = move |identity: &safe_cex_engine::signing::Identity| -> Box<dyn safe_cex_engine::SignStep> {
            Box::new(sign::dispatch::Http::new(identity.clone(), method, recipe_path, recipe_payload, window))
        };
        let caller = matches!(
            pending,
            Pending::Identity(_) | Pending::Account(crate::account::Request::Leverage { .. })
        );
        let result = if caller {
            let Ok(root) = SelectedRead::command_root(
                cx,
                selected,
                LocalRefusal {
                    diagnostic: None,
                    error: VenueError::new("MEXC read signing identity unavailable"),
                },
            ) else {
                return;
            };
            let leg = match root.leg(
                cx,
                ReadLegPolicy {
                    timeout_ms: 20_000,
                    pacing: None,
                },
            ) {
                Ok(leg) => leg,
                Err(error) => {
                    let _ = cx.fail_context(root.context(), error.venue_error());
                    return;
                }
            };
            leg.queue(cx, request, recipe, |leg| Pending::Private {
                owner: Owner::Caller { root, leg },
                pending: Box::new(pending),
            })
        } else {
            let read = match SelectedRead::background(
                cx,
                selected,
                ReadPolicy {
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            ) {
                Ok(read) => read,
                Err(safe_cex_engine::selected_read::BackgroundReadError::Selection(_)) => return,
                Err(safe_cex_engine::selected_read::BackgroundReadError::Admission(error)) => {
                    if cx.now.is_finite() && matches!(error, Error::Capacity | Error::TimedOut) {
                        self.failure(cx, pending, &error.venue_error().message);
                    } else {
                        self.discard_private(pending);
                        if cx.now.is_finite() {
                            cx.notice(
                                safe_cex_engine::Level::Warn,
                                safe_cex_engine::log::code::REQUEST_FAILED,
                                &error.venue_error().message,
                            );
                        }
                    }
                    return;
                }
            };
            read.queue(cx, request, recipe, |read| Pending::Private {
                owner: Owner::Background(read),
                pending: Box::new(pending),
            })
        };
        if let Err((pending, error)) = result {
            self.private_precommit_failed(cx, pending, error, error.venue_error());
        }
    }
    pub(crate) fn flush_private(&mut self, cx: &mut Cx<'_, Pending>) {
        for request in std::mem::take(&mut self.deferred_private) {
            self.signed(
                cx,
                request.method,
                &request.path,
                request.payload,
                request.streamed,
                request.pending,
            );
        }
    }
    pub(crate) fn private_done(&mut self, _cx: &mut Cx<'_, Pending>, data: String) {
        self.private_result = Some(Ok(data));
    }
    pub(crate) fn private_fail(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        error: impl Into<VenueError>,
        diagnostic: bool,
    ) {
        let error = error.into();
        if diagnostic && let Some(root) = &self.private_delivery {
            let _ = cx.command_error_context(root, &error.message);
        }
        self.private_result = Some(Err(error));
    }
    pub(crate) fn finish_private(&mut self, cx: &mut Cx<'_, Pending>, owner: Owner) {
        let _ = cx.finish_http_context(owner.context());
        self.complete_private(cx, owner);
    }
    fn complete_private(&mut self, cx: &mut Cx<'_, Pending>, owner: Owner) {
        if let Some(root) = owner.root()
            && let Some(result) = self.private_result.take()
        {
            match result {
                Ok(data) => {
                    let _ = cx.done_context(root, data);
                }
                Err(error) => {
                    let _ = cx.fail_context(root, error);
                }
            }
        }
        self.private_delivery = None;
        self.defer_private = false;
        self.flush_private(cx);
    }
    pub(crate) fn private_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: VenueError,
    ) {
        self.retire_private_failure(cx, pending, error, true);
    }
    pub(crate) fn private_precommit_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: VenueError,
    ) {
        let recover = matches!(reason, Error::Capacity | Error::TimedOut)
            || (reason == Error::Invalid
                && matches!(
                    error.kind,
                    Some(
                        safe_cex_engine::ErrorKind::Network
                            | safe_cex_engine::ErrorKind::Timeout
                            | safe_cex_engine::ErrorKind::Internal
                    )
                ));
        self.retire_private_failure(cx, pending, error, recover);
    }
    fn retire_private_failure(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: VenueError,
        recover: bool,
    ) {
        let Pending::Private { owner, pending } = pending else {
            self.failure(cx, pending, &error.message);
            return;
        };
        if cx.finish_http_context(owner.context()).is_err() {
            return;
        }
        if !cx.now.is_finite() {
            self.discard_private(*pending);
            if let Some(root) = owner.root() {
                let _ = cx.fail_context(root, error);
            }
            return;
        }
        self.private_delivery = owner.root().cloned();
        self.defer_private = true;
        if recover {
            self.failure(cx, *pending, &error.message);
        } else {
            self.discard_private(*pending);
            if owner.root().is_some() {
                self.private_fail(cx, error, true);
            } else {
                cx.notice(
                    safe_cex_engine::Level::Warn,
                    safe_cex_engine::log::code::REQUEST_FAILED,
                    &error.message,
                );
            }
        }
        self.complete_private(cx, owner);
    }
    fn discard_private(&mut self, pending: Pending) {
        if let Pending::Account(crate::account::Request::Snapshot(snapshot)) = pending {
            self.account.in_flight.remove(&snapshot.scope);
            if snapshot.full {
                self.account.stage_mask = 0;
                self.account.full_failed = true;
                self.account.needs_full = true;
                self.account.staged.clear();
            }
        }
    }
}
