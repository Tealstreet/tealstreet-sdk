//! Production Bitget venue: the assembled class graph behind the typed Exchange trait.
use super::{
    globals,
    promise::await_js,
    rt::{BitgetImports, Rt, Shared, Variant},
    store,
    value::{Js, Thrown, from_typed, to_typed},
};
use crate::assembly;
use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding},
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    runtime::{ExchangeError, ExchangeFuture},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};

pub struct BitgetDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub imports: Rc<dyn BitgetImports>,
    /// Provider for `options.externalPublicData`; required when that option is set.
    pub external_public_data: Option<super::rt::ExternalPublicDataFactory>,
}

pub(crate) fn settings() -> ExchangeSettings {
    ExchangeSettings {
        tick_interval: None,
        positions_supported: Some(true),
        supports_public_only: true,
        supports_executions_history: true,
        client_order_id_rule: Some(super::hosts::assembly::client_order_id_rule()),
    }
}

pub struct BitgetExchange {
    rt: Rt,
    capabilities: ExchangeCapabilities,
}

fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

impl BitgetExchange {
    // ts: BitgetExchange.constructor (and the USDC / inverse subclasses by name)
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: BitgetDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        let variant = match name {
            ExchangeName::Bitget => Variant::Classic,
            ExchangeName::Bitgetusdc => Variant::Usdc,
            ExchangeName::Bitgetinverse => Variant::Inverse,
            other => {
                return Err(ExchangeError::new(format!(
                    "BitgetExchange cannot construct {}",
                    serde_json::to_value(&other).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
                )));
            }
        };
        if options.external_public_data.is_some() && deps.external_public_data.is_none() {
            return Err(ExchangeError::new(
                "BitgetExchange: externalPublicData is configured but no external public data provider was supplied",
            ));
        }
        let shared = Rc::new_cyclic(|weak: &std::rc::Weak<Shared>| {
            let globals = globals::build(weak.clone());
            super::hosts::register_all(&globals, weak.clone());
            Shared {
                runtime: deps.runtime.clone(),
                venue: RefCell::new(None),
                imports: deps.imports.clone(),
                store: store.clone(),
                base: OnceCell::new(),
                globals,
                exchange: RefCell::new(Js::Undefined),
                variant,
                name: name.clone(),
                options: options.clone(),
                external_public_data: deps.external_public_data.clone(),
                timers: RefCell::default(),
                socket_ids: std::cell::Cell::new(0),
                socket_cores: RefCell::default(),
            }
        });
        let rt = Rt { shared: shared.clone(), this: Js::Undefined };
        let js_name = from_typed(&name).map_err(err)?;
        let js_options = from_typed(&options).map_err(err)?;
        let js_store = store::facade(store.clone());
        let receiver = match variant {
            Variant::Classic => assembly::BitgetExchange::new(rt.clone(), js_name, js_options, js_store).map_err(err)?.receiver,
            Variant::Usdc => {
                assembly::BitgetUSDCExchange::new(rt.clone(), js_name, js_options, js_store).map_err(err)?.inner.receiver
            }
            Variant::Inverse => {
                assembly::BitgetInverseExchange::new(rt.clone(), js_name, js_options, js_store).map_err(err)?.inner.receiver
            }
        };
        *shared.exchange.borrow_mut() = receiver.clone();
        super::getters::install(&rt.with_this(receiver.clone()));
        super::prototype::install(&rt.with_this(receiver.clone()));
        let venue = Rc::new(Self {
            rt: rt.with_this(receiver),
            capabilities: capabilities(),
        });
        let dyn_venue: Rc<dyn Exchange> = venue.clone();
        *shared.venue.borrow_mut() = Some(Rc::downgrade(&dyn_venue));
        Ok(venue)
    }

    fn invoke<T: serde::de::DeserializeOwned + 'static>(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, T> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            let value = await_js(this.call_method(member, args).map_err(err)?).await.map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            await_js(this.call_method(member, args).map_err(err)?).await.map_err(err)?;
            Ok(())
        })
    }
}

fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    from_typed(value).map_err(err)
}

fn opt_js<T: serde::Serialize>(value: Option<T>) -> Result<Js, ExchangeError> {
    value.map_or(Ok(Js::Undefined), |v| js(&v))
}

fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<BitgetExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<BitgetExchange>(Exchange::place_orders),
        fetch_historical_fills: CapabilityBinding::implemented(|venue, opts| venue.fetch_historical_fills(opts)),
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

impl Exchange for BitgetExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.rt.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.rt.this.call_method("dispose", vec![]).map(|_| ()).map_err(err)
    }
    // ts: BitgetExchange.placeOrder — the ValidatedOrder carries the client-order-ID
    // contract, already enforced by BaseExchange.place_order before this call.
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(order.options()));
        self.invoke("placeOrder", a)
    }
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        let list: Vec<PlaceOrderOpts> = orders.orders().iter().map(|o| o.options().clone()).collect();
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
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        let a = args!(opt_js(symbols));
        self.invoke("fetchOrders", a)
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    // ts: base.ts:799 — BitgetExchange inherits fetchSymbolMaxLeverage, which rejects.
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn set_leverage(&self, symbol: String, leverage: f64, side: Option<PositionSide>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol), js(&leverage), opt_js(side));
        self.invoke_unit("setLeverage", a)
    }
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&hedged), opt_js(symbol));
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
    // ts: base.ts:989 — BitgetExchange inherits scheduleCancel, which rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // ts: base.ts:1138 — BitgetExchange inherits fetchPublicTrades, which rejects.
    fn fetch_public_trades(&self, _symbol: String, _limit: Option<f64>) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn fetch_historical_fills(&self, options: ExecutionsOptions) -> ExchangeFuture<'_, Vec<Execution>> {
        let a = args!(js(&options));
        self.invoke("fetchHistoricalFills", a)
    }
}
