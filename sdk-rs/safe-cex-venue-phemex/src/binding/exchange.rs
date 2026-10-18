//! Production Phemex venue: the assembled class graph behind the typed Exchange trait.
#[cfg(test)]
#[path = "order_regressions.rs"]
mod order_regressions;
use super::{
    globals,
    rt::{ExternalPublicDataFactory, Extra, Host, Rt, Shared, TransportInstaller},
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
        value::{Js, Thrown, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::rc::Rc;

pub struct PhemexDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    /// HTTP wire host for every client the venue creates.
    pub http: Rc<dyn JsHttpHost>,
    /// Install the caller's tunnel adapter when options.tealstreetTunnel is set.
    pub install_transport: Option<TransportInstaller>,
    /// Provider for `options.externalPublicData`; required when that option is set.
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

// ts: exchangeMetadata.phemex and PhemexExchange.clientOrderIdRule.
pub(crate) fn settings(rt: &Rt) -> Result<ExchangeSettings, Thrown> {
    use safe_cex_exchange::client_order_id::{
        ClientOrderIdRuleOverrides, pattern_client_order_id_rule,
    };
    let runtime = rt.shared.runtime.clone();
    let rule = pattern_client_order_id_rule(
        "Phemex".into(),
        "/^.{1,40}$/".into(),
        Rc::new(|id| {
            (1..=40).contains(&id.encode_utf16().count())
                && !id
                    .chars()
                    .any(|c| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        }),
        ClientOrderIdRuleOverrides {
            mint: Some(Rc::new(move || {
                // Same nanoid alphabet/random draws and UUID stripping as the shared library.
                let alphabet = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";
                let id: String = (0..21)
                    .map(|_| alphabet[(runtime.random() * 64.0) as usize & 63] as char)
                    .filter(|c| *c != '-' && *c != '_')
                    .collect();
                format!("Tealstreet_{id}")
            })),
            ..Default::default()
        },
    );
    Ok(ExchangeSettings {
        tick_interval: Some(2000.0),
        positions_supported: None,
        supports_public_only: true,
        client_order_id_rule: Some(rule),
    })
}

pub struct PhemexExchange {
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

/// The source BaseExchange default for a member Phemex does not override.
fn not_implemented<'a, T: 'a>() -> ExchangeFuture<'a, T> {
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}

impl PhemexExchange {
    // ts: PhemexExchange.constructor
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: PhemexDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Phemex {
            return Err(ExchangeError::new(format!(
                "PhemexExchange cannot construct {}",
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
                Extra {
                    name: name.clone(),
                    options: options.clone(),
                    external_public_data: deps.external_public_data.clone(),
                    install_transport: deps.install_transport.clone(),
                },
            )
        });
        let mut host = Host {
            rt: Rt {
                shared: shared.clone(),
                this: Js::Undefined,
            },
        };
        let js_name = from_typed(&name).map_err(err)?;
        let js_options = from_typed(&options).map_err(err)?;
        let js_store = store::facade(store.clone());
        crate::start_order_formatting::new_exchange(&mut host, js_name, js_options, js_store)
            .map_err(err)?;
        let rt = host.rt;
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
    fn listen<T: serde::de::DeserializeOwned + 'static>(
        &self,
        member: &'static str,
        first: Js,
        callback: Rc<dyn Fn(T)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let listener = Js::function("callback", move |_, args| {
            callback(to_typed(&safe_cex_exchange::js_runtime::value::arg(
                &args, 0,
            ))?);
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .this
            .get("publicWebsocket")
            .map_err(err)?
            .call_method(member, vec![first, listener])
            .map_err(err)?;
        Ok(Box::new(move || {
            cleanup.call(Js::Undefined, vec![]).map_err(err)?;
            Ok(())
        }))
    }
}

fn capabilities() -> ExchangeCapabilities {
    ExchangeCapabilities {
        start: CapabilityBinding::implemented(|venue, ()| venue.start()),
        place_order: PlaceOrderBinding::implemented::<PhemexExchange>(Exchange::place_order),
        fetch_historical_fills: CapabilityBinding::implemented(|venue, opts| {
            venue.fetch_historical_fills(opts)
        }),
        place_orders: PlaceOrdersBinding::implemented::<PhemexExchange>(Exchange::place_orders),
        ..Default::default()
    }
}

impl Exchange for PhemexExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.rt.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        // The source body invokes superclass cleanup once before its sockets;
        // do not add another base disposal ahead of that body.
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
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let options = js(&options)?;
        let symbol = self
            .rt
            .this
            .call_method("formatSymbolOut", vec![options.get("symbol").map_err(err)?])
            .map_err(err)?;
        options.define("symbol", symbol).map_err(err)?;
        self.listen("listenOHLCV", options, callback)
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let symbol = self
            .rt
            .this
            .call_method("formatSymbolOut", vec![js(&symbol)?])
            .map_err(err)?;
        self.listen("listenOrderBook", symbol, callback)
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        self.invoke("fetchOHLCV", args!(js(&options)))
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.invoke("getAccount", vec![])
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", vec![])
    }
    // ts: fetchPositions / fetchOrders take no symbol filter.
    fn fetch_positions(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.invoke("fetchPositions", args!(opt_js(_symbols)))
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
    // ts: PhemexExchange.fetchHistoricalFills, symbol-scoped primary and legacy routes.
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        self.invoke("fetchHistoricalFills", args!(js(&options)))
    }
}
