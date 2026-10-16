use safe_cex_venue_bingx::errors::{
    build_tradfi_one_way_mode_error, normalize_order_placement_error,
};
use serde_json::Value;

#[test]
fn errors_match_actual_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/errors_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "errors oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    println!("error helper capture: {}", corpus["runtime"]);
    let build = corpus["build"].as_array().unwrap();
    let normalize = corpus["normalize"].as_array().unwrap();
    assert_eq!(build.len(), 70, "build corpus census");
    assert_eq!(normalize.len(), 65, "normalize corpus census");
    let mut build_compared = 0;
    for case in build {
        assert_eq!(
            build_tradfi_one_way_mode_error(
                case["symbol"].as_str().unwrap(),
                case["message"].as_str()
            ),
            case["expected"].as_str().unwrap(),
            "native buildTradfiOneWayModeError {case}"
        );
        build_compared += 1;
    }
    assert_eq!(build_compared, 70, "BINGX-CARDINALITY build comparisons");
    let mut normalize_compared = 0;
    for case in normalize {
        assert_eq!(
            normalize_order_placement_error(
                case["symbol"].as_str().unwrap(),
                case["message"].as_str().unwrap()
            ),
            case["expected"].as_str().unwrap(),
            "native normalizeOrderPlacementError {case}"
        );
        normalize_compared += 1;
    }
    assert_eq!(
        normalize_compared, 65,
        "BINGX-CARDINALITY normalize comparisons"
    );
    let boundary = corpus["boundary"].as_array().unwrap();
    assert_eq!(boundary.len(), 2, "helper UTF-8 census");
    for case in boundary {
        for field in ["symbolUnits", "units"] {
            let units: Vec<u16> = case[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
                .collect();
            assert!(
                String::from_utf16(&units).is_err(),
                "UTF-8 helper boundary {case}"
            );
        }
    }
    println!(
        "{} build / {} normalize / {} UTF-8 witnesses",
        build.len(),
        normalize.len(),
        boundary.len()
    );
}
