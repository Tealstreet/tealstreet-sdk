//! Actual PM member futures and native request continuations; only HTTP is held.
use super::*;
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Held {
    release: [Arc<AtomicBool>; 2],
    requests: Arc<Mutex<Vec<RestRequest>>>,
    reject_conditional: bool,
}
impl RestAdapter for Held {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let conditional = request.url.contains("/conditional/");
        self.requests.lock().unwrap().push(request.clone());
        let release = self.release[usize::from(conditional)].clone();
        let rejected = conditional && self.reject_conditional;
        let mut response = Some(RestResponse {
            status: if rejected { 400 } else { 200 },
            reason: None,
            headers: Default::default(),
            body: RestResponseBody::Untransformed(if rejected {
                Input::from_value(json!({"code":-123,"msg":"snapshot continuation refusal"}))
                    .unwrap()
            } else {
                let mut row = Input::from_value(json!({
                    "clientOrderId":"regular", "newClientStrategyId":"conditional",
                    "symbol":"BTCUSDT", "type":"LIMIT", "strategyType":"STOP_MARKET",
                    "side":"BUY", "price":"101.25", "stopPrice":"99.5",
                    "origQty":"3.5", "executedQty":"0.75", "reduceOnly":false
                }))
                .unwrap();
                let Input::Object(fields) = &mut row else {
                    unreachable!()
                };
                fields.extend([
                    ("negativeZero".encode_utf16().collect(), Input::Float(-0.0)),
                    ("nan".encode_utf16().collect(), Input::Float(f64::NAN)),
                    ("ownUndefined".encode_utf16().collect(), Input::Undefined),
                ]);
                Input::Array(vec![row])
            }),
        });
        Box::pin(std::future::poll_fn(move |_| {
            if !release.load(Ordering::SeqCst) {
                return Poll::Pending;
            }
            let response = response.take().expect("one terminal HTTP settlement");
            Poll::Ready(if response.status == 200 {
                Ok(response)
            } else {
                Err(safe_cex_transport::RestFailure {
                    code: Some("ERR_BAD_REQUEST".into()),
                    message: "Request failed with status code 400".into(),
                    transport_error: None,
                    response: Some(Box::new(response)),
                    request: Box::new(request.clone()),
                    context: None,
                })
            })
        }))
    }
}
fn exercise(rejected: bool) {
    let runtime = Rc::new(Runtime::default());
    let releases = [
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    ];
    let requests = Arc::new(Mutex::new(vec![]));
    let deps = safe_cex_venue_binance::native_host::native_dependencies(
        runtime.clone(),
        Arc::new(Held {
            release: releases.clone(),
            requests: requests.clone(),
            reject_conditional: rejected,
        }),
        Rc::new(|_, _| panic!("unexpected external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let owner = ExchangeOwner::new(
        Venue::PM,
        safe_cex_types::ExchangeName::Binancepm,
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        deps,
    )
    .unwrap();
    let errors = Rc::new(RefCell::new(vec![]));
    let seen = errors.clone();
    owner.base.on(
        "error",
        Rc::new(move |_, error| {
            seen.borrow_mut().push(error.clone());
            Ok(())
        }),
    );
    let mut future = Box::pin(owner.invoke(Method::Orders, vec![]));
    for _ in 0..3 {
        runtime.pump();
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
    {
        let calls = requests.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "SnapshotFuture must await regular before conditional invocation"
        );
        assert!(calls[0].url.starts_with("/papi/v1/um/openOrders?"));
    }
    releases[0].store(true, Ordering::SeqCst);
    for _ in 0..12 {
        runtime.pump();
    }
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending(),
        "SnapshotRequestFuture remains pending until conditional HTTP settles"
    );
    {
        let calls = requests.lock().unwrap();
        assert_eq!(
            calls.len(),
            2,
            "each native member executes exactly one request"
        );
        assert!(
            calls[1]
                .url
                .starts_with("/papi/v1/um/conditional/openOrders?")
        );
        for call in calls.iter() {
            assert_eq!(call.method, "get");
            assert!(call.url.contains("signature=") && call.url.contains("timestamp="));
        }
    }
    releases[1].store(true, Ordering::SeqCst);
    let result = run(&runtime, future).unwrap();
    let EventValue::Array(rows) = result else {
        panic!("actual native orders array")
    };
    if rejected {
        assert!(
            rows.borrow().is_empty(),
            "failed conditional continuation returns source Store fallback, never partial regular rows"
        );
        assert_eq!(
            errors.borrow().len(),
            1,
            "caught request rejection reaches actual emitter"
        );
        assert!(
            errors.borrow()[0]
                .text()
                .unwrap()
                .contains("snapshot continuation refusal")
        );
    } else {
        let rows = rows.borrow();
        assert_eq!(
            rows.len(),
            2,
            "SnapshotFuture retains both sequential member results"
        );
        for (index, id) in ["regular", "conditional"].iter().enumerate() {
            let row = rows[index].as_ref().unwrap();
            assert_eq!(row.read("id").unwrap().text().unwrap(), *id);
            assert_eq!(
                row.read("amount").unwrap().number().unwrap().to_bits(),
                3.5f64.to_bits()
            );
            assert_eq!(
                row.read("filled").unwrap().number().unwrap().to_bits(),
                0.75f64.to_bits()
            );
            assert_eq!(
                row.read("remaining").unwrap().number().unwrap().to_bits(),
                2.75f64.to_bits()
            );
            let info = row.read("info").unwrap();
            let alias = info.clone();
            assert!(info.same(&alias), "original result referent survives await");
            assert_eq!(
                info.read("negativeZero")
                    .unwrap()
                    .number()
                    .unwrap()
                    .to_bits(),
                (-0.0f64).to_bits()
            );
            assert!(info.read("nan").unwrap().number().unwrap().is_nan());
            assert!(info.has_own("ownUndefined"));
            assert!(matches!(
                info.read("ownUndefined").unwrap(),
                EventValue::Undefined
            ));
        }
        assert!(errors.borrow().is_empty());
    }
    for _ in 0..3 {
        runtime.pump();
    }
    assert_eq!(
        requests.lock().unwrap().len(),
        2,
        "settlement never resends"
    );
    owner.dispose().unwrap();
}
#[test]
fn snapshot_futures_wait_sequentially_and_retain_native_results() {
    exercise(false)
}
#[test]
fn snapshot_futures_propagate_conditional_rejection_to_source_fallback() {
    exercise(true)
}
