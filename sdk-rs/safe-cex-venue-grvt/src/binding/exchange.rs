//! Production Grvt venue: the assembled class graph behind the typed Exchange trait.
use super::runtime::{Extra, Host};
use safe_cex_exchange::js_runtime::scope::ScopeShared;
use safe_cex_exchange::js_runtime::{
    globals::{install_builtins, install_enums},
    library::install_library,
    promise::await_js,
    store,
    value::{Js, Thrown, from_typed, to_typed},
};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    runtime::{ExchangeError, ExchangeFuture},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::{
    Candle, ExchangeAccount, ExchangeName, ExchangeOptions, FetchedLeverage, Market, OHLCVOptions,
    Order, OrderBook, PlaceOrderOpts, Position, PositionSide, PublicTrade, Ticker, UpdateOrderOpts,
    ValidateAccountResult,
};
use std::rc::Rc;

pub struct GrvtDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    /// HTTP wire host for every client the venue creates.
    pub http: Rc<dyn safe_cex_exchange::js_runtime::client::JsHttpHost>,
    /// Provider for `options.externalPublicData`; required when that option is set.
    pub external_public_data:
        Option<safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory>,
}

pub(crate) fn settings() -> ExchangeSettings {
    ExchangeSettings {
        tick_interval: None,
        positions_supported: Some(true),
        supports_public_only: true,
        client_order_id_rule: Some(super::assembly::client_order_id_rule()),
    }
}

pub struct GrvtExchange {
    rt: Host,
    capabilities: ExchangeCapabilities,
}

fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

impl GrvtExchange {
    // ts: GrvtExchange.constructor
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: GrvtDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Grvt {
            return Err(ExchangeError::new(
                "GrvtExchange requires the grvt venue name",
            ));
        }
        let shared = Rc::new_cyclic(|weak| {
            let globals = Js::object(vec![]);
            install_enums(&globals);
            install_builtins(&globals, weak.clone());
            install_library(&globals, weak.clone());
            ScopeShared::new(
                deps.runtime.clone(),
                deps.http.clone(),
                store.clone(),
                globals,
                Extra {
                    name: name.clone(),
                    options: options.clone(),
                    external_public_data: deps.external_public_data.clone(),
                    builder_error_proto: Js::object(vec![]),
                    nonce_ids: Default::default(),
                    nonce_times: Default::default(),
                    public: Default::default(),
                    private: Default::default(),
                },
            )
        });
        let host = Host::from_shared(shared.clone(), Js::Undefined);
        let receiver = super::runtime::thrown(crate::composition::new_exchange(
            &host,
            from_typed(&name).map_err(err)?,
            from_typed(&options).map_err(err)?,
            store::facade(store),
        ))?
        .base;
        *shared.exchange.borrow_mut() = receiver.clone();
        let venue = Rc::new(Self {
            rt: Host::from_shared(shared.clone(), receiver),
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
        let this = self.rt.0.this.clone();
        Box::pin(async move {
            let value = await_js(
                &self.rt.0.exchange_runtime(),
                this.call_method(member, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
            to_typed(&value).map_err(err)
        })
    }
    fn invoke_unit(&self, member: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt.0.this.clone();
        Box::pin(async move {
            await_js(
                &self.rt.0.exchange_runtime(),
                this.call_method(member, args).map_err(err)?,
            )
            .await
            .map_err(err)?;
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
        place_order: PlaceOrderBinding::implemented::<GrvtExchange>(Exchange::place_order),
        place_orders: PlaceOrdersBinding::implemented::<GrvtExchange>(Exchange::place_orders),
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

impl Exchange for GrvtExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.rt.0.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.rt
            .0
            .this
            .call_method("dispose", vec![])
            .map(|_| ())
            .map_err(err)
    }
    // ts: GrvtExchange.placeOrder — the ValidatedOrder carries the client-order-ID
    // contract, already enforced by BaseExchange.place_order before this call.
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
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        let a = args!(opt_js(symbols));
        self.invoke("fetchOrders", a)
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    // ts: GrvtExchange.refreshMarkets always replaces metadata, even for unchanged IDs.
    fn refresh_markets(&self) -> ExchangeFuture<'_, safe_cex_exchange::snapshots::MarketChanges> {
        let this = self.rt.0.this.clone();
        Box::pin(async move {
            let changes = await_js(
                &self.rt.0.exchange_runtime(),
                this.call_method("refreshMarkets", vec![]).map_err(err)?,
            )
            .await
            .map_err(err)?;
            Ok(safe_cex_exchange::snapshots::MarketChanges {
                added: to_typed(&changes.get("added").map_err(err)?).map_err(err)?,
                removed: to_typed(&changes.get("removed").map_err(err)?).map_err(err)?,
            })
        })
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        let a = args!(js(&options));
        self.invoke("fetchOHLCV", a)
    }
    fn fetch_leverage_for_symbol(
        &self,
        symbol: String,
    ) -> ExchangeFuture<'_, Option<FetchedLeverage>> {
        let a = args!(js(&symbol));
        self.invoke("fetchLeverageForSymbol", a)
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.invoke_unit("cancelAllOrders", vec![])
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&symbol));
        self.invoke_unit("cancelSymbolOrders", a)
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let callback = Js::function("candleCallback", move |_, args| {
            callback(to_typed(&safe_cex_exchange::js_runtime::value::arg(
                &args, 0,
            ))?);
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .0
            .this
            .call_method("listenOHLCV", vec![js(&options)?, callback])
            .map_err(err)?;
        Ok(Box::new(move || {
            cleanup.call(Js::Undefined, vec![]).map(|_| ()).map_err(err)
        }))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let callback = Js::function("bookCallback", move |_, args| {
            callback(to_typed(&safe_cex_exchange::js_runtime::value::arg(
                &args, 0,
            ))?);
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .0
            .this
            .call_method("listenOrderBook", vec![js(&symbol)?, callback])
            .map_err(err)?;
        Ok(Box::new(move || {
            cleanup.call(Js::Undefined, vec![]).map(|_| ()).map_err(err)
        }))
    }
    // ts: base.ts:799 — GrvtExchange inherits fetchSymbolMaxLeverage, which rejects.
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
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
    // ts: BaseExchange.changePositionMode and changeMarginType; GRVT has no overrides.
    fn change_position_mode(
        &self,
        _hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn change_margin_type(
        &self,
        _isolated: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn update_order(&self, options: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        let a = args!(js(&options));
        self.invoke("updateOrder", a)
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        let a = args!(js(&orders));
        self.invoke_unit("cancelOrders", a)
    }
    // ts: base.ts:989 — GrvtExchange inherits scheduleCancel, which rejects.
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    // ts: base.ts:1138 — GrvtExchange inherits fetchPublicTrades, which rejects.
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
}

#[cfg(test)]
mod interval_binding_tests {
    use super::*;
    #[allow(dead_code)]
    mod fixture {
        use crate as safe_cex_venue_grvt;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/binding_harness.rs"
        ));
    }
    #[test]
    fn assembled_convert_interval_preserves_all_inherited_source_members() {
        let compiler = std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").into()
        });
        let source = std::process::Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/interval.cjs"))
            .env("SAFE_CEX_TYPESCRIPT_PATH", compiler)
            .output()
            .unwrap();
        assert!(
            source.status.success(),
            "{}",
            String::from_utf8_lossy(&source.stderr)
        );
        let source: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
        assert_eq!(source["rows"].as_array().unwrap().len(), 93);
        let h = fixture::harness(
            safe_cex_types::ExchangeName::Grvt,
            fixture::options(),
            |_| {},
        );
        let mut inherited = 0;
        for row in source["rows"].as_array().unwrap() {
            let key = row["input"].as_str().unwrap();
            let actual = h
                .exchange
                .rt
                .0
                .this
                .call_method("convertInterval", vec![Js::str(key)])
                .unwrap_or_else(|e| {
                    panic!(
                        "assembled interval source member {key}: {}",
                        e.to_exchange_error().message
                    )
                });
            if let Some(mapped) = row["mapped"].as_str() {
                assert_eq!(
                    String::from_utf16_lossy(&actual.to_js_string().unwrap()),
                    mapped
                );
            } else {
                inherited += 1;
                assert_eq!(
                    String::from_utf16_lossy(&actual.to_js_string().unwrap()),
                    row["string"].as_str().unwrap(),
                    "assembled interval inherited text: {key}"
                );
                assert_eq!(
                    actual.type_of(),
                    row["type"].as_str().unwrap(),
                    "assembled interval inherited kind: {key}"
                );
                let length = actual.get("length").unwrap();
                if row["length"].is_null() {
                    assert!(length.is_undefined());
                } else {
                    assert_eq!(length.to_number().unwrap(), row["length"].as_f64().unwrap());
                }
                let again = h
                    .exchange
                    .rt
                    .0
                    .this
                    .call_method("convertInterval", vec![Js::str(key)])
                    .unwrap();
                assert!(
                    actual.same_reference(&again),
                    "inherited member identity: {key}"
                );
            }
        }
        assert_eq!(inherited, 12);
    }
}

#[cfg(test)]
#[path = "signing_admission_tests.rs"]
mod signing_admission_tests;

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "update_diagnostic_tests.rs"]
mod update_diagnostic_tests;
