// Finite hosts for production accepted-set observations; no live I/O.
use crate::{lifecycle::*, polling::*, runtime::RuntimeReads};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{exchange::BaseExchange, runtime::*};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_store::Store;
use safe_cex_transport::provider::RestRequest;
use safe_cex_types::{ExchangeName, ExchangeOptions, Market, Ticker};
use std::{cell::RefCell, rc::Rc};

mod clock {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-exchange/tests/support/runtime.rs"
    ));
}

pub struct Ancestor;
impl PollingAncestor for Ancestor {
    fn now_ms(&self) -> f64 {
        120_000.0
    }
    fn cleanup_message_handler_stats(&self, _: &str) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        Ok(())
    }
}

#[derive(Default)]
pub struct Host {
    pub requests: Rc<RefCell<Vec<RestRequest>>>,
}
struct Client(Rc<RefCell<Vec<RestRequest>>>);
impl DxClient for Client {
    fn rate_limit(&self, _: f64) -> Result<(), ExchangeError> {
        panic!("finite source-helper fixture must not enter production client limiting")
    }
    fn request(&self, request: RestRequest) -> ExchangeFuture<'static, JsNumberInput> {
        self.0.borrow_mut().push(request);
        Box::pin(async {
            Ok(serde_json::from_value(serde_json::json!({"accepted":true})).unwrap())
        })
    }
}
impl BreakoutHost for Host {
    fn runtime_host(&self) -> Rc<dyn crate::native_reads::NativeRuntimeHost> {
        panic!("finite source-helper fixture must not enter production runtime reads")
    }
    fn construct_base(
        &self,
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
    ) -> Result<BaseExchange, ExchangeError> {
        Ok(BaseExchange::new(
            name,
            options,
            store,
            Rc::new(clock::Runtime::default()),
            Default::default(),
        ))
    }
    fn create_api(&self, _: Rc<BreakoutpropExchange>) -> Result<Rc<dyn DxClient>, ExchangeError> {
        Ok(Rc::new(Client(self.requests.clone())))
    }
    fn rate_limit(
        &self,
        client: Rc<dyn DxClient>,
        _: f64,
    ) -> Result<Rc<dyn DxClient>, ExchangeError> {
        Ok(client)
    }
    fn construct_public_ancestor(
        &self,
        _: Rc<BreakoutpropExchange>,
        _: HandlerTable,
    ) -> Result<Rc<dyn PollingAncestor>, ExchangeError> {
        Ok(Rc::new(Ancestor))
    }
    fn spawn_login(&self, _: ExchangeFuture<'static, ()>) {
        panic!("accepted fixture must use cached session")
    }
    fn report_abandoned_login(&self, _: &ExchangeError) {
        panic!("unexpected abandoned login")
    }
}
pub fn exchange() -> (Rc<BreakoutpropExchange>, Rc<Host>) {
    let host = Rc::new(Host::default());
    let exchange = BreakoutpropExchange::new(
        host.clone(),
        ExchangeName::Breakoutprop,
        serde_json::from_value(serde_json::json!({"key":"fixture-user","secret":""})).unwrap(),
        Rc::new(DefaultStore::new()),
    )
    .unwrap();
    *exchange.session_token.borrow_mut() = Some("fixture-token".into());
    exchange.session_token_expires_at.set(1_000_000.0);
    (exchange, host)
}

pub struct Reads;
impl RuntimeReads for Reads {
    fn init_external_public_data(&self) -> ExchangeFuture<'static, bool> {
        Box::pin(async { Ok(false) })
    }
    fn get_account_code(&self) -> ExchangeFuture<'static, String> {
        Box::pin(async { Ok("fixture-account".into()) })
    }
    fn fetch_markets(&self) -> ExchangeFuture<'static, Vec<Market>> {
        Box::pin(async { panic!("unused markets") })
    }
    fn fetch_tickers(&self) -> ExchangeFuture<'static, Option<Vec<Ticker>>> {
        Box::pin(async { panic!("unused tickers") })
    }
    fn fetch_portfolio_raw(&self) -> ExchangeFuture<'static, Option<JsNumberInput>> {
        Box::pin(async { panic!("unused portfolio") })
    }
    fn local_to_utc(&self, value: f64) -> f64 {
        value
    }
    fn uuid(&self) -> String {
        "fixture-id".into()
    }
    fn spawn_order(&self, _: ExchangeFuture<'static, ()>) {
        panic!("unused order task")
    }
    fn schedule_loop(
        &self,
        _: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
        _: Option<f64>,
    ) -> Result<(), ExchangeError> {
        panic!("unused loop")
    }
}
pub fn ticker(symbol: String, bid: f64, ask: f64) -> Ticker {
    Ticker {
        id: symbol.clone(),
        symbol,
        bid,
        ask,
        last: bid,
        mark: bid,
        index: bid,
        mid_price: None,
        percentage: 0.0,
        open_interest: 0.0,
        funding_rate: 0.0,
        funding_time: 0.0,
        volume: 0.0,
        quote_volume: 0.0,
        timestamp: None,
        info: None,
    }
}
