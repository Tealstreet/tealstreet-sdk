use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput,
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_types::{ExchangeName, ExecutionsOptions};
use safe_cex_venue_bitget::history_reads::fetch_historical_fills;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
struct Runtime;
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.
    }
    fn now_ms(&self) -> f64 {
        123000.
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected spawn")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "reporter-fixture".into()
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
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected fixture pending"),
    }
}
#[test]
fn historical_fills_concrete_reporter_preserves_original_listener_error() {
    let base = BaseExchange::new(
        ExchangeName::Bitget,
        serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
        Rc::new(DefaultStore::new()),
        Rc::new(Runtime),
        ExchangeSettings::default(),
    );
    let later = Rc::new(Cell::new(0));
    let first = Rc::new(Cell::new(0));
    let mut marker = ExchangeError::new("listener marker");
    marker.name = "ReporterMarkerError".into();
    marker.is_axios_error = true;
    marker.request = Some(json!({"requestMarker":73}));
    marker.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(79)));
    marker = marker.with_retained_response(json!({"status":429,"data":{"code":71}}).into());
    marker.context = Some(json!({"marker":"native-listener"}));
    let expected_metadata = marker.metadata.clone();
    let pointer = std::ptr::from_ref(marker.metadata.as_ref());
    let error = Rc::new(RefCell::new(Some(marker)));
    let first_count = first.clone();
    let owned = error.clone();
    base.on(
        "error",
        Rc::new(move |event, payload| {
            let payload = safe_cex_exchange::js_runtime::value::to_json(payload).unwrap();
            assert_eq!(event, "error", "concrete reporter event");
            assert_eq!(
                payload, "Exchange Error: page failure",
                "concrete reporter message"
            );
            first_count.set(first_count.get() + 1);
            Err(owned
                .borrow_mut()
                .take()
                .expect("one original listener error"))
        }),
    );
    let later_count = later.clone();
    base.on(
        "error",
        Rc::new(move |_, _| {
            later_count.set(later_count.get() + 1);
            Ok(())
        }),
    );
    let pages = Cell::new(0);
    let result = run(fetch_historical_fills(
        false,
        &ExecutionsOptions {
            symbol: None,
            order_id: None,
            from: None,
            to: None,
            limit: None,
        },
        || std::future::ready(Ok(false)),
        |_| -> std::future::Ready<Result<Vec<safe_cex_types::Execution>, ExchangeError>> {
            unreachable!("classic routing")
        },
        |_| Ok(None),
        || Ok("USDT-FUTURES".into()),
        |_| {
            pages.set(pages.get() + 1);
            std::future::ready(Err(ExchangeError::new("page failure")))
        },
        (
            |e: &ExchangeError| Some(json!({"message":e.message})),
            |message| {
                let input = match message {
                    Some(Value::String(v)) => ExchangeErrorInput::Message(v),
                    None => ExchangeErrorInput::Missing,
                    _ => panic!("this witness covers source string messages only"),
                };
                base.emit_and_log_error(input, None, None)
            },
        ),
    ));
    let failure =
        result.expect_err("concrete reporter listener failure must escape historical fills catch");
    assert_eq!(
        std::ptr::from_ref(failure.metadata.as_ref()),
        pointer,
        "concrete reporter original metadata allocation"
    );
    assert_eq!(
        failure.name, "ReporterMarkerError",
        "concrete reporter original error name"
    );
    assert_eq!(
        failure.metadata.as_ref(),
        expected_metadata.as_ref(),
        "concrete reporter full native error metadata"
    );
    assert_eq!(
        failure.message, "listener marker",
        "concrete reporter original message"
    );
    assert_eq!(
        failure.response_json_view(),
        Some(json!({"status":429,"data":{"code":71}})),
        "concrete reporter full response"
    );
    assert_eq!(
        failure.context,
        Some(json!({"marker":"native-listener"})),
        "concrete reporter full context"
    );
    assert_eq!(first.get(), 1, "concrete reporter first listener count");
    assert_eq!(later.get(), 0, "concrete reporter later listener skipped");
    assert_eq!(
        pages.get(),
        1,
        "concrete reporter independent request count"
    );
}
