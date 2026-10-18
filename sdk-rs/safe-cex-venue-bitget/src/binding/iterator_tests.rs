//! The actual private-topic host on a constructed production exchange.
#[path = "../../tests/support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;

use crate::private_topics::PrivateTopicsHost;
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_types::ExchangeName;
use std::rc::Rc;

#[test]
fn private_host_accepts_native_iterator_objects() {
    let h = harness::harness(ExchangeName::Bitget, harness::options(), |_| {});
    let rt = &h.exchange.rt;
    let reference = Js::object(vec![("id", Js::str("position"))]);
    let map = Js::new_map();
    map.call_method("set", vec![Js::str("first"), reference.clone()])
        .unwrap();
    map.call_method("set", vec![Js::str("second"), Js::Number(-0.0)])
        .unwrap();
    let values = map.call_method("values", vec![]).unwrap();
    let mut iterator = PrivateTopicsHost::iterator(rt, &values)
        .expect("real private host must admit Map.values iterator");
    let first = PrivateTopicsHost::next(rt, &mut iterator).unwrap().unwrap();
    assert!(
        first.strict_equals(&reference),
        "iterator keeps native value identity"
    );
    let Js::Number(zero) = PrivateTopicsHost::next(rt, &mut iterator).unwrap().unwrap() else {
        panic!("native number")
    };
    assert_eq!(zero.to_bits(), (-0.0f64).to_bits());
    assert!(
        PrivateTopicsHost::next(rt, &mut iterator)
            .unwrap()
            .is_none()
    );
    // Exhaustion is permanent even when the source subsequently grows.
    map.call_method("set", vec![Js::str("late"), Js::Number(3.0)])
        .unwrap();
    assert!(
        PrivateTopicsHost::next(rt, &mut iterator)
            .unwrap()
            .is_none()
    );
}

#[test]
fn private_host_collection_deletion_keeps_the_next_insertion_position() {
    let h = harness::harness(ExchangeName::Bitget, harness::options(), |_| {});
    let rt = &h.exchange.rt;
    for collection in [Js::new_set(), Js::new_map()] {
        let insert = |n| {
            if matches!(collection, Js::Map(_)) {
                collection
                    .call_method("set", vec![Js::Number(n), Js::Number(n)])
                    .unwrap();
            } else {
                collection.call_method("add", vec![Js::Number(n)]).unwrap();
            }
        };
        insert(1.0);
        insert(2.0);
        let mut iterator = PrivateTopicsHost::iterator(rt, &collection).unwrap();
        assert!(
            PrivateTopicsHost::next(rt, &mut iterator)
                .unwrap()
                .is_some()
        );
        collection
            .call_method("delete", vec![Js::Number(1.0)])
            .unwrap();
        insert(3.0);
        let next = PrivateTopicsHost::next(rt, &mut iterator).unwrap().unwrap();
        let next = if matches!(collection, Js::Map(_)) {
            next.get("0").unwrap()
        } else {
            next
        };
        assert!(
            next.strict_equals(&Js::Number(2.0)),
            "real private host skips survivor after deleting current Map/Set entry"
        );
        let next = PrivateTopicsHost::next(rt, &mut iterator).unwrap().unwrap();
        let next = if matches!(collection, Js::Map(_)) {
            next.get("0").unwrap()
        } else {
            next
        };
        assert!(
            next.strict_equals(&Js::Number(3.0)),
            "live insertion must be visited"
        );
        assert!(
            PrivateTopicsHost::next(rt, &mut iterator)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn private_host_iterator_retains_next_and_propagates_abrupt_steps() {
    let h = harness::harness(ExchangeName::Bitget, harness::options(), |_| {});
    let rt = &h.exchange.rt;
    let values = Js::array(vec![Js::Number(1.0)]).iterator().unwrap();
    let mut iterator = PrivateTopicsHost::iterator(rt, &values).unwrap();
    values
        .define(
            "next",
            Js::function("replaced", |_, _| panic!("GetIterator caches next")),
        )
        .unwrap();
    assert!(
        PrivateTopicsHost::next(rt, &mut iterator)
            .unwrap()
            .unwrap()
            .strict_equals(&Js::Number(1.0))
    );

    let invalid = Js::array(vec![]).iterator().unwrap();
    invalid
        .define("next", Js::function("invalid", |_, _| Ok(Js::Number(1.0))))
        .unwrap();
    let mut iterator = PrivateTopicsHost::iterator(rt, &invalid).unwrap();
    let error = PrivateTopicsHost::next(rt, &mut iterator).unwrap_err();
    assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");

    let abrupt = Js::array(vec![]).iterator().unwrap();
    let thrown = Js::object(vec![("tag", Js::str("done-getter"))]);
    let original = thrown.clone();
    let result = Js::object(vec![]);
    result.define_accessor(
        "done",
        Some(Rc::new(move |_| {
            Err(safe_cex_exchange::js_runtime::value::Thrown::new(
                thrown.clone(),
            ))
        })),
        None,
    );
    abrupt
        .define(
            "next",
            Js::function("abrupt", move |_, _| Ok(result.clone())),
        )
        .unwrap();
    let mut iterator = PrivateTopicsHost::iterator(rt, &abrupt).unwrap();
    assert!(
        PrivateTopicsHost::next(rt, &mut iterator)
            .unwrap_err()
            .value
            .strict_equals(&original)
    );

    let mut iterator = PrivateTopicsHost::iterator(rt, &Js::str("A😀B")).unwrap();
    for expected in ["A", "😀", "B"] {
        assert!(
            PrivateTopicsHost::next(rt, &mut iterator)
                .unwrap()
                .unwrap()
                .strict_equals(&Js::str(expected))
        );
    }
    assert!(
        PrivateTopicsHost::next(rt, &mut iterator)
            .unwrap()
            .is_none()
    );
}

#[test]
fn private_host_body_throw_closes_iterator_and_keeps_original_failure() {
    use safe_cex_exchange::js_runtime::value::Thrown;
    use std::cell::Cell;
    let h = harness::harness(ExchangeName::Bitget, harness::options(), |_| {});
    let rt = &h.exchange.rt;
    rt.this
        .define("wsPositionUpdatedAtById", Js::new_map())
        .unwrap();
    let original = Js::object(vec![("tag", Js::str("position-id"))]);
    let thrown = original.clone();
    let position = Js::object(vec![]);
    position.define_accessor(
        "id",
        Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
        None,
    );
    let values = Js::array(vec![position]).iterator().unwrap();
    let closed = Rc::new(Cell::new(0));
    let count = closed.clone();
    let receiver = values.clone();
    values
        .define(
            "return",
            Js::function("return", move |this, _| {
                assert!(this.strict_equals(&receiver));
                count.set(count.get() + 1);
                Err(Thrown::type_error(
                    "close error must not replace body throw",
                ))
            }),
        )
        .unwrap();
    let error = crate::private_topics::mark_ws_positions(rt, values, Js::Undefined).unwrap_err();
    assert!(error.value.strict_equals(&original));
    assert_eq!(closed.get(), 1, "production loop must close on body throw");
}

#[test]
fn production_get_key_reads_inherited_aliases_and_keeps_shadowing() {
    let h = harness::harness(ExchangeName::Bitget, harness::options(), |_| {});
    let helper = h.exchange.rt.global("v").unwrap();
    let parent = Js::object(vec![("some_field", Js::Number(23.0))]);
    let object = Js::object(vec![]);
    let Js::Object(data) = &object else {
        unreachable!()
    };
    *data.proto.borrow_mut() = Some(parent);
    let get = || {
        helper
            .call(Js::Undefined, vec![object.clone(), Js::str("someField")])
            .unwrap()
    };
    assert!(
        get().strict_equals(&Js::Number(23.0)),
        "inherited snake-case key must resolve"
    );
    object.define("someField", Js::Undefined).unwrap();
    assert!(get().is_undefined(), "own undefined shadows later alias");
}
