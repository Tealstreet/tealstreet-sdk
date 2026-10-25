//! Command-owned public snapshots never publish subscription depth.
use crate::{DeferredGet, Pending, Task, WeexFutures, paths, rate};
use safe_cex_engine::history::ReadCompletion;
use safe_cex_engine::request_context::{Context, Operation, Owner, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Outcome, Response, VenueError};
use safe_cex_engine::{CommandWait, Method};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use std::borrow::Cow;

fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<&'de RawValue>, D::Error> {
    <&RawValue>::deserialize(d).map(Some)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    #[serde(default, borrow, deserialize_with = "present")]
    code: Option<&'a RawValue>,
    #[serde(borrow)]
    msg: Option<&'a RawValue>,
    #[serde(borrow)]
    message: Option<&'a RawValue>,
    #[serde(borrow)]
    bids: Option<&'a RawValue>,
    #[serde(borrow)]
    b: Option<&'a RawValue>,
    #[serde(borrow)]
    asks: Option<&'a RawValue>,
    #[serde(borrow)]
    a: Option<&'a RawValue>,
    #[serde(borrow)]
    last_update_id: Option<&'a RawValue>,
}
#[derive(Serialize)]
struct Level {
    price: f64,
    amount: f64,
    total: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    bids: Vec<Level>,
    asks: Vec<Level>,
    last_update_id: String,
}
pub(crate) fn failure(message: impl Into<String>, kind: ErrorKind) -> VenueError {
    VenueError::new(message)
        .kind(kind)
        .outcome(Outcome::NotSent)
}
fn invalid() -> VenueError {
    failure("Invalid WEEX order-book snapshot", ErrorKind::Internal)
}
fn envelope(raw: &str) -> Result<Envelope<'_>, VenueError> {
    if !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    serde_json::from_str(raw).map_err(|_| invalid())
}
fn text(raw: Option<&RawValue>) -> Option<Cow<'_, str>> {
    raw.and_then(|v| serde_json::from_str::<Cow<'_, str>>(v.get()).ok())
}
fn levels(raw: Option<&RawValue>) -> Result<Vec<Level>, VenueError> {
    let rows = raw
        .map(|v| serde_json::from_str::<Vec<Vec<&RawValue>>>(v.get()))
        .transpose()
        .map_err(|_| invalid())?
        .unwrap_or_default();
    let mut total = 0.0;
    rows.into_iter()
        .map(|row| {
            let number = |index: usize| {
                row.get(index)
                    .and_then(|v| serde_json::from_str::<safe_cex_engine::de::Number>(v.get()).ok())
                    .map(|n| n.0)
                    .ok_or_else(invalid)
            };
            let price = number(0)?;
            let amount = number(1)?;
            total += amount;
            if price <= 0.0 || amount < 0.0 || !total.is_finite() {
                return Err(invalid());
            }
            Ok(Level {
                price,
                amount,
                total,
            })
        })
        .collect()
}
fn version(raw: Option<&RawValue>) -> Result<String, VenueError> {
    match raw {
        None => Ok(String::new()),
        Some(raw) => {
            if let Some(text) = text(Some(raw)) {
                return Ok(text.into_owned());
            }
            let raw = raw.get();
            if !raw.is_empty() && raw.bytes().all(|c| c.is_ascii_digit()) {
                Ok(raw.into())
            } else {
                Err(invalid())
            }
        }
    }
}
pub(crate) struct Request {
    completion: ReadCompletion,
    leg: Option<Context>,
}
impl Request {
    pub(crate) fn owner(&self) -> Owner {
        self.completion.root().owner()
    }
    fn delivered(&self, cx: &Cx<'_, Pending>) -> bool {
        self.leg
            .as_ref()
            .is_some_and(|leg| self.completion.validate_delivery(cx, leg).is_ok())
    }
    fn retire_leg(&self, cx: &mut Cx<'_, Pending>) {
        if let Some(leg) = &self.leg {
            let _ = cx.finish_http_context(leg);
        }
    }
    pub(crate) fn failed(self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        self.retire_leg(cx);
        let _ = self.completion.finish(cx, Err(error));
    }
    fn complete(self, cx: &mut Cx<'_, Pending>, json: String) {
        let leg = self.leg;
        let _ = self.completion.finish(cx, Ok(json));
        if let Some(leg) = leg {
            let _ = cx.finish_http_context(&leg);
        }
    }
}
impl WeexFutures {
    pub(crate) fn fetch_book_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: &str,
        limit: Option<u32>,
    ) {
        match cx.command_wait() {
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return;
            }
            None => {
                let _ = cx.reject_unstamped_command(Operation::Read);
                return;
            }
            Some(CommandWait::Remaining(_)) => {}
        }
        let Ok(root) = cx.admit_caller_read_root(None, None) else {
            return;
        };
        let completion = match ReadCompletion::from_context(cx, command, root.clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(&root, error.read_venue_error());
                return;
            }
        };
        let query = format!(
            "symbol={}&limit={}",
            safe_cex_engine::url::encode(symbol),
            limit.unwrap_or(200)
        );
        self.send_book_query(
            cx,
            Request {
                completion,
                leg: None,
            },
            query,
        );
    }
    pub(crate) fn send_book_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        query: String,
    ) {
        if let Err(error) = cx.validate_http_context(request.completion.root()) {
            if error != safe_cex_engine::request_context::Error::Revoked {
                request.failed(cx, error.read_venue_error());
            }
            return;
        }
        if cx.now < self.paused_until {
            let timer = cx.set_timer(self.paused_until - cx.now);
            self.deferred_gets.push(DeferredGet {
                timer,
                seed_generation: self.seed_generation,
                path: paths::DEPTH.into(),
                query,
                task: Task::BookQuery(request),
            });
            return;
        }
        let leg = match cx.http_leg_context(
            request.completion.root(),
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(leg) => leg,
            Err(error) => {
                request.failed(cx, error.read_venue_error());
                return;
            }
        };
        request.leg = Some(leg.clone());
        let pending = Pending {
            task: Task::BookQuery(request),
            invalid: false,
            skipped: Vec::new(),
            seed_generation: self.seed_generation,
        };
        let queued = cx.request_managed_with_headers(
            leg,
            None,
            Method::Get,
            format!("{}{}?{query}", paths::REST, paths::DEPTH),
            &[],
            None,
            pending,
        );
        if let Err((
            Pending {
                task: Task::BookQuery(request),
                ..
            },
            error,
        )) = queued
        {
            request.failed(cx, error.read_venue_error());
        }
    }

    pub(crate) fn book_query_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if !request.delivered(cx) {
            request.retire_leg(cx);
            return;
        }
        if response.status == 429 {
            self.paused_until = self
                .paused_until
                .max(cx.now + rate::delay(response, cx.now));
        }
        let envelope = envelope(response.body);
        let native = envelope.as_ref().ok().and_then(|e| {
            text(e.code).map(Cow::into_owned).or_else(|| {
                e.code
                    .and_then(|raw| serde_json::from_str::<i64>(raw.get()).ok())
                    .map(|n| n.to_string())
            })
        });
        let refused = native
            .as_deref()
            .is_some_and(|code| !matches!(code, "0" | "200"));
        if !(200..300).contains(&response.status) || refused {
            let kind = match response.status {
                429 => ErrorKind::RateLimited,
                408 | 504 => ErrorKind::Timeout,
                _ => ErrorKind::VenueUnavailable,
            };
            let mut error = failure(
                format!("WEEX order-book query HTTP {}", response.status),
                kind,
            );
            if let Some(code) = native {
                error = error.venue_code(code);
            }
            if let Ok(e) = envelope
                && let Some(message) = text(e.msg)
                    .filter(|s| !s.is_empty())
                    .or_else(|| text(e.message).filter(|s| !s.is_empty()))
            {
                error.message.push_str(": ");
                error.message.push_str(&message);
                error = error.venue_message(message.into_owned());
            }
            if kind == ErrorKind::RateLimited {
                error = error.retry_after_ms(rate::delay(response, cx.now));
            }
            request.failed(cx, error);
            return;
        }
        let malformed_code = envelope.as_ref().is_ok_and(|e| e.code.is_some()) && native.is_none();
        let result = envelope
            .and_then(|e| {
                if malformed_code {
                    return Err(invalid());
                }
                Ok(Output {
                    bids: levels(e.bids.or(e.b))?,
                    asks: levels(e.asks.or(e.a))?,
                    last_update_id: version(e.last_update_id)?,
                })
            })
            .and_then(|book| serde_json::to_string(&book).map_err(|_| invalid()));
        match result {
            Ok(json) => request.complete(cx, json),
            Err(error) => request.failed(cx, error),
        }
    }
}
