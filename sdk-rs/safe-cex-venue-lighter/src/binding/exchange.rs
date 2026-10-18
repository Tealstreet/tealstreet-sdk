//! Production Lighter venue: the ported `LighterExchange` class, assembled on the
//! shared js_runtime, behind the typed `Exchange` trait.
use super::{
    realm::{LighterExtra, Realm, Shared},
    value::{Microtasks, to_js, to_thrown},
};
use crate::{
    deployment::get_lighter_api_base_url, exchange_class::LighterExchange,
    public_ingress::create_sdk_client, types::RECV_WINDOW,
};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    js_runtime::{
        base_exchange::{self, BaseExchangeParts},
        client::JsHttpHost,
        globals::{install_builtins, install_enums},
        library::install_library,
        promise::await_js,
        scope::ScopeShared,
        store,
        value::{Js, Thrown, arg, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ListenerCleanup},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_transport::TransportRegistry;
use safe_cex_types::*;
use std::rc::{Rc, Weak};

/// Host services the venue is built over.
pub struct LighterDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    /// HTTP wire host for every client the venue creates.
    pub http: Rc<dyn JsHttpHost>,
    /// The realm's default transport (TS `setDefaultTransport`), which the
    /// Lighter base URL is resolved through on every call.
    pub registry: Rc<TransportRegistry>,
    /// Optional Schnorr nonce source. Omit to use the host CSPRNG. Scripted
    /// transports can inject deterministic entropy for byte-exact wire tests.
    pub signing_entropy: Option<Rc<dyn Fn(&mut [u8; 40]) -> Result<(), String>>>,
}

/// Lighter's metadata: a 2s tick, no public-only mode, and no client order ID
/// rule, so the base refuses every caller-supplied ID before any write.
fn settings() -> ExchangeSettings {
    ExchangeSettings {
        tick_interval: Some(2000.0),
        ..ExchangeSettings::default()
    }
}

pub struct LighterNativeExchange {
    realm: Realm,
    /// The constructed class instance; filled once construction returns.
    this: std::cell::RefCell<Js>,
    capabilities: ExchangeCapabilities,
}

fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

/// The `BaseExchange` binding the class extends: the shared JS base over the
/// Rust core, whose handle is kept for `Exchange::base()`.
fn base_exchange_class(weak: Weak<Shared>, name: ExchangeName, options: ExchangeOptions) -> Js {
    let class = Js::constructor("BaseExchange", None, move |_, a| {
        let shared = weak
            .upgrade()
            .ok_or_else(|| Thrown::error("Error", "Lighter scope was dropped"))?;
        let parts = BaseExchangeParts {
            name: name.clone(),
            options: options.clone(),
            store: shared.store.clone(),
            settings: settings(),
            external_public_data: None,
        };
        let (instance, core) = base_exchange::create(
            &shared.sockets,
            parts,
            arg(&a, 0),
            arg(&a, 1),
            arg(&a, 2),
            |_| Ok(()),
        )?;
        if shared.base.set(core).is_err() {
            return Err(Thrown::error("Error", "BaseExchange constructed twice"));
        }
        Ok(instance)
    });
    let _ = class.define("prototype", Js::object(vec![]));
    class
}

/// The realm the class runs in: scope, globals and every module binding the
/// ported bodies resolve by name.
pub(crate) fn build_realm(
    name: &ExchangeName,
    options: &ExchangeOptions,
    store: &Rc<dyn Store>,
    deps: &LighterDependencies,
) -> Result<Realm, Thrown> {
    let runtime: Rc<dyn ExchangeRuntime> = deps.runtime.clone();
    let shared = Rc::new_cyclic(|weak: &Weak<Shared>| {
        let globals = Js::object(vec![]);
        install_builtins(&globals, weak.clone());
        install_enums(&globals);
        install_library(&globals, weak.clone());
        let _ = globals.define(
            "BaseExchange",
            base_exchange_class(weak.clone(), name.clone(), options.clone()),
        );
        ScopeShared::new(
            deps.runtime.clone(),
            deps.http.clone(),
            store.clone(),
            globals,
            LighterExtra {
                microtasks: Microtasks::new(runtime),
                supers: Default::default(),
                registry: deps.registry.clone(),
            },
        )
    });
    super::globals::install(&shared.globals)?;
    let realm = Realm(shared.clone());
    install_sdk_client(&realm, deps.registry.clone())?;
    super::sdk_client::install(&realm)?;
    match &deps.signing_entropy {
        Some(entropy) => super::signer::install_with_entropy(&realm, entropy.clone())?,
        None => super::signer::install(&realm)?,
    }
    super::socket_globals::install(&realm)?;
    super::ws_public::install(&realm)?;
    super::ws_private::install(&realm)?;
    Ok(realm)
}

impl LighterNativeExchange {
    // ts: LighterExchange.constructor
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: LighterDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if !matches!(name, ExchangeName::Lighter | ExchangeName::LighterRobinhood) {
            return Err(ExchangeError::new(
                "LighterNativeExchange cannot construct this venue",
            ));
        }
        let realm = build_realm(&name, &options, &store, &deps).map_err(err)?;
        let shared = realm.0.clone();
        let class = LighterExchange::define(&realm).map_err(|e| err(to_thrown(e)))?;
        // The venue exists before construction: base getters such as
        // isPublicDataOnly dispatch through it while the constructor runs.
        let venue = Rc::new(Self {
            realm,
            this: std::cell::RefCell::new(Js::Undefined),
            capabilities: capabilities(),
        });
        let dyn_venue: Rc<dyn Exchange> = venue.clone();
        *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&dyn_venue));
        let this = to_js(&class)
            .construct(
                vec![
                    from_typed(&name).map_err(err)?,
                    from_typed(&options).map_err(err)?,
                    store::facade(store)?,
                ],
                None,
            )
            .map_err(err)?;
        *shared.exchange.borrow_mut() = this.clone();
        *venue.this.borrow_mut() = this;
        Ok(venue)
    }

    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let this = self.this.borrow().clone();
        Box::pin(async move {
            let value = await_js(
                &self.realm.exchange_runtime(),
                this.call_method(member, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }

    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        options: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let listener = Js::function("callback", move |_, args| {
            callback(to_typed(&arg(&args, 0))?);
            Ok(Js::Undefined)
        });
        let this = self.this.borrow().clone();
        let cleanup = this
            .call_method(member, vec![options, listener])
            .map_err(err)?;
        Ok(Box::new(move || {
            cleanup.call(Js::Undefined, vec![]).map(|_| ()).map_err(err)
        }))
    }

    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.this.borrow().clone();
        Box::pin(async move {
            await_js(
                &self.realm.exchange_runtime(),
                this.call_method(member, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
            Ok(())
        })
    }
}

/// lighter.api.ts and the lighter.types.ts helpers it reads.
fn install_sdk_client(realm: &Realm, registry: Rc<TransportRegistry>) -> Result<(), Thrown> {
    let globals = &realm.0.globals;
    globals.define(
        "getLighterApiBaseUrl",
        Js::function("getLighterApiBaseUrl", move |_, a| {
            let params = arg(&a, 0);
            let name = params.get("exchangeName")?;
            let name: Option<ExchangeName> = if name.is_nullish() {
                None
            } else {
                Some(to_typed(&name)?)
            };
            let testnet = params.get("testnet")?;
            let testnet = (!testnet.is_nullish()).then(|| testnet.truthy());
            Ok(Js::str(&get_lighter_api_base_url(
                name.as_ref(),
                testnet,
                &registry,
            )))
        }),
    )?;
    globals.define("RECV_WINDOW", Js::Number(RECV_WINDOW as f64))?;
    globals.define(
        "UNSUPPORTED_LIGHTER_REST_HEADERS",
        Js::array(vec![Js::str("x-timestamp"), Js::str("x-account-index")]),
    )?;
    let ops = Rc::new(super::ingress::ops(realm));
    globals.define(
        "createLighterSDKClient",
        Js::function("createLighterSDKClient", move |_, a| {
            create_sdk_client(ops.clone(), arg(&a, 0), arg(&a, 1))
        }),
    )
}

fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    from_typed(value).map_err(err)
}

fn opt_js<T: serde::Serialize>(value: Option<T>) -> Result<Js, ExchangeError> {
    value.map_or(Ok(Js::Undefined), |v| js(&v))
}

/// The TS class inherits this member from BaseExchange, which rejects it.
fn inherited_rejection<T: 'static>() -> ExchangeFuture<'static, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<LighterNativeExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<LighterNativeExchange>(
            Exchange::place_orders,
        ),
        fetch_historical_fills: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_historical_fills(opts)
        }),
        ..Default::default()
    }
}

macro_rules! args {
    ($($e:expr),*) => {
        match (|| -> Result<Vec<Js>, ExchangeError> { Ok(vec![$($e?),*]) })() {
            Ok(v) => v,
            Err(e) => return Box::pin(async move { Err(e) }),
        }
    };
}

impl Exchange for LighterNativeExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.realm
            .0
            .base
            .get()
            .expect("BaseExchange is constructed with the venue")
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.this
            .borrow()
            .call_method("dispose", vec![])
            .map(|_| ())
            .map_err(err)
    }
    // ts: LighterExchange.placeOrder — the validated order already passed the base
    // client-order-ID refusal (Lighter honors none) before this call.
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(order.options()));
        self.invoke("placeOrder", a)
    }
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        let list: Vec<PlaceOrderOpts> = orders
            .orders()
            .iter()
            .map(|o| o.options().clone())
            .collect();
        let a = args!(js(&list));
        self.invoke("placeOrders", a)
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("start", vec![])
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.invoke("getAccount", vec![])
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", vec![])
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        let a = args!(opt_js(symbols));
        self.invoke("fetchPositions", a)
    }
    // ts: LighterExchange.fetchOrders takes no symbols; the trait argument is unused.
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.invoke("fetchOrders", vec![])
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    // ts: base.ts:759 — LighterExchange inherits fetchTickers.
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        inherited_rejection()
    }
    // ts: base.ts:799 — inherited fetchSymbolMaxLeverage rejects.
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        inherited_rejection()
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol), js(&leverage), opt_js(side));
        self.invoke_unit("setLeverage", a)
    }
    // ts: base.ts:832 — inherited changePositionMode rejects (Lighter is one-way only).
    fn change_position_mode(
        &self,
        _hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        inherited_rejection()
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&isolated), opt_js(symbol));
        self.invoke_unit("changeMarginType", a)
    }
    fn update_order(&self, opts: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(&opts));
        self.invoke("updateOrder", a)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&orders));
        self.invoke_unit("cancelOrders", a)
    }
    // ts: base.ts:989 — inherited scheduleCancel rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        inherited_rejection()
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
        self.invoke("fetchOHLCV", args!(js(&options)))
    }
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        self.invoke("fetchHistoricalFills", args!(js(&options)))
    }
    // Source performs one venue snapshot attempt with its own error handling.
    fn fetch_and_set_private_snapshot(&self, _tries: u32) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("fetchAndSetPrivateSnapshot", vec![])
    }
    // ts: base.ts:1138 — inherited fetchPublicTrades rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        inherited_rejection()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_exchange::{
        js_runtime::promise::JsFuture,
        runtime::ExchangeTask,
        websocket::{WebSocketConnectionOptions, WsSocket},
    };
    use safe_cex_transport::{TransportProvider, socket::TunnelHost};
    use std::{future::Future, pin::Pin, sync::Arc};

    /// Installing globals touches no host service; any call is a test failure.
    struct Untouched;
    impl ExchangeRuntime for Untouched {
        fn monotonic_now_ms(&self) -> f64 {
            panic!("runtime used while installing globals")
        }
        fn now_ms(&self) -> f64 {
            panic!("runtime used while installing globals")
        }
        fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
            panic!("runtime used while installing globals")
        }
        fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
            panic!("runtime used while installing globals")
        }
        fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
        fn unique_id(&self) -> String {
            panic!("runtime used while installing globals")
        }
        fn is_worker(&self) -> bool {
            false
        }
        fn random(&self) -> f64 {
            panic!("runtime used while installing globals")
        }
        fn report_exception(
            &self,
            _: &ExchangeError,
            _: &safe_cex_types::retained_json::RetainedJsonInput,
        ) {
        }
        fn report_raw_exception(
            &self,
            _: &safe_cex_exchange::js_runtime::value::Js,
            _: &safe_cex_types::retained_json::RetainedJsonInput,
        ) {
        }
        fn execute_worker_request(
            &self,
            _: serde_json::Value,
        ) -> ExchangeFuture<'static, serde_json::Value> {
            panic!("runtime used while installing globals")
        }
    }
    impl WsRuntime for Untouched {
        fn transport(&self) -> Arc<dyn TransportProvider> {
            panic!("runtime used while installing globals")
        }
        fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
            panic!("runtime used while installing globals")
        }
        fn dial(
            &self,
            _: String,
            _: Vec<String>,
            _: WebSocketConnectionOptions,
            _: bool,
        ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
            panic!("runtime used while installing globals")
        }
        fn headers_capable(&self) -> bool {
            false
        }
        fn enable_reconnecting(&self) -> bool {
            false
        }
        fn min_reconnection_delay_ms(&self) -> f64 {
            1000.0
        }
        fn library_min_reconnection_delay_ms(&self) -> f64 {
            1000.0
        }
        fn diagnostics_enabled(&self) -> bool {
            false
        }
        fn lifecycle_logs_enabled(&self) -> bool {
            false
        }
    }
    impl JsHttpHost for Untouched {
        fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
            Rc::new(Untouched)
        }
        fn send(&self, _: Js) -> JsFuture<'static> {
            panic!("http used while installing globals")
        }
    }

    /// Every name a ported LighterExchange body resolves through `global`,
    /// `constant` or `binding_value` (exchange_*.rs), other than `super.*`
    /// (per-instance) and class statics (bound by `finish_class_binding`).
    const RESOLVED_NAMES: &[&str] = &[
        "adjust",
        "Array.from",
        "Array.isArray",
        "BaseExchange",
        "BigInt",
        "Boolean",
        "CandlestickApi",
        "chunk",
        "clearTimeout",
        "console.error",
        "console.warn",
        "createLighterSDKClient",
        "createLighterSignerAdapter",
        "createPositionId",
        "Date.now",
        "Date.parse",
        "dayjs",
        "ENDPOINTS.ACCOUNT",
        "ENDPOINTS.ACCOUNT_LIMITS",
        "ENDPOINTS.ACCOUNTS_BY_L1_ADDRESS",
        "ENDPOINTS.CANDLES",
        "ENDPOINTS.CHANGE_ACCOUNT_TIER",
        "ENDPOINTS.INACTIVE_ORDERS",
        "ENDPOINTS.OPEN_ORDERS",
        "ENDPOINTS.TRADES",
        "Error",
        "extractLighterAccountTierFromPayload",
        "forEachSeries",
        "getCryptoApi",
        "getLighterAmountDecimals",
        "getLighterMarketLeverageMax",
        "getLighterOrderMinimums",
        "getLighterPriceDecimals",
        "GRAFANA_ENDPOINTS_EVERY",
        "GRAFANA_ENDPOINTS_PERIODIC",
        "hasNonZeroLighterIntegratorFee",
        "initWasm",
        "isLighterL1Address",
        "isLighterAccountTierEligibleForPartnerAttribution",
        "isWasmReady",
        "JSON.stringify",
        "LIGHTER_ACCOUNT_LIMITS_CACHE_MS",
        "LIGHTER_ACCOUNT_LIMITS_RETRY_MS",
        "LIGHTER_ACCOUNT_SNAPSHOT_REUSE_MS",
        "LIGHTER_DEFAULT_QUOTE",
        "LIGHTER_DEFAULT_STOP_SLIPPAGE_PCT",
        "LIGHTER_INITIAL_TICK_MAX_DELAY_MS",
        "LIGHTER_KNOWN_QUOTES",
        "LIGHTER_MARGIN_FRACTION_TICK",
        "LIGHTER_MAX_STOP_SLIPPAGE_PCT",
        "LIGHTER_OHLCV_FALLBACK_403_COOLDOWN_MS",
        "LIGHTER_OHLCV_POLL_INTERVAL_MS",
        "LIGHTER_OHLCV_WS_STALE_MS",
        "LIGHTER_ORDER_RECONCILIATION_COOLDOWN_MS",
        "LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE",
        "LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE",
        "LighterPrivateWebsocket",
        "LighterPublicWebsocket",
        "LogSeverity.Debug",
        "LogSeverity.Info",
        "LogSeverity.Warning",
        "loop",
        "Map",
        "MarginType.Isolated",
        "Math.abs",
        "Math.ceil",
        "Math.floor",
        "Math.max",
        "Math.min",
        "Math.random",
        "mergeUpdatesUsingLastUpdate",
        "normalizeLighterStopSlippagePct",
        "Number",
        "Number.isFinite",
        "Number.NaN",
        "Number.parseFloat",
        "Number.parseInt",
        "Object.entries",
        "Object.values",
        "ORDER_SIDE",
        "ORDER_STATUS",
        "ORDER_TIME_IN_FORCE",
        "ORDER_TYPE",
        "OrderApi",
        "orderBy",
        "OrderSide.Buy",
        "OrderSide.Sell",
        "OrderStatus.Open",
        "OrderTimeInForce.GoodTillCancel",
        "OrderTimeInForce.ImmediateOrCancel",
        "OrderTimeInForce.PostOnly",
        "OrderType.Limit",
        "OrderType.Market",
        "OrderType.StopLoss",
        "OrderType.TakeProfit",
        "OrderType.TrailingStopLoss",
        "parseFloat",
        "parseInt",
        "parseLighterApiKeyIndex",
        "parseLighterMarginFractionRatio",
        "parseLighterMarketSymbol",
        "parseLighterPositiveIntegerString",
        "PositionMode.OneWay",
        "PositionSide.Long",
        "PositionSide.Short",
        "Promise.all",
        "Promise.allSettled",
        "Promise.resolve",
        "resolveLighterBuilderFeeConfig",
        "resolveLighterCancelTargets",
        "resolveLighterDeploymentProfile",
        "resolveLighterInterval",
        "setTimeout",
        "sleep",
        "String",
        "subtract",
        "toLighterApiKeyIndexString",
        "toLighterScaledInt",
        "uniq",
        "uniqBy",
        "URLSearchParams",
        "v",
        "virtualClock.getCurrentTimeMs",
    ];

    #[test]
    fn every_name_the_exchange_bodies_resolve_is_bound() {
        let deps = LighterDependencies {
            runtime: Rc::new(Untouched),
            http: Rc::new(Untouched),
            registry: Rc::new(TransportRegistry::default()),
            signing_entropy: None,
        };
        let store: Rc<dyn Store> = Rc::new(safe_cex_exchange::native_store::DefaultStore::new());
        let realm = build_realm(
            &ExchangeName::Lighter,
            &serde_json::from_value(serde_json::json!({ "key": "", "secret": "" })).unwrap(),
            &store,
            &deps,
        )
        .unwrap_or_else(|e| panic!("realm: {}", e.message()));
        let class = LighterExchange::define(&realm)
            .map_err(to_thrown)
            .map(|c| to_js(&c));
        assert!(class.is_ok(), "the class defines over the realm");
        let missing: Vec<&str> = RESOLVED_NAMES
            .iter()
            .copied()
            .filter(|name| realm.global(name).map_or(true, |v| v.is_undefined()))
            .collect();
        assert!(missing.is_empty(), "unbound globals: {missing:?}");
    }
}
