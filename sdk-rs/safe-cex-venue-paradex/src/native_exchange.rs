//! Paradex's constructed source workflows over the shared runtime.
use crate::binding::{self, Rt};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedBatch, ValidatedOrder},
    js_runtime::{
        base_exchange::{BaseExchangeParts, ExternalPublicDataFactory},
        client::JsHttpHost,
        promise::await_js,
        value::{Js, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::rc::Rc;
pub type ParadexHttpFactory =
    Rc<dyn Fn(&ExchangeOptions) -> Result<Rc<dyn JsHttpHost>, ExchangeError>>;
pub struct ParadexDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: ParadexHttpFactory,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}
pub struct ParadexExchange {
    rt: Rt,
    capabilities: ExchangeCapabilities,
}
impl ParadexExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: ParadexDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Paradex {
            return Err(ExchangeError::new(
                "ParadexExchange requires the paradex exchange name",
            ));
        }
        let external = deps.external_public_data.clone();
        let runtime = deps.runtime.clone();
        let rule = safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
            "Paradex".into(),
            "/^.{1,64}$/".into(),
            Rc::new(|s| {
                let u: Vec<_> = s.encode_utf16().collect();
                (1..=64).contains(&u.len())
                    && !u.iter().any(|c| matches!(c, 10 | 13 | 0x2028 | 0x2029))
            }),
            safe_cex_exchange::client_order_id::ClientOrderIdRuleOverrides {
                is_honorable_shape: Some(Rc::new(|o| {
                    safe_cex_exchange::client_order_id::is_regular_client_order_shape(o)
                        && o.close != Some(true)
                })),
                mint: Some(Rc::new(move || format!("ts_{}", runtime.unique_id()))),
            },
        );
        let rt = binding::assemble(
            BaseExchangeParts {
                name,
                options,
                store,
                settings: ExchangeSettings {
                    supports_public_only: true,
                    positions_supported: Some(true),
                    client_order_id_rule: Some(rule),
                    ..Default::default()
                },
                external_public_data: external,
            },
            deps,
        )?;
        let venue = Rc::new(Self {
            rt,
            capabilities: ExchangeCapabilities {
                start: CapabilityBinding::implemented(|v, ()| v.start()),
                place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
                place_orders: PlaceOrdersBinding::implemented::<Self>(Exchange::place_orders),
                ..Default::default()
            },
        });
        let erased: Rc<dyn Exchange> = venue.clone();
        *venue.rt.shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&erased));
        Ok(venue)
    }
    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        n: &'static str,
        a: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let this = self.rt.this.clone();
        Box::pin(async move { Ok(to_typed(&await_js(this.call_method(n, a)?).await?)?) })
    }
    fn unit(&self, n: &'static str, a: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            await_js(this.call_method(n, a)?).await?;
            Ok(())
        })
    }
}
fn js<T: serde::Serialize>(v: &T) -> Result<Js, ExchangeError> {
    Ok(from_typed(v)?)
}
macro_rules! args {($($v:expr),*)=>{match(||->Result<Vec<Js>,ExchangeError>{Ok(vec![$($v?),*])})(){Ok(a)=>a,Err(e)=>return Box::pin(async move{Err(e)})}};}
impl Exchange for ParadexExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.rt.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn is_public_data_only(&self) -> bool {
        let o = self.base().options();
        o.public_data_only == Some(true)
            || o.key.is_empty()
            || o.paradex_address.as_deref().is_none_or(str::is_empty)
            || o.subkey_address.as_deref().is_none_or(str::is_empty)
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        self.unit("start", vec![])
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        self.invoke("getAccount", vec![])
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        self.invoke("validateAccount", vec![])
    }
    fn fetch_positions(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        self.invoke("fetchPositions", vec![])
    }
    fn fetch_orders(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.invoke("fetchOrders", vec![])
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        self.invoke("fetchMarkets", vec![])
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        self.invoke("fetchTickers", vec![])
    }
    // Inherited source BaseExchange.fetchSymbolMaxLeverage:832.
    fn fetch_symbol_max_leverage(&self, _: String) -> ExchangeFuture<'_, f64> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn set_leverage(&self, s: String, l: f64, _: Option<PositionSide>) -> ExchangeFuture<'_, ()> {
        self.unit("setLeverage", args!(js(&s), js(&l)))
    }
    // Source inherits these BaseExchange unsupported methods.
    fn change_position_mode(&self, _: bool, _: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn change_margin_type(&self, _: bool, _: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn place_order(&self, o: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        if let Err(e) = self.base().resolve_caller_client_order_id(o.options()) {
            return Box::pin(async move { Err(e) });
        }
        self.invoke("placeOrder", args!(js(o.options())))
    }
    fn place_orders<'a>(&'a self, b: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        let o: Vec<_> = b.orders().iter().map(|o| o.options()).collect();
        self.invoke("placeOrders", args!(js(&o)))
    }
    fn update_order(&self, o: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        self.invoke("updateOrder", args!(js(&o)))
    }
    fn cancel_orders(&self, o: Vec<Order>) -> ExchangeFuture<'_, ()> {
        self.unit("cancelOrders", args!(js(&o)))
    }
    fn schedule_cancel(&self, _: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn fetch_public_trades(
        &self,
        _: String,
        _: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn fetch_ohlcv(&self, o: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        self.invoke("fetchOHLCV", args!(js(&o)))
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.rt.this.call_method("dispose", vec![])?;
        Ok(())
    }
}
impl Drop for ParadexExchange {
    fn drop(&mut self) {
        if !self.rt.base().is_disposed() {
            if let Err(e) = self.dispose() {
                self.rt
                    .shared
                    .runtime
                    .report_exception(&e, &serde_json::json!({"source":"ParadexExchange.drop"}));
            }
        }
        for (_, task) in self.rt.shared.timers.borrow_mut().drain(..) {
            task.cancel();
        }
        self.rt.shared.sockets.cores.borrow_mut().clear();
    }
}
