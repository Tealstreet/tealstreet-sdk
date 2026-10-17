//! MEXC account orchestration over the shared exchange and Store contracts.
use crate::{
    exchange_helpers::{
        extract_data, extract_rows, filter_recently_canceled_stop_like_orders, is_plan_order,
        is_recently_canceled_stop_like_order, is_recently_placed_stop_like_order,
        is_recently_rejected_regular_order, is_stop_like_order, is_stop_order,
        mark_stop_like_order_seen, note_recently_canceled_stop_like_orders, order_list_endpoint,
    },
    symbol_config::{MexcSymbolConfig, derive_symbol_config},
    types::{ENDPOINTS, REVERSE_MARGIN_TYPE},
    utils::{
        MexcLeverage, fake_positions_for_market, from_mexc_symbol, is_tradable_mexc_market,
        normalize_balance, normalize_execution, normalize_kline_response, normalize_margin_type,
        normalize_market, normalize_order, normalize_order_book, normalize_plan_order,
        normalize_position, normalize_position_mode, normalize_stop_order, normalize_ticker,
        to_integer_string, to_mexc_symbol, to_number,
    },
};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    error_reporting::ExchangeErrorInput,
    exchange::{BaseExchange, Exchange, ValidatedBatch, ValidatedOrder},
    js_runtime::base_exchange::ExternalPublicDataFactory,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_store::{StoreUpdate, model_patch::ModelPatch};
use safe_cex_transport::{RestAdapter, RestFailure, RestRequest, RestResponseBody};
use safe_cex_types::{
    AccountValidationError, Candle, ExchangeAccount, Execution, ExecutionsOptions, FetchedLeverage,
    MarginType, OHLCVOptions, Order, OrderBook, OrderStatus, PlaceOrderOpts, Position,
    PositionMode, PositionSide, UpdateOrderOpts, ValidateAccountResult,
};
#[cfg(test)]
use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
#[cfg(test)]
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    rc::Rc,
};

// ts: MexcExchange
pub struct MexcExchange {
    base: Rc<BaseExchange>,
    external: ExternalPublicDataFactory,
    capabilities: ExchangeCapabilities,
    xhr: Rc<dyn RestAdapter>,
    local_to_utc: Rc<dyn Fn(f64) -> f64>,
    current_time_ms: Rc<dyn Fn() -> f64>,
    placed: RefCell<BTreeMap<String, f64>>,
    canceled: RefCell<BTreeMap<String, f64>>,
    rejected: RefCell<BTreeMap<String, f64>>,
    symbol_settings: RefCell<BTreeMap<String, MexcSymbolConfig>>,
    account_position_mode: RefCell<PositionMode>,
    public_socket: RefCell<Option<Rc<crate::ws_public::MexcPublicSocket>>>,
    binding: std::cell::OnceCell<Rc<binding::Host>>,
}
#[path = "exchange_binding.rs"]
mod binding;
enum OrderKind {
    Regular,
    Plan,
    Stop,
}
fn parse_response_json(body: &RestResponseBody) -> Result<Value, serde_json::Error> {
    #[cfg(test)]
    if std::env::var_os("MEXC_CONSUME_ARRAYS_RUST_CONTROL").is_some() {
        return match body {
            RestResponseBody::Encoded(bytes) => {
                serde_json::from_slice::<ConsumeArrays>(bytes).map(|value| value.0)
            }
            RestResponseBody::Untransformed(value) | RestResponseBody::Transformed(value) => {
                serde_json::from_value::<ConsumeArrays>(value.clone()).map(|value| value.0)
            }
        };
    }
    body.json_value()
}

#[cfg(test)]
struct ConsumeArrays(Value);

#[cfg(test)]
impl<'de> Deserialize<'de> for ConsumeArrays {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ConsumeArraysVisitor)
    }
}

#[cfg(test)]
struct ConsumeArraysVisitor;

#[cfg(test)]
impl<'de> Visitor<'de> for ConsumeArraysVisitor {
    type Value = ConsumeArrays;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(ConsumeArrays(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(ConsumeArrays(Value::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(ConsumeArrays(Value::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
        Ok(ConsumeArrays(Value::from(value)))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(ConsumeArrays(Value::String(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(ConsumeArrays(Value::String(value)))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(ConsumeArrays(Value::Null))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<IgnoredAny>()?.is_some() {}
        Ok(ConsumeArrays(Value::Array(Vec::new())))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut object = serde_json::Map::new();
        while let Some((key, value)) = map.next_entry::<String, ConsumeArrays>()? {
            object.insert(key, value.0);
        }
        Ok(ConsumeArrays(Value::Object(object)))
    }
}

fn unique_orders(orders: impl IntoIterator<Item = Order>) -> Vec<Order> {
    let mut seen = HashSet::new();
    orders
        .into_iter()
        .filter(|o| seen.insert(o.id.clone()))
        .collect()
}
fn timestamp_seconds(value: f64, local_to_utc: &dyn Fn(f64) -> f64) -> f64 {
    let milliseconds = if value.is_finite() && value > 0.0 {
        if value >= 1_000_000_000_000.0 {
            value.floor()
        } else {
            (value * 1000.0).floor()
        }
    } else {
        let parsed =
            safe_cex_num::js_date_parse(&safe_cex_num::js_number_string(value), local_to_utc);
        if parsed.is_finite() { parsed } else { 0.0 }
    };
    (milliseconds / 1000.0).floor()
}
impl Exchange for MexcExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        MexcExchange::get_account(self)
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        MexcExchange::validate_account(self)
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn fetch_and_set_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        MexcExchange::fetch_and_set_orders(self, symbols)
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        MexcExchange::fetch_orders(self, symbols)
    }
    fn fetch_leverage_for_symbol(
        &self,
        symbol: String,
    ) -> ExchangeFuture<'_, Option<FetchedLeverage>> {
        MexcExchange::fetch_leverage_for_symbol(self, symbol)
    }
    fn update_symbol_metadata(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        MexcExchange::update_symbol_metadata(self, symbol)
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<safe_cex_types::Market>> {
        MexcExchange::fetch_markets(self)
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<safe_cex_types::Ticker>> {
        MexcExchange::fetch_tickers(self)
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        MexcExchange::fetch_positions(self, symbols)
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        // ts: mexc.exchange.ts:599 reads the provider itself, not super.listenOHLCV.
        let provider = self.base.external_public_data_provider.borrow().clone();
        if let Some(provider) = provider {
            return provider.listen_ohlcv(options, callback);
        }
        let socket = self
            .public_socket
            .borrow()
            .clone()
            .ok_or_else(|| ExchangeError::new("MEXC public socket has not been constructed"))?;
        socket.listen_ohlcv(options, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        // ts: mexc.exchange.ts:606 reads the provider itself, not super.listenOrderBook.
        let provider = self.base.external_public_data_provider.borrow().clone();
        if let Some(provider) = provider {
            return provider.listen_order_book(symbol, callback);
        }
        let socket = self
            .public_socket
            .borrow()
            .clone()
            .ok_or_else(|| ExchangeError::new("MEXC public socket has not been constructed"))?;
        socket.listen_order_book(symbol, callback)
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        MexcExchange::fetch_ohlcv(self, options)
    }
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        MexcExchange::fetch_historical_fills(self, options)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        MexcExchange::cancel_orders(self, orders)
    }
    // ts: MexcExchange.placeOrder (mexc.exchange.ts:626); the bound body's first
    // statement is the resolveCallerClientOrderId guard (order_workflow::place_order).
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let opts = binding::value(order.options());
        binding::typed(self.js(), move |h| {
            Ok(crate::order_workflow::place_order(h, opts?))
        })
    }
    // ts: MexcExchange.placeOrders (mexc.exchange.ts:1030); assertCallerClientOrderIds first.
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        let opts: Vec<&PlaceOrderOpts> = orders.orders().iter().map(|o| o.options()).collect();
        let opts = binding::value(&opts);
        binding::typed(self.js(), move |h| {
            Ok(crate::order_workflow::place_orders(h, opts?))
        })
    }
    // ts: MexcExchange.start (mexc.exchange.ts:212)
    fn start(&self) -> ExchangeFuture<'_, ()> {
        binding::unit(self.js(), |h| {
            Ok(crate::runtime_assembly::start_exchange(h))
        })
    }
    // ts: MexcExchange.setLeverage (mexc.exchange.ts:1436)
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        let side = binding::value(&side);
        binding::unit(self.js(), move |h| {
            Ok(crate::exchange_runtime::set_leverage(
                h,
                binding::value(&symbol)?,
                binding::value(&leverage)?,
                side?,
            ))
        })
    }
    // ts: MexcExchange.changePositionMode (mexc.exchange.ts:1460); the TS member takes no symbol.
    fn change_position_mode(
        &self,
        hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        binding::unit(self.js(), move |h| {
            Ok(crate::exchange_runtime::change_position_mode(
                h,
                binding::value(&hedged)?,
            ))
        })
    }
    // ts: MexcExchange.changeMarginType (mexc.exchange.ts:1476)
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let symbol = binding::value(&symbol);
        binding::unit(self.js(), move |h| {
            Ok(crate::exchange_runtime::change_margin_type(
                h,
                binding::value(&isolated)?,
                symbol?,
            ))
        })
    }
    // ts: MexcExchange.updateOrder (mexc.exchange.ts:1144)
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let input = binding::value(&options);
        binding::typed(self.js(), move |h| {
            Ok(crate::runtime_assembly::update_order(h, input?))
        })
    }
    // ts: MexcExchange.cancelSymbolOrders (mexc.exchange.ts:1340)
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        binding::unit(self.js(), move |h| {
            Ok(crate::exchange_runtime::cancel_symbol_orders(
                h,
                binding::value(&symbol)?,
            ))
        })
    }
    // ts: MexcExchange.cancelAllOrders (mexc.exchange.ts:1349)
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        binding::unit(self.js(), |h| {
            Ok(crate::exchange_runtime::cancel_all_orders(h))
        })
    }
    // ts: MexcExchange.dispose (mexc.exchange.ts:1580)
    fn dispose(&self) -> Result<(), ExchangeError> {
        match self.binding.get() {
            Some(host) => binding::dispose(host),
            None => self.base.dispose(),
        }
    }
    // ts: base.ts:799 — MexcExchange inherits fetchSymbolMaxLeverage, which rejects.
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // ts: base.ts:989 — MexcExchange inherits scheduleCancel, which rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // ts: base.ts:1138 — MexcExchange inherits fetchPublicTrades, which rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
}
fn failure_message(error: &RestFailure) -> Result<String, ExchangeError> {
    let message = error
        .response
        .as_ref()
        .and_then(|r| r.body.json_value().ok())
        .and_then(|v| v.get("message").cloned())
        .filter(|v| safe_cex_num::js_truthy(Some(v)));
    match message {
        Some(message) => safe_cex_num::js_string(Some(&message)).map_err(|e| {
            let mut error = ExchangeError::new(e.message);
            error.name = e.name.into();
            error
        }),
        None => Ok(error.message.clone()),
    }
}

fn failure_report_value(error: &RestFailure) -> Value {
    error
        .response
        .as_ref()
        .and_then(|response| response.body.json_value().ok())
        .and_then(|data| data.get("message").cloned())
        .filter(|value| safe_cex_num::js_truthy(Some(value)))
        .unwrap_or_else(|| Value::String(error.message.clone()))
}

impl MexcExchange {
    fn js(&self) -> Option<Rc<binding::Host>> {
        self.binding.get().cloned()
    }
    fn capabilities_table() -> ExchangeCapabilities {
        ExchangeCapabilities {
            start: CapabilityBinding::implemented(|venue, ()| venue.start()),
            place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
            place_orders: PlaceOrdersBinding::implemented::<Self>(Exchange::place_orders),
            fetch_historical_fills: CapabilityBinding::implemented(|venue, options| {
                venue.fetch_historical_fills(options)
            }),
            ..Default::default()
        }
    }
    pub fn new(
        base: BaseExchange,
        xhr: Rc<dyn RestAdapter>,
        local_to_utc: Rc<dyn Fn(f64) -> f64>,
        sockets: crate::sockets::MexcSocketDependencies,
        external: ExternalPublicDataFactory,
    ) -> Rc<Self> {
        let exchange = Rc::new(Self {
            base: Rc::new(base),
            external,
            xhr,
            local_to_utc: local_to_utc.clone(),
            current_time_ms: sockets.current_time_ms.clone(),
            capabilities: Self::capabilities_table(),
            placed: RefCell::default(),
            canceled: RefCell::default(),
            rejected: RefCell::default(),
            symbol_settings: RefCell::default(),
            account_position_mode: RefCell::new(PositionMode::Hedged),
            public_socket: RefCell::default(),
            binding: std::cell::OnceCell::new(),
        });
        let _ = exchange.binding.set(binding::Host::new(&exchange));
        let public = crate::ws_public::MexcPublicProtocol::bind(
            &exchange,
            sockets.runtime.clone(),
            local_to_utc.clone(),
        );
        exchange.base.set_socket(
            safe_cex_exchange::socket_lifecycle::SocketSlot::Public,
            Some(public.base.clone()),
        );
        *exchange.public_socket.borrow_mut() = Some(public);
        let reject = Rc::downgrade(&exchange);
        let seen = Rc::downgrade(&exchange);
        let canceled = Rc::downgrade(&exchange);
        let handlers = crate::ws_private::MexcPrivateHandlers::new_with_fallible_hooks(
            local_to_utc,
            None,
            Rc::new(move |id, code| {
                reject
                    .upgrade()
                    .ok_or_else(|| ExchangeError::new("MEXC parent was dropped"))?
                    .note_rejected_regular_order(&id, code)
            }),
            Rc::new(move |id| {
                let parent = seen
                    .upgrade()
                    .ok_or_else(|| ExchangeError::new("MEXC parent was dropped"))?;
                mark_stop_like_order_seen(&mut parent.placed.borrow_mut(), &id);
                Ok(())
            }),
            Rc::new(move |id| {
                let parent = canceled
                    .upgrade()
                    .ok_or_else(|| ExchangeError::new("MEXC parent was dropped"))?;
                let found = crate::exchange_helpers::is_recently_canceled_stop_like_order_id(
                    &mut parent.canceled.borrow_mut(),
                    id,
                    &|| (parent.current_time_ms)(),
                );
                Ok(found)
            }),
            sockets.emit_fill,
        );
        let private = crate::sockets::PrivateProtocol::bind(
            &exchange,
            sockets.runtime,
            handlers,
            sockets.remote_account_name,
            sockets.current_time_ms,
        );
        exchange.base.set_socket(
            safe_cex_exchange::socket_lifecycle::SocketSlot::Private,
            Some(private),
        );
        exchange
    }
    pub(crate) fn login_signature(
        self: &Rc<Self>,
        api_key: String,
        req_time: String,
        account_name: Option<String>,
    ) -> ExchangeFuture<'static, Option<String>> {
        let owner = self.clone();
        let remote = self.base.signers.remote_signer.clone();
        let local = remote.is_none().then(|| {
            crate::api::sign_mexc_ws_login(&self.base.options().secret, &api_key, &req_time)
        });
        let id = remote.as_ref().map(|_| self.base.runtime.unique_id());
        let mut future = Box::pin(async move {
            if let Some(local) = local {
                return local.map(Some).map_err(ExchangeError::new);
            }
            let remote = remote.expect("MEXC remote signer branch");
            let request = safe_cex_transport::signing::RemoteSignerRequest::WsAuth {
                exchange_name: safe_cex_types::ExchangeName::Mexc,
                account_name,
                request: safe_cex_types::ws_auth::WsAuthRequest {
                    id: id.map(Value::String),
                    request_type: Some(
                        serde_json::value::RawValue::from_string("\"ws-auth\"".into())
                            .expect("WS request type"),
                    ),
                    ws_auth: Some(safe_cex_types::ws_auth::WsAuth {
                        to_sign: Some(format!("{api_key}{req_time}")),
                        output_format: Some(json!("hex")),
                        extra_data: Some(
                            serde_json::value::RawValue::from_string(
                                json!({"reqTime":req_time}).to_string(),
                            )
                            .expect("WS request data"),
                        ),
                    }),
                },
            };
            let response = crate::sockets::reaction(remote.sign(request))
                .await
                .map_err(|error| ExchangeError::new(error.to_string()))?;
            let safe_cex_transport::signing::RemoteSignerResponse::WsAuth(response) = response
            else {
                return Err(ExchangeError::new("MEXC WS signer returned HTTP response"));
            };
            if response.error.as_ref().is_some_and(|s| !s.is_empty())
                || response
                    .ws_auth_signature
                    .as_ref()
                    .is_none_or(|s| s.is_empty())
            {
                owner.base.emit_and_log_error(
                    ExchangeErrorInput::Message(format!(
                        "Remote WS signing unavailable: {}",
                        response.error.as_deref().unwrap_or("no signature returned")
                    )),
                    None,
                    None,
                )?;
                return Ok(None);
            }
            Ok(response.ws_auth_signature)
        });
        use std::future::Future;
        match future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            std::task::Poll::Ready(value) => Box::pin(async move { value }),
            std::task::Poll::Pending => future,
        }
    }

    // ts: MexcExchange.validateAccount
    pub fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        Box::pin(async move {
            match self.fetch_balance().await {
                Ok(balance) => Ok(ValidateAccountResult {
                    error: AccountValidationError::from(String::new()),
                    balance: Some(balance),
                }),
                Err(error) => {
                    let data = error
                        .response
                        .as_ref()
                        .and_then(|response| response.get("data"));
                    let message = data
                        .and_then(|data| data.get("message"))
                        .filter(|value| safe_cex_num::js_truthy(Some(value)))
                        .or_else(|| {
                            data.and_then(|data| data.get("msg"))
                                .filter(|value| safe_cex_num::js_truthy(Some(value)))
                        })
                        .cloned()
                        .unwrap_or_else(|| {
                            if error.message.is_empty() {
                                json!("Invalid MEXC key")
                            } else {
                                Value::String(error.message)
                            }
                        });
                    self.base.emit_and_log_error(
                        ExchangeErrorInput::Raw(message.clone()),
                        None,
                        None,
                    )?;
                    Ok(ValidateAccountResult {
                        error: AccountValidationError::from_value(message),
                        balance: None,
                    })
                }
            }
        })
    }

    // ts: MexcExchange.getAccount
    pub fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        Box::pin(async move {
            let validation = self.validate_account().await?;
            if safe_cex_num::js_truthy(Some(validation.error.as_value())) {
                let message = safe_cex_num::js_string(Some(validation.error.as_value()))
                    .map_err(crate::exchange_errors::coercion)?;
                return Err(ExchangeError::new(message));
            }
            let options = self.base.options();
            let key = if safe_cex_num::js_truthy(Some(&Value::String(options.key.clone()))) {
                options.key
            } else {
                "mexc".into()
            };
            #[cfg(test)]
            let key = if std::env::var_os("MEXC_ACCOUNT_RUST_CONTROL").is_some() {
                "wrong-key".into()
            } else {
                key
            };
            let digest = Sha256::digest(key.as_bytes());
            let suffix = digest[..12]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            Ok(ExchangeAccount {
                user_id: format!("mexc:{suffix}"),
                sub_id: None,
                affiliate_id: None,
            })
        })
    }

    // ts: MexcExchange.cancelOrders
    pub fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            if orders.is_empty() {
                return Ok(());
            }

            let stop_orders: Vec<_> = orders
                .iter()
                .filter(|order| {
                    #[cfg(test)]
                    if std::env::var("MEXC_CANCEL_ORDERS_RUST_CONTROL").as_deref()
                        == Ok("stop-kind")
                    {
                        return is_stop_like_order(order);
                    }
                    is_stop_order(order)
                })
                .cloned()
                .collect();
            let plan_orders: Vec<_> = orders
                .iter()
                .filter(|order| is_plan_order(order))
                .cloned()
                .collect();
            let regular_orders: Vec<_> = orders
                .iter()
                .filter(|order| !is_stop_like_order(order))
                .cloned()
                .collect();
            let mut accepted_stop_like_cancels = Vec::new();

            for lot in stop_orders.chunks(50) {
                let payload = Value::Array(lot.iter().map(|order| {
                    json!({"stopPlanOrderId": order.id.split("__").next().unwrap_or_default()})
                }).collect());
                let payload = self
                    .post_cancel_payload("STOP_ORDER_CANCEL", payload)
                    .await?;
                crate::exchange_errors::assert_common_success(
                    Some(&payload),
                    "MEXC position TP/SL cancellation",
                )?;
                accepted_stop_like_cancels.extend_from_slice(lot);
            }

            for lot in plan_orders.chunks(50) {
                let payload = Value::Array(
                    lot.iter()
                        .map(|order| {
                            json!({
                                "symbol": to_mexc_symbol(&order.symbol),
                                "orderId": order.id.split("__").next().unwrap_or_default()
                            })
                        })
                        .collect(),
                );
                let payload = self
                    .post_cancel_payload("PLAN_ORDER_CANCEL", payload)
                    .await?;
                crate::exchange_errors::assert_common_success(
                    Some(&payload),
                    "MEXC plan order cancellation",
                )?;
                accepted_stop_like_cancels.extend_from_slice(lot);
            }

            for lot in regular_orders.chunks(50) {
                let payload = Value::Array(
                    lot.iter()
                        .map(|order| {
                            Value::String(
                                order.id.split("__").next().unwrap_or_default().to_owned(),
                            )
                        })
                        .collect(),
                );
                let payload = self.post_cancel_payload("ORDER_CANCEL", payload).await?;
                crate::exchange_errors::assert_regular_cancel_succeeded(Some(&payload))?;
            }

            note_recently_canceled_stop_like_orders(
                &mut self.canceled.borrow_mut(),
                &mut self.placed.borrow_mut(),
                &accepted_stop_like_cancels,
                &|| (self.current_time_ms)(),
            );
            let ids: Vec<_> = orders.iter().map(|order| order.id.as_str()).collect();
            self.base
                .store
                .update_orders_status(&ids, OrderStatus::Canceled)
                .map_err(ExchangeError::from)?;
            Ok(())
        })
    }

    async fn post_cancel_payload(
        &self,
        endpoint: &str,
        body: Value,
    ) -> Result<Value, ExchangeError> {
        let url = ENDPOINTS[endpoint]
            .as_str()
            .ok_or_else(|| ExchangeError::new(format!("Missing MEXC {endpoint} endpoint")))?;
        let request = RestRequest {
            method: "post".into(),
            url: url.into(),
            body: Some(safe_cex_transport::RestBody::Untransformed(body)),
            ..Default::default()
        };
        let response = self
            .xhr
            .execute(request)
            .await
            .map_err(ExchangeError::from)?;
        parse_response_json(&response.body).map_err(|error| {
            let mut exchange_error = ExchangeError::new(error.to_string());
            exchange_error.name = "SyntaxError".into();
            exchange_error
        })
    }

    // ts: MexcExchange.fetchOHLCV
    pub fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        Box::pin(async move {
            let interval = match options.interval {
                safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M1) => "Min1",
                safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M5) => "Min5",
                safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M15) => "Min15",
                safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M30) => "Min30",
                safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H1) => "Min60",
                safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H4) => "Hour4",
                safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::D1) => "Day1",
                safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::W1) => "Week1",
                unsupported => {
                    let literal = serde_json::to_value(unsupported)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .unwrap_or_default();
                    return Err(ExchangeError::new(format!(
                        "MEXC does not support {literal} candles"
                    )));
                }
            };
            let mut params = serde_json::Map::new();
            params.insert("interval".into(), Value::String(interval.into()));
            if let Some(from) = options.from {
                params.insert(
                    "start".into(),
                    Value::from(timestamp_seconds(from, self.local_to_utc.as_ref())),
                );
            }
            if let Some(to) = options.to {
                params.insert(
                    "end".into(),
                    Value::from(timestamp_seconds(to, self.local_to_utc.as_ref())),
                );
            }
            let request = RestRequest {
                method: "get".into(),
                url: format!(
                    "{}/{}",
                    ENDPOINTS["KLINE"].as_str().unwrap_or_default(),
                    to_mexc_symbol(&options.symbol)
                ),
                timeout_ms: Some(5 * 60 * 1000),
                params: Some(Value::Object(params)),
                ..Default::default()
            };
            let response = self
                .xhr
                .execute(request)
                .await
                .map_err(ExchangeError::from)?;
            let payload: Value = parse_response_json(&response.body)
                .map_err(|error| ExchangeError::new(error.to_string()))?;
            let mut candles = normalize_kline_response(&payload, self.local_to_utc.as_ref())
                .map_err(|error| ExchangeError::new(error.message))?;
            candles.sort_by(|left, right| {
                left.timestamp
                    .partial_cmp(&right.timestamp)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            #[cfg(test)]
            match std::env::var("MEXC_FETCH_OHLCV_RUST_CONTROL").as_deref() {
                Ok("reverse-sort") => candles.reverse(),
                Ok("empty") => candles.clear(),
                Ok("drop-one") => {
                    candles.pop();
                }
                Ok("extra") if !candles.is_empty() => candles.push(candles[0].clone()),
                _ => {}
            }
            Ok(candles)
        })
    }

    // ts: MexcExchange.fetchHistoricalFills
    pub fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        Box::pin(async move {
            let Some(symbol) = options.symbol.filter(|symbol| !symbol.is_empty()) else {
                return Ok(vec![]);
            };
            if Exchange::is_public_data_only(self) {
                return Ok(vec![]);
            }

            let endpoint = ENDPOINTS["ORDER_DEALS"]
                .as_str()
                .ok_or_else(|| ExchangeError::new("Missing MEXC order deals endpoint"))?;
            let limit = options
                .limit
                .filter(|value| *value != 0.0 && !value.is_nan())
                .unwrap_or(100.0)
                .clamp(1.0, 100.0);
            use safe_cex_types::QueryValue as Q;
            let mut params: safe_cex_types::object_map::JsObjectMap<Q> = Default::default();
            params.insert("symbol".into(), Q::String(to_mexc_symbol(&symbol)));
            if let Some(order_id) = options.order_id {
                params.insert("order_id".into(), Q::String(order_id));
            }
            if let Some(from) = options.from {
                params.insert("start_time".into(), Q::Number(from));
            }
            if let Some(to) = options.to {
                params.insert("end_time".into(), Q::Number(to));
            }
            params.insert("page_num".into(), Q::Number(1.0));
            params.insert("page_size".into(), Q::Number(limit));
            let request = RestRequest {
                method: "get".into(),
                url: endpoint.into(),
                query_params: Some(Q::Object(params)),
                ..Default::default()
            };
            let response = self
                .xhr
                .execute(request)
                .await
                .map_err(ExchangeError::from)?;
            let payload: Value = parse_response_json(&response.body).map_err(|error| {
                let mut exchange_error = ExchangeError::new(error.to_string());
                exchange_error.name = "SyntaxError".into();
                exchange_error
            })?;
            let mut fills = Vec::new();
            let mut output_refusal = None;
            for row in extract_rows(Some(&payload), &["orders".into(), "items".into()]) {
                // The source reads the Store getter once per row, before reading
                // row.symbol. Release its borrow before entering the normalizer.
                let markets = self.base.store.markets_by_symbol();
                if row.is_null() {
                    let mut error =
                        ExchangeError::new("Cannot read properties of null (reading 'symbol')");
                    error.name = "TypeError".into();
                    return Err(error);
                }
                let empty = serde_json::Map::new();
                let raw = row.as_object().unwrap_or(&empty);
                let symbol = from_mexc_symbol(raw.get("symbol"))
                    .map_err(crate::exchange_errors::coercion)?;
                let market = markets.get(&symbol).cloned();
                drop(markets);
                let normalized =
                    match normalize_execution(raw, market.as_ref(), self.local_to_utc.as_ref()) {
                        Err(error) if error == crate::utils::EXECUTION_ASSET_BOUNDARY => {
                            // The source accepts this row and continues. Keep reading
                            // later rows so their original errors and getter calls win.
                            output_refusal.get_or_insert(error);
                            continue;
                        }
                        result => result.map_err(crate::exchange_errors::coercion)?,
                    };
                if let Some(fill) = normalized {
                    #[cfg(test)]
                    let fill = {
                        let mut fill = fill;
                        if std::env::var("MEXC_FETCH_FILLS_RUST_CONTROL").as_deref() == Ok("swap") {
                            std::mem::swap(&mut fill.commission, &mut fill.realized_pnl);
                        }
                        fill
                    };
                    // The cfg(test) arm above shadows `fill` to run the
                    // MEXC_FETCH_FILLS_RUST_CONTROL swap; this is its production
                    // counterpart, kept so the pair reads as one hook.
                    #[cfg(not(test))]
                    #[allow(clippy::redundant_locals)]
                    let fill = fill;
                    fills.push(fill);
                }
            }
            if let Some(error) = output_refusal {
                return Err(crate::exchange_errors::coercion(error));
            }
            fills.sort_by(|left, right| {
                right
                    .timestamp
                    .partial_cmp(&left.timestamp)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            #[cfg(test)]
            match std::env::var("MEXC_FETCH_FILLS_RUST_CONTROL").as_deref() {
                Ok("empty") => fills.clear(),
                Ok("drop-one") => {
                    fills.pop();
                }
                Ok("extra") => {
                    if let Some(fill) = fills.first().cloned() {
                        fills.push(fill);
                    }
                }
                _ => {}
            }
            Ok(fills)
        })
    }

    // ts: MexcExchange.fetchPositions
    pub fn fetch_positions(
        &self,
        symbols: Option<Vec<String>>,
    ) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(async move {
            if Exchange::is_public_data_only(self) {
                return Ok(vec![]);
            }
            let request = RestRequest {
                method: "get".into(),
                url: ENDPOINTS["OPEN_POSITIONS"]
                    .as_str()
                    .ok_or_else(|| ExchangeError::new("Missing MEXC open positions endpoint"))?
                    .into(),
                ..Default::default()
            };
            let result = async {
                let response = self
                    .xhr
                    .execute(request.clone())
                    .await
                    .map_err(|error| failure_report_value(&error))?;
                let payload: Value =
                    parse_response_json(&response.body).map_err(|error| error.to_string())?;
                let requested_symbols = symbols
                    .as_ref()
                    .filter(|symbols| !symbols.is_empty())
                    .cloned()
                    .unwrap_or_else(|| {
                        self.base
                            .store
                            .markets()
                            .iter()
                            .map(|market| market.symbol.clone())
                            .collect()
                    });
                let requested: HashSet<_> = requested_symbols.iter().cloned().collect();

                // Match source .map(...).filter(...): normalize every source row
                // before discarding symbols outside the requested set.
                let mut open_positions = Vec::new();
                for row in extract_rows(Some(&payload), &[]) {
                    let market = {
                        let markets = self.base.store.markets_by_symbol();
                        if row.is_null() {
                            return Err("Cannot read properties of null (reading 'symbol')".into());
                        }
                        // ts-bug: fetchPositions looks up only symbol, even when
                        // normalizePosition subsequently accepts contract.
                        let symbol = from_mexc_symbol(row.get("symbol"))
                            .map_err(|error| error.message.to_string())?;
                        markets.get(&symbol).cloned()
                    };
                    let raw = row.as_object().cloned().unwrap_or_default();
                    if let Some(position) = normalize_position(
                        &raw,
                        market.as_ref(),
                        Some(*self.account_position_mode.borrow()),
                        self.local_to_utc.as_ref(),
                    )
                    .map_err(|error| error.message.to_string())?
                    .filter(|position| requested.contains(&position.symbol))
                    {
                        open_positions.push(position);
                    }
                }

                let open_ids: HashSet<_> = open_positions
                    .iter()
                    .map(|position| position.id.clone())
                    .collect();
                let mut all_positions = open_positions;
                for symbol in requested_symbols {
                    let market = self.base.store.markets_by_symbol().get(&symbol).cloned();
                    let Some(market) = market else {
                        continue;
                    };
                    let config = self.symbol_settings.borrow().get(&symbol).cloned();
                    let mode = config
                        .as_ref()
                        .map_or(*self.account_position_mode.borrow(), |config| {
                            config.position_mode
                        });
                    let margin = config
                        .as_ref()
                        .map_or(MarginType::Cross, |config| config.margin_type);
                    let leverage = config.as_ref().map(|config| MexcLeverage::Rails {
                        long: Some(config.buy_leverage),
                        short: Some(config.sell_leverage),
                    });
                    all_positions.extend(
                        fake_positions_for_market(
                            &market,
                            Some(mode),
                            Some(margin),
                            leverage.as_ref(),
                        )
                        .into_iter()
                        .filter(|position| !open_ids.contains(&position.id)),
                    );
                }

                let mut seen = HashSet::new();
                all_positions.retain(|position| seen.insert(position.id.clone()));
                #[cfg(test)]
                match std::env::var("MEXC_FETCH_POSITIONS_RUST_CONTROL").as_deref() {
                    Ok("empty") => all_positions.clear(),
                    Ok("drop-one") => {
                        all_positions.pop();
                    }
                    Ok("extra") if !all_positions.is_empty() => {
                        let mut extra = all_positions[0].clone();
                        extra.id.push_str("-extra");
                        all_positions.push(extra);
                    }
                    _ => {}
                }
                Ok::<_, Value>(all_positions)
            }
            .await;

            match result {
                Ok(positions) => Ok(positions),
                Err(message) => {
                    self.base
                        .emit_and_log_error(ExchangeErrorInput::Raw(message), None, None)?;
                    let positions = self.base.store.positions();
                    Ok(match symbols {
                        Some(symbols) => positions
                            .iter()
                            .filter(|position| symbols.contains(&position.symbol))
                            .cloned()
                            .collect(),
                        None => positions.to_vec(),
                    })
                }
            }
        })
    }

    // ts: MexcExchange.fetchPositionMode
    pub fn fetch_position_mode(&self) -> ExchangeFuture<'_, PositionMode> {
        Box::pin(async move {
            let request = RestRequest {
                method: "get".into(),
                url: ENDPOINTS["POSITION_MODE"]
                    .as_str()
                    .ok_or_else(|| ExchangeError::new("Missing MEXC position mode endpoint"))?
                    .into(),
                ..Default::default()
            };
            let response = self
                .xhr
                .execute(request)
                .await
                .map_err(ExchangeError::from)?;
            let payload: Value = parse_response_json(&response.body).map_err(|error| {
                let mut exchange_error = ExchangeError::new(error.to_string());
                exchange_error.name = "SyntaxError".into();
                exchange_error
            })?;
            normalize_position_mode(extract_data(Some(&payload))).map_err(|error| {
                let mut exchange_error = ExchangeError::new(error.message);
                exchange_error.name = error.name.into();
                exchange_error
            })
        })
    }

    // ts: MexcExchange.fetchBalance
    pub fn fetch_balance(&self) -> ExchangeFuture<'_, safe_cex_types::Balance> {
        Box::pin(async move {
            let endpoint = ENDPOINTS["ASSETS"]
                .as_str()
                .ok_or_else(|| ExchangeError::new("Missing MEXC assets endpoint"))?;
            let request = RestRequest {
                method: "get".into(),
                url: endpoint.into(),
                ..Default::default()
            };
            let response = self
                .xhr
                .execute(request)
                .await
                .map_err(ExchangeError::from)?;
            let payload: Value = parse_response_json(&response.body).map_err(|error| {
                let mut exchange_error = ExchangeError::new(error.to_string());
                exchange_error.name = "SyntaxError".into();
                exchange_error
            })?;
            let rows = extract_rows(Some(&payload), &[]).to_vec();
            normalize_balance(&Value::Array(rows)).map_err(|error| {
                let mut exchange_error = ExchangeError::new(error.message);
                exchange_error.name = error.name.into();
                exchange_error
            })
        })
    }

    // ts: MexcExchange.fetchTickers
    pub fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<safe_cex_types::Ticker>> {
        Box::pin(async move {
            let endpoint = ENDPOINTS["TICKER"]
                .as_str()
                .ok_or_else(|| ExchangeError::new("Missing MEXC ticker endpoint"))?;
            let request = RestRequest {
                method: "get".into(),
                url: endpoint.into(),
                ..Default::default()
            };
            let result = async {
                let response = self
                    .xhr
                    .execute(request.clone())
                    .await
                    .map_err(|error| failure_report_value(&error))?;
                let payload: Value =
                    parse_response_json(&response.body).map_err(|error| error.to_string())?;
                let mut rows = BTreeMap::new();
                for row in extract_rows(Some(&payload), &[]) {
                    let object = row
                        .as_object()
                        .ok_or_else(|| "MEXC ticker row is not an object".to_owned())?;
                    let id = safe_cex_num::js_string(object.get("symbol"))
                        .map_err(|error| error.message.to_string())?;
                    rows.insert(id, object.clone());
                }
                let mut tickers = Vec::new();
                for market in self.base.store.markets().iter() {
                    if let Some(raw) = rows.get(&market.id) {
                        tickers.push(
                            normalize_ticker(raw, market, self.local_to_utc.as_ref())
                                .map_err(|error| error.message.to_string())?,
                        );
                    }
                }
                Ok::<_, Value>(tickers)
            }
            .await;

            match result {
                Ok(tickers) => Ok(tickers),
                Err(message) => {
                    self.base
                        .emit_and_log_error(ExchangeErrorInput::Raw(message), None, None)?;
                    Ok(self.base.store.tickers().to_vec())
                }
            }
        })
    }

    // ts: MexcExchange.fetchOrderBook
    pub fn fetch_order_book(
        &self,
        symbol: String,
        limit: Option<f64>,
    ) -> ExchangeFuture<'_, (OrderBook, String)> {
        Box::pin(async move {
            // Own the market BEFORE the request. `markets_by_symbol()` hands
            // back a RefCell `Ref`, and borrowing out of it kept that guard
            // alive across the `.await` below, so a concurrent `store.update`
            // calling `borrow_mut` while this future was suspended would panic.
            // The exchange-core contract is explicit that no RefCell borrow
            // crosses a callback. An owned snapshot is also this venue's
            // declared choice for Market (see CLAUDE.md on
            // owned-snapshot-vs-live-alias), so this is not a new divergence.
            let market = {
                let markets = self.base.store.markets_by_symbol();
                markets
                    .get(&symbol)
                    .ok_or_else(|| ExchangeError::new(format!("Market {symbol} not found")))?
                    .clone()
            };
            let mut params = serde_json::Map::new();
            params.insert("limit".into(), json!(limit.unwrap_or(100.0)));
            let request = RestRequest {
                method: "get".into(),
                url: format!(
                    "{}/{}",
                    ENDPOINTS["DEPTH"].as_str().unwrap_or_default(),
                    market.id
                ),
                params: Some(Value::Object(params)),
                ..Default::default()
            };
            let response = self.xhr.execute(request).await.map_err(|error| {
                ExchangeError::new(
                    failure_message(&error).unwrap_or_else(|message| message.message),
                )
            })?;
            let payload: Value = parse_response_json(&response.body)
                .map_err(|error| ExchangeError::new(error.to_string()))?;
            let raw = payload
                .as_object()
                .ok_or_else(|| ExchangeError::new("MEXC order book response is not an object"))?;
            let book = normalize_order_book(raw, &market)
                .map_err(|error| ExchangeError::new(error.message))?;
            #[cfg(test)]
            let book = {
                let mut book = book;
                match std::env::var("MEXC_FETCH_ORDER_BOOK_RUST_CONTROL").as_deref() {
                    Ok("empty") => {
                        book.bids.clear();
                        book.asks.clear();
                    }
                    Ok("drop-one") => {
                        book.bids.pop();
                    }
                    Ok("extra") => {
                        if let Some(level) = book.bids.first().cloned() {
                            book.bids.push(level);
                        }
                    }
                    _ => {}
                }
                book
            };
            let data = extract_data(Some(&payload));
            let last_update_id =
                to_integer_string(data.as_ref().and_then(|value| value.get("version")))
                    .map_err(|error| ExchangeError::new(error.message))?;
            Ok((book, last_update_id))
        })
    }

    // ts: MexcExchange.fetchMarkets
    pub fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<safe_cex_types::Market>> {
        Box::pin(async move {
            let endpoint = ENDPOINTS["CONTRACT_DETAIL"]
                .as_str()
                .ok_or_else(|| ExchangeError::new("Missing MEXC contract detail endpoint"))?;
            let request = RestRequest {
                method: "get".into(),
                url: endpoint.into(),
                ..Default::default()
            };
            let result = async {
                let response = self
                    .xhr
                    .execute(request.clone())
                    .await
                    .map_err(|error| failure_report_value(&error))?;
                let payload: Value =
                    parse_response_json(&response.body).map_err(|error| error.to_string())?;
                let rows = extract_rows(Some(&payload), &[]);

                // The source completes filter() over every row before beginning
                // map(). Preserve that stage ordering so a later predicate error
                // wins over an earlier normalization error.
                let mut tradable = Vec::new();
                for row in rows {
                    let object = row
                        .as_object()
                        .ok_or_else(|| "MEXC market row is not an object".to_owned())?;
                    if is_tradable_mexc_market(object).map_err(|error| error.message.to_string())? {
                        tradable.push(object.clone());
                    }
                }
                let mut markets = Vec::with_capacity(tradable.len());
                for row in &tradable {
                    markets.push(normalize_market(row).map_err(|error| error.message.to_string())?);
                }
                Ok::<_, Value>(markets)
            }
            .await;

            match result {
                Ok(markets) => Ok(markets),
                Err(message) => {
                    self.base
                        .emit_and_log_error(ExchangeErrorInput::Raw(message), None, None)?;
                    Ok(self.base.store.markets().to_vec())
                }
            }
        })
    }

    // ts: MexcExchange.noteRejectedRegularOrder
    pub fn note_rejected_regular_order(
        &self,
        order_id: &str,
        error_code: f64,
    ) -> Result<(), ExchangeError> {
        if order_id.is_empty() {
            return Ok(());
        }
        let clean_id = order_id.split("__").next().unwrap_or_default();
        self.rejected
            .borrow_mut()
            .insert(clean_id.to_owned(), (self.current_time_ms)());

        if self
            .base
            .store
            .orders()
            .iter()
            .any(|order| order.id == clean_id && order.status != OrderStatus::Canceled)
        {
            // Propagation stops before the source's rejection-error emission.
            self.base
                .store
                .update_order_status(clean_id, OrderStatus::Canceled)?;
        }

        let reason = match error_code {
            6.0 => Some("total reduce-only close exceeds open short position"),
            7.0 => Some("total reduce-only close exceeds open long position"),
            8.0 => Some("order rejected by venue"),
            10.0 => Some(
                "venue rejected the order (commonly: reduce-only would over-close, or post-only would cross the book)",
            ),
            11.0 => Some("IOC / FOK order canceled by matching rule"),
            _ => None,
        };
        let detail = reason.map_or_else(
            || format!("errorCode {}", safe_cex_num::js_number_string(error_code)),
            |reason| {
                format!(
                    "{reason} (errorCode {})",
                    safe_cex_num::js_number_string(error_code)
                )
            },
        );
        self.base.emit_and_log_error(
            ExchangeErrorInput::Message(format!("MEXC canceled order {clean_id}: {detail}")),
            None,
            None,
        )?;
        Ok(())
    }

    // ts: MexcExchange.updateSymbolMetadata
    pub fn update_symbol_metadata(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.fetch_leverage_for_symbol(symbol).await?;
            Ok(())
        })
    }

    // ts: MexcExchange.fetchLeverageForSymbol
    pub fn fetch_leverage_for_symbol(
        &self,
        symbol: String,
    ) -> ExchangeFuture<'_, Option<FetchedLeverage>> {
        Box::pin(async move {
            let market = self.base.store.markets_by_symbol().get(&symbol).cloned();
            let config = self.symbol_settings.borrow().get(&symbol).cloned();
            let config = match config {
                Some(config) => config,
                None => {
                    let positions = self.base.store.positions().to_vec();
                    derive_symbol_config(
                        &symbol,
                        &positions,
                        None,
                        *self.account_position_mode.borrow(),
                    )
                    .map_err(|error| ExchangeError::new(error.message))?
                }
            };

            if let Some(market) = market.filter(|_| !self.base.is_public_data_only(self)) {
                let request = RestRequest {
                    method: "get".into(),
                    url: ENDPOINTS["POSITION_LEVERAGE"]
                        .as_str()
                        .ok_or_else(|| ExchangeError::new("Missing MEXC leverage endpoint"))?
                        .into(),
                    params: Some(json!({"symbol": market.id})),
                    ..Default::default()
                };
                match self.xhr.execute(request.clone()).await {
                    Ok(response) => {
                        let result = (|| -> Result<_, ExchangeError> {
                            let data: Value = parse_response_json(&response.body)
                                .map_err(|e| ExchangeError::new(e.to_string()))?;
                            let rows = extract_rows(Some(&data), &[]);
                            let mut long = None;
                            for row in rows {
                                if to_number(row.get("positionType"), Some(f64::NAN))
                                    .map_err(|e| ExchangeError::new(e.message))?
                                    == 1.0
                                {
                                    long = Some(row);
                                    break;
                                }
                            }
                            let mut short = None;
                            for row in rows {
                                if to_number(row.get("positionType"), Some(f64::NAN))
                                    .map_err(|e| ExchangeError::new(e.message))?
                                    == 2.0
                                {
                                    short = Some(row);
                                    break;
                                }
                            }
                            let buy = to_number(
                                long.and_then(|row| row.get("leverage")),
                                Some(config.buy_leverage),
                            )
                            .map_err(|e| ExchangeError::new(e.message))?;
                            let sell = to_number(
                                short.and_then(|row| row.get("leverage")),
                                Some(config.sell_leverage),
                            )
                            .map_err(|e| ExchangeError::new(e.message))?;
                            let mut max_candidates = Vec::new();
                            for row in rows {
                                let raw = row
                                    .get("maxLeverageView")
                                    .filter(|v| !v.is_null())
                                    .or_else(|| row.get("maxLeverage"));
                                let value = to_number(raw, Some(f64::NAN))
                                    .map_err(|e| ExchangeError::new(e.message))?;
                                if value.is_finite() {
                                    max_candidates.push(value);
                                }
                            }
                            let margin_raw = long
                                .and_then(|row| row.get("openType"))
                                .filter(|v| !v.is_null())
                                .or_else(|| short.and_then(|row| row.get("openType")))
                                .filter(|v| !v.is_null())
                                .or_else(|| {
                                    REVERSE_MARGIN_TYPE.get(match config.margin_type {
                                        MarginType::Isolated => "isolated",
                                        MarginType::Cross => "cross",
                                    })
                                });
                            let margin = normalize_margin_type(margin_raw)
                                .map_err(|e| ExchangeError::new(e.message))?;
                            Ok((
                                buy,
                                sell,
                                max_candidates.into_iter().reduce(f64::min),
                                margin,
                            ))
                        })();
                        match result {
                            Ok((buy, sell, max, margin)) => {
                                let next = MexcSymbolConfig {
                                    buy_leverage: buy,
                                    sell_leverage: sell,
                                    margin_type: margin,
                                    ..config.clone()
                                };
                                if let Some(max) =
                                    max.filter(|v| *v > 0.0 && *v != market.limits.leverage.max)
                                {
                                    let mut limits = market.limits.clone();
                                    limits.leverage.max = max;
                                    let mut patch = ModelPatch::default();
                                    patch.set_native("limits", limits);
                                    if let Err(error) =
                                        self.base.store.update_market(&market.id, &patch)
                                    {
                                        self.base.log(
                                            &format!(
                                                "Failed to fetch MEXC leverage cap for {symbol}: {error}"
                                            ),
                                            "warning",
                                        )?;
                                    } else {
                                        self.symbol_settings
                                            .borrow_mut()
                                            .insert(symbol.clone(), next);
                                        let fetched = FetchedLeverage {
                                            buy_leverage: buy,
                                            sell_leverage: sell,
                                            margin_type: Some(margin),
                                            position_mode: Some(config.position_mode),
                                            is_fetching: None,
                                        };
                                        self.update_fetched_leverage(&symbol, fetched.clone())?;
                                        return Ok(Some(fetched));
                                    }
                                } else {
                                    self.symbol_settings
                                        .borrow_mut()
                                        .insert(symbol.clone(), next);
                                    let fetched = FetchedLeverage {
                                        buy_leverage: buy,
                                        sell_leverage: sell,
                                        margin_type: Some(margin),
                                        position_mode: Some(config.position_mode),
                                        is_fetching: None,
                                    };
                                    self.update_fetched_leverage(&symbol, fetched.clone())?;
                                    return Ok(Some(fetched));
                                }
                            }
                            Err(error) => self.base.log(
                                &format!(
                                    "Failed to fetch MEXC leverage cap for {symbol}: {}",
                                    error.message
                                ),
                                "warning",
                            )?,
                        }
                    }
                    Err(error) => self.base.log(
                        &format!(
                            "Failed to fetch MEXC leverage cap for {symbol}: {}",
                            error.message
                        ),
                        "warning",
                    )?,
                }
            }
            let fetched = FetchedLeverage {
                buy_leverage: config.buy_leverage,
                sell_leverage: config.sell_leverage,
                margin_type: Some(config.margin_type),
                position_mode: Some(config.position_mode),
                is_fetching: None,
            };
            self.update_fetched_leverage(&symbol, fetched.clone())?;
            Ok(Some(fetched))
        })
    }

    fn update_fetched_leverage(
        &self,
        symbol: &str,
        leverage: FetchedLeverage,
    ) -> Result<(), ExchangeError> {
        let mut fetched = self.base.store.fetched_leverage().clone();
        fetched.insert(symbol.to_owned(), leverage);
        self.base.store.update(
            StoreUpdate {
                fetched_leverage: Some(fetched),
                ..Default::default()
            },
            false,
        )?;
        Ok(())
    }
    // ts: MexcExchange.fetchAndSetOrders
    pub fn fetch_and_set_orders(
        &self,
        symbols: Option<Vec<String>>,
    ) -> ExchangeFuture<'_, Vec<Order>> {
        self.reconcile_orders(symbols.clone(), self.fetch_orders(symbols), false)
    }
    // ts: MexcExchange.fetchOrders
    pub fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(async move {
            if Exchange::is_public_data_only(self) {
                return Ok(vec![]);
            }
            let mut orders = self.fetch_regular_orders(symbols.clone()).await?;
            let scopes: Vec<_> = symbols
                .filter(|s| !s.is_empty())
                .map(|s| s.into_iter().map(Some).collect())
                .unwrap_or_else(|| vec![None]);
            for symbol in scopes {
                let venue = symbol.filter(|s| !s.is_empty()).map(|s| to_mexc_symbol(&s));
                for (endpoint, label, plan) in [
                    ("PLAN_ORDER_LIST", "plan orders", true),
                    ("STOP_ORDER_LIST", "position TP/SL orders", false),
                ] {
                    let now = (self.current_time_ms)();
                    let mut params = json!({"start_time":now - 89.0*24.0*60.0*60.0*1000.0,"end_time":now,"page_num":1,"page_size":100});
                    if let Some(symbol) = &venue {
                        params["symbol"] = json!(symbol);
                    }
                    params[if plan { "states" } else { "is_finished" }] =
                        if plan { json!("1") } else { json!(0) };
                    let url = ENDPOINTS[endpoint]
                        .as_str()
                        .ok_or_else(|| ExchangeError::new("Missing MEXC endpoint"))?;
                    match self
                        .fetch_order_rows(
                            url.to_owned(),
                            params,
                            if plan {
                                OrderKind::Plan
                            } else {
                                OrderKind::Stop
                            },
                        )
                        .await
                    {
                        Ok(rows) => orders.extend(rows),
                        Err(error) => self.base.log(
                            &format!("Failed to fetch MEXC {label}: {}", failure_message(&error)?),
                            "warning",
                        )?,
                    }
                }
            }
            Ok(filter_recently_canceled_stop_like_orders(
                &mut self.canceled.borrow_mut(),
                &unique_orders(orders),
                &|| (self.current_time_ms)(),
            ))
        })
    }
    async fn fetch_order_rows(
        &self,
        url: String,
        params: Value,
        kind: OrderKind,
    ) -> Result<Vec<Order>, RestFailure> {
        let request = RestRequest {
            method: "get".into(),
            url,
            params: Some(params),
            ..Default::default()
        };
        let response = self.xhr.execute(request.clone()).await?;
        let data: Value = parse_response_json(&response.body)
            .map_err(|e| RestFailure::pre_transmit(e.to_string(), request.clone()))?;
        let mut orders = vec![];
        // No Store borrow survives the request await or a Store mutation.
        for row in extract_rows(Some(&data), &["orders".into(), "items".into()]) {
            if row.is_null() {
                return Err(RestFailure::pre_transmit(
                    "Cannot read properties of null (reading 'symbol')".into(),
                    request.clone(),
                ));
            }
            // JSON scalars/arrays have no symbol/id fields; source property
            // access boxes them and the mapper rejects the missing identity.
            let empty = serde_json::Map::new();
            let raw = row.as_object().unwrap_or(&empty);
            let symbol = from_mexc_symbol(raw.get("symbol"))
                .map_err(|e| RestFailure::pre_transmit(e.message.into(), request.clone()))?;
            let markets = self.base.store.markets_by_symbol();
            let market = markets.get(&symbol);
            let normalized = match kind {
                OrderKind::Regular => normalize_order(raw, market, self.local_to_utc.as_ref()),
                OrderKind::Plan => normalize_plan_order(raw, market, self.local_to_utc.as_ref()),
                OrderKind::Stop => normalize_stop_order(raw, market, self.local_to_utc.as_ref()),
            }
            .map_err(|e| RestFailure::pre_transmit(e.message.into(), request.clone()))?;
            if let Some(order) = normalized {
                orders.push(order);
            }
        }
        Ok(orders)
    }
    // ts: MexcExchange.fetchRegularOrders
    pub fn fetch_regular_orders(
        &self,
        symbols: Option<Vec<String>>,
    ) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(async move {
            if Exchange::is_public_data_only(self) {
                return Ok(vec![]);
            }
            let scopes: Vec<_> = symbols
                .filter(|s| !s.is_empty())
                .map(|s| s.into_iter().map(Some).collect())
                .unwrap_or_else(|| vec![None]);
            let mut orders = vec![];
            for symbol in scopes {
                let venue = symbol.filter(|s| !s.is_empty()).map(|s| to_mexc_symbol(&s));
                match self
                    .fetch_order_rows(
                        order_list_endpoint(venue.as_deref()),
                        json!({"page_num":1,"page_size":100}),
                        OrderKind::Regular,
                    )
                    .await
                {
                    Ok(rows) => orders.extend(rows),
                    Err(error) => self.base.log(
                        &format!(
                            "Failed to fetch MEXC open orders: {}",
                            failure_message(&error)?
                        ),
                        "warning",
                    )?,
                }
            }
            Ok(unique_orders(orders))
        })
    }
    // ts: MexcExchange.fetchAndSetRegularOrders
    pub fn fetch_and_set_regular_orders(
        &self,
        symbols: Option<Vec<String>>,
    ) -> ExchangeFuture<'_, Vec<Order>> {
        self.reconcile_orders(symbols.clone(), self.fetch_regular_orders(symbols), true)
    }
    fn reconcile_orders<'a>(
        &'a self,
        symbols: Option<Vec<String>>,
        fetch: ExchangeFuture<'a, Vec<Order>>,
        regular: bool,
    ) -> ExchangeFuture<'a, Vec<Order>> {
        Box::pin(async move {
            // Cancellation before this await resolves mutates no venue state.
            // After it, reconciliation is synchronous; no latch can survive Drop.
            let orders = fetch.await?;
            if self.base.is_disposed() {
                return Ok(orders);
            }
            if !regular {
                for order in &orders {
                    mark_stop_like_order_seen(&mut self.placed.borrow_mut(), &order.id);
                }
            }
            if let Some(symbols) = symbols.filter(|s| !s.is_empty()) {
                for symbol in symbols {
                    let fetched: Vec<_> = orders
                        .iter()
                        .filter(|o| o.symbol == symbol)
                        .cloned()
                        .collect();
                    let merged = self.merge_snapshot(fetched, Some(&symbol), regular);
                    self.base.store.set_orders_for_symbol(&symbol, merged)?;
                }
            } else {
                let merged = self.merge_snapshot(orders.clone(), None, regular);
                let mut loaded = self.base.store.loaded().clone();
                loaded.orders = true;
                self.base.store.update(
                    StoreUpdate {
                        orders: Some(merged),
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )?;
            }
            Ok(orders)
        })
    }
    fn merge_snapshot(
        &self,
        fetched: Vec<Order>,
        symbol: Option<&str>,
        regular: bool,
    ) -> Vec<Order> {
        let ids: HashSet<_> = fetched.iter().map(|o| o.id.as_str()).collect();
        let stored = self.base.store.orders();
        if regular {
            let stops = stored
                .iter()
                .filter(|o| self.should_preserve_stop_like_order(o, symbol))
                .cloned();
            // Keep the two source passes separate: their shield expiry reads have order.
            let stops: Vec<_> = stops.collect();
            let preserved: Vec<_> = stored
                .iter()
                .filter(|o| self.should_preserve_recent_regular_order(o, &ids, symbol))
                .cloned()
                .collect();
            unique_orders(stops.into_iter().chain(preserved).chain(fetched))
        } else {
            let keep = |o: &&Order| {
                o.status == OrderStatus::Open
                    && symbol.is_none_or(|s| o.symbol == s)
                    && !ids.contains(o.id.as_str())
            };
            let existing: Vec<_> = if symbol.is_some() {
                stored
                    .iter()
                    .filter(keep)
                    .filter(|o| {
                        !is_stop_like_order(o)
                            && !is_recently_canceled_stop_like_order(
                                &mut self.canceled.borrow_mut(),
                                o,
                                &|| (self.current_time_ms)(),
                            )
                    })
                    .cloned()
                    .collect()
            } else {
                vec![]
            };
            let shields: Vec<_> = stored
                .iter()
                .filter(keep)
                .filter(|o| {
                    is_stop_like_order(o)
                        && !is_recently_canceled_stop_like_order(
                            &mut self.canceled.borrow_mut(),
                            o,
                            &|| (self.current_time_ms)(),
                        )
                        && is_recently_placed_stop_like_order(
                            &mut self.placed.borrow_mut(),
                            o,
                            &|| (self.current_time_ms)(),
                        )
                })
                .cloned()
                .collect();
            unique_orders(fetched.into_iter().chain(existing).chain(shields))
        }
    }
    // ts: MexcExchange.shouldPreserveStopLikeOrder
    fn should_preserve_stop_like_order(&self, order: &Order, symbol: Option<&str>) -> bool {
        is_stop_like_order(order)
            && order.status == OrderStatus::Open
            && symbol.is_none_or(|s| order.symbol == s)
            && !is_recently_canceled_stop_like_order(
                &mut self.canceled.borrow_mut(),
                order,
                &|| (self.current_time_ms)(),
            )
    }
    // ts: MexcExchange.shouldPreserveRecentRegularOrder
    fn should_preserve_recent_regular_order(
        &self,
        order: &Order,
        fetched: &HashSet<&str>,
        symbol: Option<&str>,
    ) -> bool {
        if fetched.contains(order.id.as_str())
            || order.status != OrderStatus::Open
            || is_stop_like_order(order)
            || symbol.is_some_and(|s| order.symbol != s)
            || is_recently_rejected_regular_order(
                &mut self.rejected.borrow_mut(),
                &order.id,
                &|| (self.current_time_ms)(),
            )
        {
            return false;
        }
        let last = if order.last_update.is_finite() {
            order.last_update
        } else {
            0.0
        };
        last != 0.0 && (self.current_time_ms)() - last <= 15000.0
    }
}

#[cfg(test)]
#[path = "exchange_tests.rs"]
mod tests;
