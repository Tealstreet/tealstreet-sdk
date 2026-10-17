//! REST order snapshots and private user-order event field bodies. Native order
//! fields pair with original raw info roots; stores/snapshot assembly are supplied.
use crate::{
    market_position_reads::{ReadItem, ReadItems, ReadMarket},
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
    utils::{format_order_state, format_order_type, normalize_cfm_contract_size},
};
use indexmap::IndexMap;
use safe_cex_num::{add, js_string, js_truthy, multiply, parse_float};
use safe_cex_types::{Order, OrderExtra, OrderSide};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
#[derive(Debug)]
pub struct SyncedOrder<I = ReadItem> {
    pub fields: Order,
    pub info: I,
}
pub type SyncedOrderRoot<I = ReadItem> = Rc<RefCell<SyncedOrder<I>>>;
pub type SyncedOrders<I = ReadItem> = Rc<RefCell<Vec<SyncedOrderRoot<I>>>>;
pub type OrderSyncAsync<T> = Result<Promise<Result<T, PortfolioFailure>>, PortfolioFailure>;
#[derive(Debug, Clone)]
pub struct OrderPageQuery {
    /// Native page query slots; an absent cursor represents an omitted property.
    pub product_type: Option<&'static str>,
    pub contract_expiry_type: Option<&'static str>,
    pub order_status: &'static str,
    pub limit: f64,
    pub cursor: Option<Value>,
}
#[derive(Clone)]
pub struct OrderPage {
    pub orders: ReadItems,
    pub has_next: Option<Value>,
    pub cursor: Option<Value>,
}
pub trait OrderSnapshotHost {
    fn public_only(&self) -> bool;
    fn supported_types(&self) -> OrderSyncAsync<Rc<RefCell<Vec<String>>>>;
    fn get_orders(
        &self,
        endpoint: &'static str,
        query: OrderPageQuery,
    ) -> OrderSyncAsync<OrderPage>;
    fn resolve_market_by_product(
        &self,
        product_id: String,
    ) -> Result<Option<ReadMarket>, PortfolioFailure>;
    fn map_rest_order(
        &self,
        raw: ReadItem,
        market: ReadMarket,
        now: f64,
    ) -> Result<Option<SyncedOrderRoot>, PortfolioFailure>;
    fn now_ms(&self) -> Result<f64, PortfolioFailure>;
    fn emit_and_log_error(&self, message: String) -> Result<(), PortfolioFailure>;
    fn emit_orders_fetch_error(&self, error: PortfolioFailure) -> Result<(), PortfolioFailure>;
}
pub struct OrderSnapshotReceiver<H> {
    pub host: H,
    pub spawn: CoinbaseStarbasePromiseSpawner,
}
fn error(name: &str, message: &str) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(
        json!({"name":name,"message":message}),
    )))
}
fn property(raw: &ReadItem, name: &str) -> Result<Option<Value>, PortfolioFailure> {
    let raw = raw.borrow();
    if raw.is_null() {
        return Err(error(
            "TypeError",
            &format!("Cannot read properties of null (reading '{name}')"),
        ));
    }
    Ok(raw.get(name).cloned())
}
fn text(value: Option<&Value>) -> Result<String, PortfolioFailure> {
    js_string(value).map_err(|e| error(e.name, e.message))
}
fn float(value: Option<&Value>) -> Result<f64, PortfolioFailure> {
    Ok(parse_float(&text(value)?))
}
pub fn fetch_orders<H: OrderSnapshotHost + 'static>(
    r: Rc<OrderSnapshotReceiver<H>>,
) -> Promise<Result<SyncedOrders, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            match fetch_open_orders_snapshot(r.clone()).await {
                Ok(v) => Ok(v),
                Err(e) => {
                    r.host.emit_orders_fetch_error(e.clone())?;
                    Err(e)
                }
            }
        }),
        &spawn,
    )
}
pub fn fetch_open_orders_snapshot<H: OrderSnapshotHost + 'static>(
    r: Rc<OrderSnapshotReceiver<H>>,
) -> Promise<Result<SyncedOrders, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            if r.host.public_only() {
                return Ok(Rc::new(RefCell::new(vec![])));
            }
            let types = r.host.supported_types()?.await?;
            let requests: Vec<_> = types
                .borrow()
                .iter()
                .map(|kind| {
                    (
                        if kind == "spot" { "SPOT" } else { "FUTURE" },
                        if kind == "cfm" {
                            Some("EXPIRING")
                        } else if kind == "intx" {
                            Some("PERPETUAL")
                        } else {
                            None
                        },
                    )
                })
                .collect();
            let mut by_id = IndexMap::<String, SyncedOrderRoot>::new();
            let mut missing = IndexMap::<String, f64>::new();
            let mut unsupported = IndexMap::<String, f64>::new();
            for (product_type, expiry) in requests {
                for status in ["PENDING", "OPEN", "QUEUED", "CANCEL_QUEUED", "EDIT_QUEUED"] {
                    let mut cursor = None;
                    loop {
                        let page = r
                            .host
                            .get_orders(
                                "/api/v3/brokerage/orders/historical/batch",
                                OrderPageQuery {
                                    product_type: Some(product_type),
                                    contract_expiry_type: expiry,
                                    order_status: status,
                                    limit: 100.0,
                                    cursor: cursor.clone(),
                                },
                            )?
                            .await?;
                        let mut index = 0;
                        loop {
                            let raw = page.orders.borrow().get(index).cloned();
                            let Some(raw) = raw else { break };
                            index += 1;
                            let product = text(property(&raw, "product_id")?.as_ref())?;
                            let market = r.host.resolve_market_by_product(product)?;
                            let Some(market) = market else {
                                let product = text(property(&raw, "product_id")?.as_ref())?;
                                *missing.entry(product).or_default() += 1.0;
                                continue;
                            };
                            let now = r.host.now_ms()?;
                            let order = r.host.map_rest_order(raw.clone(), market, now)?;
                            let Some(order) = order else {
                                let product = text(property(&raw, "product_id")?.as_ref())?;
                                *unsupported.entry(product).or_default() += 1.0;
                                continue;
                            };
                            let id = order.borrow().fields.id.clone();
                            by_id.insert(id, order);
                        }
                        cursor = if js_truthy(page.has_next.as_ref()) {
                            page.cursor
                        } else {
                            None
                        };
                        if !js_truthy(cursor.as_ref()) {
                            break;
                        }
                    }
                }
            }
            for (counts, prefix) in [
                (&missing, "Coinbase orders skipped, missing market: "),
                (
                    &unsupported,
                    "Coinbase orders skipped, unsupported REST shape: ",
                ),
            ] {
                if !counts.is_empty() {
                    let summary = counts
                        .iter()
                        .map(|(product, count)| {
                            format!("{product} ({})", safe_cex_num::js_number_string(*count))
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    r.host.emit_and_log_error(format!("{prefix}{summary}"))?;
                }
            }
            Ok(Rc::new(RefCell::new(by_id.into_values().collect())))
        }),
        &spawn,
    )
}
pub type PrivateOrderMarkets = Rc<RefCell<IndexMap<String, ReadMarket>>>;
#[derive(Clone)]
pub struct PrivateOrderEvent {
    pub event_type: Option<String>,
    pub orders: ReadItems,
}
pub type PrivateOrderEventRoot = Rc<RefCell<PrivateOrderEvent>>;
#[derive(Debug, Clone)]
pub struct PrivateOrderFill<S = Value> {
    pub id: String,
    pub side: safe_cex_types::ModelField<OrderSide>,
    pub symbol: String,
    pub price: S,
    pub amount: f64,
}

/// Raw private order data stays in its original native object graph.
pub trait PrivateOrderScalar: Clone {
    fn float(&self) -> Result<f64, PortfolioFailure>;
    fn text(&self) -> Result<String, PortfolioFailure>;
    fn truthy(&self) -> bool;
    fn is_text(&self, expected: &str) -> bool;
    fn from_text(value: &str) -> Self;
}
impl PrivateOrderScalar for Value {
    fn float(&self) -> Result<f64, PortfolioFailure> {
        float(Some(self))
    }
    fn text(&self) -> Result<String, PortfolioFailure> {
        text(Some(self))
    }
    fn truthy(&self) -> bool {
        js_truthy(Some(self))
    }
    fn is_text(&self, expected: &str) -> bool {
        self.as_str() == Some(expected)
    }
    fn from_text(value: &str) -> Self {
        Value::from(value)
    }
}
impl PrivateOrderScalar for crate::starbase_public_frames::CoinbaseStarbaseFrameValue {
    fn float(&self) -> Result<f64, PortfolioFailure> {
        let units =
            crate::public_data::string(Some(self.clone())).map_err(|e| error(e.name, e.message))?;
        Ok(parse_float(&String::from_utf16_lossy(&units)))
    }
    fn text(&self) -> Result<String, PortfolioFailure> {
        let units =
            crate::public_data::string(Some(self.clone())).map_err(|e| error(e.name, e.message))?;
        String::from_utf16(&units).map_err(|_| error("TypeError", "Expected a UTF-8 order field"))
    }
    fn truthy(&self) -> bool {
        use crate::starbase_public_frames::CoinbaseStarbaseFrameValue as F;
        match self {
            F::Null => false,
            F::Bool(v) => *v,
            F::Number(v) => *v != 0.0 && !v.is_nan(),
            F::String(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn is_text(&self, expected: &str) -> bool {
        matches!(self,Self::String(value) if value.iter().copied().eq(expected.encode_utf16()))
    }
    fn from_text(value: &str) -> Self {
        Self::String(value.encode_utf16().collect())
    }
}
pub trait PrivateOrderInfo: Clone {
    type Scalar: PrivateOrderScalar;
    fn property(&self, name: &str) -> Result<Option<Self::Scalar>, PortfolioFailure>;
}
impl PrivateOrderInfo for ReadItem {
    type Scalar = Value;
    fn property(&self, name: &str) -> Result<Option<Value>, PortfolioFailure> {
        property(self, name)
    }
}
impl PrivateOrderInfo for crate::starbase_public_frames::CoinbaseStarbaseFrameValue {
    type Scalar = Self;
    fn property(&self, name: &str) -> Result<Option<Self>, PortfolioFailure> {
        if matches!(self, Self::Null) {
            Err(error(
                "TypeError",
                &format!("Cannot read properties of null (reading '{name}')"),
            ))
        } else {
            Ok(self.get(name))
        }
    }
}
pub trait PrivateOrderArray<I> {
    fn item(&self, index: usize) -> Option<I>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
impl<I: Clone> PrivateOrderArray<I> for Rc<RefCell<Vec<I>>> {
    fn item(&self, index: usize) -> Option<I> {
        self.borrow().get(index).cloned()
    }
    fn len(&self) -> usize {
        self.borrow().len()
    }
}
pub trait PrivateOrderEventInput<I> {
    type Array: PrivateOrderArray<I>;
    fn orders(&self) -> Result<Self::Array, PortfolioFailure>;
    fn kind(&self) -> Result<Option<String>, PortfolioFailure>;
}
impl PrivateOrderEventInput<ReadItem> for PrivateOrderEventRoot {
    type Array = ReadItems;
    fn orders(&self) -> Result<ReadItems, PortfolioFailure> {
        Ok(self.borrow().orders.clone())
    }
    fn kind(&self) -> Result<Option<String>, PortfolioFailure> {
        Ok(self.borrow().event_type.clone())
    }
}
impl PrivateOrderEventInput<crate::starbase_public_frames::CoinbaseStarbaseFrameValue>
    for crate::starbase_public_frames::CoinbaseStarbaseFrameValue
{
    type Array = Rc<RefCell<Vec<Self>>>;
    fn orders(&self) -> Result<<Self as PrivateOrderEventInput<Self>>::Array, PortfolioFailure> {
        match self.property("orders")? {
            Some(Self::Array(value)) => Ok(value),
            _ => Err(error("TypeError", "data.orders is not iterable")),
        }
    }
    fn kind(&self) -> Result<Option<String>, PortfolioFailure> {
        Ok(match self.property("type")? {
            Some(Self::String(value)) => String::from_utf16(&value).ok(),
            _ => None,
        })
    }
}
fn event_text<S: PrivateOrderScalar>(value: Option<&S>) -> Result<String, PortfolioFailure> {
    value.map_or_else(|| Ok("undefined".into()), PrivateOrderScalar::text)
}
fn event_float<S: PrivateOrderScalar>(value: Option<&S>) -> Result<f64, PortfolioFailure> {
    value.map_or_else(|| Ok(f64::NAN), PrivateOrderScalar::float)
}
fn event_truthy<S: PrivateOrderScalar>(value: Option<&S>) -> bool {
    value.is_some_and(PrivateOrderScalar::truthy)
}

pub trait PrivateOrderEventHost<I: PrivateOrderInfo = ReadItem> {
    fn markets_by_id(&self) -> Result<PrivateOrderMarkets, PortfolioFailure>;
    /// Imported alias resolver over the captured record; preserves selected market identity.
    fn resolve_coinbase_market(
        &self,
        product_id: String,
        markets: PrivateOrderMarkets,
    ) -> Result<Option<ReadMarket>, PortfolioFailure>;
    fn emit_and_log_error(&self, message: String) -> Result<(), PortfolioFailure>;
    fn console_error(&self, message: &str, value: &str) -> Result<(), PortfolioFailure>;
    fn current_time_ms(&self) -> Result<f64, PortfolioFailure>;
    fn snapshot_active(&self) -> Result<bool, PortfolioFailure>;
    fn merge_snapshot_orders(&self, orders: SyncedOrders<I>) -> Result<(), PortfolioFailure>;
    fn apply_order_snapshot_batch(
        &self,
        raw_count: f64,
        orders: SyncedOrders<I>,
    ) -> Result<(), PortfolioFailure>;
    fn add_or_update_orders(&self, orders: SyncedOrders<I>) -> Result<(), PortfolioFailure>;
    fn emit_fill(&self, fill: PrivateOrderFill<I::Scalar>) -> Result<(), PortfolioFailure>;
}
pub fn resolve_order_market<I: PrivateOrderInfo>(
    host: &impl PrivateOrderEventHost<I>,
    product_id: String,
) -> Result<Option<ReadMarket>, PortfolioFailure> {
    let markets = host.markets_by_id()?;
    host.resolve_coinbase_market(product_id, markets)
}
pub fn handle_user_event_orders<I: PrivateOrderInfo, E: PrivateOrderEventInput<I>>(
    host: &impl PrivateOrderEventHost<I>,
    event: E,
) -> Result<(), PortfolioFailure> {
    let orders = Rc::new(RefCell::new(vec![]));
    let raw_orders = event.orders()?;
    let mut index = 0;
    loop {
        let Some(raw) = raw_orders.item(index) else {
            break;
        };
        index += 1;
        let filled_native = event_float(raw.property("cumulative_quantity")?.as_ref())?;
        let remaining_native = event_float(raw.property("leaves_quantity")?.as_ref())?;
        let product = event_text(raw.property("product_id")?.as_ref())?;
        let market = resolve_order_market(host, product)?;
        let Some(market) = market else {
            host.emit_and_log_error(format!(
                "Coinbase order skipped: missing market for {}",
                event_text(raw.property("product_id")?.as_ref())?
            ))?;
            continue;
        };
        let contract = if market.borrow().is_expiring.unwrap_or(false) {
            market
                .borrow()
                .contract_size
                .and_then(|v| normalize_cfm_contract_size(&json!(v)))
        } else {
            Some(1.0)
        };
        let Some(contract) = contract else {
            host.emit_and_log_error(format!(
                "Coinbase CFM order skipped: invalid contract size for {}",
                event_text(raw.property("product_id")?.as_ref())?
            ))?;
            continue;
        };
        let filled = multiply(filled_native, contract);
        let remaining = multiply(remaining_native, contract);
        let amount = add(filled, remaining);
        let id = event_text(raw.property("order_id")?.as_ref())?;
        let parent_id = Some(event_text(raw.property("client_order_id")?.as_ref())?);
        let state = event_text(raw.property("status")?.as_ref())?;
        let mut failed = None;
        let status = format_order_state(&state, |m, v| {
            if let Err(e) = host.console_error(m, v) {
                failed = Some(e)
            }
        });
        if let Some(e) = failed {
            return Err(e);
        }
        let symbol = market.borrow().symbol.clone();
        let kind = event_text(raw.property("order_type")?.as_ref())?;
        let mut failed = None;
        let kind = format_order_type(&kind, |m, v| {
            if let Err(e) = host.console_error(m, v) {
                failed = Some(e)
            }
        });
        if let Some(e) = failed {
            return Err(e);
        }
        let side = if raw
            .property("order_side")?
            .as_ref()
            .is_some_and(|value| value.is_text("BUY"))
        {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let stop = raw.property("stop_price")?;
        let limit = raw.property("limit_price")?;
        let zero = I::Scalar::from_text("0");
        let price = event_float(if event_truthy(stop.as_ref()) {
            stop.as_ref()
        } else if event_truthy(limit.as_ref()) {
            limit.as_ref()
        } else {
            Some(&zero)
        })?;
        let now = host.current_time_ms()?;
        orders.borrow_mut().push(Rc::new(RefCell::new(SyncedOrder {
            fields: Order {
                client_order_id: (None).into(),
                id,
                parent_id: parent_id.into(),
                status: status.into(),
                symbol,
                r#type: (kind).into(),
                side: side.into(),
                price: price.into(),
                amount,
                filled,
                remaining,
                extra: (Some(OrderExtra {
                    remaining_native: Some(remaining_native),
                    filled_in_placement_units: None,
                    remaining_notional: None,
                    conditional_order_price: None,
                    reject_reason: None,
                    is_from_chaser: None,
                    chaser_id: None,
                    chaser_attempts: None,
                    chaser_max_attempts: None,
                }))
                .into(),
                reduce_only: false.into(),
                info: (None).into(),
                last_update: now,
                close: (None).into(),
                time_in_force: (None).into(),
                trigger_type: (None).into(),
                tp_trigger_type: (None).into(),
                sl_trigger_type: (None).into(),
                take_profit: (None).into(),
                stop_loss: (None).into(),
            },
            info: raw,
        })));
    }
    let kind = event.kind()?;
    if kind.as_deref() == Some("snapshot") {
        let raw_count = event.orders()?.len() as f64;
        host.apply_order_snapshot_batch(raw_count, orders)?;
    } else if kind.as_deref() == Some("update") {
        if host.snapshot_active()? {
            host.merge_snapshot_orders(orders.clone())?;
        } else {
            host.add_or_update_orders(orders.clone())?;
        }
        let len = orders.borrow().len();
        for i in 0..len {
            let Some(order) = orders.borrow().get(i).cloned() else {
                continue;
            };
            let info = order.borrow().info.clone();
            if info
                .property("status")?
                .as_ref()
                .is_some_and(|value| value.is_text("FILLED"))
            {
                let id = order.borrow().fields.id.clone();
                let side = order.borrow().fields.side;
                let symbol = order.borrow().fields.symbol.clone();
                let price = info.property("avg_price")?;
                let price = if event_truthy(price.as_ref()) {
                    price.expect("truthy avg price")
                } else {
                    I::Scalar::from_text("market")
                };
                let amount = order.borrow().fields.filled;
                host.emit_fill(PrivateOrderFill {
                    id,
                    side,
                    symbol,
                    price,
                    amount,
                })?;
            }
        }
    }
    Ok(())
}
