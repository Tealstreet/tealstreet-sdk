use crate::{Coinbase, Pending};
use safe_cex_engine::credential_selection::{CredentialRole, PrimaryReason};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::{CredentialSlot, Cx, Method, TimerId, Venue};
use std::collections::VecDeque;

/// Existing Coinbase REST HTTP emission boundary.
pub(crate) fn emit<P>(
    cx: &mut Cx<'_, P>,
    method: Method,
    url: String,
    headers: Vec<(&'static str, String)>,
    body: Option<String>,
    pending: P,
) -> safe_cex_engine::RequestId {
    struct Packet<P> {
        method: Method,
        body: Option<String>,
        pending: P,
    }
    let request = Packet {
        method,
        body,
        pending,
    };
    cx.request(request.method, url, headers, request.body, request.pending)
}

struct Outbound {
    method: Method,
    path: String,
    body: Option<String>,
    at: Option<ArrayAt>,
    pending: Pending,
}

#[derive(Default)]
pub(crate) struct Rest {
    queue: VecDeque<Outbound>,
    timer: Option<TimerId>,
    next: f64,
}

impl Coinbase {
    pub(crate) fn request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: String,
        body: Option<String>,
        at: Option<ArrayAt>,
        pending: Pending,
    ) -> Result<(), String> {
        if self.rest.queue.len() >= 256 {
            return Err("Coinbase request queue is full".into());
        }
        let public = path == "https://api.international.coinbase.com/api/v1/instruments/"
            || path.starts_with("/api/v3/brokerage/market/");
        if !public && (!self.signers.has_primary() || !path.starts_with("/api/v3/brokerage/")) {
            return Err("Coinbase authenticated request is unavailable".into());
        }
        if path.len() > 4096 || path.chars().any(char::is_control) || path.contains('#') {
            return Err("Coinbase invalid request path".into());
        }
        self.rest.queue.push_back(Outbound {
            method,
            path,
            body,
            at,
            pending,
        });
        self.dispatch(cx);
        Ok(())
    }

    fn dispatch(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.rest.timer.is_some() || self.rest.queue.is_empty() {
            return;
        }
        if cx.now < self.rest.next {
            self.rest.timer = Some(cx.set_timer(self.rest.next - cx.now));
            return;
        }
        let Some(request) = self.rest.queue.pop_front() else {
            return;
        };
        let public = request
            .path
            .starts_with("https://api.international.coinbase.com/")
            || request.path.starts_with("/api/v3/brokerage/market/");
        let role = match &request.pending {
            Pending::Account(
                crate::account::Request::KeyPermissions {
                    trading_portfolio: None,
                },
                _,
            ) => CredentialRole::PrimaryRead {
                reason: PrimaryReason::KeyPermissions,
                scope: None,
            },
            Pending::Account(..) | Pending::History(_) => self.read_role(),
            Pending::Public(_) => CredentialRole::PrimaryRead {
                reason: PrimaryReason::Catalog,
                scope: None,
            },
            Pending::Trading(pending) => pending.signing_role(),
        };
        let selected = self.signers.select(role).ok();
        let recipe = if public {
            None
        } else {
            selected.and_then(|(_, c)| {
                c.identity().map(|identity| {
                    crate::sign::Http::new(
                        identity.clone(),
                        c.key(),
                        c.algorithm(),
                        request.method,
                        &request.path,
                    )
                })
            })
        };
        let headers = if public || recipe.is_some() {
            Ok(vec![])
        } else {
            selected
                .ok_or_else(|| "Coinbase credentials unavailable".to_owned())
                .and_then(|(_, credentials)| {
                    credentials.headers(request.method, &request.path, cx.protocol_now(), |bytes| {
                        cx.fill_signing_entropy(bytes)
                    })
                })
        };
        let mut request = request;
        if let Pending::Account(_, signed) = &mut request.pending {
            *signed = !public
                && selected
                    .is_some_and(|(selection, _)| selection.slot() == CredentialSlot::Secondary);
        }
        let selection = selected.map(|(selection, _)| selection);
        let headers = match headers {
            Ok(headers) => headers,
            Err(error) => {
                self.fail_unsent(cx, request.pending, &error);
                self.dispatch(cx);
                return;
            }
        };
        if let Pending::Trading(pending) = &mut request.pending
            && let Err(error) = self.trading_before_dispatch(cx, pending)
        {
            self.fail_unsent(cx, request.pending, &error);
            self.dispatch(cx);
            return;
        }
        let url = if public && request.path.starts_with("https://") {
            request.path
        } else {
            format!("https://api.coinbase.com{}", request.path)
        };
        let id = emit(
            cx,
            request.method,
            url,
            headers,
            request.body,
            request.pending,
        );
        if !public && let Some(selection) = selection {
            if let Some(recipe) = recipe {
                selection.sign(cx, id, Box::new(recipe));
            } else {
                selection.bind(cx, id);
            }
        }
        if let Some(at) = request.at {
            cx.stream_response(id, at);
        }
        self.rest.next = cx.now + 334.;
        if !self.rest.queue.is_empty() {
            self.rest.timer = Some(cx.set_timer(334.));
        }
    }

    pub(crate) fn rest_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.rest.timer != Some(timer) {
            return false;
        }
        self.rest.timer = None;
        self.dispatch(cx);
        true
    }
}

impl Coinbase {
    /// A request that failed before it was sent.
    pub(crate) fn fail_unsent(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Trading(request) => self.trading_unsent(cx, request, error),
            Pending::History(request) => self.history_unsent(cx, request, error),
            other => self.on_request_failed(cx, other, error),
        }
    }
}
