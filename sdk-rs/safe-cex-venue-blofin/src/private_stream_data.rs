//! Concrete private socket and the remaining three topic bodies.
use crate::{
    orders::{OrdersInput, map_orders_for_private_topic},
    positions::{PositionMapError, PositionsInput, map_positions},
    private_stream_control as core,
};
use safe_cex_num::{
    js_number, js_truthy, multiply, numeric_ingress::js_from_value, parse_float, round_usd,
    subtract,
};
use safe_cex_store::{
    Store, StoreMutationError, StoreUpdate,
    incremental_fills::{IncrementalFill, clean_float, create_incremental_fills},
};
use safe_cex_types::{
    Order, OrderSide, OrderStatus, Position,
    js_coercion::{JsCoercionError, js_string},
};
use serde_json::Value;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

#[derive(Debug)]
pub enum PrivateDataError<E> {
    Host(E),
    Coercion(JsCoercionError),
    Ingress(serde_json::Error),
    Store(StoreMutationError),
    Shape(String),
}
impl<E: std::fmt::Display> std::fmt::Display for PrivateDataError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(e) => e.fmt(f),
            Self::Coercion(e) => f.write_str(e.message),
            Self::Ingress(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
            Self::Shape(e) => f.write_str(e),
        }
    }
}
impl<E> From<JsCoercionError> for PrivateDataError<E> {
    fn from(e: JsCoercionError) -> Self {
        Self::Coercion(e)
    }
}
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateFill {
    pub id: String,
    pub order_id: String,
    #[serde(skip_serializing_if = "safe_cex_types::ModelField::is_none")]
    pub side: safe_cex_types::ModelField<OrderSide>,
    pub symbol: String,
    pub price: f64,
    pub amount: f64,
    pub timestamp: f64,
}
pub struct PrivateTopicState {
    /// ts: incrementalFills. Cumulative filledSize and filledSize x averagePrice
    /// become this execution's quantity and value.
    pub incremental_fills: RefCell<Box<dyn FnMut(&str, f64, Option<f64>) -> IncrementalFill>>,
}
impl Default for PrivateTopicState {
    fn default() -> Self {
        Self {
            incremental_fills: RefCell::new(Box::new(create_incremental_fills(500.0))),
        }
    }
}

pub trait PrivateDataHost: core::PrivateStreamHost {
    fn super_construct(&self) -> Result<(), Self::Error>;
    fn set_treat_any_message_as_pong(&self, enabled: bool);
    fn wall_clock_ms(&self) -> f64;
    fn emit_fill(&self, fill: PrivateFill) -> Result<(), Self::Error>;
    fn emit_mapping_error(&self, message: &str) -> Result<(), Self::Error>;
    /// Preserve the typed domain error through the inherited socket dispatcher.
    fn body_error(&self, error: PrivateDataError<Self::Error>) -> Self::Error;
    fn map_order_rows(&self, rows: &Value) -> Result<Vec<Order>, PrivateDataError<Self::Error>> {
        map_orders_for_private_topic(
            &OrdersInput::from_retained(rows.clone()),
            self.store(),
            || self.timestamp_ms(),
        )
        .map_err(PrivateDataError::from)
    }
    fn map_position_rows(
        &self,
        rows: &Value,
    ) -> Result<Vec<Position>, PrivateDataError<Self::Error>> {
        let input =
            js_from_value::<PositionsInput>(rows.clone()).map_err(PrivateDataError::Ingress)?;
        map_positions(&input, self.store(), |msg| self.emit_mapping_error(msg)).map_err(|e| match e
        {
            PositionMapError::Coercion(e) => PrivateDataError::Coercion(e),
            PositionMapError::Emitter(e) => PrivateDataError::Host(e),
            PositionMapError::Store(e) => PrivateDataError::Store(e.into()),
        })
    }
}
fn field<'a, E>(row: &'a Value, name: &str) -> Result<Option<&'a Value>, PrivateDataError<E>> {
    if row.is_null() {
        Err(PrivateDataError::Shape(format!(
            "Cannot read properties of null (reading '{name}')"
        )))
    } else {
        Ok(row.get(name))
    }
}
fn truth_or<'a>(values: impl IntoIterator<Item = Option<&'a Value>>) -> Option<&'a Value> {
    values.into_iter().flatten().find(|v| js_truthy(Some(v)))
}
fn float(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    Ok(parse_float(&js_string(value)?))
}
fn num_truth(n: f64) -> bool {
    n != 0.0 && !n.is_nan()
}
fn topic_data<E>(data: &Value) -> Result<Option<&Value>, PrivateDataError<E>> {
    if data.is_null() {
        Err(PrivateDataError::Shape(
            "Cannot destructure property 'data' of 'object null' as it is null.".into(),
        ))
    } else {
        Ok(data.get("data"))
    }
}
fn rows<E>(data: &Value) -> Result<Vec<Value>, PrivateDataError<E>> {
    match topic_data(data)? {
        Some(Value::Array(rows)) => Ok(rows.clone()),
        Some(Value::String(text)) => {
            Ok(text.chars().map(|c| Value::String(c.to_string())).collect())
        }
        _ => Err(PrivateDataError::Shape(
            "blofinOrders is not iterable".into(),
        )),
    }
}

pub fn handle_order_topic<H: PrivateDataHost>(
    state: &PrivateTopicState,
    host: &H,
    data: &Value,
    mut throttle: impl FnMut() -> Result<(), H::Error>,
) -> Result<(), PrivateDataError<H::Error>> {
    for row in &rows(data)? {
        let orders = host.map_order_rows(&Value::Array(vec![row.clone()]))?;
        let fallback = orders.first().map(|o| Value::String(o.id.clone()));
        let order_id = js_string(
            truth_or([
                field(row, "orderId")?,
                field(row, "ordId")?,
                fallback.as_ref(),
            ])
            .or(Some(&Value::String(String::new()))),
        )?;
        let raw_state = field(row, "state")?.and_then(Value::as_str);
        if !orders.is_empty() {
            let ids: Vec<_> = orders.iter().map(|o| o.id.as_str()).collect();
            for (raw, status) in [
                ("filled", OrderStatus::Closed),
                ("canceled", OrderStatus::Canceled),
                ("effective", OrderStatus::Closed),
                ("order_failed", OrderStatus::Canceled),
            ] {
                if raw_state == Some(raw) {
                    host.store()
                        .update_orders_status(&ids, status)
                        .map_err(PrivateDataError::Store)?;
                }
            }
            if matches!(raw_state, Some("live" | "partially_filled")) {
                host.store()
                    .add_or_update_orders(orders.clone())
                    .map_err(PrivateDataError::Store)?;
            }
            if matches!(raw_state, Some("filled" | "partially_filled")) {
                let key = js_string(field(row, "instId")?)?;
                let market = host.store().markets_by_id()?.get(&key).cloned();
                let contract = if let Some(size) = market
                    .as_ref()
                    .and_then(|m| m.contract_size)
                    .filter(|v| num_truth(*v))
                {
                    size
                } else {
                    safe_cex_num::js_retained_parse_float(
                        market
                            .as_ref()
                            .and_then(|m| m.info.as_ref())
                            .and_then(|i| i.get("contractValue"))
                            .filter(|v| v.is_truthy())
                            .or(Some(
                                &safe_cex_types::retained_json::RetainedJsonInput::Text(
                                    "1".encode_utf16().collect(),
                                ),
                            )),
                    )?
                };
                let filled = float(
                    truth_or([field(row, "filledSize")?]).or(Some(&Value::String("0".into()))),
                )?;
                let average = float(field(row, "averagePrice")?)?;
                if market.is_some() && filled.is_finite() && average.is_finite() && average > 0.0 {
                    // The borrow ends before emission, so an emitter re-entering this
                    // body sees the committed cumulative fill.
                    let fill = (state.incremental_fills.borrow_mut())(
                        &order_id,
                        filled,
                        Some(filled * average),
                    );
                    if fill.qty > 0.0 {
                        let time = match truth_or([
                            field(row, "fillTime")?,
                            field(row, "uTime")?,
                            field(row, "cTime")?,
                        ]) {
                            Some(raw) => js_number(Some(raw))?,
                            None => host.wall_clock_ms(),
                        };
                        let timestamp = if time > 1_000_000_000_000.0 {
                            (time / 1000.0).floor()
                        } else {
                            time.floor()
                        };
                        host.emit_fill(PrivateFill {
                            id: order_id.clone(),
                            order_id: order_id.clone(),
                            side: orders[0].side,
                            symbol: orders[0].symbol.clone(),
                            // This execution's own price, not the order's running average.
                            price: fill
                                .value
                                .map_or(average, |value| clean_float(value / fill.qty)),
                            amount: multiply(fill.qty, contract),
                            timestamp,
                        })
                        .map_err(PrivateDataError::Host)?;
                    }
                }
            }
        }
    }
    throttle().map_err(PrivateDataError::Host)
}
pub fn handle_position_topic<H: PrivateDataHost>(
    host: &H,
    data: &Value,
) -> Result<(), PrivateDataError<H::Error>> {
    let raw = topic_data(data)?.ok_or_else(|| {
        PrivateDataError::Shape("Cannot read properties of undefined (reading 'filter')".into())
    })?;
    let positions = host.map_position_rows(raw)?;
    let nonempty = !positions.is_empty();
    host.store()
        .add_or_update_positions_full(positions)
        .map_err(PrivateDataError::Store)?;
    if nonempty {
        let raw = raw.as_array().ok_or_else(|| {
            PrivateDataError::Shape("Position topic data must be an array".into())
        })?;
        let sum = |name: &str| -> Result<f64, PrivateDataError<H::Error>> {
            let mut value = None;
            for row in raw {
                let n = float(field(row, name)?)?;
                value = Some(value.map_or(n, |sum| sum + n));
            }
            Ok(round_usd(value.unwrap_or(0.0)))
        };
        let used = sum("maintenanceMargin")?;
        let upnl = sum("unrealizedPnl")?;
        let mut balance = host.store().balance()?.clone();
        balance.used = if num_truth(used) { used } else { 0.0 };
        balance.upnl = if num_truth(upnl) { upnl } else { 0.0 };
        host.store()
            .update(
                StoreUpdate {
                    balance: Some(balance),
                    ..Default::default()
                },
                false,
            )
            .map_err(PrivateDataError::Store)?;
    }
    Ok(())
}
pub fn handle_account_topic<H: PrivateDataHost>(
    host: &H,
    data: &Value,
) -> Result<(), PrivateDataError<H::Error>> {
    let data = topic_data(data)?.ok_or_else(|| {
        PrivateDataError::Shape(
            "Cannot read properties of undefined (reading 'totalEquity')".into(),
        )
    })?;
    let total = round_usd(float(field(data, "totalEquity")?)?);
    let mut balance = host.store().balance()?.clone();
    balance.free = subtract(total, host.store().balance()?.used);
    balance.total = subtract(total, host.store().balance()?.upnl);
    balance.info = Some(data.clone().into());
    host.store()
        .update(
            StoreUpdate {
                balance: Some(balance),
                ..Default::default()
            },
            false,
        )
        .map_err(PrivateDataError::Store)
}

pub struct BlofinPrivateWebsocket<H: PrivateDataHost> {
    pub core: Rc<core::PrivateStreamState<H::Timer>>,
    pub topics: Rc<PrivateTopicState>,
    bridge: Rc<PrivateBridge<H>>,
}
impl<H: PrivateDataHost> BlofinPrivateWebsocket<H> {
    pub fn new(host: Rc<H>) -> Result<Rc<Self>, H::Error> {
        host.super_construct()?;
        host.set_treat_any_message_as_pong(true);
        let core: Rc<core::PrivateStreamState<H::Timer>> = Rc::default();
        let topics: Rc<PrivateTopicState> = Rc::default();
        let bridge = Rc::new_cyclic(|weak| PrivateBridge {
            host,
            core: core.clone(),
            topics: topics.clone(),
            weak: weak.clone(),
            parent: RefCell::new(None),
        });
        Ok(Rc::new(Self {
            core,
            topics,
            bridge,
        }))
    }
    pub fn endpoint(&self) -> &'static str {
        core::endpoint(self.bridge.as_ref())
    }
    pub fn dispose(&self) -> Result<(), H::Error> {
        core::dispose(&self.core, self.bridge.as_ref())
    }
    pub fn connect_and_subscribe(&self) -> Result<(), H::Error> {
        core::connect_and_subscribe(self.bridge.as_ref())
    }
    pub fn on_open(&self) -> Result<(), H::Error> {
        core::on_open(&self.core, &self.bridge)
    }
    pub fn ping(&self) -> Result<(), H::Error> {
        core::ping(&self.core, self.bridge.as_ref())
    }
    pub fn auth(&self) -> core::PrivateStreamFuture<Result<(), core::PrivateAuthError<H::Error>>> {
        core::auth(&self.bridge)
    }
    pub fn subscribe(&self) -> Result<(), H::Error> {
        core::subscribe(self.bridge.as_ref())
    }
    pub fn on_message(&self, data: &str) -> Result<(), H::Error> {
        core::on_message(self.bridge.as_ref(), data)
    }
    pub fn handle_order_topic(&self, data: &Value) -> Result<(), PrivateDataError<H::Error>> {
        handle_order_topic(&self.topics, self.bridge.as_ref(), data, || {
            self.throttled_fetch_orders()
        })
    }
    pub fn handle_position_topic(&self, data: &Value) -> Result<(), PrivateDataError<H::Error>> {
        handle_position_topic(self.bridge.as_ref(), data)
    }
    pub fn handle_account_topic(&self, data: &Value) -> Result<(), PrivateDataError<H::Error>> {
        handle_account_topic(self.bridge.as_ref(), data)
    }
    pub fn throttled_fetch_orders(&self) -> Result<(), H::Error> {
        core::throttled_fetch_orders(&self.core, &self.bridge)
    }
    pub fn fetch_throttled_orders(&self) -> core::PrivateStreamFuture<()> {
        core::fetch_throttled_orders(&self.core, &self.bridge)
    }
}
struct PrivateBridge<H: PrivateDataHost> {
    host: Rc<H>,
    core: Rc<core::PrivateStreamState<H::Timer>>,
    topics: Rc<PrivateTopicState>,
    weak: Weak<Self>,
    parent: RefCell<Option<PrivateParentCallbacks<H::Error>>>,
}

impl<H: PrivateDataHost> core::PrivateStreamHost for PrivateBridge<H> {
    type Error = H::Error;
    type Timer = H::Timer;
    fn is_disposed(&self) -> bool {
        self.host.is_disposed()
    }
    fn is_testnet(&self) -> bool {
        self.host.is_testnet()
    }
    fn resolve_private_route(&self) -> Result<core::PrivateRoute, Self::Error> {
        self.host.resolve_private_route()
    }
    fn create_and_assign_socket(&self, url: Option<&str>) -> Result<(), Self::Error> {
        self.host.create_and_assign_socket(url)
    }
    fn add_socket_listener(&self, event: &str) -> Result<(), Self::Error> {
        self.host.add_socket_listener(event)
    }
    fn super_dispose(&self) -> Result<(), Self::Error> {
        self.host.super_dispose()
    }
    fn timer_is_truthy(&self, timer: &Self::Timer) -> bool {
        self.host.timer_is_truthy(timer)
    }
    fn clear_timeout(&self, timer: &Self::Timer) -> Result<(), Self::Error> {
        self.host.clear_timeout(timer)
    }
    fn set_timeout(
        &self,
        callback: Rc<dyn Fn() -> core::PrivateStreamFuture<()>>,
        delay_ms: f64,
    ) -> Result<Self::Timer, Self::Error> {
        self.host.set_timeout(callback, delay_ms)
    }
    fn send_with_log(&self, frame: &str) -> Result<(), Self::Error> {
        self.host.send_with_log(frame)
    }
    fn performance_now(&self) -> f64 {
        self.host.performance_now()
    }
    fn nonce(&self) -> String {
        self.host.nonce()
    }
    fn timestamp_ms(&self) -> f64 {
        self.host.timestamp_ms()
    }
    fn remote_signer_present(&self) -> bool {
        self.host.remote_signer_present()
    }
    fn remote_account_name(&self) -> Option<Value> {
        self.host.remote_account_name()
    }
    fn request_id(&self) -> String {
        self.host.request_id()
    }
    fn sign_remote(
        &self,
        request: core::RemoteAuthRequest,
    ) -> core::PrivateStreamStart<core::RemoteAuthResult, Self::Error> {
        self.host.sign_remote(request)
    }
    fn secret(&self) -> String {
        self.host.secret()
    }
    fn key(&self) -> String {
        self.host.key()
    }
    fn passphrase(&self) -> Option<Value> {
        self.host.passphrase()
    }
    fn emit_signing_error(&self, message: &str) -> Result<(), Self::Error> {
        self.host.emit_signing_error(message)
    }
    fn launch_auth(
        &self,
        task: core::PrivateStreamFuture<Result<(), core::PrivateAuthError<Self::Error>>>,
    ) {
        self.host.launch_auth(task)
    }
    fn track_last_received(&self) -> Result<(), Self::Error> {
        self.host.track_last_received()
    }
    fn diagnostic_receive(&self, data: &str) -> Result<(), Self::Error> {
        self.host.diagnostic_receive(data)
    }
    fn log_last_log(&self, data: &str) -> Result<(), Self::Error> {
        self.host.log_last_log(data)
    }
    fn handle_pong(&self) -> Result<(), Self::Error> {
        self.host.handle_pong()
    }
    fn reconnecting_enabled(&self) -> bool {
        self.host.reconnecting_enabled()
    }
    fn fetch_and_set_private_snapshot(&self) -> Result<(), Self::Error> {
        self.host.fetch_and_set_private_snapshot()
    }
    fn handle_order_topic(&self, data: &Value) -> Result<(), Self::Error> {
        handle_order_topic(&self.topics, self, data, || {
            core::throttled_fetch_orders(&self.core, &self.weak.upgrade().expect("live bridge"))
        })
        .map_err(|e| self.host.body_error(e))
    }
    fn handle_position_topic(&self, data: &Value) -> Result<(), Self::Error> {
        handle_position_topic(self, data).map_err(|e| self.host.body_error(e))
    }
    fn handle_account_topic(&self, data: &Value) -> Result<(), Self::Error> {
        handle_account_topic(self, data).map_err(|e| self.host.body_error(e))
    }
    fn fetch_orders(&self) -> core::PrivateStreamStart<Vec<Order>, Self::Error> {
        let callback = self
            .parent
            .borrow()
            .as_ref()
            .map(|p| p.fetch_orders.clone());
        match callback {
            Some(callback) => callback(),
            None => self.host.fetch_orders(),
        }
    }
    fn store(&self) -> &dyn Store {
        self.host.store()
    }
}

pub type ParentOrderMapper<E> = Rc<dyn Fn(&Value) -> Result<Vec<Order>, PrivateDataError<E>>>;
pub type ParentPositionMapper<E> = Rc<dyn Fn(&Value) -> Result<Vec<Position>, PrivateDataError<E>>>;
pub type ParentOrdersReader<E> = Rc<dyn Fn() -> core::PrivateStreamStart<Vec<Order>, E>>;
pub struct PrivateParentCallbacks<E> {
    pub map_orders: ParentOrderMapper<E>,
    pub map_positions: ParentPositionMapper<E>,
    pub fetch_orders: ParentOrdersReader<E>,
}
impl<H: PrivateDataHost> BlofinPrivateWebsocket<H> {
    pub fn bind_parent(&self, callbacks: PrivateParentCallbacks<H::Error>) {
        *self.bridge.parent.borrow_mut() = Some(callbacks);
    }
}
impl<H: PrivateDataHost> PrivateDataHost for PrivateBridge<H> {
    fn super_construct(&self) -> Result<(), H::Error> {
        self.host.super_construct()
    }
    fn set_treat_any_message_as_pong(&self, enabled: bool) {
        self.host.set_treat_any_message_as_pong(enabled)
    }
    fn wall_clock_ms(&self) -> f64 {
        self.host.wall_clock_ms()
    }
    fn emit_fill(&self, fill: PrivateFill) -> Result<(), H::Error> {
        self.host.emit_fill(fill)
    }
    fn emit_mapping_error(&self, msg: &str) -> Result<(), H::Error> {
        self.host.emit_mapping_error(msg)
    }
    fn body_error(&self, e: PrivateDataError<H::Error>) -> H::Error {
        self.host.body_error(e)
    }
    fn map_order_rows(&self, rows: &Value) -> Result<Vec<Order>, PrivateDataError<H::Error>> {
        let f = self.parent.borrow().as_ref().map(|p| p.map_orders.clone());
        match f {
            Some(f) => f(rows),
            None => self.host.map_order_rows(rows),
        }
    }
    fn map_position_rows(&self, rows: &Value) -> Result<Vec<Position>, PrivateDataError<H::Error>> {
        let f = self
            .parent
            .borrow()
            .as_ref()
            .map(|p| p.map_positions.clone());
        match f {
            Some(f) => f(rows),
            None => self.host.map_position_rows(rows),
        }
    }
}

impl<E> From<safe_cex_types::StoreReadError> for PrivateDataError<E> {
    fn from(error: safe_cex_types::StoreReadError) -> Self {
        Self::Store(error.into())
    }
}

impl<E> From<crate::reads::MappingError> for PrivateDataError<E> {
    fn from(error: crate::reads::MappingError) -> Self {
        match error {
            crate::reads::MappingError::Coercion(error) => Self::Coercion(error),
            crate::reads::MappingError::Store(error) => Self::Store(error.into()),
            crate::reads::MappingError::Input(error) => Self::Shape(error),
        }
    }
}
