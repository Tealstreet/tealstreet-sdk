//! The actual private-topic host on a constructed production exchange.
#[path = "../../tests/support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;

use crate::private_topics::PrivateTopicsHost;
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_types::ExchangeName;
use std::rc::Rc;

#[test]
fn retained_position_time_matches_source_through_constructed_private_receiver() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/retained-info-time-source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 9);
    for name in [
        ExchangeName::Bitget,
        ExchangeName::Bitgetusdc,
        ExchangeName::Bitgetinverse,
    ] {
        let h = harness::harness(name.clone(), harness::options(), |_| {});
        let socket = h.exchange.rt.this.get("privateWebsocket").unwrap();
        for (index, case) in cases.iter().enumerate() {
            let info = Js::object(vec![
                ("unread", Js::Number(f64::NAN)),
                ("ownUndefined", Js::Undefined),
            ]);
            for field in case["fields"].as_array().unwrap() {
                let value = &field[1];
                let value = match value["kind"].as_str().unwrap() {
                    "undefined" => Js::Undefined,
                    "null" => Js::Null,
                    "number" => Js::Number(f64::from_bits(
                        u64::from_str_radix(value["bits"].as_str().unwrap(), 16).unwrap(),
                    )),
                    "text" => Js::utf16(
                        &value["units"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
                            .collect::<Vec<_>>(),
                    ),
                    _ => panic!("source input kind"),
                };
                info.define(field[0].as_str().unwrap(), value).unwrap();
            }
            let result = socket
                .call_method(
                    "getPositionPayloadUpdateTime",
                    vec![Js::object(vec![("info", info.clone())])],
                )
                .expect("retained info must enter the production time reader");
            if let Some(bits) = case["expected"].as_str() {
                assert_eq!(
                    result.as_f64().expect("native time").to_bits(),
                    u64::from_str_radix(bits, 16).unwrap(),
                    "{name:?} source time {index}"
                );
            } else {
                assert!(result.is_undefined(), "{name:?} source refusal {index}");
            }
            assert!(info.get("unread").unwrap().as_f64().unwrap().is_nan());
            assert!(info.get("ownUndefined").unwrap().is_undefined());
        }
    }
}

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
    result
        .define_accessor(
            "done",
            Some(Rc::new(move |_| {
                Err(safe_cex_exchange::js_runtime::value::Thrown::new(
                    thrown.clone(),
                ))
            })),
            None,
        )
        .expect("fresh test accessor target");
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
    position
        .define_accessor(
            "id",
            Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
            None,
        )
        .expect("fresh test accessor target");
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

#[test]
fn retained_plan_type_matches_source_through_constructed_exchange() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/retained-info-plan-source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for name in [
        ExchangeName::Bitget,
        ExchangeName::Bitgetusdc,
        ExchangeName::Bitgetinverse,
    ] {
        let h = harness::harness(name.clone(), harness::options(), |_| {});
        for case in cases {
            let input = &case["input"];
            let units = || {
                input["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|unit| unit.as_u64().unwrap() as u16)
                    .collect::<Vec<_>>()
            };
            let raw = match input["kind"].as_str().unwrap() {
                "text" => Js::utf16(&units()),
                "array" => Js::array(vec![Js::utf16(&units())]),
                "nan" => Js::Number(f64::NAN),
                "infinity" => Js::Number(f64::INFINITY),
                "object" => Js::object(vec![]),
                "undefined" => Js::Undefined,
                _ => unreachable!(),
            };
            let order = safe_cex_exchange::js_runtime::value::from_json(
                &serde_json::json!({"id":"retained","status":"open","symbol":"BTCUSDT","type":"trailing_stop_market","side":"buy","price":1,"amount":2,"filled":0,"remaining":2,"reduceOnly":false,"lastUpdate":0}),
            );
            let info = Js::object(vec![
                ("planType", raw.clone()),
                ("unreadNaN", Js::Number(f64::NAN)),
                ("ownUndefined", Js::Undefined),
            ]);
            order.define("info", info.clone()).unwrap();
            let result = h
                .exchange
                .rt
                .this
                .call_method("getCancelPlanType", vec![order])
                .expect("retained plan type must select its source fallback");
            assert!(
                result.strict_equals(&Js::str(case["expected"].as_str().unwrap())),
                "native cancel plan {name:?}: {case}"
            );
            assert!(info.get("planType").unwrap().same_value_zero(&raw));
            assert!(info.get("unreadNaN").unwrap().to_number().unwrap().is_nan());
            assert!(info.has_own("ownUndefined"));
        }
    }
}

#[test]
fn constructed_order_mappers_retain_unread_native_info() {
    use safe_cex_exchange::js_runtime::value::from_json;
    for name in [
        ExchangeName::Bitget,
        ExchangeName::Bitgetusdc,
        ExchangeName::Bitgetinverse,
    ] {
        let h = harness::harness(name.clone(), harness::options(), |_| {});
        let raw = from_json(
            &serde_json::json!({"orderId":"native-info","status":"new","orderType":"limit","side":"buy","symbol":"BTCUSDT","size":"2","price":"10","baseVolume":"0"}),
        );
        raw.define("opaque", Js::utf16(&[0xd800, 65, 0xdc00]))
            .unwrap();
        raw.define("nan", Js::Number(f64::NAN)).unwrap();
        raw.define("infinity", Js::Number(f64::INFINITY)).unwrap();
        raw.define("zero", Js::Number(-0.0)).unwrap();
        raw.define("undefined", Js::Undefined).unwrap();
        let result = h
            .exchange
            .rt
            .this
            .call_method("mapOrder", vec![raw])
            .expect("constructed mapper admits opaque info");
        assert_eq!(result.get("length").unwrap().as_f64(), Some(1.0));
        let info = result.get("0").unwrap().get("info").unwrap();
        assert_eq!(
            info.get("opaque").unwrap().as_str().unwrap().as_ref(),
            &[0xd800, 65, 0xdc00],
            "{name:?}"
        );
        assert!(info.get("nan").unwrap().as_f64().unwrap().is_nan());
        assert_eq!(
            info.get("infinity").unwrap().as_f64().unwrap().to_bits(),
            f64::INFINITY.to_bits()
        );
        assert_eq!(
            info.get("zero").unwrap().as_f64().unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        assert!(info.has_own("undefined"));
        assert!(info.get("undefined").unwrap().is_undefined());
    }
}
