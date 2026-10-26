#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::{ProtoMember, TableHit, js_table_lookup};
use serde_json::Value;

#[test]
fn ordinary_table_hits_match_live_node_branches() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_table_lookup.cjs"
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    source_cardinality::for_each(rows, 40850, "table", |row| {
        let entries: Vec<(String, Value)> = serde_json::from_value(row["entries"].clone()).unwrap();
        let entries: Vec<_> = entries
            .into_iter()
            .enumerate()
            .map(|(index, (key, value))| {
                let undefined = row["undefinedIndices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v.as_u64() == Some(index as u64));
                (key, if undefined { None } else { Some(value) })
            })
            .collect();
        let key = row["key"].as_str().unwrap();
        let hit = js_table_lookup(&entries, key);
        let (kind, callable) = match hit {
            TableHit::Mapped(value) => {
                assert_eq!(
                    value.as_ref().unwrap_or(&Value::Null),
                    &row["mapped"],
                    "own shadow/last-wins {key}"
                );
                ("mapped", false)
            }
            TableHit::Inherited(member) => ("inherited", member != ProtoMember::Prototype),
            TableHit::Missing => ("missing", false),
        };
        let truthy = hit.is_truthy_by(|value| safe_cex_num::js_truthy(value.as_ref()));
        let nullish = hit.is_nullish_by(|value| value.as_ref().is_none_or(Value::is_null));
        assert_eq!(kind, row["kind"].as_str().unwrap(), "table lookup {key}");
        assert_eq!(
            callable,
            row["callable"].as_bool().unwrap(),
            "callability {key}"
        );
        assert_eq!(
            hit.is_present(),
            row["presence"].as_bool().unwrap(),
            "presence {key}"
        );
        assert_eq!(truthy, row["truthy"].as_bool().unwrap(), "truthiness {key}");
        assert_eq!(!truthy, row["orFallback"].as_bool().unwrap(), "OR {key}");
        assert_eq!(
            nullish,
            row["nullishFallback"].as_bool().unwrap(),
            "nullish {key}"
        );
        assert_eq!(!truthy, row["negated"].as_bool().unwrap(), "guard {key}");
        assert_eq!(
            truthy,
            row["callGuardReached"].as_bool().unwrap(),
            "dispatch guard {key}"
        );
        let json_hit = match hit {
            TableHit::Mapped(Some(value)) => TableHit::Mapped(value),
            TableHit::Mapped(None) => return,
            TableHit::Inherited(member) => TableHit::Inherited(member),
            TableHit::Missing => TableHit::Missing,
        };
        assert_eq!(json_hit.is_truthy(), truthy, "shared JSON truthiness {key}");
        assert_eq!(json_hit.is_nullish(), nullish, "JSON nullishness {key}");
    });
    source_cardinality::for_each(
        corpus["surrogates"].as_array().unwrap(),
        2,
        "table-surrogates",
        |witness| {
            assert!(
                serde_json::from_str::<String>(witness["json"].as_str().unwrap()).is_err(),
                "shared UTF-8 representation refusal"
            );
            assert_eq!(witness["emptyTruthy"], false);
            assert_eq!(witness["ownTruthy"], true);
        },
    );
    source_cardinality::for_each(
        corpus["numeric"].as_array().unwrap(),
        5,
        "table-numeric",
        |row| {
            let value: Value = serde_json::from_str(row["json"].as_str().unwrap()).unwrap();
            let hit = TableHit::Mapped(value);
            assert!(hit.is_present());
            assert_eq!(
                hit.is_truthy(),
                row["truthy"].as_bool().unwrap(),
                "mapped numeric {}",
                row["json"]
            );
            assert_eq!(hit.is_nullish(), row["nullish"].as_bool().unwrap());
        },
    );
    let expected = [
        ("constructor", ProtoMember::Constructor),
        ("__defineGetter__", ProtoMember::DefineGetter),
        ("__defineSetter__", ProtoMember::DefineSetter),
        ("hasOwnProperty", ProtoMember::HasOwnProperty),
        ("__lookupGetter__", ProtoMember::LookupGetter),
        ("__lookupSetter__", ProtoMember::LookupSetter),
        ("isPrototypeOf", ProtoMember::IsPrototypeOf),
        ("propertyIsEnumerable", ProtoMember::PropertyIsEnumerable),
        ("toString", ProtoMember::ToString),
        ("valueOf", ProtoMember::ValueOf),
        ("__proto__", ProtoMember::Prototype),
        ("toLocaleString", ProtoMember::ToLocaleString),
    ];
    let inherited = corpus["inherited"].as_array().unwrap();
    assert_eq!(inherited.len(), expected.len());
    let empty: [(&str, ()); 0] = [];
    for (key, member) in expected {
        let witness = inherited.iter().find(|row| row["key"] == key).unwrap();
        assert_eq!(js_table_lookup(&empty, key), TableHit::Inherited(member));
        assert_eq!(witness["truthy"], true, "inherited truthiness {key}");
        assert_eq!(
            member.js_string(),
            witness["string"].as_str().unwrap(),
            "inherited String conversion {key}"
        );
        assert_eq!(
            member.length().map(u64::from),
            witness["length"].as_u64(),
            "inherited length metadata {key}"
        );
        assert_eq!(witness["accessor"], key == "__proto__");
        assert_eq!(
            js_table_lookup(&empty, &key.to_uppercase()),
            TableHit::Missing,
            "existing uppercase lookup is immune: {key}"
        );
    }
}

#[test]
fn typed_table_callbacks_only_visit_mapped_values() {
    let values = [("zero", 0), ("one", 1)];
    let zero = js_table_lookup(&values, "zero");
    assert!(zero.is_present());
    assert!(!zero.is_truthy_by(|value| **value != 0));
    assert!(!zero.is_nullish_by(|_| false));
    for key in ["constructor", "__proto__", "missing"] {
        let hit = js_table_lookup(&values, key);
        let should_hit = key != "missing";
        assert_eq!(hit.is_truthy_by(|_| panic!("not mapped")), should_hit);
        assert_eq!(hit.is_nullish_by(|_| panic!("not mapped")), !should_hit);
    }
}
