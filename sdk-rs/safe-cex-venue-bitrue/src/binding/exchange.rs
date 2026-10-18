//! Concrete Bitrue Exchange implementation over its assembled native class.
use super::{Extra, Rt, classes};
use safe_cex_exchange::{
    capabilities::{ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding},
    client_order_id,
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    js_runtime::{
        base_exchange::{BaseExchangeParts, ExternalPublicDataFactory},
        client::JsHttpHost,
        promise::await_js,
        scope::ScopeShared,
        store,
        value::{Js, Thrown, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::{cell::RefCell, rc::Rc};

#[cfg(test)]
#[path = "../../tests/support/receiver_tests.rs"]
mod receiver_tests;

pub struct BitrueDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Rc<dyn JsHttpHost>,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}
pub struct BitrueExchange {
    rt: Rt,
    capabilities: ExchangeCapabilities,
}
fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}
fn js<T: serde::Serialize>(v: &T) -> Result<Js, ExchangeError> {
    from_typed(v).map_err(err)
}
fn opt<T: serde::Serialize>(v: Option<T>) -> Result<Js, ExchangeError> {
    v.as_ref().map_or(Ok(Js::Undefined), js)
}
macro_rules! args{($($v:expr),* $(,)?)=>{match(||->Result<Vec<Js>,ExchangeError>{Ok(vec![$($v?),*])})(){Ok(v)=>v,Err(e)=>return Box::pin(async move{Err(e)})}}}

impl BitrueExchange {
    /// ts: BitrueExchange constructor, retaining the shared base and both sockets.
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: BitrueDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Bitrue {
            return Err(ExchangeError::new("BitrueExchange requires bitrue"));
        }
        let settings = ExchangeSettings {
            tick_interval: Some(2000.),
            positions_supported: Some(true),
            supports_public_only: true,
            client_order_id_rule: Some(client_order_id::pattern_client_order_id_rule(
                "Bitrue".into(),
                "/^.{1,31}$/".into(),
                Rc::new(|id| {
                    let n = id.encode_utf16().count();
                    (1..=31).contains(&n)
                        && !id
                            .chars()
                            .any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
                }),
                Default::default(),
            )),
        };
        let shared = Rc::new(ScopeShared::new(
            deps.runtime,
            deps.http,
            store.clone(),
            Js::object(vec![]),
            Extra {
                parts: RefCell::new(Some(BaseExchangeParts {
                    name: name.clone(),
                    options: options.clone(),
                    store: store.clone(),
                    settings,
                    external_public_data: deps.external_public_data,
                })),
                supers: RefCell::new(vec![]),
                metadata: RefCell::new(vec![]),
            },
        ));
        classes::install(&shared.globals, Rc::downgrade(&shared)).map_err(err)?;
        let rt = Rt {
            shared: shared.clone(),
            this: Js::Undefined,
        };
        let receiver = crate::composition::construct_exchange(
            &rt,
            vec![js(&name)?, js(&options)?, store::facade(store)?],
        )
        .map_err(err)?;
        // Source base.ts getAccount, fetchTickers, changePositionMode,
        // scheduleCancel and fetchPublicTrades are async rejections inherited
        // unchanged by Bitrue. The shared JS facade omits these optional members.
        for name in [
            "getAccount",
            "fetchTickers",
            "changePositionMode",
            "scheduleCancel",
            "fetchPublicTrades",
        ] {
            let runtime = shared.runtime.clone();
            receiver
                .define(
                    name,
                    Js::function(name, move |_, _| {
                        let runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> =
                            runtime.clone();
                        Ok(Js::Promise(
                            safe_cex_exchange::js_runtime::promise::Promise::spawn(
                                &runtime,
                                Box::pin(async { Err(Thrown::error("Error", "Not implemented")) }),
                            ),
                        ))
                    }),
                )
                .map_err(err)?;
        }
        *shared.exchange.borrow_mut() = receiver.clone();
        let venue = Rc::new(Self {
            rt: rt.with_this(receiver),
            capabilities: ExchangeCapabilities {
                place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
                place_orders: PlaceOrdersBinding::implemented::<Self>(Exchange::place_orders),
                ..Default::default()
            },
        });
        let dynamic: Rc<dyn Exchange> = venue.clone();
        *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&dynamic));
        Ok(venue)
    }
    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        name: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            let value = await_js(
                &self.rt.exchange_runtime(),
                this.call_method(name, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        name: &str,
        first: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let reporter = self.rt.shared.runtime.clone();
        let listener = Js::function("callback", move |_, a| {
            match to_typed::<T>(&safe_cex_exchange::js_runtime::value::arg(&a, 0)) {
                Ok(value) => callback(value),
                Err(error) => reporter.report_exception(
                    &error.to_exchange_error(),
                    &serde_json::json!({"source":"bitrue listener payload"}).into(),
                ),
            }
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .this
            .call_method(name, vec![first, listener])
            .map_err(err)?;
        Ok(Box::new(move || {
            cleanup.call(Js::Undefined, vec![]).map_err(err)?;
            Ok(())
        }))
    }
    fn unit(&self, name: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            await_js(
                &self.rt.exchange_runtime(),
                this.call_method(name, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
            Ok(())
        })
    }
}
impl Exchange for BitrueExchange {
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
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        let orders: Vec<_> = orders
            .orders()
            .iter()
            .map(|o| o.options().clone())
            .collect();
        let a = args!(js(&orders));
        self.invoke("placeOrders", a)
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
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.unit("start", vec![])
    }
    // These members are inherited from the actual BaseExchange, including its
    // exact rejection for methods Bitrue does not implement (base.ts).
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.invoke("getAccount", vec![])
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", vec![])
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        let a = args!(opt(symbols));
        self.invoke("fetchPositions", a)
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        let a = args!(opt(symbols));
        self.invoke("fetchOrders", a)
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    fn fetch_symbol_max_leverage(&self, symbol: String) -> ExchangeFuture<'_, f64> {
        let a = args!(js(&symbol));
        self.invoke("fetchSymbolMaxLeverage", a)
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        _side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol), js(&leverage));
        self.unit("setLeverage", a)
    }
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&hedged), opt(symbol));
        self.unit("changePositionMode", a)
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&isolated), opt(symbol));
        self.unit("changeMarginType", a)
    }
    fn update_symbol_metadata(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol));
        self.unit("updateSymbolMetadata", a)
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(&options));
        self.invoke("updateOrder", a)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&orders));
        self.unit("cancelOrders", a)
    }
    fn schedule_cancel(&self, time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        let a = args!(opt(time));
        self.unit("scheduleCancel", a)
    }
    fn fetch_public_trades(
        &self,
        symbol: String,
        limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        let a = args!(js(&symbol), opt(limit));
        self.invoke("fetchPublicTrades", a)
    }
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        let a = args!(js(&options));
        self.invoke("fetchHistoricalFills", a)
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let a = args!(js(&options));
        self.invoke("fetchOHLCV", a)
    }
}
