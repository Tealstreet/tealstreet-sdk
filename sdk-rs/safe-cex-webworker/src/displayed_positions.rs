use safe_cex_types::store_model::TwapStatus;
use safe_cex_types::{
    ActiveChaser, ActiveTWAP, DisplayedPosition, ExchangeName, ExchangeOptions, Market, ModelField,
    Order, OrderSide, OrderStatus, OrderType, Position, PositionSide, StoreData, Ticker,
};
use std::collections::{HashMap, HashSet, VecDeque};

fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}
fn position_key(symbol: &str, side: ModelField<PositionSide>) -> String {
    let side = match side {
        ModelField::Value(PositionSide::Long) => "long",
        ModelField::Value(PositionSide::Short) => "short",
        ModelField::Absent | ModelField::Undefined => "undefined",
    };
    format!("{symbol}:{side}")
}
#[derive(Default)]
struct StopTpCoverage {
    has_full_stop: bool,
    has_full_tp: bool,
    stop_amount: f64,
    tp_amount: f64,
}
#[derive(Default)]
struct PnlQueue {
    values: VecDeque<(f64, f64)>,
    last_enqueue: f64,
}
impl PnlQueue {
    // Mirrors TimeLimitedQueue: rate-limit before eviction, and evict at >= age.
    fn enqueue(&mut self, now: f64, upnl: f64, seconds: usize) -> f64 {
        if (now - self.last_enqueue).partial_cmp(&1000.0) != Some(std::cmp::Ordering::Less) {
            while self
                .values
                .front()
                .is_some_and(|(at, _)| now - at >= seconds as f64 * 1000.0)
            {
                self.values.pop_front();
            }
            if self.values.len() == seconds {
                self.values.pop_front();
            }
            self.values.push_back((now, upnl));
            self.last_enqueue = now;
        }
        self.values
            .front()
            .map(|(_, pnl)| *pnl)
            .filter(|pnl| truthy(*pnl))
            .unwrap_or(0.0)
    }
}

pub struct DisplayedPositionsPostprocessor {
    account_name: String,
    exchange_name: ExchangeName,
    last_minute_pnl: HashMap<String, PnlQueue>,
    last_five_minutes_pnl: HashMap<String, PnlQueue>,
}
impl DisplayedPositionsPostprocessor {
    pub fn new(account_name: String, exchange_name: ExchangeName) -> Self {
        Self {
            account_name,
            exchange_name,
            last_minute_pnl: HashMap::new(),
            last_five_minutes_pnl: HashMap::new(),
        }
    }
    fn calculate_unrealized_pnl(
        &self,
        position: &Position,
        ticker: Option<&Ticker>,
        market: Option<&Market>,
        always_calculate: bool,
    ) -> f64 {
        if (truthy(position.unrealized_pnl) || ticker.is_none_or(|t| t.last == 0.0))
            && !always_calculate
        {
            return position.unrealized_pnl;
        }
        let Some(ticker) = ticker.filter(|t| {
            t.last != 0.0 && (position.entry_price.is_nan() || position.entry_price > 0.0)
        }) else {
            return position.unrealized_pnl;
        };
        let price_move = if position.side == PositionSide::Long {
            ticker.last - position.entry_price
        } else {
            position.entry_price - ticker.last
        };
        let native = position
            .extra
            .as_ref()
            .and_then(|extra| extra.contracts_native);
        if let (Some(market), Some(native)) = (
            market.filter(|m| m.is_inverse == Some(true)),
            native.filter(|v| truthy(*v)),
        ) {
            let contract_size = market.contract_size.filter(|v| truthy(*v)).unwrap_or(1.0);
            return price_move * (native * contract_size).abs() / position.entry_price;
        }
        price_move * position.contracts.abs()
    }
    fn get_chaser_position_side(chaser: &ActiveChaser) -> PositionSide {
        if (chaser.side == OrderSide::Buy) != chaser.reduce_only.unwrap_or(false) {
            PositionSide::Long
        } else {
            PositionSide::Short
        }
    }
    fn get_twap_position_side(twap: &ActiveTWAP) -> PositionSide {
        if twap.side == OrderSide::Buy {
            PositionSide::Long
        } else {
            PositionSide::Short
        }
    }
    fn build_active_chaser_keys(chasers: &[ActiveChaser]) -> HashSet<String> {
        chasers
            .iter()
            .filter(|c| !c.is_stopped)
            .map(|c| position_key(&c.symbol, Self::get_chaser_position_side(c).into()))
            .collect()
    }
    fn build_active_twap_keys(twaps: &[ActiveTWAP]) -> HashSet<String> {
        twaps
            .iter()
            .filter(|t| t.status == TwapStatus::Running)
            .map(|t| {
                position_key(
                    &t.active_market.symbol,
                    Self::get_twap_position_side(t).into(),
                )
            })
            .collect()
    }
    fn build_stop_tp_coverage(orders: &[Order]) -> HashMap<String, StopTpCoverage> {
        let mut coverage: HashMap<String, StopTpCoverage> = HashMap::new();
        for order in orders {
            if order.status != OrderStatus::Open {
                continue;
            }
            let side = if order.side == OrderSide::Sell {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            let is_stop = matches!(
                order.r#type,
                ModelField::Value(OrderType::StopLoss | OrderType::TrailingStopLoss)
            );
            let is_tp = matches!(
                order.r#type,
                ModelField::Value(OrderType::TakeProfit | OrderType::Limit)
            );
            if !is_stop && !is_tp {
                continue;
            }
            let entry = coverage
                .entry(position_key(&order.symbol, side.into()))
                .or_default();
            let amount = if truthy(order.amount) {
                order.amount
            } else {
                0.0
            };
            if is_stop {
                if order.amount == 0.0 {
                    entry.has_full_stop = true;
                } else {
                    entry.stop_amount += amount;
                }
            }
            if is_tp {
                if order.amount == 0.0 {
                    entry.has_full_tp = true;
                } else {
                    entry.tp_amount += amount;
                }
            }
        }
        coverage
    }
    fn track_position_pnl(
        &mut self,
        position: &mut DisplayedPosition,
        clock: &mut dyn FnMut() -> f64,
    ) -> Option<String> {
        if !position.ticker.as_ref().is_some_and(|t| truthy(t.last)) {
            return None;
        }
        let id = format!("{}:{}", position.account, position.position.id);
        position.five_min_upnl = Some(
            self.last_five_minutes_pnl
                .entry(id.clone())
                .or_default()
                .enqueue(clock(), position.position.unrealized_pnl, 300),
        );
        position.one_min_upnl = Some(self.last_minute_pnl.entry(id.clone()).or_default().enqueue(
            clock(),
            position.position.unrealized_pnl,
            60,
        ));
        Some(id)
    }
    pub fn process(
        &mut self,
        store: &mut StoreData,
        clock: &mut dyn FnMut() -> f64,
        options: &ExchangeOptions,
        metadata_always_calculate: bool,
    ) {
        let always_calculate = metadata_always_calculate
            || options
                .tuning
                .as_ref()
                .and_then(|t| t.calculate_upnl_from_realtime_last_price)
                .unwrap_or(false);
        // Owned snapshots have no JS array identity; rebuild borrowed maps to see in-place edits.
        let tickers: HashMap<_, _> = store
            .tickers
            .iter()
            .map(|t| (t.symbol.as_str(), t))
            .collect();
        let markets: HashMap<_, _> = store
            .markets
            .iter()
            .map(|m| (m.symbol.as_str(), m))
            .collect();
        let chasers = Self::build_active_chaser_keys(&store.active_chasers);
        let twaps = Self::build_active_twap_keys(&store.active_twaps);
        let coverage = Self::build_stop_tp_coverage(&store.orders);
        let mut real_keys = HashSet::new();
        let mut active_pnl = HashSet::new();
        let prune = !self.last_minute_pnl.is_empty() || !self.last_five_minutes_pnl.is_empty();
        let mut displayed = Vec::new();
        for position in &store.positions {
            let contracts = position.contracts.abs();
            if contracts <= 0.0 {
                continue;
            }
            let active_key = position_key(&position.symbol, position.side);
            real_keys.insert(active_key.clone());
            let ticker = tickers.get(position.symbol.as_str()).copied();
            let market = markets.get(position.symbol.as_str()).copied();
            let coverage = coverage.get(&active_key);
            let mut value = DisplayedPosition {
                position: position.clone(),
                key: Some(format!("{}:{active_key}", self.account_name)),
                account: self.account_name.clone(),
                ticker: ticker.cloned(),
                market: market.cloned(),
                balance: Some(store.balance.clone()),
                exchange_name: Some(
                    serde_json::to_value(&self.exchange_name)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ),
                one_min_upnl: None,
                five_min_upnl: None,
                has_active_chaser: Some(chasers.contains(&active_key)),
                has_active_twap: Some(twaps.contains(&active_key)),
                has_full_stop: Some(
                    coverage.is_some_and(|c| c.has_full_stop || c.stop_amount >= contracts),
                ),
                has_full_tp: Some(
                    coverage.is_some_and(|c| c.has_full_tp || c.tp_amount >= contracts),
                ),
                is_fake_position: None,
                chaser_size: None,
            };
            value.position.contracts = contracts;
            value.position.unrealized_pnl =
                self.calculate_unrealized_pnl(position, ticker, market, always_calculate);
            if let Some(key) = self.track_position_pnl(&mut value, clock) {
                active_pnl.insert(key);
            }
            displayed.push(value);
        }
        self.append_fake_positions_from_chasers(
            store,
            &tickers,
            &markets,
            &real_keys,
            &mut displayed,
            &mut active_pnl,
            clock,
        );
        if prune {
            self.last_minute_pnl
                .retain(|key, _| active_pnl.contains(key));
            self.last_five_minutes_pnl
                .retain(|key, _| active_pnl.contains(key));
        }
        store.displayed_positions = displayed;
    }
    #[allow(clippy::too_many_arguments)]
    fn append_fake_positions_from_chasers(
        &mut self,
        store: &StoreData,
        tickers: &HashMap<&str, &Ticker>,
        markets: &HashMap<&str, &Market>,
        real_keys: &HashSet<String>,
        displayed: &mut Vec<DisplayedPosition>,
        active_pnl: &mut HashSet<String>,
        clock: &mut dyn FnMut() -> f64,
    ) {
        for chaser in &store.active_chasers {
            if chaser.is_stopped || chaser.reduce_only == Some(true) {
                continue;
            }
            let side = Self::get_chaser_position_side(chaser);
            if real_keys.contains(&position_key(&chaser.symbol, side.into())) {
                continue;
            }
            // Own entries shadow Object.prototype. The empty-table fallback delegates
            // inherited-name inventory to the shared read operation, not a local list.
            let ticker = tickers
                .get(chaser.symbol.as_str())
                .map(safe_cex_num::TableHit::Mapped)
                .unwrap_or_else(|| {
                    safe_cex_num::js_table_lookup::<&str, &Ticker>(&[], &chaser.symbol)
                });
            let market = markets
                .get(chaser.symbol.as_str())
                .map(safe_cex_num::TableHit::Mapped)
                .unwrap_or_else(|| {
                    safe_cex_num::js_table_lookup::<&str, &Market>(&[], &chaser.symbol)
                });
            if !ticker.is_truthy_by(|_| true) || !market.is_truthy_by(|_| true) {
                continue;
            }
            // ts-bug: inherited members pass the source guard. CHOSEN closed typed
            // output refuses callable ticker/market values, omitting the whole row;
            // this does NOT converge after serialization. __proto__ object values
            // and custom prototype writes remain separate FINDINGs.
            let (safe_cex_num::TableHit::Mapped(ticker), safe_cex_num::TableHit::Mapped(market)) =
                (ticker, market)
            else {
                continue;
            };
            let mut value = DisplayedPosition {
                position: Position {
                    id: format!("fake-chaser-{}", chaser.id),
                    symbol: chaser.symbol.clone(),
                    side: side.into(),
                    entry_price: if truthy(ticker.last) {
                        ticker.last
                    } else {
                        0.0
                    },
                    notional: 0.0,
                    // ts-bug: fakePosition reads absent market.maxLeverage, not limits.leverage.max.
                    leverage: 1.0,
                    unrealized_pnl: 0.0,
                    contracts: 0.0,
                    liquidation_price: 0.0,
                    realized_pnl: ModelField::Value(0.0),
                    margin_type: ModelField::Absent,
                    position_mode: ModelField::Absent,
                    margin_size: ModelField::Absent,
                    info: ModelField::Absent,
                    extra: ModelField::Absent,
                    last_update: ModelField::Absent,
                    option: ModelField::Absent,
                },
                key: None,
                account: self.account_name.clone(),
                ticker: Some((*ticker).clone()),
                market: Some((*market).clone()),
                balance: Some(store.balance.clone()),
                exchange_name: Some(
                    serde_json::to_value(&self.exchange_name)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ),
                one_min_upnl: Some(0.0),
                five_min_upnl: Some(0.0),
                has_active_chaser: Some(true),
                has_active_twap: Some(false),
                is_fake_position: Some(true),
                chaser_size: Some(chaser.size),
                has_full_stop: Some(false),
                has_full_tp: Some(false),
            };
            if let Some(key) = self.track_position_pnl(&mut value, clock) {
                active_pnl.insert(key);
            }
            displayed.push(value);
        }
    }
}
