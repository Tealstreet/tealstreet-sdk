#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::TableHit;
use safe_cex_num::get_key::{GetKeyObject, GetKeyValue, v, vn};
use serde_json::{Value, json};

fn encode(hit: TableHit<GetKeyValue<'_>>) -> Value {
    match hit {
        TableHit::Missing | TableHit::Mapped(GetKeyValue::Undefined) => json!({"kind":"undefined"}),
        TableHit::Mapped(GetKeyValue::Json(value)) => json!({"kind":"json", "value":value}),
        TableHit::Inherited(member) => json!({"kind":"inherited", "text":member.js_string()}),
    }
}

#[test]
fn source_get_key_preserves_null_undefined_and_inherited_values() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/get_key.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for escaped in [r#""\ud800""#, r#""a\udfff""#] {
        assert!(
            serde_json::from_str::<String>(escaped).is_err(),
            "chosen UTF-8 ingress refusal {escaped}"
        );
    }
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality::for_each(rows, 836, "get-key", |row| {
        let entries: Vec<_> = row["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                (
                    entry[0].as_str().unwrap().to_owned(),
                    if entry[2] == true {
                        None
                    } else {
                        Some(entry[1].clone())
                    },
                )
            })
            .collect();
        let keys: Vec<_> = row["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap())
            .collect();
        let object = Some(GetKeyObject::Entries(&entries));
        assert_eq!(encode(v(object, keys[0])), row["v"], "v presence {row}");
        assert_eq!(
            encode(vn(object, &keys)),
            row["vn"],
            "vn alias termination {row}"
        );
    });
    let fields =
        serde_json::from_value::<serde_json::Map<String, Value>>(json!({"first":null,"second":9}))
            .unwrap();
    assert_eq!(
        encode(vn(Some(GetKeyObject::Json(&fields)), &["first", "second"])),
        json!({"kind":"json","value":null})
    );
    assert_eq!(encode(v(None, "constructor")), json!({"kind":"undefined"}));
}
