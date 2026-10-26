use safe_cex_num::{js_own_keys, normalize_json_order, normalize_object_order};
use serde_json::Value;
#[test]
fn own_key_order_matches_live_node() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_own_keys.cjs"
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 300);
    for row in rows {
        let keys: Vec<&str> = row["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        let actual = js_own_keys(keys);
        assert_eq!(serde_json::json!(actual), row["expected"], "{row}");
        let mut value: Value = serde_json::from_str(row["raw"].as_str().unwrap()).unwrap();
        let mut object = value.as_object().unwrap().clone();
        normalize_object_order(&mut object);
        assert_eq!(
            serde_json::to_string(&object).unwrap(),
            row["json"].as_str().unwrap(),
            "OWN-KEYS normalized object {row}"
        );
        normalize_json_order(&mut value);
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            row["json"].as_str().unwrap(),
            "OWN-KEYS normalized JSON {row}"
        );
        let mut nested = serde_json::json!([{"b":value,"1":0,"a":{"2":2,"1":1}}]);
        nested[0]["nested"] = serde_json::from_str(r#"[[{"2":2,"1":1}]]"#).unwrap();
        normalize_json_order(&mut nested);
        assert_eq!(
            nested[0]["a"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
        assert_eq!(
            nested[0]["nested"][0][0]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
        assert_eq!(
            nested[0]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["1", "b", "a", "nested"]
        );
    }
}
