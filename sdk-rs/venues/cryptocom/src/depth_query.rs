//! Public REST snapshots belong to their command, never the subscription cache.
use crate::{Cryptocom, Pending};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{Cx, ErrorKind, Method, Outcome, Response, Venue, VenueError};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use std::borrow::Cow;

// An explicit null result differs from a missing result in the TS interceptor.
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<&'de RawValue>, D::Error> {
    <&RawValue>::deserialize(d).map(Some)
}
#[derive(Deserialize)]
struct Envelope<'a> {
    code: i64,
    #[serde(borrow)]
    message: Option<&'a RawValue>,
    #[serde(default, borrow, deserialize_with = "present")]
    result: Option<&'a RawValue>,
    #[serde(borrow)]
    data: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Data<'a> {
    #[serde(borrow)]
    data: Option<&'a RawValue>,
}
#[derive(Deserialize)]
struct Snapshot<'a> {
    #[serde(borrow)]
    bids: Vec<Vec<&'a RawValue>>,
    #[serde(borrow)]
    asks: Vec<Vec<&'a RawValue>>,
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
pub(crate) fn failure(message: impl Into<String>, kind: ErrorKind) -> VenueError {
    VenueError::new(message)
        .kind(kind)
        .outcome(Outcome::NotSent)
}
fn invalid() -> VenueError {
    failure(
        "Invalid Crypto.com order-book snapshot",
        ErrorKind::Internal,
    )
}
// Named Serde structs otherwise also accept sequences such as [0].
fn object<'a, T: Deserialize<'a>>(raw: &'a str) -> Result<T, VenueError> {
    if !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    serde_json::from_str(raw).map_err(|_| invalid())
}
fn levels(rows: Vec<Vec<&RawValue>>) -> Result<Vec<Level<'_>>, VenueError> {
    rows.into_iter()
        .map(|row| {
            let id = serde_json::from_str::<Cow<'_, str>>(row.first().ok_or_else(invalid)?.get())
                .map_err(|_| invalid())?;
            let quantity =
                serde_json::from_str::<Cow<'_, str>>(row.get(1).ok_or_else(invalid)?.get())
                    .map_err(|_| invalid())?;
            let price = id.parse::<f64>().ok().filter(|n| n.is_finite() && *n > 0.0);
            let amount = quantity
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.0);
            match (price, amount) {
                (Some(price), Some(amount)) => Ok(Level {
                    id,
                    price,
                    amount,
                    total: 0.0,
                }),
                _ => Err(invalid()),
            }
        })
        .collect()
}
fn snapshot<'a>(envelope: Envelope<'a>) -> Result<Output<'a>, VenueError> {
    let data = match envelope.result {
        Some(result) => object::<Data<'_>>(result.get())?.data,
        None => envelope.data,
    };
    let rows = data
        .map(|v| serde_json::from_str::<Vec<&RawValue>>(v.get()))
        .transpose()
        .map_err(|_| invalid())?
        .unwrap_or_default();
    match rows.first().filter(|v| v.get() != "null") {
        Some(row) => {
            let book = object::<Snapshot<'_>>(row.get())?;
            Ok(Output {
                bids: levels(book.bids)?,
                asks: levels(book.asks)?,
            })
        }
        None => Ok(Output {
            bids: vec![],
            asks: vec![],
        }),
    }
}
impl Cryptocom {
    pub(crate) fn fetch_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Ok(context) = cx.admit_http_command(Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) else {
            return;
        };
        if let Err((pending, reason)) = cx.request_managed(
            context.clone(),
            None,
            Method::Get,
            format!(
                "{}/public/get-book?instrument_name={}&depth=50",
                self.rest,
                crate::encode(symbol)
            ),
            None,
            Pending::BookQuery { context },
        ) {
            self.on_sign_error(cx, pending, reason.venue_error());
        }
    }

    pub(crate) fn book_query_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        response: Response<'_>,
    ) {
        let envelope = object::<Envelope<'_>>(response.body);
        let native = envelope.as_ref().ok().map(|e| e.code).unwrap_or_default();
        if !(200..300).contains(&response.status) || native != 0 {
            // An unsigned public request never supplies account-auth evidence.
            let kind = match response.status {
                429 => ErrorKind::RateLimited,
                408 | 504 => ErrorKind::Timeout,
                200..300 => match native {
                    42901 => ErrorKind::RateLimited,
                    40801 => ErrorKind::Timeout,
                    _ => ErrorKind::VenueUnavailable,
                },
                _ => ErrorKind::VenueUnavailable,
            };
            let mut error = failure(
                format!("Crypto.com order-book query HTTP {}", response.status),
                kind,
            );
            if let Ok(e) = envelope {
                error = error.venue_code(e.code.to_string());
                if let Some(message) = e
                    .message
                    .and_then(|v| serde_json::from_str::<Cow<'_, str>>(v.get()).ok())
                    .filter(|s| !s.is_empty())
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
            let _ = cx.fail_context(&context, error);
            return;
        }
        let result = envelope
            .map_err(|_| invalid())
            .and_then(snapshot)
            .and_then(|book| serde_json::to_string(&book).map_err(|_| invalid()));
        match result {
            Ok(json) => {
                let _ = cx.done_context(&context, json);
            }
            Err(error) => {
                let _ = cx.fail_context(&context, error);
            }
        }
    }
}
