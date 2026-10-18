//! Actual DefaultStore upserts through the production JS facade, against maintained TS.
use safe_cex_exchange::js_runtime::{
    store,
    value::{Js, from_json, from_typed, to_json},
};
use safe_cex_exchange::native_store::DefaultStore;
use serde_json::Value;
use std::{collections::BTreeSet, process::Command, rc::Rc};

#[test]
fn position_upserts_distinguish_absent_undefined_and_present_fields() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-store/tests/position_model_merge_source.mjs"
        ))
        .output()
        .expect("execute maintained TypeScript Store");
    assert!(
        output.status.success(),
        "source Store: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        81,
        "three upserts, nine model slots, three presence states"
    );
    assert_eq!(
        source["replacements"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "side",
            "marginType",
            "positionMode",
            "realizedPnl",
            "marginSize",
            "info",
            "extra",
            "lastUpdate",
            "option"
        ])
    );
    for case in cases {
        let method = case["method"].as_str().unwrap();
        let field = case["field"].as_str().unwrap();
        let mode = case["mode"].as_str().unwrap();
        let native = Rc::new(DefaultStore::new());
        let facade = store::facade(native.clone()).unwrap();
        let sibling = from_json(&source["seed"]);
        sibling.define("id", Js::str("sibling")).unwrap();
        facade
            .call_method(
                "update",
                vec![Js::object(vec![(
                    "positions",
                    Js::array(vec![from_json(&source["seed"]), sibling.clone()]),
                )])],
            )
            .unwrap();
        let incoming = from_json(&source["seed"]);
        incoming.define("contracts", Js::Number(0.0)).unwrap();
        incoming.define("entryPrice", Js::Number(0.0)).unwrap();
        incoming
            .delete(&field.encode_utf16().collect::<Vec<_>>())
            .unwrap();
        match mode {
            "absent" => {}
            "undefined" => {
                incoming.define(field, Js::Undefined).unwrap();
            }
            "value" => {
                incoming
                    .define(field, from_json(&source["replacements"][field]))
                    .unwrap();
            }
            _ => panic!("unexpected source presence mode"),
        }
        let revision = native.update_ids().unwrap().positions;
        let argument = if method == "addOrUpdatePosition" {
            incoming
        } else {
            Js::array(vec![incoming, sibling])
        };
        facade.call_method(method, vec![argument]).unwrap();
        assert_eq!(
            native.update_ids().unwrap().positions,
            revision + 1.0,
            "{method}/{field}/{mode}"
        );
        let positions = native.positions().unwrap();
        assert_eq!(positions.len(), 2);
        let position = positions.iter().find(|p| p.id == "p").unwrap();
        assert_eq!(position.contracts, 0.0, "closed position applied");
        let row = from_typed(position).unwrap();
        assert!(
            row.has_own(field),
            "{method}/{field}/{mode}: own field must survive"
        );
        let value = row.get(field).unwrap();
        assert_eq!(
            value.is_undefined(),
            case["undefined"].as_bool().unwrap(),
            "{method}/{field}/{mode}: source presence"
        );
        if !value.is_undefined() {
            assert_eq!(
                to_json(&value).unwrap(),
                to_json(&from_json(&case["value"])).unwrap(),
                "{method}/{field}/{mode}: source value"
            );
        }
    }
}
