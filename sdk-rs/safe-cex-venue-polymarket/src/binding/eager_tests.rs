use super::{PolymarketDependencies, PolymarketExchange};
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client::JsHttpHost,
        promise::{JsFuture, Promise, await_js},
        value::{Js, Thrown, from_json, from_retained_input, to_json},
    },
    native::native_runtime,
    native_store::DefaultStore,
    runtime::ExchangeRuntime,
};
use safe_cex_types::{ExchangeName, retained_json::RetainedJsonInput};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    sync::OnceLock,
};
struct NoHttp(Rc<dyn ExchangeRuntime>);
impl JsHttpHost for NoHttp {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _: Js) -> JsFuture<'static> {
        panic!("terminal xhr seam must handle this request")
    }
}
fn run(body: impl Future<Output = ()>) {
    let host = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&host, body);
}
fn venue() -> Rc<PolymarketExchange> {
    let runtime = native_runtime();
    PolymarketExchange::new(
        ExchangeName::Polymarket,
        serde_json::from_value(
            json!({"key":"","secret":"","publicDataOnly":true,"tuning":{"consoleLogging":false}}),
        )
        .unwrap(),
        Rc::new(DefaultStore::new()),
        PolymarketDependencies {
            runtime: runtime.clone(),
            http: Rc::new(NoHttp(runtime)),
            external_public_data: None,
        },
    )
    .unwrap()
}
fn source(name: &str, whole: bool) -> Value {
    let mut command = std::process::Command::new("node");
    if whole {
        command.args(["--import", "tsx"]);
    }
    let result = command
        .arg(format!("{}/tests/{name}", env!("CARGO_MANIFEST_DIR")))
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "maintained source: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn sdk_source() -> &'static Value {
    static SOURCE: OnceLock<Value> = OnceLock::new();
    SOURCE.get_or_init(|| source("eager_sdk_source.cjs", true))
}
#[test]
fn sdk_typed_prefix_and_dropped_waiter_retain_real_market_continuation() {
    run(async {
        let expected = sdk_source();
        let venue = venue();
        let runtime = venue.rt.exchange_runtime();
        let xhr = venue.rt.this.get("api").unwrap().get("gammaXhr").unwrap();
        let Js::Object(object) = &xhr else {
            panic!("xhr must be object")
        };
        let receiver = Rc::downgrade(object);
        let calls = Rc::new(RefCell::new(vec![]));
        let seen = calls.clone();
        let pending = Promise::pending(&runtime);
        let reply = pending.clone();
        xhr.define(
            "get",
            Js::function("get", move |this, args| {
                assert!(
                    this.same_reference(&Js::Object(receiver.upgrade().unwrap())),
                    "SDK xhr receiver identity"
                );
                seen.borrow_mut().push(args);
                Ok(Js::Promise(reply.clone()))
            }),
        )
        .unwrap();
        let waiter = venue.fetch_markets();
        assert_eq!(
            calls.borrow().len(),
            1,
            "POLY-SDK-TYPED-PREFIX invokes actual API before waiter polling"
        );
        assert_eq!(
            to_json(&Js::array(calls.borrow()[0].clone())).unwrap(),
            to_json(&from_json(&expected["calls"][0])).unwrap()
        );
        drop(waiter);
        let row = from_json(&expected["gamma"]);
        pending.settle(Ok(Js::object(vec![("data", Js::array(vec![row.clone()]))])));
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
        let metadata = venue
            .rt
            .this
            .get("marketMetadata")
            .unwrap()
            .call_method("get", vec![Js::str("condition")])
            .unwrap();
        assert!(
            metadata.same_reference(&row),
            "POLY-SDK-DROP retains source metadata alias"
        );
        assert_eq!(
            venue
                .rt
                .this
                .get("tokenToSymbol")
                .unwrap()
                .get("size")
                .unwrap()
                .to_number()
                .unwrap(),
            expected["marketCount"].as_f64().unwrap(),
            "POLY-SDK-DROP preserves complete token mapping count"
        );
        let symbol = venue
            .rt
            .this
            .get("tokenToSymbol")
            .unwrap()
            .call_method("get", vec![Js::str("123")])
            .unwrap();
        assert_eq!(
            to_json(&symbol).unwrap(),
            expected["symbol"],
            "POLY-SDK-DROP completes token mapping"
        );
        venue.dispose().unwrap();
    });
}
#[test]
fn sdk_unit_warning_precedes_waiter_poll_and_drop() {
    run(async {
        let venue = venue();
        let logs = Rc::new(RefCell::new(vec![]));
        let seen = logs.clone();
        venue.base().on(
            "log",
            Rc::new(move |_, value| {
                seen.borrow_mut().push(to_json(&value)?);
                Ok(())
            }),
        );
        let waiter = venue.set_leverage("event-Yes".into(), 15., None);
        assert_eq!(
            logs.borrow().len(),
            1,
            "POLY-SDK-UNIT-PREFIX logs before waiter polling"
        );
        assert_eq!(logs.borrow()[0], sdk_source()["warnings"][0][0]);
        drop(waiter);
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            logs.borrow().len(),
            1,
            "unit does not duplicate source Promise"
        );
        venue.dispose().unwrap();
    });
}
fn time_getter(
    venue: &PolymarketExchange,
    result: Result<Js, Thrown>,
    count: Rc<Cell<usize>>,
) -> (Js, Js) {
    let api = venue.rt.this.get("api").unwrap();
    let xhr = api.get("xhr").unwrap();
    let Js::Object(object) = &xhr else {
        panic!("xhr must be object")
    };
    let owner = Rc::downgrade(object);
    xhr.define(
        "get",
        Js::function("get", move |this, args| {
            count.set(count.get() + 1);
            assert!(
                this.same_reference(&Js::Object(owner.upgrade().unwrap())),
                "live time xhr receiver identity"
            );
            assert_eq!(args.len(), 2, "live API config slot");
            assert_eq!(to_json(&args[0]).unwrap(), "/time", "class_cost_http_url");
            result.clone()
        }),
    )
    .unwrap();
    (api.clone(), api.get("getServerTime").unwrap())
}
#[test]
fn live_server_time_keeps_eager_calls_raw_values_and_original_failures() {
    run(async {
        let expected = source("public_reads_oracle.cjs", false);
        let venue = venue();
        let runtime = venue.rt.exchange_runtime();
        let cases = expected["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 20, "time_source_case_count");
        for (index, case) in cases.iter().enumerate() {
            let value = from_retained_input(
                RetainedJsonInput::parse_json(
                    &case["raw"]
                        .as_str()
                        .unwrap()
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                )
                .unwrap(),
            );
            let count = Rc::new(Cell::new(0));
            let (api, get) = time_getter(
                &venue,
                Ok(Js::object(vec![("data", value.clone())])),
                count.clone(),
            );
            let result = get.call(api, vec![]).unwrap();
            assert_eq!(count.get(), 1, "live time prefix:{index}");
            let result = await_js(&runtime, result).await.unwrap();
            assert!(
                result.same_reference(&value) || result.strict_equals(&value),
                "time raw identity:{index}"
            );
            if case["type"] == "number" {
                assert_eq!(
                    format!("{:016x}", result.to_number().unwrap().to_bits()),
                    case["bits"],
                    "time_native_bits:{index}"
                );
            }
        }
        assert_eq!(expected["failures"].as_array().unwrap().len(), 3);
        for case in expected["failures"].as_array().unwrap() {
            let cause = from_json(
                &json!({"message":"fixture failure","code":case["code"],"request":{"url":"/changed"},"response":{"status":429,"data":{"error":"limit"}},"context":{"id":7}}),
            );
            let count = Rc::new(Cell::new(0));
            for asynchronous in [false, true] {
                let failure = Err(Thrown::new(cause.clone()));
                let reply = if asynchronous {
                    Ok(Js::Promise(Promise::settled(&runtime, failure)))
                } else {
                    failure
                };
                let (api, get) = time_getter(&venue, reply, count.clone());
                let result = get.call(api, vec![]).unwrap();
                assert_eq!(count.get(), 1);
                let error = await_js(&runtime, result).await.unwrap_err();
                assert!(
                    error.value.same_reference(&cause),
                    "time original failure identity"
                );
                assert_eq!(to_json(&error.value).unwrap(), to_json(&cause).unwrap());
                count.set(0);
            }
        }
        venue.dispose().unwrap();
    });
}
#[test]
fn class_cost_http_native() {
    run(async {
        let expected = source("class_cost_http.cjs", false);
        let cases = expected["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 512, "HTTP independent case floor");
        let venue = venue();
        let runtime = venue.rt.exchange_runtime();
        for (index, case) in cases.iter().enumerate() {
            let n = index as f64;
            let value = [
                n * 123.25,
                -n * 0.125,
                -0.,
                n / 1024.,
                1e21 + n * 1048576.,
                1e-300 * (n + 1.),
                1.7e308,
                0.,
            ][index % 8];
            let failure = Js::object(vec![("marker", Js::Number(n))]);
            let reply = if index % 8 == 7 {
                Err(Thrown::new(failure.clone()))
            } else {
                Ok(Js::object(vec![("data", Js::Number(value))]))
            };
            let count = Rc::new(Cell::new(0));
            let (api, get) = time_getter(&venue, reply, count.clone());
            let promise = get.call(api, vec![]).unwrap();
            assert_eq!(count.get(), 1, "HTTP native eager prefix:{index}");
            assert_eq!(case["immediateCalls"], 1);
            assert_eq!(case["index"], index, "HTTP iteration identity");
            assert_eq!(case["calls"], json!(["/time"]), "HTTP source request");
            assert_eq!(
                case["inputBits"],
                format!("{:016x}", value.to_bits()),
                "HTTP independent input"
            );
            let actual = await_js(&runtime, promise).await;
            if index % 8 == 7 {
                assert!(
                    actual.unwrap_err().value.same_reference(&failure),
                    "class_cost_http_failure:{index}"
                );
                assert_eq!(case["preserved"], true);
            } else {
                assert_eq!(
                    format!("{:016x}", actual.unwrap().to_number().unwrap().to_bits()),
                    case["outputBits"],
                    "class_cost_http_native:{index}"
                );
            }
        }
        venue.dispose().unwrap();
    });
}
