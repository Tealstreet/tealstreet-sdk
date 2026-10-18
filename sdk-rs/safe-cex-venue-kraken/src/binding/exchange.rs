//! Production Kraken venues: the assembled class graph behind the typed Exchange trait.
use super::{
    globals,
    promise::await_js,
    rt::{ExternalPublicDataFactory, KrakenExtra, Rt, Shared, SpotIds},
    store,
    value::{Js, Thrown, from_typed, to_typed},
};
use crate::{api::FuturesModule, construction::Venue};
use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding},
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedOrder},
    journal::{HistoricalPositionOptions, JournalFolder},
    js_runtime::{client::JsHttpHost, scope::ScopeShared},
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::{cell::RefCell, rc::Rc};

pub struct KrakenDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    /// HTTP wire host for every client the venue creates.
    pub http: Rc<dyn JsHttpHost>,
    /// Provider for `options.externalPublicData`; required when that option is set.
    pub external_public_data: Option<ExternalPublicDataFactory>,
    /// The imported journal folding engine behind `fetchHistoricalPositions`.
    pub journal: Option<Rc<dyn JournalFolder>>,
    /// The module-scoped Spot client-ID generator (see `spot_client_ids`), shared by
    /// every Kraken Spot instance of one SDK runtime. Required for `krakenspot`.
    pub spot_client_ids: Option<Rc<RefCell<SpotIds>>>,
    /// `kraken.api.ts`'s module-scoped nonce counter: create once per SDK runtime and
    /// share it, so every Futures client of every instance strictly increases one nonce.
    pub futures_nonces: Rc<FuturesModule>,
}

/// Wrap `helpers::create_kraken_spot_client_order_id` once per SDK runtime.
pub fn spot_client_ids(
    fill: impl FnMut(&mut [u8]) -> Result<(), safe_cex_num::JsCoercionError> + 'static,
) -> Rc<RefCell<SpotIds>> {
    Rc::new(RefCell::new(Box::new(
        crate::helpers::create_kraken_spot_client_order_id(fill),
    )))
}

// ts: exchangeMetadata kraken/krakenspot — tickInterval 500, supportsPublicOnly; no clientOrderIdRule.
pub(crate) fn settings() -> ExchangeSettings {
    ExchangeSettings {
        tick_interval: Some(500.0),
        positions_supported: None,
        supports_public_only: true,
        client_order_id_rule: None,
    }
}

pub struct KrakenExchange {
    rt: Rt,
    capabilities: ExchangeCapabilities,
}

fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    from_typed(value).map_err(err)
}

fn opt_js<T: serde::Serialize>(value: Option<T>) -> Result<Js, ExchangeError> {
    value.map_or(Ok(Js::Undefined), |v| js(&v))
}

macro_rules! args {
    ($($e:expr),*) => {
        match (|| -> Result<Vec<Js>, ExchangeError> { Ok(vec![$($e?),*]) })() {
            Ok(v) => v,
            Err(e) => return Box::pin(async move { Err(e) }),
        }
    };
}

/// The source BaseExchange default for a member Kraken does not override.
fn not_implemented<'a, T: 'a>() -> ExchangeFuture<'a, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

impl KrakenExchange {
    // ts: KrakenExchange.constructor / KrakenSpotExchange.constructor, selected by name.
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: KrakenDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        let venue = match name {
            ExchangeName::Kraken => Venue::Futures,
            ExchangeName::Krakenspot => Venue::Spot,
            other => {
                return Err(ExchangeError::new(format!(
                    "KrakenExchange cannot construct {}",
                    serde_json::to_value(&other)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .unwrap_or_default()
                )));
            }
        };
        if options.external_public_data.is_some() && deps.external_public_data.is_none() {
            return Err(ExchangeError::new(
                "KrakenExchange: externalPublicData is configured but no external public data provider was supplied",
            ));
        }
        let spot_ids = match (venue, deps.spot_client_ids) {
            (_, Some(ids)) => ids,
            (Venue::Spot, None) => {
                return Err(ExchangeError::new(
                    "KrakenExchange: krakenspot requires the module-scoped spot client-ID generator",
                ));
            }
            (Venue::Futures, None) => Rc::new(RefCell::new(Box::new(|| {
                Err(safe_cex_num::JsCoercionError {
                    name: "Error",
                    message: "Kraken Futures has no Spot client-ID generator",
                })
            }) as SpotIds)),
        };
        let module = deps.futures_nonces.clone();
        let shared = Rc::new_cyclic(|weak: &std::rc::Weak<Shared>| {
            let globals = globals::build(weak.clone());
            ScopeShared::new(
                deps.runtime.clone(),
                deps.http.clone(),
                store.clone(),
                globals,
                KrakenExtra {
                    venue,
                    name: name.clone(),
                    options: options.clone(),
                    module,
                    spot_ids,
                    sockets: RefCell::default(),
                    startup: crate::startup::StartupState {
                        disposed: Default::default(),
                        public_data_only: Default::default(),
                        public_socket: RefCell::new(None),
                        private_socket: RefCell::new(None),
                    },
                    external_public_data: deps.external_public_data.clone(),
                    journal: deps.journal.clone(),
                    prototypes: RefCell::default(),
                },
            )
        });
        let rt = Rt {
            shared: shared.clone(),
            this: Js::Undefined,
        };
        let js_name = from_typed(&name).map_err(err)?;
        let js_options = from_typed(&options).map_err(err)?;
        let js_store = store::facade(store.clone());
        let h = |v| Rc::new(v);
        let receiver = match venue {
            Venue::Futures => {
                crate::classes::futures_exchange(&rt, h(js_name), h(js_options), h(js_store))
            }
            Venue::Spot => {
                crate::classes::spot_exchange(&rt, h(js_name), h(js_options), h(js_store))
            }
        }
        .map_err(err)?;
        let receiver = (*receiver).clone();
        *shared.exchange.borrow_mut() = receiver.clone();
        let exchange = Rc::new(Self {
            rt: rt.with_this(receiver),
            capabilities: capabilities(),
        });
        let dyn_venue: Rc<dyn Exchange> = exchange.clone();
        *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&dyn_venue));
        Ok(exchange)
    }

    fn spot(&self) -> bool {
        self.rt.shared.extra.venue == Venue::Spot
    }

    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            let value = await_js(
                &self.rt.exchange_runtime(),
                this.call_method(member, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    // ts: the member returns its unsubscribe function; payloads reach the typed callback.
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        first: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let reporter = self.rt.shared.runtime.clone();
        let listener = Js::function("callback", move |_, a| {
            let value = a.into_iter().next().unwrap_or(Js::Undefined);
            match to_typed::<T>(&value) {
                Ok(typed) => callback(typed),
                Err(error) => reporter.report_exception(
                    &error.to_exchange_error(),
                    &serde_json::json!({"source": "kraken listener payload"}).into(),
                ),
            }
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .this
            .call_method(member, vec![first, listener])
            .map_err(err)?;
        Ok(Box::new(move || {
            if cleanup.is_callable() {
                cleanup.call(Js::Undefined, vec![]).map_err(err)?;
            }
            Ok(())
        }))
    }
    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            await_js(
                &self.rt.exchange_runtime(),
                this.call_method(member, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
            Ok(())
        })
    }
}

fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<KrakenExchange>(Exchange::place_order),
        fetch_historical_fills: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_historical_fills(opts)
        }),
        ..Default::default()
    }
}

impl Exchange for KrakenExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.rt.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.rt
            .this
            .call_method("dispose", vec![])
            .map(|_| ())
            .map_err(err)
    }
    // ts: placeOrder — BaseExchange.place_order already enforced the client-order-ID
    // contract; the body's first statement re-runs resolveCallerClientOrderId.
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(order.options()));
        self.invoke("placeOrder", a)
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("start", vec![])
    }
    // ts: base.ts:734 — Kraken inherits getAccount, which rejects.
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        not_implemented()
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", vec![])
    }
    fn fetch_positions(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.invoke("fetchPositions", vec![])
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.invoke("fetchOrders", vec![])
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    // ts: base.ts:799 — inherited fetchSymbolMaxLeverage rejects.
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        not_implemented()
    }
    // ts: KrakenExchange.setLeverage; Spot inherits base.ts:808, which rejects.
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        if self.spot() {
            return not_implemented();
        }
        let a = args!(js(&symbol), js(&leverage), opt_js(side));
        self.invoke_unit("setLeverage", a)
    }
    // ts: base.ts:832 — inherited changePositionMode rejects.
    fn change_position_mode(
        &self,
        _hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    // ts: KrakenExchange.changeMarginType; Spot inherits base.ts:836, which rejects.
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        if self.spot() {
            return not_implemented();
        }
        let a = args!(js(&isolated), opt_js(symbol));
        self.invoke_unit("changeMarginType", a)
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(&options));
        self.invoke("updateOrder", a)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&orders));
        self.invoke_unit("cancelOrders", a)
    }
    // ts: base.ts:989 — inherited scheduleCancel rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        not_implemented()
    }
    // ts: base.ts:1138 — inherited fetchPublicTrades rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        not_implemented()
    }
    // ts: KrakenExchange.fetchHistoricalFills; Spot inherits the base empty default.
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        if self.spot() {
            return Box::pin(async { Ok(vec![]) });
        }
        let a = args!(js(&options));
        self.invoke("fetchHistoricalFills", a)
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.listen("listenOHLCV", js(&options)?, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        self.listen("listenOrderBook", js(&symbol)?, callback)
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let a = args!(js(&options));
        self.invoke("fetchOHLCV", a)
    }
    // ts: KrakenExchange.fetchIncomeHistory; Spot inherits the base empty default.
    fn fetch_income_history(
        &self,
        options: Option<IncomeHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        if self.spot() {
            return Box::pin(async { Ok(vec![]) });
        }
        let a = args!(opt_js(options));
        self.invoke("fetchIncomeHistory", a)
    }
    // ts: KrakenExchange.fetchIncomeHistoryRange; Spot inherits the base empty default.
    fn fetch_income_history_range(
        &self,
        start: f64,
        end: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        if self.spot() {
            return Box::pin(async { Ok(vec![]) });
        }
        let a = args!(js(&start), js(&end), opt_js(kind));
        self.invoke("fetchIncomeHistoryRange", a)
    }
    // ts: KrakenExchange.fetchHistoricalPositions folds fills; Spot inherits the base refusal.
    fn fetch_historical_positions(
        &self,
        options: HistoricalPositionOptions,
    ) -> ExchangeFuture<'_, Vec<serde_json::Value>> {
        if self.spot() {
            return Box::pin(async move {
                Err(ExchangeError::new(format!(
                    "{}: no journal records path declared. Override fetchHistoricalPositions — fold from fills via foldClosedPositionRecords, or map this venue's reported closed axis.",
                    self.class_name()
                )))
            });
        }
        let opts = Js::object(vec![
            ("account", Js::str(&options.account)),
            ("from", options.from.map_or(Js::Undefined, Js::Number)),
            ("to", options.to.map_or(Js::Undefined, Js::Number)),
        ]);
        self.invoke("fetchHistoricalPositions", vec![opts])
    }
    // ts: KrakenSpotExchange.fetchAndSetOrders ignores symbols and replaces the full set.
    fn fetch_and_set_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        if !self.spot() {
            return Box::pin(async move { self.base().fetch_and_set_orders(self, symbols).await });
        }
        self.invoke("fetchAndSetOrders", vec![])
    }
    // ts: KrakenExchange.signWsAuth; Spot inherits the base signer.
    fn sign_ws_auth<'a>(
        &'a self,
        request: &'a safe_cex_crypto::WsAuthRequest,
    ) -> ExchangeFuture<'a, safe_cex_crypto::WsAuthResponse> {
        if self.spot() {
            return Box::pin(async move { Ok(self.base().sign_ws_auth(request)) });
        }
        let a = args!(js(request));
        self.invoke("signWsAuth", a)
    }
    fn class_name(&self) -> &'static str {
        if self.spot() {
            "KrakenSpotExchange"
        } else {
            "KrakenExchange"
        }
    }
}
