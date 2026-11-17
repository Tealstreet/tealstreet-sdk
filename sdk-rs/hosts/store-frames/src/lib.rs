//! Host-owned native UI projections, shared by hosted accounts and LOCAL wasm.
//! The engine keeps its small venue records; generated public rows are built at
//! this boundary, without a legacy Store or a JSON stringify/parse round trip.
use safe_cex_engine::model::*;
use safe_cex_engine::store_frame_wire::{self as wire, Section};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};
mod codec;
mod model;
use model::*;
pub mod risk;
pub use codec::decode as decode_section;

#[derive(Clone)]
pub struct Inputs {
    pub account: String,
    pub venue: String,
    pub calculate_upnl_from_last: bool,
    pub active_chasers: Vec<ActiveChaser>,
    active_twaps: Value,
    pub balance_info: Value,
    pub socket_health: Value,
}
impl Default for Inputs {
    fn default() -> Self {
        Self {
            account: String::new(),
            venue: String::new(),
            calculate_upnl_from_last: false,
            active_chasers: Vec::new(),
            active_twaps: Value::Array(Vec::new()),
            balance_info: Value::Null,
            socket_health: serde_json::json!({"publicHealthy":false,"privateHealthy":false,"publicInitialized":false,"privateInitialized":false}),
        }
    }
}
impl Inputs {
    pub fn configure(&mut self, account: &str, venue: &str) {
        self.account = account.into();
        self.venue = venue.into();
    }
    pub fn algos<C: Serialize + ?Sized, T: Serialize + ?Sized>(&mut self, chasers: &C, twaps: &T) {
        self.active_chasers =
            serde_json::from_value(serde_json::to_value(chasers).expect("algo rows"))
                .expect("native chasers");
        self.active_twaps = serde_json::to_value(twaps).expect("native TWAP options");
    }
    pub fn balance_info(&mut self, info: &serde_json::Value) {
        self.balance_info = info.clone();
    }
    pub fn socket_health(&mut self, health: &serde_json::Value) {
        let mut health = health.clone();
        if let Some(fields) = health.as_object_mut() {
            for name in [
                "publicLastMessage",
                "privateLastMessage",
                "publicCloseCode",
                "privateCloseCode",
            ] {
                if fields.get(name).is_some_and(serde_json::Value::is_null) {
                    fields.remove(name);
                }
            }
        }
        self.socket_health = health;
    }
}

/// Last emitted bytes per section ensure unrelated updates preserve identities
/// in the consumer. A new client's snapshot does not consume this baseline.
#[derive(Default)]
pub struct Emitter {
    sent: BTreeMap<Section, Vec<u8>>,
    pnl: HashMap<String, VecDeque<(f64, f64)>>,
    data: Option<Projection>,
    current: BTreeMap<Section, Vec<u8>>,
    last_inputs: Option<Inputs>,
    last_pnl_sample: f64,
    socket_health: Option<Value>,
    socket_health_at: f64,
}
impl Emitter {
    pub fn frame(
        &mut self,
        mut frame: Vec<u8>,
        store: &safe_cex_engine::Store,
        input: &Inputs,
        snapshot: bool,
    ) -> Vec<u8> {
        let now = f64::from_le_bytes(frame[8..16].try_into().expect("SCXF time"));
        let first = self.data.is_none();
        let changed: Vec<_> = safe_cex_engine::wire::sections(&frame)
            .map(|(kind, _)| kind)
            .collect();
        let has = |kind| first || snapshot || changed.contains(&kind);
        let inputs_changed = self.last_inputs.as_ref().is_none_or(|old| {
            old.account != input.account
                || old.venue != input.venue
                || old.calculate_upnl_from_last != input.calculate_upnl_from_last
                || old.active_chasers != input.active_chasers
                || old.active_twaps != input.active_twaps
                || old.balance_info != input.balance_info
        });
        let refresh = inputs_changed || [2, 3, 4, 7, 8, 9].iter().any(|&kind| has(kind));
        let sample_due = !self.pnl.is_empty() && now - self.last_pnl_sample >= 1_000.0;
        if refresh || sample_due {
            let mut data = self.data.take().unwrap_or_default();
            if has(9) || inputs_changed {
                data.balance.balance = store.balance().clone();
                data.balance.info = match &input.balance_info {
                    Value::Null => None,
                    info => Some(info.clone()),
                };
                data.balance.coins = store.coin_balances().map(|rows| {
                    rows.iter()
                        .map(|row| (row.base.clone(), row.clone()))
                        .collect()
                });
                data.balance.currencies = store.currency_balances().map(|rows| {
                    rows.iter()
                        .map(|row| (row.currency.clone(), row.balance.clone()))
                        .collect()
                });
            }
            if has(2) {
                data.markets = store.markets().to_vec();
            }
            if has(3) || has(4) {
                data.tickers = store.tickers().to_vec();
            }
            if has(8) || has(2) {
                data.positions = store.positions().to_vec();
            }
            if has(7) {
                data.orders = store.orders().to_vec();
            }
            data.active_chasers.clone_from(&input.active_chasers);
            data.active_twaps = twap_rows(&input.active_twaps, &data.markets, &data.active_twaps);
            self.display(&mut data, store, input, now);
            if refresh {
                data.orders_risk = risk::calculate(&data);
            }
            self.current
                .insert(Section::OrdersRisk, encoded(&data.orders_risk));
            self.current.insert(
                Section::DisplayedPositions,
                encoded(&data.displayed_positions),
            );
            self.current
                .insert(Section::DisplayedMargin, encoded(&data.displayed_margin));
            self.last_inputs = Some(input.clone());
            self.last_pnl_sample = now;
            self.data = Some(data);
        }
        let health_due = self.socket_health.as_ref().is_none_or(|old| {
            let state = |value: &Value| {
                value.as_object().map(|fields| {
                    fields
                        .iter()
                        .filter(|(key, _)| {
                            key.as_str() != "publicLastMessage"
                                && key.as_str() != "privateLastMessage"
                        })
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<Vec<_>>()
                })
            };
            state(old) != state(&input.socket_health) || now - self.socket_health_at >= 5_000.0
        });
        if snapshot || health_due {
            self.current
                .insert(Section::SocketHealth, wire::encode(&input.socket_health));
            self.socket_health = Some(input.socket_health.clone());
            self.socket_health_at = now;
        }
        self.current
            .insert(Section::ActiveChasers, encoded(&input.active_chasers));
        self.current.insert(
            Section::ActiveTwaps,
            encoded(&self.data.as_ref().expect("native data").active_twaps),
        );
        self.current
            .insert(Section::BalanceInfo, wire::encode(&input.balance_info));
        for (&kind, payload) in &self.current {
            if snapshot || self.sent.get(&kind) != Some(payload) {
                wire::append(&mut frame, kind, payload);
            }
            if !snapshot && self.sent.get(&kind) != Some(payload) {
                self.sent.insert(kind, payload.clone());
            }
        }
        frame
    }

    fn display(
        &mut self,
        data: &mut Projection,
        store: &safe_cex_engine::Store,
        input: &Inputs,
        now: f64,
    ) {
        let markets: HashMap<_, _> = data
            .markets
            .iter()
            .map(|m| (m.symbol.as_str(), m))
            .collect();
        let tickers: HashMap<_, _> = data
            .tickers
            .iter()
            .map(|t| (t.symbol.as_str(), t))
            .collect();
        let chaser_side = |c: &ActiveChaser| match (c.side, c.reduce_only) {
            (OrderSide::Buy, false) | (OrderSide::Sell, true) => PositionSide::Long,
            _ => PositionSide::Short,
        };
        let mut rows = Vec::new();
        for position in data.positions.iter().filter(|p| p.contracts != 0.0) {
            let mut p = position.clone();
            p.contracts = p.contracts.abs();
            let side = p.side;
            let ticker = tickers.get(p.symbol.as_str()).copied();
            let market = markets.get(p.symbol.as_str()).copied();
            if let Some(t) = ticker
                && t.last != 0.0
                && (p.entry_price > 0.0 || p.entry_price.is_nan())
                && (input.calculate_upnl_from_last
                    || p.unrealized_pnl == 0.0
                    || p.unrealized_pnl.is_nan())
            {
                let move_by = if side == PositionSide::Long {
                    t.last - p.entry_price
                } else {
                    p.entry_price - t.last
                };
                let native_contracts = store
                    .positions()
                    .iter()
                    .position(|row| row.id == p.id)
                    .and_then(|i| store.position_output_extras(i))
                    .and_then(|v| v[0]);
                p.unrealized_pnl = if let (Some(m), Some(size)) = (market, native_contracts)
                    && m.is_inverse == Some(true)
                    && size != 0.0
                {
                    move_by * (size * m.contract_size.filter(|s| *s != 0.0).unwrap_or(1.0)).abs()
                        / p.entry_price
                } else {
                    move_by * p.contracts
                };
            }
            let mut stop = 0.0;
            let mut tp = 0.0;
            let mut full_stop = false;
            let mut full_tp = false;
            for o in data.orders.iter().filter(|o| {
                o.status == OrderStatus::Open
                    && o.symbol == p.symbol
                    && o.side
                        == if side == PositionSide::Long {
                            OrderSide::Sell
                        } else {
                            OrderSide::Buy
                        }
            }) {
                if matches!(o.kind, OrderType::StopLoss | OrderType::TrailingStopLoss) {
                    full_stop |= o.amount == 0.0;
                    stop += o.amount;
                }
                if matches!(o.kind, OrderType::TakeProfit | OrderType::Limit) {
                    full_tp |= o.amount == 0.0;
                    tp += o.amount;
                }
            }
            rows.push(DisplayedPosition {
                key: Some(format!(
                    "{}:{}:{}",
                    input.account,
                    p.symbol,
                    side_name(side)
                )),
                account: input.account.clone(),
                ticker: ticker.cloned(),
                market: market.cloned(),
                balance: data.balance.clone(),
                exchange_name: input.venue.clone(),
                one_min_upnl: None,
                five_min_upnl: None,
                has_active_chaser: input
                    .active_chasers
                    .iter()
                    .any(|c| !c.is_stopped && c.symbol == p.symbol && chaser_side(c) == side),
                has_active_twap: data.active_twaps.iter().any(|t| {
                    t.status == "running"
                        && t.active_market.symbol == p.symbol
                        && (if t.side == OrderSide::Buy {
                            PositionSide::Long
                        } else {
                            PositionSide::Short
                        }) == side
                }),
                has_full_stop: full_stop || stop >= p.contracts,
                has_full_tp: full_tp || tp >= p.contracts,
                is_fake_position: None,
                chaser_size: None,
                position: PositionView {
                    extra: store
                        .positions()
                        .iter()
                        .position(|row| row.id == p.id)
                        .and_then(|i| store.position_output_extras(i))
                        .filter(|v| v.iter().any(Option::is_some))
                        .map(|v| PositionExtra {
                            contracts_native: v[0],
                            unrealized_pnl_native: v[1],
                        }),
                    native: p,
                },
            });
        }
        for c in input
            .active_chasers
            .iter()
            .filter(|c| !c.is_stopped && !c.reduce_only)
        {
            let side = chaser_side(c);
            if data
                .positions
                .iter()
                .any(|p| p.contracts != 0.0 && p.symbol == c.symbol && p.side == side)
            {
                continue;
            }
            if let (Some(ticker), Some(market)) = (
                tickers.get(c.symbol.as_str()),
                markets.get(c.symbol.as_str()),
            ) {
                let p = Position {
                    id: format!("fake-chaser-{}", c.id),
                    symbol: c.symbol.clone(),
                    side,
                    entry_price: if ticker.last.is_finite() {
                        ticker.last
                    } else {
                        0.0
                    },
                    notional: 0.0,
                    leverage: 1.0,
                    unrealized_pnl: 0.0,
                    contracts: 0.0,
                    liquidation_price: 0.0,
                    realized_pnl: Some(0.0),
                    margin_type: None,
                    position_mode: None,
                    margin_size: None,
                    last_update: None,
                };
                rows.push(DisplayedPosition {
                    key: None,
                    position: PositionView {
                        extra: store
                            .positions()
                            .iter()
                            .position(|row| row.id == p.id)
                            .and_then(|i| store.position_output_extras(i))
                            .filter(|v| v.iter().any(Option::is_some))
                            .map(|v| PositionExtra {
                                contracts_native: v[0],
                                unrealized_pnl_native: v[1],
                            }),
                        native: p,
                    },
                    account: input.account.clone(),
                    ticker: Some((*ticker).clone()),
                    market: Some((*market).clone()),
                    balance: data.balance.clone(),
                    exchange_name: input.venue.clone(),
                    one_min_upnl: Some(0.0),
                    five_min_upnl: Some(0.0),
                    has_active_chaser: true,
                    has_active_twap: false,
                    is_fake_position: Some(true),
                    chaser_size: Some(c.size),
                    has_full_stop: false,
                    has_full_tp: false,
                });
            }
        }
        let mut active_keys = std::collections::HashSet::new();
        for row in &mut rows {
            if row
                .ticker
                .as_ref()
                .is_none_or(|t| t.last == 0.0 || t.last.is_nan())
            {
                continue;
            }
            let key = format!("{}:{}", input.account, row.position.id);
            active_keys.insert(key.clone());
            let samples = self.pnl.entry(key).or_default();
            // Sampling follows TimeLimitedQueue: at most once each second.
            if samples.back().is_none_or(|s| now - s.0 >= 1_000.0) {
                samples.push_back((now, row.position.unrealized_pnl));
            }
            while samples.front().is_some_and(|s| now - s.0 >= 300_000.0) {
                samples.pop_front();
            }
            row.five_min_upnl = Some(samples.front().map_or(0.0, |s| s.1));
            row.one_min_upnl = Some(
                samples
                    .iter()
                    .find(|s| now - s.0 < 60_000.0)
                    .map_or(0.0, |s| s.1),
            );
        }
        self.pnl.retain(|key, _| active_keys.contains(key));
        let (mut long, mut short, mut total) = (0.0, 0.0, 0.0);
        for row in &rows {
            let n = row.position.notional.abs();
            total += n;
            if row.position.side == PositionSide::Long {
                long += n;
            } else {
                short += n;
            }
        }
        data.displayed_margin = DisplayedMargin {
            long_exposure: long.is_finite().then_some(long),
            short_exposure: short.is_finite().then_some(short),
            total_notional: total.is_finite().then_some(total),
        };
        data.displayed_positions = rows;
    }
}
fn twap_rows(input: &Value, markets: &[Market], previous: &[ActiveTwap]) -> Vec<ActiveTwap> {
    let Value::Array(rows) = input else {
        return Vec::new();
    };
    rows.iter()
        .map(|row| {
            let mut row = row.clone();
            // Algo options may contain only the selected symbol. The UI gets the
            // engine's canonical generated Market, never the partial caller object.
            if let Some(active) = row.get_mut("activeMarket") {
                let symbol = active
                    .get("symbol")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                if let Some(market) = markets
                    .iter()
                    .find(|m| Some(&m.symbol) == symbol.as_ref())
                    .or_else(|| {
                        previous
                            .iter()
                            .find(|t| Some(&t.active_market.symbol) == symbol.as_ref())
                            .map(|t| &t.active_market)
                    })
                {
                    *active = serde_json::to_value(market).expect("native active market");
                }
            }
            serde_json::from_value(row).expect("native active TWAP")
        })
        .collect()
}
fn side_name(side: PositionSide) -> &'static str {
    match side {
        PositionSide::Long => "long",
        PositionSide::Short => "short",
    }
}
fn encoded(value: &(impl Serialize + ?Sized)) -> Vec<u8> {
    wire::encode(value)
}

pub fn empty(time: f64) -> Vec<u8> {
    wire::empty(time)
}

/// A holder may label a shared resident account differently. Rewrite only the
/// two native row sections which contain account labels, retaining all other
/// sections byte-for-byte (including independent price/source clocks).
pub fn with_account_label(frame: &[u8], label: &str) -> Vec<u8> {
    let mut out = frame[..16].to_vec();
    out[6..8].copy_from_slice(&0u16.to_le_bytes());
    let mut offset = 16;
    for _ in 0..u16::from_le_bytes([frame[6], frame[7]]) {
        let kind = u16::from_le_bytes([frame[offset], frame[offset + 1]]);
        let length = u32::from_le_bytes(frame[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let end = offset + 8 + length;
        let section = match kind {
            28 => Some(Section::ActiveTwaps),
            30 => Some(Section::DisplayedPositions),
            _ => None,
        };
        if let Some(section) = section {
            let payload = codec::relabel(
                &frame[offset + 8..end],
                label,
                if kind == 28 { "accountName" } else { "account" },
            )
            .expect("native account record");
            wire::append(&mut out, section, &payload);
        } else {
            let count = u16::from_le_bytes([out[6], out[7]]) + 1;
            out[6..8].copy_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&frame[offset..end]);
        }
        offset = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> safe_cex_engine::Store {
        let mut store = safe_cex_engine::Store::default();
        store.set_markets(vec![Market {
            id: "BTC".into(),
            symbol: "BTCUSDT".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                amount: 0.001,
                price: 0.1,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.001,
                    max: 1000.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 100.0,
                },
                price: None,
            },
            contract_size: Some(1.0),
            is_expiring: None,
            is_inverse: Some(false),
            is_spot: Some(false),
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: Some(0.001),
            maker_fee: Some(0.0002),
        }]);
        store.set_tickers(vec![Ticker {
            id: "BTC".into(),
            symbol: "BTCUSDT".into(),
            last: 110.0,
            mark: 111.0,
            ..Ticker::default()
        }]);
        store.set_positions(vec![Position {
            id: "position-1".into(),
            symbol: "BTCUSDT".into(),
            side: PositionSide::Long,
            entry_price: 100.0,
            notional: 220.0,
            leverage: 2.0,
            margin_type: Some(MarginType::Cross),
            position_mode: Some(PositionMode::Oneway),
            unrealized_pnl: 0.0,
            realized_pnl: Some(5.0),
            contracts: -2.0,
            liquidation_price: 0.0,
            margin_size: None,
            last_update: None,
        }]);
        let mut order = CancelOrderRef {
            id: "profit".into(),
            symbol: "BTCUSDT".into(),
        }
        .limit_stand_in();
        order.side = OrderSide::Sell;
        order.price = 120.0;
        order.amount = 1.0;
        order.remaining = 1.0;
        let mut stop = order.clone();
        stop.id = "stop".into();
        stop.kind = OrderType::StopLoss;
        stop.price = 90.0;
        stop.amount = 0.0;
        stop.remaining = 0.0;
        store.set_orders(vec![order, stop]);
        store.set_balance(Balance {
            used: 20.0,
            free: 80.0,
            total: 100.0,
            upnl: 0.0,
        });
        store
    }
    fn input() -> Inputs {
        let mut input = Inputs::default();
        input.configure("account-1", "bybit");
        input
    }
    fn payload(frame: &[u8], kind: Section) -> Value {
        let (_, bytes) = safe_cex_engine::wire::sections(frame)
            .find(|(k, _)| *k == kind as u16)
            .expect("section");
        decode_section(bytes).unwrap()
    }
    #[test]
    fn native_host_retains_the_engine_json_map_profile() {
        // Source-parity features must not unify through the host dependency:
        // account fingerprints and venue encoders depend on this sorted map.
        assert_eq!(
            serde_json::to_string(&serde_json::json!({"z":1,"a":2})).unwrap(),
            r#"{"a":2,"z":1}"#
        );
    }

    #[test]
    fn real_rows_include_native_valuation_risk_coverage_and_balance_info() {
        let mut emitter = Emitter::default();
        let store = store();
        let mut input = input();
        input.balance_info(&serde_json::json!({"tier":"pro"}));
        let frame = emitter.frame(empty(1000.0), &store, &input, true);
        assert_eq!(safe_cex_engine::wire::sections(&frame).count(), 7);
        let data = emitter.data.as_ref().unwrap();
        let row = &data.displayed_positions[0];
        assert_eq!(row.key.as_deref(), Some("account-1:BTCUSDT:long"));
        assert_eq!(row.position.contracts, 2.0);
        assert_eq!(row.position.unrealized_pnl, 20.0);
        assert_eq!(row.position.liquidation_price, 0.0);
        assert!(row.has_full_stop);
        assert!(!row.has_full_tp);
        assert_eq!(data.displayed_margin.total_notional, Some(220.0));
        let profit = &data.orders_risk["profit"][0];
        assert_eq!(profit.order_pnl, 20.0);
        assert_eq!(profit.position_size, 1.0);
        assert_eq!(profit.fee, 0.024);
        let stop = &data.orders_risk["stop"][0];
        assert_eq!(stop.order_pnl, -20.0);
        assert_eq!(stop.is_looking_like_a_full_stop, Some(true));
        assert_eq!(stop.position_size, 0.0);
        assert_eq!(
            payload(&frame, Section::BalanceInfo)
                .get("tier")
                .unwrap()
                .as_str(),
            Some("pro")
        );
    }
    #[test]
    fn new_client_snapshot_never_consumes_pending_sections_and_empty_rows_clear() {
        let mut emitter = Emitter::default();
        let mut store = store();
        let input = input();
        let first = emitter.frame(empty(1000.0), &store, &input, false);
        assert_eq!(safe_cex_engine::wire::sections(&first).count(), 7);
        assert_eq!(
            emitter.frame(empty(1000.0), &store, &input, false).len(),
            16
        );
        store.set_positions(vec![]);
        store.set_orders(vec![]);
        let snapshot = emitter.frame(empty(2000.0), &store, &input, true);
        assert_eq!(
            payload(&snapshot, Section::DisplayedPositions),
            Value::Array(vec![])
        );
        // The actual stream's POSITIONS/ORDERS flags arrive after that snapshot.
        let mut changed = safe_cex_engine::wire::FrameWriter::new(vec![], 2000.0);
        changed.begin(7);
        changed.end();
        changed.begin(8);
        changed.end();
        let next = emitter.frame(changed.finish(), &store, &input, false);
        assert_eq!(
            payload(&next, Section::DisplayedPositions),
            Value::Array(vec![])
        );
        assert_eq!(payload(&next, Section::OrdersRisk), serde_json::json!({}));
        assert_eq!(
            emitter.frame(empty(2000.0), &store, &input, false).len(),
            16
        );
    }
    #[test]
    fn active_chasers_make_pending_rows_and_native_inverse_evidence_is_used() {
        let mut emitter = Emitter::default();
        let mut store = store();
        store.set_positions(vec![]);
        let mut input = input();
        input.algos(&serde_json::json!([{"id":"chase-1","symbol":"BTCUSDT","size":3,"side":"buy","maxAttempts":10,"attempts":1,"isStopped":false,"stopReason":"","currentOrderIds":["o"]}]),&Vec::<serde_json::Value>::new());
        let frame = emitter.frame(empty(1000.0), &store, &input, true);
        let data = emitter.data.as_ref().unwrap();
        let row = &data.displayed_positions[0];
        assert_eq!(row.is_fake_position, Some(true));
        assert_eq!(row.position.contracts, 0.0);
        assert_eq!(row.chaser_size, Some(3.0));
        assert_eq!(row.key, None);
        assert_eq!(
            payload(&frame, Section::ActiveChasers)
                .as_array()
                .unwrap()
                .len(),
            1
        );
        store.set_positions(self::store().positions().to_vec());
        store.update_markets(|m| {
            m.is_inverse = Some(true);
            m.contract_size = Some(10.0);
        });
        store.set_quantity_units(safe_cex_engine::QuantityUnits::InverseContractSized);
        let frame = emitter.frame(empty(2000.0), &store, &input, true);
        let row = &emitter.data.as_ref().unwrap().displayed_positions[0];
        assert_eq!(row.position.unrealized_pnl, 22.0);
        assert_eq!(
            row.position.extra.as_ref().unwrap().contracts_native,
            Some(22.0)
        );
        let relabeled = with_account_label(&frame, "alias");
        let rows = payload(&relabeled, Section::DisplayedPositions);
        assert_eq!(
            rows.as_array().unwrap()[0].get("account").unwrap().as_str(),
            Some("alias")
        );
        assert_eq!(
            rows.as_array().unwrap()[0].get("key").unwrap().as_str(),
            Some("alias:BTCUSDT:long")
        );
    }
    #[test]
    fn active_twap_keeps_its_typed_market_when_the_catalogue_delists_it() {
        let mut emitter = Emitter::default();
        let mut store = store();
        let mut input = input();
        input.algos(&Vec::<serde_json::Value>::new(),&serde_json::json!([{
            "id":"twap-1","symbol":"BTCUSDT","accountName":"account-1","activeMarket":{"symbol":"BTCUSDT"},
            "size":1,"duration":60,"ordersCount":2,"side":"buy","chaserDelay":500,"reduceOnly":false,
            "randomnessPercentage":0,"useLimitOrders":false,"useChaserOrders":false,
            "ordersSizes":[0.5,0.5],"status":"running","ordersExecuted":0,"nextOrderAt":1000
        }]));
        emitter.frame(empty(1000.0), &store, &input, true);
        store.set_markets(vec![]);
        emitter.frame(empty(2000.0), &store, &input, true);
        let twaps = &emitter.data.as_ref().unwrap().active_twaps;
        assert_eq!(twaps.len(), 1);
        assert_eq!(twaps[0].active_market.id, "BTC");
    }

    #[test]
    fn pnl_windows_sample_once_a_second_and_evict_closed_positions() {
        let mut emitter = Emitter::default();
        let mut store = store();
        let mut input = input();
        input.calculate_upnl_from_last = false;
        emitter.frame(empty(1000.0), &store, &input, false);
        store.update_positions(|p| p.unrealized_pnl = 99.0);
        emitter.frame(empty(1500.0), &store, &input, false);
        assert_eq!(emitter.pnl["account-1:position-1"].len(), 1);
        emitter.frame(empty(2000.0), &store, &input, true);
        emitter.frame(empty(61_000.0), &store, &input, false);
        let row = &emitter.data.as_ref().unwrap().displayed_positions[0];
        assert_eq!(row.one_min_upnl, Some(99.0));
        assert_eq!(row.five_min_upnl, Some(20.0));
        store.set_positions(vec![]);
        emitter.frame(empty(62_000.0), &store, &input, true);
        assert!(emitter.pnl.is_empty());
    }
    #[test]
    fn risk_matches_the_maintained_typescript_across_position_and_order_branches() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut fixtures = Vec::new();
        let mut expected = Vec::new();
        for side in [PositionSide::Long, PositionSide::Short] {
            for contracts in [0.0, 0.5, 2.0, 5.0] {
                for hedged in [false, true] {
                    let mut store = store();
                    store.update_positions(|p| {
                        p.side = side;
                        p.contracts = contracts;
                        p.position_mode = Some(if hedged {
                            PositionMode::Hedged
                        } else {
                            PositionMode::Oneway
                        });
                    });
                    let mut orders = Vec::new();
                    for (i, price) in [80.0, 90.0, 105.0, 120.0, 130.0].into_iter().enumerate() {
                        for (j, kind) in [
                            OrderType::Limit,
                            OrderType::StopLoss,
                            OrderType::TrailingStopLoss,
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let mut o = CancelOrderRef {
                                id: format!("{i}-{j}"),
                                symbol: "BTCUSDT".into(),
                            }
                            .limit_stand_in();
                            o.kind = kind;
                            o.side = if (i + j) % 2 == 0 {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            };
                            o.price = price;
                            o.amount = if i == 0 { 0.0 } else { 1.0 };
                            o.remaining = o.amount;
                            o.reduce_only = i % 2 == 0;
                            orders.push(o);
                        }
                    }
                    store.set_orders(orders);
                    let data = Projection {
                        markets: store.markets().to_vec(),
                        tickers: store.tickers().to_vec(),
                        positions: store.positions().to_vec(),
                        orders: store.orders().to_vec(),
                        ..Projection::default()
                    };
                    expected.push(serde_json::to_value(risk::calculate(&data)).unwrap());
                    fixtures.push(serde_json::to_value(data).unwrap());
                }
            }
        }
        let mut child = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/risk_oracle.cjs"
            ))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&fixtures).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let actual: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(actual.len(), 16);
        for (i, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
            fn compare(actual: &serde_json::Value, expected: &serde_json::Value, path: &str) {
                use serde_json::Value;
                match (actual, expected) {
                    (Value::Number(a), Value::Number(b)) => {
                        assert!(
                            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-10,
                            "{path}: {a} != {b}"
                        )
                    }
                    (Value::Array(a), Value::Array(b)) => {
                        assert_eq!(a.len(), b.len(), "{path}");
                        for (i, (a, b)) in a.iter().zip(b).enumerate() {
                            compare(a, b, &format!("{path}[{i}]"));
                        }
                    }
                    (Value::Object(a), Value::Object(b)) => {
                        assert_eq!(a.len(), b.len(), "{path}");
                        for (key, b) in b {
                            compare(a.get(key).expect("risk field"), b, &format!("{path}.{key}"));
                        }
                    }
                    _ => assert_eq!(actual, expected, "{path}"),
                }
            }
            compare(actual, expected, &format!("risk fixture {i}"));
        }
    }

    #[test]
    fn the_pure_js_reader_decodes_the_actual_rust_encoder() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut emitter = Emitter::default();
        let mut input = input();
        input.account = "账户🌊".into();
        let frame = emitter.frame(empty(1000.0), &store(), &input, true);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../wasm/js/wire.mjs")
            .canonicalize()
            .unwrap();
        let script = format!(
            r#"import {{Frame}} from {};import assert from 'node:assert/strict';import {{readFileSync}} from 'node:fs';const b=readFileSync(0);const f=new Frame(b.buffer.slice(b.byteOffset,b.byteOffset+b.byteLength));assert.equal(f.time,1000);assert.deepEqual(f.activeChasers(),[]);assert.deepEqual(f.activeTWAPs(),[]);assert.equal(f.balanceInfo(),null);assert.equal(f.displayedPositions()[0].unrealizedPnl,20);assert.equal(f.displayedPositions()[0].account,'账户🌊');assert.equal(f.displayedMargin().longExposure,220);assert.equal(f.ordersRisk().stop[0].orderPnl,-20);assert.equal(f.socketHealth().privateInitialized,false);"#,
            serde_json::to_string(path.to_str().unwrap()).unwrap()
        );
        let mut child = Command::new("node")
            .args(["--input-type=module", "-e", &script])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&frame).unwrap();
        assert!(child.wait().unwrap().success());
    }
}
