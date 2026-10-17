use super::*;
use serde_json::{Value, json};

pub(crate) fn expectations() -> Value {
    static EXPECTED: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    EXPECTED
        .get_or_init(|| {
            let result = std::process::Command::new("node")
                .arg("tests/iterator_contract_oracle.cjs")
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .output()
                .expect("Node is required for the independently authored iterator expectations");
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            serde_json::from_slice(&result.stdout).unwrap()
        })
        .clone()
}

fn native_step(value: Result<Js, Thrown>) -> Value {
    match value {
        Ok(step) => {
            json!({"value":step.get("value").unwrap().to_number().unwrap(),"done":step.get("done").unwrap().truthy()})
        }
        Err(error) => json!({"error":error.value.get("name").unwrap().to_display()}),
    }
}

#[test]
fn accepted_iterator_collection_dense_view() {
    for row in expectations()["collection"].as_array().unwrap() {
        let mut collection = Collection::<Js>::default();
        for value in row["input"].as_array().unwrap() {
            collection.push(Js::Number(value.as_f64().unwrap()));
        }
        let borrowed = std::ops::Deref::deref(&collection);
        let values: Vec<_> = borrowed
            .iter()
            .map(|value| value.to_number().unwrap())
            .collect();
        assert_eq!(
            json!({"len":borrowed.len(),"values":values}),
            row["output"],
            "Collection native ordered view"
        );
    }
    let mut collection = Collection::<Js>::default();
    let marker = Js::object(vec![]);
    collection.push(Js::Number(-0.0));
    collection.push(marker.clone());
    assert_eq!(
        collection[0].to_number().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert!(collection[1].same_reference(&marker));
}

#[test]
fn accepted_iterator_object_retains_protocol_and_native_step() {
    for row in expectations()["object"].as_array().unwrap() {
        let tag = if row["kind"] == "Map" {
            "Map Iterator"
        } else {
            "Set Iterator"
        };
        let number = row["value"].as_f64().unwrap();
        let next: NativeFn = Rc::new(move |_, _| {
            Ok(Js::object(vec![
                ("value", Js::Number(number)),
                ("done", Js::Bool(false)),
            ]))
        });
        let iterator = iterator_object(tag, next);
        let self_iterator = iterator.iterator();
        let actual = json!({"kind":row["kind"],"value":number,"tag":iterator.to_display(),
            "self":self_iterator.is_ok_and(|value|value.same_reference(&iterator)),
            "ownNext":iterator.has_own("next"),"first":native_step(iterator.call_method("next",vec![]))});
        assert_eq!(actual, *row, "iterator_object native protocol fields");
    }
}

#[test]
fn accepted_iterator_next_uses_actual_branded_receiver() {
    for row in expectations()["next"].as_array().unwrap() {
        let tag = if row["kind"] == "Map" {
            "Map Iterator"
        } else {
            "Set Iterator"
        };
        let number = row["value"].as_f64().unwrap();
        let next: NativeFn = Rc::new(move |_, _| {
            Ok(Js::object(vec![
                ("value", Js::Number(number)),
                ("done", Js::Bool(false)),
            ]))
        });
        let other = iterator_object(tag, next);
        let method = iterator_next_method(tag);
        let error = method.call(Js::object(vec![]), vec![]).unwrap_err();
        let actual = json!({"kind":row["kind"],"value":number,"step":native_step(method.call(other,vec![])),
            "error":error.value.get("name").unwrap().to_display()});
        assert_eq!(
            actual, *row,
            "iterator_next_method actual receiver and refusal"
        );
        let fake = Js::object_with_class(tag, vec![]);
        assert!(
            method.call(fake, vec![]).is_err(),
            "a matching class string does not mint iterator state"
        );
    }
}

#[test]
fn accepted_iterator_js_protocol_entrypoints() {
    let expected = expectations();
    for row in expected["js"]["get_property"].as_array().unwrap() {
        let owner = Js::object(vec![
            ("plain", Js::str("text")),
            ("Symbol.iterator", Js::str("string-key")),
        ]);
        if let Js::Object(data) = &owner {
            *data.symbol_iterator.borrow_mut() =
                Some(Prop::Data(Js::function("[Symbol.iterator]", |this, _| {
                    Ok(this)
                })));
        }
        let key = if row["input"] == "symbol" {
            Js::SymbolIterator
        } else {
            Js::str("plain")
        };
        let actual = match owner.get_property(&key) {
            Ok(value) if value.is_callable() => {
                json!({"kind":"function","self":value.call(owner.clone(),vec![]).is_ok_and(|value|value.same_reference(&owner))})
            }
            Ok(value) => json!({"kind":value.type_of(),"value":value.to_display()}),
            Err(error) => json!({"error":error.value.get("name").unwrap().to_display()}),
        };
        assert_eq!(
            actual, row["output"],
            "Js::get_property native symbol front door"
        );
    }
    for row in expected["js"]["iterator"].as_array().unwrap() {
        let owner = if row["input"] == "array" {
            Js::array(vec![Js::str("a"), Js::str("b")])
        } else {
            Js::str("A😀")
        };
        let actual = (|| -> Result<Value, Thrown> {
            let iterator = owner.iterator()?;
            let is_self = iterator.iterator()?.same_reference(&iterator);
            let mut values = vec![];
            for _ in 0..2 {
                values.push(
                    iterator
                        .call_method("next", vec![])?
                        .get("value")?
                        .to_display(),
                );
            }
            Ok(
                json!({"self":is_self,"values":values,"done":iterator.call_method("next",vec![])?.get("done")?.truthy()}),
            )
        })();
        let actual = actual
            .unwrap_or_else(|error| json!({"error":error.value.get("name").unwrap().to_display()}));
        assert_eq!(actual, row["output"], "Js::iterator native protocol result");
    }
    for row in expected["js"]["iterable_values"].as_array().unwrap() {
        let owner = if row["input"] == "map" {
            let owner = Js::new_map();
            owner
                .call_method("set", vec![Js::str("a"), Js::Number(41.5)])
                .unwrap();
            owner
                .call_method("set", vec![Js::str("b"), Js::Number(43.5)])
                .unwrap();
            owner
        } else {
            let owner = Js::new_set();
            owner.call_method("add", vec![Js::Number(47.5)]).unwrap();
            owner.call_method("add", vec![Js::Number(53.5)]).unwrap();
            owner
        };
        let actual = owner
            .iterable_values()
            .and_then(|values| super::super::value::to_json(&Js::array(values)))
            .unwrap_or_else(|error| json!({"error":error.value.get("name").unwrap().to_display()}));
        assert_eq!(
            actual, row["output"],
            "Js::iterable_values native Map/Set protocol"
        );
    }
}
