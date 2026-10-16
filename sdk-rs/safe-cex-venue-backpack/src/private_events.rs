//! Private socket update bodies and reconciliation over compatible native host operations.
use crate::{
    private_order_utils as mapping,
    public_control::{JsString, SubscriptionState, Subscriptions},
    socket_events::{ErrorSink, report},
};
use safe_cex_num::parse_float;
use safe_cex_types::{
    MarginType, OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionMode, PositionSide,
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
};

#[derive(Debug, Default)]
pub struct FetchState {
    pub last: Cell<f64>,
    pub throttle_ms: Cell<f64>,
    pub scheduled: Cell<bool>,
    pub pending: Cell<bool>,
}
pub struct PrivateState<T> {
    pub subscriptions: SubscriptionState,
    pub positions: Rc<FetchState>,
    pub balance: Rc<FetchState>,
    pub orders_timeout: RefCell<Option<T>>,
}
pub struct PrivateSocketState<B, T> {
    pub base: B,
    pub state: PrivateState<T>,
}
// ts: BackpackPrivateWebSocket.constructor
pub fn construct<B, T, E>(
    super_constructor: impl FnOnce() -> Result<B, E>,
) -> Result<PrivateSocketState<B, T>, E> {
    let base = super_constructor()?;
    let fetch_state = || {
        Rc::new(FetchState {
            throttle_ms: Cell::new(500.0),
            ..FetchState::default()
        })
    };
    Ok(PrivateSocketState {
        base,
        state: PrivateState {
            subscriptions: Rc::new(RefCell::new(Subscriptions::default())),
            positions: fetch_state(),
            balance: fetch_state(),
            orders_timeout: RefCell::new(None),
        },
    })
}
fn number(text: &[u16]) -> f64 {
    parse_float(&String::from_utf16_lossy(text))
}
fn or_zero(value: f64) -> f64 {
    if value == 0.0 || value.is_nan() {
        0.0
    } else {
        value
    }
}
fn text_truthy(value: &Option<JsString>) -> bool {
    value.as_ref().is_some_and(|s| !s.is_empty())
}
fn text(value: Option<JsString>) -> JsString {
    value.unwrap_or_else(|| "undefined".encode_utf16().collect())
}
fn equals(value: &Option<JsString>, literal: &str) -> bool {
    value.as_deref() == Some(literal.encode_utf16().collect::<Vec<_>>().as_slice())
}

#[derive(Debug, Clone)]
pub struct OrderUpdate {
    pub id: JsString,
    pub symbol: JsString,
    pub order_type: OrderType,
    pub side: OrderSide,
    pub price: f64,
    pub amount: f64,
    pub filled: f64,
    pub remaining: f64,
    pub status: OrderStatus,
    pub last_update: f64,
    pub reduce_only: bool,
    pub time_in_force: OrderTimeInForce,
}
#[derive(Debug, Clone)]
pub struct FillUpdate {
    pub order_id: JsString,
    pub symbol: JsString,
    pub side: OrderSide,
    pub price: f64,
    pub amount: f64,
    pub timestamp: f64,
}
#[derive(Debug, Clone)]
pub struct PositionUpdate {
    pub id: JsString,
    pub symbol: JsString,
    pub side: PositionSide,
    pub entry_price: f64,
    pub notional: f64,
    pub leverage: f64,
    pub unrealized_pnl: f64,
    pub contracts: f64,
    pub liquidation_price: f64,
    pub last_update: f64,
    pub position_mode: PositionMode,
    pub margin_type: MarginType,
}
pub trait UpdateHost<D>: ErrorSink {
    type Market;
    type FillTarget;
    type TriggerTarget;
    fn trigger_target(&mut self) -> Result<Self::TriggerTarget, Self::Error>;
    fn field(&mut self, data: &D, key: &str) -> Result<Option<JsString>, Self::Error>;
    fn strict_true(&mut self, data: &D, key: &str) -> Result<bool, Self::Error>;
    fn numeric_field(&mut self, data: &D, key: &str) -> Result<f64, Self::Error>;
    fn market(&mut self, symbol: JsString) -> Result<Option<Self::Market>, Self::Error>;
    fn market_symbol(&mut self, market: &Self::Market) -> Result<JsString, Self::Error>;
    fn trigger_type(
        &mut self,
        target: Self::TriggerTarget,
        symbol: JsString,
        side: OrderSide,
        price: Option<f64>,
    ) -> Result<OrderType, Self::Error>;
    fn add_order(&mut self, order: OrderUpdate) -> Result<(), Self::Error>;
    fn fill_target(&mut self) -> Result<Self::FillTarget, Self::Error>;
    fn emit_fill(&mut self, target: Self::FillTarget, fill: FillUpdate) -> Result<(), Self::Error>;
    fn fetch_positions_throttled(&mut self) -> Result<(), Self::Error>;
    fn fetch_balance_throttled(&mut self) -> Result<(), Self::Error>;
    fn fetch_orders_throttled(&mut self) -> Result<(), Self::Error>;
}
// ts: BackpackPrivateWebSocket#handleOrderUpdate
pub fn handle_order_update<D, H: UpdateHost<D>>(data: &D, host: &mut H) -> Result<(), H::Error> {
    let result = (|| {
        let symbol = text(host.field(data, "s")?);
        let Some(market) = host.market(symbol)? else {
            return Ok(());
        };
        let side = if equals(&host.field(data, "S")?, "Bid") {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let trigger = host.field(data, "P")?;
        let trigger_price = if text_truthy(&trigger) {
            Some(number(&text(host.field(data, "P")?)))
        } else {
            None
        };
        let is_market =
            String::from_utf16_lossy(&text(host.field(data, "o")?)).to_lowercase() == "market";
        let is_trigger = trigger_price.is_some()
            || (is_market && host.strict_true(data, "r")? && !text_truthy(&host.field(data, "p")?));
        let order_type = if is_market && is_trigger {
            let target = host.trigger_target()?;
            let symbol = host.market_symbol(&market)?;
            host.trigger_type(target, symbol, side, trigger_price)?
        } else {
            mapping::map_order_type(&text(host.field(data, "o")?))
        };
        let price = if let Some(price) = trigger_price {
            price
        } else if text_truthy(&host.field(data, "p")?) {
            number(&text(host.field(data, "p")?))
        } else if text_truthy(&host.field(data, "L")?) {
            number(&text(host.field(data, "L")?))
        } else {
            0.0
        };
        let raw_amount = if is_trigger && text_truthy(&host.field(data, "Q")?) {
            host.field(data, "Q")?
        } else {
            host.field(data, "q")?
        };
        let amount = or_zero(number(&text(raw_amount)));
        let filled = number(&text(host.field(data, "z")?));
        let id = text(host.field(data, "i")?);
        let symbol = host.market_symbol(&market)?;
        let status = mapping::map_order_status(&text(host.field(data, "X")?));
        let last_update = (host.numeric_field(data, "E")? / 1000.0).floor();
        let reduce_only = host.strict_true(data, "r")? || is_trigger;
        let tif = host.field(data, "f")?;
        let time_in_force = mapping::map_time_in_force(&if text_truthy(&tif) {
            text(tif)
        } else {
            "GTC".encode_utf16().collect()
        });
        host.add_order(OrderUpdate {
            id,
            symbol,
            order_type,
            side,
            price,
            amount,
            filled,
            remaining: amount - filled,
            status,
            last_update,
            reduce_only,
            time_in_force,
        })?;
        if equals(&host.field(data, "e")?, "orderFill") && text_truthy(&host.field(data, "l")?) {
            let amount = number(&text(host.field(data, "l")?));
            let fill_price = if text_truthy(&host.field(data, "L")?) {
                number(&text(host.field(data, "L")?))
            } else {
                price
            };
            let target = host.fill_target()?;
            let order_id = text(host.field(data, "i")?);
            let symbol = host.market_symbol(&market)?;
            let side = if equals(&host.field(data, "S")?, "Bid") {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            };
            let timestamp = (host.numeric_field(data, "E")? / 1000.0).floor();
            host.emit_fill(
                target,
                FillUpdate {
                    order_id,
                    symbol,
                    side,
                    price: fill_price,
                    amount,
                    timestamp,
                },
            )?;
            host.fetch_positions_throttled()?;
            host.fetch_balance_throttled()?;
        }
        host.fetch_orders_throttled()
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => report(host, "Error handling order update: ", &error),
    }
}

pub trait PositionHost<D>: UpdateHost<D> {
    type ModeFuture: Future<Output = Result<PositionMode, Self::Error>>;
    type StoredPosition: Clone;
    type PositionList;
    type ReplaceTarget;
    fn replace_target(&mut self) -> Result<Self::ReplaceTarget, Self::Error>;
    fn position_mode(&mut self) -> Result<Self::ModeFuture, Self::Error>;
    fn positions(&mut self) -> Result<Self::PositionList, Self::Error>;
    fn positions_len(&mut self, list: &Self::PositionList) -> Result<usize, Self::Error>;
    fn position_at(
        &mut self,
        list: &Self::PositionList,
        index: usize,
    ) -> Result<Option<Self::StoredPosition>, Self::Error>;
    fn position_id(&mut self, position: &Self::StoredPosition) -> Result<JsString, Self::Error>;
    fn replace_positions(
        &mut self,
        target: Self::ReplaceTarget,
        positions: Vec<Self::StoredPosition>,
    ) -> Result<(), Self::Error>;
    fn add_position(&mut self, position: PositionUpdate) -> Result<(), Self::Error>;
}
fn position_id(symbol: JsString, side: PositionSide, mode: PositionMode) -> JsString {
    if mode == PositionMode::OneWay {
        return symbol;
    }
    let mut id = symbol;
    id.extend(
        if side == PositionSide::Long {
            ":long"
        } else {
            ":short"
        }
        .encode_utf16(),
    );
    id
}
// ts: BackpackPrivateWebSocket#handlePositionUpdate
pub fn handle_position_update<D: 'static, H: PositionHost<D> + 'static>(
    data: D,
    host: Rc<RefCell<H>>,
) -> impl Future<Output = Result<(), H::Error>> {
    let start = (|| {
        let mut h = host.borrow_mut();
        let symbol = text(h.field(&data, "s")?);
        let Some(market) = h.market(symbol)? else {
            return Ok(None);
        };
        let quantity = number(&text(h.field(&data, "q")?));
        let size = quantity.abs();
        let side = if quantity >= 0.0 {
            PositionSide::Long
        } else {
            PositionSide::Short
        };
        let mode = h.position_mode()?;
        Ok(Some((market, size, side, mode)))
    })();
    let start = match start {
        Err(e) => {
            let result = report(
                &mut *host.borrow_mut(),
                "Error handling position update: ",
                &e,
            );
            Err(result)
        }
        Ok(value) => Ok(value),
    };
    async move {
        let (market, size, side, mode_future) = match start {
            Err(result) => return result,
            Ok(None) => return Ok(()),
            Ok(Some(value)) => value,
        };
        let result = async {
            let mode = mode_future.await?;
            let mut h = host.borrow_mut();
            let id = position_id(h.market_symbol(&market)?, side, mode);
            let opposite_side = if side == PositionSide::Long {
                PositionSide::Short
            } else {
                PositionSide::Long
            };
            let opposite_id = position_id(h.market_symbol(&market)?, opposite_side, mode);
            if size == 0.0 {
                let mut filtered = Vec::new();
                let list = h.positions()?;
                let length = h.positions_len(&list)?;
                for index in 0..length {
                    let Some(position) = h.position_at(&list, index)? else {
                        continue;
                    };
                    if h.position_id(&position)? != id && h.position_id(&position)? != opposite_id {
                        filtered.push(position);
                    }
                }
                let current = h.positions()?;
                if filtered.len() != h.positions_len(&current)? {
                    let target = h.replace_target()?;
                    h.replace_positions(target, filtered)?;
                }
                return Ok(());
            }
            let mut has_opposite = false;
            let list = h.positions()?;
            let length = h.positions_len(&list)?;
            for index in 0..length {
                let Some(position) = h.position_at(&list, index)? else {
                    continue;
                };
                if h.position_id(&position)? == opposite_id {
                    has_opposite = true;
                    break;
                }
            }
            if has_opposite {
                let target = h.replace_target()?;
                let mut filtered = Vec::new();
                let list = h.positions()?;
                let length = h.positions_len(&list)?;
                for index in 0..length {
                    let Some(position) = h.position_at(&list, index)? else {
                        continue;
                    };
                    if h.position_id(&position)? != opposite_id {
                        filtered.push(position);
                    }
                }
                h.replace_positions(target, filtered)?;
            }
            let b = h.field(&data, "b")?;
            let entry = if text_truthy(&b) {
                b
            } else {
                let b = h.field(&data, "B")?;
                if text_truthy(&b) {
                    b
                } else {
                    Some("0".encode_utf16().collect())
                }
            };
            let entry_price = number(&text(entry));
            let parsed_imf = number(&text(h.field(&data, "f")?));
            let imf = if parsed_imf == 0.0 || parsed_imf.is_nan() {
                0.1
            } else {
                parsed_imf
            };
            let leverage = if imf > 0.0 {
                safe_cex_num::math_round(1.0 / imf)
            } else {
                1.0
            };
            let symbol = h.market_symbol(&market)?;
            let parsed_notional = number(&text(h.field(&data, "n")?));
            let notional = if parsed_notional == 0.0 || parsed_notional.is_nan() {
                (size * entry_price).abs()
            } else {
                parsed_notional
            };
            let unrealized_pnl = number(&text(h.field(&data, "P")?));
            let liquidation_price = or_zero(number(&text(h.field(&data, "l")?)));
            let last_update = (h.numeric_field(&data, "E")? / 1000.0).floor();
            h.add_position(PositionUpdate {
                id,
                symbol,
                side,
                entry_price,
                notional,
                leverage,
                unrealized_pnl,
                contracts: size,
                liquidation_price,
                last_update,
                position_mode: mode,
                margin_type: MarginType::Cross,
            })
        }
        .await;
        match result {
            Ok(()) => Ok(()),
            Err(error) => report(
                &mut *host.borrow_mut(),
                "Error handling position update: ",
                &error,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchKind {
    Positions,
    Balance,
    Orders,
}
pub type TimeoutCallback<E> = Box<dyn FnOnce() -> Result<(), E>>;
pub trait FetchHost {
    type Error;
    type Value;
    type OrdersTarget;
    fn orders_target(&mut self) -> Result<Self::OrdersTarget, Self::Error>;
    type FetchFuture: Future<Output = Result<Self::Value, Self::Error>>;
    fn now(&mut self) -> Result<f64, Self::Error>;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn execute(&mut self, kind: FetchKind) -> Result<(), Self::Error>;
    fn timeout(
        &mut self,
        callback: TimeoutCallback<Self::Error>,
        delay: f64,
    ) -> Result<(), Self::Error>;
    fn fetch(&mut self, kind: FetchKind) -> Result<Self::FetchFuture, Self::Error>;
    fn update(&mut self, kind: FetchKind, value: Self::Value) -> Result<(), Self::Error>;
    fn loaded(&mut self) -> Result<Vec<(JsString, bool)>, Self::Error>;
    fn update_orders(
        &mut self,
        target: Self::OrdersTarget,
        value: Self::Value,
        loaded: Vec<(JsString, bool)>,
    ) -> Result<(), Self::Error>;
}
fn throttled_fetch<H: FetchHost + 'static>(
    kind: FetchKind,
    state: Rc<FetchState>,
    host: Rc<RefCell<H>>,
) -> Result<(), H::Error> {
    let now = host.borrow_mut().now()?;
    if host.borrow_mut().disposed()? {
        return Ok(());
    }
    state.pending.set(true);
    if state.scheduled.get() {
        return Ok(());
    }
    let elapsed = now - state.last.get();
    if elapsed >= state.throttle_ms.get() {
        host.borrow_mut().execute(kind)?;
    } else {
        state.scheduled.set(true);
        let timer_state = state.clone();
        let timer_host = host.clone();
        host.borrow_mut().timeout(
            Box::new(move || {
                timer_state.scheduled.set(false);
                if !timer_host.borrow_mut().disposed()? && timer_state.pending.get() {
                    timer_host.borrow_mut().execute(kind)?;
                }
                Ok(())
            }),
            state.throttle_ms.get() - elapsed,
        )?;
    }
    Ok(())
}
// ts: BackpackPrivateWebSocket#throttledFetchPositions
pub fn throttled_fetch_positions<H: FetchHost + 'static>(
    state: Rc<FetchState>,
    host: Rc<RefCell<H>>,
) -> Result<(), H::Error> {
    throttled_fetch(FetchKind::Positions, state, host)
}
// ts: BackpackPrivateWebSocket#throttledFetchBalance
pub fn throttled_fetch_balance<H: FetchHost + 'static>(
    state: Rc<FetchState>,
    host: Rc<RefCell<H>>,
) -> Result<(), H::Error> {
    throttled_fetch(FetchKind::Balance, state, host)
}
fn execute_fetch<H: FetchHost>(
    kind: FetchKind,
    state: &FetchState,
    host: Rc<RefCell<H>>,
) -> impl Future<Output = Result<(), H::Error>> + use<H> {
    let start = (|| {
        let mut h = host.borrow_mut();
        state.last.set(h.now()?);
        state.pending.set(false);
        h.fetch(kind)
    })();
    async move {
        let value = start?.await?;
        let mut h = host.borrow_mut();
        if !h.disposed()? {
            h.update(kind, value)?;
        }
        Ok(())
    }
}
// ts: BackpackPrivateWebSocket#executePositionFetch
pub fn execute_position_fetch<H: FetchHost>(
    state: &FetchState,
    host: Rc<RefCell<H>>,
) -> impl Future<Output = Result<(), H::Error>> + use<H> {
    execute_fetch(FetchKind::Positions, state, host)
}
// ts: BackpackPrivateWebSocket#executeBalanceFetch
pub fn execute_balance_fetch<H: FetchHost>(
    state: &FetchState,
    host: Rc<RefCell<H>>,
) -> impl Future<Output = Result<(), H::Error>> + use<H> {
    execute_fetch(FetchKind::Balance, state, host)
}
// ts: BackpackPrivateWebSocket#fetchThrottledOrders
pub fn fetch_throttled_orders<T, H: FetchHost>(
    timeout: &RefCell<Option<T>>,
    host: Rc<RefCell<H>>,
) -> impl Future<Output = Result<(), H::Error>> + use<T, H> {
    *timeout.borrow_mut() = None;
    let start = (|| {
        let mut h = host.borrow_mut();
        if h.disposed()? {
            Ok(None)
        } else {
            h.fetch(FetchKind::Orders).map(Some)
        }
    })();
    async move {
        if let Some(future) = start? {
            let orders = future.await?;
            let mut h = host.borrow_mut();
            let target = h.orders_target()?;
            let mut loaded = h.loaded()?;
            let key: JsString = "orders".encode_utf16().collect();
            if let Some(entry) = loaded.iter_mut().find(|(name, _)| *name == key) {
                entry.1 = true;
            } else {
                loaded.push((key, true));
            }
            h.update_orders(target, orders, loaded)?;
        }
        Ok(())
    }
}
