//! Concrete bindings for the already-mapped account, REST, market, and quote reads.
use crate::{lifecycle::BreakoutpropExchange, runtime::RuntimeReads};
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput,
    external::ExternalPublicData,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_transport::provider::RestRequest;
use safe_cex_types::{Candle, ExchangeOptionsExternalPublicData, Market, OHLCVOptions, Ticker};
use serde_json::Value;
use std::rc::Rc;

pub trait NativeRuntimeHost {
    fn create_external(
        &self,
        config: &ExchangeOptionsExternalPublicData,
    ) -> Rc<dyn ExternalPublicData>;
    fn local_to_utc(&self, local: f64) -> f64;
    fn uuid(&self) -> String;
    fn spawn_order(&self, future: ExchangeFuture<'static, ()>);
    fn schedule_loop(
        &self,
        next: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
        delay: Option<f64>,
    ) -> Result<(), ExchangeError>;
}
#[derive(Clone)]
pub struct NativeReads {
    pub exchange: Rc<BreakoutpropExchange>,
    pub host: Rc<dyn NativeRuntimeHost>,
}
impl NativeReads {
    fn report(&self, value: Value) -> Result<(), ExchangeError> {
        let text = safe_cex_types::js_coercion::js_string(Some(&value))
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        self.exchange
            .base
            .emit_and_log_error(ExchangeErrorInput::Message(text), None, None)
    }
    fn request(&self, request: RestRequest) -> ExchangeFuture<'static, JsNumberInput> {
        match self.exchange.xhr.borrow().clone() {
            Some(client) => client.request(request),
            None => {
                Box::pin(async { Err(ExchangeError::new("DXtrade API construction incomplete")) })
            }
        }
    }
    pub(crate) fn fetch_ohlcv(
        &self,
        options: OHLCVOptions,
    ) -> ExchangeFuture<'static, Vec<Candle>> {
        let session = self.exchange.get_session_token();
        let this = self.clone();
        Box::pin(async move {
            let to_iso = |milliseconds| {
                safe_cex_num::js_date_to_iso_string(milliseconds).map_err(|error| {
                    let mut value = ExchangeError::new(error.message);
                    value.name = error.name.into();
                    value
                })
            };
            crate::candles::fetch_ohlcv(
                options,
                || session,
                || this.get_account_code(),
                |request| this.request(request),
                &to_iso,
                &|value| this.host.local_to_utc(value),
                &|value| this.report(value),
            )
            .await
        })
    }
    fn instruments(&self, details: bool) -> ExchangeFuture<'static, Option<JsNumberInput>> {
        let session = self.exchange.get_session_token();
        let this = self.clone();
        Box::pin(async move {
            let log = |v| this.report(v);
            if details {
                crate::raw_reads::fetch_instrument_details_raw(
                    || session,
                    || this.get_account_code(),
                    |r| this.request(r),
                    &log,
                )
                .await
            } else {
                crate::raw_reads::fetch_instruments_raw(
                    || session,
                    || this.get_account_code(),
                    |r| this.request(r),
                    &log,
                )
                .await
            }
        })
    }
}
impl RuntimeReads for NativeReads {
    fn init_external_public_data(&self) -> ExchangeFuture<'static, bool> {
        let this = self.clone();
        let mut future: ExchangeFuture<'static, bool> = Box::pin(async move {
            this.exchange
                .base
                .init_external_public_data(|c| this.host.create_external(c))
                .await
        });
        // Preserve the source initializer's synchronous factory/connect prefix.
        match future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            std::task::Poll::Ready(result) => Box::pin(async move { result }),
            std::task::Poll::Pending => future,
        }
    }
    fn get_account_code(&self) -> ExchangeFuture<'static, String> {
        let cached = self
            .exchange
            .account_code
            .borrow()
            .clone()
            .filter(|s| !s.is_empty());
        if let Some(cached) = cached {
            return Box::pin(async move { Ok(cached) });
        }
        let session = self.exchange.get_session_token();
        let opts = self.exchange.base.options();
        let this = self.clone();
        Box::pin(async move {
            let cache = std::cell::RefCell::new(None);
            let result = crate::account_auth::get_account_code(
                &cache,
                opts.key,
                opts.subaccount,
                || session,
                |r| this.request(r),
                &|v| this.report(v),
            )
            .await;
            if let Ok(account) = &result {
                *this.exchange.account_code.borrow_mut() = Some(account.clone());
            }
            result
        })
    }
    fn fetch_markets(&self) -> ExchangeFuture<'static, Vec<Market>> {
        let left = self.instruments(false);
        let right = self.instruments(true);
        let combined = crate::market_reads::fetch_combined_instruments(|| left, || right);
        let this = self.clone();
        Box::pin(async move {
            crate::quotes::fetch_markets(|| combined, this.exchange.base.store.as_ref(), &|v| {
                this.report(v)
            })
            .await
        })
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'static, Option<Vec<Ticker>>> {
        let markets = match self.exchange.base.store.markets() {
            Ok(markets) => markets,
            Err(error) => return Box::pin(async move { Err(error.into()) }),
        };
        let symbols = markets.iter().map(|m| m.symbol.clone()).collect();
        let session = self.exchange.get_session_token();
        let this = self.clone();
        Box::pin(async move {
            crate::quotes::fetch_tickers_for_symbols(
                symbols,
                || session,
                || this.get_account_code(),
                |r| this.request(r),
                this.exchange.base.store.as_ref(),
                &|v| this.report(v),
            )
            .await
        })
    }
    fn fetch_portfolio_raw(&self) -> ExchangeFuture<'static, Option<JsNumberInput>> {
        let session = self.exchange.get_session_token();
        let this = self.clone();
        Box::pin(async move {
            let input = crate::raw_reads::fetch_portfolio_raw(
                || session,
                || this.get_account_code(),
                |r| this.request(r),
                &|v| this.report(v),
            )
            .await?;
            // Source start/tick check truthiness, not only non-nullness.
            match input {
                Some(input)
                    if !input
                        .clone()
                        .decode::<safe_cex_types::retained_json::RetainedJsonInput>()
                        .map_err(|error| ExchangeError::new(error.to_string()))?
                        .is_truthy() =>
                {
                    Ok(None)
                }
                value => Ok(value),
            }
        })
    }
    fn local_to_utc(&self, local: f64) -> f64 {
        self.host.local_to_utc(local)
    }
    fn uuid(&self) -> String {
        self.host.uuid()
    }
    fn spawn_order(&self, future: ExchangeFuture<'static, ()>) {
        self.host.spawn_order(future)
    }
    fn schedule_loop(
        &self,
        next: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
        delay: Option<f64>,
    ) -> Result<(), ExchangeError> {
        self.host.schedule_loop(next, delay)
    }
}
