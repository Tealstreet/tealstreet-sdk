#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_venue_okx::exchange::{
    TickerMarkInput, find_ticker_mark_for_currency, get_okx_trigger_px_type,
    parse_okx_trigger_px_type,
};
use serde_json::Value;

#[test]
fn anonymous_ticker_optionals_accept_absence_and_reject_present_null() {
    for key in ["id", "symbol", "mark"] {
        let mut invalid = serde_json::Map::new();
        invalid.insert(key.into(), Value::Null);
        assert!(
            serde_json::from_value::<TickerMarkInput>(Value::Object(invalid)).is_err(),
            "{key} may be absent, but never null"
        );
    }
    let missing: TickerMarkInput = serde_json::from_str("{}").unwrap();
    assert_eq!(find_ticker_mark_for_currency(&[missing], "ETH"), 1.0);
    let zero: TickerMarkInput = serde_json::from_str(r#"{"id":"ETH-USDT-SWAP","mark":0}"#).unwrap();
    assert_eq!(find_ticker_mark_for_currency(&[zero], "ETH"), 0.0);
}

#[test]
#[ignore = "requires Node and the isolated TypeScript compiler; mandatory in CI"]
fn maintained_typescript_exact_currency_match_and_trigger_policy() {
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/policy-oracle.mjs"
        ))
        .output()
        .expect("Node is required for the live TypeScript oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let inherited_control = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/policy-oracle.mjs"
        ))
        .arg("--mutate-inherited-ticker-field")
        .output()
        .unwrap();
    assert_eq!(inherited_control.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&inherited_control.stderr)
            .contains("ticker census requires direct type literal"),
        "{}",
        String::from_utf8_lossy(&inherited_control.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(cases.len(), 46);
    cardinality::assert_groups(
        &cases,
        "exchange_policy operations",
        |case| case["op"].as_str().unwrap().to_owned(),
        &[("mark", 35), ("trigger", 4), ("parse", 7)],
    );
    for (index, case) in cases.iter().enumerate() {
        match case["op"].as_str().unwrap() {
            "mark" => {
                let tickers =
                    serde_json::from_value::<Vec<TickerMarkInput>>(case["tickers"].clone())
                        .unwrap();
                assert_eq!(
                    find_ticker_mark_for_currency(&tickers, case["currency"].as_str().unwrap()),
                    case["expected"].as_f64().unwrap(),
                    "case {index}"
                );
            }
            "trigger" => assert_eq!(
                get_okx_trigger_px_type(
                    case.get("trigger")
                        .map(|v| serde_json::from_value(v.clone()).unwrap())
                ),
                case["expected"].as_str().unwrap()
            ),
            "parse" => assert_eq!(
                serde_json::to_value(parse_okx_trigger_px_type(case["trigger"].as_str())).unwrap(),
                case["expected"]
            ),
            op => panic!("unexpected operation {op}"),
        }
    }
}

#[test]
fn qualified_ticker_decoder_rejects_positional_arrays() {
    use serde::Deserialize;
    for input in [r#"["ETH-USDT-SWAP","ETHUSDT",1]"#, "[]", "[null,null,null]"] {
        let mut decoder = serde_json::Deserializer::from_str(input);
        assert!(
            TickerMarkInput::deserialize(&mut decoder).is_err(),
            "{input}"
        );
    }
}
