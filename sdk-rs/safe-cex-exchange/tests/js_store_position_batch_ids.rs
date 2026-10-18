//! Adaptive source position-batch indexing through the real Store facade.
use safe_cex_exchange::js_runtime::{
    store,
    value::{Js, Thrown, from_json},
};
use safe_cex_exchange::native_store::DefaultStore;
use serde_json::{Value, json};
use std::rc::Rc;
fn selector(tag: &str) -> Js {
    match tag {
        "object_symbol_id" => Js::object(vec![(
            "id",
            Js::object(vec![(
                "toString",
                Js::function("toString", |_, _| Ok(Js::SymbolIterator)),
            )]),
        )]),
        "symbol_id" => Js::object(vec![("id", Js::SymbolIterator)]),
        "surrogate_id" => Js::object(vec![("id", Js::utf16(&[0xd800]))]),
        "string_id" => Js::object(vec![("id", Js::str("123"))]),
        "number_id" => Js::object(vec![("id", Js::Number(123.))]),
        "bare_string" => Js::str("abc"),
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "missing_id" => Js::object(vec![]),
        "undefined_id" => Js::object(vec![("id", Js::Undefined)]),
        "null_id" => Js::object(vec![("id", Js::Null)]),
        "object_id" => Js::object(vec![(
            "id",
            Js::object(vec![(
                "toString",
                Js::function("toString", |_, _| Ok(Js::str("123"))),
            )]),
        )]),
        "noncoercible_id" => Js::object(vec![(
            "id",
            Js::object(vec![(
                "toString",
                Js::function("toString", |_, _| {
                    Err(Thrown::error("RangeError", "selector conversion"))
                }),
            )]),
        )]),
        "throwing_id" => {
            let object = Js::object(vec![]);
            object
                .define_accessor(
                    "id",
                    Some(Rc::new(|_| {
                        Err(Thrown::error("RangeError", "selector getter"))
                    })),
                    None,
                )
                .expect("test accessor target permits definition");
            object
        }
        _ => panic!("unknown source selector {tag}"),
    }
}

#[test]
fn position_batch_ids_match_source_strict_and_indexed_lookup_modes() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_store_position_batch_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        168,
        "four states,three counts,fourteen selectors"
    );
    for row in cases {
        let native = Rc::new(DefaultStore::new());
        let facade = store::facade(native.clone());
        let seeds = row["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| {
                let seed = from_json(&source["seed"]);
                seed.define("id", from_json(id)).unwrap();
                seed
            })
            .collect();
        facade
            .call_method(
                "update",
                vec![Js::object(vec![("positions", Js::array(seeds))])],
            )
            .unwrap();
        let before = safe_cex_store::Store::update_ids(native.as_ref()).positions;
        let count = row["count"].as_u64().unwrap();
        let mut updates = vec![];
        if count > 0 {
            updates.push(Js::array(vec![
                selector(row["tag"].as_str().unwrap()),
                Js::object(vec![("contracts", Js::Number(7.))]),
            ]))
        }
        if count == 2 {
            updates.push(Js::array(vec![
                Js::object(vec![("id", Js::str("123"))]),
                Js::object(vec![("contracts", Js::Number(9.))]),
            ]))
        }
        let error = facade
            .call_method("updatePositions", vec![Js::array(updates)])
            .err()
            .map(|error| error.value.get("name").unwrap().to_display());
        assert_eq!(
            json!(error),
            row["error"],
            "POSITION-ID {} count={count} {} error",
            row["mode"],
            row["tag"]
        );
        let rows:Vec<Value>=facade.get("positions").unwrap().array_values().unwrap().iter().map(|value|json!({"id":value.get("id").unwrap().to_display(),"contracts":value.get("contracts").unwrap().to_display()})).collect();
        assert_eq!(
            json!(rows),
            row["rows"],
            "POSITION-ID {} count={count} {} updates",
            row["mode"],
            row["tag"]
        );
        let after = safe_cex_store::Store::update_ids(native.as_ref()).positions;
        assert_eq!(
            after - before,
            row["delta"].as_f64().unwrap(),
            "POSITION-ID {} count={count} {} updateId",
            row["mode"],
            row["tag"]
        );
    }
}
