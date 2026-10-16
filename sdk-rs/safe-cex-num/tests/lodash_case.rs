use safe_cex_num::{lodash_camel_case, lodash_snake_case};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

fn oracle_path() -> std::path::PathBuf {
    std::env::var_os("SAFE_CEX_NUM_ORACLE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests")))
        .join("lodash_case.cjs")
}

#[test]
fn lodash_compounders_match_pinned_live_package() {
    let mut command = Command::new("node");
    command.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/lodash_case.cjs"
    ));
    if std::env::var_os("SAFE_CEX_LODASH_CONTROL").is_some() {
        command.arg("--control-uppercase");
    }
    let mut child = command.stdout(Stdio::piped()).spawn().unwrap();
    let reader = BufReader::new(child.stdout.take().unwrap());
    let mut count = 0;
    let mut compared = 0;
    let mut seen = 0;
    let control = std::env::var("SOURCE_CARDINALITY_CONTROL").unwrap_or_default();
    let mut failures = Vec::new();
    for line in reader.lines() {
        let row: [String; 3] = serde_json::from_str(&line.unwrap()).unwrap();
        seen += 1;
        if control == "lodash-case:empty" || (control == "lodash-case:subset" && seen == 1_148_148)
        {
            continue;
        }
        count += 1;
        if control == "lodash-case:after" {
            continue;
        }
        let actual = [lodash_snake_case(&row[0]), lodash_camel_case(&row[0])];
        if actual != row[1..] && failures.len() < 20 {
            failures.push(format!("{:?}: {actual:?} != {:?}", row[0], &row[1..]));
        }
        compared += 1;
    }
    assert!(
        child.wait().unwrap().success(),
        "pinned lodash oracle must execute"
    );
    assert_eq!(
        count, 1_148_148,
        "SOURCE-CARDINALITY lodash-case source total"
    );
    assert_eq!(
        compared, 1_148_148,
        "SOURCE-CARDINALITY lodash-case compared total"
    );
    assert!(
        failures.is_empty(),
        "lodash compounder mismatches: {}",
        failures.join("\n")
    );
}

#[test]
fn lodash_oracle_rejects_missing_package() {
    let output = Command::new("node")
        .arg(oracle_path())
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/nonexistent-install/typescript"
            ),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
}
#[test]
fn lodash_oracle_detects_live_package_mutation() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "lodash_compounders_match_pinned_live_package",
            "--nocapture",
        ])
        .env("SAFE_CEX_LODASH_CONTROL", "uppercase")
        .output()
        .unwrap();
    assert!(!output.status.success(), "live-package mutation must fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("lodash compounder mismatches"),
        "mutation must fail at named comparison: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lodash_lone_surrogates_are_shared_representation_refusals() {
    let output = Command::new("node")
        .arg(oracle_path())
        .arg("--surrogate-witnesses")
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 4);
    for row in rows.as_array().unwrap() {
        let json = row["json"].as_str().unwrap();
        assert!(
            serde_json::from_str::<String>(json).is_err(),
            "shared UTF-8 representation refusal: {json}"
        );
        assert!(
            row["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|x| (0xd800..=0xdfff).contains(&x.as_u64().unwrap()))
        );
    }
    assert_eq!(rows[0]["snake"], serde_json::json!([]));
    assert_eq!(rows[1]["snake"], serde_json::json!([97, 95, 98]));
    assert_eq!(rows[1]["camel"], serde_json::json!([97, 66]));
}
