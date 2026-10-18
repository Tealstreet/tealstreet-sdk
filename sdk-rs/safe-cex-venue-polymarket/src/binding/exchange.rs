//! Production Polymarket exchange over the shared native JS runtime.
use super::{Extra, Rt, Shared, globals};
use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding},
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedOrder},
    js_runtime::{
        promise::await_js,
        scope::ScopeShared,
        store,
        value::{Js, Thrown, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::rc::Rc;

pub struct PolymarketDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Rc<dyn safe_cex_exchange::js_runtime::client::JsHttpHost>,
    pub external_public_data:
        Option<safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory>,
}
pub(crate) fn settings() -> ExchangeSettings {
    ExchangeSettings {
        tick_interval: None,
        positions_supported: Some(true),
        supports_public_only: true,
        client_order_id_rule: None,
    }
}
pub struct PolymarketExchange {
    rt: Rt,
    capabilities: ExchangeCapabilities,
}
fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}
impl PolymarketExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: PolymarketDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Polymarket {
            return Err(ExchangeError::new("PolymarketExchange requires polymarket"));
        }
        let shared = Rc::new_cyclic(|weak: &std::rc::Weak<Shared>| {
            ScopeShared::new(
                deps.runtime.clone(),
                deps.http.clone(),
                store.clone(),
                globals::build(weak.clone()),
                Extra {
                    name: name.clone(),
                    options: options.clone(),
                    external: deps.external_public_data.clone(),
                    classes: Default::default(),
                },
            )
        });
        let rt = Rt {
            shared: shared.clone(),
            this: Js::Undefined,
        };
        shared
            .globals
            .define(
                "BaseWebSocket",
                safe_cex_exchange::js_runtime::sockets::base_websocket_class(&shared.sockets),
            )
            .map_err(err)?;
        let classes =
            Rc::new(crate::classes::PolymarketClasses::new(Rc::new(rt.clone())).map_err(err)?);
        shared
            .extra
            .classes
            .set(classes)
            .map_err(|_| ExchangeError::new("Classes initialized twice"))?;
        let class =
            crate::exchange::PolymarketExchangeClass::new(Rc::new(rt.clone())).map_err(err)?;
        let receiver = crate::exchange::PolymarketExchange::new(
            &class,
            from_typed(&name).map_err(err)?,
            from_typed(&options).map_err(err)?,
            store::facade(store)?,
        )
        .map_err(err)?
        .receiver();
        *shared.exchange.borrow_mut() = receiver.clone();
        let venue = Rc::new(Self {
            rt: rt.with_this(receiver),
            capabilities: ExchangeCapabilities {
                start: CapabilityBinding::implemented(|v, ()| v.start()),
                place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
                ..Default::default()
            },
        });
        let erased: Rc<dyn Exchange> = venue.clone();
        *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&erased));
        Ok(venue)
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

impl PolymarketExchange {
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        subject: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let deliver = Js::function("", move |_, a| {
            callback(to_typed(&a.into_iter().next().unwrap_or(Js::Undefined))?);
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .this
            .call_method(member, vec![subject, deliver])
            .map_err(err)?;
        Ok(Box::new(move || {
            if cleanup.is_callable() {
                cleanup.call(Js::Undefined, vec![]).map_err(err)?;
            }
            Ok(())
        }))
    }
}

fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    from_typed(value).map_err(err)
}

fn opt_js<T: serde::Serialize>(value: Option<T>) -> Result<Js, ExchangeError> {
    value.map_or(Ok(Js::Undefined), |v| js(&v))
}

fn inherited_rejection<T: 'static>() -> ExchangeFuture<'static, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

macro_rules! args {
    ($($e:expr),*) => {
        match (|| -> Result<Vec<Js>, ExchangeError> { Ok(vec![$($e?),*]) })() {
            Ok(v) => v,
            Err(e) => return Box::pin(async move { Err(e) }),
        }
    };
}

impl Exchange for PolymarketExchange {
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
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(order.options()));
        self.invoke("placeOrder", a)
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
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.invoke("fetchOrders", vec![])
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        self.invoke("fetchSymbolMaxLeverage", args!(js(&_symbol)))
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
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&hedged), opt_js(symbol));
        self.invoke_unit("changePositionMode", a)
    }
    fn change_margin_type(
        &self,
        _isolated: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("changeMarginType", args!(js(&_isolated), opt_js(_symbol)))
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(&options));
        self.invoke("updateOrder", a)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&orders));
        self.invoke_unit("cancelOrders", a)
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("cancelAllOrders", vec![])
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let a = args!(js(&options));
        self.invoke("fetchOHLCV", a)
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.listen("listenOHLCV", js(&options)?, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.listen("listenOrderBook", js(&symbol)?, callback)
    }
    // ts: BaseExchange.scheduleCancel; Polymarket inherits this exact rejection.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        inherited_rejection()
    }
    // ts: BaseExchange.fetchPublicTrades; Polymarket inherits this exact rejection.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        inherited_rejection()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "receiver_tests.rs"]
mod receiver_tests;
