//! Test adapters over the actual production intrinsic dispatcher and JS values.
//! Fixture callbacks supply external operations; intrinsic semantics live in src.
#[path = "binding_harness.rs"]
#[allow(dead_code)]
mod binding;
#[path = "../../src/binding/intrinsics.rs"]
mod table;
use safe_cex_exchange::{
    js_runtime::{
        promise::Promise,
        value::{Js, Thrown},
    },
    runtime::ExchangeRuntime,
};
use serde_json::{Value, json};
use std::{
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
pub type V = Js;
pub type E = Thrown;
struct Host(Js);
impl table::IntrinsicHost for Host {
    fn this(&self) -> Js {
        self.0.clone()
    }
    fn initialize_okx_fields(&self, _: &Js) -> Result<(), Thrown> {
        panic!("unused fixture field initialization")
    }
}
pub fn intrinsic(receiver: Js, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    table::intrinsic(&Host(receiver), name, args)
}
pub fn s(value: &str) -> Js {
    Js::str(value)
}
pub fn n(value: f64) -> Js {
    Js::Number(value)
}
pub fn u() -> Js {
    Js::Undefined
}
pub fn b(value: bool) -> Js {
    Js::Bool(value)
}
pub fn array(values: Vec<Js>) -> Js {
    Js::array(values)
}
pub fn object(fields: Vec<(&str, Js)>) -> Js {
    Js::object(fields)
}
pub fn list(value: &Js) -> Vec<Js> {
    value
        .array_values()
        .expect("fixture array property reads")
        .expect("native fixture array")
}
pub fn get(value: &Js, key: &str) -> Js {
    value.get(key).expect("native fixture Get")
}
pub fn string(value: &Js) -> String {
    String::from_utf16(&value.to_js_string().expect("native fixture String"))
        .expect("fixture UTF8 domain")
}
pub fn numeric(value: &Js) -> f64 {
    value.to_number().expect("native fixture Number")
}
pub fn truthy(value: &Js) -> bool {
    value.truthy()
}
pub fn equal(a: &Js, b: &Js) -> bool {
    a.strict_equals(b)
}
pub fn fields(value: &Js) -> Vec<(String, Js)> {
    value
        .own_entries()
        .expect("fixture entries")
        .into_iter()
        .map(|(k, v)| (String::from_utf16(&k).unwrap(), v))
        .collect()
}
pub fn keys(value: &Js) -> Vec<String> {
    fields(value).into_iter().map(|(k, _)| k).collect()
}
pub fn named(name: &str) -> Js {
    let label = name.to_owned();
    Js::function(name, move |_, _| {
        panic!("fixture-only captured callee invoked outside script: {label}")
    })
}
pub fn name(value: &Js) -> String {
    let Js::Function(f) = value else {
        panic!("fixture native function")
    };
    String::from_utf16(&f.name).unwrap()
}
pub fn pack(value: &Js) -> Value {
    match value {
        Js::Undefined => json!({"u":true}),
        Js::Null => Value::Null,
        Js::Bool(b) => json!(b),
        Js::Number(n) => json!({"n":format!("{:016x}",n.to_bits())}),
        Js::String(s) => json!(String::from_utf16(s).unwrap()),
        Js::Array(_) => json!(list(value).iter().map(pack).collect::<Vec<_>>()),
        Js::Object(_) => {
            json!({"o":fields(value).into_iter().map(|(k,v)|json!([k,pack(&v)])).collect::<Vec<_>>()})
        }
        _ => panic!("fixture unpacked native value"),
    }
}
std::thread_local! {
    static RUNTIME: Rc<binding::Runtime> = Rc::default();
}
pub fn promise(result: Result<Js, Thrown>) -> Js {
    RUNTIME.with(|runtime| {
        let runtime: Rc<dyn ExchangeRuntime> = runtime.clone();
        Js::Promise(Promise::settled(&runtime, result))
    })
}
pub fn outcome(value: &Js) -> Result<Js, Thrown> {
    let Js::Promise(p) = value else {
        panic!("fixture native promise")
    };
    let mut awaited = Box::pin(p.wait());
    let mut context = Context::from_waker(Waker::noop());
    RUNTIME.with(|runtime| {
        assert!(awaited.as_mut().poll(&mut context).is_pending());
        runtime.pump();
        assert!(
            runtime.reports.borrow().is_empty(),
            "handled fixture rejection"
        );
        match awaited.as_mut().poll(&mut context) {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("fixture settled promise did not resume after checkpoint"),
        }
    })
}
pub fn failure() -> Thrown {
    Thrown::new(object(vec![
        ("name", s("TypeError")),
        ("message", s("down")),
        ("code", s("ORDERS")),
        ("ids", array(vec![n(8.), n(3.)])),
    ]))
}
pub fn assert_failure(actual: &Thrown, expected: &Thrown) {
    assert!(
        actual.value.same_reference(&expected.value),
        "ORDER native original error identity"
    );
    assert_eq!(string(&get(&actual.value, "name")), "TypeError");
    assert_eq!(actual.message(), "down");
    assert_eq!(string(&get(&actual.value, "code")), "ORDERS");
    assert_eq!(
        list(&get(&actual.value, "ids"))
            .iter()
            .map(numeric)
            .collect::<Vec<_>>(),
        [8., 3.]
    );
}
pub fn error_pack(error: &Thrown, original: &Thrown) -> Value {
    json!({"name":string(&get(&error.value,"name")),"message":error.message(),"code":string(&get(&error.value,"code")),"ids":list(&get(&error.value,"ids")).iter().map(|v|numeric(v) as u8).collect::<Vec<_>>(),"original":error.value.same_reference(&original.value)})
}
