use super::hosts::Host;
use crate::exchange::{BingxExchange as Bodies, BingxHosts};
use safe_cex_exchange::{
    capabilities::{
        CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding, PlaceOrdersBinding,
    },
    exchange::{BaseExchange, Exchange, ValidatedBatch, ValidatedOrder},
    js_runtime::{base_exchange::ExternalPublicDataFactory, client::JsHttpHost},
    runtime::{ExchangeError, ExchangeFuture},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::rc::Rc;

pub struct BingxDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Rc<dyn JsHttpHost>,
    pub local_wall_time_to_utc: Rc<dyn Fn(f64) -> f64>,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

/// Binds the already-ported BingX bodies; all platform capabilities use js_runtime.
pub struct BingxExchange {
    inner: Rc<Bodies>,
    host: Rc<Host>,
    capabilities: ExchangeCapabilities,
}
impl BingxExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: BingxDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Bingx {
            return Err(ExchangeError::new("BingxExchange requires bingx"));
        }
        if (options.remote_signer.is_some() && deps.http.hosted_signer().is_none())
            || options.payload_signer.is_some()
        {
            return Err(ExchangeError::new(
                "BingX hosted signers require a host signing capability",
            ));
        }
        if options.external_public_data.is_some() && deps.external_public_data.is_none() {
            return Err(ExchangeError::new(
                "BingX externalPublicData requires a provider",
            ));
        }
        let runtime = deps.runtime.clone();
        let host = Host::new(options.clone(), store.clone(), deps);
        let inner = Bodies::new(
            name,
            options,
            store,
            runtime,
            BingxHosts {
                api: host.clone(),
                exchange: host.clone(),
                history: host.clone(),
                placement: host.clone(),
                public_socket: host.clone(),
                subscription: host.clone(),
                private_keys: host.clone(),
                private_events: host.clone(),
                private_message: host.clone(),
                bindings: host.clone(),
            },
        )?;
        host.configure_signing(&inner.unlimited_xhr)?;
        *host.inner.borrow_mut() = Rc::downgrade(&inner);
        host.bind_protocols()?;
        let venue = Rc::new(Self {
            inner,
            host,
            capabilities: ExchangeCapabilities {
                start: CapabilityBinding::implemented(|v, ()| v.start()),
                place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
                place_orders: PlaceOrdersBinding::implemented::<Self>(Exchange::place_orders),
                fetch_historical_fills: CapabilityBinding::implemented(|v, o| {
                    v.fetch_historical_fills(o)
                }),
                ..Default::default()
            },
        });
        let erased: Rc<dyn Exchange> = venue.clone();
        *venue.host.scope.sockets.venue.borrow_mut() = Some(Rc::downgrade(&erased));
        venue.base().set_borrowed_socket(
            safe_cex_exchange::socket_lifecycle::SocketSlot::Public,
            &venue.host.core(false)?,
        );
        venue.base().set_borrowed_socket(
            safe_cex_exchange::socket_lifecycle::SocketSlot::Private,
            &venue.host.core(true)?,
        );
        venue.inner.base.bind_socket_watchdog();
        Ok(venue)
    }
}
fn inherited<T: 'static>() -> ExchangeFuture<'static, T> {
    // bingx.exchange.ts has no overrides. Source base.ts:799-801, 989-991,
    // 1138-1140 each reject with this exact Error.
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}
fn ids(
    values: Vec<Option<safe_cex_types::retained_json::RetainedJsonInput>>,
) -> Result<Vec<String>, ExchangeError> {
    values
        .into_iter()
        .map(|id| match id {
            Some(safe_cex_types::retained_json::RetainedJsonInput::Text(units)) => {
                String::from_utf16(&units)
                    .map_err(|_| ExchangeError::new("BingX typed order id requires UTF8"))
            }
            Some(
                value @ (safe_cex_types::retained_json::RetainedJsonInput::Float(_)
                | safe_cex_types::retained_json::RetainedJsonInput::Lexeme(_)),
            ) => Ok(safe_cex_num::js_number_string(value.as_f64().unwrap())),
            _ => Err(ExchangeError::new(
                "BingX typed order result requires an order id",
            )),
        })
        .collect()
}
impl Exchange for BingxExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.inner.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        let preflight = self
            .base()
            .assert_caller_client_order_ids(std::slice::from_ref(order.options()));
        Box::pin(async move {
            preflight?;
            ids(self
                .inner
                .placement
                .place_order(&safe_cex_types::PreparedOrderOpts::from(order.options()))
                .await?)
        })
    }
    fn place_orders<'a>(&'a self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'a, Vec<String>> {
        let options: Vec<_> = orders
            .orders()
            .iter()
            .map(|o| safe_cex_types::PreparedOrderOpts::from(o.options()))
            .collect();
        Box::pin(async move { ids(self.inner.placement.place_orders(&options).await?) })
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(self.inner.start())
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        Box::pin(async move {
            let value =
                crate::account_reads::get_account(|request| self.inner.xhr.send(request)).await?;
            serde_json::from_value(value).map_err(|e| ExchangeError::new(e.to_string()))
        })
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        Box::pin(async move {
            let v = self.inner.reads.validate_account().await?;
            Ok(ValidateAccountResult {
                error: v.error.into(),
                balance: v.balance,
            })
        })
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(async move { self.inner.reads.fetch_positions(symbols.as_deref()).await })
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(self.inner.history.fetch_orders())
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        Box::pin(self.inner.reads.fetch_markets())
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        Box::pin(self.inner.reads.fetch_tickers())
    }
    fn fetch_leverage_for_symbol(
        &self,
        symbol: String,
    ) -> ExchangeFuture<'_, Option<FetchedLeverage>> {
        Box::pin(async move { self.inner.reads.fetch_leverage_for_symbol(&symbol).await })
    }
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        inherited()
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.inner.reads.set_leverage(&symbol, leverage, side).await })
    }
    fn change_position_mode(
        &self,
        hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(self.inner.reads.change_position_mode(hedged))
    }
    fn change_margin_type(&self, isolated: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move {
            self.inner
                .reads
                .change_margin_type(isolated, symbol.as_deref())
                .await
        })
    }
    fn update_order(&self, opts: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(async move { ids(self.inner.amendment.update_order(&opts).await?) })
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.inner.reads.cancel_orders(&orders).await })
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.inner.reads.cancel_symbol_orders(&symbol).await })
    }
    fn schedule_cancel(&self, _time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        inherited()
    }
    fn fetch_public_trades(
        &self,
        _symbol: String,
        _limit: Option<f64>,
    ) -> ExchangeFuture<'_, Vec<PublicTrade>> {
        inherited()
    }
    fn fetch_ohlcv(&self, opts: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        Box::pin(async move { self.inner.history.fetch_ohlcv(&opts).await })
    }
    fn fetch_historical_fills(
        &self,
        opts: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        Box::pin(async move { self.inner.history.fetch_historical_fills(&opts).await })
    }
    fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let cleanup = self.inner.public_websocket.subscriptions.listen_ohlcv(
            &opts,
            Rc::new(move |candle| {
                callback(candle);
                Ok(())
            }),
        )?;
        Ok(Box::new(move || cleanup()))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let cleanup = self
            .inner
            .public_websocket
            .subscriptions
            .listen_order_book(
                &symbol,
                Rc::new(move |book| {
                    callback(book.borrow().clone());
                    Ok(())
                }),
            )?;
        Ok(Box::new(move || cleanup()))
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.inner.dispose()?;
        self.host.dispose_timers();
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod retained_info_native {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/retained_info_native.rs"
    ));
}
