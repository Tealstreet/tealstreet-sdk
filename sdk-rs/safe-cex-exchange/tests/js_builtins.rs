//! Array pop/reverse, the dayjs utc-plugin facade, Object.getOwnPropertyNames,
//! JSON.stringify property lists and console.log, compared with the installed
//! engine and dayjs through the production global tables.
use safe_cex_exchange::{
    js_runtime::{
        client::JsHttpHost,
        globals, library,
        promise::JsFuture,
        scope::ScopeShared,
        value::{Js, from_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};

#[derive(Default)]
struct Runtime {
    console: RefCell<Vec<(String, String)>>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        1000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected timer")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected task")
    }
    fn console_values(&self, severity: &str, values: &[Js]) {
        self.console.borrow_mut().push((
            severity.into(),
            safe_cex_exchange::runtime::console_text(values),
        ));
    }
    fn unique_id(&self) -> String {
        "fixture".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_exception(&self, e: &ExchangeError, _: &Value) {
        panic!("unexpected error: {}", e.message)
    }
    fn report_raw_exception(&self, _: &Js, _: &Value) {
        panic!("unexpected raw error")
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
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
        panic!("unexpected dial")
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
struct Http(Rc<Runtime>);
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _: Js) -> JsFuture<'static> {
        panic!("unexpected HTTP")
    }
}

fn fixture() -> (Rc<Runtime>, Rc<ScopeShared<()>>) {
    let runtime = Rc::new(Runtime::default());
    let shared = Rc::new(ScopeShared::new(
        runtime.clone(),
        Rc::new(Http(runtime.clone())),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        Js::object(vec![]),
        (),
    ));
    globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
    library::install_library(&shared.globals, Rc::downgrade(&shared));
    (runtime, shared)
}

/// serde_json keeps `1` and `1.0` distinct; the engine has one Number type.
fn floats(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.iter().map(floats).collect()),
        Value::Object(map) => {
            Value::Object(map.iter().map(|(k, v)| (k.clone(), floats(v))).collect())
        }
        other => other.clone(),
    }
}

fn oracle() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_builtins_oracle.cjs"
        ))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn global(shared: &ScopeShared<()>, path: &str) -> Js {
    let mut parts = path.split('.');
    let mut value = shared.globals.get(parts.next().unwrap()).unwrap();
    for part in parts {
        value = value.get(part).unwrap();
    }
    value
}

fn holey(cells: &Value) -> Js {
    Js::holey_array(
        cells
            .as_array()
            .unwrap()
            .iter()
            .map(|cell| (cell[0] == true).then(|| from_json(&cell[1])))
            .collect(),
    )
}

fn cells(value: &Js) -> Value {
    Value::Array(
        value
            .array_items()
            .unwrap()
            .into_iter()
            .map(|cell| match cell {
                Some(v) => json!([
                    true,
                    safe_cex_exchange::js_runtime::value::to_json(&v).unwrap()
                ]),
                None => json!([false]),
            })
            .collect(),
    )
}

/// The oracle spells NaN and negative zero as strings; keep both exact.
fn number(value: &Js) -> Value {
    let n = value.as_f64().expect("numeric dayjs observation");
    if n.is_nan() {
        json!("NaN")
    } else if n == 0.0 && n.is_sign_negative() {
        json!("-0")
    } else {
        json!(n)
    }
}

fn argument(value: &Value) -> Js {
    match value {
        Value::Null => Js::Undefined,
        Value::String(s) if s == "NaN" => Js::Number(f64::NAN),
        Value::String(s) if s == "-0" => Js::Number(-0.0),
        other => from_json(other),
    }
}

#[test]
fn array_pop_and_reverse_match_engine_holes_results_and_receiver() {
    let cases = oracle()["array"].clone();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 16, "array source case floor");
    for case in cases {
        let input = holey(&case["input"]);
        let kind = case["kind"].as_str().unwrap();
        let result = input.call_method(kind, vec![]).unwrap();
        let observed = match kind {
            "pop" if result.is_undefined() => json!(["undefined"]),
            "pop" => json!([
                "value",
                safe_cex_exchange::js_runtime::value::to_json(&result).unwrap()
            ]),
            _ => json!(["self", result.same_reference(&input)]),
        };
        assert_eq!(floats(&observed), floats(&case["result"]), "{case}");
        assert_eq!(floats(&cells(&input)), floats(&case["after"]), "{case}");
    }
}

#[test]
fn dayjs_utc_offset_and_start_of_match_installed_dayjs_utc_plugin() {
    let (_, shared) = fixture();
    let dayjs = global(&shared, "dayjs");
    let cases = oracle()["dates"].clone();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 420, "dayjs source chain floor");
    for case in cases {
        let observe = |d: &Js| {
            json!({
                "valueOf": number(&d.call_method("valueOf", vec![]).unwrap()),
                "utcOffset": number(&d.call_method("utcOffset", vec![]).unwrap()),
                "unix": number(&d.call_method("unix", vec![]).unwrap()),
            })
        };
        let mut d = dayjs
            .call(Js::Undefined, vec![argument(&case["start"])])
            .unwrap();
        let mut trace = vec![observe(&d)];
        for step in case["steps"].as_array().unwrap() {
            let step = step.as_array().unwrap();
            let op = step[0].as_str().unwrap();
            let args = step[1..].iter().map(argument).collect();
            d = if op == "clone" {
                dayjs.call(Js::Undefined, vec![d]).unwrap()
            } else {
                d.call_method(op, args).unwrap()
            };
            trace.push(observe(&d));
        }
        assert_eq!(
            floats(&Value::Array(trace)),
            floats(&case["trace"]),
            "{case}"
        );
    }
}

#[test]
fn object_get_own_property_names_and_stringify_property_lists_match_engine() {
    let (_, shared) = fixture();
    let oracle = oracle();
    let names = global(&shared, "Object.getOwnPropertyNames");
    let define = global(&shared, "Object.defineProperty");
    let stringify = global(&shared, "JSON.stringify");
    let plain = || {
        let value = from_json(&json!({"b": 1, "2": "two", "a": 2, "1": "one"}));
        define
            .call(
                Js::Undefined,
                vec![
                    value.clone(),
                    Js::str("hidden"),
                    from_json(&json!({"value": 3, "enumerable": false})),
                ],
            )
            .unwrap();
        value
    };
    let tree = || {
        from_json(&json!({"a": 1, "b": {"a": 2, "c": 3}, "c": [{"a": 4, "b": 5}, 6], "1": "one"}))
    };
    let list = |v: Js| safe_cex_exchange::js_runtime::value::to_json(&v).unwrap();
    let inputs = [
        ("object", plain()),
        ("array", holey(&json!([[true, 1], [false], [true, 3]]))),
        ("string", Js::str("ab")),
        ("number", Js::Number(5.0)),
        ("bool", Js::Bool(true)),
    ];
    let observed: Vec<Value> = inputs
        .into_iter()
        .map(|(label, input)| json!([label, list(names.call(Js::Undefined, vec![input]).unwrap())]))
        .collect();
    assert_eq!(Value::Array(observed), oracle["names"]);
    let error = names.call(Js::Undefined, vec![Js::Null]).unwrap_err();
    assert_eq!(
        safe_cex_exchange::js_runtime::value::to_json(&error.value.get("name").unwrap()).unwrap(),
        oracle["nullish"]
    );

    let to_json_member = Js::function("toJSON", |_, _| Ok(from_json(&json!({"a": 7, "z": 8}))));
    let with_to_json = Js::object(vec![
        ("a", Js::object(vec![("toJSON", to_json_member)])),
        ("z", Js::Number(9.0)),
    ]);
    let hidden_keys = names.call(Js::Undefined, vec![plain()]).unwrap();
    let cases = [
        ("keys", tree(), from_json(&json!(["a", "c"]))),
        (
            "numeric-and-duplicates",
            tree(),
            Js::array(vec![
                Js::Number(1.0),
                Js::str("1"),
                Js::object(vec![]),
                Js::Null,
                Js::str("a"),
                Js::str("a"),
                Js::Bool(true),
            ]),
        ),
        ("empty-list", tree(), Js::array(vec![])),
        ("hidden-listed", plain(), hidden_keys),
        (
            "holey-list",
            tree(),
            Js::holey_array(vec![Some(Js::str("c")), None, Some(Js::str("a"))]),
        ),
        ("to-json", with_to_json, from_json(&json!(["a"]))),
        (
            "missing-key",
            from_json(&json!({"a": 1})),
            from_json(&json!(["a", "missing"])),
        ),
    ];
    let observed: Vec<Value> = cases
        .into_iter()
        .map(|(label, value, replacer)| {
            json!([
                label,
                list(
                    stringify
                        .call(Js::Undefined, vec![value, replacer])
                        .unwrap()
                )
            ])
        })
        .collect();
    assert_eq!(Value::Array(observed), oracle["stringify"]);
}

#[test]
fn console_log_reaches_the_exchange_runtime_console() {
    let (runtime, shared) = fixture();
    let log = global(&shared, "console.log");
    let result = log
        .call(
            Js::Undefined,
            vec![Js::str("Unknown data"), Js::Number(1.0)],
        )
        .unwrap();
    assert!(result.is_undefined());
    assert_eq!(
        *runtime.console.borrow(),
        vec![("log".to_owned(), "Unknown data 1".to_owned())]
    );
}
