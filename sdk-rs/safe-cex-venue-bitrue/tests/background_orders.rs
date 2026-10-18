#[path = "support/binding_runtime.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, from_json, from_typed, to_json},
    },
    runtime::ExchangeRuntime,
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::ExchangeName;
use safe_cex_venue_bitrue::{BitrueDependencies, BitrueExchange};
use serde_json::json;
use std::{
    cell::Cell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

struct Http {
    runtime: Rc<support::Runtime>,
    native_type: &'static str,
    order_reads: Cell<usize>,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        assert_eq!(
            to_json(&config.get("url").unwrap()).unwrap(),
            "/fapi/v2/openOrders"
        );
        assert_eq!(
            to_json(&config.get("params").unwrap().get("contractName").unwrap()).unwrap(),
            "E-BTC-USDT"
        );
        let headers = to_json(&config.get("headers").unwrap()).unwrap();
        assert_eq!(headers["X-CH-APIKEY"], "fixture-key");
        let timestamp = headers["X-CH-TS"].as_f64().unwrap();
        let payload = format!("{timestamp}GET/fapi/v2/openOrders?contractName=E-BTC-USDT");
        assert_eq!(
            headers["X-CH-SIGN"],
            safe_cex_crypto::hmac_sha256(
                b"fixture-secret",
                payload.as_bytes(),
                safe_cex_crypto::Encoding::Hex
            )
        );
        self.order_reads.set(self.order_reads.get() + 1);
        let row = |id: &str, kind: &str| json!({"orderId":id,"side":"BUY","type":kind,"status":"NEW","action":"OPEN","origQty":0.4,"executedQty":0.1,"price":27234.5,"transactTime":1700000000000.0});
        let response = Js::object(vec![
            ("status", Js::Number(200.0)),
            (
                "data",
                from_json(
                    &json!({"code":"0","data":[row("known-order","LIMIT"),row("native-order",self.native_type)]}),
                ),
            ),
            ("config", config),
        ]);
        Box::pin(async move { Ok(response) })
    }
}
fn complete<T>(runtime: &support::Runtime, future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..128 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.pump();
        runtime.advance(100.0);
    }
    panic!("production Bitrue read must settle")
}
struct Fixture {
    venue: Rc<BitrueExchange>,
    runtime: Rc<support::Runtime>,
    http: Rc<Http>,
}
impl Fixture {
    fn new(kind: &'static str) -> Self {
        let runtime = Rc::new(support::Runtime::default());
        let store = Rc::new(DefaultStore::new());
        let http = Rc::new(Http {
            runtime: runtime.clone(),
            native_type: kind,
            order_reads: Cell::new(0),
        });
        let venue = BitrueExchange::new(
            ExchangeName::Bitrue,
            serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
            store.clone(),
            BitrueDependencies {
                runtime: runtime.clone(),
                http: http.clone(),
                external_public_data: None,
            },
        )
        .unwrap();
        let market = serde_json::from_value(json!({"id":"E-BTC-USDT","symbol":"E-BTC-USDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":20}}})).unwrap();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(vec![market]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        Self {
            venue,
            runtime,
            http,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.venue.dispose().unwrap();
    }
}
#[test]
fn background_refresh_reads_symbols_from_the_real_priority_set() {
    let f = Fixture::new("LIMIT");
    let result = complete(&f.runtime, f.venue.fetch_orders(None)).unwrap();
    assert!(
        result.is_empty(),
        "source returns before background reconciliation"
    );
    for _ in 0..5 {
        f.runtime.pump();
        f.runtime.advance(100.0);
    }
    assert_eq!(
        f.http.order_reads.get(),
        1,
        "source background loop must read the listed market; reports={:?}",
        f.runtime.reports.borrow()
    );
    let orders = f.venue.base().store.orders();
    assert_eq!(orders.len(), 2);
    assert!(orders.iter().any(|order| order.id == "known-order"));
    assert!(orders.iter().any(|order| order.id == "native-order"));
}

fn assert_native_type(order: &safe_cex_types::Order, kind: &str) {
    assert!(
        order.r#type.is_undefined(),
        "unmapped native type must remain own undefined"
    );
    let native = from_typed(order).unwrap();
    assert!(native.has_own("type"));
    assert!(matches!(native.get("type").unwrap(), Js::Undefined));
    assert!(
        order
            .info
            .value()
            .unwrap()
            .get("type")
            .unwrap()
            .text_equals(kind)
    );
}
fn typed_case(kind: &'static str) {
    let f = Fixture::new(kind);
    let orders = complete(
        &f.runtime,
        f.venue.fetch_orders(Some(vec!["E-BTC-USDT".into()])),
    )
    .expect("source retains both known and unmapped native order types");
    assert_eq!(
        orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["known-order", "native-order"]
    );
    assert_eq!(
        orders[0].r#type.value(),
        Some(&safe_cex_types::OrderType::Limit)
    );
    assert_native_type(&orders[1], kind);
    assert_eq!(f.http.order_reads.get(), 1);
}
#[test]
fn real_binding_retains_ioc_order() {
    typed_case("IOC");
}
#[test]
fn real_binding_retains_fok_order() {
    typed_case("FOK");
}
#[test]
fn real_binding_retains_post_only_order() {
    typed_case("POST_ONLY");
}
#[test]
fn real_binding_retains_stop_limit_order() {
    typed_case("STOP_LIMIT");
}
#[test]
fn background_refresh_commits_unmapped_native_order_types() {
    for kind in ["IOC", "FOK", "POST_ONLY", "STOP_LIMIT"] {
        let f = Fixture::new(kind);
        assert!(
            complete(&f.runtime, f.venue.fetch_orders(None))
                .unwrap()
                .is_empty()
        );
        for _ in 0..5 {
            f.runtime.pump();
            f.runtime.advance(100.0);
        }
        assert_eq!(f.http.order_reads.get(), 1);
        let orders = f.venue.base().store.orders();
        assert_eq!(
            orders.len(),
            2,
            "{kind} must not stop the symbol's Store reconciliation"
        );
        assert_native_type(
            orders.iter().find(|o| o.id == "native-order").unwrap(),
            kind,
        );
        assert!(f.runtime.reports.borrow().is_empty());
    }
}
