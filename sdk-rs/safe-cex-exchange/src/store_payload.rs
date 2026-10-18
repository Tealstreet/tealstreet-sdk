//! Snapshot bookkeeping used by ExchangeWorker's store payload preparation.
use safe_cex_store::PackedTickerDelta;
use safe_cex_types::Ticker;

pub fn should_send_update_id_backed_snapshot(
    update_id: Option<f64>,
    last_sent_update_id: Option<f64>,
    force_full_store: Option<bool>,
) -> bool {
    force_full_store == Some(true) || update_id.is_none() || last_sent_update_id != update_id
}

pub fn record_sent_update_id_backed_snapshot(update_id: Option<f64>) -> Option<f64> {
    update_id
}

struct Snapshot {
    id: String,
    symbol: String,
    values: [f64; 11],
}
fn values(ticker: &Ticker) -> [f64; 11] {
    [
        ticker.bid,
        ticker.ask,
        ticker.last,
        ticker.mark,
        ticker.index,
        ticker.percentage,
        ticker.open_interest,
        ticker.funding_rate,
        ticker.funding_time,
        ticker.volume,
        ticker.quote_volume,
    ]
}

/// The worker's private last-sent ticker state, independent of the live store.
#[derive(Default)]
pub struct TickerBaseline {
    tickers: Option<Vec<Snapshot>>,
}
impl TickerBaseline {
    pub fn copy_from(&mut self, tickers: &[Ticker]) {
        let baseline = self.tickers.get_or_insert_with(Vec::new);
        if baseline.len() != tickers.len() {
            *baseline = tickers
                .iter()
                .map(|ticker| Snapshot {
                    id: ticker.id.clone(),
                    symbol: ticker.symbol.clone(),
                    values: values(ticker),
                })
                .collect();
            return;
        }
        for (snapshot, ticker) in baseline.iter_mut().zip(tickers) {
            snapshot.id.clone_from(&ticker.id);
            snapshot.symbol.clone_from(&ticker.symbol);
            snapshot.values = values(ticker);
        }
    }

    pub fn pack_deltas(&mut self, tickers: &[Ticker]) -> Option<Vec<PackedTickerDelta>> {
        let baseline = self.tickers.as_mut()?;
        if baseline.len() != tickers.len() {
            return None;
        }
        let mut packed = Vec::new();
        for (index, (previous, ticker)) in baseline.iter_mut().zip(tickers).enumerate() {
            if ticker.id != previous.id || ticker.symbol != previous.symbol {
                // ts-bug: earlier baseline rows remain mutated on this fallback.
                return None;
            }
            let mut mask = 0_u32;
            let mut changed = Vec::new();
            for (field, value) in values(ticker).into_iter().enumerate() {
                // JS !==, including NaN !== NaN and +0 === -0.
                if value != previous.values[field] {
                    previous.values[field] = value;
                    mask |= 1 << field;
                    changed.push(value);
                }
            }
            if mask != 0 {
                packed.push(PackedTickerDelta {
                    index: index as f64,
                    mask: mask as f64,
                    values: changed,
                });
            }
        }
        Some(packed)
    }
}

pub fn sanitize_for_post_message(
    data: &safe_cex_types::StoreData,
    omit_orders: bool,
    omit_positions: bool,
) -> safe_cex_store::StoreUpdate {
    // Explicit destructuring makes additions to the shared store a compile error
    // until the worker's full-snapshot projection carries them too.
    let safe_cex_types::StoreData {
        latency,
        balance,
        markets,
        tickers,
        orders,
        stark_config,
        zk_link_config,
        user_data,
        positions,
        displayed_positions,
        displayed_margin,
        orders_risk,
        active_chasers,
        active_twaps,
        loaded,
        options,
        update_ids,
        message_handler_stats,
        request_stats,
        fetched_leverage,
        socket_health,
    } = data;
    let orders = (!omit_orders).then(|| {
        let mut orders = orders.clone();
        for order in &mut orders {
            if safe_cex_num::js_truthy(order.info.as_ref()) {
                order.info = safe_cex_types::ModelField::Absent;
            }
        }
        orders
    });
    let positions = (!omit_positions).then(|| {
        let mut positions = positions.clone();
        for position in &mut positions {
            if safe_cex_num::js_truthy(position.info.as_ref()) {
                position.info = safe_cex_types::ModelField::Absent;
            }
        }
        positions
    });
    let mut displayed_positions = displayed_positions.clone();
    for position in &mut displayed_positions {
        if safe_cex_num::js_truthy(position.position.info.as_ref()) {
            position.position.info = safe_cex_types::ModelField::Absent;
        }
    }
    safe_cex_store::StoreUpdate {
        latency: Some(*latency),
        balance: Some(balance.clone()),
        markets: Some(markets.clone()),
        tickers: Some(tickers.clone()),
        orders,
        stark_config: stark_config.clone(),
        zk_link_config: zk_link_config.clone(),
        user_data: user_data.clone(),
        positions,
        displayed_positions: Some(displayed_positions),
        displayed_margin: displayed_margin.clone(),
        orders_risk: Some(orders_risk.clone()),
        active_chasers: Some(active_chasers.clone()),
        active_twaps: Some(active_twaps.clone()),
        loaded: Some(loaded.clone()),
        options: Some(options.clone()),
        update_ids: Some(update_ids.clone().into()),
        message_handler_stats: message_handler_stats.clone(),
        request_stats: request_stats.clone(),
        fetched_leverage: Some(fetched_leverage.clone()),
        socket_health: socket_health.clone(),
        ticker_deltas: None,
    }
}

#[derive(Default)]
pub struct StorePayloadPreparer {
    last_sent_markets_version: Option<f64>,
    last_sent_tickers_version: Option<f64>,
    last_sent_orders_update_id: Option<f64>,
    last_sent_positions_update_id: Option<f64>,
    last_payload_had_active_chasers: bool,
    baseline: TickerBaseline,
}
impl StorePayloadPreparer {
    pub fn prepare(
        &mut self,
        data: &safe_cex_types::StoreData,
        markets_version: f64,
        tickers_version: f64,
        hosted_mode: bool,
        force_full_store: Option<bool>,
    ) -> safe_cex_store::StoreUpdate {
        let has_active_chasers = !data.active_chasers.is_empty();
        let can_omit_orders = !has_active_chasers && !self.last_payload_had_active_chasers;
        let should_send_orders = !can_omit_orders
            || should_send_update_id_backed_snapshot(
                Some(data.update_ids.orders),
                self.last_sent_orders_update_id,
                force_full_store,
            );
        let should_send_positions = should_send_update_id_backed_snapshot(
            Some(data.update_ids.positions),
            self.last_sent_positions_update_id,
            force_full_store,
        );
        let mut payload =
            sanitize_for_post_message(data, !should_send_orders, !should_send_positions);
        let pack_ticker_deltas = !hosted_mode;
        let should_send_full_tickers = !pack_ticker_deltas
            || force_full_store == Some(true)
            || self.last_sent_tickers_version != Some(tickers_version)
            || self.baseline.tickers.is_none();
        if should_send_orders {
            self.last_sent_orders_update_id =
                record_sent_update_id_backed_snapshot(Some(data.update_ids.orders));
        }
        if should_send_positions {
            self.last_sent_positions_update_id =
                record_sent_update_id_backed_snapshot(Some(data.update_ids.positions));
        }
        self.last_payload_had_active_chasers = has_active_chasers;
        if should_send_full_tickers {
            self.last_sent_tickers_version = Some(tickers_version);
            if pack_ticker_deltas {
                self.baseline.copy_from(&data.tickers);
            }
        } else {
            match self.baseline.pack_deltas(&data.tickers) {
                None => {
                    self.last_sent_tickers_version = Some(tickers_version);
                    self.baseline.copy_from(&data.tickers);
                }
                Some(deltas) => {
                    payload.tickers = None;
                    if !deltas.is_empty() {
                        payload.ticker_deltas = Some(deltas);
                    }
                }
            }
        }
        if force_full_store != Some(true) && self.last_sent_markets_version == Some(markets_version)
        {
            payload.markets = None;
            return payload;
        }
        self.last_sent_markets_version = Some(markets_version);
        payload
    }
}
