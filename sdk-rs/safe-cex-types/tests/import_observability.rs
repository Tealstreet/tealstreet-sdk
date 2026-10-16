use safe_cex_types::accounts::parse_import_account_data;
use serde_json::Value;
use std::collections::BTreeMap;
use std::process::Command;

#[test]
fn actual_importer_returned_units_are_observable_before_serialization() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/import_observability_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(capture["header"]["architecture"].as_str().is_some());
    assert!(capture["header"]["node"].as_str().is_some());
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 10, "IMPORT-OBSERVABILITY source total");
    let mut indexed = BTreeMap::new();
    for row in rows {
        assert!(indexed.insert(row["id"].as_str().unwrap(), row).is_none());
    }
    let cases: [(&str, &[u16], bool); 10] = [
        ("unknown-high", &[0xd800], false),
        ("unknown-low", &[0xdc00], false),
        ("name-high", &[0xd800], false),
        ("array-high", &[0xd800], false),
        ("unknown-key-high", &[0xd800], false),
        ("unknown-pair", &[0xd83d, 0xdca9], true),
        ("unknown-ascii-escape", &[92, 117, 100, 56, 48, 48], true),
        ("unknown-scalar", &[97, 98, 99], true),
        ("unknown-replacement", &[0xfffd], true),
        ("deep-unknown", &[], false),
    ];
    let mut outcomes = 0;
    let mut returned_views = 0;
    let mut positive_values = 0;
    for (id, units, accepted) in cases {
        let row = indexed
            .get(id)
            .expect("IMPORT-OBSERVABILITY independent case coverage");
        assert_eq!(row["valid"], true, "IMPORT-OBSERVABILITY source acceptance");
        let count = if id == "array-high" { 2 } else { 1 };
        assert_eq!(
            row["count"], count,
            "IMPORT-OBSERVABILITY source account count"
        );
        assert_eq!(row["array"], id == "array-high");
        let views = row["views"].as_array().unwrap();
        assert_eq!(
            views.len(),
            count,
            "IMPORT-OBSERVABILITY returned view count"
        );
        returned_views += views.len();
        if id == "deep-unknown" {
            assert_eq!(views[0]["type"], "object");
            assert_eq!(views[0]["depth"], 130);
            assert_eq!(views[0]["leaf"], 0);
        } else {
            assert_eq!(
                views[0]["type"], "string",
                "IMPORT-OBSERVABILITY returned string"
            );
            assert_eq!(views[0]["length"], units.len());
            assert_eq!(
                views[0]["units"],
                serde_json::json!(units),
                "IMPORT-OBSERVABILITY returned unit identity"
            );
        }
        if id == "array-high" {
            assert_eq!(views[1]["type"], "string");
            assert_eq!(views[1]["length"], 4);
            assert_eq!(views[1]["units"], serde_json::json!([115, 97, 102, 101]));
        }
        let native = parse_import_account_data(row["raw"].as_str().unwrap());
        assert_eq!(
            native.is_valid, accepted,
            "IMPORT-OBSERVABILITY native acceptance {id}"
        );
        if accepted {
            let data = native
                .data
                .as_ref()
                .expect("IMPORT-OBSERVABILITY native account present");
            let value = data["unknown"]
                .as_str()
                .expect("IMPORT-OBSERVABILITY native field present");
            assert_eq!(
                value.encode_utf16().collect::<Vec<_>>(),
                units,
                "IMPORT-OBSERVABILITY native unit identity"
            );
            positive_values += 1;
        } else {
            assert!(
                native.data.is_none(),
                "IMPORT-OBSERVABILITY whole import refusal"
            );
            assert_eq!(native.error.as_deref(), Some("Invalid JSON format"));
        }
        outcomes += 1;
    }
    assert_eq!(outcomes, 10, "IMPORT-OBSERVABILITY native outcome count");
    assert_eq!(
        returned_views, 11,
        "IMPORT-OBSERVABILITY source returned views"
    );
    assert_eq!(
        positive_values, 4,
        "IMPORT-OBSERVABILITY native positive values"
    );
}
