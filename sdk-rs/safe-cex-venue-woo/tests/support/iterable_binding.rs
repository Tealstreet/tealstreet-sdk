//! Intrinsic language operations from the actual constructed WOO host.
use crate::binding_test_harness as harness;
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_types::ExchangeName;
use std::{cell::Cell, rc::Rc};

#[test]
fn woo_host_spread_accepts_iterator_objects_and_preserves_native_values() {
    let h = harness::harness(ExchangeName::Woo, harness::options(), |_| {});
    let ops = crate::binding::ops::canonical(Rc::downgrade(&h.exchange.rt.shared));
    let reference = Js::object(vec![("id", Js::str("native"))]);
    let map = Js::new_map();
    map.call_method("set", vec![Js::str("first"), reference.clone()])
        .unwrap();
    map.call_method("set", vec![Js::str("second"), Js::Number(-0.0)])
        .unwrap();
    let values = map.call_method("values", vec![]).unwrap();
    let rows = (ops.spread)(&values).expect("constructed WOO spread must consume Map.values");
    assert_eq!(rows.len(), 2);
    assert!(rows[0].strict_equals(&reference));
    let Js::Number(zero) = rows[1] else {
        panic!("native number")
    };
    assert_eq!(zero.to_bits(), (-0.0f64).to_bits());
    assert!((ops.spread)(&values).unwrap().is_empty());
    let text = (ops.spread)(&Js::utf16(&[65, 0xd83d, 0xde00, 0xd800])).unwrap();
    assert_eq!(text.len(), 3);
    assert!(text[1].strict_equals(&Js::str("😀")));
    assert!(
        text[2].strict_equals(&Js::utf16(&[0xd800])),
        "lone UTF-16 unit stays native"
    );
}

#[test]
fn woo_host_for_of_observes_collection_changes_during_the_body() {
    let h = harness::harness(ExchangeName::Woo, harness::options(), |_| {});
    let ops =
        crate::binding::ops::lifecycle(Rc::downgrade(&h.exchange.rt.shared), h.runtime.clone());
    let set = Js::new_set();
    for n in [1.0, 2.0] {
        set.call_method("add", vec![Js::Number(n)]).unwrap();
    }
    let mut seen = vec![];
    (ops.iterate)(set.clone(), &mut |value| {
        let n = value.to_number()?;
        seen.push(n);
        if n == 1.0 {
            set.call_method("delete", vec![Js::Number(2.0)])?;
            set.call_method("add", vec![Js::Number(3.0)])?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(
        seen,
        vec![1.0, 3.0],
        "constructed WOO for-of must observe live Set mutation"
    );
}

#[test]
fn woo_host_array_pattern_closes_without_consuming_the_unused_tail() {
    let h = harness::harness(ExchangeName::Woo, harness::options(), |_| {});
    let ops =
        crate::binding::ops::lifecycle(Rc::downgrade(&h.exchange.rt.shared), h.runtime.clone());
    let values = Js::array(vec![Js::Number(1.0), Js::Number(2.0), Js::Number(3.0)])
        .iterator()
        .unwrap();
    let closed = Rc::new(Cell::new(0));
    let count = closed.clone();
    values
        .define(
            "return",
            Js::function("return", move |_, _| {
                count.set(count.get() + 1);
                Ok(Js::object(vec![]))
            }),
        )
        .unwrap();
    let rows = (ops.binding)(values.clone(), &[false, true])
        .expect("constructed WOO pattern must consume iterator");
    assert_eq!(rows.len(), 2);
    assert!(rows[0].is_undefined());
    assert!(rows[1].strict_equals(&Js::Number(2.0)));
    assert_eq!(closed.get(), 1);
    let tail = values.call_method("next", vec![]).unwrap();
    assert!(
        tail.get("value").unwrap().strict_equals(&Js::Number(3.0)),
        "pattern must leave the tail unconsumed"
    );
}

#[test]
fn woo_host_for_of_closes_on_body_throw_with_original_error_precedence() {
    use safe_cex_exchange::js_runtime::value::Thrown;
    let h = harness::harness(ExchangeName::Woo, harness::options(), |_| {});
    let ops =
        crate::binding::ops::lifecycle(Rc::downgrade(&h.exchange.rt.shared), h.runtime.clone());
    let values = Js::array(vec![Js::Number(1.0)]).iterator().unwrap();
    let closed = Rc::new(Cell::new(0));
    let count = closed.clone();
    values
        .define(
            "return",
            Js::function("return", move |_, _| {
                count.set(count.get() + 1);
                Err(Thrown::type_error(
                    "cleanup must not replace original error",
                ))
            }),
        )
        .unwrap();
    let original = Js::object(vec![("tag", Js::str("body"))]);
    let error = (ops.iterate)(values, &mut |_| Err(Thrown::new(original.clone()))).unwrap_err();
    assert!(
        error.value.strict_equals(&original),
        "production for-of must retain original body failure"
    );
    assert_eq!(closed.get(), 1);
}

#[test]
fn woo_host_array_pattern_elisions_skip_value_and_step_errors_do_not_close() {
    use safe_cex_exchange::js_runtime::value::Thrown;
    let h = harness::harness(ExchangeName::Woo, harness::options(), |_| {});
    let ops =
        crate::binding::ops::lifecycle(Rc::downgrade(&h.exchange.rt.shared), h.runtime.clone());
    for phase in ["next", "done", "value", "elision"] {
        let values = Js::array(vec![]).iterator().unwrap();
        let original = Js::object(vec![("phase", Js::str(phase))]);
        let thrown = original.clone();
        let result = Js::object(vec![]);
        let done_error = original.clone();
        result
            .define_accessor(
                "done",
                Some(Rc::new(move |_| {
                    if phase == "done" {
                        Err(Thrown::new(done_error.clone()))
                    } else {
                        Ok(Js::Bool(false))
                    }
                })),
                None,
            )
            .expect("fresh test accessor target");
        result
            .define_accessor(
                "value",
                Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
                None,
            )
            .expect("fresh test accessor target");
        let next_error = original.clone();
        values
            .define(
                "next",
                Js::function("next", move |_, _| {
                    if phase == "next" {
                        Err(Thrown::new(next_error.clone()))
                    } else {
                        Ok(result.clone())
                    }
                }),
            )
            .unwrap();
        let closed = Rc::new(Cell::new(0));
        let count = closed.clone();
        values
            .define(
                "return",
                Js::function("return", move |_, _| {
                    count.set(count.get() + 1);
                    Ok(Js::object(vec![]))
                }),
            )
            .unwrap();
        let output = (ops.binding)(values, &[phase != "elision"]);
        if phase == "elision" {
            assert!(output.unwrap()[0].is_undefined());
            assert_eq!(
                closed.get(),
                1,
                "partial pattern closes without reading skipped value"
            );
        } else {
            assert!(output.unwrap_err().value.strict_equals(&original));
            assert_eq!(
                closed.get(),
                0,
                "abrupt iterator step/value never closes: {phase}"
            );
        }
    }
    let values = Js::array(vec![]).iterator().unwrap();
    values
        .define("return", Js::function("return", |_, _| Ok(Js::Number(1.0))))
        .unwrap();
    let error = (ops.binding)(values, &[]).unwrap_err();
    assert_eq!(
        error.value.get("name").unwrap().to_display(),
        "TypeError",
        "normal pattern close requires an object result"
    );
}
