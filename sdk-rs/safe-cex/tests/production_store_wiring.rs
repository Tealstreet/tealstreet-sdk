//! Composition gate: exchange core must accept the actual production Store.
use safe_cex::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_types::ExchangeName;
use std::{future::Future, pin::Pin, rc::Rc};
struct QuietRuntime;
impl ExchangeRuntime for QuietRuntime {
    fn now_ms(&self) -> f64 {
        0.0
    }
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("construction must not spawn")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "wiring-test".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn execute_worker_request(
        &self,
        _: serde_json::Value,
    ) -> ExchangeFuture<'static, serde_json::Value> {
        panic!("construction must not contact a worker")
    }
}
#[test]
fn real_default_store_satisfies_the_exchange_core_constructor() {
    let production = Rc::new(DefaultStore::new());
    let core = BaseExchange::new(
        ExchangeName::Binance,
        serde_json::from_value(serde_json::json!({"key":"", "secret":""})).unwrap(),
        production.clone(),
        Rc::new(QuietRuntime),
        ExchangeSettings::default(),
    );
    assert!(core.store.markets().unwrap().is_empty());
    core.store
        .set_setting("isHedged", true)
        .expect("fixture setting notification must succeed");
    assert!(production.options().unwrap().is_hedged);
}
