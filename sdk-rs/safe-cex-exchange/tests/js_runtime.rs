//! js_runtime semantics a venue binding depends on, over a recording runtime.
use safe_cex_exchange::{
    js_runtime::{
        client::{self, JsHttpHost},
        promise::{JsFuture, Promise, await_js},
        value::{Js, Thrown, arg, to_json, utf16},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Runtime {
    jobs: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    next_id: std::cell::Cell<u64>,
    clock: std::cell::Cell<f64>,
    wall_clock: std::cell::Cell<f64>,
    consoled: RefCell<Vec<(String, String)>>,
    console_arguments: RefCell<Vec<Vec<Js>>>,
    raw_exceptions: RefCell<Vec<Js>>,
    exceptions: RefCell<Vec<(String, Value)>>,
    hold_sleep: std::cell::Cell<bool>,
    headers_capable: std::cell::Cell<bool>,
    worker: std::cell::Cell<bool>,
    dial_socket: RefCell<Option<Rc<dyn safe_cex_exchange::websocket::WsSocket>>>,
    dials: RefCell<
        Vec<(
            String,
            Vec<String>,
            safe_cex_exchange::websocket::WebSocketConnectionOptions,
        )>,
    >,
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.clock.get()
    }
    fn now_ms(&self) -> f64 {
        self.wall_clock.get()
    }
    fn sleep(&self, ms: u64) -> ExchangeFuture<'static, ()> {
        if self.hold_sleep.get() {
            return Box::pin(std::future::pending());
        }
        self.clock.set(self.clock.get() + ms as f64);
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        self.console_arguments.borrow_mut().push(message.to_vec());
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.consoled
            .borrow_mut()
            .push((severity.into(), message.into()));
    }
    fn unique_id(&self) -> String {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        format!("fixture-{id}")
    }
    fn is_worker(&self) -> bool {
        self.worker.get()
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(&self, error: &ExchangeError, context: &Value) {
        self.exceptions
            .borrow_mut()
            .push((error.to_string(), context.clone()));
    }
    fn report_raw_exception(&self, error: &Js, _: &Value) {
        self.raw_exceptions.borrow_mut().push(error.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Ok(Value::Null) })
    }
}

#[test]
fn base_worker_rpc_forwards_arguments_results_errors_and_completion_hook() {
    use safe_cex_exchange::{
        capabilities::ExchangeCapabilities,
        exchange::{BaseExchange, Exchange},
        worker::CallableMethodsInWorker,
    };
    struct Venue {
        base: Rc<BaseExchange>,
        caps: ExchangeCapabilities,
        completed: RefCell<Vec<(Value, Value, Vec<Value>)>>,
    }
    impl Exchange for Venue {
        type PlaceOpts = safe_cex_types::PlaceOrderOpts;
        fn base(&self) -> &BaseExchange {
            &self.base
        }
        fn capabilities(&self) -> &ExchangeCapabilities {
            &self.caps
        }
        fn after_method_called_in_worker(
            &self,
            result: &Value,
            method: CallableMethodsInWorker,
            args: &[Value],
        ) -> Result<(), ExchangeError> {
            self.completed.borrow_mut().push((
                result.clone(),
                serde_json::to_value(method).unwrap(),
                args.to_vec(),
            ));
            Ok(())
        }
    }
    for success in [true, false] {
        let assembled = assemble(json!({"key":"k","secret":"s"})).unwrap();
        assembled.runtime.hold_sleep.set(true);
        let venue = Rc::new(Venue {
            base: assembled.base.clone(),
            caps: Default::default(),
            completed: RefCell::new(vec![]),
        });
        let dynamic: Rc<dyn Exchange> = venue.clone();
        *assembled.context.venue.borrow_mut() = Some(Rc::downgrade(&dynamic));
        let requests = Rc::new(RefCell::new(vec![]));
        let captured = requests.clone();
        assembled.base.on(
            "callMethod",
            Rc::new(move |_, request| {
                captured.borrow_mut().push(request.clone());
                Ok(())
            }),
        );
        let promise = assembled
            .this
            .call_method(
                "callMethodInWorker",
                vec![Js::str("removeOrders"), worker_order_ids(), Js::Number(17.)],
            )
            .expect("shared worker RPC is installed");
        assert_eq!(
            requests.borrow().len(),
            1,
            "source async prefix emits one worker request"
        );
        let request = to_json(&requests.borrow()[0]).unwrap();
        assert_eq!(request["method"], json!("removeOrders"));
        assert_eq!(request["args"], json!([[{"id":"wire-7"}],17.0]));
        let result = if success {
            json!({"removed":["wire-7"],"version":23.0})
        } else {
            json!({"name":"WorkerFailure","message":"worker rejected","code":409,"stack":"worker stack"})
        };
        assembled
            .base
            .emit(
                "callMethod:done",
                safe_cex_exchange::js_runtime::value::from_json(
                    &json!({"requestId":request["requestId"],"success":success,"result":result}),
                ),
            )
            .unwrap();
        let reply = settle(&assembled.runtime, promise);
        if success {
            assert_eq!(to_json(&reply.unwrap()).unwrap(), result);
            assert_eq!(
                venue.completed.borrow().as_slice(),
                &[(
                    result,
                    json!("removeOrders"),
                    vec![json!([{"id":"wire-7"}]), json!(17.0)]
                )]
            );
        } else {
            let error = reply.unwrap_err();
            assert_eq!(
                error.value.get("name").unwrap().to_display(),
                "WorkerFailure"
            );
            assert_eq!(
                to_json(&error.value.get("code").unwrap()).unwrap(),
                json!(409.0)
            );
            assert_eq!(
                error.value.get("stack").unwrap().to_display(),
                "worker stack"
            );
            assert_eq!(error.to_exchange_error().message, "worker rejected");
            assert!(venue.completed.borrow().is_empty());
        }
    }
    // The default test clock resolves its sleep immediately: the facade must
    // preserve the existing core timeout rather than leave an unsettled promise.
    let assembled = assemble(json!({"key":"k","secret":"s"})).unwrap();
    let venue: Rc<dyn Exchange> = Rc::new(Venue {
        base: assembled.base.clone(),
        caps: Default::default(),
        completed: RefCell::new(vec![]),
    });
    *assembled.context.venue.borrow_mut() = Some(Rc::downgrade(&venue));
    let promise = assembled
        .this
        .call_method(
            "callMethodInWorker",
            vec![Js::str("removeOrders"), worker_order_ids()],
        )
        .unwrap();
    assert_eq!(
        settle(&assembled.runtime, promise).unwrap_err().message(),
        "Timeout callMethodInWorker"
    );
}

fn worker_order_ids() -> Js {
    safe_cex_exchange::js_runtime::value::from_json(&json!([{"id":"wire-7"}]))
}

struct Http {
    runtime: Rc<Runtime>,
    sent: RefCell<Vec<Value>>,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        self.sent.borrow_mut().push(to_json(&config).unwrap());
        let status = if config.get("url").unwrap().to_display() == "/fail" {
            500.0
        } else {
            200.0
        };
        Box::pin(async move {
            Ok(Js::object(vec![
                ("status", Js::Number(status)),
                ("data", Js::object(vec![("ok", Js::Bool(true))])),
            ]))
        })
    }
}

fn settle(runtime: &Runtime, value: Js) -> Result<Js, Thrown> {
    let mut future = Box::pin(await_js(value));
    for _ in 0..16 {
        if let Poll::Ready(v) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return v;
        }
        let jobs = std::mem::take(&mut *runtime.jobs.borrow_mut());
        for mut job in jobs {
            if job
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                runtime.jobs.borrow_mut().push(job);
            }
        }
    }
    panic!("promise did not settle")
}

#[test]
fn client_retry_preserves_the_failed_config_and_retry_stamp() {
    let runtime = Rc::new(Runtime::default());
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
    });
    let xhr = client::create(http.clone(), Js::object(vec![]));
    xhr.call_method(
        "__configureRetry",
        vec![Js::object(vec![
            ("retries", Js::Number(3.0)),
            (
                "retryCondition",
                Js::function("retryCondition", |_, a| {
                    let config = arg(&a, 0).get("config")?;
                    if config.get("signatureRetried")?.truthy() {
                        return Ok(Js::Bool(false));
                    }
                    config.set("signatureRetried", Js::Bool(true))?;
                    config.set("data", Js::str("replayed body"))?;
                    Ok(Js::Bool(true))
                }),
            ),
        ])],
    )
    .unwrap();
    let result = settle(
        &runtime,
        xhr.call_method("post", vec![Js::str("/fail"), Js::str("original body")])
            .unwrap(),
    );
    assert!(result.is_err());
    let sent = http.sent.borrow();
    assert_eq!(
        sent.len(),
        2,
        "retry predicate stamp survives the next attempt"
    );
    assert_eq!(sent[0]["data"], "original body");
    assert_eq!(sent[1]["data"], "replayed body");
    assert_eq!(sent[1]["signatureRetried"], true);
}

#[test]
fn client_headers_support_source_set_get_has_without_wire_methods() {
    let runtime = Rc::new(Runtime::default());
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
    });
    let xhr = client::create(
        http.clone(),
        Js::object(vec![(
            "headers",
            Js::object(vec![
                ("X-Trace", Js::str("default")),
                ("Content-Type", Js::str("application/json")),
            ]),
        )]),
    );
    xhr.get("interceptors")
        .unwrap()
        .get("request")
        .unwrap()
        .call_method(
            "use",
            vec![Js::function("source headers", |_, a| {
                let config = arg(&a, 0);
                let headers = config.get("headers")?;
                assert!(
                    headers.get("set")?.is_callable(),
                    "source-interceptor headers.set must be callable"
                );
                assert!(
                    !headers.has_own("set"),
                    "AxiosHeaders methods are inherited"
                );
                assert_eq!(
                    headers
                        .call_method("get", vec![Js::str("x-trace")])?
                        .to_display(),
                    "request",
                    "request header wins case-insensitively"
                );
                let returned = headers
                    .call_method("set", vec![Js::str("POLY_SIGNATURE"), Js::str("signed")])?;
                assert!(returned.same_reference(&headers), "headers.set is fluent");
                headers.call_method("set", vec![Js::str("poly_signature"), Js::str("updated")])?;
                assert_eq!(
                    headers
                        .call_method("get", vec![Js::str("Poly_Signature")])?
                        .to_display(),
                    "updated"
                );
                assert!(
                    headers
                        .call_method("has", vec![Js::str("POLY_SIGNATURE")])?
                        .truthy()
                );
                assert!(
                    !headers
                        .call_method("has", vec![Js::str("absent")])?
                        .truthy()
                );
                assert_eq!(
                    headers
                        .own_keys()
                        .iter()
                        .map(|k| String::from_utf16(k).unwrap())
                        .collect::<Vec<_>>(),
                    vec!["X-Trace", "Content-Type", "POLY_SIGNATURE"],
                    "only header entries reach the wire"
                );
                Ok(config)
            })],
        )
        .unwrap();
    settle(
        &runtime,
        xhr.call_method(
            "get",
            vec![
                Js::str("/ok"),
                Js::object(vec![(
                    "headers",
                    Js::object(vec![("x-trace", Js::str("request"))]),
                )]),
            ],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        http.sent.borrow()[0]["headers"],
        json!({"X-Trace":"request","Content-Type":"application/json","POLY_SIGNATURE":"updated"}),
        "wire headers contain data only"
    );
}

#[test]
fn client_header_facade_matches_installed_axios() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_headers_oracle.cjs"
        ))
        .output()
        .expect("Axios oracle launches");
    assert!(
        output.status.success(),
        "Axios oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = capture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4, "independent header corpus floor");
    let mut compared = 0;
    for case in cases {
        let runtime = Rc::new(Runtime::default());
        let http = Rc::new(Http {
            runtime: runtime.clone(),
            sent: RefCell::default(),
        });
        let xhr = client::create(
            http,
            Js::object(vec![(
                "headers",
                safe_cex_exchange::js_runtime::value::from_json(&case["defaults"]),
            )]),
        );
        let source = case.clone();
        let checked = Rc::new(std::cell::Cell::new(0));
        let performed = checked.clone();
        xhr.get("interceptors")
            .unwrap()
            .get("request")
            .unwrap()
            .call_method(
                "use",
                vec![Js::function("compare source headers", move |_, a| {
                    let config = arg(&a, 0);
                    let headers = config.get("headers")?;
                    for (sets, expected) in source["sets"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .zip(source["snapshots"].as_array().unwrap())
                    {
                        let args = sets
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(safe_cex_exchange::js_runtime::value::from_json)
                            .collect();
                        assert!(
                            headers.call_method("set", args)?.same_reference(&headers),
                            "source fluent receiver"
                        );
                        let entries: Vec<_> = headers
                            .own_entries()?
                            .into_iter()
                            .map(|(k, v)| {
                                json!([String::from_utf16(&k).unwrap(), to_json(&v).unwrap()])
                            })
                            .collect();
                        assert_eq!(
                            json!(entries),
                            expected["entries"],
                            "actual Axios native entries"
                        );
                        assert_eq!(
                            json!(
                                headers
                                    .own_keys()
                                    .iter()
                                    .map(|k| String::from_utf16(k).unwrap())
                                    .collect::<Vec<_>>()
                            ),
                            expected["keys"],
                            "actual Axios own-key order"
                        );
                        for probe in expected["probes"].as_array().unwrap() {
                            let name = Js::str(probe["name"].as_str().unwrap());
                            let value = headers.call_method("get", vec![name.clone()])?;
                            assert_eq!(
                                headers.call_method("has", vec![name])?.truthy(),
                                probe["present"].as_bool().unwrap(),
                                "actual Axios has"
                            );
                            if let Some(expected) = probe.get("value") {
                                assert_eq!(to_json(&value)?, *expected, "actual Axios get");
                            } else {
                                assert!(value.is_undefined(), "absent Axios get stays undefined");
                            }
                        }
                        performed.set(performed.get() + 1);
                    }
                    Ok(config)
                })],
            )
            .unwrap();
        settle(
            &runtime,
            xhr.call_method(
                "get",
                vec![
                    Js::str("/ok"),
                    Js::object(vec![(
                        "headers",
                        safe_cex_exchange::js_runtime::value::from_json(&case["request"]),
                    )]),
                ],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            checked.get(),
            case["sets"].as_array().unwrap().len(),
            "no skipped source operations"
        );
        compared += checked.get();
    }
    assert_eq!(compared, 17, "independent performed operation count");
}

#[test]
fn client_runs_request_interceptors_lifo_and_response_interceptors_fifo() {
    let runtime = Rc::new(Runtime::default());
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
    });
    let order = Rc::new(RefCell::new(vec![]));
    let xhr = client::create(
        http.clone(),
        Js::object(vec![("headers", Js::object(vec![("A", Js::str("1"))]))]),
    );
    let interceptors = xhr.get("interceptors").unwrap();
    for tag in ["first", "second"] {
        let request_order = order.clone();
        interceptors
            .get("request")
            .unwrap()
            .call_method(
                "use",
                vec![Js::function("", move |_, a| {
                    request_order.borrow_mut().push(format!("request {tag}"));
                    let config = arg(&a, 0);
                    config.get("headers")?.set(tag, Js::str(tag))?;
                    Ok(config)
                })],
            )
            .unwrap();
        let response_order = order.clone();
        interceptors
            .get("response")
            .unwrap()
            .call_method(
                "use",
                vec![Js::function("", move |_, a| {
                    response_order.borrow_mut().push(format!("response {tag}"));
                    Ok(arg(&a, 0))
                })],
            )
            .unwrap();
    }
    let response = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/ok")]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        to_json(&response.get("data").unwrap()).unwrap(),
        json!({"ok": true})
    );
    assert_eq!(
        *order.borrow(),
        [
            "request second",
            "request first",
            "response first",
            "response second"
        ]
    );
    let sent = http.sent.borrow()[0].clone();
    assert_eq!(
        sent["headers"],
        json!({"A": "1", "second": "second", "first": "first"})
    );

    let failure = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/fail")]).unwrap(),
    )
    .unwrap_err();
    assert_eq!(failure.message(), "Request failed with status code 500");
    assert_eq!(
        to_json(
            &failure
                .value
                .get("response")
                .unwrap()
                .get("status")
                .unwrap()
        )
        .unwrap(),
        json!(500.0)
    );
}

#[test]
fn promise_bodies_start_eagerly_and_settle_through_the_runtime() {
    let runtime: Rc<dyn ExchangeRuntime> = Rc::new(Runtime::default());
    let started = Rc::new(RefCell::new(false));
    let flag = started.clone();
    let promise = Promise::spawn(
        &runtime,
        Box::pin(async move {
            *flag.borrow_mut() = true;
            Ok(Js::Number(1.0))
        }),
    );
    assert!(*started.borrow(), "body ran before anyone awaited it");
    assert!(
        promise
            .result()
            .is_some_and(|r| r.unwrap().strict_equals(&Js::Number(1.0)))
    );
}

#[test]
fn bigint_and_url_search_params_follow_ecmascript() {
    let big = Js::BigInt(Rc::new(12_345_678_901_234_567_890_u128.into()));
    assert_eq!(big.type_of(), "bigint");
    assert!(big.truthy());
    assert!(!Js::BigInt(Rc::new(0.into())).truthy());
    assert_eq!(big.to_display(), "12345678901234567890");
    assert!(big.strict_equals(&Js::BigInt(Rc::new(12_345_678_901_234_567_890_u128.into()))));
    assert_eq!(
        big.to_number().unwrap_err().message(),
        "Cannot convert a BigInt value to a number"
    );
    assert_eq!(to_json(&big).unwrap(), json!("12345678901234567890"));

    let globals = Js::object(vec![]);
    safe_cex_exchange::js_runtime::globals::install_builtins::<()>(&globals, std::rc::Weak::new());
    let params = globals
        .get("URLSearchParams")
        .unwrap()
        .call(
            Js::Undefined,
            vec![Js::object(vec![
                ("a b", Js::str("x y&z")),
                ("k", Js::str("~*")),
            ])],
        )
        .unwrap();
    assert_eq!(
        params.call_method("toString", vec![]).unwrap().to_display(),
        "a+b=x+y%26z&k=%7E*"
    );
}

#[test]
fn operators_follow_ecmascript_and_refuse_bigint_mixing() {
    use safe_cex_exchange::js_runtime::ops;
    let big = |n: i64| Js::BigInt(Rc::new(n.into()));
    assert_eq!(
        ops::add(&Js::str("a"), &Js::Number(1.0))
            .unwrap()
            .to_display(),
        "a1"
    );
    assert_eq!(
        ops::add(&Js::Number(1.0), &Js::Bool(true))
            .unwrap()
            .to_display(),
        "2"
    );
    assert!(ops::add(&big(2), &big(3)).unwrap().strict_equals(&big(5)));
    assert_eq!(ops::add(&big(2), &Js::str("x")).unwrap().to_display(), "2x");
    assert_eq!(
        ops::multiply(&big(2), &Js::Number(3.0))
            .unwrap_err()
            .message(),
        "Cannot mix BigInt and other types, use explicit conversions"
    );
    assert!(
        ops::subtract(&Js::str("5"), &Js::Number(2.0))
            .unwrap()
            .strict_equals(&Js::Number(3.0))
    );
    assert!(ops::less_than(&Js::str("10"), &Js::str("9")).unwrap());
    assert!(!ops::less_than(&Js::str("10"), &Js::Number(9.0)).unwrap());
    assert!(ops::less_than(&big(1), &Js::Number(1.5)).unwrap());
    assert!(!ops::less_equal(&Js::Number(f64::NAN), &Js::Number(1.0)).unwrap());
    assert!(ops::greater_equal(&big(2), &Js::str("2")).unwrap());
    // Exact BigInt ordering beyond 2^53 and StringToBigInt radix literals.
    let beyond = Js::BigInt(Rc::new("9007199254740993".parse().unwrap()));
    assert!(ops::less_than(&beyond, &Js::Number(9007199254740994.0)).unwrap());
    assert!(ops::greater_than(&beyond, &Js::Number(9007199254740992.0)).unwrap());
    assert!(ops::less_than(&big(1), &Js::Number(1.5)).unwrap());
    assert!(!ops::less_than(&big(2), &Js::Number(1.5)).unwrap());
    assert!(ops::less_than(&big(15), &Js::str("0x10")).unwrap());
    assert!(!ops::less_than(&big(1), &Js::str("1.5")).unwrap());
    assert!(!ops::greater_equal(&big(1), &Js::str("1.5")).unwrap());
    // OrdinaryToPrimitive consults an own valueOf.
    let boxed = Js::object(vec![(
        "valueOf",
        Js::function("valueOf", |_, _| Ok(Js::Number(7.0))),
    )]);
    assert!(
        ops::multiply(&boxed, &Js::Number(2.0))
            .unwrap()
            .strict_equals(&Js::Number(14.0))
    );
    // Remainder and unary minus.
    assert!(
        ops::remainder(&Js::Number(-7.0), &Js::Number(2.0))
            .unwrap()
            .strict_equals(&Js::Number(-1.0))
    );
    assert!(
        ops::remainder(&big(-7), &big(2))
            .unwrap()
            .strict_equals(&big(-1))
    );
    assert_eq!(
        ops::remainder(&big(1), &big(0)).unwrap_err().message(),
        "Division by zero"
    );
    let negative_zero = ops::negate(&Js::Number(0.0)).unwrap().as_f64().unwrap();
    assert!(negative_zero == 0.0 && negative_zero.is_sign_negative());
    assert!(ops::negate(&big(3)).unwrap().strict_equals(&big(-3)));
}

#[test]
fn set_refuses_writes_it_cannot_store_instead_of_dropping_them() {
    let array = Js::array(vec![]);
    array.set("0", Js::Number(1.0)).unwrap();
    assert!(array.set("extra", Js::Number(1.0)).is_err());
    assert_eq!(
        Js::str("abc")
            .set("x", Js::Number(1.0))
            .unwrap_err()
            .message(),
        "Cannot create property 'x' on string 'abc'"
    );
}

#[test]
fn json_stringify_prints_numbers_and_escapes_like_javascript() {
    let globals = Js::object(vec![]);
    safe_cex_exchange::js_runtime::globals::install_builtins::<()>(&globals, std::rc::Weak::new());
    let stringify = |v: Js| {
        globals
            .get("JSON")
            .unwrap()
            .call_method("stringify", vec![v])
            .unwrap()
            .to_display()
    };
    let body = Js::object(vec![
        ("qty", Js::Number(1.0)),
        ("px", Js::Number(0.1)),
        ("big", Js::Number(1e21)),
        ("nan", Js::Number(f64::NAN)),
        ("skip", Js::Undefined),
        ("s", Js::str("a\"\n")),
        ("list", Js::array(vec![Js::Undefined, Js::Number(-0.0)])),
    ]);
    assert_eq!(
        stringify(body),
        r#"{"qty":1,"px":0.1,"big":1e+21,"nan":null,"s":"a\"\n","list":[null,0]}"#
    );
}

#[test]
fn function_bind_retains_receiver_and_prepends_arguments_for_array_map() {
    let receiver = Js::object(vec![("offset", Js::Number(10.0))]);
    let expected_receiver = receiver.clone();
    let mapper = Js::function("mapPosition", move |this, args| {
        assert!(this.same_reference(&expected_receiver));
        Ok(Js::Number(
            this.get("offset")?.to_number()?
                + arg(&args, 0).to_number()?
                + arg(&args, 1).to_number()?,
        ))
    });
    let bound = mapper
        .call_method("bind", vec![receiver.clone(), Js::Number(100.0)])
        .unwrap();
    receiver.set("offset", Js::Number(20.0)).unwrap();
    let mapped = Js::array(vec![Js::Number(1.0), Js::Number(2.0)])
        .call_method("map", vec![bound.clone()])
        .unwrap();
    assert_eq!(to_json(&mapped).unwrap(), json!([121.0, 122.0]));
    assert_eq!(bound.get("name").unwrap().to_display(), "bound mapPosition");
    let rebound = bound
        .call_method("bind", vec![Js::Null, Js::Number(7.0)])
        .unwrap();
    assert_eq!(
        rebound
            .call(Js::Undefined, vec![])
            .unwrap()
            .to_number()
            .unwrap(),
        127.0
    );
    assert!(!mapper.same_reference(&bound));
}

#[test]
fn function_bind_uses_its_call_receiver_and_preserves_original_errors() {
    let sentinel = Js::object(vec![("failure", Js::Bool(true))]);
    let thrown = sentinel.clone();
    let target = Js::function("throws", move |this, args| {
        assert!(this.is_nullish());
        assert!(arg(&args, 0).is_undefined());
        Err(Thrown::new(thrown.clone()))
    });
    let bind = target.get("bind").unwrap();
    let bound = bind.call(target.clone(), vec![]).unwrap();
    assert!(
        bound
            .call(Js::Bool(true), vec![])
            .unwrap_err()
            .value
            .same_reference(&sentinel)
    );
    assert!(bind.call(Js::Number(1.0), vec![]).is_err());
    target.define("bind", Js::Number(42.0)).unwrap();
    assert_eq!(target.get("bind").unwrap().to_number().unwrap(), 42.0);
}

#[test]
fn json_stringify_callable_replacer_handles_bigint_and_root_replacement() {
    let g = library();
    let json = g.get("JSON").unwrap();
    let big = Js::BigInt(Rc::new(123_u32.into()));
    let replace = Js::function("replacer", |_, args| {
        let value = arg(&args, 1);
        if matches!(value, Js::BigInt(_)) {
            Ok(Js::String(value.to_js_string()?))
        } else if arg(&args, 0).strict_equals(&Js::str("omit")) {
            Ok(Js::Undefined)
        } else {
            Ok(value)
        }
    });
    let value = Js::object(vec![
        ("amount", big.clone()),
        ("omit", Js::Bool(true)),
        ("list", Js::holey_array(vec![None, Some(big.clone())])),
    ]);
    assert!(json.call_method("stringify", vec![value.clone()]).is_err());
    assert_eq!(
        json.call_method("stringify", vec![value, replace.clone()])
            .unwrap()
            .to_display(),
        r#"{"amount":"123","list":[null,"123"]}"#
    );
    assert_eq!(
        json.call_method("stringify", vec![big, replace])
            .unwrap()
            .to_display(),
        "\"123\""
    );
    let from_undefined = Js::function("replaceRoot", |holder, args| {
        assert!(arg(&args, 0).strict_equals(&Js::str("")));
        assert!(holder.has_own(""));
        Ok(Js::Number(7.0))
    });
    assert_eq!(
        json.call_method("stringify", vec![Js::Undefined, from_undefined])
            .unwrap()
            .to_display(),
        "7"
    );
}

#[test]
fn json_stringify_replacer_observes_holders_live_siblings_and_to_json_keys() {
    let g = library();
    let visits = Rc::new(RefCell::new(Vec::new()));
    let sink = visits.clone();
    let nested = Js::object(vec![]);
    nested
        .define(
            "toJSON",
            Js::function("toJSON", |_, args| Ok(arg(&args, 0))),
        )
        .unwrap();
    let input = Js::object(vec![
        ("first", Js::Number(1.0)),
        ("later", Js::Number(2.0)),
        ("nested", nested),
    ]);
    let receiver = input.clone();
    let replace = Js::function("replacer", move |holder, args| {
        let key = arg(&args, 0).to_display();
        sink.borrow_mut().push(key.clone());
        if key == "first" {
            assert!(holder.same_reference(&receiver));
            holder.set("later", Js::Number(9.0))?;
            holder.set("added", Js::Number(10.0))?;
        }
        Ok(arg(&args, 1))
    });
    let result = g
        .get("JSON")
        .unwrap()
        .call_method("stringify", vec![input, replace])
        .unwrap();
    assert_eq!(
        result.to_display(),
        r#"{"first":1,"later":9,"nested":"nested"}"#
    );
    assert_eq!(*visits.borrow(), ["", "first", "later", "nested"]);
}

#[test]
fn json_stringify_replacer_visits_holes_and_preserves_thrown_identity() {
    let g = library();
    let array = Js::holey_array(vec![None, Some(Js::Number(2.0))]);
    let receiver = array.clone();
    let replace = Js::function("replacer", move |holder, args| {
        if arg(&args, 0).strict_equals(&Js::str("0")) {
            assert!(holder.same_reference(&receiver));
            assert!(arg(&args, 1).is_undefined());
            holder.set("1", Js::Number(8.0))?;
            return Ok(Js::Number(3.0));
        }
        Ok(arg(&args, 1))
    });
    assert_eq!(
        g.get("JSON")
            .unwrap()
            .call_method("stringify", vec![array, replace])
            .unwrap()
            .to_display(),
        "[3,8]"
    );
    let sentinel = Js::object(vec![]);
    let thrown = sentinel.clone();
    let replace = Js::function("throws", move |_, _| Err(Thrown::new(thrown.clone())));
    assert!(
        g.get("JSON")
            .unwrap()
            .call_method("stringify", vec![Js::Null, replace])
            .unwrap_err()
            .value
            .same_reference(&sentinel)
    );
}

#[test]
fn json_stringify_replacer_can_remove_cycles_and_repeat_shared_objects() {
    let g = library();
    let cycle = Js::object(vec![]);
    cycle.define("self", cycle.clone()).unwrap();
    let json = g.get("JSON").unwrap();
    let error = json
        .call_method("stringify", vec![cycle.clone()])
        .unwrap_err();
    assert!(error.message().contains("circular structure"));
    let replacer = Js::function("skipSelf", |_, args| {
        if arg(&args, 0).strict_equals(&Js::str("self")) {
            Ok(Js::Undefined)
        } else {
            Ok(arg(&args, 1))
        }
    });
    assert_eq!(
        json.call_method("stringify", vec![cycle.clone(), replacer])
            .unwrap()
            .to_display(),
        "{}"
    );
    cycle.delete(&utf16("self"));
    assert_eq!(
        json.call_method("stringify", vec![Js::array(vec![cycle.clone(), cycle])])
            .unwrap()
            .to_display(),
        "[{},{}]"
    );
}

fn library() -> Js {
    let globals = Js::object(vec![]);
    safe_cex_exchange::js_runtime::globals::install_builtins::<()>(&globals, std::rc::Weak::new());
    safe_cex_exchange::js_runtime::library::install_library::<()>(&globals, std::rc::Weak::new());
    globals
}

fn run(globals: &Js, path: &str, args: Vec<Js>) -> Js {
    let mut parts = path.split('.');
    let mut f = globals.get(parts.next().unwrap()).unwrap();
    for part in parts {
        f = f.get(part).unwrap();
    }
    f.call(Js::Undefined, args).unwrap()
}

fn stringify(globals: &Js, v: Js) -> String {
    let text = run(globals, "JSON.stringify", vec![v]);
    String::from_utf16_lossy(&text.to_js_string().unwrap())
}

#[test]
fn library_lodash_order_by_and_get_key_case_fallbacks() {
    let g = library();
    let row = |a: Js, b: f64| Js::object(vec![("a", a), ("b", Js::Number(b))]);
    let rows = Js::array(vec![
        row(Js::Number(2.0), 1.0),
        row(Js::Undefined, 2.0),
        row(Js::Number(1.0), 3.0),
        row(Js::Number(2.0), 0.0),
    ]);
    let keys = Js::array(vec![Js::str("a"), Js::str("b")]);
    let sorted = run(
        &g,
        "lodash.orderBy",
        vec![rows, keys, Js::array(vec![Js::str("desc"), Js::str("asc")])],
    );
    assert_eq!(
        stringify(&g, sorted),
        r#"[{"b":2},{"a":2,"b":0},{"a":2,"b":1},{"a":1,"b":3}]"#
    );
    let record = Js::object(vec![
        ("order_id", Js::Number(7.0)),
        ("clientOid", Js::Number(8.0)),
    ]);
    assert_eq!(
        stringify(&g, run(&g, "v", vec![record.clone(), Js::str("orderId")])),
        "7"
    );
    assert_eq!(
        stringify(&g, run(&g, "v", vec![record, Js::str("client_oid")])),
        "8"
    );
}

#[test]
fn library_safe_math_and_big_number_are_decimal_exact() {
    let g = library();
    let n = Js::Number;
    assert_eq!(run(&g, "add", vec![n(0.1), n(0.2)]).as_f64(), Some(0.3));
    assert_eq!(
        run(&g, "subtract", vec![n(0.3), n(0.1)]).as_f64(),
        Some(0.2)
    );
    assert_eq!(
        run(&g, "adjust", vec![n(1.23456), Js::str("0.01")]).as_f64(),
        Some(1.23)
    );
    let step = run(&g, "BigNumber", vec![n(0.01)]);
    let lot = run(&g, "BigNumber", vec![n(1000.005)])
        .call_method("dividedToIntegerBy", vec![step.clone()])
        .unwrap()
        .call_method("multipliedBy", vec![step])
        .unwrap();
    assert_eq!(
        stringify(&g, lot.call_method("toFixed", vec![]).unwrap()),
        r#""1000""#
    );
    let half = run(&g, "BigNumber", vec![Js::str("1.005")])
        .call_method("toFixed", vec![n(2.0)])
        .unwrap();
    assert_eq!(stringify(&g, half), r#""1.01""#);
}

#[test]
fn library_dayjs_subtracts_calendar_months_and_minutes() {
    let g = library();
    let jan31 = run(&g, "dayjs", vec![Js::Number(1_706_659_200_000.0)]);
    let feb = jan31
        .call_method("add", vec![Js::Number(1.0), Js::str("M")])
        .unwrap();
    assert_eq!(
        feb.call_method("valueOf", vec![]).unwrap().as_f64(),
        Some(1_709_164_800_000.0)
    );
    let earlier = jan31
        .call_method("subtract", vec![Js::Number(300.0), Js::str("m")])
        .unwrap();
    assert_eq!(
        earlier.call_method("unix", vec![]).unwrap().as_f64(),
        Some(1_706_641_200.0)
    );
    assert!(
        earlier
            .call_method("isBefore", vec![jan31])
            .unwrap()
            .truthy()
    );
    let stamp = run(&g, "dayjs", vec![Js::Number(1_706_659_200_123.0)]);
    let iso = stamp.call_method("toISOString", vec![]).unwrap();
    assert_eq!(stringify(&g, iso), r#""2024-01-31T00:00:00.123Z""#);
    let invalid = run(&g, "dayjs", vec![Js::Null]);
    assert!(invalid.call_method("toISOString", vec![]).is_err());
}

#[test]
fn library_qs_stringify_and_hmac_digest() {
    let g = library();
    let params = Js::object(vec![
        ("instId", Js::str("BTC-USDT-SWAP")),
        ("ids", Js::array(vec![Js::Number(1.0), Js::Number(2.0)])),
        ("skip", Js::Undefined),
        ("note", Js::str("a b")),
    ]);
    let query = run(&g, "qs.stringify", vec![params]);
    assert_eq!(
        stringify(&g, query.clone()),
        r#""instId=BTC-USDT-SWAP&ids%5B0%5D=1&ids%5B1%5D=2&note=a%20b""#
    );
    let decoded = run(&g, "decodeURIComponent", vec![query]);
    assert_eq!(
        stringify(&g, decoded),
        r#""instId=BTC-USDT-SWAP&ids[0]=1&ids[1]=2&note=a b""#
    );
    let digest = run(&g, "createHmac", vec![Js::str("sha256"), Js::str("key")])
        .call_method("update", vec![Js::str("The quick brown fox ")])
        .unwrap()
        .call_method("update", vec![Js::str("jumps over the lazy dog")])
        .unwrap()
        .call_method("digest", vec![Js::str("base64")])
        .unwrap();
    assert_eq!(
        stringify(&g, digest),
        r#""97yD9DBThCSxMpjmqm+xQ+9NWaFJRhdZl0edvC0aPNg=""#
    );
}

fn regex(pattern: &str, flags: &str) -> Js {
    let units = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    safe_cex_exchange::js_runtime::regexp::regexp(&units(pattern), &units(flags)).unwrap()
}

fn string_method(receiver: &str, name: &str, args: Vec<Js>) -> Js {
    Js::str(receiver).call_method(name, args).unwrap()
}

#[test]
fn string_methods_dispatch_a_regexp_argument_to_its_symbol_method() {
    // ts: normalizeSymbolForMatching(symbol).replace(/[-_/]/g, '')
    let replaced = string_method(
        "eth-usd_x/y",
        "replace",
        vec![regex("[-_/]", "g"), Js::str("")],
    );
    assert_eq!(replaced.to_display(), "ethusdxy");
    // Captures are spliced into a regex split, as interval parsing relies on.
    let parts = string_method("1h30m", "split", vec![regex(r"(\d+)", "")]);
    assert_eq!(to_json(&parts).unwrap(), json!(["", "1", "h", "30", "m"]));
    let address = format!("addr 0x{}", "aB".repeat(20));
    let found = string_method(&address, "match", vec![regex("0x[a-fA-F0-9]{40}", "")]);
    assert_eq!(found.get("0").unwrap().to_display(), &address[5..]);
    assert!(matches!(
        string_method("no address", "match", vec![regex("0x[a-f0-9]{40}", "")]),
        Js::Null
    ));
    assert_eq!(
        string_method("abc", "search", vec![regex("c", "")])
            .to_number()
            .unwrap(),
        2.0
    );
    // A plain string pattern keeps the first-occurrence string semantics.
    assert_eq!(
        string_method("a-b-c", "replace", vec![Js::str("-"), Js::str("+")]).to_display(),
        "a+b-c"
    );
    assert_eq!(
        to_json(&string_method("a,b", "split", vec![Js::str(",")])).unwrap(),
        json!(["a", "b"])
    );
}

#[test]
fn chained_rate_limits_hold_a_request_until_every_window_has_room() {
    let runtime = Rc::new(Runtime::default());
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
    });
    let xhr = client::create(http.clone(), Js::object(vec![]));
    for (max, per) in [(4.0, 1000.0), (5.0, 6000.0)] {
        xhr.call_method(
            "__configureRateLimit",
            vec![Js::object(vec![
                ("maxRequests", Js::Number(max)),
                ("perMilliseconds", Js::Number(per)),
            ])],
        )
        .unwrap();
    }
    let mut sent_at = vec![];
    for _ in 0..6 {
        settle(
            &runtime,
            xhr.call_method("get", vec![Js::str("/ok")]).unwrap(),
        )
        .unwrap();
        sent_at.push(runtime.clock.get());
    }
    assert_eq!(sent_at, [0.0, 0.0, 0.0, 0.0, 1000.0, 6000.0]);
}

impl safe_cex_exchange::websocket::WsRuntime for Runtime {
    fn transport(&self) -> std::sync::Arc<dyn safe_cex_transport::TransportProvider> {
        std::sync::Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn safe_cex_transport::socket::TunnelHost> {
        panic!("no tunnel in js_runtime tests")
    }
    fn dial(
        &self,
        url: String,
        protocols: Vec<String>,
        options: safe_cex_exchange::websocket::WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn safe_cex_exchange::websocket::WsSocket>> {
        self.dials.borrow_mut().push((url, protocols, options));
        let socket = self.dial_socket.borrow().clone();
        Box::pin(async move { socket.ok_or_else(|| ExchangeError::new("recorded dial")) })
    }
    fn headers_capable(&self) -> bool {
        self.headers_capable.get()
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

struct Assembled {
    this: Js,
    base: Rc<safe_cex_exchange::exchange::BaseExchange>,
    runtime: Rc<Runtime>,
    context: Rc<safe_cex_exchange::js_runtime::sockets::JsSocketContext>,
}

fn base_exchange(options: Value) -> Result<Js, Thrown> {
    Ok(assemble(options)?.this)
}

fn assemble(options: Value) -> Result<Assembled, Thrown> {
    assemble_with_fields(options, |_| Ok(()))
}

fn assemble_with_fields(
    options: Value,
    initialize_fields: impl FnOnce(&Js) -> Result<(), Thrown>,
) -> Result<Assembled, Thrown> {
    use safe_cex_exchange::{
        client_order_id::pattern_client_order_id_rule,
        exchange::ExchangeSettings,
        js_runtime::{base_exchange, sockets::JsSocketContext, value::from_json},
    };
    let runtime = Rc::new(Runtime::default());
    let context = JsSocketContext::new(runtime.clone());
    let settings = ExchangeSettings {
        client_order_id_rule: Some(pattern_client_order_id_rule(
            "OKX".into(),
            "/^[A-Za-z0-9]{1,32}$/".into(),
            Rc::new(|id: &str| {
                (1..=32).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric())
            }),
            Default::default(),
        )),
        ..Default::default()
    };
    let parts = base_exchange::BaseExchangeParts {
        name: safe_cex_types::ExchangeName::Okx,
        options: serde_json::from_value(options.clone()).unwrap(),
        store: Rc::new(safe_cex_store::store::DefaultStore::new()),
        settings,
        external_public_data: None,
    };
    let (this, base) = base_exchange::create(
        &context,
        parts,
        Js::str("okx"),
        from_json(&options),
        Js::object(vec![]),
        initialize_fields,
    )?;
    Ok(Assembled {
        this,
        base,
        runtime,
        context,
    })
}

#[test]
fn base_exchange_bridges_client_order_ids_algo_derivation_and_tick_interval() {
    use safe_cex_exchange::js_runtime::value::from_json;
    let this = base_exchange(json!({"key": "k", "secret": "s"})).unwrap();
    let order = |id: &str| {
        from_json(&json!({
            "symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": 1, "price": 100,
            "clientOrderId": id,
        }))
    };
    let resolved = this
        .call_method("resolveCallerClientOrderId", vec![order("abc123")])
        .unwrap();
    assert_eq!(to_json(&resolved).unwrap(), json!("abc123"));
    let refused = this.call_method("resolveCallerClientOrderId", vec![order("bad id!")]);
    let refused = refused.unwrap_err().message();
    assert!(refused.contains("OKX"), "{refused}");
    let duplicate = Js::array(vec![order("a1"), order("a1")]);
    assert!(
        this.call_method("assertCallerClientOrderIds", vec![duplicate])
            .is_err()
    );
    let bracket = from_json(&json!({
        "symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": 1, "price": 100,
        "stopLoss": 90,
    }));
    let derived = this
        .call_method(
            "deriveAlgoOrdersFromNormalOrdersOpts",
            vec![Js::array(vec![bracket])],
        )
        .unwrap();
    let derived = to_json(&derived).unwrap();
    assert_eq!(derived[0]["type"], json!("stop_market"));
    assert_eq!(derived[0]["side"], json!("sell"));
    assert_eq!(derived[0]["price"].as_f64(), Some(90.0));
    assert!(
        this.call_method("getTickInterval", vec![])
            .unwrap()
            .is_undefined()
    );
}

#[test]
fn base_exchange_refuses_external_public_data_without_a_provider() {
    let refused = base_exchange(
        json!({"key": "k", "secret": "s", "externalPublicData": {"type": "websocket", "url": "wss://gateway", "exchangeId": "okx"}}),
    );
    assert!(
        refused
            .unwrap_err()
            .message()
            .contains("no external public data provider")
    );
}

#[test]
fn rest_adapter_runs_the_js_client_and_rethrows_its_rejection_unchanged() {
    use safe_cex_transport::{RestAdapter, RestRequest, RestResponseBody};
    let runtime = Rc::new(Runtime::default());
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
    });
    let xhr = client::create(http.clone(), Js::object(vec![]));
    let adapter = Rc::new(client::JsRestAdapter::new(xhr));
    let exchange_runtime: Rc<dyn ExchangeRuntime> = runtime.clone();
    let run = |method: &str, url: &str| {
        let adapter = adapter.clone();
        let request = RestRequest {
            method: method.into(),
            url: url.into(),
            params: Some(json!({"instId": "BTC-USDT-SWAP"})),
            config_extensions: [(
                "grvtCookie".into(),
                safe_cex_types::retained_json::RetainedJsonInput::Text(
                    "gravity=bridge".encode_utf16().collect(),
                ),
            )]
            .into(),
            ..Default::default()
        };
        settle(
            &runtime,
            Js::Promise(Promise::spawn(
                &exchange_runtime,
                Box::pin(async move {
                    match adapter.execute(request).await {
                        Ok(response) => {
                            let RestResponseBody::Untransformed(body) = response.body else {
                                panic!("adapter bodies are untransformed JSON");
                            };
                            Ok(Js::object(vec![
                                ("status", Js::Number(response.status.into())),
                                (
                                    "body",
                                    safe_cex_exchange::js_runtime::value::from_retained_input(body),
                                ),
                            ]))
                        }
                        Err(failure) => Err(adapter.thrown(failure)),
                    }
                }),
            )),
        )
    };
    let ok = to_json(&run("get", "/ok").unwrap()).unwrap();
    assert_eq!(ok, json!({"status": 200.0, "body": {"ok": true}}));
    assert_eq!(
        http.sent.borrow()[0]["params"],
        json!({"instId": "BTC-USDT-SWAP"})
    );
    assert_eq!(
        http.sent.borrow()[0]["grvtCookie"],
        json!("gravity=bridge"),
        "REST CONFIG native-to-JS bridge"
    );
    let upper =
        to_json(&run("GET", "/ok").expect("an uppercase verb dispatches like Axios")).unwrap();
    assert_eq!(upper, ok);
    assert_eq!(http.sent.borrow()[1]["method"], json!("get"));
    let rejected = run("get", "/fail").unwrap_err();
    assert_eq!(
        to_json(
            &rejected
                .value
                .get("response")
                .unwrap()
                .get("status")
                .unwrap()
        )
        .unwrap(),
        json!(500.0)
    );
}

#[test]
fn to_js_json_keeps_undefined_and_bigint_and_ordered_fields() {
    use safe_cex_exchange::js_runtime::value::to_js_json;
    use safe_cex_num::JsJsonValue as J;
    let value = Js::object(vec![
        ("b", Js::Number(1.0)),
        ("a", Js::Undefined),
        ("n", Js::BigInt(Rc::new(7.into()))),
        ("xs", Js::array(vec![Js::Null, Js::str("s")])),
    ]);
    assert_eq!(
        to_js_json(&value).unwrap(),
        J::Object(vec![
            ("b".into(), J::Number(1.0)),
            ("a".into(), J::Undefined),
            ("n".into(), J::BigInt(7.into())),
            ("xs".into(), J::Array(vec![J::Null, J::String("s".into())])),
        ])
    );
}

#[test]
fn ws_direct_fallback_moves_to_the_next_host_after_silent_connects() {
    let g = library();
    let fallback = run(&g, "WsDirectFallback", vec![Js::Number(1.0)]);
    let resolve = |f: &Js| {
        let url = f
            .call_method(
                "resolveUrl",
                vec![
                    Js::str("wss://primary"),
                    Js::Undefined,
                    Js::str("wss://backup"),
                ],
            )
            .unwrap();
        String::from_utf16_lossy(&url.to_js_string().unwrap())
    };
    assert_eq!(resolve(&fallback), "wss://primary");
    fallback.call_method("onMessage", vec![]).unwrap();
    assert_eq!(resolve(&fallback), "wss://primary");
    assert_eq!(resolve(&fallback), "wss://backup");
}

#[test]
fn emitter_on_and_off_follow_listener_identity() {
    let this = base_exchange(json!({"key": "k", "secret": "s"})).unwrap();
    let emitter = this.get("emitter").unwrap();
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    let listener = Js::function("listener", move |_, a| {
        sink.borrow_mut().push(to_json(&Js::array(a)).unwrap());
        Ok(Js::Undefined)
    });
    let other = Js::function("other", |_, _| Ok(Js::Undefined));
    // `this.on/off` are the emitter's own members bound to it (base.ts:454).
    this.call_method("on", vec![Js::str("markets:changed"), listener.clone()])
        .unwrap();
    emitter
        .call_method("emit", vec![Js::str("markets:changed"), Js::Number(1.0)])
        .unwrap();
    emitter
        .call_method("off", vec![Js::str("markets:changed"), other])
        .unwrap();
    emitter
        .call_method("emit", vec![Js::str("markets:changed"), Js::Number(2.0)])
        .unwrap();
    this.call_method("off", vec![Js::str("markets:changed"), listener])
        .unwrap();
    emitter
        .call_method("emit", vec![Js::str("markets:changed"), Js::Number(3.0)])
        .unwrap();
    assert_eq!(*seen.borrow(), vec![json!([1.0]), json!([2.0])]);
}

/// A terminal RestAdapter that records requests and answers by url.
#[derive(Default)]
struct Terminal {
    seen: std::sync::Mutex<Vec<safe_cex_transport::RestRequest>>,
}
impl safe_cex_transport::RestAdapter for Terminal {
    fn execute(
        &self,
        request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        use safe_cex_transport::{RestFailure, RestResponse, RestResponseBody};
        self.seen.lock().unwrap().push(request.clone());
        let response = |status: u16, body: &str| RestResponse {
            status,
            reason: None,
            headers: [("content-type".to_owned(), "application/json".to_owned())].into(),
            body: RestResponseBody::Encoded(body.as_bytes().to_vec()),
        };
        let result = match request.url.as_str() {
            "/large-id" => Ok(response(200, r#"{ "orderId":9007199254740993 }"#)),
            "/down" => {
                let mut failure = RestFailure::pre_transmit("socket hang up".into(), request);
                failure.code = Some("ECONNRESET".into());
                Err(failure)
            }
            "/fail" => {
                let mut failure = RestFailure::pre_transmit("status 500".into(), request);
                failure.response = Some(Box::new(response(500, r#"{"code":"1"}"#)));
                Err(failure)
            }
            _ => Ok(response(200, r#"{"code":"0","data":[1]}"#)),
        };
        Box::pin(async move { result })
    }
}

#[test]
fn rest_http_host_carries_config_to_the_terminal_adapter() {
    use safe_cex_transport::RestBody;
    let runtime = Rc::new(Runtime::default());
    let terminal = std::sync::Arc::new(Terminal::default());
    let host = client::rest_http_host(runtime.clone(), terminal.clone());
    let xhr = client::create(
        host,
        Js::object(vec![
            ("baseURL", Js::str("https://www.okx.com")),
            ("timeout", Js::Number(20000.0)),
            ("headers", Js::object(vec![("X-A", Js::str("1"))])),
        ]),
    );
    let params = Js::object(vec![("instId", Js::str("BTC-USDT-SWAP"))]);
    let response = settle(
        &runtime,
        xhr.call_method(
            "get",
            vec![Js::str("/ok"), Js::object(vec![("params", params.clone())])],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        to_json(&response.get("data").unwrap()).unwrap(),
        json!({"code": "0", "data": [1.0]})
    );
    let posted = settle(
        &runtime,
        xhr.call_method("post", vec![Js::str("/ok"), Js::str(r#"{"a":1}"#)])
            .unwrap(),
    );
    assert!(posted.is_ok());
    {
        let seen = terminal.seen.lock().unwrap();
        assert_eq!(seen[0].method, "get");
        assert_eq!(seen[0].base_url.as_deref(), Some("https://www.okx.com"));
        // query_in_url marks this adapter as a physical URL-consuming terminal.
        assert_eq!(seen[0].url, "/ok?instId=BTC-USDT-SWAP");
        assert_eq!(seen[0].params, None);
        assert_eq!(seen[0].timeout_ms, Some(20000));
        assert_eq!(seen[0].headers.get("X-A").map(String::as_str), Some("1"));
        assert_eq!(seen[1].method, "post");
        assert_eq!(
            seen[1].body,
            Some(RestBody::Encoded(br#"{"a":1}"#.to_vec()))
        );
    }
    let failed = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/fail")]).unwrap(),
    )
    .unwrap_err();
    let status = failed.value.get("response").unwrap().get("status").unwrap();
    assert_eq!(status.as_f64(), Some(500.0));
    let down = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/down")]).unwrap(),
    )
    .unwrap_err();
    assert!(down.value.get("response").unwrap().is_undefined());
    assert_eq!(
        to_json(&down.value.get("code").unwrap()).unwrap(),
        json!("ECONNRESET")
    );
}

#[test]
fn rest_http_host_text_response_preserves_raw_big_integer_tokens() {
    let runtime = Rc::new(Runtime::default());
    let terminal = std::sync::Arc::new(Terminal::default());
    let host = client::rest_http_host(runtime.clone(), terminal);
    let xhr = client::create(host, Js::object(vec![("responseType", Js::str("text"))]));
    let response = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/large-id")]).unwrap(),
    )
    .unwrap();
    let raw = response.get("data").unwrap();
    assert!(
        raw.is_string(),
        "text response must reach the venue before numeric projection"
    );
    assert_eq!(raw.to_display(), r#"{ "orderId":9007199254740993 }"#);
    assert_eq!(
        safe_cex_transport::how::big_int_as_string_json_parse(&raw.to_display())
            .into_value()
            .unwrap()["orderId"],
        "9007199254740993"
    );
    let failed = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/fail")]).unwrap(),
    )
    .unwrap_err();
    assert_eq!(
        failed
            .value
            .get("response")
            .unwrap()
            .get("data")
            .unwrap()
            .to_display(),
        r#"{"code":"1"}"#
    );
}

#[test]
fn rest_http_host_lets_the_venue_serialize_its_query() {
    let runtime = Rc::new(Runtime::default());
    let terminal = std::sync::Arc::new(Terminal::default());
    let host = client::rest_http_host(runtime.clone(), terminal.clone());
    let sorted = Js::function("paramsSerializer", |_, a| {
        let params = arg(&a, 0);
        let mut keys: Vec<String> = params
            .own_keys()
            .iter()
            .map(|k| String::from_utf16_lossy(k))
            .collect();
        keys.sort();
        let pairs: Vec<String> = keys
            .iter()
            .map(|k| format!("{k}={}", params.get(k).unwrap().to_display()))
            .collect();
        Ok(Js::str(&pairs.join("&")))
    });
    let xhr = client::create(host, Js::object(vec![("paramsSerializer", sorted)]));
    let params = Js::object(vec![("b", Js::Number(2.0)), ("a", Js::Number(1.0))]);
    settle(
        &runtime,
        xhr.call_method(
            "get",
            vec![Js::str("/ok"), Js::object(vec![("params", params)])],
        )
        .unwrap(),
    )
    .unwrap();
    let seen = terminal.seen.lock().unwrap();
    assert_eq!(seen[0].url, "/ok?a=1&b=2");
    assert_eq!(seen[0].params, None);
}

/// A dropped awaiter releases its waker at once; the promise still settles for the
/// awaiters that remain.
#[test]
fn dropped_promise_wait_releases_its_waker() {
    #[derive(Default)]
    struct Count(std::sync::atomic::AtomicBool);
    impl std::task::Wake for Count {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let runtime: Rc<dyn ExchangeRuntime> = Rc::new(Runtime::default());
    let promise = Promise::pending(&runtime);
    let (dropped, kept) = (
        std::sync::Arc::<Count>::default(),
        std::sync::Arc::<Count>::default(),
    );
    let mut first = Box::pin(promise.wait());
    let mut second = Box::pin(promise.wait());
    for (wait, waker) in [(&mut first, &dropped), (&mut second, &kept)] {
        let waker = Waker::from(waker.clone());
        assert!(
            wait.as_mut()
                .poll(&mut Context::from_waker(&waker))
                .is_pending()
        );
    }
    assert_eq!(
        std::sync::Arc::strong_count(&dropped),
        2,
        "registered while pending"
    );
    drop(first);
    assert_eq!(
        std::sync::Arc::strong_count(&dropped),
        1,
        "WAITER drop releases the waker"
    );
    assert_eq!(
        std::sync::Arc::strong_count(&kept),
        2,
        "the other awaiter stays registered"
    );
    promise.settle(Ok(Js::Bool(true)));
    assert_eq!(
        std::sync::Arc::strong_count(&kept),
        1,
        "settlement wakes and releases"
    );
    assert!(
        kept.0.load(std::sync::atomic::Ordering::SeqCst),
        "the kept awaiter is woken"
    );
    assert!(
        !dropped.0.load(std::sync::atomic::Ordering::SeqCst),
        "the dropped one is not"
    );
    let waker = Waker::from(kept.clone());
    assert!(matches!(
        second.as_mut().poll(&mut Context::from_waker(&waker)),
        Poll::Ready(Ok(Js::Bool(true)))
    ));
}

/// clearTimeout cancels the host task, and a fired timer leaves the timer list.
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
#[test]
fn clear_timeout_cancels_and_fired_timers_are_released() {
    use safe_cex_exchange::js_runtime::scope::{Scope, ScopeShared};
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let runtime = safe_cex_exchange::native::native_runtime();
        let http = Rc::new(Http {
            runtime: Rc::new(Runtime::default()),
            sent: RefCell::default(),
        });
        let store: Rc<dyn safe_cex_store::Store> =
            Rc::new(safe_cex_store::store::DefaultStore::new());
        let scope = Scope {
            shared: Rc::new(ScopeShared::new(
                runtime.clone(),
                http,
                store,
                Js::Undefined,
                (),
            )),
            this: Js::Undefined,
        };
        let ran = Rc::new(std::cell::Cell::new(0));
        let (cleared, kept) = (ran.clone(), ran.clone());
        let timer = scope.set_timeout(
            Rc::new(move || {
                cleared.set(cleared.get() + 10);
                Ok(())
            }),
            20.0,
        );
        scope.set_timeout(
            Rc::new(move || {
                kept.set(kept.get() + 1);
                Ok(())
            }),
            20.0,
        );
        assert_eq!(scope.shared.timers.borrow().len(), 2);
        scope.clear_timeout(&timer);
        assert_eq!(scope.shared.timers.borrow().len(), 1);
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert_eq!(
            ran.get(),
            1,
            "the cleared timer never fires; the other fires once"
        );
        assert!(
            scope.shared.timers.borrow().is_empty(),
            "a fired timer is released"
        );
    });
}

#[test]
fn store_facade_mutators_key_on_the_entity_id() {
    use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
    let store = Rc::new(DefaultStore::new());
    Store::update(
        &*store,
        StoreUpdate {
            tickers: Some(vec![
                serde_json::from_value(json!({
                    "id": "BTCUSDT", "symbol": "BTCUSDT", "bid": 1, "ask": 2, "last": 3,
                    "mark": 4, "index": 5, "percentage": 0, "openInterest": 0,
                    "fundingRate": 0, "fundingTime": 0, "volume": 0, "quoteVolume": 0
                }))
                .unwrap(),
            ]),
            ..StoreUpdate::default()
        },
        false,
    )
    .unwrap();
    let facade = safe_cex_exchange::js_runtime::store::facade(store.clone());
    // ts: store.updateTicker(ticker, changes) receives the ticker object.
    let ticker = Js::object(vec![("id", Js::str("BTCUSDT"))]);
    facade
        .call_method(
            "updateTicker",
            vec![ticker, Js::object(vec![("fundingRate", Js::Number(0.25))])],
        )
        .unwrap();
    assert_eq!(store.tickers()[0].funding_rate, 0.25);
}

/// Builtins installed over a live scope, so runtime-backed members reach `runtime`.
fn scoped_globals() -> (
    Rc<Runtime>,
    Rc<safe_cex_exchange::js_runtime::scope::ScopeShared<()>>,
) {
    use safe_cex_exchange::js_runtime::{globals, library, scope::ScopeShared};
    let runtime = Rc::new(Runtime::default());
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
    });
    let shared = Rc::new(ScopeShared::new(
        runtime.clone(),
        http,
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        Js::object(vec![]),
        (),
    ));
    globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
    library::install_library(&shared.globals, Rc::downgrade(&shared));
    (runtime, shared)
}

fn json_of(g: &Js, v: Js) -> String {
    stringify(g, v)
}

fn thrown_message(result: Result<Js, Thrown>) -> String {
    result.unwrap_err().message()
}

#[test]
fn array_flat_skips_holes_and_honours_depth() {
    let g = library();
    let n = Js::Number;
    let nested = || {
        Js::holey_array(vec![
            Some(n(1.0)),
            Some(Js::array(vec![
                n(2.0),
                Js::array(vec![n(3.0), Js::array(vec![n(4.0)])]),
            ])),
            None,
            Some(n(5.0)),
        ])
    };
    let flat = |v: Js, args: Vec<Js>| json_of(&g, v.call_method("flat", args).unwrap());
    assert_eq!(flat(nested(), vec![]), "[1,2,[3,[4]],5]");
    assert_eq!(flat(nested(), vec![n(f64::INFINITY)]), "[1,2,3,4,5]");
    assert_eq!(flat(nested(), vec![Js::str("2")]), "[1,2,3,[4],5]");
    // Depth 0 and negative depths flatten nothing but still drop holes.
    assert_eq!(flat(nested(), vec![n(0.0)]), "[1,[2,[3,[4]]],5]");
    assert_eq!(flat(nested(), vec![n(-1.0)]), "[1,[2,[3,[4]]],5]");
    let inner_hole = Js::array(vec![Js::holey_array(vec![
        Some(n(1.0)),
        None,
        Some(n(2.0)),
    ])]);
    assert_eq!(flat(inner_hole, vec![]), "[1,2]");
    // Non-array elements, array-likes included, are never spread.
    let like = Js::object(vec![("length", n(1.0)), ("0", n(9.0))]);
    assert_eq!(
        flat(Js::array(vec![like, Js::str("ab")]), vec![]),
        r#"[{"0":9,"length":1},"ab"]"#
    );
}

#[test]
fn number_statics_and_parse_float_identity() {
    let g = library();
    let number = g.get("Number").unwrap();
    let is_finite = |v: Js| run(&g, "Number.isFinite", vec![v]).truthy();
    assert!(is_finite(Js::Number(1.5)));
    assert!(!is_finite(Js::str("1")));
    assert!(!is_finite(Js::Number(f64::INFINITY)));
    assert!(!is_finite(Js::Number(f64::NAN)));
    assert!(!is_finite(Js::Undefined));
    assert_eq!(
        number.get("MAX_SAFE_INTEGER").unwrap().as_f64(),
        Some(9_007_199_254_740_991.0)
    );
    assert!(number.get("NaN").unwrap().as_f64().unwrap().is_nan());
    let parse_float = g.get("parseFloat").unwrap();
    assert!(
        number
            .get("parseFloat")
            .unwrap()
            .same_reference(&parse_float)
    );
    assert_eq!(
        run(&g, "Number.parseFloat", vec![Js::str(" 1.5e2x")]).as_f64(),
        Some(150.0)
    );
}

#[test]
fn performance_now_and_math_random_read_the_runtime() {
    let (runtime, shared) = scoped_globals();
    let g = &shared.globals;
    runtime.clock.set(1234.5);
    assert_eq!(run(g, "performance.now", vec![]).as_f64(), Some(1234.5));
    runtime.clock.set(2000.25);
    assert_eq!(run(g, "performance.now", vec![]).as_f64(), Some(2000.25));
    assert_eq!(run(g, "Math.random", vec![]).as_f64(), Some(0.5));
}

#[test]
fn map_constructor_adds_entries_from_iterables() {
    let g = library();
    let new_map = |v: Vec<Js>| g.get("Map").unwrap().call(Js::Undefined, v);
    let entry = |k: &str, v: f64| Js::array(vec![Js::str(k), Js::Number(v)]);
    let map = new_map(vec![Js::array(vec![
        entry("a", 1.0),
        entry("b", 2.0),
        entry("a", 3.0),
    ])])
    .unwrap();
    assert_eq!(map.get("size").unwrap().as_f64(), Some(2.0));
    assert_eq!(
        json_of(
            &g,
            Js::array(
                map.call_method("entries", vec![])
                    .unwrap()
                    .iterable_values()
                    .unwrap()
            )
        ),
        r#"[["a",3],["b",2]]"#
    );
    // A Map source copies its entries; a Set of entry arrays works too.
    let copy = new_map(vec![map.clone()]).unwrap();
    assert!(!copy.same_reference(&map));
    assert_eq!(
        json_of(
            &g,
            Js::array(
                copy.call_method("entries", vec![])
                    .unwrap()
                    .iterable_values()
                    .unwrap()
            )
        ),
        r#"[["a",3],["b",2]]"#
    );
    let set = g
        .get("Set")
        .unwrap()
        .call(Js::Undefined, vec![Js::array(vec![entry("s", 1.0)])])
        .unwrap();
    let from_set = new_map(vec![set]).unwrap();
    assert_eq!(
        from_set
            .call_method("get", vec![Js::str("s")])
            .unwrap()
            .as_f64(),
        Some(1.0)
    );
    // Array-like objects with "0"/"1" are entry objects; -0 keys normalise to +0.
    let like = Js::object(vec![("0", Js::Number(-0.0)), ("1", Js::str("z"))]);
    let zero = new_map(vec![Js::array(vec![like])]).unwrap();
    let key = zero
        .call_method("keys", vec![])
        .unwrap()
        .call_method("next", vec![])
        .unwrap()
        .get("value")
        .unwrap();
    assert!(key.as_f64().unwrap().is_sign_positive());
    for empty in [vec![], vec![Js::Undefined], vec![Js::Null]] {
        assert_eq!(
            new_map(empty).unwrap().get("size").unwrap().as_f64(),
            Some(0.0)
        );
    }
    assert_eq!(
        thrown_message(new_map(vec![Js::array(vec![Js::Number(1.0)])])),
        "Iterator value 1 is not an entry object"
    );
    assert_eq!(
        thrown_message(new_map(vec![Js::str("ab")])),
        "Iterator value a is not an entry object"
    );
    assert_eq!(
        thrown_message(new_map(vec![Js::holey_array(vec![None])])),
        "Iterator value undefined is not an entry object"
    );
    assert_eq!(
        thrown_message(new_map(vec![Js::object(vec![])])),
        "object is not iterable (cannot read property Symbol(Symbol.iterator))"
    );
    assert_eq!(
        thrown_message(new_map(vec![Js::Number(5.0)])),
        "number 5 is not iterable (cannot read property Symbol(Symbol.iterator))"
    );
}

#[test]
fn array_from_reads_iterables_and_array_likes() {
    let g = library();
    let from = |v: Vec<Js>| g.get("Array").unwrap().call_method("from", v);
    let holey = Js::holey_array(vec![Some(Js::Number(1.0)), None, Some(Js::Number(3.0))]);
    let copied = from(vec![holey.clone()]).unwrap();
    assert!(!copied.same_reference(&holey));
    assert!(copied.has_own("1"), "a hole becomes an own undefined");
    assert_eq!(json_of(&g, copied), "[1,null,3]");
    // Strings iterate by code point: a surrogate pair is one element.
    let text = Js::String("a\u{1F600}b".encode_utf16().collect::<Vec<_>>().into());
    let chars = from(vec![text]).unwrap();
    assert_eq!(chars.get("length").unwrap().as_f64(), Some(3.0));
    assert_eq!(json_of(&g, chars), "[\"a\",\"\u{1F600}\",\"b\"]");
    let map = run(
        &g,
        "Map",
        vec![Js::array(vec![Js::array(vec![
            Js::str("k"),
            Js::Number(1.0),
        ])])],
    );
    assert_eq!(json_of(&g, from(vec![map]).unwrap()), r#"[["k",1]]"#);
    let set = run(
        &g,
        "Set",
        vec![Js::array(vec![Js::str("x"), Js::str("x"), Js::str("y")])],
    );
    assert_eq!(json_of(&g, from(vec![set]).unwrap()), r#"["x","y"]"#);
    let like = Js::object(vec![("length", Js::str("2.9")), ("0", Js::str("x"))]);
    let like = from(vec![like]).unwrap();
    assert_eq!(like.get("length").unwrap().as_f64(), Some(2.0));
    assert!(like.get("1").unwrap().is_undefined() && like.has_own("1"));
    assert_eq!(json_of(&g, from(vec![Js::Number(5.0)]).unwrap()), "[]");
    let indexed = Js::function("", |_, a| {
        Ok(Js::str(&format!(
            "{}{}",
            arg(&a, 0).to_display(),
            arg(&a, 1).to_display()
        )))
    });
    assert_eq!(
        json_of(&g, from(vec![holey, indexed]).unwrap()),
        r#"["10","undefined1","32"]"#
    );
    assert_eq!(
        thrown_message(from(vec![Js::array(vec![]), Js::Number(3.0)])),
        "number 3 is not a function"
    );
    assert_eq!(
        thrown_message(from(vec![Js::array(vec![]), Js::str("x")])),
        "string \"x\" is not a function"
    );
    assert_eq!(
        thrown_message(from(vec![Js::Null])),
        "object null is not iterable (cannot read property Symbol(Symbol.iterator))"
    );
    assert_eq!(
        thrown_message(from(vec![Js::Undefined])),
        "undefined is not iterable (cannot read property Symbol(Symbol.iterator))"
    );
}

#[test]
fn console_error_and_warn_preserve_original_argument_lists() {
    let (runtime, shared) = scoped_globals();
    let g = &shared.globals;
    let object = Js::object(vec![("a", Js::Number(1.0))]);
    let array = Js::array(vec![Js::Number(1.0), Js::Null, Js::Number(2.0)]);
    run(
        g,
        "console.error",
        vec![
            Js::str("reject in updatePositionsInWebworker"),
            Js::Number(1.0),
            object.clone(),
            array.clone(),
            Js::Undefined,
        ],
    );
    run(
        g,
        "console.warn",
        vec![Js::str("No positions found for BTC")],
    );
    run(g, "console.warn", vec![]);
    let values = runtime.console_arguments.borrow();
    assert_eq!(values.len(), 3);
    assert_eq!(values[0].len(), 5);
    assert!(values[0][0].strict_equals(&Js::str("reject in updatePositionsInWebworker")));
    assert!(values[0][1].strict_equals(&Js::Number(1.0)));
    assert!(values[0][2].same_reference(&object));
    assert!(values[0][3].same_reference(&array));
    assert!(values[0][4].is_undefined());
    assert_eq!(values[1].len(), 1);
    assert!(values[1][0].strict_equals(&Js::str("No positions found for BTC")));
    assert!(values[2].is_empty());
    assert_eq!(
        runtime
            .consoled
            .borrow()
            .iter()
            .map(|(severity, _)| severity.as_str())
            .collect::<Vec<_>>(),
        ["error", "warn", "warn"]
    );
}

#[test]
fn date_parse_reads_the_date_time_string_format() {
    let g = library();
    let parse = |text: &str| run(&g, "Date.parse", vec![Js::str(text)]).as_f64().unwrap();
    assert_eq!(parse("2024-01-31"), 1_706_659_200_000.0);
    assert_eq!(parse("2024-01"), 1_704_067_200_000.0);
    assert_eq!(parse("2024"), 1_704_067_200_000.0);
    assert_eq!(parse("2024-01-31T10:20:30.456Z"), 1_706_696_430_456.0);
    assert_eq!(parse("2024-01-31T10:20:30Z"), 1_706_696_430_000.0);
    assert_eq!(parse("2024-01-31T10:20+05:30"), 1_706_676_600_000.0);
    assert_eq!(parse("+002024-01-31T00:00:00Z"), 1_706_659_200_000.0);
    // No host timezone hook: an offset-less date-time is read on a UTC host.
    assert_eq!(parse("2024-01-31T10:20"), 1_706_696_400_000.0);
    for invalid in ["2024-13-01", "2024-01-31T25:00Z", "garbage", ""] {
        assert!(parse(invalid).is_nan(), "{invalid}");
    }
}

fn order(id: &str, symbol: &str) -> safe_cex_types::Order {
    serde_json::from_value(
        json!({"id": id, "status": "open", "symbol": symbol, "type": "limit",
        "side": "buy", "price": 1, "amount": 1, "filled": 0, "remaining": 1,
        "reduceOnly": false, "lastUpdate": 0}),
    )
    .unwrap()
}

#[test]
fn store_facade_reads_fetched_leverage_and_writes_settings_and_statuses() {
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    let facade = safe_cex_exchange::js_runtime::store::facade(store.clone());
    let g = library();
    assert_eq!(json_of(&g, facade.get("fetchedLeverage").unwrap()), "{}");
    facade
        .call_method(
            "update",
            vec![safe_cex_exchange::js_runtime::value::from_json(&json!({
                "fetchedLeverage": {"BTC": {"buyLeverage": 5, "sellLeverage": 3}}
            }))],
        )
        .unwrap();
    assert_eq!(
        to_json(&facade.get("fetchedLeverage").unwrap()).unwrap()["BTC"]["sellLeverage"],
        json!(3.0)
    );

    facade
        .call_method("setSetting", vec![Js::str("isHedged"), Js::Bool(true)])
        .unwrap();
    assert!(store.options().is_hedged);
    assert!(
        facade
            .call_method("setSetting", vec![Js::str("other"), Js::Bool(true)])
            .is_err()
    );

    store.add_order(order("one", "BTC")).unwrap();
    store.add_order(order("two", "BTC")).unwrap();
    let ids = |ids: Vec<Js>| Js::array(ids);
    let by_id = |id: &str| Js::object(vec![("id", Js::str(id))]);
    facade
        .call_method(
            "updateOrdersStatus",
            vec![ids(vec![by_id("one")]), Js::str("canceled")],
        )
        .unwrap();
    // ts keys on `item.id`: a bare string id names the key "undefined" and matches nothing.
    facade
        .call_method(
            "updateOrdersStatus",
            vec![ids(vec![Js::str("two")]), Js::str("closed")],
        )
        .unwrap();
    let statuses: Vec<_> = store
        .orders()
        .iter()
        .map(|o| serde_json::to_value(o.status).unwrap())
        .collect();
    assert_eq!(statuses, [json!("canceled"), json!("open")]);
    // An empty list returns before the status is read, as the ts early return does.
    facade
        .call_method("updateOrdersStatus", vec![ids(vec![]), Js::str("bogus")])
        .unwrap();
}

struct SnapshotVenue {
    base: Rc<safe_cex_exchange::exchange::BaseExchange>,
    capabilities: safe_cex_exchange::capabilities::ExchangeCapabilities,
    orders: RefCell<Result<Vec<safe_cex_types::Order>, String>>,
    calls: RefCell<Vec<Option<Vec<String>>>>,
}

impl safe_cex_exchange::exchange::Exchange for SnapshotVenue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &safe_cex_exchange::exchange::BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &safe_cex_exchange::capabilities::ExchangeCapabilities {
        &self.capabilities
    }
    fn fetch_orders(
        &self,
        symbols: Option<Vec<String>>,
    ) -> ExchangeFuture<'_, Vec<safe_cex_types::Order>> {
        self.calls.borrow_mut().push(symbols);
        let result = self.orders.borrow().clone().map_err(ExchangeError::new);
        Box::pin(async move { result })
    }
}

fn assembled_venue(
    orders: Result<Vec<safe_cex_types::Order>, String>,
) -> (Assembled, Rc<SnapshotVenue>) {
    let assembled = assemble(json!({"key": "k", "secret": "s"})).unwrap();
    let venue = Rc::new(SnapshotVenue {
        base: assembled.base.clone(),
        capabilities: Default::default(),
        orders: RefCell::new(orders),
        calls: RefCell::default(),
    });
    let dynamic: Rc<dyn safe_cex_exchange::exchange::Exchange> = venue.clone();
    *assembled.context.venue.borrow_mut() = Some(Rc::downgrade(&dynamic));
    (assembled, venue)
}

#[test]
fn fetch_and_set_private_snapshot_retries_and_stops_once_disposed() {
    let (a, venue) = assembled_venue(Ok(vec![order("one", "BTC")]));
    settle(
        &a.runtime,
        a.this
            .call_method("fetchAndSetPrivateSnapshot", vec![])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(*venue.calls.borrow(), [None]);
    assert_eq!(a.base.store.orders().len(), 1);
    assert!(a.base.store.loaded().orders);

    // `while (count < tries)`: 2.5 tries is three attempts; null and 0 are none.
    let (a, venue) = assembled_venue(Err("down".into()));
    settle(
        &a.runtime,
        a.this
            .call_method("fetchAndSetPrivateSnapshot", vec![Js::Number(2.5)])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(venue.calls.borrow().len(), 3);
    assert_eq!(a.runtime.clock.get(), 2000.0, "one second between attempts");
    for none in [Js::Null, Js::Number(0.0)] {
        settle(
            &a.runtime,
            a.this
                .call_method("fetchAndSetPrivateSnapshot", vec![none])
                .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(venue.calls.borrow().len(), 3);
    a.base.dispose().unwrap();
    settle(
        &a.runtime,
        a.this
            .call_method("fetchAndSetPrivateSnapshot", vec![Js::Number(10.0)])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        venue.calls.borrow().len(),
        3,
        "a disposed exchange never fetches"
    );
}

#[test]
fn fetch_and_set_orders_scopes_by_symbol_or_replaces_all() {
    let (a, venue) = assembled_venue(Ok(vec![order("one", "BTC"), order("two", "ETH")]));
    let scoped = settle(
        &a.runtime,
        a.this
            .call_method("fetchAndSetOrders", vec![Js::array(vec![Js::str("BTC")])])
            .unwrap(),
    )
    .unwrap();
    assert_eq!(scoped.get("length").unwrap().as_f64(), Some(2.0));
    let ids: Vec<_> = a.base.store.orders().iter().map(|o| o.id.clone()).collect();
    assert_eq!(ids, ["one"]);
    assert!(!a.base.store.loaded().orders);
    settle(
        &a.runtime,
        a.this.call_method("fetchAndSetOrders", vec![]).unwrap(),
    )
    .unwrap();
    assert_eq!(a.base.store.orders().len(), 2);
    assert!(a.base.store.loaded().orders);
    assert_eq!(*venue.calls.borrow(), [Some(vec!["BTC".to_owned()]), None]);
}

#[derive(Default)]
struct FakeProvider {
    candles: RefCell<Vec<Rc<dyn Fn(safe_cex_types::Candle)>>>,
    books: RefCell<Vec<String>>,
    cleanups: Rc<std::cell::Cell<u32>>,
}

impl safe_cex_exchange::external::ExternalPublicData for FakeProvider {
    fn connect(&self) {}
    fn dispose(&self) {}
    fn is_initialized(&self) -> bool {
        true
    }
    fn is_healthy(&self) -> bool {
        true
    }
    fn last_message_timestamp(&self) -> Option<f64> {
        None
    }
    fn listen_ohlcv(
        &self,
        _: safe_cex_types::OHLCVOptions,
        callback: Rc<dyn Fn(safe_cex_types::Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.candles.borrow_mut().push(callback);
        let cleanups = self.cleanups.clone();
        Ok(Box::new(move || {
            cleanups.set(cleanups.get() + 1);
            Ok(())
        }))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        _: Rc<dyn Fn(safe_cex_types::OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        self.books.borrow_mut().push(symbol);
        Ok(Box::new(|| Ok(())))
    }
}

#[test]
fn external_public_data_provider_binds_subscriptions_to_the_rust_provider() {
    let a = assemble(json!({"key": "k", "secret": "s"})).unwrap();
    assert!(
        a.this
            .get("externalPublicDataProvider")
            .unwrap()
            .is_undefined()
    );
    let provider = Rc::new(FakeProvider::default());
    *a.base.external_public_data_provider.borrow_mut() = Some(provider.clone());
    let object = a.this.get("externalPublicDataProvider").unwrap();
    assert!(
        object.same_reference(&a.this.get("externalPublicDataProvider").unwrap()),
        "the field keeps its identity across reads"
    );
    let seen = Rc::new(RefCell::new(vec![]));
    let sink = seen.clone();
    let unsubscribe = object
        .call_method(
            "subscribeCandle",
            vec![
                safe_cex_exchange::js_runtime::value::from_json(
                    &json!({"symbol": "BTCUSDT", "interval": "1m"}),
                ),
                Js::function("", move |_, a| {
                    sink.borrow_mut().push(to_json(&arg(&a, 0)).unwrap());
                    Ok(Js::Undefined)
                }),
            ],
        )
        .unwrap();
    let candle: safe_cex_types::Candle = serde_json::from_value(json!({"timestamp": 60,
        "open": 1, "high": 2, "low": 0.5, "close": 1.5, "volume": 10}))
    .unwrap();
    (provider.candles.borrow()[0])(candle);
    assert_eq!(seen.borrow()[0]["close"], json!(1.5));
    unsubscribe.call(Js::Undefined, vec![]).unwrap();
    unsubscribe.call(Js::Undefined, vec![]).unwrap();
    assert_eq!(provider.cleanups.get(), 1, "cleanup runs once");
    object
        .call_method(
            "subscribeOrderbook",
            vec![
                Js::str("ETHUSDT"),
                Js::function("", |_, _| Ok(Js::Undefined)),
            ],
        )
        .unwrap();
    assert_eq!(*provider.books.borrow(), ["ETHUSDT"]);
    a.base.dispose().unwrap();
    assert!(
        a.this
            .get("externalPublicDataProvider")
            .unwrap()
            .is_undefined()
    );
}

/// A createApi that records the options each client was built with.
fn recording_create_api(built: Rc<RefCell<Vec<Value>>>) -> Js {
    Js::function("createApi", move |_, a| {
        let options = arg(&a, 0);
        let mut seen = to_json(&options)?;
        seen["overrideProxyUrlPresent"] = json!(options.has_own("overrideProxyUrl"));
        built.borrow_mut().push(seen);
        let tag = format!("client{}", built.borrow().len());
        let get_tag = tag.clone();
        Ok(Js::object(vec![
            ("tag", Js::str(&tag)),
            (
                "get",
                Js::function("get", move |_, a| {
                    Ok(Js::str(&format!(
                        "{get_tag} get {}",
                        arg(&a, 0).to_display()
                    )))
                }),
            ),
            (
                "post",
                Js::function("post", move |_, a| {
                    Ok(Js::str(&format!("{tag} post {}", arg(&a, 0).to_display())))
                }),
            ),
        ]))
    })
}

fn limiter(name: &'static str) -> Js {
    Js::function(name, move |_, a| {
        let inner = arg(&a, 0);
        let tag = format!("{}+{name}", inner.get("tag")?.to_display());
        let get_inner = inner.clone();
        Ok(Js::object(vec![
            ("tag", Js::str(&tag)),
            (
                "get",
                Js::function("get", move |_, a| get_inner.call_method("get", a)),
            ),
            (
                "post",
                Js::function("post", move |_, a| inner.call_method("post", a)),
            ),
        ]))
    })
}

#[test]
fn create_xhr_with_secondary_key_is_one_client_without_a_secondary_key() {
    let a = assemble(json!({"key": "k", "secret": "s"})).unwrap();
    let built = Rc::new(RefCell::new(vec![]));
    let xhr = a
        .this
        .call_method(
            "createXhrWithSecondaryKey",
            vec![
                a.this.get("options").unwrap(),
                recording_create_api(built.clone()),
                Js::object(vec![("rateLimit", limiter("limit"))]),
            ],
        )
        .unwrap();
    assert_eq!(built.borrow().len(), 1);
    assert_eq!(xhr.get("tag").unwrap().to_display(), "client1+limit");
    assert!(xhr.same_reference(&a.this.get("_primaryXhr").unwrap()));
    assert!(xhr.same_reference(&a.this.get("_secondaryXhr").unwrap()));
}

#[test]
fn create_xhr_with_secondary_key_routes_gets_to_the_secondary_client() {
    let options = json!({"key": "k", "secret": "s", "secondary_key": "rk",
        "secondary_secret": "rs", "secondaryProxyUrl": "https://reads"});
    let a = assemble(options).unwrap();
    let built = Rc::new(RefCell::new(vec![]));
    let xhr = a
        .this
        .call_method(
            "createXhrWithSecondaryKey",
            vec![
                a.this.get("options").unwrap(),
                recording_create_api(built.clone()),
                Js::object(vec![
                    ("proxyUrl", Js::str("https://other")),
                    ("rateLimit", limiter("limit")),
                    ("secondaryRateLimit", limiter("readLimit")),
                ]),
            ],
        )
        .unwrap();
    let built = built.borrow();
    assert_eq!(built.len(), 2);
    assert_eq!(built[0]["key"], json!("k"));
    assert!(!built[0]["overrideProxyUrlPresent"].as_bool().unwrap());
    assert_eq!(built[1]["key"], json!("rk"));
    assert_eq!(built[1]["secret"], json!("rs"));
    assert_eq!(built[1]["overrideProxyUrl"], json!("https://reads"));
    let primary = a.this.get("_primaryXhr").unwrap();
    let secondary = a.this.get("_secondaryXhr").unwrap();
    assert_eq!(primary.get("tag").unwrap().to_display(), "client1+limit");
    assert_eq!(
        secondary.get("tag").unwrap().to_display(),
        "client2+readLimit"
    );
    let call = |name: &str| {
        xhr.call_method(name, vec![Js::str("/x")])
            .unwrap()
            .to_display()
    };
    assert_eq!(call("get"), "client2 get /x");
    assert_eq!(call("post"), "client1 post /x");
    assert_eq!(xhr.get("tag").unwrap().to_display(), "client1+limit");
}

#[test]
fn create_xhr_with_secondary_key_proxy_precedence() {
    let overrides = |options: Value, other: Js| {
        let a = assemble(options).unwrap();
        let built = Rc::new(RefCell::new(vec![]));
        a.this
            .call_method(
                "createXhrWithSecondaryKey",
                vec![
                    a.this.get("options").unwrap(),
                    recording_create_api(built.clone()),
                    other,
                ],
            )
            .unwrap();
        let secondary = built.borrow()[1].clone();
        (
            secondary["overrideProxyUrl"].clone(),
            secondary["overrideProxyUrlPresent"].clone(),
        )
    };
    let keys = json!({"key": "k", "secret": "s", "secondary_key": "rk"});
    let mut empty_secondary_proxy = keys.clone();
    empty_secondary_proxy["secondaryProxyUrl"] = json!("");
    assert_eq!(
        overrides(
            empty_secondary_proxy,
            Js::object(vec![("proxyUrl", Js::str("https://other"))])
        ),
        (json!("https://other"), json!(true))
    );
    // Direct transport has no public CORS proxy: the key is set to undefined.
    assert_eq!(overrides(keys, Js::Undefined), (Value::Null, json!(true)));
}

#[test]
fn store_facade_update_positions_patches_each_pair_by_position_id() {
    use safe_cex_store::store::DefaultStore;
    let store = Rc::new(DefaultStore::new());
    let position = |id: &str| {
        serde_json::from_value(json!({"id":id,"symbol":id,"side":"long","entryPrice":1,
            "notional":1,"leverage":1,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0}))
        .unwrap()
    };
    store
        .add_or_update_positions(vec![position("BTC"), position("ETH")])
        .unwrap();
    let facade = safe_cex_exchange::js_runtime::store::facade(store.clone());
    // ts: updatePositions(updates: Array<[Pick<Position, 'id'>, Partial<Position>]>)
    let pair = |id: &str, pnl: f64| {
        Js::array(vec![
            Js::object(vec![("id", Js::str(id))]),
            Js::object(vec![("unrealizedPnl", Js::Number(pnl))]),
        ])
    };
    facade
        .call_method(
            "updatePositions",
            vec![Js::array(vec![pair("ETH", 7.0), pair("BTC", 3.0)])],
        )
        .unwrap();
    let pnl: Vec<f64> = store.positions().iter().map(|p| p.unrealized_pnl).collect();
    assert_eq!(pnl, [3.0, 7.0]);
}

#[test]
fn number_converts_bigint_like_ecmascript() {
    let g = library();
    let number = |args: Vec<Js>| run(&g, "Number", args).as_f64().unwrap();
    let big = |text: &str| Js::BigInt(Rc::new(text.parse().unwrap()));
    assert_eq!(number(vec![big("5")]), 5.0, "Number(5n) === 5");
    assert_eq!(number(vec![big("-42")]), -42.0);
    // 2^53 + 1 is not a double; the nearest-even double is 2^53.
    assert_eq!(
        number(vec![big("9007199254740993")]),
        9_007_199_254_740_992.0
    );
    assert_eq!(number(vec![]), 0.0, "Number() is 0");
    assert_eq!(number(vec![Js::str(" 12 ")]), 12.0);
}

#[test]
fn function_name_reads_its_function_data_name() {
    let f = Js::function("handleTickers", |_, _| Ok(Js::Undefined));
    assert_eq!(f.get("name").unwrap().to_display(), "handleTickers");
    f.define("name", Js::str("renamed")).unwrap();
    assert_eq!(f.get("name").unwrap().to_display(), "renamed");
}

#[test]
fn client_rejection_handlers_follow_the_axios_promise_chain() {
    let runtime = Rc::new(Runtime::default());
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
    });
    let xhr = client::create(http.clone(), Js::object(vec![]));
    let interceptors = xhr.get("interceptors").unwrap();
    let order = Rc::new(RefCell::new(Vec::<String>::new()));
    let log = |tag: &'static str, result: fn(Js) -> Result<Js, Thrown>| {
        let order = order.clone();
        Js::function("", move |_, a| {
            order.borrow_mut().push(tag.into());
            result(arg(&a, 0))
        })
    };
    let throws: fn(Js) -> Result<Js, Thrown> = |_| Err(Thrown::error("Error", "boom"));
    let passes: fn(Js) -> Result<Js, Thrown> = Ok;
    let recovers: fn(Js) -> Result<Js, Thrown> =
        |error| Ok(Js::object(vec![("recovered", error.get("message")?)]));
    let rethrows: fn(Js) -> Result<Js, Thrown> = |error| Err(Thrown::new(error));
    let request = interceptors.get("request").unwrap();
    let response = interceptors.get("response").unwrap();
    // LIFO: "outer" runs first and throws, so "inner"'s onRejected sees it.
    request
        .call_method(
            "use",
            vec![log("inner ok", passes), log("inner rejected", rethrows)],
        )
        .unwrap();
    request
        .call_method(
            "use",
            vec![log("outer ok", throws), log("outer rejected", rethrows)],
        )
        .unwrap();
    response
        .call_method(
            "use",
            vec![log("first ok", passes), log("first rejected", rethrows)],
        )
        .unwrap();
    response
        .call_method(
            "use",
            vec![log("second ok", passes), log("second rejected", recovers)],
        )
        .unwrap();
    response
        .call_method("use", vec![log("third ok", passes), Js::Undefined])
        .unwrap();
    let result = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/ok")]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        *order.borrow(),
        [
            "outer ok",
            "inner rejected",
            "first rejected",
            "second rejected",
            "third ok"
        ]
    );
    assert_eq!(result.get("recovered").unwrap().to_display(), "boom");
    assert!(
        http.sent.borrow().is_empty(),
        "a rejected request is never sent"
    );

    // An adapter-side failure (non-2xx) reaches the response rejection handlers.
    let xhr = client::create(http.clone(), Js::object(vec![]));
    let response = xhr.get("interceptors").unwrap().get("response").unwrap();
    response
        .call_method("use", vec![Js::Undefined, log("status rejected", recovers)])
        .unwrap();
    order.borrow_mut().clear();
    let result = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/fail")]).unwrap(),
    )
    .unwrap();
    assert_eq!(*order.borrow(), ["status rejected"]);
    assert_eq!(
        result.get("recovered").unwrap().to_display(),
        "Request failed with status code 500"
    );
}

#[test]
fn source_socket_fields_reach_typed_groups_and_track_replacement_and_removal() {
    use safe_cex_exchange::{
        js_runtime::sockets::{base_websocket, core_of},
        socket_lifecycle::SocketSlot,
    };
    let (a, _venue) = assembled_venue(Ok(vec![]));
    assert!(a.base.get_all_sockets().is_empty());
    // Exact order from source BaseExchange.getAllSockets/getPublicSockets.
    let fields = [
        "privateWebsocket",
        "publicWebsocket",
        "publicWebsocketSecondary",
        "publicInverseWebsocket",
        "futuresWebsocket",
        "businessWebsocket",
        "secondsTickersWebsocket",
        "thirdTickersWebsocket",
    ];
    let sockets: Vec<_> = fields
        .iter()
        .map(|field| {
            let socket = base_websocket(&a.context, a.this.clone());
            a.this.set(field, socket.clone()).unwrap();
            socket
        })
        .collect();
    let expected: Vec<_> = sockets
        .iter()
        .map(|socket| core_of(&a.context, socket).unwrap().base().unwrap())
        .collect();
    let actual = a.base.get_all_sockets();
    assert_eq!(
        actual.len(),
        8,
        "typed lifecycle sees every source-owned socket"
    );
    for (actual, expected) in actual.iter().zip(&expected) {
        assert!(Rc::ptr_eq(actual, expected));
    }
    assert!(Rc::ptr_eq(&a.base.get_private_sockets()[0], &expected[0]));
    assert_eq!(
        a.base
            .get_public_sockets()
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        expected[1..].iter().map(|s| s.id).collect::<Vec<_>>()
    );
    let replacement = base_websocket(&a.context, a.this.clone());
    a.this.set("privateWebsocket", replacement.clone()).unwrap();
    let replacement_core = core_of(&a.context, &replacement).unwrap().base().unwrap();
    assert!(Rc::ptr_eq(
        &a.base.get_socket(SocketSlot::Private).unwrap(),
        &replacement_core
    ));
    a.this.delete(&utf16("publicWebsocket"));
    assert!(a.base.get_socket(SocketSlot::Public).is_none());
    assert_eq!(a.base.get_all_sockets().len(), 7);
    assert_eq!(a.base.get_public_sockets().len(), 6);
    a.base.dispose().unwrap();
    assert!(replacement_core.is_disposed());
    assert!(expected[1..].iter().all(|socket| socket.is_disposed()));
}

#[test]
fn date_now_is_integer_while_performance_retains_fractional_milliseconds() {
    let (runtime, shared) = scoped_globals();
    for (wall, expected) in [
        (1_735_689_600_000.75, 1_735_689_600_000.0),
        (1234.125, 1234.0),
    ] {
        runtime.wall_clock.set(wall);
        runtime.clock.set(7.625);
        assert_eq!(
            run(&shared.globals, "Date.now", vec![]).as_f64(),
            Some(expected)
        );
        assert_eq!(
            run(&shared.globals, "performance.now", vec![]).as_f64(),
            Some(7.625)
        );
    }
}

mod http_factory {
    use super::*;
    use safe_cex_exchange::js_runtime::client::factory;
    use safe_cex_transport::{
        DirectTransport, PrivateProxy, ResolvedRouting, RestAdapter, RestAdapterContext,
        RestAuthorizer, RestFuture, RestRequest, RestResponse, RestResponseBody, RestTransport,
        RestTransportContext, RoutingOptions, TransportError, TransportFuture, TransportProvider,
        WsConnection,
    };
    use safe_cex_types::{ExchangeName, ExchangeOptions};
    use std::sync::Arc;
    struct Authorizer {
        events: Rc<RefCell<Vec<String>>>,
        fail: bool,
    }
    impl RestAuthorizer for Authorizer {
        fn authorize<'a>(&'a self, request: &'a mut RestRequest) -> TransportFuture<'a, ()> {
            Box::pin(async move {
                self.events.borrow_mut().push("authorize".into());
                if self.fail {
                    return Err(TransportError::Host("token rejected".into()));
                }
                request
                    .headers
                    .insert("Proxy-Authorization".into(), "Bearer token".into());
                Ok(())
            })
        }
    }
    struct Provider {
        proxied: bool,
        authorize: Arc<dyn RestAuthorizer>,
        adapter: Option<Arc<dyn RestAdapter>>,
        variants: RefCell<Vec<safe_cex_transport::RestAdapterVariant>>,
    }
    impl TransportProvider for Provider {
        fn name(&self) -> &str {
            "recording"
        }
        fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
            Ok(RestTransport {
                base_url: if self.proxied {
                    c.config
                        .base_url
                        .as_ref()
                        .map(|base| format!("https://proxy/{base}"))
                } else {
                    c.config.base_url.clone()
                },
                proxied: self.proxied,
                authorize: Some(self.authorize.clone()),
            })
        }
        fn rest_adapter(&self, c: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
            self.variants.borrow_mut().push(c.variant);
            self.adapter.clone()
        }
        fn proxy_prefix(&self, _: RestTransportContext<'_>) -> String {
            String::new()
        }
        fn public_cors_proxy(&self) -> Option<String> {
            None
        }
        fn private_proxy(&self, _: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
            None
        }
        fn require_proxy(&self, o: ExchangeOptions) -> ExchangeOptions {
            o
        }
        fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
            None
        }
        fn ws_routing(&self, e: &ExchangeName, o: &RoutingOptions) -> ResolvedRouting {
            DirectTransport.ws_routing(e, o)
        }
        fn ws_connection(&self, _: &str) -> Option<WsConnection> {
            None
        }
    }
    struct Terminal {
        events: Rc<RefCell<Vec<String>>>,
        sent: Rc<RefCell<Vec<RestRequest>>>,
        query_in_url: bool,
    }
    impl RestAdapter for Terminal {
        fn query_in_url(&self) -> bool {
            self.query_in_url
        }
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            self.events.borrow_mut().push("send".into());
            self.sent.borrow_mut().push(request);
            Box::pin(async {
                Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(
                            json!({"ok":true}),
                        )
                        .unwrap(),
                    ),
                })
            })
        }
    }
    fn opts(value: Value) -> Js {
        safe_cex_exchange::js_runtime::value::from_json(&value)
    }
    #[test]
    #[allow(clippy::arc_with_non_send_sync)]
    fn source_factory_routes_and_authorizes_after_venue_signing_without_losing_headers() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/js_http_factory_source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(rows.len(), 4);
        for row in rows {
            let runtime = Rc::new(Runtime::default());
            let events = Rc::new(RefCell::new(vec![]));
            let sent = Rc::new(RefCell::new(vec![]));
            let terminal = Arc::new(Terminal {
                events: events.clone(),
                sent: sent.clone(),
                query_in_url: true,
            });
            let host = client::rest_http_host(runtime.clone(), terminal);
            let provider = Provider {
                proxied: row["proxied"].as_bool().unwrap(),
                authorize: Arc::new(Authorizer {
                    events: events.clone(),
                    fail: row["failAuth"].as_bool().unwrap(),
                }),
                adapter: None,
                variants: RefCell::default(),
            };
            let config = opts(
                json!({"baseURL":"https://venue","timeout":31,"headers":{"X-Venue":"initial"}}),
            );
            let xhr = factory::create(
                host,
                &provider,
                config,
                &opts(json!({"key":"","secret":""})),
                &factory_owner(runtime.clone()),
            )
            .unwrap();
            let trace = events.clone();
            xhr.get("interceptors")
                .unwrap()
                .get("request")
                .unwrap()
                .call_method(
                    "use",
                    vec![Js::function("venue", move |_, a| {
                        trace.borrow_mut().push("venue".into());
                        let config = arg(&a, 0);
                        config
                            .get("headers")?
                            .call_method("set", vec![Js::str("X-Venue"), Js::str("signed")])?;
                        Ok(config)
                    })],
                )
                .unwrap();
            let result = settle(
                &runtime,
                xhr.call_method("get", vec![Js::str("/book")]).unwrap(),
            );
            assert_eq!(json!(*events.borrow()), row["events"]);
            assert_eq!(
                result.as_ref().err().map(Thrown::message),
                row["error"].as_str().map(str::to_owned)
            );
            let requests = sent.borrow();
            if row["wire"].is_null() {
                assert!(requests.is_empty());
            } else {
                assert_eq!(requests.len(), 1);
                let request = &requests[0];
                assert_eq!(
                    json!({"baseURL":request.base_url,"timeout":request.timeout_ms,"headers":request.headers}),
                    row["wire"]
                );
                assert_eq!(
                    result
                        .unwrap()
                        .get("config")
                        .unwrap()
                        .get("headers")
                        .unwrap()
                        .call_method("get", vec![Js::str("x-venue")])
                        .unwrap()
                        .to_display(),
                    "signed"
                );
            }
        }
    }
    #[test]
    fn direct_factory_preserves_override_and_refuses_unavailable_routing_before_send() {
        let runtime = Rc::new(Runtime::default());
        let http = Rc::new(Http {
            runtime: runtime.clone(),
            sent: RefCell::default(),
        });
        let options = opts(json!({"key":"","secret":"","overrideProxyUrl":"https://override"}));
        let xhr = factory::create(
            http.clone(),
            &DirectTransport,
            opts(json!({"baseURL":"https://venue","timeout":999})),
            &options,
            &factory_owner(http.runtime()),
        )
        .unwrap();
        settle(
            &runtime,
            xhr.call_method("get", vec![Js::str("/book")]).unwrap(),
        )
        .unwrap();
        assert_eq!(
            http.sent.borrow()[0]["baseURL"],
            "https://override/https://venue"
        );
        for flag in ["tealstreetProxy", "tealstreetTunnel"] {
            let error = factory::create(
                http.clone(),
                &DirectTransport,
                opts(json!({"baseURL":"https://venue"})),
                &opts(json!({"key":"","secret":"",flag:true})),
                &factory_owner(http.runtime()),
            )
            .unwrap_err();
            assert!(error.message().contains("direct transport is active"));
        }
        assert_eq!(http.sent.borrow().len(), 1);
    }
    #[test]
    #[allow(clippy::arc_with_non_send_sync)]
    fn explicit_transport_installation_selects_variants_and_leaves_how_params_in_frame() {
        use safe_cex_transport::{RestAdapterRegion, RestAdapterVariant as V};
        let runtime = Rc::new(Runtime::default());
        let direct = Rc::new(Http {
            runtime: runtime.clone(),
            sent: RefCell::default(),
        });
        let events = Rc::new(RefCell::new(vec![]));
        let sent = Rc::new(RefCell::new(vec![]));
        let provider = Provider {
            proxied: false,
            authorize: Arc::new(Authorizer {
                events: events.clone(),
                fail: false,
            }),
            adapter: Some(Arc::new(Terminal {
                events,
                sent: sent.clone(),
                query_in_url: false,
            })),
            variants: RefCell::default(),
        };
        let options = opts(json!({"key":"","secret":"","tealstreetTunnel":true}));
        let xhr = factory::create(
            direct.clone(),
            &provider,
            opts(json!({"baseURL":"https://venue"})),
            &options,
            &factory_owner(direct.runtime()),
        )
        .unwrap();
        let variants = [
            V::Default,
            V::BigInt,
            V::Region {
                region: Some(RestAdapterRegion::Ja),
            },
        ];
        for variant in &variants {
            factory::install_transport_adapter(
                direct.clone(),
                &provider,
                &xhr,
                &options,
                ExchangeName::Polymarket,
                variant.clone(),
            )
            .unwrap();
            settle(
                &runtime,
                xhr.call_method(
                    "get",
                    vec![
                        Js::str("/book"),
                        Js::object(vec![
                            ("params", opts(json!({"id":"9007199254740993"}))),
                            (
                                "paramsSerializer",
                                Js::function("serialize", |_, _| {
                                    panic!("HOW must retain frame params")
                                }),
                            ),
                        ]),
                    ],
                )
                .unwrap(),
            )
            .unwrap();
        }
        assert_eq!(*provider.variants.borrow(), variants);
        assert!(direct.sent.borrow().is_empty());
        for request in sent.borrow().iter() {
            assert_eq!(request.url, "/book");
            assert_eq!(request.params, Some(json!({"id":"9007199254740993"})));
        }
        let absent = Provider {
            adapter: None,
            ..provider
        };
        factory::install_transport_adapter(
            direct,
            &absent,
            &xhr,
            &options,
            ExchangeName::Polymarket,
            V::Default,
        )
        .unwrap();
        settle(
            &runtime,
            xhr.call_method("get", vec![Js::str("/again")]).unwrap(),
        )
        .unwrap();
        assert_eq!(
            sent.borrow().last().unwrap().url,
            "/again",
            "absent adapter retains the installed inner adapter"
        );
    }

    struct Signer {
        events: Rc<RefCell<Vec<String>>>,
    }
    impl safe_cex_transport::signing::RemoteSigner for Signer {
        fn sign(
            &self,
            request: safe_cex_transport::signing::RemoteSignerRequest,
        ) -> TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse> {
            use safe_cex_transport::signing::*;
            self.events.borrow_mut().push("sign".into());
            let RemoteSignerRequest::Http(request) = request else {
                panic!("HTTP expected")
            };
            assert_eq!(request.exchange_name, ExchangeName::Polymarket);
            assert_eq!(request.account_name.as_deref(), Some("hosted"));
            assert_eq!(
                request
                    .config
                    .headers
                    .get("Proxy-Authorization")
                    .map(String::as_str),
                Some("Bearer token")
            );
            Box::pin(async move {
                Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                    id: request.id,
                    config: Some(SignedConfig {
                        url: Some("/signed".into()),
                        params: Some(json!({"signed":7.0})),
                        headers: [("Remote-Signature".into(), "signed".into())]
                            .into_iter()
                            .collect(),
                        ..Default::default()
                    }),
                    error: None,
                }))
            })
        }
    }
    impl safe_cex_exchange::runtime::RemoteSigner for Signer {
        fn is_available(&self) -> bool {
            true
        }
        fn on_availability_change(
            &self,
            _: Rc<dyn Fn(bool)>,
        ) -> safe_cex_exchange::runtime::Cleanup {
            Box::new(|| {})
        }
    }
    #[test]
    #[allow(clippy::arc_with_non_send_sync)]
    fn replacing_transport_keeps_remote_signing_below_proxy_auth_and_before_query_serialization() {
        use safe_cex_transport::RestAdapterVariant;
        let runtime = Rc::new(Runtime::default());
        let events = Rc::new(RefCell::new(vec![]));
        let sent = Rc::new(RefCell::new(vec![]));
        let terminal = Arc::new(Terminal {
            events: events.clone(),
            sent: sent.clone(),
            query_in_url: true,
        });
        let host = client::rest_http_host_signed(
            runtime.clone(),
            terminal.clone(),
            Some((
                Rc::new(Signer {
                    events: events.clone(),
                }),
                Some("hosted".into()),
            )),
        );
        let provider = Provider {
            proxied: true,
            authorize: Arc::new(Authorizer {
                events: events.clone(),
                fail: false,
            }),
            adapter: Some(terminal),
            variants: RefCell::default(),
        };
        let options = opts(json!({"key":"","secret":""}));
        let trace = events.clone();
        let xhr = factory::create(
            host.clone(),
            &provider,
            Js::object(vec![
                ("baseURL", Js::str("https://venue")),
                (
                    "paramsSerializer",
                    Js::function("serialize", move |_, a| {
                        trace.borrow_mut().push("serialize".into());
                        assert_eq!(to_json(&arg(&a, 0))?, json!({"signed":7.0}));
                        Ok(Js::str("signed=7"))
                    }),
                ),
            ]),
            &options,
            &factory_owner(host.runtime()),
        )
        .unwrap();
        factory::install_transport_adapter(
            host,
            &provider,
            &xhr,
            &options,
            ExchangeName::Polymarket,
            RestAdapterVariant::Default,
        )
        .unwrap();
        let response = settle(
            &runtime,
            xhr.call_method(
                "get",
                vec![Js::str("/book"), opts(json!({"params":{"unsigned":3}}))],
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(*events.borrow(), ["authorize", "sign", "serialize", "send"]);
        let sent = sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].url, "/signed?signed=7");
        assert_eq!(
            sent[0].headers.get("Remote-Signature").map(String::as_str),
            Some("signed")
        );
        assert_eq!(
            response
                .get("config")
                .unwrap()
                .get("url")
                .unwrap()
                .to_display(),
            "/signed"
        );
        assert_eq!(
            to_json(&response.get("config").unwrap().get("params").unwrap()).unwrap(),
            json!({"signed":7.0})
        );
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn disposed_assignment_matches_actual_venue_cleanup_sequence() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js-runtime-disposal-oracle.cjs"
        ))
        .output()
        .expect("source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        let assembled = assemble(json!({"key":"k","secret":"s"})).unwrap();
        let this = assembled.this;
        let base = assembled.base;
        let events = Rc::new(RefCell::new(vec![]));
        let observed = Rc::new(std::cell::Cell::new(0));
        let observed_in_listener = observed.clone();
        base.on(
            "update",
            Rc::new(move |_, _| {
                observed_in_listener.set(observed_in_listener.get() + 1);
                Ok(())
            }),
        );
        for side in ["public", "private"] {
            let events = events.clone();
            let base = base.clone();
            let fail = row["fail"] == side;
            this.define(
                &format!("{side}Websocket"),
                Js::object(vec![(
                    "dispose",
                    Js::function("dispose", move |_, _| {
                        events.borrow_mut().push(json!([side, base.is_disposed()]));
                        if fail {
                            Err(Thrown::new(Js::str(&format!("{side}-cleanup"))))
                        } else {
                            Ok(Js::Undefined)
                        }
                    }),
                )]),
            )
            .unwrap();
        }
        let result = (|| -> Result<(), Thrown> {
            this.set("isDisposed", Js::Bool(true))?;
            this.get("publicWebsocket")?
                .call_method("dispose", vec![])?;
            this.get("privateWebsocket")?
                .call_method("dispose", vec![])?;
            Ok(())
        })();
        assert_eq!(
            json!(events.borrow().clone()),
            row["events"],
            "{} {} callback sequence",
            row["venue"],
            row["fail"]
        );
        assert_eq!(
            base.is_disposed(),
            row["disposed"].as_bool().unwrap(),
            "shared core observes source assignment"
        );
        assert!(
            this.get("isDisposed")
                .unwrap()
                .strict_equals(&Js::Bool(true)),
            "facade reads the same cell"
        );
        assert_eq!(
            result
                .err()
                .map(|e| to_json(&e.value).unwrap())
                .unwrap_or(Value::Null),
            row["error"],
            "cleanup throw identity"
        );
        base.emit("update", Js::object(vec![])).unwrap();
        assert_eq!(
            observed.get(),
            1,
            "flag assignment must not run superclass listener cleanup"
        );
        this.set("isDisposed", Js::Bool(false)).unwrap();
        assert!(
            !base.is_disposed(),
            "the source field is writable in either direction"
        );
    }
}

#[test]
fn rest_http_host_retains_adapter_config_without_inventing_direct_cookie_headers() {
    let runtime = Rc::new(Runtime::default());
    let terminal = std::sync::Arc::new(Terminal::default());
    let xhr = client::create(
        client::rest_http_host(runtime.clone(), terminal.clone()),
        Js::object(vec![("baseURL", Js::str("https://trades.grvt.io"))]),
    );
    let response = settle(
        &runtime,
        xhr.call_method(
            "post",
            vec![
                Js::str("/ok"),
                Js::Null,
                Js::object(vec![
                    ("grvtCookie", Js::str("rm=true; gravity=session-42")),
                    ("grvtAccountId", Js::str("42")),
                    (
                        "customTrace",
                        Js::object(vec![("attempt", Js::Number(2.0))]),
                    ),
                    (
                        "headers",
                        Js::object(vec![("X-Ts-Grvt-Session", Js::str("session-42"))]),
                    ),
                ]),
            ],
        )
        .unwrap(),
    )
    .unwrap();
    let seen = terminal.seen.lock().unwrap();
    assert_eq!(
        seen[0].config_extensions["grvtCookie"]
            .clone()
            .into_value()
            .unwrap(),
        json!("rm=true; gravity=session-42"),
        "REST ADAPTER CONFIG lost GRVT cookie before terminal transport"
    );
    assert_eq!(
        seen[0].config_extensions["grvtAccountId"]
            .clone()
            .into_value()
            .unwrap(),
        json!("42")
    );
    assert_eq!(
        seen[0].config_extensions["customTrace"]
            .clone()
            .into_value()
            .unwrap(),
        json!({"attempt":2.0})
    );
    assert!(
        !seen[0].headers.contains_key("Cookie"),
        "DIRECT source cookie behavior"
    );
    assert_eq!(seen[0].headers["X-Ts-Grvt-Session"], "session-42");
    assert_eq!(
        to_json(&response.get("config").unwrap().get("customTrace").unwrap()).unwrap(),
        json!({"attempt":2.0})
    );
}

#[path = "support/js_websocket_options.rs"]
mod ws_options;

#[path = "support/js_socket_dispose.rs"]
mod socket_dispose;

#[test]
fn source_identity_fields_are_read_before_the_typed_base_is_published() {
    for (spot, demo, expected_spot, expected_demo) in [
        (Js::Bool(true), Js::Bool(false), true, false),
        (Js::Bool(false), Js::Bool(true), false, true),
        (Js::str("source truthy"), Js::Number(0.0), true, false),
        (Js::Null, Js::object(vec![]), false, true),
    ] {
        let assembled = assemble_with_fields(json!({"key":"k","secret":"s"}), |receiver| {
            receiver.define_accessor("isSpot", Some(Rc::new(move |_| Ok(spot.clone()))), None);
            receiver.define("isDemo", demo)
        })
        .unwrap();
        assert_eq!(
            assembled.base.is_spot, expected_spot,
            "source isSpot projection"
        );
        assert_eq!(
            assembled.base.is_demo, expected_demo,
            "source isDemo projection"
        );
        assert_eq!(
            assembled.this.get("isSpot").unwrap().truthy(),
            expected_spot
        );
        assert_eq!(
            assembled.this.get("isDemo").unwrap().truthy(),
            expected_demo
        );
    }
    let inherited = assemble(json!({"key":"k","secret":"s"})).unwrap();
    assert!(!inherited.base.is_spot && !inherited.base.is_demo);
    let cause = Thrown::error("RangeError", "source flag getter");
    let expected = cause.value.clone();
    let result = assemble_with_fields(json!({"key":"k","secret":"s"}), |receiver| {
        receiver.define_accessor("isDemo", Some(Rc::new(move |_| Err(cause.clone()))), None);
        Ok(())
    });
    assert!(result.err().unwrap().value.strict_equals(&expected));
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn hosted_signing_accepts_source_string_data_before_wire_encoding() {
    use safe_cex_transport::signing::*;
    use safe_cex_types::ExchangeName;
    struct EchoSigner(Rc<RefCell<Vec<Option<Value>>>>);
    impl RemoteSigner for EchoSigner {
        fn sign(
            &self,
            request: RemoteSignerRequest,
        ) -> safe_cex_transport::TransportFuture<'_, RemoteSignerResponse> {
            let RemoteSignerRequest::Http(request) = request else {
                panic!("HTTP expected")
            };
            self.0.borrow_mut().push(
                request
                    .config
                    .data
                    .clone()
                    .map(|value| value.into_value().unwrap()),
            );
            let data = request.config.data.map(|value| {
                if matches!(
                    value,
                    safe_cex_types::retained_json::RetainedJsonInput::Object(_)
                        | safe_cex_types::retained_json::RetainedJsonInput::Array(_)
                ) {
                    safe_cex_types::retained_json::RetainedJsonInput::Text(
                        safe_cex_num::js_retained_json_stringify(&value)
                            .unwrap()
                            .unwrap()
                            .encode_utf16()
                            .collect(),
                    )
                } else {
                    value
                }
            });
            Box::pin(async move {
                Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                    id: request.id,
                    error: None,
                    config: Some(SignedConfig {
                        headers: request.config.headers,
                        data,
                        ..Default::default()
                    }),
                }))
            })
        }
    }
    impl safe_cex_exchange::runtime::RemoteSigner for EchoSigner {
        fn is_available(&self) -> bool {
            true
        }
        fn on_availability_change(
            &self,
            _: Rc<dyn Fn(bool)>,
        ) -> safe_cex_exchange::runtime::Cleanup {
            Box::new(|| {})
        }
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_hosted_string_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 4, "source string-data corpus count");
    for row in rows {
        let runtime = Rc::new(Runtime::default());
        let terminal = std::sync::Arc::new(Terminal::default());
        let calls = Rc::new(RefCell::new(Vec::new()));
        let host = client::rest_http_host_signed(
            runtime.clone(),
            terminal.clone(),
            Some((Rc::new(EchoSigner(calls.clone())), Some("hosted".into()))),
        );
        host.configure_exchange(ExchangeName::Apexomni).unwrap();
        let xhr = client::create(host, Js::object(vec![]));
        let response = xhr
            .call_method(
                "post",
                vec![
                    Js::str("/order"),
                    Js::str(row["data"].as_str().unwrap()),
                    safe_cex_exchange::js_runtime::value::from_json(
                        &json!({"headers":{"Content-Type":row["contentType"]}}),
                    ),
                ],
            )
            .unwrap();
        settle(&runtime, response).expect("hosted signing accepts a source JS string");
        assert_eq!(
            &*calls.borrow(),
            &[Some(row["requestData"].clone())],
            "remote signer sees source transform/tryParseJson data"
        );
        let seen = terminal.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let request = &seen[0];
        let content_type = request
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value.as_str());
        let bytes = request
            .body
            .as_ref()
            .unwrap()
            .wire_bytes_for_content_type(content_type)
            .unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            row["wireData"].as_str().unwrap(),
            "signed source string reaches terminal unchanged"
        );
    }
}

#[path = "support/js_string_coercion.rs"]
mod string_coercion;

#[path = "support/js_raw_socket_close.rs"]
mod raw_socket_close;

#[test]
fn virtual_clock_global_preserves_synced_time_without_changing_date_now() {
    use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
    let (runtime, shared) = scoped_globals();
    runtime.wall_clock.set(123.0);
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 40_000.0);
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    let actual = shared
        .globals
        .get("virtualClock")
        .unwrap()
        .call_method("getCurrentTimeMs", vec![])
        .unwrap()
        .as_f64()
        .unwrap();
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    assert!(
        actual >= before && actual <= after,
        "JS virtualClock must retain server offset"
    );
    let before = VIRTUAL_CLOCK.get_current_time();
    let actual = shared
        .globals
        .get("virtualClock")
        .unwrap()
        .call_method("getCurrentTime", vec![])
        .unwrap()
        .to_number()
        .unwrap();
    let after = VIRTUAL_CLOCK.get_current_time();
    assert!(
        actual >= before && actual <= after,
        "dayjs virtualClock must retain server offset"
    );
    assert_eq!(
        shared
            .globals
            .get("Date")
            .unwrap()
            .call_method("now", vec![])
            .unwrap()
            .as_f64(),
        Some(123.0)
    );
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
}

/// The BaseExchange a safeCexAxiosCreate client reports outcomes and errors to.
fn factory_owner(
    runtime: Rc<dyn ExchangeRuntime>,
) -> Rc<safe_cex_exchange::exchange::BaseExchange> {
    Rc::new(safe_cex_exchange::exchange::BaseExchange::new(
        safe_cex_types::ExchangeName::Polymarket,
        serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        runtime,
        safe_cex_exchange::exchange::ExchangeSettings::default(),
    ))
}

#[test]
fn http_factory_excludes_signing_captures_and_keeps_null_response_rejection_identity() {
    use safe_cex_exchange::js_runtime::client::factory;
    use safe_cex_transport::DirectTransport;
    let runtime = Rc::new(Runtime::default());
    let owner = factory_owner(runtime.clone());
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
    });
    let xhr = factory::create(
        http,
        &DirectTransport,
        Js::object(vec![("baseURL", Js::str("https://venue"))]),
        &Js::object(vec![("key", Js::str("")), ("secret", Js::str(""))]),
        &owner,
    )
    .unwrap();
    for capture in [false, true] {
        let config = Js::object(vec![("__tealstreetSigningCapture", Js::Bool(capture))]);
        settle(
            &runtime,
            xhr.call_method("get", vec![Js::str("/ok"), config])
                .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(owner.store.request_stats().unwrap().ok, 1.0);
    let error =
        safe_cex_exchange::js_runtime::value::error_object("AxiosError", "connection refused");
    error.define("response", Js::Null).unwrap();
    let original = error.clone();
    let adapter = Js::function("networkFailure", move |_, args| {
        error.define("config", arg(&args, 0))?;
        Err(Thrown::new(error.clone()))
    });
    let config = Js::object(vec![("adapter", adapter)]);
    let rejected = settle(
        &runtime,
        xhr.call_method("get", vec![Js::str("/network"), config])
            .unwrap(),
    )
    .unwrap_err();
    assert!(
        rejected.value.strict_equals(&original),
        "the same error reaches venue handlers"
    );
    assert_eq!(owner.store.request_stats().unwrap().network, 1.0);
    assert_eq!(owner.store.request_stats().unwrap().other, 0.0);
}

#[test]
fn rate_limit_dequeues_before_signing_and_keeps_interceptor_registration_order() {
    let runtime = Rc::new(Runtime::default());
    runtime.clock.set(1000.0);
    let http = Rc::new(Http {
        runtime: runtime.clone(),
        sent: RefCell::default(),
    });
    let xhr = client::create(http.clone(), Js::object(vec![]));
    let request = xhr.get("interceptors").unwrap().get("request").unwrap();
    let signing_clock = runtime.clone();
    request
        .call_method(
            "use",
            vec![Js::function("sign", move |_, args| {
                let config = arg(&args, 0);
                config.get("headers")?.call_method(
                    "set",
                    vec![Js::str("X-Nonce"), Js::Number(signing_clock.clock.get())],
                )?;
                Ok(config)
            })],
        )
        .unwrap();
    xhr.call_method(
        "__configureRateLimit",
        vec![Js::object(vec![
            ("maxRequests", Js::Number(1.0)),
            ("perMilliseconds", Js::Number(25.0)),
        ])],
    )
    .unwrap();
    let observed = Rc::new(RefCell::new(vec![]));
    let capture = observed.clone();
    let callback_clock = runtime.clone();
    request
        .call_method(
            "use",
            vec![Js::function("registeredAfterLimiter", move |_, args| {
                capture.borrow_mut().push(callback_clock.clock.get());
                Ok(arg(&args, 0))
            })],
        )
        .unwrap();
    for path in ["/first", "/second"] {
        settle(
            &runtime,
            xhr.call_method("get", vec![Js::str(path)]).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        *observed.borrow(),
        [1000.0, 1000.0],
        "later-registered callback runs before the limiter under Axios LIFO"
    );
    assert_eq!(runtime.clock.get(), 1025.0);
    let sent = http.sent.borrow();
    assert_eq!(sent[0]["headers"]["X-Nonce"], json!("1000"));
    assert_eq!(
        sent[1]["headers"]["X-Nonce"],
        json!("1025"),
        "queued request signs at dequeue, after its rate window advances"
    );
}

#[test]
fn installed_axios_rate_limit_orders_the_gate_before_signing() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rate-limit-signing-source.cjs"
        ))
        .output()
        .expect("installed Axios source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[path = "support/js_socket_receive.rs"]
mod socket_receive;

#[test]
fn promise_combinators_consume_live_iterables_and_reject_iterator_failures() {
    use safe_cex_exchange::js_runtime::value::from_json;
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_promise_iterables_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(rows.len(), 18);
    for row in rows {
        let (runtime, shared) = scoped_globals();
        let promise = shared.globals.get("Promise").unwrap();
        let resolved = |n| promise.call_method("resolve", vec![Js::Number(n)]).unwrap();
        let map = Js::new_map();
        let set = Js::new_set();
        let input = match row["mode"].as_str().unwrap() {
            "array" => Js::array(vec![resolved(1.0), resolved(2.0)]),
            "map-values" => {
                for (key, n) in [("a", 1.0), ("b", 2.0)] {
                    map.call_method("set", vec![Js::str(key), resolved(n)])
                        .unwrap();
                }
                map.call_method("values", vec![]).unwrap()
            }
            "set" => {
                for n in [1.0, 2.0] {
                    set.call_method("add", vec![resolved(n)]).unwrap();
                }
                set
            }
            "string" => Js::str("a😀"),
            "map-entries" => {
                for (key, n) in [("a", 1.0), ("b", 2.0)] {
                    map.call_method("set", vec![Js::str(key), Js::Number(n)])
                        .unwrap();
                }
                map
            }
            "empty" => set,
            "rejected-set" => {
                set.call_method("add", vec![resolved(1.0)]).unwrap();
                set.call_method(
                    "add",
                    vec![
                        promise
                            .call_method("reject", vec![Js::str("sentinel")])
                            .unwrap(),
                    ],
                )
                .unwrap();
                set
            }
            "noniterable" => Js::object(vec![]),
            "next-throw" => {
                let iterator = map.call_method("values", vec![]).unwrap();
                iterator
                    .define(
                        "next",
                        Js::function("next", |_, _| {
                            Err(Thrown::error("RangeError", "iterator sentinel"))
                        }),
                    )
                    .unwrap();
                iterator
            }
            _ => unreachable!(),
        };
        let method = row["method"].as_str().unwrap();
        let result = promise
            .call_method(method, vec![input])
            .expect("iterator failures reject a Promise, not the call");
        assert!(
            matches!(result, Js::Promise(_)),
            "combinator returns Promise"
        );
        let actual = match settle(&runtime, result) {
            Ok(value) => json!({"fulfilled":to_json(&value).unwrap()}),
            Err(error) if error.value.is_string() => {
                json!({"rejected":to_json(&error.value).unwrap()})
            }
            Err(error) => {
                let error = error.to_exchange_error();
                if error.name == "RangeError" {
                    json!({"rejected":{"name":error.name,"message":error.message}})
                } else {
                    json!({"rejected":{"name":error.name}})
                }
            }
        };
        let expected = if row.get("fulfilled").is_some() {
            json!({"fulfilled":row["fulfilled"]})
        } else {
            json!({"rejected":row["rejected"]})
        };
        assert_eq!(
            actual,
            to_json(&from_json(&expected)).unwrap(),
            "Promise iterable source result: {row}"
        );
    }
}

#[test]
fn promise_combinators_keep_pending_values_and_native_identity() {
    for method in ["all", "allSettled"] {
        let (runtime, shared) = scoped_globals();
        let rt: Rc<dyn ExchangeRuntime> = runtime.clone();
        let pending = Promise::pending(&rt);
        let marker = Js::object(vec![]);
        let nan = f64::from_bits(0x7ff8_0000_0000_0042);
        let map = Js::new_map();
        for (key, value) in [
            ("pending", Js::Promise(pending.clone())),
            ("negative-zero", Js::Number(-0.0)),
            ("nan", Js::Number(nan)),
        ] {
            map.call_method("set", vec![Js::str(key), value]).unwrap();
        }
        let iterator = map.call_method("values", vec![]).unwrap();
        let returned = shared
            .globals
            .get("Promise")
            .unwrap()
            .call_method(method, vec![iterator.clone()])
            .unwrap();
        assert!(
            iterator
                .call_method("next", vec![])
                .unwrap()
                .get("done")
                .unwrap()
                .truthy(),
            "combinator consumes the iterator synchronously"
        );
        let Js::Promise(ref aggregate) = returned else {
            panic!("expected Promise")
        };
        assert!(
            aggregate.result().is_none(),
            "Promise combinator waits for iterable member"
        );
        pending.settle(Ok(marker.clone()));
        let result = settle(&runtime, returned).unwrap();
        let values = result.array_values().unwrap();
        assert_eq!(values.len(), 3);
        let value = |i: usize| {
            if method == "allSettled" {
                values[i].get("value").unwrap()
            } else {
                values[i].clone()
            }
        };
        assert!(value(0).same_reference(&marker));
        assert_eq!(
            value(1).to_number().unwrap().to_bits(),
            (-0.0_f64).to_bits()
        );
        assert_eq!(value(2).to_number().unwrap().to_bits(), nan.to_bits());
    }
}

#[path = "support/emitter_retained.rs"]
mod emitter_retained;

#[test]
fn retained_adapter_config_preserves_native_values_at_terminal_and_effective_config() {
    use safe_cex_exchange::js_runtime::value::from_retained_input;
    for url in ["/ok", "/down"] {
        let runtime = Rc::new(Runtime::default());
        let terminal = std::sync::Arc::new(Terminal::default());
        let xhr = client::create(
            client::rest_http_host(runtime.clone(), terminal.clone()),
            Js::object(vec![]),
        );
        let data = Js::object(vec![
            ("missing", Js::Undefined),
            ("text", Js::utf16(&[116, 112, 32, 0xd800])),
            ("nan", Js::Number(f64::from_bits(0x7ff8000000000042))),
            ("infinity", Js::Number(f64::INFINITY)),
            ("negativeZero", Js::Number(-0.0)),
            (
                "array",
                Js::array(vec![Js::Undefined, Js::Number(f64::NEG_INFINITY)]),
            ),
        ]);
        data.define_key(&[0xdfff], Js::utf16(&[0xd800, 120]))
            .unwrap();
        let result = settle(
            &runtime,
            xhr.call_method(
                "get",
                vec![
                    Js::str(url),
                    Js::object(vec![("nativeData", data), ("ownUndefined", Js::Undefined)]),
                ],
            )
            .unwrap(),
        );
        let effective = match result {
            Ok(response) => response.get("config").unwrap(),
            Err(error) => error.value.get("config").unwrap(),
        };
        let seen = terminal.seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "one actual terminal attempt");
        assert!(matches!(
            seen[0].config_extensions.get("ownUndefined"),
            Some(safe_cex_types::retained_json::RetainedJsonInput::Undefined)
        ));
        assert!(effective.has_own("ownUndefined"));
        assert!(effective.get("ownUndefined").unwrap().is_undefined());
        for native in [
            from_retained_input(seen[0].config_extensions["nativeData"].clone()),
            effective.get("nativeData").unwrap(),
        ] {
            assert_eq!(
                native.own_entries().unwrap().len(),
                7,
                "all retained members survive"
            );
            assert_eq!(
                native.get("text").unwrap().to_js_string().unwrap().as_ref(),
                &[116, 112, 32, 0xd800]
            );
            assert_eq!(
                native
                    .get_property(&Js::utf16(&[0xdfff]))
                    .unwrap()
                    .to_js_string()
                    .unwrap()
                    .as_ref(),
                &[0xd800, 120]
            );
            assert!(native.has_own("missing"));
            assert!(native.get("missing").unwrap().is_undefined());
            assert_eq!(
                native.get("nan").unwrap().as_f64().unwrap().to_bits(),
                0x7ff8000000000042
            );
            assert_eq!(
                native.get("infinity").unwrap().as_f64(),
                Some(f64::INFINITY)
            );
            assert_eq!(
                native
                    .get("negativeZero")
                    .unwrap()
                    .as_f64()
                    .unwrap()
                    .to_bits(),
                (-0.0_f64).to_bits()
            );
            let array = native.get("array").unwrap();
            assert!(array.get("0").unwrap().is_undefined());
            assert_eq!(array.get("1").unwrap().as_f64(), Some(f64::NEG_INFINITY));
        }
    }
}

#[test]
fn retained_rest_adapter_reaches_real_client_interceptor_before_json_projection() {
    use safe_cex_exchange::js_runtime::value::{from_retained_input, to_retained_input};
    use safe_cex_transport::{RestAdapter, RestBody, RestRequest};
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    fn body() -> Input {
        Input::Object(vec![
            (
                "nan".encode_utf16().collect(),
                Input::Float(f64::from_bits(0x7ff8_0000_0000_0042)),
            ),
            ("zero".encode_utf16().collect(), Input::Float(-0.0)),
            ("own".encode_utf16().collect(), Input::Undefined),
            (vec![0xd800], Input::Text(vec![0xd801])),
        ])
    }
    fn assert_body(input: Input) {
        let Input::Object(fields) = input else {
            panic!("retained adapter native object before JSON")
        };
        assert_eq!(fields.len(), 4, "retained adapter all own properties");
        let Input::Float(nan) = fields[0].1 else {
            panic!("retained adapter native NaN")
        };
        assert_eq!(
            nan.to_bits(),
            0x7ff8_0000_0000_0042,
            "retained adapter exact NaN payload"
        );
        let Input::Float(zero) = fields[1].1 else {
            panic!("retained adapter native zero")
        };
        assert_eq!(
            zero.to_bits(),
            (-0.0_f64).to_bits(),
            "retained adapter signed zero"
        );
        assert_eq!(
            fields[2].1,
            Input::Undefined,
            "retained adapter owns undefined"
        );
        assert_eq!(
            fields[3],
            (vec![0xd800], Input::Text(vec![0xd801])),
            "retained adapter exact UTF16 key and value"
        );
    }
    struct NativeHost(Rc<Runtime>);
    impl JsHttpHost for NativeHost {
        fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
            self.0.clone()
        }
        fn send(&self, config: Js) -> JsFuture<'static> {
            // The real client delivers native data to its host after interception.
            let data = to_retained_input(&config.get("data").unwrap()).unwrap();
            assert_body(data.clone());
            assert_eq!(
                String::from_utf8(RestBody::Untransformed(data).wire_bytes().unwrap()).unwrap(),
                r#"{"nan":null,"zero":0,"\ud800":"\ud801"}"#,
                "retained adapter physical JSON projection"
            );
            Box::pin(async {
                Ok(Js::object(vec![
                    ("status", Js::Number(207.0)),
                    ("data", from_retained_input(body())),
                ]))
            })
        }
    }
    let runtime = Rc::new(Runtime::default());
    let xhr = client::create(Rc::new(NativeHost(runtime.clone())), Js::object(vec![]));
    let calls = Rc::new(std::cell::Cell::new(0));
    let observed = calls.clone();
    xhr.get("interceptors")
        .unwrap()
        .get("request")
        .unwrap()
        .call_method(
            "use",
            vec![Js::function("native signing observer", move |_, args| {
                let config = arg(&args, 0);
                assert_body(to_retained_input(&config.get("data")?)?);
                observed.set(observed.get() + 1);
                Ok(config)
            })],
        )
        .unwrap();
    let adapter = Rc::new(client::JsRestAdapter::new(xhr));
    let worker_runtime: Rc<dyn ExchangeRuntime> = runtime.clone();
    let pending = Js::Promise(Promise::spawn(
        &worker_runtime,
        Box::pin(async move {
            let response = adapter
                .execute(RestRequest {
                    method: "post".into(),
                    url: "/retained".into(),
                    body: Some(RestBody::Untransformed(body())),
                    ..Default::default()
                })
                .await
                .map_err(|failure| adapter.thrown(failure))?;
            assert_eq!(
                response.status, 207,
                "retained adapter native response status"
            );
            Ok(from_retained_input(response.body.into_data()))
        }),
    ));
    let output = settle(&runtime, pending).unwrap();
    assert_eq!(calls.get(), 1, "retained adapter real interceptor count");
    assert_body(to_retained_input(&output).unwrap());
}

#[path = "support/raw_error_reporting.rs"]
mod raw_error_reporting;
#[test]
fn owned_callback_error_clone_preserves_independent_native_metadata() {
    use safe_cex_exchange::js_runtime::value::owned::{OwnedCallbackError, OwnedJsValue};

    let original = OwnedCallbackError::new(OwnedJsValue::object([
        ("name", OwnedJsValue::string("TypeError")),
        ("message", OwnedJsValue::string("signer refused")),
        (
            "code",
            OwnedJsValue::Array(vec![
                OwnedJsValue::Number(f64::from_bits(0x7ff8000000000042)),
                OwnedJsValue::Undefined,
                OwnedJsValue::Number(-0.0),
                OwnedJsValue::String(vec![0xd800, 120]),
            ]),
        ),
    ]));
    let copied = original.clone();
    let OwnedJsValue::Array(original_code) = &original.value()["code"] else {
        panic!("native code array")
    };
    let OwnedJsValue::Array(copied_code) = &copied.value()["code"] else {
        panic!("cloned native code array")
    };
    assert_ne!(original_code.as_ptr(), copied_code.as_ptr());
    drop(original);
    assert_eq!(copied.to_string(), "signer refused");
    assert_eq!(copied.value()["name"], OwnedJsValue::string("TypeError"));
    let OwnedJsValue::Array(code) = &copied.value()["code"] else {
        panic!("retained cloned native code array")
    };
    assert_eq!(code.len(), 4);
    let OwnedJsValue::Number(nan) = code[0] else {
        panic!("native NaN")
    };
    assert_eq!(nan.to_bits(), 0x7ff8000000000042);
    assert!(matches!(code[1], OwnedJsValue::Undefined));
    let OwnedJsValue::Number(zero) = code[2] else {
        panic!("native signed zero")
    };
    assert_eq!(zero.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(code[3], OwnedJsValue::String(vec![0xd800, 120]));
}

#[path = "support/retained_info.rs"]
mod retained_info;
