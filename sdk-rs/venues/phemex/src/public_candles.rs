//! Unsigned candle commands borrow the engine's prepared HTTP authority.
use crate::{Pending, PhemexPerpetuals, resolution};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{Cx, Method, Venue};

impl PhemexPerpetuals {
    pub(crate) fn fetch_public_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        interval: Interval,
        limit: Option<u32>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
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
        if cx.now < self.paused_until {
            let _ = cx.fail_context(
                &context,
                "Phemex requests are rate limited; candle read was not sent",
            );
            return;
        }
        let Some(n) = resolution(interval) else {
            let _ = cx.fail_context(&context, interval.unsupported("Phemex"));
            return;
        };
        let to = to_ms.unwrap_or(cx.now);
        let requested_limit = limit.filter(|n| *n > 0);
        let default_from = to - n as f64 * 1000. * requested_limit.unwrap_or(500).min(1000) as f64;
        let from = if requested_limit.is_some() {
            default_from
        } else {
            from_ms.unwrap_or(default_from)
        };
        let url = format!(
            "{}/exchange/public/md/v2/kline/list?symbol={}&resolution={n}&from={:.0}&to={:.0}",
            self.rest(),
            safe_cex_engine::url::encode(self.native_symbol(symbol)),
            (from / 1000.).floor(),
            (to / 1000.).ceil(),
        );
        match cx.request_managed(
            context.clone(),
            None,
            Method::Get,
            url,
            None,
            Pending::PublicCandles {
                context,
                rows: vec![],
                invalid: false,
            },
        ) {
            Ok(id) => cx.stream_response(id, ArrayAt::Path(&["data", "rows"])),
            Err((pending, reason)) => self.on_sign_error(cx, pending, reason.venue_error()),
        }
    }
}
