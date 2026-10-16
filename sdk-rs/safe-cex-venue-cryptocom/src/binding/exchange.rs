//! Production Crypto.com venue: the assembled class graph behind the typed Exchange trait.
use super::{
    globals, ops,
    rt::{CryptocomExtra, ExternalPublicDataFactory, Rt, Shared},
};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    js_runtime::{
        client::JsHttpHost,
        promise::await_js,
        scope::ScopeShared,
        store,
        value::{Js, Thrown, arg, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::rc::Rc;

pub struct CryptocomDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    /// HTTP wire host for every client the venue creates.
    pub http: Rc<dyn JsHttpHost>,
    /// Provider for `options.externalPublicData`; required when that option is set.
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

// ts: exchangeMetadata cryptocom — tickInterval 2000, supportsPublicOnly, /^.{1,36}$/ client IDs.
pub(crate) fn settings() -> ExchangeSettings {
    ExchangeSettings {
        tick_interval: Some(2000.0),
        positions_supported: None,
        supports_public_only: true,
        client_order_id_rule: Some(globals::client_order_id_rule()),
    }
}

pub struct CryptocomExchange {
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

/// The source BaseExchange default for a member Crypto.com does not override.
fn not_implemented<'a, T: 'a>() -> ExchangeFuture<'a, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

/// CHOSEN: REST `fetchBalance` casts `{ available, locked }` rows into
/// `Balance.currencies`; the typed model takes them in the private socket's
/// currency shape (free = available, used = locked, total = their sum, upnl 0).
fn project_balance(balance: &Js) -> Result<(), Thrown> {
    let currencies = balance.get("currencies")?;
    if currencies.is_nullish() {
        return Ok(());
    }
    for (key, row) in currencies.own_entries()? {
        if !row.has_own("available") || row.has_own("total") {
            continue;
        }
        let (available, locked) = (row.get("available")?, row.get("locked")?);
        let total = safe_cex_exchange::js_runtime::ops::add(&available, &locked)?;
        currencies.set_key(
            &key,
            Js::object(vec![
                ("total", total),
                ("free", available),
                ("used", locked),
                ("upnl", Js::Number(0.0)),
            ]),
        )?;
    }
    Ok(())
}

/// The Store facade, with `update({ balance })` taking the projected currencies.
fn store_facade(store: Rc<dyn Store>) -> Result<Js, Thrown> {
    let facade = store::facade(store);
    let update = facade.get("update")?;
    let receiver = facade.clone();
    facade.define(
        "update",
        Js::function("update", move |_, a| {
            let balance = arg(&a, 0).get("balance")?;
            if !balance.is_nullish() {
                project_balance(&balance)?;
            }
            update.call(receiver.clone(), a)
        }),
    )?;
    Ok(facade)
}

/// Define the module-scope classes and `createAPI`, which need the live scope.
fn define_classes(weak: &std::rc::Weak<Shared>, globals: &Js) -> Result<Js, Thrown> {
    let runtime = ops::runtime_ops(weak.clone());
    let socket = ops::socket_class_ops(weak.clone());
    globals.define(
        "CryptocomPublicWebSocket",
        crate::socket_classes::define_public_socket(socket.clone())?,
    )?;
    globals.define(
        "CryptocomPrivateWebSocket",
        crate::socket_classes::define_private_socket(socket)?,
    )?;
    let api_ops = runtime.clone();
    globals.define(
        "createAPI",
        Js::function("createAPI", move |_, a| {
            crate::runtime::create_api(api_ops.clone(), arg(&a, 0), arg(&a, 1))
        }),
    )?;
    let class = crate::runtime::define_exchange(runtime)?;
    globals.define("CryptocomExchange", class.clone())?;
    Ok(class)
}

impl CryptocomExchange {
    // ts: CryptocomExchange.constructor
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: CryptocomDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Cryptocom {
            return Err(ExchangeError::new(format!(
                "CryptocomExchange cannot construct {}",
                serde_json::to_value(&name)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            )));
        }
        let shared = Rc::new_cyclic(|weak: &std::rc::Weak<Shared>| {
            ScopeShared::new(
                deps.runtime.clone(),
                deps.http.clone(),
                store.clone(),
                globals::build(weak.clone()),
                CryptocomExtra {
                    name: name.clone(),
                    options: options.clone(),
                    classes: Default::default(),
                    external_public_data: deps.external_public_data.clone(),
                },
            )
        });
        let weak = Rc::downgrade(&shared);
        let class = define_classes(&weak, &shared.globals).map_err(err)?;
        let js_name = from_typed(&name).map_err(err)?;
        let js_options = from_typed(&options).map_err(err)?;
        let js_store = store_facade(store.clone()).map_err(err)?;
        let receiver =
            ops::construct(&weak, class, vec![js_name, js_options, js_store]).map_err(err)?;
        *shared.exchange.borrow_mut() = receiver.clone();
        let rt = Rt {
            shared: shared.clone(),
            this: receiver,
        };
        let venue = Rc::new(Self {
            rt,
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
        let this = self.rt.this.clone();
        Box::pin(async move {
            let value = await_js(this.call_method(member, args).map_err(err)?)
                .await
                .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            await_js(this.call_method(member, args).map_err(err)?)
                .await
                .map_err(err)?;
            Ok(())
        })
    }
}

fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<CryptocomExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<CryptocomExchange>(Exchange::place_orders),
        ..Default::default()
    }
}

impl Exchange for CryptocomExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.rt.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        // js_runtime exposes isDisposed through the typed base's read-only
        // getter. Close that owner before the source tears down its sockets.
        self.base().dispose()?;
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
        let this = self.rt.this.clone();
        Box::pin(async move {
            let value = await_js(this.call_method("validateAccount", vec![]).map_err(err)?)
                .await
                .map_err(err)?;
            let balance = value.get("balance").map_err(err)?;
            if !balance.is_nullish() {
                project_balance(&balance).map_err(err)?;
            }
            to_typed(&value).map_err(err)
        })
    }
    // ts: fetchPositions / fetchOrders take no symbol filter.
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
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol), js(&leverage), opt_js(side));
        self.invoke_unit("setLeverage", a)
    }
    fn change_position_mode(
        &self,
        hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&hedged));
        self.invoke_unit("changePositionMode", a)
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
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
    // ts: base.ts:997 — inherited fetchHistoricalFills resolves [].
    fn fetch_historical_fills(
        &self,
        _options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        Box::pin(async { Ok(vec![]) })
    }
}
