use safe_cex_types::{ExchangeName, symbol_converter::*};
use serde_json::{Value, json};
#[test]
#[ignore = "runs current TypeScript symbol converter across all exchange pairs"]
fn symbol_conversion_typescript_differential() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_symbols_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(corpus["symbols"], 21);
    assert_eq!(corpus["names"], 49);
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 52500);
    for row in rows {
        let s = row["symbol"].as_str().unwrap();
        let from: Option<ExchangeName> = serde_json::from_value(row["from"].clone()).unwrap();
        let actual = match row["op"].as_str().unwrap() {
            "parseBinanceSymbol" => {
                let (base, quote) = parse_binance_symbol(s);
                json!({"base":base,"quote":quote})
            }
            "toBinance" => json!(to_binance(s, from.as_ref().unwrap())),
            "fromBinance" => json!(from_binance(s, from.as_ref().unwrap())),
            "convertSymbol" => {
                let to: ExchangeName = serde_json::from_value(row["to"].clone()).unwrap();
                json!(convert_symbol(s, from.as_ref().unwrap(), &to))
            }
            _ => unreachable!(),
        };
        assert_eq!(actual, row["expected"], "{row}");
    }
}
