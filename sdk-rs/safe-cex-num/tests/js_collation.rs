#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::JsRootCollator;
use serde_json::Value;

#[test]
fn root_collation_matches_live_node() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_collation.cjs"
        ))
        .output()
        .unwrap();
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.status.success());
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let collator = JsRootCollator::try_new().unwrap();
    source_cardinality::for_each(rows, 165422, "collation", |row| {
        let order = collator.compare(row["a"].as_str().unwrap(), row["b"].as_str().unwrap());
        assert_eq!(
            order as i8 as i64,
            row["expected"].as_i64().unwrap(),
            "{row}"
        );
    });
}

#[test]
fn collation_data_versions_are_pinned() {
    let output = std::process::Command::new(env!("CARGO"))
        // A scoped test build does not cache every workspace package. Metadata
        // may fetch missing manifests; the lock and assertions still pin data.
        .args(["metadata", "--locked", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "collation metadata query failed before pin validation: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    for name in [
        "icu_collator_data",
        "icu_normalizer_data",
        "icu_locale_fallback_data",
    ] {
        let found: Vec<_> = packages.iter().filter(|p| p["name"] == name).collect();
        assert_eq!(found.len(), 1, "duplicate or missing {name}");
        assert_eq!(found[0]["version"], "2.3.0", "{name}");
        assert_eq!(
            found[0]["metadata"]["sources"]["cldr"]["tagged"], "48.2.1",
            "{name}"
        );
        assert_eq!(
            found[0]["metadata"]["sources"]["icuexport"]["tagged"], "release-78.1rc",
            "{name}"
        );
    }
}
