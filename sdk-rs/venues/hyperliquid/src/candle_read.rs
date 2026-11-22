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
    pub(super) fn record_spot_candle_catalog(
        &mut self,
        meta: &SpotMeta,
        markets: &[Market],
        excluded: &[String],
    ) {
        self.candle_spot_catalog_complete = meta.universe.iter().all(|row| {
            row.0.as_ref().is_some_and(|native| {
                markets.iter().any(|market| market.id == native.name)
                    || excluded.contains(&native.name)
            })
        });
        self.candle_spot_excluded = meta
            .universe
            .iter()
            .filter_map(|row| row.0.as_ref())
            .filter(|native| excluded.contains(&native.name))
            .filter_map(|native| {
                let token = |index| {
                    meta.tokens
                        .iter()
                        .filter_map(|row| row.0.as_ref())
                        .find(|token| token.index == index)
                };
                let base = token(*native.tokens.first()?)?;
                let quote = token(*native.tokens.get(1)?)?;
                Some(format!("{}/{}", base.name, quote.name))
            })
            .collect();
    }

    fn candle_discovery_pending(&self, symbol: &str) -> bool {
        if self.spot {
            return self.core_discovery_incomplete && !self.candle_spot_excluded.contains(symbol);
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
        symbol.contains('/')
            && self.spot_markets
            && !self.candle_spot_catalog_complete
            && !self.candle_spot_excluded.contains(symbol)
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
        let Some((body, weight)) = candle_body(
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
            weight,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// AGT-168: a catalogue that nothing is reading or retrying never resolves,
    /// so a read for one of its missing rows must not wait out its bound.
    #[test]
    fn a_catalogue_with_no_discovery_running_holds_no_read() {
        let mut venue = Hyperliquid::new(Options {
            public_data_only: true,
            ..Default::default()
        });
        venue.core_discovery_incomplete = false;
        venue.perps_published = true;
        assert!(!venue.candle_discovery_pending("XYZ|GOLD-PERP"));
    }
}
