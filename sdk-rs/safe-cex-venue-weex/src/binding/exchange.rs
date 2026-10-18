//! Concrete WEEX composition over native bodies and shared runtime services.
use super::{Host, Js, Thrown, arg, await_js, from_typed, rt, to_typed};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ValidatedBatch, ValidatedOrder},
    js_runtime::{
        client::JsHttpHost,
        globals::{install_builtins, install_enums},
        library::install_library,
        scope::{Scope, ScopeShared},
        store,
    },
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::{
    Balance, Candle, ExchangeAccount, ExchangeName, ExchangeOptions, Execution, ExecutionsOptions,
    FetchedLeverage, IncomeHistoryOptions, IncomeRecord, Market, OHLCVOptions, Order, OrderBook,
    PlaceOrderOpts, Position, PositionSide, PublicTrade, Ticker, UpdateOrderOpts,
    ValidateAccountResult,
};
use std::rc::Rc;

pub struct WeexDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Rc<dyn JsHttpHost>,
    pub external_public_data:
        Option<safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory>,
    pub local_wall_time_to_utc: Rc<dyn Fn(f64) -> f64>,
}
pub struct WeexExchange {
    host: Host,
    capabilities: ExchangeCapabilities,
}
fn err(error: Thrown) -> ExchangeError {
    error.to_exchange_error()
}
fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    from_typed(value).map_err(err)
}
fn optional<T: serde::Serialize>(value: Option<T>) -> Result<Js, ExchangeError> {
    value.map_or(Ok(Js::Undefined), |v| js(&v))
}
macro_rules! args { ($($value:expr),*)=>{match (||->Result<Vec<Js>,ExchangeError>{Ok(std::vec![$($value?),*])})(){Ok(args)=>args,Err(error)=>return Box::pin(async move{Err(error)})}}; }
impl WeexExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: WeexDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Weex {
            return Err(ExchangeError::new("WeexExchange requires weex"));
        }
        let globals = Js::object(std::vec![]);
        let shared = Rc::new(ScopeShared::new(
            deps.runtime,
            deps.http,
            store.clone(),
            globals,
            rt::Extra {
                name: name.clone(),
                options: options.clone(),
                external_public_data: deps.external_public_data,
                local_to_utc: deps.local_wall_time_to_utc,
                prototypes: Default::default(),
            },
        ));
        install_builtins(&shared.globals, Rc::downgrade(&shared));
        install_enums(&shared.globals);
        install_library(&shared.globals, Rc::downgrade(&shared));
        let class = crate::classes::WeexExchange::new(
            Rc::new(super::classes::Factory(Rc::downgrade(&shared))),
            js(&name)?,
            js(&options)?,
            store::facade(store),
        )
        .map_err(err)?;
        *shared.exchange.borrow_mut() = class.receiver.clone();
        let venue = Rc::new(Self {
            host: Host(Scope {
                shared: shared.clone(),
                this: class.receiver,
            }),
            capabilities: ExchangeCapabilities {
                start: CapabilityBinding::implemented(|venue, ()| venue.start()),
                place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
                place_orders: PlaceOrdersBinding::implemented::<Self>(Exchange::place_orders),
                fetch_historical_fills: CapabilityBinding::implemented(|venue, opts| {
                    venue.fetch_historical_fills(opts)
                }),
                fetch_income_history: CapabilityBinding::implemented(|venue, opts| {
                    venue.fetch_income_history(opts)
                }),
                fetch_income_history_range: CapabilityBinding::implemented(
                    |venue, (start, end, kind)| venue.fetch_income_history_range(start, end, kind),
                ),
                ..Default::default()
            },
        });
        let dynamic: Rc<dyn Exchange> = venue.clone();
        *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&dynamic));
        Ok(venue)
    }
    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let receiver = self.host.this();
        Box::pin(async move {
            let value = await_js(&self.host.0.exchange_runtime(), receiver.call_method(member, args).map_err(err)?)
                .await
                .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    fn unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let receiver = self.host.this();
        Box::pin(async move {
            await_js(&self.host.0.exchange_runtime(), receiver.call_method(member, args).map_err(err)?)
                .await
                .map_err(err)?;
            Ok(())
        })
    }
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        subject: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let deliver = Js::function("deliver", move |_, args| {
            callback(to_typed(&arg(&args, 0))?);
            Ok(Js::Undefined)
        });
        let cleanup = self
            .host
            .call(member, std::vec![subject, deliver])
            .map_err(err)?;
        Ok(Box::new(move || {
            cleanup.call(Js::Undefined, std::vec![]).map_err(err)?;
            Ok(())
        }))
    }
    pub fn fetch_balance(&self) -> ExchangeFuture<'_, Balance> {
        self.invoke("fetchBalance", std::vec![])
    }
    pub fn fetch_order_book(
        &self,
        symbol: String,
        limit: Option<f64>,
    ) -> ExchangeFuture<'_, OrderBook> {
        let args = args!(js(&symbol), optional(limit));
        self.invoke("fetchOrderBook", args)
    }
}
impl Exchange for WeexExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.host.0.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn class_name(&self) -> &'static str {
        "WeexExchange"
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.unit("start", std::vec![])
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.invoke("getAccount", std::vec![])
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", std::vec![])
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", std::vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", std::vec![])
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        let args = args!(optional(symbols));
        self.invoke("fetchPositions", args)
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        let args = args!(optional(symbols));
        self.invoke("fetchOrders", args)
    }
    fn fetch_leverage_for_symbol(
        &self,
        symbol: String,
    ) -> ExchangeFuture<'_, Option<FetchedLeverage>> {
        let args = args!(js(&symbol));
        self.invoke("fetchLeverageForSymbol", args)
    }
    fn nuke(&self, _tries: u32) -> ExchangeFuture<'_, ()> {
        self.unit("nuke", std::vec![])
    }
    fn nuke_positions(&self, _tries: u32) -> ExchangeFuture<'_, ()> {
        self.unit("nukePositions", std::vec![])
    }
    fn fetch_symbol_max_leverage(&self, symbol: String) -> ExchangeFuture<'_, f64> {
        let args = args!(js(&symbol));
        self.invoke("fetchSymbolMaxLeverage", args)
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        let args = args!(js(&symbol), js(&leverage), optional(side));
        self.unit("setLeverage", args)
    }
    fn change_position_mode(&self, hedged: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let args = args!(js(&hedged), optional(symbol));
        self.unit("changePositionMode", args)
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let args = args!(js(&isolated), optional(symbol));
        self.unit("changeMarginType", args)
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let args = args!(js(&options));
        self.invoke("updateOrder", args)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let args = args!(js(&orders));
        self.unit("cancelOrders", args)
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        let args = args!(js(&symbol));
        self.unit("cancelSymbolOrders", args)
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.unit("cancelAllOrders", std::vec![])
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let args = args!(js(order.options()));
        self.invoke("placeOrder", args)
    }
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        let args = args!(js(&orders
            .orders()
            .iter()
            .map(|o| o.options())
            .collect::<Vec<_>>()));
        self.invoke("placeOrders", args)
    }
    // ts: base.ts scheduleCancel — Weex inherits the source rejection.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn fetch_public_trades(
        &self,
        symbol: String,
        limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        let args = args!(js(&symbol), optional(limit));
        self.invoke("fetchPublicTrades", args)
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let args = args!(js(&options));
        self.invoke("fetchOHLCV", args)
    }
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        let args = args!(js(&options));
        self.invoke("fetchHistoricalFills", args)
    }
    fn fetch_income_history(
        &self,
        options: Option<IncomeHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        let args = args!(optional(options));
        self.invoke("fetchIncomeHistory", args)
    }
    fn fetch_income_history_range(
        &self,
        start: f64,
        end: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        let args = args!(js(&start), js(&end), optional(kind));
        self.invoke("fetchIncomeHistoryRange", args)
    }
    fn update_symbol_metadata(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        let args = args!(js(&symbol));
        self.unit("updateSymbolMetadata", args)
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
}
