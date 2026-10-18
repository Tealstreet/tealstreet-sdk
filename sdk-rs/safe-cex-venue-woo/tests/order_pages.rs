use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::ExchangeName;
use safe_cex_venue_woo::order_pages::fetch_orders_recursively;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
struct Runtime {
    logs: Rc<RefCell<Vec<Value>>>,
    serial: Cell<u64>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        0.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("no history tasks")
    }
    fn console(&self, level: &str, message: &str) {
        if level == "error" {
            self.logs.borrow_mut().push(json!(
                message
                    .strip_prefix("exchange error ")
                    .expect("caller diagnostic prefix")
            ));
        }
    }
    fn unique_id(&self) -> String {
        self.serial.set(self.serial.get() + 1);
        self.serial.get().to_string()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Err(ExchangeError::new("no worker")) })
    }
}
fn base(logs: Rc<RefCell<Vec<Value>>>, throw: bool) -> Rc<BaseExchange> {
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Woo,
        serde_json::from_value(json!({"key":"test","secret":"test"})).unwrap(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        Rc::new(Runtime {
            logs,
            serial: Cell::new(0),
        }),
        ExchangeSettings::default(),
    ));
    if throw {
        base.on(
            "error",
            Rc::new(|_, _| {
                let mut e = ExchangeError::new("logger failure");
                e.name = "TypeError".into();
                Err(e)
            }),
        );
    }
    base
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("PAGES unexpected suspension"),
    }
}
struct Adapter {
    label: String,
    base: Rc<BaseExchange>,
    calls: RefCell<Vec<Value>>,
    retained: RefCell<Vec<Value>>,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            assert_eq!(request.method, "GET", "PAGES native request method");
            self.calls
                .borrow_mut()
                .push(json!({"url":request.url,"params":request.params}));
            let ordinal = self.calls.borrow().len();
            assert!(ordinal <= 3, "PAGES native bounded attempts");
            let label = self.label.as_str();
            if label == "reject-later" && ordinal == 2 {
                return Err(RestFailure::pre_transmit("network failure".into(), request));
            }
            if label == "dispose-later" && ordinal == 2 {
                self.base.dispose().unwrap();
            }
            let rows: Vec<_> = ["a", "b", "c", "d", "e", "f"][(ordinal - 1) * 2..ordinal * 2]
                .iter()
                .map(|id| json!({"id":id}))
                .collect();
            let multi = matches!(
                label,
                "two"
                    | "dispose-later"
                    | "failure-later"
                    | "reject-later"
                    | "fraction"
                    | "mutate-earlier"
                    | "noniterable-first"
                    | "noniterable-tail"
            );
            let mut data = json!({"success":true,"rows":rows,"meta":{"total":if label=="three"{6}else if multi{4}else{2},"records_per_page":2}});
            if label == "empty" {
                data["rows"] = json!([]);
                data["meta"] = Value::Null;
            }
            if matches!(
                label,
                "failure" | "meta-message" | "default-message" | "logger-throw"
            ) || label == "failure-later" && ordinal == 2
            {
                data = json!({"success":false,"message":if matches!(label,"meta-message"|"default-message"){""}else{"top"},"meta":{"message":if label=="default-message"{json!(0)}else{json!("nested")}}});
            }
            if label == "nested" {
                data = json!({"success":true,"data":data,"rows":[{"id":"wrong"}]});
            }
            if label == "false-nested" {
                data["data"] = json!(false);
            }
            if label == "nested-array" {
                data["data"] = json!([]);
            }
            if label == "missing-rows" {
                data.as_object_mut().unwrap().remove("rows");
            }
            if label == "missing-meta" {
                data.as_object_mut().unwrap().remove("meta");
            }
            if matches!(label, "noniterable-first" | "metadata-refusal")
                || label == "noniterable-tail" && ordinal == 2
            {
                data["rows"] = json!({"length":2});
            }
            if label == "metadata-error" {
                data["rows"] = json!({"length":2});
                data["meta"]["total"] = json!({"toString":null});
            }
            self.retained.borrow_mut().push(data.clone());
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Transformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(data).unwrap(),
                ),
            })
        })
    }
}
fn pack(v: &Value) -> Value {
    match v {
        Value::Number(_) => {
            json!({"$bits":format!("{:016x}",safe_cex_num::js_number(Some(v)).unwrap().to_bits())})
        }
        Value::Array(a) => Value::Array(a.iter().map(pack).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), pack(v))).collect()),
        v => v.clone(),
    }
}
#[test]
fn order_pages_match_source_phases() {
    let output = Command::new("node")
        .arg(format!(
            "{}/scripts/order-pages-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 22, "PAGES source corpus count");
    let (mut compared, mut exclusions, mut outputs, mut requests, mut reports) = (0, 0, 0, 0, 0);
    for index in 0..22 {
        let c = &cases[index];
        let label = c["label"].as_str().unwrap();
        if matches!(
            label,
            "nan-page"
                | "mutate-earlier"
                | "missing-rows"
                | "missing-meta"
                | "nested-array"
                | "noniterable-first"
                | "noniterable-tail"
        ) {
            exclusions += 1;
            continue;
        }
        let logs = Rc::new(RefCell::new(vec![]));
        let base = base(logs.clone(), label == "logger-throw");
        if label == "disposed" {
            base.dispose().unwrap();
        }
        let adapter = Adapter {
            label: label.into(),
            base: base.clone(),
            calls: RefCell::new(vec![]),
            retained: RefCell::new(vec![]),
        };
        let outcome = ready(fetch_orders_recursively(
            &base,
            &adapter,
            "/order-test",
            Some(if label == "fraction" { 1.5 } else { 1.0 }),
        ));
        let expected = &c["outcome"];
        if let Some(rows) = expected.get("rows") {
            let actual = outcome.unwrap();
            assert_eq!(
                actual.len(),
                rows.as_array().unwrap().len(),
                "PAGES native output cardinality {label}"
            );
            assert_eq!(
                &Value::Array(actual.clone()),
                rows,
                "PAGES native row identity {label}"
            );
            outputs += actual.len();
        } else {
            let error = outcome.unwrap_err();
            assert_eq!(
                error.to_string(),
                expected["error"]["message"].as_str().unwrap(),
                "PAGES native error phase {label}"
            );
            if label == "reject-later" {
                assert!(
                    error.downcast_ref::<RestFailure>().is_some(),
                    "PAGES transport error retained"
                );
            } else {
                assert_eq!(
                    error.downcast_ref::<ExchangeError>().unwrap().name,
                    "TypeError",
                    "PAGES reporter error retained"
                );
            }
        }
        assert_eq!(
            &*logs.borrow(),
            c["logs"].as_array().unwrap(),
            "PAGES native reporting {label}"
        );
        reports += logs.borrow().len();
        let calls = adapter.calls.borrow();
        assert_eq!(
            calls.len(),
            c["calls"].as_array().unwrap().len(),
            "PAGES native request cardinality {label}"
        );
        for j in 0..calls.len() {
            assert_eq!(
                pack(&calls[j]),
                c["calls"][j],
                "PAGES native exact request {label}"
            );
        }
        requests += calls.len();
        compared += 1;
    }
    assert_eq!(
        (compared, exclusions),
        (15, 7),
        "PAGES admitted source partition"
    );
    assert_eq!(
        (outputs, requests, reports),
        (24, 22, 5),
        "PAGES independent native totals"
    );
}
#[test]
fn metadata_coercion_precedes_typed_row_refusal() {
    // Competing failures identify the first phase; the paired valid metadata
    // case proves these same rows really reach the later representation refusal.
    for label in ["metadata-error", "metadata-refusal"] {
        let base = base(Rc::new(RefCell::new(vec![])), false);
        let adapter = Adapter {
            label: label.into(),
            base: base.clone(),
            calls: RefCell::new(vec![]),
            retained: RefCell::new(vec![]),
        };
        let error = ready(fetch_orders_recursively(
            &base,
            &adapter,
            "/order-test",
            None,
        ))
        .unwrap_err();
        if label == "metadata-error" {
            let error = error
                .downcast_ref::<safe_cex_num::JsCoercionError>()
                .expect("PAGES metadata coercion wins first over typed rows");
            assert_eq!(error.name, "TypeError", "PAGES metadata error name");
            assert_eq!(
                error.message, "Cannot convert object to primitive value",
                "PAGES metadata coercion before typed rows"
            );
        } else {
            assert_eq!(
                error
                    .downcast_ref::<ExchangeError>()
                    .expect("PAGES valid metadata reaches typed rows")
                    .message,
                "order page requires array rows",
                "PAGES valid metadata reaches typed rows"
            );
        }
        assert_eq!(
            adapter.calls.borrow().len(),
            1,
            "PAGES metadata request count"
        );
    }
}

#[test]
fn order_page_owned_snapshot_is_not_source_alias() {
    let output = Command::new("node")
        .arg(format!(
            "{}/scripts/order-pages-boundaries.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["cases"].as_array().unwrap().len(),
        4,
        "PAGES boundary source total"
    );
    assert_eq!(
        source["ordering"].as_array().unwrap().len(),
        2,
        "PAGES source ordering count"
    );
    for index in 0..2 {
        let mut expected = vec![
            "rows:length",
            "meta:total",
            "meta:records_per_page",
            "size:coerce",
            "total:coerce",
        ];
        if index == 0 {
            expected.push("rows:iterator");
        }
        assert_eq!(
            source["ordering"][index]["events"],
            json!(expected),
            "PAGES source metadata first phase order"
        );
        assert_eq!(
            source["ordering"][index]["firstFailure"],
            if index == 0 { "rows" } else { "metadata" },
            "PAGES source first failure identity"
        );
    }
    for index in 0..2 {
        let base = base(Rc::new(RefCell::new(vec![])), false);
        let adapter = Adapter {
            label: if index == 0 { "direct" } else { "two" }.into(),
            base: base.clone(),
            calls: RefCell::new(vec![]),
            retained: RefCell::new(vec![]),
        };
        let rows = ready(fetch_orders_recursively(
            &base,
            &adapter,
            "/order-test",
            None,
        ))
        .unwrap();
        assert_eq!(
            rows.len(),
            if index == 0 { 2 } else { 4 },
            "PAGES snapshot native count"
        );
        assert_eq!(
            source["cases"][index]["before"].as_array().unwrap().len(),
            if index == 0 { 2 } else { 4 },
            "PAGES snapshot source count"
        );
        assert_eq!(
            source["cases"][index]["before"][0]["id"],
            if index == 0 {
                "a"
            } else {
                "changed-during-next-request"
            },
            "PAGES source referent before return"
        );
        adapter.retained.borrow_mut()[0]["rows"][0]["id"] = json!("changed-after-return");
        assert_eq!(
            adapter.retained.borrow()[0]["rows"][0]["id"],
            "changed-after-return",
            "PAGES retained input changed"
        );
        assert_eq!(
            rows[0]["id"], "a",
            "PAGES owned native snapshot remains frozen"
        );
        assert_eq!(
            source["cases"][index]["after"][0]["id"], "changed-after-return",
            "PAGES source live referent changes"
        );
    }
}
