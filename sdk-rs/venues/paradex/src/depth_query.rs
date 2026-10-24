//! Request-owned REST books never enter the live SBE/JSON book state machine.
use crate::{Paradex, Pending};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::borrow::Cow;

#[derive(Deserialize)]
struct Envelope<'a> {
    #[serde(borrow)]
    error: Option<&'a RawValue>,
    #[serde(borrow)]
    message: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Detail<'a> {
    #[serde(borrow)]
    code: Option<&'a RawValue>,
    #[serde(borrow)]
    message: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Snapshot<'a> {
    #[serde(borrow)]
    bids: Vec<Vec<&'a RawValue>>,
    #[serde(borrow)]
    asks: Vec<Vec<&'a RawValue>>,
}
#[derive(Serialize)]
struct Level {
    price: f64,
    amount: f64,
    total: f64,
}
#[derive(Serialize)]
struct Output {
    bids: Vec<Level>,
    asks: Vec<Level>,
}
pub(crate) fn failure(message: impl Into<String>, kind: ErrorKind) -> VenueError {
    VenueError::new(message)
        .kind(kind)
        .outcome(Outcome::NotSent)
}
fn invalid() -> VenueError {
    failure("Invalid Paradex order-book snapshot", ErrorKind::Internal)
}
fn object<'a, T: Deserialize<'a>>(raw: &'a str) -> Result<T, VenueError> {
    if !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    serde_json::from_str(raw).map_err(|_| invalid())
}
fn text(raw: Option<&RawValue>) -> Option<Cow<'_, str>> {
    raw.and_then(|v| serde_json::from_str::<Cow<'_, str>>(v.get()).ok())
        .filter(|s| !s.is_empty())
}
fn levels(rows: Vec<Vec<&RawValue>>) -> Result<Vec<Level>, VenueError> {
    rows.into_iter()
        .map(|row| {
            let price = text(row.first().copied())
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|n| n.is_finite() && *n > 0.0)
                .ok_or_else(invalid)?;
            let amount = text(row.get(1).copied())
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|n| n.is_finite() && *n >= 0.0)
                .ok_or_else(invalid)?;
            let total = price * amount;
            if !total.is_finite() {
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
impl Paradex {
    pub(crate) fn fetch_book_query(
        &self,
        cx: &mut Cx<'_, Pending>,
        _command: CommandId,
        symbol: &str,
        limit: Option<u32>,
    ) {
        let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) else {
            return;
        };
        if cx
            .request_managed(
                context.clone(),
                Method::Get,
                format!(
                    "{}/orderbook/{}?depth={}",
                    self.rest(),
                    safe_cex_engine::url::encode(symbol),
                    limit.unwrap_or(20)
                ),
                None,
                Pending::BookQuery(context.clone()),
            )
            .is_err()
        {
            let _ = cx.fail_context(
                &context,
                failure(
                    "Paradex book preparation is unavailable",
                    ErrorKind::Internal,
                ),
            );
        }
    }
    pub(crate) fn book_query_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        response: Response<'_>,
    ) {
        if cx.validate_read_http_delivery(context, context).is_err() {
            return;
        }
        let envelope = object::<Envelope<'_>>(response.body);
        let native = envelope
            .as_ref()
            .ok()
            .and_then(|e| e.error)
            .filter(|raw| raw.get() != "\"\"");
        if !(200..300).contains(&response.status) || native.is_some() {
            let kind = match response.status {
                429 => ErrorKind::RateLimited,
                408 | 504 => ErrorKind::Timeout,
                _ => ErrorKind::VenueUnavailable,
            };
            let mut error = failure(
                format!("Paradex order-book query HTTP {}", response.status),
                kind,
            );
            let detail = native.and_then(|v| object::<Detail<'_>>(v.get()).ok());
            let code = text(native).map(Cow::into_owned).or_else(|| {
                detail.as_ref().and_then(|d| {
                    text(d.code).map(Cow::into_owned).or_else(|| {
                        d.code
                            .and_then(|raw| serde_json::from_str::<i64>(raw.get()).ok())
                            .map(|n| n.to_string())
                    })
                })
            });
            let message = envelope
                .as_ref()
                .ok()
                .and_then(|e| text(e.message))
                .or_else(|| detail.as_ref().and_then(|d| text(d.message)));
            if let Some(code) = code {
                let code: String = code.chars().take(512).collect();
                error.message.push_str(": ");
                error.message.push_str(&code);
                error = error.venue_code(code);
            }
            if let Some(message) = message {
                let message: String = message.chars().take(512).collect();
                error.message.push_str(": ");
                error.message.push_str(&message);
                error = error.venue_message(message);
            }
            if kind == ErrorKind::RateLimited
                && let Some(seconds) = response
                    .header("retry-after")
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|n| n.is_finite() && *n >= 0.0)
            {
                error = error.retry_after_ms(
                    (seconds * 1000.0).min(safe_cex_engine::startup::MAX_RATE_LIMIT_PAUSE_MS),
                );
            }
            let _ = cx.fail_context(context, error);
            return;
        }
        let result = envelope
            .and_then(|_| object::<Snapshot<'_>>(response.body))
            .and_then(|book| {
                Ok(Output {
                    bids: levels(book.bids)?,
                    asks: levels(book.asks)?,
                })
            })
            .and_then(|book| serde_json::to_string(&book).map_err(|_| invalid()));
        match result {
            Ok(json) => {
                let _ = cx.done_context(context, json);
            }
            Err(error) => {
                let _ = cx.fail_context(context, error);
            }
        }
    }
}
