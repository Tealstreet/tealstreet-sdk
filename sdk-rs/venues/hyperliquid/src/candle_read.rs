//! Missing public rows wait only for their catalogue, within the original Context.
use super::*;

pub(super) struct CandleQuery {
    pub context: Context,
    pub symbol: String,
    pub interval: Interval,
    pub limit: Option<u32>,
    pub from_ms: Option<f64>,
    pub to_ms: Option<f64>,
    pub now: f64,
}

pub(super) struct WaitingCandle {
    read: CandleQuery,
    timer: TimerId,
}

impl Hyperliquid {
    fn candle_discovery_pending(&self, symbol: &str) -> bool {
        if self.spot {
            return self.core_discovery_incomplete;
        }
        if let Some(coin) = symbol.strip_suffix("-PERP") {
            let native = coin.replace('|', ":").to_ascii_lowercase();
            if self.excluded.contains(&native) || self.candle_excluded.contains(&native) {
                return false;
            }
            let Some((dex, _)) = coin.split_once('|') else {
                return self.core_discovery_incomplete;
            };
            return !self.candle_dex_catalog_complete
                || (self
                    .dex_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(dex))
                    && !self
                        .candle_complete_scopes
                        .contains(&dex.to_ascii_lowercase()));
        }
        symbol.contains('/') && self.spot_markets && !self.spot_committed
    }

    pub(super) fn start_candle_read(&mut self, cx: &mut Cx<'_, Pending>, read: CandleQuery) {
        if self.coin(cx, &read.symbol).is_none() && self.candle_discovery_pending(&read.symbol) {
            let timer = cx.set_timer((read.context.deadline() - cx.now).max(0.));
            self.waiting_candles.push(WaitingCandle { read, timer });
        } else {
            self.dispatch_candle_read(cx, read);
        }
    }

    fn dispatch_candle_read(&mut self, cx: &mut Cx<'_, Pending>, read: CandleQuery) {
        let Some(coin) = self.coin(cx, &read.symbol) else {
            let _ = cx.fail_context(&read.context, "Market not found");
            return;
        };
        let Some(body) = candle_body(
            read.now,
            coin,
            read.interval,
            read.limit,
            read.from_ms,
            read.to_ms,
        ) else {
            let _ = cx.fail_context(&read.context, "Invalid candle time range");
            return;
        };
        self.queue_candles(
            cx,
            body,
            Box::new(ManagedCandleRead {
                context: read.context,
                candles: Vec::new(),
                valid: true,
            }),
        );
    }

    pub(super) fn resume_candle_reads(&mut self, cx: &mut Cx<'_, Pending>) {
        for waiting in std::mem::take(&mut self.waiting_candles) {
            if cx.now >= waiting.read.context.deadline() {
                cx.clear_timer(waiting.timer);
                let _ = cx.fail_context(
                    &waiting.read.context,
                    VenueError::new("Candle discovery timed out")
                        .kind(safe_cex_engine::ErrorKind::Timeout),
                );
            } else if cx.validate_http_context(&waiting.read.context).is_err() {
                cx.clear_timer(waiting.timer);
            } else if self.coin(cx, &waiting.read.symbol).is_none()
                && self.candle_discovery_pending(&waiting.read.symbol)
            {
                self.waiting_candles.push(waiting);
            } else {
                cx.clear_timer(waiting.timer);
                self.dispatch_candle_read(cx, waiting.read);
            }
        }
    }

    pub(super) fn candle_discovery_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        for waiting in &mut self.waiting_candles {
            if waiting.timer == timer && cx.now < waiting.read.context.deadline() {
                waiting.timer = cx.set_timer(waiting.read.context.deadline() - cx.now);
            }
        }
    }
}
