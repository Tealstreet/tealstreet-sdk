#![cfg(not(target_arch = "wasm32"))]
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_registry::native_value::NativeValue;

#[test]
fn native_value_capture_refuses_excessive_nesting() {
    let mut value = Js::Null;
    for _ in 0..513 {
        value = Js::array(vec![value]);
    }
    let error = NativeValue::from_js(&value).expect_err("native capture must bound recursion");
    assert_eq!(error.message, "native account value nesting exceeds 512");
}

#[test]
fn native_value_capture_refuses_object_and_array_cycles() {
    for value in [Js::object(vec![]), Js::array(vec![])] {
        value.set("0", value.clone()).unwrap();
        let error = NativeValue::from_js(&value).expect_err("cycles cannot form an owned tree");
        assert_eq!(error.message, "native account value contains a cycle");
        value.set("0", Js::Undefined).unwrap();
    }
}

#[test]
fn native_value_capture_accepts_shared_references_and_the_depth_limit() {
    let shared = Js::object(vec![("value", Js::Number(1.0))]);
    let captured = NativeValue::from_js(&Js::array(vec![shared.clone(), shared])).unwrap();
    assert_eq!(captured[0], captured[1]);
    let mut value = Js::Null;
    for _ in 0..512 {
        value = Js::array(vec![value]);
    }
    NativeValue::from_js(&value).unwrap();
}
