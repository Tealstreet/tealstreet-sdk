//! Maintained Store identity semantics through the actual typed Store facade.
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
fn singular_store_ids_match_actual_source_property_access_and_lookup_rules() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_store_id_source.mjs"
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
        132,
        "six mutators,eleven selectors,empty/nonempty stores"
    );
    for row in cases {
        let facade = store::facade(Rc::new(DefaultStore::new())).unwrap();
        let state = row["state"].as_str().unwrap();
        let seed = &source["seeds"][state];
        let seeds = if row["empty"] == true {
            vec![]
        } else {
            ["123", "abc", "undefined"]
                .into_iter()
                .map(|id| {
                    let value = from_json(seed);
                    value.define("id", Js::str(id)).unwrap();
                    value
                })
                .collect()
        };
        facade
            .call_method("update", vec![Js::object(vec![(state, Js::array(seeds))])])
            .unwrap();
        let result = facade.call_method(
            row["method"].as_str().unwrap(),
            vec![
                selector(row["tag"].as_str().unwrap()),
                from_json(&row["changes"]),
            ],
        );
        let error = result
            .err()
            .map(|error| error.value.get("name").unwrap().to_display());
        assert_eq!(
            json!(error),
            row["error"],
            "STORE-ID {} {} empty={} error",
            row["method"],
            row["tag"],
            row["empty"]
        );
        let field = row["field"].as_str().unwrap();
        let actual:Vec<Value>=facade.get(state).unwrap().array_values().unwrap().into_iter().map(|value|json!({"id":value.get("id").unwrap().to_display(),"value":value.get(field).unwrap().to_display()})).collect();
        assert_eq!(
            json!(actual),
            row["rows"],
            "STORE-ID {} {} empty={} mutation",
            row["method"],
            row["tag"],
            row["empty"]
        );
    }
}
