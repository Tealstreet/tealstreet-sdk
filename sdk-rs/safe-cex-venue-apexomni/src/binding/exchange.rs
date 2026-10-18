use super::hosts::{Host, HostLink};
use crate::exchange::ApexOmniExchange as Bodies;
use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities, PlaceOrderBinding},
    exchange::{BaseExchange, Exchange, ExchangeSettings, ValidatedOrder},
    js_runtime::{base_exchange::ExternalPublicDataFactory, client::JsHttpHost},
    runtime::{ExchangeError, ExchangeFuture},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::*;
use std::rc::Rc;

pub struct ApexDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Rc<dyn JsHttpHost>,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

/// The existing Apex bodies assembled on the shared JS runtime capabilities.
pub struct ApexOmniExchange {
    pub(super) inner: Rc<Bodies>,
    pub(super) host: Rc<Host>,
    capabilities: ExchangeCapabilities,
}
impl ApexOmniExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: ApexDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        if name != ExchangeName::Apexomni {
            return Err(ExchangeError::new("ApexOmniExchange requires apexomni"));
        }
        let remote = deps.http.hosted_signer();
        if (options.remote_signer.is_some() && remote.is_none()) || options.payload_signer.is_some()
        {
            return Err(ExchangeError::new(
                "ApexOmni hosted signing capability unavailable",
            ));
        }
        if options.external_public_data.is_some() && deps.external_public_data.is_none() {
            return Err(ExchangeError::new(
                "ApexOmni externalPublicData requires a provider",
            ));
        }
        let mut base = BaseExchange::new(
            name,
            options.clone(),
            store.clone(),
            deps.runtime.clone(),
            ExchangeSettings::default(),
        );
        base.signers.remote_signer = remote.map(|(signer, _)| signer);
        let host = Host::new(options, store, deps)?;
        // A weak imported host capability prevents the socket->parent->host cycle.
        let inner = Bodies::new(base, Rc::new(HostLink(Rc::downgrade(&host))))?;
        let venue = Rc::new(Self {
            inner,
            host,
            capabilities: ExchangeCapabilities {
                start: CapabilityBinding::implemented(|v, ()| v.start()),
                place_order: PlaceOrderBinding::implemented::<Self>(Exchange::place_order),
                fetch_historical_fills: CapabilityBinding::implemented(|v, o| {
                    v.fetch_historical_fills(o)
                }),
                ..Default::default()
            },
        });
        let erased: Rc<dyn Exchange> = venue.clone();
        *venue.host.scope.sockets.venue.borrow_mut() = Some(Rc::downgrade(&erased));
        venue.host.bind_protocols()?;
        for (private, slot) in [
            (
                false,
                safe_cex_exchange::socket_lifecycle::SocketSlot::Public,
            ),
            (
                true,
                safe_cex_exchange::socket_lifecycle::SocketSlot::Private,
            ),
        ] {
            venue
                .base()
                .set_borrowed_socket(slot, &venue.host.core(private)?);
        }
        venue.inner.base.bind_socket_watchdog();
        Ok(venue)
    }
}
fn inherited<T: 'static>() -> ExchangeFuture<'static, T> {
    // Apex has no overrides for these six methods. base.ts:759,799,832,836,
    // 989,1138 rejects with this exact Error, without transport calls.
    Box::pin(async { Err(ExchangeError::new("Not implemented")) })
}
impl Exchange for ApexOmniExchange {
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
            self.inner.place_order(order.options()).await
        })
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(self.inner.start())
    }
    fn fetch_and_set_private_snapshot(&self, _tries: u32) -> ExchangeFuture<'_, ()> {
        Box::pin(self.inner.fetch_and_set_private_snapshot())
    }
    fn get_account(&self) -> ExchangeFuture<'_, ExchangeAccount> {
        Box::pin(async move {
            let value = self.inner.get_account().await?;
            safe_cex_exchange::js_runtime::value::to_typed(
                &safe_cex_exchange::js_runtime::value::from_retained_input(value),
            )
            .map_err(super::hosts::error)
        })
    }
    fn validate_account(&self) -> ExchangeFuture<'_, ValidateAccountResult> {
        Box::pin(async move {
            Ok(ValidateAccountResult {
                error: self.inner.validate_account().await?,
                balance: None,
            })
        })
    }
    fn fetch_positions(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(self.inner.fetch_positions())
    }
    fn fetch_orders(&self, _symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(self.inner.fetch_orders())
    }
    fn fetch_markets(&self) -> ExchangeFuture<'_, Vec<Market>> {
        Box::pin(self.inner.fetch_markets())
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'_, Vec<Ticker>> {
        inherited()
    }
    fn fetch_symbol_max_leverage(&self, _symbol: String) -> ExchangeFuture<'_, f64> {
        inherited()
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        _side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.inner.set_leverage(&symbol, leverage).await })
    }
    fn change_position_mode(
        &self,
        _hedged: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        inherited()
    }
    fn change_margin_type(
        &self,
        _isolated: bool,
        _symbol: Option<String>,
    ) -> ExchangeFuture<'_, ()> {
        inherited()
    }
    fn update_order(&self, opts: UpdateOrderOpts) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(async move { self.inner.update_order(&opts).await })
    }
    fn cancel_orders(&self, orders: Vec<Order>) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.inner.cancel_orders(&orders).await })
    }
    fn cancel_symbol_orders(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        Box::pin(async move { self.inner.cancel_symbol_orders(&symbol, true).await })
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
        Box::pin(async move { self.inner.fetch_ohlcv(&opts).await })
    }
    fn fetch_historical_fills(
        &self,
        opts: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        Box::pin(async move {
            self.inner
                .fetch_historical_fills(&opts, &safe_cex_num::JsJsonValue::Undefined)
                .await
        })
    }
    fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        let cleanup = self.inner.listen_ohlcv(
            opts,
            Rc::new(move |c| {
                callback(c);
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
        let cleanup = self.inner.listen_order_book(
            &symbol,
            Rc::new(move |b| {
                callback(b);
                Ok(())
            }),
        )?;
        Ok(Box::new(move || cleanup()))
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.inner.dispose()?;
        self.host.dispose_tasks();
        Ok(())
    }
}
