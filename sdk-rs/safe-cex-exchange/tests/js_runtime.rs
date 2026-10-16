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
    clock: std::cell::Cell<f64>,
    consoled: RefCell<Vec<(String, String)>>,
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
        0.0
    }
    fn sleep(&self, ms: u64) -> ExchangeFuture<'static, ()> {
        self.clock.set(self.clock.get() + ms as f64);
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console(&self, severity: &str, message: &str) {
        self.consoled
            .borrow_mut()
            .push((severity.into(), message.into()));
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
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn report_raw_exception(&self, _: &Value, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Ok(Value::Null) })
    }
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
        _: String,
        _: Vec<String>,
        _: safe_cex_exchange::websocket::WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn safe_cex_exchange::websocket::WsSocket>> {
        panic!("no sockets in js_runtime tests")
    }
    fn headers_capable(&self) -> bool {
        false
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
                            Ok(safe_cex_exchange::js_runtime::value::from_json(
                                &json!({"status": response.status, "body": body}),
                            ))
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
        assert_eq!(seen[0].params, Some(json!({"instId": "BTC-USDT-SWAP"})));
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
        json_of(&g, map.call_method("entries", vec![]).unwrap()),
        r#"[["a",3],["b",2]]"#
    );
    // A Map source copies its entries; a Set of entry arrays works too.
    let copy = new_map(vec![map.clone()]).unwrap();
    assert!(!copy.same_reference(&map));
    assert_eq!(
        json_of(&g, copy.call_method("entries", vec![]).unwrap()),
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
    let key = zero.call_method("keys", vec![]).unwrap().get("0").unwrap();
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
fn console_error_and_warn_join_string_conversions_with_spaces() {
    let (runtime, shared) = scoped_globals();
    let g = &shared.globals;
    run(
        g,
        "console.error",
        vec![
            Js::str("reject in updatePositionsInWebworker"),
            Js::Number(1.0),
            Js::object(vec![("a", Js::Number(1.0))]),
            Js::array(vec![Js::Number(1.0), Js::Null, Js::Number(2.0)]),
            Js::Undefined,
        ],
    );
    run(
        g,
        "console.warn",
        vec![Js::str("No positions found for BTC")],
    );
    run(g, "console.warn", vec![]);
    assert_eq!(
        *runtime.consoled.borrow(),
        [
            (
                "error".to_owned(),
                "reject in updatePositionsInWebworker 1 [object Object] 1,,2 undefined".to_owned()
            ),
            ("warn".to_owned(), "No positions found for BTC".to_owned()),
            ("warn".to_owned(), String::new()),
        ]
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
