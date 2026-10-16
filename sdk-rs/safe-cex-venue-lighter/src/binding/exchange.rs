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
        promise::await_js,
        scope::ScopeShared,
        store,
        value::{Js, Thrown, arg, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
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
    this: Js,
    capabilities: ExchangeCapabilities,
}

fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

/// The `BaseExchange` binding the class extends: the shared JS base over the
/// Rust core, whose handle is kept for `Exchange::base()`.
fn base_exchange_class(weak: Weak<Shared>, name: ExchangeName, options: ExchangeOptions) -> Js {
    let class = Js::constructor("BaseExchange", move |_, a| {
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
        let (instance, core) =
            base_exchange::create(&shared.sockets, parts, arg(&a, 0), arg(&a, 1), arg(&a, 2))?;
        if shared.base.set(core).is_err() {
            return Err(Thrown::error("Error", "BaseExchange constructed twice"));
        }
        Ok(instance)
    });
    let _ = class.define("prototype", Js::object(vec![]));
    class
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
        let runtime: Rc<dyn ExchangeRuntime> = deps.runtime.clone();
        let shared = Rc::new_cyclic(|weak: &Weak<Shared>| {
            let globals = Js::object(vec![]);
            install_builtins(&globals, weak.clone());
            install_enums(&globals);
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
                },
            )
        });
        super::globals::install(&shared.globals).map_err(err)?;
        let realm = Realm(shared.clone());
        install_sdk_client(&realm, deps.registry.clone()).map_err(err)?;
        let class = LighterExchange::define(&realm).map_err(|e| err(to_thrown(e)))?;
        let this = to_js(&class)
            .call(
                Js::Undefined,
                vec![
                    from_typed(&name).map_err(err)?,
                    from_typed(&options).map_err(err)?,
                    store::facade(store),
                ],
            )
            .map_err(err)?;
        *shared.exchange.borrow_mut() = this.clone();
        let venue = Rc::new(Self {
            realm,
            this,
            capabilities: capabilities(),
        });
        let dyn_venue: Rc<dyn Exchange> = venue.clone();
        *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&dyn_venue));
        Ok(venue)
    }

    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let this = self.this.clone();
        Box::pin(async move {
            let value = await_js(this.call_method(member, args).map_err(err)?)
                .await
                .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }

    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.this.clone();
        Box::pin(async move {
            await_js(this.call_method(member, args).map_err(err)?)
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
    // ts: base.ts:1138 — inherited fetchPublicTrades rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        inherited_rejection()
    }
}
