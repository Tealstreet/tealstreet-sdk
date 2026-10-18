//! Composition gate: exchange core must accept the actual production Store.
use safe_cex::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::store::DefaultStore;
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
    assert!(core.store.markets().is_empty());
    core.store
        .set_setting("isHedged", true)
        .expect("fixture setting notification must succeed");
    assert!(production.options().is_hedged);
}

#[test]
fn factory_constructs_real_exchange_while_clock_source_is_pending() {
    use safe_cex::factory::{ExchangeConstructor, ExchangeFactory, ExchangeLoader};
    use safe_cex_store::virtual_clock::VirtualClock;
    use safe_cex_transport::TransportRegistry;
    use std::{
        future::poll_fn,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        task::{Context, Poll, Waker},
    };

    type Scheduled = Vec<(u64, Pin<Box<dyn Future<Output = ()> + Send>>)>;
    let released = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let timers = Arc::new(Mutex::new(Scheduled::new()));
    let trace = Arc::new(Mutex::new(Vec::new()));
    let mut clock = VirtualClock::default();
    clock.runtime = "browser".into();
    clock.now = Arc::new(|| 1000.0);
    clock.set_server_time_source(Some(Arc::new({
        let released = released.clone();
        let calls = calls.clone();
        let trace = trace.clone();
        move || {
            calls.fetch_add(1, Ordering::SeqCst);
            trace.lock().unwrap().push("source");
            let released = released.clone();
            Box::pin(poll_fn(move |_| {
                if released.load(Ordering::SeqCst) {
                    Poll::Ready(Ok(Some(5000.0)))
                } else {
                    Poll::Pending
                }
            }))
        }
    })));
    clock.schedule = Arc::new({
        let timers = timers.clone();
        move |delay, task| {
            let mut timers = timers.lock().unwrap();
            timers.push((delay, task));
            timers.len() as u64
        }
    });
    let constructor: ExchangeConstructor<BaseExchange> = Rc::new({
        let trace = trace.clone();
        move |name, options, store| {
            trace.lock().unwrap().push("constructor");
            Ok(BaseExchange::new(
                name,
                options,
                store,
                Rc::new(QuietRuntime),
                ExchangeSettings::default(),
            ))
        }
    });
    let loader: ExchangeLoader<BaseExchange> = Rc::new({
        let trace = trace.clone();
        move || {
            trace.lock().unwrap().push("loader");
            let constructor = constructor.clone();
            Box::pin(async move { Ok(constructor) })
        }
    });
    let factory = ExchangeFactory::new(
        [("binance".into(), loader)].into_iter().collect(),
        clock,
        TransportRegistry::default(),
    );
    let store = Rc::new(DefaultStore::new());
    let mut context = Context::from_waker(Waker::noop());
    let options = || serde_json::from_value(serde_json::json!({"key":"", "secret":""})).unwrap();
    let mut create = Box::pin(factory.create_exchange("binance", options(), Some(store.clone())));
    let Poll::Ready(Ok(exchange)) = create.as_mut().poll(&mut context) else {
        panic!("factory must construct the real exchange before clock synchronization resolves");
    };
    assert_eq!(*trace.lock().unwrap(), ["source", "loader", "constructor"]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    exchange.store.set_setting("isHedged", true).unwrap();
    assert!(
        store.options().is_hedged,
        "factory must retain the production Store"
    );
    assert_eq!(factory.clock.get_current_time_ms(), 1000.0);

    let mut second = Box::pin(factory.create_exchange("binance", options(), Some(store)));
    assert!(matches!(
        second.as_mut().poll(&mut context),
        Poll::Ready(Ok(_))
    ));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "pending synchronization must not restart"
    );
    let mut unknown = Box::pin(factory.create_exchange("unknown", options(), None));
    assert!(
        matches!(unknown.as_mut().poll(&mut context), Poll::Ready(Err(error)) if error == "Unknown exchange: unknown")
    );

    released.store(true, Ordering::SeqCst);
    let mut initial = timers.lock().unwrap().remove(0).1;
    assert!(
        initial.as_mut().poll(&mut context).is_ready(),
        "clock synchronization must continue after construction"
    );
    assert_eq!(factory.clock.get_current_time_ms(), 5000.0);
    assert_eq!(
        timers
            .lock()
            .unwrap()
            .iter()
            .map(|(delay, _)| *delay)
            .collect::<Vec<_>>(),
        [300_000]
    );

    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/factory_clock_source.cjs"
        ))
        .output()
        .expect("run maintained TS Clock and factory declarations");
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(source["trace"], serde_json::json!(*trace.lock().unwrap()));
    assert_eq!(source["timers"], serde_json::json!([300_000]));
}
