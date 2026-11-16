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
    for name in ["cryptocom.json", "kucoinspot.json"] {
        files.iter_mut().find(|(file, _)| file == name).unwrap().1 =
            r#"{"status":"ready","probeTests":"src/delegated.rs","sessionTests":"src/session/tests/cosigned_venues.rs"}"#.into();
    }
    let result = catalog::render(&root, files).unwrap();
    let ready = result
        .lines()
        .find(|line| line.contains("const PRIVATE_STARTUP_VENUES"))
        .unwrap();
    assert!(ready.contains("\"cryptocom\""));
    assert!(ready.contains("\"kucoinspot\""));
}
