#[path = "support/oracle_clock.rs"]
mod oracle_clock;

use safe_cex_exchange::{
    client_order_id::ClientOrderIdRule,
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::numeric_ingress::{JsNumberInput, js_value_as_f64};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_venue_bingx::{
    data_reads::{BingxDataReads, ReadTransport},
    placement::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let o = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/placement_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "BINGX-PLACE source {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..1000 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
    }
    panic!("fixture did not resolve within 1000 local polls")
}
fn number(n: f64) -> Value {
    json!({"number":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
}
fn observe(v: &Value) -> Value {
    match v {
        Value::Number(_) => number(js_value_as_f64(v).unwrap()),
        Value::Array(v) => Value::Array(v.iter().map(observe).collect()),
        Value::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        _ => v.clone(),
    }
}
#[derive(Default)]
struct Host {
    logs: RefCell<Vec<String>>,
    events: Rc<RefCell<Vec<String>>>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    exceptions: RefCell<Vec<String>>,
}
impl ExchangeRuntime for Host {
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn now_ms(&self) -> f64 {
        1234.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("no timers in this slice")
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        struct Task;
        impl ExchangeTask for Task {
            fn cancel(&self) {}
        }
        self.tasks.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        if severity == "error" {
            self.logs
                .borrow_mut()
                .push(message.strip_prefix("exchange error ").unwrap().into());
        }
    }
    fn unique_id(&self) -> String {
        "id".into()
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
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        self.exceptions.borrow_mut().push(error.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no worker in this slice")
    }
}
struct UuidHost {
    uuid: String,
    count: Cell<usize>,
}
impl PlacementHost for UuidHost {
    fn uuid(&self) -> Result<String, ExchangeError> {
        let n = self.count.get();
        self.count.set(n + 1);
        Ok(if n == 0 {
            self.uuid.clone()
        } else {
            format!("{}-{}", self.uuid, n + 1)
        })
    }
}
struct Transport {
    input: Value,
    cursor: Cell<usize>,
    calls: RefCell<Vec<Value>>,
}
impl ReadTransport for Transport {
    fn send(&self, r: safe_cex_transport::RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        let body = r.body.as_ref().map(|b| match b {
            safe_cex_transport::RestBody::Untransformed(v) => v.clone().into_value().unwrap(),
            _ => panic!("unexpected transformed request"),
        });
        let mut call = json!({"method":r.method,"url":r.url});
        if let Some(body) = body {
            call["body"] = observe(&body);
        }
        self.calls.borrow_mut().push(call);
        let result = if r.method == "GET" {
            Ok(json!({"code":0,"data":{"dualSidePosition":self.input["mode"]}}))
        } else {
            let index = self.cursor.get();
            self.cursor.set(index + 1);
            let response = self.input["replies"]
                .get(index)
                .cloned()
                .unwrap_or_else(|| json!({"code":0,"data":{}}));
            if let Some(reject) = response.get("reject") {
                let mut e = ExchangeError::new(reject["message"].as_str().unwrap());
                e.response = Some(json!({"data":{"msg":reject["msg"]}}));
                Err(e)
            } else {
                Ok(response)
            }
        };
        Box::pin(std::future::ready(
            result.map(|v| serde_json::from_value(v).unwrap()),
        ))
    }
}
fn fixture(
    input: Value,
) -> (
    Rc<BingxPlacement>,
    Rc<DefaultStore>,
    Rc<Transport>,
    Rc<Host>,
) {
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(serde_json::from_value(input["markets"].clone()).unwrap()),
            positions: Some(serde_json::from_value(input["positions"].clone()).unwrap()),
            tickers: Some(serde_json::from_value(input["tickers"].clone()).unwrap()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let host = Rc::new(Host::default());
    let rule = ClientOrderIdRule {
        label: "BingX".into(),
        requirement: "be 1-40 characters with no uppercase letters".into(),
        is_valid: Rc::new(|s| {
            !s.is_empty() && s.encode_utf16().count() <= 40 && s == s.to_lowercase()
        }),
        is_honorable_shape: None,
        mint: None,
    };
    let settings = ExchangeSettings {
        client_order_id_rule: Some(rule),
        ..Default::default()
    };
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Bingx,
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
        store.clone(),
        host.clone(),
        settings,
    ));
    let events = host.events.clone();
    base.on(
        "error",
        Rc::new(move |_, v| {
            let v = safe_cex_exchange::js_runtime::value::to_json(v).unwrap();
            events.borrow_mut().push(v.as_str().unwrap().into());
            Ok(())
        }),
    );
    let ids = Rc::new(UuidHost {
        uuid: input["uuid"].as_str().unwrap().into(),
        count: Cell::new(0),
    });
    let transport = Rc::new(Transport {
        input,
        cursor: Cell::new(0),
        calls: RefCell::new(vec![]),
    });
    (
        BingxPlacement::new(
            Rc::new(BingxDataReads::new(base, transport.clone())),
            transport.clone(),
            ids,
        ),
        store,
        transport,
        host,
    )
}
fn state(store: &DefaultStore) -> Value {
    json!(store.orders().iter().map(|o|{let mut out=json!({"id":o.id,"symbol":o.symbol,"status":o.status,"type":o.r#type,"side":o.side,"price":number((o.price).to_number()),"amount":number(o.amount),"filled":number(o.filled),"remaining":number(o.remaining),"reduceOnly":o.reduce_only,"close":o.close.into_value().map_or(json!({"undefined":true}),|b|json!(b)),"timeInForce":o.time_in_force.into_value().map_or(json!({"undefined":true}),|b|json!(b)),"lastUpdate":number(o.last_update)});out["info"]=o.info.as_ref().map(observe).unwrap_or(json!({"undefined":true}));out}).collect::<Vec<_>>())
}
fn invoke(venue: &Rc<BingxPlacement>, method: &str, input: &Value) -> Result<Value, ExchangeError> {
    let opts: PlaceOrderOpts = serde_json::from_value(input["opts"].clone()).unwrap();
    let ids = |v: Vec<Option<Value>>| {
        json!(
            v.iter()
                .map(|v| v.as_ref().map(observe).unwrap_or(json!({"undefined":true})))
                .collect::<Vec<_>>()
        )
    };
    match method {
        "createClientOrderId" => Ok(json!(create_client_order_id(
            input["uuid"].as_str().unwrap()
        ))),
        "getOrderPositionSide" => ready(venue.get_order_position_side(
            &safe_cex_types::PreparedOrderOpts::from(&opts),
            input["override"].as_str().map(|s| {
                if s == "hedged" {
                    PositionMode::Hedged
                } else {
                    PositionMode::OneWay
                }
            }),
        ))
        .map(|s| json!(s)),
        "formatCreateOrder" => {
            ready(venue.format_create_order(&safe_cex_types::PreparedOrderOpts::from(&opts)))
                .map(|v| observe(&json!(v)))
        }
        "formatCreateTrailingStopLossOrder" => ready(venue.format_create_trailing_stop_loss_order(
            &safe_cex_types::PreparedOrderOpts::from(&opts),
        ))
        .map(|v| observe(&json!(v))),
        "placeOrder" => {
            ready(venue.place_order(&safe_cex_types::PreparedOrderOpts::from(&opts))).map(ids)
        }
        "placeOrders" => ready(
            venue.place_orders(
                &serde_json::from_value::<Vec<safe_cex_types::PreparedOrderOpts>>(
                    input["orders"].clone(),
                )
                .unwrap(),
            ),
        )
        .map(ids),
        "placeOrderBatch" => ready(venue.place_order_batch(
            &serde_json::from_value::<Vec<Value>>(input["payloads"].clone()).unwrap(),
        ))
        .map(ids),
        "placeOrderBatchOld" => ready(venue.place_order_batch_old(
            &serde_json::from_value::<Vec<Value>>(input["payloads"].clone()).unwrap(),
        ))
        .map(ids),
        _ => unreachable!(),
    }
}
fn strip_attached_undefined(v: &mut Value) {
    if let Some(payloads) = v.get_mut("ok").and_then(Value::as_array_mut) {
        for payload in payloads.iter_mut().skip(1) {
            if payload["reduceOnly"] == json!({"undefined":true}) {
                payload.as_object_mut().unwrap().remove("reduceOnly");
            }
        }
    }
}
fn check(case: &Value) {
    let input = &case["input"];
    let (venue, store, transport, host) = fixture(input.clone());
    let method = case["method"].as_str().unwrap();
    let clock = oracle_clock::ClockWindow::start();
    let result = invoke(&venue, method, input);
    let clock = clock.finish();
    let mut native_state = state(&store);
    clock.normalize_orders(&mut native_state, &case["state"]);
    let output = match result {
        Ok(v) => json!({"ok":v}),
        Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
    };
    let mut expected = case["output"].clone();
    if case["label"] == "attached" {
        assert_eq!(
            expected["ok"][1]["reduceOnly"],
            json!({"undefined":true}),
            "BINGX-PLACE source attached own undefined field"
        );
        assert!(
            output["ok"][1].get("reduceOnly").is_none(),
            "BINGX-PLACE typed attached omits undefined field"
        );
        strip_attached_undefined(&mut expected);
    }
    assert_eq!(
        output, expected,
        "BINGX-PLACE native output {} {}",
        method, case["variant"]
    );
    assert_eq!(
        native_state, case["state"],
        "BINGX-PLACE decoded Store {method}"
    );
    assert_eq!(
        json!(*transport.calls.borrow()),
        case["calls"],
        "BINGX-PLACE request sequence/body {method}"
    );
    assert_eq!(
        *host.logs.borrow(),
        case["reports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r[0].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        "BINGX-PLACE visible error {method}"
    );
    assert_eq!(
        *host.events.borrow(),
        case["reports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| format!(
                "{}: {}",
                r.get(1).and_then(Value::as_str).unwrap_or("Exchange Error"),
                r[0].as_str().unwrap()
            ))
            .collect::<Vec<_>>(),
        "BINGX-PLACE error labels {method}"
    );
    assert!(
        host.tasks.borrow().is_empty(),
        "BINGX-PLACE no abandoned floor work"
    );
}
#[test]
fn placement_matches_actual_typescript() {
    let names = [
        "createClientOrderId",
        "getOrderPositionSide",
        "formatCreateOrder",
        "formatCreateTrailingStopLossOrder",
        "placeOrder",
        "placeOrders",
        "placeOrderBatch",
        "placeOrderBatchOld",
    ];
    assert_eq!(
        corpus()["names"],
        json!(names),
        "BINGX-PLACE actual source row census"
    );
    let cases = corpus()["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32, "BINGX-PLACE fixed floor");
    for (i, c) in cases.iter().enumerate() {
        assert_eq!(c["method"], names[i / 4], "BINGX-PLACE source method order");
        assert_eq!(
            c["variant"],
            json!(i % 4),
            "BINGX-PLACE source variant order"
        );
        check(c);
    }
    let extras = corpus()["extras"].as_array().unwrap();
    assert_eq!(extras.len(), 12, "BINGX-PLACE independent extra floor");
    for c in extras {
        check(c);
    }
    println!("BINGX-PLACE runtime {}", corpus()["runtime"]);
}
#[test]
fn utf16_and_typed_option_boundaries_are_explicit() {
    let units = corpus()["boundary"]["utf"]["units"].as_array().unwrap();
    assert_eq!(
        units.len(),
        40,
        "BINGX-PLACE actual source UTF16 slice count"
    );
    assert_eq!(
        units[39],
        json!(0xd83d),
        "BINGX-PLACE actual source lone surrogate"
    );
    let native = create_client_order_id(&("a".repeat(37) + "😀"));
    assert_eq!(
        native.chars().count(),
        40,
        "BINGX-PLACE native scalar slice count"
    );
    assert_eq!(
        native.encode_utf16().count(),
        41,
        "BINGX-PLACE explicit UTF8 representation boundary"
    );
    assert_eq!(
        corpus()["boundary"]["arbitrarySide"]["output"]["ok"],
        "SHORT",
        "BINGX-PLACE actual source truthy string reduceOnly"
    );
    let mut opts = corpus()["cases"][4]["input"]["opts"].clone();
    opts["reduceOnly"] = json!("false");
    assert!(
        serde_json::from_value::<PlaceOrderOpts>(opts).is_err(),
        "BINGX-PLACE typed bool refuses string"
    );
    assert!(
        corpus()["boundary"]["arbitraryTrailing"]["output"]["ok"][0]["side"]["callable"]
            .as_str()
            .unwrap()
            .contains("function Object"),
        "BINGX-PLACE source inherited side callable"
    );
    let mut opts = corpus()["cases"][12]["input"]["opts"].clone();
    opts["side"] = json!("constructor");
    assert!(
        serde_json::from_value::<PlaceOrderOpts>(opts).is_err(),
        "BINGX-PLACE typed side refuses inherited callable"
    );
}
#[test]
fn nan_payload_refusal_is_priced_for_formatter_and_wrappers() {
    for (method, index) in [
        ("formatCreateOrder", 8),
        ("placeOrder", 16),
        ("placeOrders", 20),
    ] {
        let source = &corpus()["boundary"]["nan"][method];
        if method == "formatCreateOrder" {
            assert_eq!(
                source["output"]["ok"][0]["quantity"],
                json!({"number":"NaN"}),
                "BINGX-PLACE actual source NaN formatter"
            );
        } else {
            assert_eq!(
                source["calls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| c["method"] == "POST")
                    .unwrap()["body"]["quantity"],
                json!({"number":"NaN"}),
                "BINGX-PLACE actual source NaN submission"
            );
        }
        let (venue, _, transport, _) = fixture(corpus()["cases"][index]["input"].clone());
        let mut opts: PlaceOrderOpts =
            serde_json::from_value(corpus()["cases"][index]["input"]["opts"].clone()).unwrap();
        opts.amount = f64::NAN;
        let error = match method {
            "formatCreateOrder" => {
                ready(venue.format_create_order(&safe_cex_types::PreparedOrderOpts::from(&opts)))
                    .unwrap_err()
            }
            "placeOrder" => {
                ready(venue.place_order(&safe_cex_types::PreparedOrderOpts::from(&opts)))
                    .unwrap_err()
            }
            _ => ready(venue.place_orders(&[(opts).into()])).unwrap_err(),
        };
        assert_eq!(
            error.message, "BingX typed payload refuses NaN",
            "BINGX-PLACE visible typed NaN refusal {method}"
        );
        assert!(
            transport
                .calls
                .borrow()
                .iter()
                .all(|c| c["method"] != "POST"),
            "BINGX-PLACE no native NaN POST {method}"
        );
    }
}
#[test]
fn callable_legacy_and_inherited_mapper_boundaries_are_explicit() {
    assert_eq!(
        corpus()["boundary"]["callableIds"],
        json!(["callable-id"]),
        "BINGX-PLACE actual source callable legacy forEach"
    );
    let mut input = corpus()["cases"][29]["input"].clone();
    input["replies"] = json!([{"code":0,"forEach":"callable"}]);
    let (venue, _, _, host) = fixture(input);
    assert_eq!(
        ready(venue.place_order_batch_old(&[json!({}), json!({})]))
            .unwrap_err()
            .message,
        "data?.forEach is not a function",
        "BINGX-PLACE typed legacy refuses callback property"
    );
    assert_eq!(
        *host.logs.borrow(),
        vec!["data?.forEach is not a function"],
        "BINGX-PLACE visible callback refusal"
    );
    let source = &corpus()["boundary"]["inheritedBatch"];
    assert_eq!(
        source["output"]["ok"],
        json!(["inherited-tif"]),
        "BINGX-PLACE actual source batch accepts inherited TIF"
    );
    assert!(
        source["state"][0]["timeInForce"]["callable"]
            .as_str()
            .unwrap()
            .contains("function Object"),
        "BINGX-PLACE source Store callable TIF"
    );
    let mut input = corpus()["cases"][24]["input"].clone();
    input["payloads"] = json!([input["payloads"][0]]);
    input["replies"][0]["data"]["order"]["timeInForce"] = json!("constructor");
    let (venue, store, _, host) = fixture(input.clone());
    let error = ready(venue.place_order_batch(
        &serde_json::from_value::<Vec<Value>>(input["payloads"].clone()).unwrap(),
    ))
    .unwrap_err();
    assert!(
        error.message.contains("inherited callable"),
        "BINGX-PLACE typed batch mapper refuses inherited TIF"
    );
    assert!(
        store.orders().is_empty(),
        "BINGX-PLACE no callable order cache"
    );
    assert_eq!(
        host.logs.borrow().len(),
        1,
        "BINGX-PLACE mapper refusal visible"
    );
}

#[test]
fn rejected_formatter_keeps_pending_sibling_running() {
    let source = &corpus()["boundary"]["lateSibling"];
    assert_eq!(
        source["early"], "Market MISSING not found",
        "BINGX-PLACE source early formatter rejection"
    );
    assert_eq!(
        source["before"], 0,
        "BINGX-PLACE source pending sibling before"
    );
    assert_eq!(
        source["after"], 1,
        "BINGX-PLACE source pending sibling completes"
    );
    let mut input = corpus()["cases"][20]["input"].clone();
    let mut market = input["markets"][0].clone();
    market["id"] = json!("XAUUSD-USDT");
    market["symbol"] = json!("XAUUSDUSDT");
    market["base"] = json!("XAUUSD");
    input["markets"].as_array_mut().unwrap().push(market);
    input["mode"] = json!("false");
    input["orders"][0]["symbol"] = json!("MISSING");
    input["orders"][1]["symbol"] = json!("XAUUSDUSDT");
    let (venue, _, transport, host) = fixture(input.clone());
    struct Deferred {
        inner: Rc<Transport>,
        release: Rc<Cell<bool>>,
    }
    impl ReadTransport for Deferred {
        fn send(&self, r: safe_cex_transport::RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
            let delayed = r.method == "GET" && !self.inner.calls.borrow().is_empty();
            let f = self.inner.send(r);
            let release = self.release.clone();
            Box::pin(async move {
                if delayed {
                    std::future::poll_fn(|cx| {
                        if release.get() {
                            Poll::Ready(())
                        } else {
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                    })
                    .await;
                    return Ok(serde_json::from_value(
                        json!({"code":0,"data":{"dualSidePosition":"true"}}),
                    )
                    .unwrap());
                }
                f.await
            })
        }
    }
    let release = Rc::new(Cell::new(false));
    let uuid = Rc::new(UuidHost {
        uuid: "ABCD-0123".into(),
        count: Cell::new(0),
    });
    let venue = BingxPlacement::new(
        Rc::new(BingxDataReads::new(
            venue.reads.base.clone(),
            Rc::new(Deferred {
                inner: transport.clone(),
                release: release.clone(),
            }),
        )),
        transport.clone(),
        uuid.clone(),
    );
    let error = ready(
        venue.place_orders(
            &serde_json::from_value::<Vec<safe_cex_types::PreparedOrderOpts>>(
                input["orders"].clone(),
            )
            .unwrap(),
        ),
    )
    .unwrap_err();
    assert_eq!(
        error.message, "Market MISSING not found",
        "BINGX-PLACE native early formatter rejection"
    );
    assert_eq!(uuid.count.get(), 0, "BINGX-PLACE native sibling is pending");
    assert_eq!(
        host.tasks.borrow().len(),
        1,
        "BINGX-PLACE pending sibling retained"
    );
    release.set(true);
    for task in host.tasks.borrow_mut().drain(..) {
        ready(task);
    }
    assert_eq!(
        uuid.count.get(),
        1,
        "BINGX-PLACE native sibling completes after rejection"
    );
    assert_eq!(
        json!(*transport.calls.borrow()),
        source["calls"],
        "BINGX-PLACE early failure starts all formatters"
    );
    assert!(
        host.exceptions.borrow().is_empty(),
        "BINGX-PLACE successful sibling has no exception"
    );
}
