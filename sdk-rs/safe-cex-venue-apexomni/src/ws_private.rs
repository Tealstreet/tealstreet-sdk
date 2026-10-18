//! Private login, keepalive, reconciliation and account-frame handlers.
use crate::{
    data::{get, truthy},
    exchange::{ApexOmniExchange, failure, float, numeric, text},
    protocol,
    socket_host::{ApexSocketHost, SocketListener, endpoint, object, property, send},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue as J, js_number_string, math_max};
use safe_cex_store::model_patch::ModelPatch;
use safe_cex_types::OrderStatus;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

pub struct ApexOmniPrivateWebsocket {
    pub parent: Rc<ApexOmniExchange>,
    pub host: Rc<dyn ApexSocketHost>,
    pub last_connect_time: Cell<f64>,
    orders_timer: Cell<Option<u64>>,
    positions_timer: Cell<Option<u64>>,
    cumulative_filled: RefCell<HashMap<String, f64>>,
    cumulative_fees: RefCell<HashMap<String, f64>>,
}
impl ApexOmniPrivateWebsocket {
    pub fn new(
        parent: Rc<ApexOmniExchange>,
        host: Rc<dyn ApexSocketHost>,
    ) -> Result<Rc<Self>, ExchangeError> {
        host.initialize(true, true)?;
        Ok(Rc::new(Self {
            parent,
            host,
            last_connect_time: Cell::new(0.),
            orders_timer: Cell::new(None),
            positions_timer: Cell::new(None),
            cumulative_filled: RefCell::new(HashMap::new()),
            cumulative_fees: RefCell::new(HashMap::new()),
        }))
    }
    // ts: ApexOmniPrivateWebsocket.throttleFetchOrders
    pub fn throttle_fetch_orders(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if let Some(timer) = self.orders_timer.get() {
            self.host.clear_timeout(timer)?;
        }
        let weak = Rc::downgrade(self);
        let timer = self.host.set_timeout(
            1000,
            Rc::new(move || {
                let owner = weak.upgrade();
                Box::pin(async move {
                    let owner =
                        owner.ok_or_else(|| failure("private socket owner released".into()))?;
                    owner.orders_timer.set(None);
                    if !owner.host.is_disposed() {
                        match owner.parent.fetch_orders().await {
                            Ok(orders) => {
                                if owner.host.is_disposed() || owner.parent.base.is_disposed() {
                                    return Ok(());
                                }
                                owner
                                    .parent
                                    .base
                                    .store
                                    .add_or_update_orders_full(orders)
                                    .map_err(|e| failure(e.to_string()))?;
                            }
                            Err(error) => owner.parent.base.log(
                                &format!(
                                    "Error fetching orders: {}",
                                    if error.message.is_empty() {
                                        error.js_error_string()
                                    } else {
                                        error.message
                                    }
                                ),
                                "warning",
                            )?,
                        }
                    }
                    Ok(())
                })
            }),
        )?;
        self.orders_timer.set(Some(timer));
        Ok(())
    }
    // ts: ApexOmniPrivateWebsocket.throttleFetchPositions
    pub fn throttle_fetch_positions(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if let Some(timer) = self.positions_timer.get() {
            self.host.clear_timeout(timer)?;
        }
        let weak = Rc::downgrade(self);
        let timer = self.host.set_timeout(
            1000,
            Rc::new(move || {
                let owner = weak.upgrade();
                Box::pin(async move {
                    let owner =
                        owner.ok_or_else(|| failure("private socket owner released".into()))?;
                    owner.positions_timer.set(None);
                    if !owner.host.is_disposed() {
                        match owner.parent.fetch_positions().await {
                            Ok(positions) => {
                                if owner.host.is_disposed() || owner.parent.base.is_disposed() {
                                    return Ok(());
                                }
                                owner
                                    .parent
                                    .base
                                    .store
                                    .add_or_update_positions_full(positions)
                                    .map_err(|e| failure(e.to_string()))?;
                            }
                            Err(error) => owner.parent.base.log(
                                &format!(
                                    "Error fetching positions: {}",
                                    if error.message.is_empty() {
                                        error.js_error_string()
                                    } else {
                                        error.message
                                    }
                                ),
                                "warning",
                            )?,
                        }
                    }
                    Ok(())
                })
            }),
        )?;
        self.positions_timer.set(Some(timer));
        Ok(())
    }
    // ts: ApexOmniPrivateWebsocket.connectAndSubscribe
    pub fn connect_and_subscribe(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        let default = protocol::BASE_WS_URL
            [2 + usize::from(self.parent.base.options().testnet == Some(true))]
        .1;
        let url = endpoint(default, self.host.now_ms(), &self.host.nanoid());
        let route = self
            .host
            .private_route(&self.parent.base.name, &self.parent.base.options())?;
        self.host.create_socket(route.as_deref().unwrap_or(&url))?;
        let weak = Rc::downgrade(self);
        self.host
            .add_listener(SocketListener::Open(Rc::new(move || {
                let owner = weak.upgrade();
                Box::pin(async move {
                    owner
                        .ok_or_else(|| failure("private socket owner released".into()))?
                        .on_open()
                        .await
                })
            })))?;
        let weak = Rc::downgrade(self);
        self.host
            .add_listener(SocketListener::Message(Rc::new(move |raw| {
                weak.upgrade()
                    .ok_or_else(|| failure("private socket owner released".into()))?
                    .on_message(raw)
            })))?;
        self.host.add_listener(SocketListener::Close)
    }
    // ts: ApexOmniPrivateWebsocket.onOpen
    pub async fn on_open(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        let authed = match self.auth().await {
            Ok(result) => result,
            Err(error) => {
                self.host.report_error(error)?;
                false
            }
        };
        if !authed {
            self.host.reconnect()?;
            return Ok(());
        }
        self.last_connect_time.set(self.host.now_ms());
        self.subscribe()?;
        self.ping()?;
        if self.host.enable_reconnecting() {
            let parent = self.parent.clone();
            self.host.spawn(Box::pin(async move {
                parent.fetch_and_set_private_snapshot().await
            }))?;
        }
        Ok(())
    }
    // ts: ApexOmniPrivateWebsocket.onMessage
    pub fn on_message(self: &Rc<Self>, raw: &str) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        self.host.track_last_received()?;
        self.host.diagnostic_receive(raw)?;
        let json = J::from(
            safe_cex_transport::how::json_parse(raw)
                .into_value()
                .map_err(|error| ExchangeError::new(error.to_string()))?,
        );
        if get(&json, "success") == &J::Bool(false) && truthy(get(&json, "ret_msg")) {
            self.parent.base.log(
                &format!(
                    "Private WS request rejected: {}",
                    text(get(&json, "ret_msg"))?
                ),
                "warning",
            )?;
        }
        if get(&json, "topic") == &J::String("ws_zk_accounts_v3".into()) {
            self.host.record_stats("ws_zk_accounts_v3")?;
            self.handle_ws_accounts_v2_topic(&json)?;
        }
        if get(&json, "op") == &J::String("ping".into())
            || !matches!(get(&json, "ping"), J::Undefined)
        {
            let args = match get(&json, "args") {
                J::Array(values) if !values.is_empty() => J::Array(values.clone()),
                _ => J::Array(vec![J::String(js_number_string(self.host.now_ms()))]),
            };
            send(
                self.host.as_ref(),
                object([("op", J::String("pong".into())), ("args", args)]),
                false,
            )?;
        }
        if get(&json, "op") == &J::String("pong".into())
            || get(&json, "ret_msg") == &J::String("pong".into())
            || !matches!(get(&json, "pong"), J::Undefined)
        {
            self.host.handle_pong(Some(5000))?;
        }
        Ok(())
    }
    // ts: ApexOmniPrivateWebsocket.ping
    pub fn ping(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        self.host.set_ping_at(self.host.monotonic_now_ms())?;
        send(
            self.host.as_ref(),
            object([("op", J::String("ping".into()))]),
            true,
        )?;
        self.host.schedule_ping(5000)
    }
    // ts: ApexOmniPrivateWebsocket.handleWsAccountsV2Topic
    pub fn handle_ws_accounts_v2_topic(self: &Rc<Self>, json: &J) -> Result<(), ExchangeError> {
        let contents = property(json, "contents")?;
        for key in ["fills", "orders", "positions"] {
            let data = property(contents, key)?;
            if truthy(data) {
                match key {
                    "fills" => self.handle_fill_topic(data)?,
                    "orders" => self.handle_order_topic(data)?,
                    _ => self.handle_position_topic(data)?,
                }
            }
        }
        Ok(())
    }
    // ts: ApexOmniPrivateWebsocket.handleFillTopic
    pub fn handle_fill_topic(&self, data: &J) -> Result<(), ExchangeError> {
        for fill in crate::data::array(data).map_err(failure)? {
            self.parent.record_ws_execution(fill)?;
        }
        Ok(())
    }
    // ts: ApexOmniPrivateWebsocket.handleOrderTopic
    pub fn handle_order_topic(self: &Rc<Self>, data: &J) -> Result<(), ExchangeError> {
        property(data, "some")?;
        let data = crate::data::array(data).map_err(|_| {
            let mut error = failure("data.some is not a function".into());
            error.name = "TypeError".into();
            error
        })?;
        let zero = J::String("0".into());
        let mut has_canceled = false;
        for row in data {
            if property(row, "status")? == &J::String("CANCELED".into()) {
                has_canceled = true;
                break;
            }
        }
        if !has_canceled {
            self.throttle_fetch_orders()?;
        }
        for row in data {
            let raw_id = property(row, "id")?;
            if let J::String(id) = raw_id
                && self
                    .parent
                    .recently_cancelled_orders
                    .borrow()
                    .contains_key(id)
            {
                continue;
            }
            let orders = self.parent.map_order(row)?;
            if orders.is_empty() {
                continue;
            }
            let id = text(raw_id)?;
            let price = float(get(row, "price"))?;
            let amount = float(get(row, "size"))?;
            let fresh = self.host.date_now_ms() - numeric(get(row, "updatedAt"))? <= 5000.;
            let notify = self.host.now_ms() - self.last_connect_time.get() > 5000. && fresh;
            let status = get(row, "status");
            let filled = status == &J::String("FILLED".into());
            let canceled = status == &J::String("CANCELED".into());
            if filled {
                if price <= 0. && amount <= 0. {
                    continue;
                }
                let cumulative = float(if truthy(get(row, "cumSuccessFillSize")) {
                    get(row, "cumSuccessFillSize")
                } else {
                    &zero
                })?;
                if cumulative.is_finite() && cumulative > 0. {
                    let previous = *self.cumulative_filled.borrow().get(&id).unwrap_or(&0.);
                    let delta = cumulative - previous;
                    if delta > 0. {
                        self.cumulative_filled
                            .borrow_mut()
                            .insert(id.clone(), cumulative);
                        let fee = float(if truthy(get(row, "cumSuccessFillFee")) {
                            get(row, "cumSuccessFillFee")
                        } else {
                            &zero
                        })?;
                        let previous_fee = *self.cumulative_fees.borrow().get(&id).unwrap_or(&0.);
                        let fee_delta = if fee.is_finite() {
                            math_max(fee - previous_fee, 0.)
                        } else {
                            0.
                        };
                        if fee.is_finite() {
                            self.cumulative_fees.borrow_mut().insert(id.clone(), fee);
                        }
                        let execution = object([
                            ("symbol", get(row, "symbol").clone()),
                            ("side", get(row, "side").clone()),
                            ("orderId", get(row, "id").clone()),
                            (
                                "price",
                                ["latestMatchFillPrice", "averagePrice", "price"]
                                    .into_iter()
                                    .map(|k| get(row, k))
                                    .find(|v| truthy(v))
                                    .unwrap_or(&J::Undefined)
                                    .clone(),
                            ),
                            ("size", J::String(js_number_string(delta))),
                            ("fee", J::String(js_number_string(fee_delta))),
                            ("token", get(row, "token").clone()),
                            ("feeCurrency", get(row, "feeCurrency").clone()),
                            (
                                "createdAt",
                                if truthy(get(row, "updatedAt")) {
                                    get(row, "updatedAt").clone()
                                } else {
                                    get(row, "createdAt").clone()
                                },
                            ),
                        ]);
                        self.parent.record_ws_execution(&execution)?;
                    } else if delta == 0. && previous == 0. {
                        self.cumulative_filled
                            .borrow_mut()
                            .insert(id.clone(), cumulative);
                        let fee = float(if truthy(get(row, "cumSuccessFillFee")) {
                            get(row, "cumSuccessFillFee")
                        } else {
                            &zero
                        })?;
                        if fee.is_finite() {
                            self.cumulative_fees.borrow_mut().insert(id.clone(), fee);
                        }
                    }
                }
            }
            if filled && notify {
                let order = &orders[0];
                self.parent.base.emit(
                    "fill",
                    safe_cex_exchange::js_runtime::value::Js::object(vec![
                        (
                            "id",
                            safe_cex_exchange::js_runtime::value::Js::str(&order.id),
                        ),
                        (
                            "side",
                            safe_cex_exchange::js_runtime::value::from_typed(&order.side)?,
                        ),
                        (
                            "symbol",
                            safe_cex_exchange::js_runtime::value::Js::str(&order.symbol),
                        ),
                        (
                            "price",
                            safe_cex_exchange::js_runtime::value::Js::Number(price),
                        ),
                        (
                            "amount",
                            safe_cex_exchange::js_runtime::value::Js::Number(amount),
                        ),
                    ]),
                )?;
            }
            if filled || canceled {
                self.cumulative_filled.borrow_mut().remove(&id);
                self.cumulative_fees.borrow_mut().remove(&id);
                let reason = get(row, "cancelReason");
                if !filled
                    && notify
                    && truthy(reason)
                    && ![
                        "UNKNOWN_ORDER_CANCEL_REASON",
                        "INTERNAL_FAILED",
                        "USER_CANCELED",
                    ]
                    .iter()
                    .any(|v| reason == &J::String((*v).into()))
                {
                    self.host.report_error(failure(text(reason)?))?;
                }
                if canceled {
                    let id = &orders[0].id;
                    let sl = format!("{id}__stop_loss");
                    let tp = format!("{id}__take_profit");
                    self.parent
                        .base
                        .store
                        .remove_orders(&[id.as_str(), sl.as_str(), tp.as_str()])
                        .map_err(|e| failure(e.to_string()))?;
                } else {
                    let closed = orders
                        .into_iter()
                        .map(|mut order| {
                            order.status = safe_cex_types::ModelField::Value(OrderStatus::Closed);
                            order
                        })
                        .collect();
                    self.parent
                        .base
                        .store
                        .add_or_update_orders(closed)
                        .map_err(|e| failure(e.to_string()))?;
                }
            } else if status == &J::String("OPEN".into())
                || status == &J::String("UNTRIGGERED".into())
            {
                self.parent
                    .base
                    .store
                    .add_or_update_orders(orders)
                    .map_err(|e| failure(e.to_string()))?;
            }
        }
        Ok(())
    }
    // ts: ApexOmniPrivateWebsocket.handlePositionTopic
    pub fn handle_position_topic(self: &Rc<Self>, data: &J) -> Result<(), ExchangeError> {
        let mut updates = Vec::new();
        for row in crate::data::array(data).map_err(failure)? {
            // TS binds the mapper to this socket, whose receiver has no `log`.
            // Preserve the mapper's evaluation order before the missing log call.
            if let Some(position) = self.parent.map_position_with_log(row, |_| {
                let mut error = failure("this.log is not a function".into());
                error.name = "TypeError".into();
                Err(error)
            })? {
                let serde_json::Value::Object(fields) =
                    serde_json::to_value(&position).map_err(|e| failure(e.to_string()))?
                else {
                    unreachable!()
                };
                let mut patch = ModelPatch::from(fields);
                for (key, value) in [
                    ("entryPrice", position.entry_price),
                    ("notional", position.notional),
                    ("contracts", position.contracts),
                    ("leverage", position.leverage),
                    ("liquidationPrice", position.liquidation_price),
                    ("unrealizedPnl", position.unrealized_pnl),
                ] {
                    patch.set_number(key, value);
                }
                if let Some(v) = position.realized_pnl.into_value() {
                    patch.set_number("realizedPnl", v);
                }
                if let Some(v) = position.last_update.into_value() {
                    patch.set_number("lastUpdate", v);
                }
                updates.push((Some(position.id), patch));
            }
        }
        self.parent
            .base
            .store
            .update_positions(&updates)
            .map_err(|e| failure(e.to_string()))?;
        self.parent
            .last_websocket_positions_update
            .set(Some(self.host.now_ms()));
        self.throttle_fetch_positions()
    }
    // ts: ApexOmniPrivateWebsocket#auth
    pub async fn auth(&self) -> Result<bool, ExchangeError> {
        let timestamp = self.host.now_ms();
        let to_sign = format!("{}GET/ws/accounts", js_number_string(timestamp));
        let signature = if let Some(account) = self.host.remote_signer_account() {
            let request = object([
                ("id", J::String(self.host.nanoid())),
                ("type", J::String("ws-auth".into())),
                ("exchangeName", J::String("apexomni".into())),
                ("accountName", account),
                (
                    "wsAuth",
                    object([
                        ("toSign", J::String(to_sign.clone())),
                        ("outputFormat", J::String("apexomni-base64".into())),
                        ("extraData", object([("timestamp", J::Number(timestamp))])),
                    ]),
                ),
            ]);
            let result = self.host.remote_sign(request).await?;
            let error = property(&result, "error")?;
            if truthy(error) {
                self.host.report_error(failure(format!(
                    "Remote WS signing unavailable: {}",
                    text(error)?
                )))?;
                return Ok(false);
            }
            property(&result, "wsAuthSignature")?.clone()
        } else {
            let key = STANDARD.encode(self.parent.base.options().secret.as_bytes());
            J::String(crate::utils::hmac_sha256_base64(
                &to_sign.encode_utf16().collect::<Vec<_>>(),
                &key.encode_utf16().collect::<Vec<_>>(),
            ))
        };
        let options = self.parent.base.options();
        let login = object([
            ("type", J::String("login".into())),
            ("httpMethod", J::String("GET".into())),
            ("requestPath", J::String("/ws/accounts".into())),
            ("apiKey", J::String(options.key)),
            (
                "passphrase",
                options.passphrase.map_or(J::Undefined, J::String),
            ),
            ("timestamp", J::Number(timestamp)),
            ("signature", signature),
        ]);
        let login = safe_cex_num::js_json_stringify(&login)
            .map_err(|e| failure(e.message.into()))?
            .ok_or_else(|| failure("login did not stringify".into()))?;
        send(
            self.host.as_ref(),
            object([
                ("op", J::String("login".into())),
                ("args", J::Array(vec![J::String(login)])),
            ]),
            true,
        )?;
        Ok(true)
    }
    // ts: ApexOmniPrivateWebsocket#subscribe
    pub fn subscribe(&self) -> Result<(), ExchangeError> {
        self.host.send("{\"op\":\"subscribe\",\"args\":[\"ws_zk_accounts_v3\",\"ws_user_v1\",\"ws_notify_v1\"]}",true)
    }
    // ts: ApexOmniPrivateWebsocket.dispose
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        if let Some(timer) = self.orders_timer.get() {
            self.host.clear_timeout(timer)?;
            self.orders_timer.set(None);
        }
        if let Some(timer) = self.positions_timer.get() {
            self.host.clear_timeout(timer)?;
            self.positions_timer.set(None);
        }
        self.host.dispose()
    }
}
