use safe_cex_types::Market;
use safe_cex_venue_bingx::market_utils::{is_likely_tradfi_market, to_bingx_symbol};
use serde_json::Value;

#[test]
fn market_utils_match_actual_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/market_utils_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    println!("capture {}", corpus["runtime"]);
    let classify = corpus["classify"].as_array().unwrap();
    let resolve = corpus["resolve"].as_array().unwrap();
    assert_eq!(
        classify.len(),
        75,
        "BINGX-MARKET classification source total"
    );
    assert_eq!(resolve.len(), 138, "BINGX-MARKET resolution source total");
    let mut compared = 0;
    for case in classify {
        let market: Market = serde_json::from_value(case["input"].clone()).unwrap();
        assert_eq!(
            is_likely_tradfi_market(&market),
            case["expected"].as_bool().unwrap(),
            "BINGX-MARKET native classification {case}"
        );
        compared += 1;
    }
    assert_eq!(compared, 75, "BINGX-MARKET classification comparisons");
    let mut compared = 0;
    for case in resolve {
        let by_symbol: Vec<(String, Market)> =
            serde_json::from_value(case["bySymbol"].clone()).unwrap();
        let by_id: Vec<(String, Market)> = serde_json::from_value(case["byId"].clone()).unwrap();
        assert_eq!(
            to_bingx_symbol(case["symbol"].as_str().unwrap(), &by_symbol, &by_id),
            case["expected"].as_str().unwrap(),
            "BINGX-MARKET native resolution {case}"
        );
        compared += 1;
    }
    assert_eq!(compared, 138, "BINGX-MARKET resolution comparisons");
    let units: Vec<u16> = serde_json::from_value(corpus["boundary"]["units"].clone()).unwrap();
    assert!(
        String::from_utf16(&units).is_err(),
        "BINGX-MARKET UTF8 boundary"
    );
    assert_eq!(corpus["boundary"]["classify"], true);
}
