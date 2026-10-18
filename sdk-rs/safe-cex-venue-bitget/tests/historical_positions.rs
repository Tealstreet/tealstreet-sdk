use safe_cex_exchange::{
    capabilities::{CapabilityBinding, ExchangeCapabilities},
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    journal::{HistoricalPositionOptions, JournalFolder},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use safe_cex_venue_bitget::history_reads::fetch_historical_positions;
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
        panic!("unexpected monotonic clock")
    }
    fn now_ms(&self) -> f64 {
        123000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected task")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {
        panic!("unexpected console")
    }
    fn unique_id(&self) -> String {
        "fixed".into()
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
        panic!("unexpected report")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn class_name(&self) -> &'static str {
        "BitgetExchange"
    }
}
fn venue() -> Venue {
    Venue {
        base: BaseExchange::new(
            ExchangeName::Bitget,
            serde_json::from_value(json!({"key":"fake","secret":"fake"})).unwrap(),
            Rc::new(DefaultStore::new()),
            Rc::new(Runtime),
            ExchangeSettings::default(),
        ),
        capabilities: ExchangeCapabilities {
            fetch_historical_fills: CapabilityBinding::implemented(|_, _| {
                Box::pin(async { Ok(vec![]) })
            }),
            ..Default::default()
        },
    }
}
fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("unexpected pending"),
    }
}
fn bits(n: f64) -> Value {
    json!({"bits":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
}
fn input(v: &Value) -> Option<f64> {
    v.get("bits")
        .map(|b| f64::from_bits(u64::from_str_radix(b.as_str().unwrap(), 16).unwrap()))
}
struct Folder {
    expected_base: *const BaseExchange,
    seen: RefCell<Vec<Value>>,
    records: RefCell<Option<Vec<Value>>>,
    failure: RefCell<Option<ExchangeError>>,
    dropped: Rc<Cell<bool>>,
    pending: bool,
}
impl JournalFolder for Folder {
    fn build_folded_position_records<'a>(
        &'a self,
        venue: &'a dyn Exchange,
        account: &'a str,
        from: f64,
        to: f64,
        symbol_scoped: bool,
        _: Box<dyn Fn(&str) -> String + 'a>,
    ) -> ExchangeFuture<'a, Vec<Value>> {
        assert!(
            std::ptr::eq(venue.base(), self.expected_base),
            "historical positions original venue receiver"
        );
        assert_eq!(
            venue.class_name(),
            "BitgetExchange",
            "journal receives Bitget venue"
        );
        self.seen.borrow_mut().push(
            json!({"account":account,"from":bits(from),"to":bits(to),"symbolScoped":symbol_scoped}),
        );
        struct Guard(Rc<Cell<bool>>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }
        let guard = Guard(self.dropped.clone());
        Box::pin(async move {
            let _guard = guard;
            if self.pending {
                std::future::pending::<()>().await;
            }
            if let Some(error) = self.failure.borrow_mut().take() {
                return Err(error);
            }
            Ok(self.records.borrow_mut().take().unwrap())
        })
    }
}
fn folder(records: Vec<Value>, venue: &Venue) -> Folder {
    Folder {
        expected_base: std::ptr::from_ref(&venue.base),
        seen: RefCell::new(vec![]),
        records: RefCell::new(Some(records)),
        failure: RefCell::new(None),
        dropped: Rc::new(Cell::new(false)),
        pending: false,
    }
}
#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_historical_position_delegation() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/historical-positions-capture.cjs"
        ))
        .output()
        .expect("historical positions requires Node");
    assert!(
        output.status.success(),
        "historical positions source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        capture["cases"].as_array().unwrap().len(),
        64,
        "historical positions corpus inventory"
    );
    for case in capture["cases"].as_array().unwrap() {
        let venue = venue();
        let records = case["result"].as_array().unwrap().clone();
        let mut producer_snapshot = records.clone();
        let ptr = records.as_ptr();
        let folder = folder(records, &venue);
        let options = HistoricalPositionOptions {
            account: case["account"].as_str().unwrap().into(),
            from: input(&case["from"]),
            to: input(&case["to"]),
        };
        let mut result = run(fetch_historical_positions(&venue, &folder, options)).unwrap();
        assert_eq!(
            folder.seen.borrow()[0],
            case["args"],
            "historical positions native arguments {}",
            case["name"]
        );
        assert_eq!(
            result,
            case["result"].as_array().unwrap().clone(),
            "historical positions records"
        );
        assert_eq!(
            result.as_ptr(),
            ptr,
            "historical positions moves original folder records"
        );
        producer_snapshot[0]["id"] = json!("producer-change");
        assert_eq!(
            result[0]["id"], "record-A",
            "historical positions owned snapshot ignores producer edits"
        );
        result.push(json!({"id":"consumer-added"}));
        assert_eq!(
            producer_snapshot.len(),
            2,
            "historical positions owned producer ignores consumer append"
        );
        assert_eq!(case["sameVenue"], true);
        assert_eq!(case["sameRecords"], true);
    }
    assert_eq!(
        capture["forwarded"],
        json!({"sameOptions":true,"symbolScoped":false}),
        "historical positions source option forwarding"
    );
    assert_eq!(
        capture["sameError"], true,
        "historical positions source original error"
    );
    assert_eq!(
        capture["aliases"],
        json!({"same":true,"callerSees":"mutated","producerSees":2}),
        "historical positions source live alias"
    );
    assert_eq!(
        capture["openOutput"],
        json!({"same":true,"callable":"function","ownsMissing":true,"serialized":"[{}]"}),
        "historical positions pre-serialization open output witness"
    );
    let input = capture["utf16"]["input"].as_str().unwrap();
    assert!(
        serde_json::from_str::<String>(input).is_err(),
        "historical positions chosen UTF8 boundary"
    );
    assert_eq!(
        capture["utf16"]["input"], capture["utf16"]["output"],
        "historical positions source surrogate account"
    );
}
#[test]
fn historical_positions_preserves_failure_and_cancellation() {
    let venue = venue();
    let options = || HistoricalPositionOptions {
        account: "account".into(),
        from: Some(-0.0),
        to: Some(19.0),
    };
    let mut folder = folder(vec![], &venue);
    let mut failure = ExchangeError::new("original folder failure");
    failure = failure.with_retained_response(json!({"code":17}).into());
    failure.store_subscriber_ids = vec![3, 1];
    let ptr = std::ptr::from_ref(failure.metadata.as_ref());
    let expected = failure.clone();
    *folder.failure.borrow_mut() = Some(failure);
    let error = run(fetch_historical_positions(&venue, &folder, options())).unwrap_err();
    assert_eq!(
        error, expected,
        "historical positions original structured failure"
    );
    assert_eq!(
        std::ptr::from_ref(error.metadata.as_ref()),
        ptr,
        "historical positions original error allocation"
    );
    folder.pending = true;
    folder.dropped.set(false);
    let mut future = Box::pin(fetch_historical_positions(&venue, &folder, options()));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(future);
    assert!(
        folder.dropped.get(),
        "historical positions drop owns pending journal future"
    );
}
