use super::{Realm, test_runtime::TestRuntime, value::*};
use std::rc::Rc;

fn realm() -> (Realm, Rc<TestRuntime>) {
    let runtime = Rc::new(TestRuntime::default());
    (Realm::new(runtime.clone()), runtime)
}
fn text(r: &mut Realm, v: &Value) -> String {
    r.to_rust_string(v).unwrap()
}
fn message(r: &mut Realm, e: &Value) -> String {
    let m = r.get_str(e, "message").unwrap();
    text(r, &m)
}

#[test]
fn property_reads_follow_the_prototype_chain_and_nullish_reads_throw() {
    let (mut r, _) = realm();
    let o = r.object_from(vec![("a", Value::Number(1.0))]);
    assert!(r.get_str(&o, "a").unwrap().strict_equal(&Value::Number(1.0)));
    let has = r.get_str(&o, "hasOwnProperty").unwrap();
    assert!(r.call(&has, &o, &[Value::str("a")]).unwrap().truthy());
    let e = r.get_str(&Value::Undefined, "x").unwrap_err();
    assert_eq!(message(&mut r, &e), "Cannot read properties of undefined (reading 'x')");
    let name = r.get_str(&e, "name").unwrap();
    assert_eq!(text(&mut r, &name), "TypeError");
}

#[test]
fn array_methods_and_exotic_length() {
    let (mut r, _) = realm();
    let a = r.new_array(vec![Value::Number(3.0), Value::Number(1.0), Value::Number(2.0)]);
    let doubled = r.native("", 1, |r, _, args| r.binary("*", &args[0], &Value::Number(2.0)));
    let mapped = r.call_method(&a, "map", &[doubled]).unwrap();
    let joined = r.call_method(&mapped, "join", &[Value::str("-")]).unwrap();
    assert_eq!(text(&mut r, &joined), "6-2-4");
    r.call_method(&a, "push", &[Value::Number(9.0)]).unwrap();
    let len = r.get_str(&a, "length").unwrap();
    assert_eq!(len.as_number(), Some(4.0));
    r.set_str(&a, "length", Value::Number(1.0)).unwrap();
    let s = r.call_method(&a, "toString", &[]).unwrap();
    assert_eq!(text(&mut r, &s), "3");
    let parts = r.call_method(&Value::str("15m"), "split", &[r.regexp(r"/(\d+)/").unwrap()]).unwrap();
    let joined = r.call_method(&parts, "join", &[Value::str("|")]).unwrap();
    assert_eq!(text(&mut r, &joined), "|15|m");
}

#[test]
fn map_set_iteration_is_live_and_insertion_ordered() {
    let (mut r, _) = realm();
    let map_ctor = r.global_binding("Map").unwrap();
    let m = r.construct(&map_ctor, &[], None).unwrap();
    r.call_method(&m, "set", &[Value::str("a"), Value::Number(1.0)]).unwrap();
    r.call_method(&m, "set", &[Value::str("b"), Value::Number(2.0)]).unwrap();
    let it = r.get_iterator(&m).unwrap();
    let first = r.iterator_step(&it).unwrap().unwrap();
    r.call_method(&m, "set", &[Value::str("c"), Value::Number(3.0)]).unwrap();
    let k = r.get_str(&first, "0").unwrap();
    let mut keys = vec![text(&mut r, &k)];
    while let Some(e) = r.iterator_step(&it).unwrap() {
        let k = r.get_str(&e, "0").unwrap();
        keys.push(text(&mut r, &k));
    }
    assert_eq!(keys, ["a", "b", "c"]);
}

#[test]
fn json_round_trip_keeps_key_order_and_js_numbers() {
    let (mut r, _) = realm();
    let units: Vec<u16> = r#"{"b":1,"a":[1.5,null,"x"],"2":true,"1":-0}"#.encode_utf16().collect();
    let v = r.parse_json(&units).unwrap();
    let out = r.stringify(&v).unwrap().unwrap();
    // Integer keys enumerate first in ascending order, as in V8; -0 prints as 0.
    assert_eq!(String::from_utf16_lossy(&out), r#"{"1":0,"2":true,"b":1,"a":[1.5,null,"x"]}"#);
    let bad: Vec<u16> = "{oops".encode_utf16().collect();
    let e = r.parse_json(&bad).unwrap_err();
    let name = r.get_str(&e, "name").unwrap();
    assert_eq!(text(&mut r, &name), "SyntaxError");
}

#[test]
fn promises_adopt_and_then_chains_settle_on_the_runtime() {
    let (mut r, rt) = realm();
    let promise = r.global_binding("Promise").unwrap();
    let inner = r.call_method(&promise, "resolve", &[Value::Number(2.0)]).unwrap();
    let add = r.native("", 1, |r, _, args| r.binary("+", &args[0], &Value::Number(40.0)));
    let chained = r.call_method(&inner, "then", &[add]).unwrap();
    let outer = r.spawn_value(r.await_value(chained));
    rt.drain();
    let cell = outer.promise().unwrap();
    match &cell.borrow().state {
        PromiseState::Fulfilled(v) => assert_eq!(v.as_number(), Some(42.0)),
        _ => panic!("promise did not fulfil"),
    };
}

#[test]
fn timers_fire_on_the_virtual_clock_and_clear_cancels() {
    let (mut r, rt) = realm();
    let hits = Rc::new(std::cell::Cell::new(0));
    let h = hits.clone();
    let cb = r.native("", 0, move |_, _, _| {
        h.set(h.get() + 1);
        Ok(Value::Undefined)
    });
    let set_timeout = r.global_binding("setTimeout").unwrap();
    let clear = r.global_binding("clearTimeout").unwrap();
    r.call(&set_timeout, &Value::Undefined, &[cb.clone(), Value::Number(100.0)]).unwrap();
    let id = r.call(&set_timeout, &Value::Undefined, &[cb, Value::Number(50.0)]).unwrap();
    r.call(&clear, &Value::Undefined, &[id]).unwrap();
    rt.drain();
    assert_eq!(hits.get(), 1);
    assert_eq!(rt.now.get(), 100.0);
}

#[test]
fn operators_follow_abstract_semantics() {
    let (mut r, _) = realm();
    let sum = r.binary("+", &Value::str("a"), &Value::Number(1.0)).unwrap();
    assert_eq!(text(&mut r, &sum), "a1");
    assert!(r.binary("<", &Value::str("10"), &Value::Number(9.0)).unwrap().strict_equal(&Value::Bool(false)));
    assert!(r.binary(">=", &Value::Number(f64::NAN), &Value::Number(1.0)).unwrap().strict_equal(&Value::Bool(false)));
    assert!(r.binary("==", &Value::Null, &Value::Undefined).unwrap().truthy());
    let e = r.error("boom");
    let s = r.to_string(&e).unwrap();
    assert_eq!(String::from_utf16_lossy(&s), "Error: boom");
}
