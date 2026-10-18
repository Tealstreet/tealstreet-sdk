use safe_cex_types::MarketPrecision;
use safe_cex_venue_bingx::ticker::derive_ticker_bid_ask;
use serde_json::Value;

fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
#[test]
fn ticker_matches_actual_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ticker_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ticker oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    println!("ticker capture: {}", corpus["runtime"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 168, "ticker corpus census");
    let mut compared = 0;
    for case in cases {
        let precision = (!case["precision"].is_null()).then(|| MarketPrecision {
            base: None,
            quote: None,
            price: number(&case["precision"]["price"]),
            amount: number(&case["precision"]["amount"]),
        });
        let (bid, ask) = derive_ticker_bid_ask(precision.as_ref(), number(&case["last"]));
        assert_eq!(
            bits(bid),
            case["bid"].as_str().unwrap(),
            "BINGX-PRECISION native bid {case}"
        );
        assert_eq!(
            bits(ask),
            case["ask"].as_str().unwrap(),
            "BINGX-PRECISION native ask {case}"
        );
        compared += 2;
    }
    assert_eq!(compared, 336, "BINGX-CARDINALITY bid/ask comparisons");
    println!("{} native bid/ask cases", cases.len());
}
