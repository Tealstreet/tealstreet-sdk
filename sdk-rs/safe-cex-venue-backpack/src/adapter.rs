//! Owned exchange composition and concrete state-backed method frames.
use crate::native_value::NativeValue;
use crate::{
    native_http::RoutedClient, native_value::Value, order_actions as actions,
    public_control::JsString, rest_reads as reads,
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::JsJsonValue;
use safe_cex_types::{
    Order, OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionMode, PositionSide,
};
use std::{
    cell::{Cell, OnceCell, RefCell},
    rc::{Rc, Weak},
};
pub type DateParser = Rc<dyn Fn(JsString) -> Result<f64, ExchangeError>>;
// ts: BackpackExchange
pub struct BackpackExchange {
    pub base: Rc<BaseExchange>,
    pub xhr: RoutedClient,
    pub account_leverage_limit: Cell<f64>,
    pub grafana_endpoints_every: &'static [&'static str],
    pub grafana_endpoints_periodic: &'static [&'static str],
    pub public_websocket: OnceCell<Rc<crate::public_adapter::BackpackPublicWebSocket>>,
    pub private_websocket: OnceCell<Rc<crate::private_adapter::BackpackPrivateWebSocket>>,
    pub date_parse: DateParser,
    pub runtime: Rc<dyn safe_cex_exchange::websocket::WsRuntime>,
    pub capabilities: safe_cex_exchange::capabilities::ExchangeCapabilities,
    pub self_ref: Weak<BackpackExchange>,
    pub tasks: RefCell<Vec<Box<dyn safe_cex_exchange::runtime::ExchangeTask>>>,
}
impl BackpackExchange {
    pub fn new(
        mut base: BaseExchange,
        build: crate::native_http::BuildHttp,
        install: crate::native_http::InstallTransport,
        runtime: Rc<dyn safe_cex_exchange::websocket::WsRuntime>,
        date_parse: DateParser,
    ) -> Result<Rc<Self>, ExchangeError> {
        base.settings.client_order_id_rule = Some(
            safe_cex_exchange::client_order_id::ClientOrderIdRule {
                label: "Backpack".into(),
                requirement: "be a canonical decimal uint32 between 1 and 4294967295".into(),
                is_valid: Rc::new(|id| {
                    let u: Vec<u16> = id.encode_utf16().collect();
                    crate::startup_recovery::ExchangeFields {
                account_leverage_limit: Cell::new(1.0), grafana_every: crate::constants::GRAFANA_ENDPOINTS_EVERY,
                grafana_periodic: crate::constants::GRAFANA_ENDPOINTS_PERIODIC, client_id_label: "Backpack",
                client_id_requirement: "be a canonical decimal uint32 between 1 and 4294967295",
            }.valid_client_id(&u)
                }),
                is_honorable_shape: None,
                mint: None,
            },
        );
        let base = Rc::new(base);
        let options = base.options();
        let secondary_proxy = options
            .secondary_proxy_url
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| runtime.transport().public_cors_proxy());
        let factory = Rc::new(RefCell::new(crate::native_http::Factory {
            base: base.clone(),
            build,
            install,
        }));
        let primary = crate::api_factory::create_api(
            factory.clone(),
            Rc::new(RefCell::new(options.clone())),
        )?;
        primary.limit_requests(base.runtime.clone(), 10);
        let reader = if let Some(key) = options.secondary_key.as_ref().filter(|key| !key.is_empty())
        {
            let mut secondary = options.clone();
            secondary.key.clone_from(key);
            if let Some(secret) = &options.secondary_secret {
                secondary.secret.clone_from(secret);
            }
            if let Some(passphrase) = &options.secondary_passphrase {
                secondary.passphrase = Some(passphrase.clone());
            }
            secondary.override_proxy_url = secondary_proxy;
            crate::api_factory::create_api(factory, Rc::new(RefCell::new(secondary)))?
        } else {
            primary.clone()
        };
        let xhr = RoutedClient::new(primary, reader);
        let owner = Rc::new_cyclic(|weak| Self {
            base,
            xhr,
            account_leverage_limit: Cell::new(1.0),
            grafana_endpoints_every: crate::constants::GRAFANA_ENDPOINTS_EVERY,
            grafana_endpoints_periodic: crate::constants::GRAFANA_ENDPOINTS_PERIODIC,
            public_websocket: OnceCell::new(),
            private_websocket: OnceCell::new(),
            date_parse,
            runtime,
            capabilities: Self::capabilities_table(),
            self_ref: weak.clone(),
            tasks: RefCell::new(vec![]),
        });
        let public = crate::public_adapter::BackpackPublicWebSocket::new(owner.clone())?;
        owner
            .public_websocket
            .set(public)
            .map_err(|_| ExchangeError::new("Public socket already initialized"))?;
        owner.base.set_socket(
            safe_cex_exchange::socket_lifecycle::SocketSlot::Public,
            Some(owner.public_websocket.get().unwrap().base.clone()),
        );
        let private = crate::private_adapter::BackpackPrivateWebSocket::new(owner.clone())?;
        owner
            .private_websocket
            .set(private)
            .map_err(|_| ExchangeError::new("Private socket already initialized"))?;
        owner.base.set_socket(
            safe_cex_exchange::socket_lifecycle::SocketSlot::Private,
            Some(owner.private_websocket.get().unwrap().base.clone()),
        );
        Ok(owner)
    }
    pub fn frame(self: &Rc<Self>) -> Rc<RefCell<Frame>> {
        Rc::new(RefCell::new(Frame {
            owner: self.clone(),
        }))
    }
    pub fn fetch_markets(self: &Rc<Self>) -> reads::Task<Vec<reads::Market<Value>>, ExchangeError> {
        let future = reads::fetch_markets(self.frame());
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn fetch_tickers(
        self: &Rc<Self>,
    ) -> reads::Task<Vec<Option<reads::Ticker<Value>>>, ExchangeError> {
        let future = reads::fetch_tickers(self.frame());
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn fetch_ohlcv(
        self: &Rc<Self>,
        opts: reads::OHLCVOptions,
    ) -> reads::Task<Vec<Option<reads::Candle>>, ExchangeError> {
        let future = reads::fetch_ohlcv(self.frame(), opts);
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn fetch_order_book(
        self: &Rc<Self>,
        symbol: JsString,
    ) -> reads::Task<reads::OrderBook, ExchangeError> {
        let future = reads::fetch_order_book(self.frame(), symbol);
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn fetch_historical_fills(
        self: &Rc<Self>,
        opts: reads::ExecutionsOptions,
    ) -> reads::Task<Vec<reads::Execution>, ExchangeError> {
        let future = reads::fetch_historical_fills(self.frame(), opts);
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn fetch_orders(
        self: &Rc<Self>,
        symbols: Option<Vec<JsString>>,
    ) -> reads::Task<Vec<Option<Order>>, ExchangeError> {
        let future = reads::fetch_orders(self.frame(), symbols);
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn fetch_positions(self: &Rc<Self>) -> reads::Task<Vec<reads::Position>, ExchangeError> {
        let future = reads::fetch_positions(self.frame());
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn fix_trigger_orders_without_trigger_by(
        self: &Rc<Self>,
        orders: reads::Array<Value>,
    ) -> reads::Task<(), ExchangeError> {
        let future = reads::fix_trigger_orders_without_trigger_by(self.frame(), orders);
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn place_order(
        self: &Rc<Self>,
        opts: actions::Options,
    ) -> actions::Task<Vec<JsString>, ExchangeError> {
        let future = actions::place_order(self.frame(), opts);
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn update_order(
        self: &Rc<Self>,
        opts: actions::Update,
    ) -> actions::Task<Vec<JsString>, ExchangeError> {
        let future = actions::update_order(self.frame(), opts);
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn get_position_mode(&self) -> ExchangeFuture<'static, PositionMode> {
        crate::account_reads::get_position_mode()
    }
    pub fn dispose(&self) -> Result<(), ExchangeError> {
        self.base.set_disposed_flag(true);
        self.public_websocket
            .get()
            .expect("constructed public socket")
            .dispose()?;
        self.private_websocket
            .get()
            .expect("constructed private socket")
            .dispose()
    }
}
pub struct Frame {
    pub owner: Rc<BackpackExchange>,
}
fn units(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn utf8(s: &[u16]) -> Result<String, ExchangeError> {
    String::from_utf16(s).map_err(|e| ExchangeError::new(format!("Native UTF-8 boundary: {e}")))
}
pub fn prepared_json(v: &Value) -> Result<serde_json::Value, ExchangeError> {
    let s = safe_cex_num::js_json_stringify(&v.prepared()?)
        .map_err(|e| ExchangeError::new(e.message))?
        .ok_or_else(|| ExchangeError::new("Cannot decode undefined wire record"))?;
    serde_json::from_str(&s).map_err(|e| ExchangeError::new(e.to_string()))
}
fn params(v: Vec<(&'static str, reads::Param<Value>)>) -> Result<JsJsonValue, ExchangeError> {
    Ok(JsJsonValue::Object(
        v.into_iter()
            .map(|(k, v)| {
                Ok((
                    k.into(),
                    match v {
                        reads::Param::Value(v) => v.prepared()?,
                        reads::Param::Number(n) => JsJsonValue::Number(n),
                        reads::Param::Text(s) => JsJsonValue::String(utf8(&s)?),
                        reads::Param::Bool(b) => JsJsonValue::Bool(b),
                        reads::Param::Undefined => JsJsonValue::Undefined,
                    },
                ))
            })
            .collect::<Result<_, ExchangeError>>()?,
    ))
}
impl Frame {
    fn read(
        &self,
        method: &str,
        request: reads::Request<Value>,
    ) -> Result<reads::Reply<Value, ExchangeError>, ExchangeError> {
        Ok(self.owner.xhr.request(
            method,
            request.endpoint,
            if request.params.is_empty() {
                JsJsonValue::Undefined
            } else {
                params(request.params)?
            },
            if request.data.is_empty() {
                JsJsonValue::Undefined
            } else {
                params(request.data)?
            },
        ))
    }
}
impl reads::Host for Frame {
    type Value = Value;
    type Error = ExchangeError;
    type LogTarget = ();
    type Order = Order;
    type TransportTarget = ();
    type TriggerTarget = ();
    fn transport_target(&mut self, _: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn trigger_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn field(&mut self, v: &Value, k: &str) -> Result<Value, ExchangeError> {
        v.field(k)
    }
    fn array(&mut self, v: &Value) -> Result<reads::Array<Value>, ExchangeError> {
        v.array()
    }
    fn is_array(&mut self, v: &Value) -> bool {
        matches!(v, Value::Array(_))
    }
    fn truthy(&mut self, v: &Value) -> bool {
        v.truthy()
    }
    fn nullish(&mut self, v: &Value) -> bool {
        matches!(v, Value::Null | Value::Undefined)
    }
    fn strict_equal(&mut self, a: &Value, b: &Value) -> bool {
        a.same(b)
    }
    fn is_text(&mut self, v: &Value, s: &str) -> bool {
        matches!(v,Value::String(t)if t.as_ref()==units(s))
    }
    fn is_false(&mut self, v: &Value) -> bool {
        matches!(v, Value::Bool(false))
    }
    fn string(&mut self, v: &Value) -> Result<JsString, ExchangeError> {
        v.string()
    }
    fn parse_float(&mut self, v: &Value) -> Result<f64, ExchangeError> {
        v.float()
    }
    fn number(&mut self, v: &Value) -> Result<f64, ExchangeError> {
        v.number()
    }
    fn undefined(&mut self) -> Value {
        Value::Undefined
    }
    fn zero(&mut self) -> Value {
        Value::Number(0.0)
    }
    fn is_undefined(&mut self, v: &Value) -> bool {
        matches!(v, Value::Undefined)
    }
    fn is_null(&mut self, v: &Value) -> bool {
        matches!(v, Value::Null)
    }
    fn pair(&mut self, v: &Value) -> Result<(Value, Value), ExchangeError> {
        let a = v.array()?;
        let a = a.borrow();
        Ok((
            a.first().cloned().flatten().unwrap_or(Value::Undefined),
            a.get(1).cloned().flatten().unwrap_or(Value::Undefined),
        ))
    }
    fn spread(&mut self, v: &[Value]) -> Result<Value, ExchangeError> {
        Value::spread(v)
    }
    fn get(
        &mut self,
        _: (),
        req: reads::Request<Value>,
    ) -> Result<reads::Reply<Value, ExchangeError>, ExchangeError> {
        self.read("GET", req)
    }
    fn delete(
        &mut self,
        _: (),
        req: reads::Request<Value>,
    ) -> Result<reads::Reply<Value, ExchangeError>, ExchangeError> {
        self.read("DELETE", req)
    }
    fn post(
        &mut self,
        _: (),
        req: reads::Request<Value>,
    ) -> Result<reads::Reply<Value, ExchangeError>, ExchangeError> {
        self.read("POST", req)
    }
    fn log_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn log(
        &mut self,
        _: (),
        message: JsString,
        severity: reads::Severity,
    ) -> Result<(), ExchangeError> {
        self.owner.base.log(
            &utf8(&message)?,
            match severity {
                reads::Severity::Info => "info",
                reads::Severity::Warning => "warning",
                reads::Severity::Error => "error",
            },
        )
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<JsString, ExchangeError> {
        Ok(units(&e.js_error_string()))
    }
    fn error_message(&mut self, e: &ExchangeError) -> Result<Option<JsString>, ExchangeError> {
        Ok(Some(units(&e.message)))
    }
    fn stringify(&mut self, v: &Value) -> Result<JsString, ExchangeError> {
        let text = safe_cex_num::js_json_stringify(&v.prepared()?)
            .map_err(|e| ExchangeError::new(e.message))?
            .unwrap_or_else(|| "undefined".into());
        Ok(units(&text))
    }
    fn error(&mut self, text: &str) -> ExchangeError {
        ExchangeError::new(text)
    }
    fn date_parse(&mut self, v: JsString) -> Result<f64, ExchangeError> {
        (self.owner.date_parse)(v)
    }
    fn clock(&mut self) -> Result<f64, ExchangeError> {
        Ok(safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms())
    }
    fn public_only(&mut self) -> Result<bool, ExchangeError> {
        let o = self.owner.base.options();
        Ok(o.public_data_only == Some(true) || !self.owner.base.can_sign())
    }
    fn market(&mut self, s: &[u16]) -> Result<Option<Value>, ExchangeError> {
        let s = utf8(s)?;
        self.owner
            .base
            .store
            .markets()?
            .iter()
            .find(|m| m.symbol == s)
            .map(|m| {
                serde_json::to_value(m)
                    .map(|v| Value::from_prepared(v.into()))
                    .map_err(|e| ExchangeError::new(e.to_string()))
            })
            .transpose()
    }
    fn markets(&mut self) -> Result<reads::Array<Value>, ExchangeError> {
        let m = self
            .owner
            .base
            .store
            .markets()?
            .iter()
            .map(|m| {
                serde_json::to_value(m)
                    .map(|v| Some(Value::from_prepared(v.into())))
                    .map_err(|e| ExchangeError::new(e.to_string()))
            })
            .collect::<Result<_, _>>()?;
        Ok(Rc::new(RefCell::new(m)))
    }
    fn position_mode(&mut self) -> Result<reads::Task<PositionMode, ExchangeError>, ExchangeError> {
        Ok(self.owner.get_position_mode())
    }
    fn leverage(&mut self) -> Result<f64, ExchangeError> {
        Ok(self.owner.account_leverage_limit.get())
    }
    fn position_id(
        &mut self,
        s: &[u16],
        side: PositionSide,
        mode: PositionMode,
    ) -> Result<JsString, ExchangeError> {
        let s = utf8(s)?;
        let side = if side == PositionSide::Long {
            "long"
        } else {
            "short"
        };
        Ok(units(&if mode == PositionMode::OneWay {
            s
        } else {
            format!("{s}:{side}")
        }))
    }
    fn repair(
        &mut self,
        orders: reads::Array<Value>,
    ) -> Result<reads::Task<(), ExchangeError>, ExchangeError> {
        Ok(self.owner.fix_trigger_orders_without_trigger_by(orders))
    }
    fn map_order(&mut self, v: Value) -> Result<Order, ExchangeError> {
        let order = serde_json::from_value::<crate::types::BackpackOrder>(prepared_json(&v)?)
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        crate::order_utils::map_order(
            &order,
            |s, side, n| self.determine(s, side, n),
            |m, severity| self.owner.base.log(m, severity),
        )
    }
    fn trigger_type(
        &mut self,
        _: (),
        symbol: Value,
        side: OrderSide,
        n: f64,
    ) -> Result<OrderType, ExchangeError> {
        self.determine(&symbol.utf8()?, side, n)
    }
    fn trigger_by(&mut self, tp: bool) -> Result<JsString, ExchangeError> {
        Ok(self.reference(tp, None))
    }
}
impl Frame {
    fn determine(
        &self,
        symbol: &str,
        side: OrderSide,
        trigger: f64,
    ) -> Result<OrderType, ExchangeError> {
        let tickers = self
            .owner
            .base
            .store
            .tickers()?
            .iter()
            .map(|t| (t.symbol.clone(), Some(t.last), Some(t.mark)))
            .collect::<Vec<_>>();
        Ok(crate::private_order_utils::determine_trigger_order_type(
            &tickers,
            &symbol.to_owned(),
            side,
            Some(trigger),
        ))
    }
    fn reference(&self, tp: bool, explicit: Option<OrderStopTriggerType>) -> JsString {
        let v = crate::order_utils::get_trigger_by(
            if tp {
                crate::order_utils::TriggerKind::TakeProfit
            } else {
                crate::order_utils::TriggerKind::StopLoss
            },
            explicit,
            |_| {
                (
                    Some(OrderStopTriggerType::MarkPrice),
                    Some(OrderStopTriggerType::MarkPrice),
                )
            },
        );
        units(match v {
            crate::types::BackpackOrderRequestTriggerBy::LastPrice => "LastPrice",
            crate::types::BackpackOrderRequestTriggerBy::IndexPrice => "IndexPrice",
            crate::types::BackpackOrderRequestTriggerBy::MarkPrice => "MarkPrice",
        })
    }
}
impl actions::Host for Frame {
    type Error = ExchangeError;
    type Market = safe_cex_types::Market;
    type Existing = Rc<RefCell<Order>>;
    type Target = ();
    fn resolve_caller_id(
        &mut self,
        opts: &actions::Options,
    ) -> Result<Option<JsString>, ExchangeError> {
        Ok(self
            .owner
            .base
            .resolve_caller_client_order_id(&crate::model_bridge::options(opts)?)?
            .map(|s| units(&s)))
    }

    fn market(&mut self, symbol: &[u16]) -> Result<Option<Self::Market>, ExchangeError> {
        let symbol = utf8(symbol)?;
        Ok(self
            .owner
            .base
            .store
            .markets()?
            .iter()
            .find(|m| m.symbol == symbol)
            .cloned())
    }
    fn market_id(&mut self, m: &Self::Market) -> Result<JsString, ExchangeError> {
        Ok(units(&m.id))
    }
    fn precision(&mut self, m: &Self::Market, price: bool) -> Result<f64, ExchangeError> {
        Ok(if price {
            m.precision.price
        } else {
            m.precision.amount
        })
    }
    fn trigger_by(
        &mut self,
        tp: bool,
        explicit: Option<OrderStopTriggerType>,
    ) -> Result<JsString, ExchangeError> {
        Ok(self.reference(tp, explicit))
    }
    fn post(
        &mut self,
        endpoint: &str,
        request: actions::Request,
    ) -> Result<actions::Task<JsString, ExchangeError>, ExchangeError> {
        let body = JsJsonValue::Object(
            request
                .into_iter()
                .map(|(k, v)| {
                    Ok((
                        k.into(),
                        match v {
                            actions::Field::Text(s) => JsJsonValue::String(utf8(&s)?),
                            actions::Field::Number(n) => JsJsonValue::Number(n),
                            actions::Field::Bool(b) => JsJsonValue::Bool(b),
                        },
                    ))
                })
                .collect::<Result<_, ExchangeError>>()?,
        );
        let response = self
            .owner
            .xhr
            .request("POST", endpoint, JsJsonValue::Undefined, body);
        Ok(Box::pin(
            async move { response.await?.field("id")?.string() },
        ))
    }
    fn log_target(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn log(
        &mut self,
        _: (),
        message: JsString,
        severity: actions::Severity,
    ) -> Result<(), ExchangeError> {
        self.owner.base.log(
            &utf8(&message)?,
            match severity {
                actions::Severity::Info => "info",
                actions::Severity::Warning => "warning",
                actions::Severity::Error => "error",
            },
        )
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<JsString, ExchangeError> {
        Ok(units(&e.js_error_string()))
    }
    fn error(&mut self, message: JsString, range: bool) -> ExchangeError {
        let mut e = ExchangeError::new(String::from_utf16_lossy(&message));
        if range {
            e.name = "RangeError".into();
        }
        e
    }
    fn find_order(&mut self, id: &[u16]) -> Result<Option<Self::Existing>, ExchangeError> {
        let id = utf8(id)?;
        Ok(self
            .owner
            .base
            .store
            .orders()?
            .iter()
            .find(|o| o.id == id)
            .cloned()
            .map(|o| Rc::new(RefCell::new(o))))
    }
    fn order_options(&mut self, o: &Self::Existing) -> Result<actions::Options, ExchangeError> {
        let o = o.borrow();
        Ok(actions::Options {
            symbol: units(&o.symbol),
            r#type: o.r#type.into_property(),
            side: o.side.into_property(),
            amount: o.amount,
            price: o.price.into_value(),
            time_in_force: (o.time_in_force).into_value(),
            reduce_only: o.reduce_only.into_property(),
            trigger_price: None,
            take_profit: None,
            stop_loss: None,
            tp_trigger_type: None,
            sl_trigger_type: None,
            client_order_id: None,
            ..Default::default()
        })
    }
    fn order_id(&mut self, o: &Self::Existing) -> Result<JsString, ExchangeError> {
        Ok(units(&o.borrow().id))
    }
    fn cancel(
        &mut self,
        o: Self::Existing,
    ) -> Result<actions::Task<(), ExchangeError>, ExchangeError> {
        Ok(self.owner.cancel_order(o.borrow().clone()))
    }
    fn place(
        &mut self,
        o: actions::Options,
    ) -> Result<actions::Task<Vec<JsString>, ExchangeError>, ExchangeError> {
        Ok(self.owner.place_order(o))
    }
}
impl BackpackExchange {
    pub fn cancel_order(self: &Rc<Self>, order: Order) -> actions::Task<(), ExchangeError> {
        let owner = self.clone();
        let log = owner.clone();
        let xhr = self.xhr.clone();
        let future = crate::account_reads::cancel_order(
            &order.symbol,
            &order.id,
            move |symbol| {
                Ok(owner
                    .base
                    .store
                    .markets()?
                    .iter()
                    .find(|m| m.symbol == symbol)
                    .map(|m| m.id.clone()))
            },
            move |endpoint, config| {
                let data = match config {
                    JsJsonValue::Object(o) => o.into_iter().find(|(k, _)| k == "data").unwrap().1,
                    _ => unreachable!("cancel_order builds an object"),
                };
                let response = xhr.request("DELETE", endpoint, JsJsonValue::Undefined, data);
                Ok(Box::pin(async move {
                    response.await?;
                    Ok(())
                }))
            },
            move |m, s| log.base.log(m, s),
        );
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn cancel_symbol_orders(
        self: &Rc<Self>,
        symbol: String,
    ) -> actions::Task<(), ExchangeError> {
        let owner = self.clone();
        let log = owner.clone();
        let xhr = self.xhr.clone();
        let future = crate::account_reads::cancel_symbol_orders(
            &symbol,
            move |s| {
                Ok(owner
                    .base
                    .store
                    .markets()?
                    .iter()
                    .find(|m| m.symbol == s)
                    .map(|m| m.id.clone()))
            },
            move |endpoint, config| {
                let data = match config {
                    JsJsonValue::Object(o) => o.into_iter().find(|(k, _)| k == "data").unwrap().1,
                    _ => unreachable!("native symbol cancellation config"),
                };
                let response = xhr.request("DELETE", endpoint, JsJsonValue::Undefined, data);
                Ok(Box::pin(async move {
                    response.await?;
                    Ok(())
                }))
            },
            move |m, s| log.base.log(m, s),
        );
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn fetch_balance(self: &Rc<Self>) -> actions::Task<safe_cex_types::Balance, ExchangeError> {
        let xhr = self.xhr.clone();
        let owner = self.clone();
        let future = crate::account_reads::fetch_balance(
            move |endpoint| {
                let response = xhr.request(
                    "GET",
                    endpoint,
                    JsJsonValue::Undefined,
                    JsJsonValue::Undefined,
                );
                Ok(Box::pin(async move { response.await?.prepared() }))
            },
            move |m, s| owner.base.log(m, s),
        );
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn get_account(
        self: &Rc<Self>,
    ) -> actions::Task<crate::account_reads::AccountIdentity, ExchangeError> {
        let xhr = self.xhr.clone();
        let owner = self.clone();
        let future = crate::account_reads::get_account(
            move |endpoint| {
                let response = xhr.request(
                    "GET",
                    endpoint,
                    JsJsonValue::Undefined,
                    JsJsonValue::Undefined,
                );
                Box::pin(async move { response.await?.prepared() })
            },
            move |m, s| owner.base.log(m, s),
        );
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn fetch_account_info(self: &Rc<Self>) -> actions::Task<(), ExchangeError> {
        let response = self.xhr.request(
            "GET",
            "/api/v1/account",
            JsJsonValue::Undefined,
            JsJsonValue::Undefined,
        );
        let owner = self.clone();
        let log = owner.clone();
        let future = Box::pin(async move {
            crate::account_reads::fetch_account_info(
                &owner.account_leverage_limit,
                |_| async move { response.await?.prepared() },
                move |m, s| log.base.log(m, s),
            )
            .await
        });
        safe_cex_exchange::eager::start_eager(&self.base.runtime, future)
    }
    pub fn listen_ohlcv(
        self: &Rc<Self>,
        opts: reads::OHLCVOptions,
        callback: crate::public_listeners::CandleCallback<ExchangeError>,
    ) -> Result<crate::public_listeners::Action<ExchangeError>, ExchangeError> {
        self.public_websocket
            .get()
            .expect("constructed public socket")
            .listen_ohlcv(opts.symbol, opts.interval, callback)
    }
    pub fn listen_order_book(
        self: &Rc<Self>,
        symbol: JsString,
        callback: crate::public_listeners::BookCallback<ExchangeError>,
    ) -> Result<crate::public_listeners::Action<ExchangeError>, ExchangeError> {
        self.public_websocket
            .get()
            .expect("constructed public socket")
            .listen_order_book(symbol, callback)
    }
    pub fn map_order(
        self: &Rc<Self>,
        order: &crate::types::BackpackOrder,
    ) -> Result<Order, ExchangeError> {
        let frame = Frame {
            owner: self.clone(),
        };
        crate::order_utils::map_order(
            order,
            |s, side, n| frame.determine(s, side, n),
            |m, s| self.base.log(m, s),
        )
    }
    pub fn map_order_status(&self, status: &str) -> safe_cex_types::OrderStatus {
        crate::order_utils::map_order_status(status)
    }
    pub fn map_time_in_force(&self, time: Option<OrderTimeInForce>) -> &'static str {
        crate::order_utils::map_time_in_force(time)
    }
    pub fn map_backpack_time_in_force(&self, time: &str) -> OrderTimeInForce {
        crate::order_utils::map_backpack_time_in_force(time)
    }
    pub fn determine_trigger_order_type(
        self: &Rc<Self>,
        symbol: &str,
        side: OrderSide,
        n: f64,
    ) -> Result<OrderType, ExchangeError> {
        Frame {
            owner: self.clone(),
        }
        .determine(symbol, side, n)
    }
    pub fn get_trigger_by(
        self: &Rc<Self>,
        tp: bool,
        explicit: Option<OrderStopTriggerType>,
    ) -> JsString {
        Frame {
            owner: self.clone(),
        }
        .reference(tp, explicit)
    }
}

// Native lifetime ruling: an unobserved final handle must not leave live socket
// callbacks referring to a dead weak parent. Explicit source disposal remains
// independently ordered; final destruction additionally closes native resources.
impl Drop for BackpackExchange {
    fn drop(&mut self) {
        safe_cex_exchange::native_final_drop(&self.base, || {
            if let Some(public) = self.public_websocket.get() {
                public.ticker_handler.cancel();
                let _ = public.dispose();
                for task in public.tasks.borrow_mut().drain(..) {
                    task.cancel();
                }
            }
            if let Some(private) = self.private_websocket.get() {
                let _ = private.dispose();
                for task in private.tasks.borrow_mut().drain(..) {
                    task.cancel();
                }
            }
            for task in self.tasks.borrow_mut().drain(..) {
                task.cancel();
            }
            Ok(())
        });
    }
}
