//! Bulk removal uses source property keys through the actual DefaultStore facade.
use safe_cex_exchange::js_runtime::{
    store,
    value::{Js, Thrown, from_json},
};
use safe_cex_exchange::native_store::DefaultStore;
use serde_json::{Value, json};
use std::rc::Rc;
fn selector(tag: &str) -> Js {
    match tag {
        "string_id" => Js::object(vec![("id", Js::str("123"))]),
        "number_id" => Js::object(vec![("id", Js::Number(123.))]),
        "bare_string" => Js::str("abc"),
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "missing_id" => Js::object(vec![]),
        "undefined_id" => Js::object(vec![("id", Js::Undefined)]),
        "null_id" => Js::object(vec![("id", Js::Null)]),
        "symbol_id" => Js::object(vec![("id", Js::SymbolIterator)]),
        "object_symbol_id" => Js::object(vec![(
            "id",
            Js::object(vec![(
                "toString",
                Js::function("toString", |_, _| Ok(Js::SymbolIterator)),
            )]),
        )]),
        "surrogate_id" => Js::object(vec![("id", Js::String(vec![0xd800].into()))]),
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
fn bulk_remove_selector_ids_match_source_property_keys_and_early_returns() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_store_bulk_id_source.mjs"
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
    assert_eq!(cases.len(), 30, "source array cases and empty Store floor");
    for row in cases {
        let facade = store::facade(Rc::new(DefaultStore::new())).unwrap();
        let seeds = if row["empty"] == true {
            vec![]
        } else {
            source["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| {
                    let seed = from_json(&source["seed"]);
                    seed.define("id", from_json(id)).unwrap();
                    seed
                })
                .collect()
        };
        facade
            .call_method(
                "update",
                vec![Js::object(vec![("orders", Js::array(seeds))])],
            )
            .unwrap();
        let tag = row["tag"].as_str().unwrap();
        let input = if tag == "empty_input" {
            Js::array(vec![])
        } else {
            Js::array(vec![selector(tag)])
        };
        let error = facade
            .call_method("removeOrders", vec![input])
            .err()
            .map(|error| error.value.get("name").unwrap().to_display());
        assert_eq!(
            json!(error),
            row["error"],
            "BULK-ID {tag} empty={} error",
            row["empty"]
        );
        let ids: Vec<String> = facade
            .get("orders")
            .unwrap()
            .array_values()
            .expect("fixture array property reads")
            .unwrap()
            .iter()
            .map(|row| row.get("id").unwrap().to_display())
            .collect();
        assert_eq!(
            json!(ids),
            row["ids"],
            "BULK-ID {tag} empty={} removal",
            row["empty"]
        );
    }
}
