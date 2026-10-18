use safe_cex_exchange::js_runtime::value::{Js, Thrown, from_json, to_json};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn live_collection_foreach_matches_node_mutation_traces() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_collection_foreach_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 14);
    for case in cases {
        let map = case["type"] == "Map";
        let collection = if map { Js::new_map() } else { Js::new_set() };
        let add = if map { "set" } else { "add" };
        for (key, value) in [(1, "A"), (2, "B")] {
            collection
                .call_method(add, vec![Js::Number(key as f64), Js::str(value)])
                .unwrap();
        }
        let trace = Rc::new(RefCell::new(vec![]));
        let changed = Cell::new(false);
        let target = collection.clone();
        let visits = trace.clone();
        let mode = case["mode"].as_str().unwrap().to_owned();
        let callback = Js::function("callback", move |_, args| {
            assert_eq!(args.len(), 3);
            assert!(args[2].same_reference(&target));
            visits
                .borrow_mut()
                .push(json!([to_json(&args[1])?, to_json(&args[0])?]));
            if changed.replace(true) {
                return Ok(Js::Undefined);
            }
            match mode.as_str() {
                "delete-add" => {
                    target.call_method("delete", vec![Js::Number(2.0)])?;
                    target.call_method(add, vec![Js::Number(3.0), Js::str("C")])?;
                }
                "delete-current" => {
                    target.call_method("delete", vec![Js::Number(1.0)])?;
                    target.call_method(add, vec![Js::Number(3.0), Js::str("C")])?;
                }
                "clear-readd" => {
                    target.call_method("clear", vec![])?;
                    target.call_method(add, vec![Js::Number(3.0), Js::str("C")])?;
                }
                "update-future" => {
                    target.call_method(add, vec![Js::Number(2.0), Js::str("B2")])?;
                }
                "revisit" => {
                    target.call_method("delete", vec![Js::Number(1.0)])?;
                    target.call_method(add, vec![Js::Number(1.0), Js::str("A2")])?;
                }
                "nested" => {
                    let visits = visits.clone();
                    target.call_method(
                        "forEach",
                        vec![Js::function("inner", move |_, a| {
                            visits.borrow_mut().push(json!([
                                "inner",
                                to_json(&a[1])?,
                                to_json(&a[0])?
                            ]));
                            Ok(Js::Undefined)
                        })],
                    )?;
                }
                "throw" => return Err(Thrown::error("RangeError", "callback sentinel")),
                _ => unreachable!(),
            }
            Ok(Js::Undefined)
        });
        let result = collection.call_method("forEach", vec![callback]);
        let error = match result {
            Ok(value) => {
                assert!(value.is_undefined());
                Value::Null
            }
            Err(error) => {
                let e = error.to_exchange_error();
                json!({"name":e.name,"message":e.message})
            }
        };
        // The JSON projection is only the observable trace; collection values stay native.
        let actual = Value::Array(trace.borrow().clone());
        let expected = to_json(&from_json(&case["trace"])).unwrap();
        assert_eq!(actual, expected, "live forEach mutation: {case}");
        assert_eq!(error, case["error"]);
    }
}

#[test]
fn foreach_preserves_native_callback_receiver_values_and_throw_identity() {
    for map in [true, false] {
        let collection = if map { Js::new_map() } else { Js::new_set() };
        let receiver = Js::object(vec![]);
        let value = Js::object(vec![("bits", Js::Number(-0.0))]);
        collection
            .call_method(
                if map { "set" } else { "add" },
                vec![value.clone(), value.clone()],
            )
            .unwrap();
        let expected_receiver = receiver.clone();
        let expected_value = value.clone();
        let thrown = Js::object(vec![(
            "sentinel",
            Js::Number(f64::from_bits(0x7ff8_0000_0000_0042)),
        )]);
        let expected_throw = thrown.clone();
        let result = collection.call_method(
            "forEach",
            vec![
                Js::function("check", move |this, a| {
                    assert!(this.same_reference(&expected_receiver), "forEach thisArg");
                    assert!(a[0].same_reference(&expected_value));
                    assert!(a[1].same_reference(&expected_value));
                    assert_eq!(
                        a[0].get("bits")?.to_number()?.to_bits(),
                        (-0.0_f64).to_bits()
                    );
                    Err(Thrown::new(expected_throw.clone()))
                }),
                receiver,
            ],
        );
        assert!(result.unwrap_err().value.same_reference(&thrown));
        let empty = if map { Js::new_map() } else { Js::new_set() };
        assert_eq!(
            empty
                .call_method("forEach", vec![Js::Number(1.0)])
                .unwrap_err()
                .to_exchange_error()
                .name,
            "TypeError"
        );
    }
}
