use crate::{Coinbase, Pending};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::{Cx, Method, RequestRoute, TimerId, Venue};
use std::collections::VecDeque;

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
        if !public && (self.credentials.is_none() || !path.starts_with("/api/v3/brokerage/")) {
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
        let trading_key_check = matches!(
            request.pending,
            Pending::Account(
                crate::account::Request::KeyPermissions {
                    trading_portfolio: None
                },
                _
            )
        );
        // Only account and history reads use the read-only key; catalogue and
        // trading reads (the amend cancellation proof) keep the trading key.
        let read = request.method == Method::Get
            && !trading_key_check
            && matches!(request.pending, Pending::Account(..) | Pending::History(_));
        let primary_route = request.method == Method::Get
            && !public
            && self.read_supplied
            && (!read || self.read_credentials.is_none());
        let headers = if public {
            Ok(vec![])
        } else {
            let credentials = if read {
                self.read_credentials.as_ref().or(self.credentials.as_ref())
            } else {
                self.credentials.as_ref()
            };
            credentials
                .ok_or_else(|| "Coinbase credentials unavailable".to_owned())
                .and_then(|credentials| {
                    credentials.headers(request.method, &request.path, cx.now, |bytes| {
                        cx.fill_signing_entropy(bytes)
                    })
                })
        };
        let mut request = request;
        if let Pending::Account(_, signed) = &mut request.pending {
            *signed = read && !public && self.read_credentials.is_some();
        }
        let headers = match headers {
            Ok(headers) => headers,
            Err(error) => {
                self.fail_unsent(cx, request.pending, &error);
                self.dispatch(cx);
                return;
            }
        };
        if let Pending::Trading(pending) = &request.pending
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
        let id = cx.request(request.method, url, headers, request.body, request.pending);
        if primary_route {
            cx.route_request(id, RequestRoute::Primary);
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
    fn fail_unsent(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Trading(request) => self.trading_unsent(cx, request, error),
            Pending::History(request) => self.history_unsent(cx, request, error),
            other => self.on_request_failed(cx, other, error),
        }
    }
}
