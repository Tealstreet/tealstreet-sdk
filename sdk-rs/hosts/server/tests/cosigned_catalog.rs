#[path = "../catalog_build.rs"]
mod catalog;

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn records(root: &Path) -> Vec<(String, String)> {
    std::fs::read_dir(root.join(catalog::DIRECTORY))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                std::fs::read_to_string(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn generated_catalog_is_deterministic_and_covers_the_current_sources() {
    let root = root();
    let mut files = records(&root);
    let expected = catalog::generate(&root).unwrap();
    files.reverse();
    assert_eq!(catalog::render(&root, files).unwrap(), expected);
}

#[test]
fn missing_unknown_duplicate_or_ambiguous_records_refuse_the_build() {
    let root = root();
    let original = records(&root);
    let mut missing = original.clone();
    missing.pop();
    assert!(
        catalog::render(&root, missing)
            .unwrap_err()
            .contains("unaccounted")
    );
    for name in ["foreign.json", "bybit.json"] {
        let mut bad = original.clone();
        bad.push((
            name.into(),
            "{\"status\":\"excluded\",\"reason\":\"no proof\"}".into(),
        ));
        assert!(
            catalog::render(&root, bad)
                .unwrap_err()
                .contains("unknown or duplicate")
        );
    }
    for text in [
        r#"{"status":"ready","reason":"still excluded"}"#,
        r#"{"status":"excluded","reason":" "}"#,
        r#"{"status":"ready","status":"excluded","reason":"ambiguous"}"#,
        r#"{"status":"ready","probeTests":"../outside.rs","sessionTests":"src/session.rs"}"#,
        r#"{"status":"ready","probeTests":"src/missing.rs","sessionTests":"src/session.rs"}"#,
    ] {
        let mut bad = original.clone();
        bad.iter_mut()
            .find(|(name, _)| name == "bybit.json")
            .unwrap()
            .1 = text.into();
        assert!(catalog::render(&root, bad).is_err(), "{text}");
    }
}

#[test]
fn independent_venue_promotions_need_no_shared_registry_edit() {
    let root = root();
    let mut files = records(&root);
    let before = catalog::render(&root, files.clone()).unwrap();
    let mut promoted: Vec<_> = files
        .iter()
        .filter(|(_, text)| {
            serde_json::from_str::<serde_json::Value>(text).unwrap()["status"] == "excluded"
        })
        .map(|(name, _)| name.clone())
        .collect();
    promoted.sort();
    promoted.truncate(2);
    assert_eq!(promoted.len(), 2);
    for name in &promoted {
        files.iter_mut().find(|(file, _)| file == name).unwrap().1 =
            r#"{"status":"ready","probeTests":"src/delegated.rs","sessionTests":"src/session/tests/cosigned_venues.rs"}"#.into();
    }
    let result = catalog::render(&root, files).unwrap();
    let ready = result
        .lines()
        .find(|line| line.contains("const PRIVATE_STARTUP_VENUES"))
        .unwrap();
    let previous = before
        .lines()
        .find(|line| line.contains("const PRIVATE_STARTUP_VENUES"))
        .unwrap();
    assert_eq!(
        ready.matches('"').count(),
        previous.matches('"').count() + 4
    );
    for name in &promoted {
        let venue = format!("\"{}\"", name.strip_suffix(".json").unwrap());
        assert!(!previous.contains(&venue));
        assert!(ready.contains(&venue));
    }
}

#[test]
fn partial_asterdex_record_keeps_a_closed_v3_shape_exclusion() {
    let root = root();
    let original = records(&root);
    let ready = serde_json::json!({
        "status": "ready",
        "probeTests": "src/delegated/asterdex_probe_tests.rs",
        "sessionTests": "src/session/tests/cosigned_venues.rs",
        "excludedShapes": ["v3"]
    });
    let render = |venue: &str, record: &serde_json::Value| {
        let mut files = original.clone();
        files
            .iter_mut()
            .find(|(name, _)| name == &format!("{venue}.json"))
            .unwrap()
            .1 = record.to_string();
        catalog::render(&root, files)
    };
    let generated = render("asterdex", &ready).expect("V1 with its explicit V3 exclusion");
    assert!(generated.contains("\"asterdex\""));
    for shapes in [
        serde_json::json!([]),
        serde_json::json!(["unknown-shape"]),
        serde_json::json!(["v3", "v3"]),
        serde_json::json!("v3"),
        serde_json::Value::Null,
    ] {
        let mut bad = ready.clone();
        bad["excludedShapes"] = shapes;
        assert!(render("asterdex", &bad).is_err(), "{bad}");
    }
    let mut missing = ready.clone();
    missing.as_object_mut().unwrap().remove("excludedShapes");
    assert!(render("asterdex", &missing).is_err());
    assert!(render("bybit", &ready).is_err());
}
