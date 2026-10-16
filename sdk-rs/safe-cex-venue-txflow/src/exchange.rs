use crate::{api::create_api, types::TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE};
use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities},
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    http::{HttpClient, HttpClientOptions},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use safe_cex_store::Store;
use safe_cex_transport::TransportRegistry;
use safe_cex_types::{ExchangeName, ExchangeOptions, PlaceOrderOpts};
use std::rc::Rc;

pub struct TxFlowExchange {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    pub xhr: HttpClient,
}
impl TxFlowExchange {
    pub fn new(
        exchange_name: ExchangeName,
        opts: ExchangeOptions,
        store: Rc<dyn Store>,
        runtime: Rc<dyn ExchangeRuntime>,
        mut http: HttpClientOptions,
        registry: &TransportRegistry,
    ) -> Result<Self, ExchangeError> {
        let base = BaseExchange::new(
            exchange_name.clone(),
            opts.clone(),
            store,
            runtime,
            ExchangeSettings {
                tick_interval: Some(2000.0),
                ..Default::default()
            },
        );
        http.exchange = exchange_name;
        http.options = opts;
        let xhr = create_api(http, registry).map_err(|e| ExchangeError::new(e.to_string()))?;
        Ok(Self {
            base,
            capabilities: ExchangeCapabilities {
                start: CapabilityBinding::implemented(|v, ()| v.start()),
                ..Default::default()
            },
            xhr,
        })
    }
    /// The source throws synchronously; the emit happens at call time and the
    /// already-failed future carries the error.
    pub fn start(&self) -> ExchangeFuture<'_, ()> {
        let result = self
            .base
            .reject_preflight(TXFLOW_PHASE_0_UNIMPLEMENTED_MESSAGE.into());
        Box::pin(async move { result })
    }
}
impl Exchange for TxFlowExchange {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn start(&self) -> ExchangeFuture<'_, ()> {
        TxFlowExchange::start(self)
    }
}
