use safe_cex_exchange::cross_trade::{CrossTradeConfig, CrossTradeCopier};

#[test]
fn public_numeric_weights_retain_javascript_number_bits() {
    let cases = [
        ("1.5", 0x3ff8_0000_0000_0000),
        ("-0", 0x8000_0000_0000_0000),
        ("-0.0", 0x8000_0000_0000_0000),
        ("1e400", 0x7ff0_0000_0000_0000),
        ("-1e400", 0xfff0_0000_0000_0000),
        ("-1e-999", 0x8000_0000_0000_0000),
        ("1e-999", 0),
        ("2", 0x4000_0000_0000_0000),
    ];
    for (token, expected) in cases {
        let copier = format!(
            r#"{{"enabled":true,"account":"copy","weight":{token},"execution":"Market","weightMode":"Absolute","symbolMappings":{{}}}}"#
        );
        let config = format!(
            r#"{{"enabled":true,"masterAccount":"master","masterSymbol":null,"masterWeight":{token},"copiers":[{copier}]}}"#
        );
        for buffered in [false, true] {
            let decoded: CrossTradeConfig = if buffered {
                serde_json::from_value(serde_json::from_str(&config).unwrap())
            } else {
                serde_json::from_str(&config)
            }
            .expect("CROSS-NUMERIC native config accepted");
            assert_eq!(
                decoded.master_weight.to_bits(),
                expected,
                "CROSS-NUMERIC native master bits: {token}/{buffered}"
            );
            assert_eq!(
                decoded.copiers.len(),
                1,
                "CROSS-NUMERIC nested copier population"
            );
            assert_eq!(
                decoded.copiers[0].weight.to_bits(),
                expected,
                "CROSS-NUMERIC nested copier bits: {token}/{buffered}"
            );
            let decoded: CrossTradeCopier = if buffered {
                serde_json::from_value(serde_json::from_str(&copier).unwrap())
            } else {
                serde_json::from_str(&copier)
            }
            .expect("CROSS-NUMERIC native copier accepted");
            assert_eq!(
                decoded.weight.to_bits(),
                expected,
                "CROSS-NUMERIC native copier bits: {token}/{buffered}"
            );
        }
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/cross-trade-numeric-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CROSS-NUMERIC actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
