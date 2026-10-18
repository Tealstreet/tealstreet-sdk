//! Private login, keepalive, reconciliation and account-frame handlers.
use crate::{
    data::truthy,
    exchange::{ApexOmniExchange, failure, text},
    protocol,
    socket_host::{ApexSocketHost, SocketListener, endpoint, object, property, send},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue as J, js_number_string, math_max};
use safe_cex_store::model_patch::ModelPatch;
use safe_cex_types::{OrderStatus, retained_json::RetainedJsonInput as Input};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

fn retained_text(value: impl AsRef<str>) -> Input {
    Input::Text(value.as_ref().encode_utf16().collect())
}
fn retained_object(values: impl IntoIterator<Item = (&'static str, Input)>) -> Input {
    Input::Object(
        values
            .into_iter()
            .map(|(key, value)| (key.encode_utf16().collect(), value))
            .collect(),
    )
}

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
        use crate::response::{get, text, truthy};
        use Input as J;
        if self.host.is_disposed() {
            return Ok(());
        }
        self.host.track_last_received()?;
        self.host.diagnostic_receive(raw)?;
        let json = safe_cex_transport::how::json_parse(raw);
        if get(&json, "success") == &J::Bool(false) && truthy(get(&json, "ret_msg")) {
            self.parent.base.log(
                &format!(
                    "Private WS request rejected: {}",
                    text(get(&json, "ret_msg"))?
                ),
                "warning",
            )?;
        }
        if get(&json, "topic") == &retained_text("ws_zk_accounts_v3") {
            self.host.record_stats("ws_zk_accounts_v3")?;
            self.handle_ws_accounts_v2_topic(&json)?;
        }
        if get(&json, "op") == &retained_text("ping") || !matches!(get(&json, "ping"), J::Undefined)
        {
            let args = match get(&json, "args") {
                J::Array(values) if !values.is_empty() => J::Array(values.clone()),
                _ => J::Array(vec![retained_text(js_number_string(self.host.now_ms()))]),
            };
            let pong = retained_object([("op", retained_text("pong")), ("args", args)]);
            let frame = pong
                .json_stringify()
                .map_err(ExchangeError::from)?
                .ok_or_else(|| failure("socket frame did not stringify".into()))?;
            self.host.send(&frame, false)?;
        }
        if get(&json, "op") == &retained_text("pong")
            || get(&json, "ret_msg") == &retained_text("pong")
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
    pub fn handle_ws_accounts_v2_topic(self: &Rc<Self>, json: &Input) -> Result<(), ExchangeError> {
        use crate::response::{property, truthy};
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
    pub fn handle_fill_topic(&self, data: &Input) -> Result<(), ExchangeError> {
        for fill in crate::response::array(data).map_err(failure)? {
            self.parent.record_ws_execution(fill)?;
        }
        Ok(())
    }
    // ts: ApexOmniPrivateWebsocket.handleOrderTopic
    pub fn handle_order_topic(self: &Rc<Self>, data: &Input) -> Result<(), ExchangeError> {
        use crate::response::{float, get, numeric, property, text, truthy};
        use Input as J;
        property(data, "some")?;
        let data = crate::response::array(data).map_err(|_| {
            let mut error = failure("data.some is not a function".into());
            error.name = "TypeError".into();
            error
        })?;
        let zero = retained_text("0");
        let mut has_canceled = false;
        for row in data {
            if property(row, "status")? == &retained_text("CANCELED") {
                has_canceled = true;
                break;
            }
        }
        if !has_canceled {
            self.throttle_fetch_orders()?;
        }
        for row in data {
            let raw_id = property(row, "id")?;
            if let Some(id) = raw_id
                .as_text()
                .and_then(|units| String::from_utf16(units).ok())
                && self
                    .parent
                    .recently_cancelled_orders
                    .borrow()
                    .contains_key(&id)
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
            let filled = status == &retained_text("FILLED");
            let canceled = status == &retained_text("CANCELED");
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
                        let execution = retained_object([
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
                            ("size", retained_text(js_number_string(delta))),
                            ("fee", retained_text(js_number_string(fee_delta))),
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
                    .any(|v| reason == &retained_text(*v))
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
            } else if status == &retained_text("OPEN") || status == &retained_text("UNTRIGGERED") {
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
    pub fn handle_position_topic(self: &Rc<Self>, data: &Input) -> Result<(), ExchangeError> {
        let mut updates = Vec::new();
        for row in crate::response::array(data).map_err(failure)? {
            // TS binds the mapper to this socket, whose receiver has no `log`.
            // Preserve the mapper's evaluation order before the missing log call.
            if let Some(mut position) = self.parent.map_position_with_log(row, |_| {
                let mut error = failure("this.log is not a function".into());
                error.name = "TypeError".into();
                Err(error)
            })? {
                // Info goes directly into the existing native patch slot. The
                // remaining strict fields keep their established patch admission.
                let info = std::mem::take(&mut position.info);
                let serde_json::Value::Object(fields) =
                    serde_json::to_value(&position).map_err(|e| failure(e.to_string()))?
                else {
                    unreachable!()
                };
                let mut patch = ModelPatch::from(fields);
                match info {
                    safe_cex_types::ModelField::Value(info) => patch.set_native("info", info),
                    safe_cex_types::ModelField::Undefined => patch.set_undefined("info"),
                    safe_cex_types::ModelField::Absent => (),
                }
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
