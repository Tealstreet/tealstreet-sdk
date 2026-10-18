use crate::binding_test_harness::{self as harness, Harness, Http};
use safe_cex_exchange::{
    js_runtime::{client::JsHttpHost, promise::JsFuture, value::Js},
    runtime::ExchangeRuntime,
};
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Gate {
    released: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}

struct HeldHttp {
    wire: Rc<Http>,
    gate: Rc<Gate>,
}

impl JsHttpHost for HeldHttp {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.wire.runtime()
    }

    fn send(&self, config: Js) -> JsFuture<'static> {
        let response = self.wire.send(config);
        let gate = self.gate.clone();
        Box::pin(async move {
            std::future::poll_fn(|cx| {
                if gate.released.get() {
                    Poll::Ready(())
                } else {
                    *gate.waker.borrow_mut() = Some(cx.waker().clone());
                    Poll::Pending
                }
            })
            .await;
            response.await
        })
    }
}

fn held(method: &str, path: &str) -> (Harness, Rc<Gate>, Rc<RefCell<Vec<Js>>>) {
    let mut h = harness::harness(ExchangeName::Woo, harness::options(), |http| {
        http.route(
            method,
            path,
            500.0,
            json!({"message":"held native failure"}),
        );
    });
    let gate = Rc::new(Gate::default());
    h.exchange = super::WooExchange::new(
        ExchangeName::Woo,
        harness::options(),
        h.store.clone(),
        super::WooDependencies {
            runtime: h.runtime.clone(),
            http: Rc::new(HeldHttp {
                wire: h.http.clone(),
                gate: gate.clone(),
            }),
            external_public_data: None,
        },
    )
    .unwrap();
    let errors = Rc::new(RefCell::new(vec![]));
    let seen = errors.clone();
    h.venue().base().on(
        "error",
        Rc::new(move |_, error| {
            seen.borrow_mut().push(error.clone());
            Ok(())
        }),
    );
    (h, gate, errors)
}

fn observe_member(h: &Harness, member: &'static str) -> Rc<Cell<usize>> {
    let original = h.exchange.rt.this.get(member).unwrap();
    let reads = Rc::new(Cell::new(0));
    let seen = reads.clone();
    h.exchange
        .rt
        .this
        .define_accessor(
            member,
            Some(Rc::new(move |_| {
                seen.set(seen.get() + 1);
                Ok(original.clone())
            })),
            None,
        )
        .unwrap();
    reads
}

fn release(h: &Harness, gate: &Gate, errors: &RefCell<Vec<Js>>, path: &str) {
    h.runtime.pump();
    assert_eq!(h.http.calls(path).len(), 1, "unpolled SDK call must send");
    assert!(
        errors.borrow().is_empty(),
        "held response cannot report yet"
    );
    gate.released.set(true);
    if let Some(waker) = gate.waker.borrow_mut().take() {
        waker.wake();
    }
    h.runtime.pump();
    let errors = errors.borrow();
    assert_eq!(errors.len(), 1, "detached source catch must still report");
    assert!(errors[0].strict_equals(&Js::str("Exchange Error: held native failure")));
    let calls = h.http.calls(path);
    if calls[0].method == "DELETE" {
        assert!(calls[0].headers["x-api-signature"].as_str().is_some());
    }
}

#[test]
fn typed_member_prefix_runs_before_the_waiter_is_polled() {
    let (h, _, _) = held("GET", "/v1/public/info");
    let reads = observe_member(&h, "fetchMarkets");
    let waiter = h.venue().fetch_markets();
    assert_eq!(reads.get(), 1, "typed member lookup is synchronous");
    drop(waiter);
}

#[test]
fn unit_member_prefix_runs_before_the_waiter_is_polled() {
    let (h, _, _) = held("DELETE", "/v3/orders/pending");
    let reads = observe_member(&h, "cancelAllOrders");
    let waiter = h.venue().cancel_all_orders();
    assert_eq!(reads.get(), 1, "unit member lookup is synchronous");
    drop(waiter);
}

#[test]
fn typed_unpolled_waiter_drop_keeps_signed_request_and_catch_alive() {
    let (h, gate, errors) = held("GET", "/v1/public/info");
    drop(h.venue().fetch_markets());
    release(&h, &gate, &errors, "/v1/public/info");
}

#[test]
fn unit_unpolled_waiter_drop_keeps_signed_request_and_catch_alive() {
    let (h, gate, errors) = held("DELETE", "/v3/orders/pending");
    drop(h.venue().cancel_all_orders());
    release(&h, &gate, &errors, "/v3/orders/pending");
}

#[test]
fn ready_and_held_source_promises_keep_result_and_error_observation() {
    let h = harness::harness(ExchangeName::Woo, harness::options(), |http| {
        http.ok("GET", "/v1/public/info", json!({"rows":[]}));
        http.ok("DELETE", "/v3/orders/pending", json!({}));
    });
    assert!(h.drive(h.venue().fetch_markets()).unwrap().is_empty());
    h.drive(h.venue().cancel_all_orders()).unwrap();
    let (h, gate, errors) = held("GET", "/v1/public/info");
    let mut waiter = h.venue().fetch_markets();
    h.runtime.pump();
    assert!(
        waiter
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    release(&h, &gate, &errors, "/v1/public/info");
    assert!(h.drive(waiter).unwrap().is_empty());
}

#[test]
fn maintained_class_runs_both_prefixes_and_detached_catches() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/eager-binding-source.cjs"
        ))
        .output()
        .expect("maintained TypeScript class oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        json!({"families":2,"prefixes":2,"detachedReports":2})
    );
}
