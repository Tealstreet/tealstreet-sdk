use safe_cex_exchange::js_runtime::{
    store,
    value::{Js, from_json, from_typed, to_retained_input, to_typed},
};
use safe_cex_store::store::DefaultStore;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{process::Command, rc::Rc, sync::OnceLock};

fn source() -> &'static Value {
    static SOURCE: OnceLock<Value> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/retained-info-source.mjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn input(value: &Value) -> Js {
    match value["kind"].as_str().unwrap() {
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "bool" => Js::Bool(value["value"].as_bool().unwrap()),
        "number" => Js::Number(f64::from_bits(
            u64::from_str_radix(value["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "text" => Js::utf16(&units(&value["units"])),
        "array" => Js::array(
            value["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(input)
                .collect(),
        ),
        "object" => {
            let result = Js::object(vec![]);
            for entry in value["entries"].as_array().unwrap() {
                result
                    .define_key(&units(&entry["key"]), input(&entry["value"]))
                    .unwrap();
            }
            result
        }
        _ => panic!("source kind"),
    }
}
fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|value| u16::try_from(value.as_u64().unwrap()).unwrap())
        .collect()
}
fn observe(actual: &Js, expected: &Value) {
    match expected["kind"].as_str().unwrap() {
        "undefined" => assert!(actual.is_undefined()),
        "null" => assert!(matches!(actual, Js::Null)),
        "bool" => {
            assert!(matches!(actual,Js::Bool(value) if Some(*value)==expected["value"].as_bool()))
        }
        "number" => assert_eq!(
            actual.as_f64().unwrap().to_bits(),
            u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap(),
            "native info number bits"
        ),
        "text" => assert!(
            matches!(actual,Js::String(value) if value.as_ref()==units(&expected["units"]).as_slice())
        ),
        "array" => {
            let values = actual.array_values().expect("native info array");
            let expected = expected["values"].as_array().unwrap();
            assert_eq!(values.len(), expected.len());
            for (value, expected) in values.iter().zip(expected) {
                observe(value, expected);
            }
        }
        "object" => {
            let entries = actual.own_entries().unwrap();
            let expected = expected["entries"].as_array().unwrap();
            assert_eq!(
                entries.len(),
                expected.len(),
                "native info own-property count"
            );
            for ((key, value), expected) in entries.iter().zip(expected) {
                assert_eq!(
                    key.as_ref(),
                    units(&expected["key"]).as_slice(),
                    "native info UTF16 key"
                );
                observe(value, &expected["value"]);
            }
        }
        _ => panic!("source kind"),
    }
}
fn info(actual: &Js, expected: &Value) {
    assert_eq!(
        actual.has_own("info"),
        expected["own"].as_bool().unwrap(),
        "native info presence"
    );
    let value = actual.get("info").unwrap();
    observe(&value, &expected["value"]);
    let wire =
        safe_cex_num::js_retained_json_stringify(&to_retained_input(&value).unwrap()).unwrap();
    assert_eq!(
        wire.as_deref(),
        expected["wire"].as_str(),
        "actual-source info JSON.stringify"
    );
}
fn typed<T: DeserializeOwned + Serialize>(value: &Js) -> Js {
    from_typed(&to_typed::<T>(value).expect("native info admission")).unwrap()
}
fn round_trip(model: &str, value: &Js) -> Js {
    match model {
        "market" => typed::<safe_cex_types::Market>(value),
        "order" => typed::<safe_cex_types::Order>(value),
        "position" => typed::<safe_cex_types::Position>(value),
        "balance" => typed::<safe_cex_types::Balance>(value),
        "ticker" => typed::<safe_cex_types::Ticker>(value),
        _ => panic!("source model"),
    }
}
#[test]
fn retained_info_round_trips_all_models_through_production_store_facade() {
    let cases = source()["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 65);
    for case in cases {
        let model = case["model"].as_str().unwrap();
        let row = from_json(&case["base"]);
        if case["input"]["own"] == true {
            row.define("info", input(&case["input"]["value"])).unwrap();
        }
        info(&round_trip(model, &row), &case["input"]);
        let facade = store::facade(Rc::new(DefaultStore::new()));
        let collection = if model == "balance" {
            "balance".into()
        } else {
            format!("{model}s")
        };
        let value = if model == "balance" {
            row
        } else {
            Js::array(vec![row])
        };
        facade
            .call_method("update", vec![Js::object(vec![(&collection, value)])])
            .expect("production Store update retains native info");
        let stored = facade.get(&collection).unwrap();
        let row = if model == "balance" {
            stored
        } else {
            stored.get("0").unwrap()
        };
        info(&row, &case["stored"]);
    }
}
#[test]
fn retained_info_patches_use_production_store_without_json_copy() {
    let cases = source()["patches"].as_array().unwrap();
    assert_eq!(cases.len(), 52);
    for case in cases {
        let model = case["model"].as_str().unwrap();
        let facade = store::facade(Rc::new(DefaultStore::new()));
        let collection = format!("{model}s");
        facade
            .call_method(
                "update",
                vec![Js::object(vec![(
                    &collection,
                    Js::array(vec![from_json(&case["base"])]),
                )])],
            )
            .unwrap();
        let patch = Js::object(vec![]);
        if case["patch"]["own"] == true {
            patch
                .define("info", input(&case["patch"]["value"]))
                .unwrap();
        }
        let id = Js::object(vec![("id", from_json(&case["base"]["id"]))]);
        let (method, args) = match case["operation"].as_str().unwrap() {
            "order" => ("updateOrder", vec![id, patch]),
            "position" => ("updatePosition", vec![id, patch]),
            "market" => ("updateMarket", vec![id, patch]),
            "positions" => (
                "updatePositions",
                vec![Js::array(vec![Js::array(vec![id, patch])])],
            ),
            _ => panic!("source operation"),
        };
        facade
            .call_method(method, args)
            .expect("production Store patch retains native info");
        info(
            &facade.get(&collection).unwrap().get("0").unwrap(),
            &case["stored"],
        );
    }
}

#[test]
fn retained_info_coercions_read_the_native_store_value() {
    let cases = source()["coercions"].as_array().unwrap();
    assert_eq!(cases.len(), 20, "native info coercion source floor");
    for case in cases {
        let native = Rc::new(DefaultStore::new());
        let facade = store::facade(native);
        let market = from_json(&source()["cases"][0]["base"]);
        market.set("info", input(&case["value"])).unwrap();
        facade
            .call_method(
                "update",
                vec![Js::object(vec![("markets", Js::array(vec![market]))])],
            )
            .unwrap();
        let value = facade.get("markets").unwrap().array_values().unwrap()[0]
            .get("info")
            .unwrap();
        let retained = to_retained_input(&value).unwrap();
        observe(&value, &case["value"]);
        for (name, result) in [
            ("number", safe_cex_num::js_retained_number(Some(&retained))),
            (
                "parseFloat",
                safe_cex_num::js_retained_parse_float(Some(&retained)),
            ),
        ] {
            if case[name].get("error").is_some() {
                let error = result.unwrap_err();
                assert_eq!(error.name, case[name]["error"]["name"]);
                assert_eq!(error.message, case[name]["error"]["message"]);
            } else {
                observe(&Js::Number(result.unwrap()), &case[name]["value"]);
            }
        }
        let text = safe_cex_num::js_retained_string(Some(&retained));
        if case["string"].get("error").is_some() {
            let error = text.unwrap_err();
            assert_eq!(error.name, case["string"]["error"]["name"]);
            assert_eq!(error.message, case["string"]["error"]["message"]);
        } else if String::from_utf16(&units(&case["string"]["value"]["units"])).is_ok() {
            observe(&Js::str(&text.unwrap()), &case["string"]["value"]);
        } else {
            assert_eq!(
                text.unwrap_err().message,
                "Cannot represent JavaScript UTF-16 string as UTF-8"
            );
            observe(&value, &case["value"]);
        }
    }
}

#[test]
fn retained_config_uses_the_production_store_update_without_json_copy() {
    let cases = source()["configurations"].as_array().unwrap();
    assert_eq!(cases.len(), 13, "native opaque config source floor");
    for case in cases {
        let native = Rc::new(DefaultStore::new());
        let facade = store::facade(native.clone());
        facade
            .call_method(
                "update",
                vec![Js::object(vec![("zkLinkConfig", input(&case["input"]))])],
            )
            .unwrap();
        let config = native.zk_link_config().expect("native config committed");
        observe(&from_typed(&*config).unwrap(), &case["stored"]);
        assert!(
            native.serialize().zk_link_config.is_none(),
            "source snapshot omits config"
        );
    }
}
