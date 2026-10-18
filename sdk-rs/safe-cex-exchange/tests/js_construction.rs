use safe_cex_exchange::js_runtime::value::Js;
use std::{cell::Cell, rc::Rc};

#[test]
fn class_call_refuses_before_running_constructor() {
    let calls = Rc::new(Cell::new(0));
    let effects = calls.clone();
    let class = Js::constructor("SourceClass", move |_, _| {
        effects.set(effects.get() + 1);
        Ok(Js::object(vec![]))
    });
    let result = class.call(Js::Undefined, vec![]);
    assert!(
        result.is_err(),
        "JS_CONSTRUCT class call refuses before constructor body"
    );
    assert_eq!(
        calls.get(),
        0,
        "JS_CONSTRUCT class call has no construction effects"
    );
    assert_eq!(
        result.unwrap_err().value.get("name").unwrap().to_display(),
        "TypeError"
    );
}
