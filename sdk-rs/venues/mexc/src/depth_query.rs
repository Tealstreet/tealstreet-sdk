//! One-shot snapshots never alter subscription books or their recovery state.
use crate::{MexcFutures, Pending, wire};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{Cx, ErrorKind, Method, Outcome, Response, Venue, VenueError};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Snapshot<'a> {
    #[serde(borrow, deserialize_with = "wire::id")]
    version: wire::Str<'a>,
    bids: Vec<wire::Level>,
    asks: Vec<wire::Level>,
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

fn error(message: impl Into<String>, kind: ErrorKind) -> VenueError {
    VenueError::new(message)
        .kind(kind)
        .outcome(Outcome::NotSent)
}
fn levels(
    raw: Vec<wire::Level>,
    size: f64,
    limit: usize,
    bids: bool,
) -> Result<Vec<Level>, VenueError> {
    let mut out = Vec::with_capacity(raw.len());
    for wire::Level(price, volume, _) in raw {
        let amount = volume * size;
        if price <= 0.0 || volume < 0.0 || !amount.is_finite() {
            return Err(error(
                "Invalid MEXC depth snapshot level",
                ErrorKind::Internal,
            ));
        }
        out.push(Level {
            price,
            amount,
            total: 0.0,
        });
    }
    out.sort_by(|a, b| {
        if bids {
            b.price.total_cmp(&a.price)
        } else {
            a.price.total_cmp(&b.price)
        }
    });
    let mut total = 0.0;
    for row in &mut out {
        total += row.amount;
        if !total.is_finite() {
            return Err(error(
                "MEXC depth snapshot total overflow",
                ErrorKind::Internal,
            ));
        }
        row.total = total;
    }
    out.truncate(limit);
    Ok(out)
}
fn snapshot(body: &str, size: f64, limit: usize) -> Result<String, VenueError> {
    let envelope = wire::Envelope::checked(body).map_err(|e| error(e, ErrorKind::Internal))?;
    let raw: Snapshot<'_> = serde_json::from_str(
        envelope
            .payload()
            .map_err(|e| error(e, ErrorKind::Internal))?,
    )
    .map_err(|_| error("Invalid MEXC depth snapshot", ErrorKind::Internal))?;
    if raw.version.is_empty()
        || !raw.version.bytes().all(|c| c.is_ascii_digit())
        || raw.version.parse::<u64>().is_err()
    {
        return Err(error(
            "Invalid MEXC depth snapshot version",
            ErrorKind::Internal,
        ));
    }
    serde_json::to_string(&Output {
        last_update_id: raw.version.into_owned(),
        bids: levels(raw.bids, size, limit, true)?,
        asks: levels(raw.asks, size, limit, false)?,
    })
    .map_err(|_| {
        error(
            "MEXC depth snapshot serialization failed",
            ErrorKind::Internal,
        )
    })
}
impl MexcFutures {
    pub(crate) fn fetch_book(
        &mut self,
        cx: &mut Cx<'_, Pending>,
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
        let limit = limit.unwrap_or(100);
        let Some(market) = cx.store.market(symbol) else {
            let _ = cx.fail_context(
                &context,
                error("Unknown MEXC market", ErrorKind::InvalidInput),
            );
            return;
        };
        if !(1..=1000).contains(&limit) {
            let _ = cx.fail_context(
                &context,
                error(
                    "MEXC order book limit must be between 1 and 1000",
                    ErrorKind::InvalidInput,
                ),
            );
            return;
        }
        let size = market.contract_size.unwrap_or(1.0);
        if !size.is_finite() || size <= 0.0 {
            let _ = cx.fail_context(
                &context,
                error("Invalid MEXC market contract size", ErrorKind::Internal),
            );
            return;
        }
        if cx.now < self.depth_paused_until {
            let _ = cx.fail_context(
                &context,
                error(
                    "MEXC depth requests are rate limited",
                    ErrorKind::RateLimited,
                )
                .retry_after_ms(self.depth_paused_until - cx.now),
            );
            return;
        }
        let path = format!(
            "/api/v1/contract/depth/{}?limit={limit}",
            safe_cex_engine::url::encode(&market.id)
        );
        if let Err((pending, reason)) = cx.request_managed(
            context.clone(),
            None,
            Method::Get,
            format!("{}{path}", crate::REST),
            None,
            Pending::BookQuery {
                context,
                size,
                limit: limit as usize,
            },
        ) {
            self.on_sign_error(cx, pending, reason.venue_error());
        }
    }
    pub(crate) fn book_query_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        size: f64,
        limit: usize,
        response: Response<'_>,
    ) {
        let envelope = serde_json::from_str::<wire::Envelope<'_>>(response.body).ok();
        if response.status != 200
            || envelope
                .as_ref()
                .is_none_or(|e| e.success != Some(true) || e.code != 0.0)
        {
            let kind = match response.status {
                429 => ErrorKind::RateLimited,
                // This is unsigned public data; a refusal cannot invalidate account keys.
                401 | 403 => ErrorKind::VenueUnavailable,
                408 | 504 => ErrorKind::Timeout,
                500..=599 => ErrorKind::VenueUnavailable,
                200 if envelope.as_ref().is_some_and(|e| e.code == 510.0) => ErrorKind::RateLimited,
                200 if envelope.is_none() => ErrorKind::Internal,
                // Unknown public refusals may also carry auth-looking venue text.
                // Keep their classification specific and independent of account health.
                _ => ErrorKind::VenueUnavailable,
            };
            let mut failure = error(format!("MEXC depth query HTTP {}", response.status), kind);
            if let Some(e) = envelope {
                failure = failure.venue_code(e.code.to_string());
                if !e.message.is_empty() {
                    failure.message.push_str(": ");
                    failure.message.push_str(&e.message);
                    failure = failure.venue_message(e.message.into_owned());
                }
            }
            if kind == ErrorKind::RateLimited {
                let wait = response
                    .header("retry-after")
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|s| s.is_finite() && *s >= 0.0 && (*s * 1000.0).is_finite())
                    .map_or(10_000.0, |s| s * 1000.0);
                self.depth_paused_until = self.depth_paused_until.max(cx.now + wait);
                self.account.paused_until = self.account.paused_until.max(self.depth_paused_until);
                failure = failure.retry_after_ms(self.depth_paused_until - cx.now);
            }
            let _ = cx.fail_context(&context, failure);
            return;
        }
        match snapshot(response.body, size, limit) {
            Ok(json) => {
                let _ = cx.done_context(&context, json);
            }
            Err(failure) => {
                let _ = cx.fail_context(&context, failure);
            }
        }
    }
}
