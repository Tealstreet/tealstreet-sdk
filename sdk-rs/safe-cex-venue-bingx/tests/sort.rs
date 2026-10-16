use safe_cex_num::JsRootCollator;
use safe_cex_venue_bingx::api::sort;
use serde_json::Value;

#[test]
fn sort_matches_actual_typescript() {
    let output = std::process::Command::new("node")
        .env("LC_ALL", "en_US.UTF-8")
        .env("LANG", "en_US.UTF-8")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/sort_oracle.cjs"
        ))
        .output()
        .expect("execute actual TypeScript sort initializer");
    assert!(
        output.status.success(),
        "sort oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    println!("sort capture: {}", corpus["runtime"]);
    let collator = JsRootCollator::try_new().unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 784, "sort corpus census");
    let mut compared = 0;
    for case in cases {
        let a = case["a"].as_str().unwrap();
        let b = case["b"].as_str().unwrap();
        assert_eq!(
            sort(&collator, a, b) as i8 as i64,
            case["expected"].as_i64().unwrap(),
            "native sort ordering {a:?}/{b:?}"
        );
        compared += 1;
    }
    assert_eq!(compared, 784, "BINGX-CARDINALITY sort comparisons");
    let boundary = corpus["boundary"].as_array().unwrap();
    assert_eq!(boundary.len(), 3, "sort UTF-8 boundary census");
    for case in boundary {
        let units: Vec<u16> = case["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
        assert!(
            case["expected"].is_number(),
            "TypeScript accepts lone surrogate sort key"
        );
        assert!(
            String::from_utf16(&units).is_err(),
            "Rust sort UTF-8 key refusal"
        );
    }
    println!(
        "{} comparator pairs; {} UTF-8 boundary witnesses",
        cases.len(),
        boundary.len()
    );
}
