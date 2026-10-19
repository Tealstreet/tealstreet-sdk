//! Actual constructor-owned clients, shared weight budget and retained promises.
use super::{HyperliquidDependencies, HyperliquidExchange};
use crate::{binding_test_runtime as host, rate_limit::RateLimiter, realm::Value};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, TransportError,
    TransportFuture, rate_limit::RateLimitClock,
};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    future::poll_fn,
    process::Command,
    rc::{Rc, Weak},
    sync::Arc,
    task::{Poll, Waker},
};

type Waiter = Weak<RefCell<Option<Waker>>>;
#[derive(Default)]
struct Clock {
    now: Cell<u64>,
    waiting: RefCell<Vec<(u64, Waiter)>>,
}
impl Clock {
    fn advance(&self, at: u64) {
        self.now.set(at);
        let mut wake = vec![];
        self.waiting.borrow_mut().retain(|(deadline, slot)| {
            let Some(slot) = slot.upgrade() else {
                return false;
            };
            if *deadline <= at {
                if let Some(waker) = slot.borrow_mut().take() {
                    wake.push(waker);
                }
                false
            } else {
                true
            }
        });
        for waker in wake {
            waker.wake();
        }
    }
}
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        self.now.get()
    }
    fn sleep_ms(&self, delay: u64) -> TransportFuture<'_, ()> {
        let deadline = self.now.get() + delay;
        let wake = Rc::new(RefCell::new(None));
        self.waiting
            .borrow_mut()
            .push((deadline, Rc::downgrade(&wake)));
        Box::pin(poll_fn(move |cx| {
            if self.now.get() >= deadline {
                Poll::Ready(Ok(()))
            } else {
                *wake.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        }))
    }
}
struct Adapter {
    clock: Arc<Clock>,
    trace: Rc<RefCell<Vec<Json>>>,
    account: &'static str,
    fail: bool,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            let body: Json =
                serde_json::from_slice(&request.body.as_ref().unwrap().wire_bytes().unwrap())
                    .unwrap();
            self.trace.borrow_mut().push(
                json!({"account":self.account,"label":body["label"],"at":self.clock.now.get()}),
            );
            if self.fail && body["label"] == "first" {
                return Err(RestFailure::from_transport(
                    TransportError::Host("recorded rejection".into()),
                    request,
                ));
            }
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Untransformed(
                    json!({"status":"ok"}).into(),
                ),
            })
        })
    }
}
async fn flush() {
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn weight_limiter_audit_constructed_clients_keep_shared_budget_and_discarded_work() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/weight-limiter-audit.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Json> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 4);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        for row in source {
            let clock = Arc::new(Clock::default());
            let limiter = Rc::new(RateLimiter::with_defaults(clock.clone(), None, None));
            let trace = Rc::new(RefCell::new(vec![]));
            let runtime = Rc::new(host::Host::with_native(safe_cex_exchange::native::native_runtime()));
            let make = |account| HyperliquidExchange::new(
                serde_json::from_value(json!({"key":"","secret":"","applicationId":"0x1111111111111111111111111111111111111111"})).unwrap(),
                Rc::new(DefaultStore::new()), HyperliquidDependencies {runtime:runtime.clone(),
                    http:Arc::new(Adapter {clock:clock.clone(),trace:trace.clone(),account,fail:row["fail"].as_bool().unwrap()}),
                    limiter:limiter.clone(),signers:Default::default(),external_public_data:None},
            ).unwrap();
            let a = make("a"); let b = make("b");
            let binding_a = a.binding.borrow().as_ref().unwrap().clone();
            let binding_b = b.binding.borrow().as_ref().unwrap().clone();
            let mut realm_a = binding_a.realm; let mut realm_b = binding_b.realm;
            let client_a = realm_a.get_str(&binding_a.instance, "unlimitedXHR").unwrap();
            let client_b = realm_b.get_str(&binding_b.instance, "unlimitedXHR").unwrap();
            let body = |label, size| json!({"label":label,"action":{"type":"order","orders":vec![Json::Null;size]}});
            let first_body = realm_a.from_serde(&body("first",80));
            let first = realm_a.call_method(&client_a,"post",&[Value::str("/exchange"),first_body]).unwrap();
            let second_body = realm_b.from_serde(&body("second",0));
            let mut second = Some(realm_b.call_method(&client_b,"post",&[Value::str("/exchange"),second_body]).unwrap());
            let third_body = realm_a.from_serde(&body("third",40));
            let third = realm_a.call_method(&client_a,"post",&[Value::str("/exchange"),third_body]).unwrap();
            if row["discard"] == true { drop(second.take()); }
            let synchronous = trace.borrow().len();
            flush().await;
            let mut stages = vec![json!({"at":0,"trace":*trace.borrow()})];
            for at in [149,150,199,200] {
                clock.advance(at); flush().await;
                stages.push(json!({"at":at,"trace":*trace.borrow()}));
            }
            let first_result = realm_a.await_value(first).await;
            assert_eq!(first_result.is_err(), row["fail"], "HL_WEIGHT_LIMITER request rejection");
            realm_a.await_value(third).await.unwrap();
            if let Some(second) = second { realm_b.await_value(second).await.unwrap(); }
            assert_eq!(stages, row["stages"].as_array().unwrap().clone(), "HL_WEIGHT_LIMITER shared FIFO/dropped promise/source timing");
            assert_eq!(synchronous as u64, row["synchronous"].as_u64().unwrap(), "HL_WEIGHT_LIMITER call-return checkpoint");
        }
    });
}
