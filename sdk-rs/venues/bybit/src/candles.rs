//! Public klines on the engine's managed Read authority: one command Context
//! with a native 20 s window, streamed `result.list`, typed terminals.
use crate::policy::read_policy;
use crate::sign::{self, Query};
use crate::trading::codes::BYBIT;
use crate::wire::{KlineRow, Status, float};
use crate::{BybitLinear, Category, MARKETS_PATH, Pending, Rest, Then, interval_code};
use safe_cex_engine::classify::{self, Native, NativeCode, Released};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Interval};
use safe_cex_engine::request_context::Context;

use safe_cex_engine::{Cx, ErrorKind, Method, Outcome, Response, VenueError};

/// One kline read in flight.
pub struct Read {
    context: Context,
    candles: Vec<Candle>,
}

fn refused(message: impl Into<String>) -> VenueError {
    VenueError::new(message)
        .kind(ErrorKind::Unsupported)
        .outcome(Outcome::NotSent)
}

impl BybitLinear {
    pub(crate) fn fetch_candles(
        &self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        interval: Interval,
        (limit, from_ms, to_ms): (Option<u32>, Option<f64>, Option<f64>),
    ) {
        // The engine owns an initial admission refusal.
        let Ok(context) = cx.admit_http_command(read_policy()) else {
            return;
        };
        // Invalid options send nothing; the read fails instead of hanging.
        if let Some(error) = self.config_error.clone() {
            let error = VenueError::new(error)
                .kind(ErrorKind::InvalidInput)
                .outcome(Outcome::NotSent);
            let _ = cx.fail_context(&context, error);
            return;
        }
        // `on_command` has already refused a disabled product.
        if self.category_for(symbol) == Category::Option {
            let _ = cx.fail_context(&context, refused("Bybit has no candles for options"));
            return;
        }
        let Some(code) = interval_code(interval) else {
            let _ = cx.fail_context(&context, refused(interval.unsupported("Bybit")));
            return;
        };
        let query = Query::new()
            .str("category", self.paths.category)
            .str("symbol", symbol)
            .str("interval", code)
            // Like TS, a limit asks for the newest bars up to `to`; `from` only bounds an unlimited read.
            .opt_num("start", from_ms.filter(|_| limit.is_none_or(|n| n == 0)))
            .opt_num("end", to_ms)
            .num(
                "limit",
                f64::from(limit.filter(|n| *n > 0).unwrap_or(500).min(1000)),
            );
        let rest = Rest::get("/v5/market/kline", query).streamed();
        let url = format!("{}{}?{}", self.paths.rest, rest.path, rest.payload);
        let pending = Pending {
            category: self.category,
            rest,
            then: Then::Candles(Read {
                context: context.clone(),
                candles: Vec::new(),
            }),
            read: None,
        };
        match cx.request_managed_with_headers(
            context.clone(),
            None,
            Method::Get,
            url,
            &[("X-Referer", sign::BROKER_ID)],
            None,
            pending,
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::Path(MARKETS_PATH)),
            Err((_, error)) => {
                let _ = cx.fail_context(&context, error.venue_error());
            }
        }
    }

    pub(crate) fn candles_item(cx: &mut Cx<'_, Pending>, read: &mut Read, item: &str) {
        // Rows publish only under this exact live delivery.
        if cx
            .validate_read_http_delivery(&read.context, &read.context)
            .is_err()
        {
            return;
        }
        if let Ok(KlineRow(row)) = serde_json::from_str::<KlineRow>(item)
            && row.len() >= 6
        {
            read.candles.push(Candle {
                timestamp: float(&row[0]) / 1000.0,
                open: float(&row[1]),
                high: float(&row[2]),
                low: float(&row[3]),
                close: float(&row[4]),
                volume: float(&row[5]),
            });
        }
    }

    pub(crate) fn candles_response(
        cx: &mut Cx<'_, Pending>,
        mut read: Read,
        response: &Response<'_>,
    ) {
        if cx
            .validate_read_http_delivery(&read.context, &read.context)
            .is_err()
        {
            return;
        }
        if let Some((code, message)) = crate::venue_error(response) {
            // Only a parsed retCode is native evidence; a bare HTTP error is not.
            let native_code = serde_json::from_str::<Status>(response.body)
                .ok()
                .and_then(|status| status.ret_code)
                .is_some()
                .then_some(code);
            let native = native_code.map(|code| Native {
                code: NativeCode::Int(code),
                message: &message,
            });
            let failure = classify::read(
                &BYBIT,
                Released::Response(response),
                native.as_ref(),
                format!("Bybit klines: {message}"),
            );
            let _ = cx.fail_context(&read.context, failure.error);
            return;
        }
        // Bybit lists newest first.
        read.candles
            .sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
        let _ = cx.done_context(
            &read.context,
            serde_json::to_string(&read.candles).unwrap_or_default(),
        );
    }

    /// Transport loss, expiry, a malformed stream or a refused preparation:
    /// the engine's typed error is the command's.
    pub(crate) fn candles_failed(cx: &mut Cx<'_, Pending>, read: Read, error: VenueError) {
        let _ = cx.fail_context(&read.context, error);
    }
}
