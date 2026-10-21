//! Command-owned public snapshots never publish subscription depth.
use crate::{Pending, WeexFutures, rate};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Outcome, Response, VenueError};
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
impl WeexFutures {
    pub(crate) fn book_query_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Response<'_>,
    ) {
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
            cx.fail(command, error);
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
            Ok(json) => cx.done(command, json),
            Err(error) => cx.fail(command, error),
        }
    }
}
