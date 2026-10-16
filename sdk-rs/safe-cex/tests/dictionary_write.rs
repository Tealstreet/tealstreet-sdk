use safe_cex_num::{ProtoMember, TableHit, js_table_lookup};
use safe_cex_store::dictionary_write::{ReferenceWrite, assign_object, mutate_object};
use safe_cex_types::object_map::JsObjectMap;
use serde_json::{Value, json};

fn compare_values(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            assert_eq!(a.as_f64().unwrap().to_bits(), b.as_f64().unwrap().to_bits())
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (key, value) in a {
                compare_values(value, &b[key]);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

#[test]
fn production_dictionary_write_mechanisms_and_safe_local_outcomes() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/dictionary_write_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 60);
    for row in rows {
        let key = row["key"].as_str().unwrap();
        let own = row["own"].as_bool().unwrap();
        let assign = row["mode"] == "assign";
        let inherited = match js_table_lookup::<&str, ()>(&[], key) {
            TableHit::Inherited(member) => Some(member),
            _ => None,
        };
        let mut dictionary = JsObjectMap::new();
        if own {
            dictionary.insert(key.into(), json!({"free":3,"total":3}));
        }
        let reference = if assign {
            let report = assign_object(
                &mut dictionary,
                key.into(),
                json!({"free":2,"used":0,"total":2,"upnl":0,"info":{"currency":key}}),
            );
            assert_eq!(report.previous.is_some(), own);
            report.reference
        } else {
            mutate_object(
                &mut dictionary,
                key.into(),
                || json!({"free":0,"used":0,"total":0,"upnl":0}),
                |value| {
                    for field in ["free", "total"] {
                        value[field] = json!(value[field].as_f64().unwrap() + 2.0);
                    }
                },
            )
        };
        let expected = if own {
            ReferenceWrite::OwnProperty
        } else {
            match (assign, inherited) {
                (true, Some(ProtoMember::Prototype)) => ReferenceWrite::ReplacePrototype,
                (true, Some(member)) => ReferenceWrite::ShadowInherited(member),
                (false, Some(member)) => ReferenceWrite::MutateInheritedReferent(member),
                (_, None) => ReferenceWrite::OwnProperty,
            }
        };
        assert_eq!(reference, expected, "reference write classification {row}");
        assert!(
            dictionary.contains_key(key),
            "safe Rust write retains balance under literal key"
        );
        if own || inherited.is_none() || (assign && inherited != Some(ProtoMember::Prototype)) {
            compare_values(
                &serde_json::to_value(&dictionary).unwrap()[key],
                &row["value"],
            );
        } else {
            assert!(
                row["keys"].as_array().unwrap().is_empty(),
                "source inherited write loses own balance"
            );
            assert_eq!(
                dictionary[key]["free"].as_f64().unwrap().to_bits(),
                2.0_f64.to_bits()
            );
        }
    }
}
