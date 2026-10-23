//! Request-owned REST snapshots never enter the streaming book cache.
use crate::{Backpack, Pending};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{Cx, ErrorKind, Outcome, Response, VenueError};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
#[derive(Deserialize)]
struct Snapshot<'a> {
    #[serde(borrow)]
    bids: Vec<[Cow<'a, str>; 2]>,
    #[serde(borrow)]
    asks: Vec<[Cow<'a, str>; 2]>,
}
#[derive(Serialize)]
struct Level<'a> {
    id: Cow<'a, str>,
    price: f64,
    amount: f64,
    total: f64,
}
#[derive(Serialize)]
struct Output<'a> {
    bids: Vec<Level<'a>>,
    asks: Vec<Level<'a>>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Code<'a> {
    Text(#[serde(borrow)] Cow<'a, str>),
    Integer(i64),
}
impl Code<'_> {
    fn text(&self) -> String {
        match self {
            Self::Text(s) => s.to_string(),
            Self::Integer(n) => n.to_string(),
        }
    }
    fn nonzero(&self) -> bool {
        match self {
            Self::Text(s) => !s.is_empty(),
            Self::Integer(n) => *n != 0,
        }
    }
}
#[derive(Deserialize)]
struct Envelope<'a> {
    #[serde(borrow)]
    error: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow)]
    code: Option<Code<'a>>,
    #[serde(borrow)]
    msg: Option<Cow<'a, str>>,
    #[serde(borrow)]
    message: Option<Cow<'a, str>>,
}
#[derive(Deserialize)]
struct NativeMessage<'a> {
    #[serde(borrow)]
    msg: Cow<'a, str>,
}
pub(crate) fn failure(message: impl Into<String>, kind: ErrorKind) -> VenueError {
    VenueError::new(message)
        .kind(kind)
        .outcome(Outcome::NotSent)
}
fn levels(raw: Vec<[Cow<'_, str>; 2]>) -> Result<Vec<Level<'_>>, VenueError> {
    raw.into_iter()
        .map(|[id, qty]| {
            let price = id.parse::<f64>().ok().filter(|x| x.is_finite() && *x > 0.0);
            let amount = qty
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite() && *x >= 0.0);
            match (price, amount) {
                (Some(price), Some(amount)) => Ok(Level {
                    id,
                    price,
                    amount,
                    total: 0.0,
                }),
                _ => Err(failure(
                    "Invalid Backpack order-book level",
                    ErrorKind::Internal,
                )),
            }
        })
        .collect()
}
impl Backpack {
    pub(crate) fn book_query_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        response: Response<'_>,
    ) {
        if cx.validate_read_http_delivery(context, context).is_err() {
            return;
        }
        let envelope = serde_json::from_str::<Envelope<'_>>(response.body);
        if (200..300).contains(&response.status) && envelope.is_err() {
            let _ = cx.fail_context(
                context,
                failure("Invalid Backpack order-book envelope", ErrorKind::Internal),
            );
            return;
        }
        let envelope = envelope.ok();
        let refused = envelope
            .as_ref()
            .is_some_and(|e| e.error.is_some() || e.code.as_ref().is_some_and(Code::nonzero));
        if !(200..300).contains(&response.status) || refused {
            // Public refusals cannot prove anything about account credentials.
            let kind = match response.status {
                429 => ErrorKind::RateLimited,
                408 | 504 => ErrorKind::Timeout,
                _ => ErrorKind::VenueUnavailable,
            };
            let mut error = failure(
                format!("Backpack order-book query HTTP {}", response.status),
                kind,
            );
            if let Some(e) = envelope {
                if let Some(code) = e.code {
                    error = error.venue_code(code.text());
                }
                let nested = e.error.and_then(|v| {
                    serde_json::from_str::<NativeMessage<'_>>(v.get())
                        .map(|x| x.msg)
                        .ok()
                        .or_else(|| serde_json::from_str::<Cow<'_, str>>(v.get()).ok())
                });
                if let Some(message) = e
                    .msg
                    .filter(|s| !s.is_empty())
                    .or(e.message.filter(|s| !s.is_empty()))
                    .or(nested.filter(|s| !s.is_empty()))
                {
                    error.message.push_str(": ");
                    error.message.push_str(&message);
                    error = error.venue_message(message.into_owned());
                }
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
        let result = serde_json::from_str::<Snapshot<'_>>(response.body)
            .map_err(|_| failure("Invalid Backpack order-book snapshot", ErrorKind::Internal))
            .and_then(|s| {
                Ok(Output {
                    bids: levels(s.bids)?,
                    asks: levels(s.asks)?,
                })
            })
            .and_then(|book| {
                serde_json::to_string(&book).map_err(|_| {
                    failure(
                        "Invalid Backpack order-book serialization",
                        ErrorKind::Internal,
                    )
                })
            });
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
