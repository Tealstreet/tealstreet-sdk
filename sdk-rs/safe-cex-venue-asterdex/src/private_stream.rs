//! AsterDEX private websocket assembly and immediate Store mutations.
//! HTTP, event emission and the virtual clock remain host seams; connection,
//! dispatch, listen-key timers and repair timers are actual SDK components.
use crate::{
    account_helpers as account,
    listen_key::{ListenKeyLifecycle, ListenKeyParent},
    private_repairs::{PrivateRepairs, RepairParent},
    ws_helpers,
};
use safe_cex_exchange::js_runtime::value::{Js, from_retained_input, to_typed};
use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture, LogSeverity},
    websocket::{BaseWebSocket, WsParent, WsProtocol, WsRuntime},
};
use safe_cex_num::TableHit;
use safe_cex_store::{StoreUpdate, model_patch::ModelPatch};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{Balance, Order, OrderSide, OrderStatus, Position, PositionSide};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

/// Venue-local host contract. It does not extend any shared runtime trait.
pub trait PrivateStreamParent: WsParent + RepairParent {
    fn owner(&self) -> Rc<BaseExchange>;
    fn get_fapi_path(&self, endpoint: &str) -> String;
    fn post_listen_key(&self, path: String) -> ExchangeFuture<'static, String>;
    fn put_listen_key(&self, path: String, key: String) -> ExchangeFuture<'static, ()>;
    fn virtual_time_ms(&self) -> f64;
    fn mark_position_state_updated(&self);
    fn mark_balance_state_updated(&self) -> f64;
    fn emit_order(&self, order: &Order) -> Result<(), ExchangeError>;
    fn emit_fill(&self, fill: &PrivateFill) -> Result<(), ExchangeError>;
    fn emit_positions(&self, positions: &[Position]) -> Result<(), ExchangeError>;
    fn emit_account(&self, data: &Js) -> Result<(), ExchangeError>;
}
#[derive(Debug, Clone, PartialEq)]
pub struct PrivateFill {
    pub id: Option<String>,
    pub order_id: Option<String>,
    pub symbol: Option<String>,
    pub side: OrderSide,
    pub price: f64,
    pub amount: f64,
}
// ts: AsterDEXPrivateWebsocket
pub struct PrivateStream {
    pub socket: Rc<BaseWebSocket>,
    pub listen_key: Rc<ListenKeyLifecycle>,
    pub repairs: Rc<PrivateRepairs>,
    parent: Rc<dyn PrivateStreamParent>,
    runtime: Rc<dyn WsRuntime>,
    last_account_event_time: Cell<f64>,
}
struct Protocol(Weak<PrivateStream>);
struct ListenParent {
    stream: Weak<PrivateStream>,
    parent: Rc<dyn PrivateStreamParent>,
}
impl ListenKeyParent for ListenParent {
    fn get_fapi_path(&self, endpoint: &str) -> String {
        self.parent.get_fapi_path(endpoint)
    }
    fn post_listen_key(&self, path: String) -> ExchangeFuture<'static, String> {
        self.parent.post_listen_key(path)
    }
    fn put_listen_key(&self, path: String, key: String) -> ExchangeFuture<'static, ()> {
        self.parent.put_listen_key(path, key)
    }
    fn reconnect(&self) -> Result<(), ExchangeError> {
        self.stream
            .upgrade()
            .map_or(Ok(()), |s| s.socket.reconnect())
    }
    fn connect_and_subscribe(&self) -> ExchangeFuture<'static, ()> {
        self.stream.upgrade().map_or_else(
            || Box::pin(async { Ok(()) }) as ExchangeFuture<'static, ()>,
            |s| s.connect_and_subscribe(),
        )
    }
}
impl WsProtocol for Protocol {
    fn ping(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        ws_helpers::private_ping(socket)
    }
    fn connect_and_subscribe(
        &self,
        socket: Rc<BaseWebSocket>,
        _: bool,
    ) -> Result<(), ExchangeError> {
        if let Some(stream) = self.0.upgrade() {
            socket.start_message_operation(stream.connect_and_subscribe());
        }
        Ok(())
    }
    fn on_open(&self, _: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let Some(stream) = self.0.upgrade() else {
            return Box::pin(async { Ok(()) });
        };
        let result = (|| {
            if stream.parent.is_disposed() {
                return Ok(());
            }
            stream.listen_key.reset_connect_retry_count();
            stream
                .parent
                .log("AsterDEX private WebSocket connected", LogSeverity::Info)?;
            stream.socket.ping()?;
            if stream.runtime.enable_reconnecting() {
                let work = stream.parent.fetch_and_set_private_snapshot(10);
                stream.socket.start_message_operation(work);
            }
            Ok(())
        })();
        Box::pin(async move { result })
    }
    fn on_message(&self, _: &Rc<BaseWebSocket>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        self.0.upgrade().map_or(Ok(()), |s| s.on_message(frame))
    }
}
fn coercion(error: safe_cex_num::JsCoercionError) -> ExchangeError {
    let mut e = ExchangeError::new(error.message);
    e.name = error.name.into();
    e
}
fn text(value: &Js) -> Result<String, ExchangeError> {
    Ok(String::from_utf16_lossy(&value.to_js_string()?))
}
fn utf8(value: Js) -> Option<String> {
    if let Js::String(units) = value {
        Some(String::from_utf16_lossy(&units))
    } else {
        None
    }
}
fn optional_id(value: &Js) -> Result<Option<String>, ExchangeError> {
    if value.is_nullish() {
        Ok(None)
    } else {
        let method = value.get("toString")?;
        if !method.is_callable() {
            return Err(ExchangeError::new("data.i?.toString is not a function"));
        }
        Ok(Some(text(&method.call(value.clone(), vec![])?)?))
    }
}
fn parsed(value: &Js) -> Result<f64, ExchangeError> {
    Ok(if value.truthy() {
        value.parse_float()?
    } else {
        0.
    })
}
fn stringify(value: &Js) -> Result<String, ExchangeError> {
    let units = safe_cex_exchange::js_runtime::globals::stringify_json(value)?.unwrap_or_default();
    Ok(String::from_utf16_lossy(&units))
}
fn or_zero(value: f64) -> f64 {
    if value == 0.0 || value.is_nan() {
        0.0
    } else {
        value
    }
}
fn status(value: &Js) -> Result<OrderStatus, ExchangeError> {
    match crate::exchange_helpers::reverse_map_order_status(&text(value)?) {
        TableHit::Mapped(status) => Ok(status),
        TableHit::Inherited(_) => Err(ExchangeError::new(
            "Inherited order status cannot enter native OrderStatus",
        )),
        TableHit::Missing => unreachable!(),
    }
}
impl PrivateStream {
    // ts: AsterDEXPrivateWebsocket.constructor
    pub fn new(
        id: u64,
        parent: Rc<dyn PrivateStreamParent>,
        runtime: Rc<dyn WsRuntime>,
    ) -> Rc<Self> {
        let stream: Rc<Self> = Rc::new_cyclic(|weak| {
            let socket = BaseWebSocket::new(
                id,
                parent.clone(),
                runtime.clone(),
                Rc::new(Protocol(weak.clone())),
            );
            let closing = weak.clone();
            socket.set_before_close(Rc::new(move || {
                if let Some(s) = closing.upgrade() {
                    s.clear_close_timers();
                }
            }));
            Self {
                socket,
                listen_key: ListenKeyLifecycle::new(
                    parent.owner(),
                    Rc::new(ListenParent {
                        stream: weak.clone(),
                        parent: parent.clone(),
                    }),
                ),
                repairs: PrivateRepairs::new(parent.owner(), parent.clone()),
                parent: parent.clone(),
                runtime: runtime.clone(),
                last_account_event_time: Cell::new(0.0),
            }
        });
        for key in [
            "ORDER_TRADE_UPDATE",
            "ACCOUNT_UPDATE",
            "listenKeyExpired",
            "executionReport",
            "outboundAccountPosition",
        ] {
            let weak = Rc::downgrade(&stream);
            stream.socket.message_handlers.borrow_mut().insert(
                key.into(),
                Rc::new(RefCell::new(vec![Rc::new(move |data: &Js| {
                    let Some(s) = weak.upgrade() else {
                        return Ok(());
                    };
                    match key {
                        "ORDER_TRADE_UPDATE" => s.handle_order_trade_update(data),
                        "ACCOUNT_UPDATE" => s.handle_account_position_update(data),
                        "listenKeyExpired" => {
                            s.parent.log("AsterDEX private listen key expired event received, reconnecting private stream", LogSeverity::Warning)?;
                            s.listen_key.clear_listen_key_refresh_timer();
                            s.listen_key.clear_listen_key_watchdog_timer();
                            s.socket.reconnect()
                        },
                        "executionReport" => s.handle_execution_report(data),
                        "outboundAccountPosition" => s.handle_account_update(data),
                        _ => unreachable!(),
                    }
                })])),
            );
        }
        stream
    }
    pub fn last_account_event_time(&self) -> f64 {
        self.last_account_event_time.get()
    }
    fn clear_close_timers(&self) {
        self.listen_key.clear_listen_key_refresh_timer();
        self.listen_key.clear_listen_key_watchdog_timer();
        self.repairs.clear_position_repair_timer();
        self.repairs.clear_balance_reconcile_timer();
        self.repairs.clear_order_reconcile_timer();
    }
    // ts: AsterDEXPrivateWebsocket.connectAndSubscribe
    pub fn connect_and_subscribe(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        if self.parent.is_disposed() {
            return Box::pin(async { Ok(()) });
        }
        let fetching = self.listen_key.fetch_listen_key();
        let s = self.clone();
        Box::pin(async move {
            let result = async {
                let key = fetching.await?;
                if s.parent.is_disposed() {
                    return Ok(());
                }
                s.listen_key.clear_connect_retry_timer();
                let endpoint = if s.parent.options().testnet.unwrap_or(false) {
                    "WS.PRIVATE.TESTNET"
                } else {
                    "WS.PRIVATE.PRODUCTION"
                };
                let stream_url = crate::tables::ASTERDEX_ENDPOINTS
                    .iter()
                    .find(|(k, _)| *k == endpoint)
                    .unwrap()
                    .1;
                let url = format!("{}/ws/{key}", stream_url.replacen("/stream", "", 1));
                s.parent.log(
                    &format!("Connecting to AsterDEX private WebSocket: {url}"),
                    LogSeverity::Info,
                )?;
                s.socket.create_websocket(url, vec![], None).await
            }
            .await;
            if let Err(error) = result {
                let message = if error.message.is_empty() {
                    "Unknown error"
                } else {
                    &error.message
                };
                s.parent.log(
                    &format!("Failed to initialize AsterDEX private WebSocket: {message}"),
                    LogSeverity::Warning,
                )?;
                s.listen_key.schedule_connect_retry()?;
            }
            Ok(())
        })
    }
    // ts: AsterDEXPrivateWebsocket.onMessage
    pub fn on_message(self: &Rc<Self>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        if self.parent.is_disposed() {
            return Ok(());
        }
        let _realm = self.socket.prototype_realm.bind();
        self.socket.track_last_received()?;
        let raw = match frame {
            SocketFrame::Text(s) => s.clone(),
            SocketFrame::Binary(b) => String::from_utf8_lossy(b).into_owned(),
            SocketFrame::Close { .. } => return Ok(()),
        };
        let result = (|| {
            let captured = safe_cex_types::retained_json::RetainedJsonInput::parse_json(
                &raw.encode_utf16().collect::<Vec<_>>(),
            );
            let Ok(captured) = captured else {
                return Ok(());
            };
            let data = from_retained_input(captured);
            if !data.truthy() {
                return Ok(());
            }
            if data.get("id")?.strict_equals(&Js::Number(42.)) {
                return self.socket.handle_pong_event(5_000);
            }
            let event = data.get("e")?;
            let key = text(&event)?;
            if event.truthy() && self.socket.message_handlers.borrow().contains_key(&key) {
                self.socket.record_message_handler_stats(&key)?;
                let callbacks = self
                    .socket
                    .message_handlers
                    .borrow()
                    .get(&key)
                    .cloned()
                    .expect("own handler list");
                // Array.forEach captures the original length, while later index
                // reads see mutations to the same array. Never borrow across a callback.
                let length = callbacks.borrow().len();
                for index in 0..length {
                    let callback = callbacks.borrow().get(index).cloned();
                    if let Some(callback) = callback {
                        callback(&data)?;
                    }
                }
            } else if event.truthy()
                && matches!(
                    safe_cex_num::js_table_lookup::<&str, ()>(&[], &key),
                    TableHit::Inherited(_)
                )
            {
                self.socket.record_message_handler_stats(&key)?;
                return Err(ExchangeError::new(
                    "this.messageHandlers[eventType].forEach is not a function",
                ));
            } else {
                // JS substring counts UTF-16 units. Rust replaces a lone cutoff
                // surrogate because its String cannot contain one (manifest FORCED).
                let units: Vec<u16> = stringify(&data)?.encode_utf16().take(200).collect();
                let summary = String::from_utf16_lossy(&units);
                if event.truthy() {
                    self.parent.log(
                        &format!("Unhandled private event type: {key}, data: {summary}"),
                        LogSeverity::Warning,
                    )?;
                } else {
                    self.parent.log(
                        &format!("Private WS message with no event type: {summary}"),
                        LogSeverity::Debug,
                    )?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.parent.log(
                &format!("Failed to process private message: {}", error.message),
                LogSeverity::Error,
            )?;
        }
        Ok(())
    }
    fn order(&self, id: Option<&str>) -> Result<Option<Order>, ExchangeError> {
        Ok(self
            .parent
            .store()
            .orders()?
            .iter()
            .find(|o| Some(o.id.as_str()) == id)
            .cloned())
    }
    fn update_order(
        &self,
        mut order: Order,
        status: OrderStatus,
        filled: Option<f64>,
        remaining: Option<f64>,
        time: Option<f64>,
        emit: bool,
    ) -> Result<(), ExchangeError> {
        let mut patch = ModelPatch::default();
        order.status = (status).into();
        patch.set_json("status", serde_json::to_value(status).unwrap());
        if let Some(v) = filled {
            order.filled = v;
            patch.set_number("filled", v);
        }
        if let Some(v) = remaining {
            order.remaining = v;
            patch.set_number("remaining", v);
        }
        if let Some(v) = time {
            order.last_update = v;
            patch.set_number("lastUpdate", v);
        }
        self.parent.store().update_order(&order.id, &patch)?;
        if emit {
            self.parent.emit_order(&order)?;
        }
        Ok(())
    }
    fn handle_execution_report(&self, data: &Js) -> Result<(), ExchangeError> {
        self.repairs.schedule_order_reconcile()?;
        if let Some(order) = self.order(optional_id(&data.get("i")?)?.as_deref())? {
            self.update_order(
                order,
                status(&data.get("X")?)?,
                Some(parsed(&data.get("z")?)?),
                Some(parsed(&data.get("q")?)? - parsed(&data.get("z")?)?),
                None,
                true,
            )?;
        }
        Ok(())
    }
    // ts: AsterDEXPrivateWebsocket#handleOrderTradeUpdate
    pub fn handle_order_trade_update(&self, data: &Js) -> Result<(), ExchangeError> {
        let d = data.get("o")?;
        if !d.truthy() {
            return Ok(());
        };
        self.repairs.schedule_order_reconcile()?;
        let id = optional_id(&d.get("i")?)?;
        let state = utf8(d.get("X")?);
        let state = state.as_deref();
        if state == Some("NEW") {
            let kind = match crate::exchange_helpers::reverse_map_order_type(&text(&d.get("o")?)?) {
                TableHit::Mapped(k) => k,
                _ => {
                    return Err(ExchangeError::new(
                        "Inherited order type cannot enter native OrderType",
                    ));
                }
            };
            let kind = serde_json::from_value(Value::String(kind)).map_err(|_| {
                ExchangeError::new("Unknown order type cannot enter native OrderType")
            })?;
            let price = parsed(&d.get("p")?)?;
            let amount = parsed(&d.get("q")?)?;
            let filled = parsed(&d.get("z")?)?;
            let order = Order {
                client_order_id: (None).into(),
                id: id.ok_or_else(|| {
                    ExchangeError::new("Missing private order id cannot enter native Order")
                })?,
                parent_id: (None).into(),
                status: safe_cex_types::ModelField::Value(OrderStatus::Open),
                symbol: utf8(d.get("s")?)
                    .ok_or_else(|| ExchangeError::new("Missing private order symbol"))?
                    .into(),
                r#type: kind,
                side: if d.get("S")?.strict_equals(&Js::str("BUY")) {
                    safe_cex_types::ModelField::Value(OrderSide::Buy)
                } else {
                    safe_cex_types::ModelField::Value(OrderSide::Sell)
                },
                price: if price == 0.0 || price.is_nan() {
                    (parsed(&d.get("sp")?)?).into()
                } else {
                    (price).into()
                },
                amount,
                filled,
                remaining: amount - filled,
                reduce_only: (d.get("R")?.truthy() || d.get("cp")?.truthy()).into(),
                last_update: self.parent.virtual_time_ms(),
                info: (Some(to_typed(&d)?)).into(),
                close: (None).into(),
                time_in_force: (None).into(),
                trigger_type: (None).into(),
                tp_trigger_type: (None).into(),
                sl_trigger_type: (None).into(),
                take_profit: (None).into(),
                stop_loss: (None).into(),
                extra: (None).into(),
            };
            self.parent.store().add_or_update_order(order)?;
            self.parent.log(
                &format!(
                    "Order added to store: {} {} {} {}",
                    text(&d.get("i")?)?,
                    text(&d.get("s")?)?,
                    text(&d.get("o")?)?,
                    text(&d.get("S")?)?
                ),
                LogSeverity::Info,
            )?;
            return Ok(());
        }
        if matches!(state, Some("PARTIALLY_FILLED" | "FILLED")) {
            self.mirror_cumulative_order_fill(&d)?;
            let amount = parsed(&d.get("l")?)?;
            let price = parsed(&d.get("L")?)?;
            if amount > 0.0 {
                self.parent.emit_fill(&PrivateFill {
                    id: id.clone(),
                    order_id: id.clone(),
                    symbol: utf8(d.get("s")?),
                    side: if d.get("S")?.strict_equals(&Js::str("BUY")) {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    },
                    price: if price > 0.0 {
                        price
                    } else {
                        parsed(&d.get("ap")?)?
                    },
                    amount,
                })?;
            }
        }
        let Some(order) = self.order(id.as_deref())? else {
            return Ok(());
        };
        let time = account::get_order_update_time(&d, || self.parent.virtual_time_ms())?;
        match state {
            Some("FILLED") => self.update_order(
                order,
                OrderStatus::Closed,
                Some(parsed(&d.get("z")?)?),
                Some(0.0),
                Some(time),
                true,
            ),
            Some("CANCELED" | "REJECTED" | "EXPIRED") => {
                self.update_order(order, OrderStatus::Canceled, None, None, Some(time), true)
            }
            _ => self.update_order(
                order,
                status(&d.get("X")?)?,
                Some(parsed(&d.get("z")?)?),
                Some(parsed(&d.get("q")?)? - parsed(&d.get("z")?)?),
                Some(time),
                true,
            ),
        }
    }
    // ts: AsterDEXPrivateWebsocket#mirrorCumulativeOrderFill
    fn mirror_cumulative_order_fill(&self, d: &Js) -> Result<(), ExchangeError> {
        if let Some(fill) =
            account::mirror_cumulative_order_fill(d, || self.parent.virtual_time_ms())?
        {
            let mut patch = ModelPatch::default();
            patch.set_json("status", serde_json::to_value(fill.status).unwrap());
            patch.set_number("filled", fill.filled);
            patch.set_number("remaining", fill.remaining);
            patch.set_number("lastUpdate", fill.last_update);
            self.parent.store().update_order(&fill.order_id, &patch)?;
        }
        Ok(())
    }
    // ts: AsterDEXPrivateWebsocket#applyBalanceUpdates
    pub fn apply_balance_updates(
        &self,
        updates: &[account::AssetBalanceUpdate],
        deltas: &[(String, f64)],
    ) -> Result<bool, ExchangeError> {
        let current = self.parent.store().balance()?.clone();
        let mut currencies = current.currencies.clone().unwrap_or_default();
        let delta = |asset: &str| {
            or_zero(
                deltas
                    .iter()
                    .find(|(a, _)| a == asset)
                    .map_or(0.0, |(_, d)| *d),
            )
        };
        for update in updates {
            let previous = currencies
                .get(&update.asset)
                .map(|row| {
                    row.as_balance().ok_or_else(|| {
                        ExchangeError::new(
                            "AsterDEX balance update requires a declared Balance currency row",
                        )
                    })
                })
                .transpose()?;
            let mut balance = previous.cloned().unwrap_or(Balance {
                total: 0.0,
                free: 0.0,
                used: 0.0,
                upnl: 0.0,
                info: None,
                currencies: None,
                coins: None,
            });
            balance.total = update.total;
            balance.free = update.free;
            balance.used = update.used;
            balance.upnl = or_zero(previous.map_or(0.0, |b| b.upnl)) + delta(&update.asset);
            currencies.insert(update.asset.clone(), balance.into());
        }
        let reconcile =
            updates.iter().any(|u| u.asset != "USDT") || currencies.keys().any(|a| a != "USDT");
        let mut next = current.clone();
        if !reconcile {
            if let Some(collateral) = updates.iter().find(|u| u.asset == "USDT") {
                if let Some(previous) = current
                    .currencies
                    .as_ref()
                    .and_then(|c| c.get("USDT"))
                    .map(|row| {
                        row.as_balance().ok_or_else(|| {
                            ExchangeError::new(
                                "AsterDEX balance update requires a declared Balance currency row",
                            )
                        })
                    })
                    .transpose()?
                {
                    next.total = current.total + collateral.total - previous.total;
                    next.free = current.free + collateral.free - previous.free;
                    next.used = current.used + collateral.used - previous.used;
                } else {
                    next.total = collateral.total;
                    next.free = collateral.free;
                    next.used = collateral.used;
                }
            }
            next.upnl = current.upnl + delta("USDT");
        }
        next.currencies = Some(currencies);
        if [next.total, next.free, next.used, next.upnl]
            .iter()
            .all(|n| n.is_finite())
        {
            self.parent.store().update(
                StoreUpdate {
                    balance: Some(next),
                    ..Default::default()
                },
                false,
            )?;
        }
        Ok(reconcile)
    }
    pub fn handle_account_update(&self, data: &Js) -> Result<(), ExchangeError> {
        if matches!(data, Js::Null) {
            return Err(coercion(safe_cex_num::JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of null (reading 'B')",
            }));
        }
        if data.get("B")?.truthy() {
            if let Some(balances) =
                account::parse_asset_balance_updates(&data.get("B")?, None, "f", Some("l"))?
            {
                self.apply_balance_updates(&balances, &[])?;
            }
        }
        self.parent.emit_account(data)
    }
    // ts: AsterDEXPrivateWebsocket#handleAccountPositionUpdate
    pub fn handle_account_position_update(&self, data: &Js) -> Result<(), ExchangeError> {
        let time = data.get("E")?;
        let time = if let Js::Number(value) = time {
            Some(value)
        } else {
            None
        };
        let Some(time) = time.filter(|v| {
            v.is_finite() && v.fract() == 0. && *v > 0. && *v <= 9_007_199_254_740_991.
        }) else {
            return self.parent.log(
                "Ignoring ACCOUNT_UPDATE without a valid numeric event time E",
                LogSeverity::Warning,
            );
        };
        if time <= self.last_account_event_time.get() {
            return self.parent.log(
                "Ignoring stale ACCOUNT_UPDATE ordered before the current event time E",
                LogSeverity::Debug,
            );
        }
        let a = data.get("a")?;
        if !matches!(a, Js::Object(_) | Js::Array(_)) {
            return self.parent.log(
                "Ignoring malformed ACCOUNT_UPDATE payload",
                LogSeverity::Warning,
            );
        };
        let store = self.parent.store();
        let positions = store.positions()?.to_vec();
        let balance = store.balance()?.clone();
        let currencies: Vec<String> = balance
            .currencies
            .as_ref()
            .map(|c| c.keys().cloned().collect())
            .unwrap_or_default();
        let deltas =
            account::get_account_update_upnl_deltas(&a.get("P")?, &positions, &currencies)?;
        let balances = if matches!(a.get("B")?, Js::Undefined) {
            Some(vec![])
        } else {
            account::parse_asset_balance_updates(&a.get("B")?, Some("wb"), "cw", None)?
        };
        let (Some(deltas), Some(balances)) = (deltas, balances) else {
            return self.parent.log(
                "Ignoring malformed ACCOUNT_UPDATE balance or position values",
                LogSeverity::Warning,
            );
        };
        self.last_account_event_time.set(time);
        let version = self.parent.mark_balance_state_updated();
        let p = a.get("P")?;
        if p.truthy() {
            self.parent.mark_position_state_updated();
            let Js::Array(positions) = &p else {
                unreachable!("validated positions")
            };
            self.parent.log(
                &format!(
                    "ACCOUNT_UPDATE received with {} position(s): {}",
                    positions.borrow().len(),
                    stringify(&p)?
                ),
                LogSeverity::Info,
            )?;
            let length = positions.borrow().len();
            for index in 0..length {
                let pos = positions.borrow().get(index).cloned().flatten();
                let Some(pos) = pos else {
                    continue;
                };
                let amount = parsed(&pos.get("pa")?)?;
                let side = account::get_position_side(&pos, amount)?;
                let symbol = utf8(pos.get("s")?).unwrap_or_default();
                let existing: Vec<Position> = store.positions()?.to_vec();
                if amount != 0.0 {
                    if let Some(current) = existing
                        .iter()
                        .find(|p| p.symbol == symbol && p.side == side)
                    {
                        self.parent.log(
                            &format!(
                                "Updating existing position: {symbol} {} with contracts={}",
                                serde_json::to_value(side).unwrap().as_str().unwrap(),
                                safe_cex_num::js_number_string(amount.abs())
                            ),
                            LogSeverity::Info,
                        )?;
                        let mut patch = ModelPatch::default();
                        patch.set_number("contracts", amount.abs());
                        let entry = parsed(&pos.get("ep")?)?;
                        patch.set_number("entryPrice", entry);
                        patch.set_number("notional", amount.abs() * entry);
                        patch.set_number("unrealizedPnl", parsed(&pos.get("up")?)?);
                        patch.set_number("realizedPnl", parsed(&pos.get("cr")?)?);
                        store.update_position(&current.id, &patch)?;
                    } else {
                        let side_str = serde_json::to_value(side).unwrap();
                        self.parent.log(&format!("Position not found in store: {symbol} {}, will be created by fetchPositions()",side_str.as_str().unwrap()),LogSeverity::Info)?;
                        if pos.get("ps")?.strict_equals(&Js::str("BOTH")) {
                            let opposite = if side == PositionSide::Long {
                                PositionSide::Short
                            } else {
                                PositionSide::Long
                            };
                            let existing = store.positions()?.to_vec();
                            if let Some(old) = existing
                                .iter()
                                .find(|p| p.symbol == symbol && p.side == opposite)
                            {
                                self.parent.log(
                                    &format!(
                                        "Removing opposite side position (flipped): {symbol} {}",
                                        serde_json::to_value(opposite).unwrap().as_str().unwrap()
                                    ),
                                    LogSeverity::Info,
                                )?;
                                let next_positions = store
                                    .positions()?
                                    .iter()
                                    .filter(|p| p.id != old.id)
                                    .cloned()
                                    .collect();
                                store.update(
                                    StoreUpdate {
                                        positions: Some(next_positions),
                                        ..Default::default()
                                    },
                                    false,
                                )?;
                            }
                        }
                    }
                } else {
                    let count = existing.iter().filter(|p| p.symbol == symbol).count();
                    if count > 0 {
                        self.parent.log(
                            &format!("Removing {count} closed position(s): {symbol}"),
                            LogSeverity::Info,
                        )?;
                        let next_positions = store
                            .positions()?
                            .iter()
                            .filter(|p| p.symbol != symbol)
                            .cloned()
                            .collect();
                        store.update(
                            StoreUpdate {
                                positions: Some(next_positions),
                                ..Default::default()
                            },
                            false,
                        )?;
                    }
                }
            }
            let emitted_positions = store.positions()?.to_vec();
            self.parent.emit_positions(&emitted_positions)?;
            self.repairs.schedule_position_repair()?;
        }
        if self.apply_balance_updates(&balances, &deltas)? {
            self.repairs.schedule_balance_reconcile(version)?;
        }
        Ok(())
    }
    // ts: AsterDEXPrivateWebsocket.dispose
    pub fn dispose(&self) {
        self.listen_key.reset_for_dispose();
        self.repairs.clear_position_repair_timer();
        self.repairs.clear_balance_reconcile_timer();
        self.repairs.clear_order_reconcile_timer();
        self.socket.dispose();
    }
}
