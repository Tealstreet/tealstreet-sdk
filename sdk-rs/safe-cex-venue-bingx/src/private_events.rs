//! Private fills/account bodies over the shared Store and landed REST position reads.
use crate::{constants, data_reads::BingxDataReads};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::{TableHit, js_table_lookup, math_max, subtract};
use safe_cex_store::StoreUpdate;
use safe_cex_types::{
    MarginType, Order, OrderStatus, OrderTimeInForce, OrderType, Position, PositionMode,
    PositionSide, model_utils::create_position_id,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut e = ExchangeError::new(message);
    e.name = "TypeError".into();
    e
}
fn text(value: Option<&Input>) -> Result<String, ExchangeError> {
    safe_cex_num::js_retained_string(value).map_err(|e| type_error(e.message))
}
fn first<'a>(row: &'a Input, keys: &[&str]) -> Option<&'a Input> {
    for (i, key) in keys.iter().enumerate() {
        let v = row.get(key);
        if v.is_some_and(|v| !v.is_nullish()) || i + 1 == keys.len() {
            return v;
        }
    }
    None
}
fn float(value: Option<&Input>) -> Result<f64, ExchangeError> {
    safe_cex_num::js_retained_parse_float(value).map_err(|e| type_error(e.message))
}

fn table<T: Copy>(rows: &[(&str, T)], key: &str) -> Result<Option<T>, ExchangeError> {
    match js_table_lookup(rows, key) {
        TableHit::Mapped(v) => Ok(Some(*v)),
        TableHit::Missing => Ok(None),
        TableHit::Inherited(_) => Err(type_error("BingX typed enum refuses an inherited callable")),
    }
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

use safe_cex_types::retained_json::RetainedJsonInput as Input;
fn number(value: Option<&Input>) -> Result<f64, ExchangeError> {
    safe_cex_num::js_retained_number(value).map_err(|e| type_error(e.message))
}
fn strict_true(value: Option<&Input>) -> bool {
    value.is_some_and(|v| matches!(v, Input::Bool(true)) || v.text_equals("true"))
}
pub fn map_order(o: &Input, orders: &[Order], now: f64) -> Result<Option<Order>, ExchangeError> {
    let id = first(o, &["orderId", "orderID", "i"]);
    let symbol = first(o, &["symbol", "s"])
        .and_then(Input::as_text)
        .and_then(|s| String::from_utf16(s).ok())
        .map(|s| s.replace('-', ""));
    let kind = table(
        &constants::ORDER_TYPE,
        &text(first(o, &["type", "o"]))?.to_uppercase(),
    )?;
    let side = table(
        &constants::ORDER_SIDE,
        &text(first(o, &["side", "S"]))?.to_uppercase(),
    )?;
    let raw_status = first(o, &["status", "X"]);
    let status = if raw_status.is_none() {
        Some(OrderStatus::Open)
    } else {
        table(&constants::ORDER_STATUS, &text(raw_status)?.to_uppercase())?
    };
    let (Some(id), Some(symbol), Some(kind), Some(side), Some(status)) =
        (id, symbol, kind, side, status)
    else {
        return Ok(None);
    };
    if symbol.is_empty() {
        return Ok(None);
    }
    let close = strict_true(o.get("closePosition"));
    let raw_amount = first(o, &["origQty", "quantity", "q"]);
    let price = number(first(o, &["price", "p"]))?;
    let stop = number(first(o, &["stopPrice", "sp"]))?;
    let amount = if raw_amount.is_none() && close {
        0.0
    } else {
        number(raw_amount)?
    };
    let zero = Input::Float(0.0);
    let filled = number(
        first(o, &["executedQty", "z"])
            .filter(|v| !v.is_nullish())
            .or(Some(&zero)),
    )?;
    if !amount.is_finite() || !filled.is_finite() {
        return Ok(None);
    }
    let id = text(Some(id))?;
    let existing = orders.iter().find(|order| order.id == id);
    let raw_tif = first(o, &["timeInForce", "f"]);
    let tif = if strict_true(o.get("postOnly")) {
        Some(OrderTimeInForce::PostOnly)
    } else {
        table(&constants::TIME_IN_FORCE, &text(raw_tif)?)?.or_else(|| {
            if raw_tif.is_none() {
                existing.and_then(|o| (o.time_in_force).into_value())
            } else {
                Some(OrderTimeInForce::GoodTillCancel)
            }
        })
    };
    let trigger = if matches!(kind, OrderType::TakeProfit | OrderType::StopLoss) {
        table(
            &constants::ORDER_TRIGGER_TYPE,
            &text(first(o, &["workingType", "wt"]))?,
        )?
    } else {
        None
    };
    Ok(Some(Order {
        client_order_id: (None).into(),
        id,
        parent_id: (None).into(),
        status: status.into(),
        symbol,
        r#type: (kind).into(),
        side: side.into(),
        price: if price.is_finite() && price > 0.0 {
            (price).into()
        } else if stop.is_finite() {
            (stop).into()
        } else {
            (0.0).into()
        },
        amount,
        filled,
        remaining: subtract(amount, filled),
        reduce_only: strict_true(first(o, &["reduceOnly", "ro", "R"])).into(),
        close: (Some(close)).into(),
        info: (Some(o.clone())).into(),
        time_in_force: (tif).into(),
        last_update: now,
        trigger_type: (None).into(),
        tp_trigger_type: if kind == OrderType::TakeProfit {
            (trigger).into()
        } else {
            (None).into()
        },
        sl_trigger_type: if kind == OrderType::StopLoss {
            (trigger).into()
        } else {
            (None).into()
        },
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (None).into(),
    }))
}
/// Generic imported operations preserve every thrown-value identity in the source body.
pub fn private_ping<E>(
    disposed: bool,
    now: impl FnOnce() -> Result<f64, E>,
    assign: impl FnOnce(f64) -> Result<(), E>,
    send: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    if !disposed {
        assign(now()?)?;
        send("Ping")?;
    }
    Ok(())
}
/// Imported timer/task, socket, clock and emitter capabilities; no pending venue body.
pub trait PrivateEventHost {
    fn is_disposed(&self) -> bool;
    fn now_ms(&self) -> f64;
    fn set_timeout(
        &self,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<(), ExchangeError>>,
    ) -> Result<(), ExchangeError>;
    fn spawn(&self, future: ExchangeFuture<'static, ()>) -> Result<(), ExchangeError>;
    fn emit_fill(&self, payload: Input) -> Result<(), ExchangeError>;
    fn report_exception(&self, error: &ExchangeError);
}
pub struct BingxPrivateEvents {
    pub parent: Rc<BingxDataReads>,
    pub host: Rc<dyn PrivateEventHost>,
    pub last_position_fetch_time: Cell<f64>,
    pub position_fetch_throttle_ms: Cell<f64>,
    pub position_fetch_scheduled: Cell<bool>,
    pub has_pending_position_events: Cell<bool>,
    pub emitted_fill_qty_by_order_id: RefCell<Vec<(String, f64)>>,
}
impl BingxPrivateEvents {
    pub fn new(parent: Rc<BingxDataReads>, host: Rc<dyn PrivateEventHost>) -> Rc<Self> {
        Rc::new(Self {
            parent,
            host,
            last_position_fetch_time: Cell::new(0.0),
            position_fetch_throttle_ms: Cell::new(750.0),
            position_fetch_scheduled: Cell::new(false),
            has_pending_position_events: Cell::new(false),
            emitted_fill_qty_by_order_id: RefCell::default(),
        })
    }
    pub fn normalize_symbol(&self, symbol: Option<&Input>) -> Result<String, ExchangeError> {
        let empty = Input::Text(vec![]);
        text(if symbol.is_some_and(Input::is_truthy) {
            symbol
        } else {
            Some(&empty)
        })
        .map(|s| s.replace('-', ""))
    }
    pub fn extract_account_positions(&self, event: &Input) -> Vec<Input> {
        for key in ["P", "B", "p"] {
            let Some(rows) = event
                .get("a")
                .and_then(|a| a.get(key))
                .and_then(Input::as_array)
            else {
                continue;
            };
            let positions = rows
                .iter()
                .filter(|row| {
                    matches!(row, Input::Object(_) | Input::Array(_))
                        && row
                            .get("s")
                            .and_then(Input::as_text)
                            .and_then(|v| String::from_utf16(v).ok())
                            .as_deref()
                            .is_some_and(|s| !s.is_empty())
                        && [
                            "pa",
                            "positionAmt",
                            "ep",
                            "avgPrice",
                            "up",
                            "unrealizedProfit",
                        ]
                        .iter()
                        .any(|key| row.get(key).is_some())
                })
                .cloned()
                .collect::<Vec<_>>();
            if !positions.is_empty() {
                return positions;
            }
        }
        Vec::new()
    }
    fn launch_refresh(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let owner = self.clone();
        self.host.spawn(Box::pin(async move {
            owner.execute_position_refresh().await;
            Ok(())
        }))
    }
    pub fn schedule_position_refresh(self: &Rc<Self>) -> Result<(), ExchangeError> {
        self.has_pending_position_events.set(true);
        let now = self.host.now_ms();
        if self.host.is_disposed() || self.position_fetch_scheduled.get() {
            return Ok(());
        }
        let since = now - self.last_position_fetch_time.get();
        if since >= self.position_fetch_throttle_ms.get() {
            return self.launch_refresh();
        }
        self.position_fetch_scheduled.set(true);
        let weak = Rc::downgrade(self);
        self.host.set_timeout(
            self.position_fetch_throttle_ms.get() - since,
            Rc::new(move || {
                let owner = weak
                    .upgrade()
                    .ok_or_else(|| ExchangeError::new("BingX private event owner released"))?;
                owner.position_fetch_scheduled.set(false);
                if !owner.host.is_disposed() && owner.has_pending_position_events.get() {
                    owner.launch_refresh()?;
                }
                Ok(())
            }),
        )
    }
    pub async fn execute_position_refresh(self: &Rc<Self>) {
        let result = async {
            self.last_position_fetch_time.set(self.host.now_ms());
            self.has_pending_position_events.set(false);
            let positions = self.parent.fetch_positions(None).await?;
            if self.host.is_disposed() {
                return Ok(());
            }
            self.parent
                .base
                .store
                .add_or_update_positions_full(positions)
                .map_err(ExchangeError::from)?;
            let mut loaded = self.parent.base.store.loaded().clone();
            loaded.positions = true;
            self.parent
                .base
                .store
                .update(
                    StoreUpdate {
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )
                .map_err(ExchangeError::from)
        }
        .await;
        if let Err(e) = result {
            self.host.report_exception(&e);
        }
    }
    pub fn handle_order_events(self: &Rc<Self>, events: &[Input]) -> Result<(), ExchangeError> {
        for event in events {
            let data = event
                .get("o")
                .ok_or_else(|| type_error("Cannot read properties of undefined (reading 'X')"))?;
            let owned_status = data
                .get("X")
                .and_then(Input::as_text)
                .and_then(|v| String::from_utf16(v).ok());
            let status = owned_status.as_deref();
            if matches!(status, Some("PARTIALLY_FILLED" | "FILLED")) {
                let id = text(data.get("i"))?;
                let previous = self
                    .emitted_fill_qty_by_order_id
                    .borrow()
                    .iter()
                    .find(|(k, _)| k == &id)
                    .map_or(0.0, |(_, v)| *v);
                let zero = Input::Float(0.0);
                let cumulative = float(data.get("z").filter(|v| !v.is_nullish()).or(Some(&zero)))?;
                let direct = float(data.get("l").filter(|v| !v.is_nullish()).or(Some(&zero)))?;
                let delta = if cumulative.is_finite() {
                    math_max(cumulative - previous, 0.0)
                } else {
                    0.0
                };
                let increment = if cumulative.is_finite() {
                    delta
                } else if direct.is_finite() && direct > 0.0 {
                    direct
                } else {
                    0.0
                };
                if increment > 0.0 {
                    self.schedule_position_refresh()?;
                    if status == Some("FILLED") {
                        let price = float(
                            data.get("ap")
                                .filter(|v| v.is_truthy())
                                .or_else(|| data.get("p").filter(|v| v.is_truthy()))
                                .or(Some(&zero)),
                        )?;
                        let amount = if cumulative.is_finite() && cumulative > 0.0 {
                            cumulative
                        } else {
                            increment
                        };
                        let symbol = data
                            .get("s")
                            .and_then(Input::as_text)
                            .and_then(|v| String::from_utf16(v).ok())
                            .as_deref()
                            .ok_or_else(|| type_error("fill symbol has no replace method"))?
                            .replace('-', "");
                        let side = table(&constants::ORDER_SIDE, &text(data.get("S"))?)?;
                        let mut payload = Input::Object(vec![
                            (
                                "symbol".encode_utf16().collect(),
                                Input::Text(symbol.encode_utf16().collect()),
                            ),
                            (
                                "price".encode_utf16().collect(),
                                Input::Float(finite_or_zero(price)),
                            ),
                            ("amount".encode_utf16().collect(), Input::Float(amount)),
                        ]);
                        if let Some(id) = data.get("i") {
                            payload
                                .as_object_mut()
                                .unwrap()
                                .push(("id".encode_utf16().collect(), id.clone()));
                        }
                        if let Some(side) = side {
                            payload.as_object_mut().unwrap().push((
                                "side".encode_utf16().collect(),
                                Input::from(
                                    serde_json::to_value(side)
                                        .map_err(|e| ExchangeError::new(e.to_string()))?,
                                ),
                            ));
                        }
                        self.host.emit_fill(payload)?;
                    }
                }
                if cumulative.is_finite() && cumulative >= previous {
                    let mut emitted = self.emitted_fill_qty_by_order_id.borrow_mut();
                    if let Some((_, qty)) = emitted.iter_mut().find(|(k, _)| k == &id) {
                        *qty = cumulative;
                    } else {
                        emitted.push((id, cumulative));
                    }
                    if emitted.len() > 5000 {
                        emitted.clear();
                    }
                }
            }
            let order = map_order(
                data,
                &self.parent.base.store.orders(),
                safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms(),
            )?;
            if let Some(order) = order
                && order.status == OrderStatus::Open
                && order.r#type != OrderType::Market
            {
                self.parent
                    .base
                    .store
                    .add_or_update_order(order)
                    .map_err(ExchangeError::from)?;
            }
            if matches!(status, Some("CANCELED" | "EXPIRED" | "FILLED")) {
                let id = text(data.get("i"))?;
                if let Some(stored_id) = data
                    .get("i")
                    .and_then(Input::as_text)
                    .and_then(|v| String::from_utf16(v).ok())
                    .as_deref()
                {
                    self.parent
                        .base
                        .store
                        .update_order_status(
                            stored_id,
                            if status == Some("FILLED") {
                                OrderStatus::Closed
                            } else {
                                OrderStatus::Canceled
                            },
                        )
                        .map_err(ExchangeError::from)?;
                }
                self.emitted_fill_qty_by_order_id
                    .borrow_mut()
                    .retain(|(k, _)| k != &id);
            }
        }
        Ok(())
    }
    pub async fn handle_account_events(
        self: &Rc<Self>,
        events: &[Input],
    ) -> Result<(), ExchangeError> {
        let account_mode = match self.parent.get_or_fetch_position_mode(false).await {
            Ok(mode) => Some(mode),
            Err(error) => {
                self.host.report_exception(&error);
                None
            }
        };
        for event in events {
            let updates = self.extract_account_positions(event);
            if !updates.is_empty() {
                self.schedule_position_refresh()?;
            }
            for update in updates {
                let symbol = self.normalize_symbol(update.get("s"))?;
                if symbol.is_empty() {
                    continue;
                }
                let zero = Input::Float(0.0);
                let raw = float(
                    first(&update, &["pa", "positionAmt"])
                        .filter(|v| !v.is_nullish())
                        .or(Some(&zero)),
                )?;
                if !raw.is_finite() {
                    continue;
                }
                let ws = update
                    .get("ps")
                    .and_then(Input::as_text)
                    .and_then(|v| String::from_utf16(v).ok())
                    .unwrap_or_default();
                let fallback = if matches!(ws.as_str(), "LONG" | "SHORT") {
                    PositionMode::Hedged
                } else {
                    PositionMode::OneWay
                };
                let mode = account_mode.unwrap_or(fallback);
                let inferred = if raw > 0.0 {
                    Some(PositionSide::Long)
                } else if raw < 0.0 {
                    Some(PositionSide::Short)
                } else {
                    None
                };
                let positions = self.parent.base.store.positions();
                let for_symbol = positions
                    .iter()
                    .filter(|p| p.symbol == symbol)
                    .collect::<Vec<_>>();
                let mapped = if mode == PositionMode::Hedged {
                    table(&constants::POSITION_SIDE, &ws)?
                } else {
                    None
                };
                let side = if mode == PositionMode::OneWay {
                    inferred.or_else(|| for_symbol.first().and_then(|p| p.side.into_value()))
                } else {
                    mapped.or(inferred)
                };
                let Some(side) = side else { continue };
                let id = create_position_id(&symbol, Some(side), Some(mode));
                let existing = positions
                    .iter()
                    .find(|p| p.id == id)
                    .or_else(|| {
                        positions
                            .iter()
                            .find(|p| p.symbol == symbol && p.side == side)
                    })
                    .or_else(|| {
                        if mode == PositionMode::OneWay {
                            for_symbol.first().copied()
                        } else {
                            None
                        }
                    })
                    .cloned();
                drop(for_symbol);
                drop(positions);
                let contracts = raw.abs();
                if contracts <= 0.0 && existing.is_none() {
                    continue;
                }
                let scalar = |keys: &[&str], old: Option<f64>| -> Result<f64, ExchangeError> {
                    if let Some(v) = first(&update, keys).filter(|v| !v.is_nullish()) {
                        float(Some(v))
                    } else {
                        Ok(old.unwrap_or(0.0))
                    }
                };
                let entry = finite_or_zero(scalar(
                    &["ep", "avgPrice"],
                    existing.as_ref().map(|p| p.entry_price),
                )?);
                let upnl = finite_or_zero(scalar(
                    &["up", "unrealizedProfit"],
                    existing.as_ref().map(|p| p.unrealized_pnl),
                )?);
                let liq = finite_or_zero(scalar(
                    &["liqp", "liquidationPrice"],
                    existing.as_ref().map(|p| p.liquidation_price),
                )?);
                let margin = update
                    .get("mt")
                    .and_then(Input::as_text)
                    .and_then(|v| String::from_utf16(v).ok())
                    .as_deref()
                    .map(|m| {
                        if m.to_lowercase() == "isolated" {
                            MarginType::Isolated
                        } else {
                            MarginType::Cross
                        }
                    })
                    .unwrap_or_else(|| {
                        existing
                            .as_ref()
                            .and_then(|p| (p.margin_type).into_value())
                            .unwrap_or(MarginType::Cross)
                    });
                let position = Position {
                    id: existing.as_ref().map_or(id, |p| p.id.clone()),
                    symbol,
                    side: side.into(),
                    entry_price: entry,
                    notional: contracts * entry + upnl,
                    leverage: existing.as_ref().map_or(1.0, |p| p.leverage),
                    margin_type: (Some(margin)).into(),
                    position_mode: (Some(
                        existing
                            .as_ref()
                            .and_then(|p| (p.position_mode).into_value())
                            .unwrap_or(mode),
                    ))
                    .into(),
                    unrealized_pnl: upnl,
                    realized_pnl: (None).into(),
                    contracts,
                    liquidation_price: liq,
                    margin_size: (None).into(),
                    info: (Some(update)).into(),
                    extra: (None).into(),
                    last_update: (None).into(),
                    option: (None).into(),
                };
                self.parent
                    .base
                    .store
                    .add_or_update_position(position)
                    .map_err(ExchangeError::from)?;
            }
        }
        Ok(())
    }
}
