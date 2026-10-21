//! Independent public MDG snapshots do not publish to the streaming book.
use crate::{Grvt, Pending, wire};
use safe_cex_engine::{CommandId, Cx, ErrorKind, Outcome, Response, VenueError};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;

fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<&'de RawValue>, D::Error> {
    <&RawValue>::deserialize(d).map(Some)
}
#[derive(Deserialize)]
struct Refusal<'a> {
    #[serde(
        default,
        borrow,
        rename = "c",
        alias = "code",
        deserialize_with = "present"
    )]
    code: Option<&'a RawValue>,
    #[serde(borrow, rename = "m", alias = "message")]
    message: Option<&'a RawValue>,
    #[serde(borrow, rename = "e", alias = "error")]
    error: Option<&'a RawValue>,
}
impl Refusal<'_> {
    fn code(&self) -> Option<wire::Text<'_>> {
        self.code
            .and_then(|raw| serde_json::from_str(raw.get()).ok())
    }
    fn message(&self) -> Option<std::borrow::Cow<'_, str>> {
        self.message
            .and_then(|raw| serde_json::from_str::<std::borrow::Cow<'_, str>>(raw.get()).ok())
            .filter(|s| !s.is_empty())
    }
}
#[derive(Deserialize)]
struct ResultEnvelope<'a> {
    #[serde(borrow, rename = "r", alias = "result")]
    result: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Snapshot<'a> {
    #[serde(borrow, rename = "b", alias = "bids")]
    bids: Option<&'a RawValue>,
    #[serde(borrow, rename = "a", alias = "asks")]
    asks: Option<&'a RawValue>,
}
#[derive(Serialize)]
struct Level {
    price: f64,
    amount: f64,
    total: f64,
}
#[derive(Default, Serialize)]
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
    failure("Invalid GRVT order-book snapshot", ErrorKind::Internal)
}
fn object<'a, T: Deserialize<'a>>(raw: &'a str) -> Result<T, VenueError> {
    if !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    serde_json::from_str(raw).map_err(|_| invalid())
}
fn levels(raw: Option<&RawValue>) -> Result<Vec<Level>, VenueError> {
    let rows = raw
        .map(|v| serde_json::from_str::<Vec<&RawValue>>(v.get()))
        .transpose()
        .map_err(|_| invalid())?
        .unwrap_or_default();
    let mut total = 0.0;
    rows.into_iter()
        .map(|row| {
            let row = object::<wire::Level>(row.get())?;
            let (price, amount) = (row.price.0, row.size.0);
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
impl Grvt {
    pub(crate) fn book_query_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Response<'_>,
    ) {
        let envelope = object::<Refusal<'_>>(response.body);
        let root = envelope.as_ref().ok();
        let nested = root
            .and_then(|e| e.error)
            .and_then(|raw| object::<Refusal<'_>>(raw.get()).ok());
        let root_code = root.and_then(Refusal::code);
        let nested_code = nested.as_ref().and_then(Refusal::code);
        let nonzero = |code: &&wire::Text<'_>| !code.is_empty() && &***code != "0";
        let native = nested_code
            .as_ref()
            .filter(nonzero)
            .or_else(|| root_code.as_ref().filter(nonzero))
            .or(nested_code.as_ref())
            .or(root_code.as_ref())
            .map(|v| &**v)
            .unwrap_or_default();
        let refused =
            !native.is_empty() && native != "0" || root.is_some_and(|e| e.error.is_some());
        if !(200..300).contains(&response.status) || refused {
            let kind = match response.status {
                429 => ErrorKind::RateLimited,
                408 | 504 => ErrorKind::Timeout,
                200..300 if native == "1006" => ErrorKind::RateLimited,
                _ => ErrorKind::VenueUnavailable,
            };
            let mut error = failure(
                format!("GRVT order-book query HTTP {}", response.status),
                kind,
            );
            if !native.is_empty() {
                error = error.venue_code(native);
            }
            if let Some(message) = nested
                .as_ref()
                .and_then(Refusal::message)
                .or_else(|| root.and_then(Refusal::message))
            {
                error.message.push_str(": ");
                error.message.push_str(&message);
                error = error.venue_message(message.into_owned());
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
            cx.fail(command, error);
            return;
        }
        let malformed_code = root.is_some_and(|e| e.code.is_some()) && root_code.is_none();
        let result = envelope
            .and_then(|_| {
                if malformed_code {
                    Err(invalid())
                } else {
                    object::<ResultEnvelope<'_>>(response.body)
                }
            })
            .and_then(|e| match e.result {
                None => Ok(Output::default()),
                Some(raw) => {
                    let book = object::<Snapshot<'_>>(raw.get())?;
                    Ok(Output {
                        bids: levels(book.bids)?,
                        asks: levels(book.asks)?,
                    })
                }
            })
            .and_then(|book| serde_json::to_string(&book).map_err(|_| invalid()));
        match result {
            Ok(json) => cx.done(command, json),
            Err(error) => cx.fail(command, error),
        }
    }
}
