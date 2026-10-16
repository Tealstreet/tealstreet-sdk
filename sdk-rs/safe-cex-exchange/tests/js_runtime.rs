//! js_runtime semantics a venue binding depends on, over a recording runtime.
use safe_cex_exchange::{
    js_runtime::{
        client::{self, JsHttpHost},
        promise::{JsFuture, Promise, await_js},
        value::{Js, Thrown, arg, to_json},
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
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
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
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console(&self, _: &str, _: &str) {}
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
