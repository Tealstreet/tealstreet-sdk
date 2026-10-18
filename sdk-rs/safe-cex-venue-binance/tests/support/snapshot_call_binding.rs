//! Native SnapshotCall producer and sink; no replacement SnapshotBindings.
use super::*;
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use std::sync::{Arc, Mutex};
struct Response {
    fail: bool,
    requests: Arc<Mutex<Vec<String>>>,
}
impl RestAdapter for Response {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let regular = request
            .url
            .split('?')
            .next()
            .unwrap()
            .ends_with("/openOrders");
        self.requests.lock().unwrap().push(request.url);
        let data = if !regular {
            Input::Array(vec![])
        } else if self.fail {
            Input::from_value(json!({"code":-123,"msg":"snapshot-call refusal"})).unwrap()
        } else {
            Input::Object(vec![
                ("negativeZero".encode_utf16().collect(), Input::Float(-0.0)),
                ("nan".encode_utf16().collect(), Input::Float(f64::NAN)),
                ("ownUndefined".encode_utf16().collect(), Input::Undefined),
            ])
        };
        let status = if regular && self.fail { 400 } else { 200 };
        Box::pin(async move {
            Ok(RestResponse {
                status,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Untransformed(data),
            })
        })
    }
}
struct Case {
    owner: Rc<ExchangeOwner>,
    runtime: Rc<Runtime>,
    logs: Rc<RefCell<Vec<Vec<EventValue>>>>,
}
fn actual_case(fail: bool) -> Case {
    let runtime = Rc::new(Runtime::default());
    let requests = Arc::new(Mutex::new(vec![]));
    let deps = safe_cex_venue_binance::native_host::native_dependencies(
        runtime.clone(),
        Arc::new(Response {
            fail,
            requests: requests.clone(),
        }),
        Rc::new(|_, _| panic!("unexpected external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let owner = ExchangeOwner::new(
        Venue::Futures,
        safe_cex_types::ExchangeName::Binance,
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        deps,
    )
    .unwrap();
    let logs = Rc::new(RefCell::new(vec![]));
    let seen = logs.clone();
    owner.base.on_args(
        "log",
        Rc::new(move |_, args| {
            seen.borrow_mut().push(args.to_vec());
            Ok(())
        }),
    );
    let rows = run(&runtime, owner.invoke(Method::Orders, vec![])).unwrap();
    assert!(matches!(rows, EventValue::Array(ref rows) if rows.borrow().is_empty()));
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        2,
        "real regular and conditional native requests"
    );
    assert!(
        requests
            .iter()
            .all(|url| url.contains("signature=") && url.contains("timestamp="))
    );
    Case {
        owner,
        runtime,
        logs,
    }
}
#[test]
fn snapshot_call_keeps_source_log_error_severity() {
    let case = actual_case(false);
    assert_eq!(case.logs.borrow().len(), 1);
    assert_eq!(
        case.logs.borrow()[0][1].text().unwrap(),
        "error",
        "source explicit severity survives captured SnapshotCall"
    );
    case.owner.dispose().unwrap();
}
#[test]
fn snapshot_call_keeps_variadic_console_native_values() {
    let case = actual_case(false);
    let frames = case.runtime.console_frames.borrow();
    let (_, args) = frames
        .iter()
        .find(|(level, args)| {
            level == "error"
                && args.first().is_some_and(|v| {
                    v.text().unwrap_or_default()
                        == "[Binance] fetchOrders received non-array response:"
                })
        })
        .expect("actual source console call");
    assert_eq!(
        args.len(),
        2,
        "source console receives message and original data"
    );
    assert_eq!(
        args[1]
            .read("negativeZero")
            .unwrap()
            .number()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(args[1].read("nan").unwrap().number().unwrap().is_nan());
    assert!(args[1].has_own("ownUndefined"));
    assert!(matches!(
        args[1].read("ownUndefined").unwrap(),
        EventValue::Undefined
    ));
    drop(frames);
    case.owner.dispose().unwrap();
}
#[test]
fn snapshot_call_keeps_native_error_console_argument() {
    let case = actual_case(true);
    let frames = case.runtime.console_frames.borrow();
    let (_, args) = frames
        .iter()
        .find(|(level, args)| {
            level == "error"
                && args
                    .first()
                    .is_some_and(|v| v.text().unwrap_or_default() == "[Binance] fetchOrders error:")
        })
        .expect("actual source caught-error console call");
    assert_eq!(
        args.len(),
        2,
        "source console receives original caught Error argument"
    );
    assert_eq!(args[1].type_of(), "object");
    assert!(!args[1].read("name").unwrap().text().unwrap().is_empty());
    assert!(
        args[1]
            .read("message")
            .unwrap()
            .text()
            .unwrap()
            .contains("snapshot-call refusal")
    );
    drop(frames);
    case.owner.dispose().unwrap();
}
