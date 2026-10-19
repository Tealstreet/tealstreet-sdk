//! Production Orderly family composition on the shared value/runtime layer.
use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding},
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedOrder},
    js_runtime::{
        base_exchange::{BaseExchangeParts, ExternalPublicDataFactory},
        client::JsHttpHost,
        promise::await_js,
        value::{Js, from_typed, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::rc::Rc;
pub type OrderlyHttpFactory =
    Rc<dyn Fn(&ExchangeOptions) -> Result<Rc<dyn JsHttpHost>, ExchangeError>>;
pub struct OrderlyDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: OrderlyHttpFactory,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}
pub struct OrderlyExchange {
    rt: crate::binding::Rt,
    capabilities: ExchangeCapabilities,
}
fn client_id_is_valid(id: &str) -> bool {
    let n = id.encode_utf16().count();
    n > 0 && n <= 36 && !id.starts_with('-') && !id.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
}
impl OrderlyExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: OrderlyDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        use crate::classes::Kind;
        let kind = match name {
            ExchangeName::Kodiakfi => Kind::KodiakFI,
            ExchangeName::Mode => Kind::Mode,
            ExchangeName::Salsadex => Kind::SalsaDEX,
            ExchangeName::TealstreetDex => Kind::TealstreetDEX,
            ExchangeName::What => Kind::What,
            ExchangeName::Woofi => Kind::Woofi,
            _ => {
                return Err(ExchangeError::new(
                    "OrderlyExchange requires an Orderly family name",
                ));
            }
        };
        let rule = safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
            "Orderly".into(),
            "/^(?!-).{1,36}$/".into(),
            Rc::new(client_id_is_valid),
            Default::default(),
        );
        let external = deps.external_public_data.clone();
        let rt = crate::binding::assemble(
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
            kind,
        )?;
        let venue = Rc::new(Self {
            rt,
            capabilities: ExchangeCapabilities {
                start: CapabilityBinding::implemented(|venue, ()| venue.start()),
                place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
                ..Default::default()
            },
        });
        let erased: Rc<dyn Exchange> = venue.clone();
        *venue.rt.shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&erased));
        Ok(venue)
    }
    fn invoke<T: serde::de::DeserializeOwned + 'static>(
        &self,
        name: &'static str,
        args: Vec<Js>,
    ) -> ExchangeFuture<'_, T> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            Ok(to_typed(
                &await_js(&self.rt.exchange_runtime(), this.call_method(name, args)?).await?,
            )?)
        })
    }
    fn unit(&self, name: &'static str, args: Vec<Js>) -> ExchangeFuture<'_, ()> {
        let this = self.rt.this.clone();
        Box::pin(async move {
            await_js(&self.rt.exchange_runtime(), this.call_method(name, args)?).await?;
            Ok(())
        })
    }
}
impl Drop for OrderlyExchange {
    fn drop(&mut self) {
        if !self.rt.base().is_disposed() {
            if let Err(e) = self.dispose() {
                self.rt.shared.runtime.report_exception(
                    &e,
                    &serde_json::json!({"source":"OrderlyExchange.drop"}).into(),
                );
            }
        }
        for (_, task) in self.rt.shared.timers.borrow_mut().drain(..) {
            task.cancel();
        }
        self.rt.shared.sockets.cores.borrow_mut().clear();
    }
}
fn js<T: serde::Serialize>(value: &T) -> Result<Js, ExchangeError> {
    Ok(from_typed(value)?)
}
macro_rules! args{($($x:expr),*$(,)?)=>{match(||->Result<Vec<Js>,ExchangeError>{Ok(vec![$(js(&$x)?),*])})(){Ok(args)=>args,Err(e)=>return Box::pin(async move{Err(e)})}};}
impl Exchange for OrderlyExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.rt.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
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
    // TS inherits base.ts:799,832,989,1138 for these four operations.
    fn fetch_symbol_max_leverage(&self, _: String) -> ExchangeFuture<'_, f64> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
    }
    fn change_position_mode(&self, _: bool, _: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Err(ExchangeError::new("Not implemented")) })
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
    fn set_all_leverage(&self, leverage: f64) -> ExchangeFuture<'_, ()> {
        self.unit("setAllLeverage", args![leverage])
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        _: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        self.unit("setLeverage", args!(symbol, leverage))
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        let symbol = symbol.map_or(Js::Undefined, |s| Js::str(&s));
        self.unit("changeMarginType", vec![Js::Bool(isolated), symbol])
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        if let Err(e) = self.base().resolve_caller_client_order_id(order.options()) {
            return Box::pin(async move { Err(e) });
        }
        self.invoke("placeOrder", args!(order.options()))
    }
    fn update_order(&self, opts: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        self.invoke("updateOrder", args!(opts))
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.unit("cancelAllOrders", vec![])
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        self.unit("cancelSymbolOrders", args![symbol])
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        self.unit("cancelOrders", args!(orders))
    }
    fn fetch_ohlcv(&self, opts: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        self.invoke("fetchOHLCV", args!(opts))
    }
    fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let cb = Js::function("", move |_, a| {
            callback(to_typed(&safe_cex_exchange::js_runtime::value::arg(&a, 0))?);
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .this
            .call_method("listenOHLCV", vec![js(&opts)?, cb])?;
        Ok(Box::new(move || {
            cleanup.call(Js::Undefined, vec![])?;
            Ok(())
        }))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let cb = Js::function("", move |_, a| {
            callback(to_typed(&safe_cex_exchange::js_runtime::value::arg(&a, 0))?);
            Ok(Js::Undefined)
        });
        let cleanup = self
            .rt
            .this
            .call_method("listenOrderBook", vec![Js::str(&symbol), cb])?;
        Ok(Box::new(move || {
            cleanup.call(Js::Undefined, vec![])?;
            Ok(())
        }))
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.rt.this.call_method("dispose", vec![])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_signing_failure_sends_nothing_and_uses_shared_recovery() {
        let (venue, runtime, _, _) = crate::binding_test_support::fixture(
            serde_json::json!({"key":"key","secret":"invalid!","applicationId":"account"}),
        );
        venue
            .rt
            .this
            .get("privateWebsocket")
            .unwrap()
            .call_method("connectAndSubscribe", vec![])
            .unwrap();
        for _ in 0..12 {
            runtime.pump();
        }
        assert!(
            !runtime.urls.borrow().is_empty(),
            "actual private socket dialed"
        );
        assert!(
            runtime.sent.borrow().is_empty(),
            "signing failure sends neither auth nor subscriptions"
        );
        assert!(
            runtime
                .console
                .borrow()
                .iter()
                .any(|message| message.contains("signing could not complete; reconnecting")),
            "shared signing recovery invoked"
        );
        assert!(
            runtime
                .console
                .borrow()
                .iter()
                .any(|message| message.contains("Explicit reconnect call")),
            "reconnect attempted"
        );
        assert_eq!(
            runtime.errors.borrow().as_slice(),
            ["Non-base58 character"],
            "shared recovery must report the original signing cause exactly once"
        );
        assert!(
            !runtime
                .console
                .borrow()
                .iter()
                .chain(runtime.errors.borrow().iter())
                .any(|message| message.contains("auth failed"))
        );
        crate::binding_test_support::drive(&runtime, async {
            venue.dispose().unwrap();
        });
    }
}

#[cfg(test)]
#[path = "../tests/support/structural_aliases.rs"]
mod structural_alias_tests;
