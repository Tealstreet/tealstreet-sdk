use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::Market;
use safe_cex_venue_bybit::variant_accessors::*;
use serde_json::Value;

fn input(case: &Value) -> Option<Value> {
    case["raw"]
        .as_str()
        .map(|raw| serde_json::from_str(raw).unwrap())
}
fn markets(case: &Value) -> Vec<Market> {
    serde_json::from_value(case["markets"].clone()).unwrap()
}
fn strings(value: &Value) -> Vec<String> {
    serde_json::from_value(value.clone()).unwrap()
}

#[test]
#[ignore = "requires installed live TypeScript"]
fn variant_accessors_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/variant-accessor-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let f: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        f["accounts"].as_array().unwrap().len(),
        38,
        "VARIANT independent account floor"
    );
    for case in f["accounts"].as_array().unwrap() {
        let status = input(case);
        assert_eq!(
            account_type(status.as_ref()),
            case["result"].as_str().unwrap(),
            "VARIANT native account type"
        );
    }
    let mut first = inverse_supported_categories();
    first.push("changed");
    assert_eq!(
        inverse_supported_categories(),
        strings(&f["configuration"]["inverseCategories"]),
        "VARIANT native inverse categories"
    );
    let mut first = hyro_supported_categories();
    first.push("changed");
    assert_eq!(
        hyro_supported_categories(),
        strings(&f["configuration"]["hyroCategories"]),
        "VARIANT native hyro categories"
    );
    assert_eq!(
        hyro_supports_trade_derived_candles(),
        f["configuration"]["hyroCandles"].as_bool().unwrap(),
        "VARIANT native hyro candles"
    );
    assert_eq!(
        hyro_requires_order_link_id(),
        f["configuration"]["hyroLinkId"].as_bool().unwrap(),
        "VARIANT native hyro link ID"
    );

    assert_eq!(
        f["categories"].as_array().unwrap().len(),
        12,
        "VARIANT independent category floor"
    );
    for case in f["categories"].as_array().unwrap() {
        let mut warnings = Vec::new();
        let result = inverse_get_category_for_symbol(
            case["symbol"].as_str().unwrap(),
            &markets(case),
            |message| warnings.push(message.to_owned()),
        );
        assert_eq!(
            result,
            case["result"].as_str().unwrap(),
            "VARIANT native symbol category"
        );
        assert_eq!(
            warnings,
            strings(&case["warnings"]),
            "VARIANT native category warning"
        );
    }
    assert_eq!(
        f["tickers"].as_array().unwrap().len(),
        40,
        "VARIANT independent ticker floor"
    );
    for case in f["tickers"].as_array().unwrap() {
        let mut calls = Vec::new();
        let active = strings(&case["active"]);
        // Explicit host boundary: capture every argument and return the source
        // base method's tested policy. This slice maps only the venue delegate.
        let subscription = |available: &[String]| {
            calls.push(available.to_vec());
            let filtered: Vec<_> = available
                .iter()
                .filter(|s| active.contains(s))
                .cloned()
                .collect();
            if filtered.is_empty() {
                available.to_vec()
            } else {
                filtered
            }
        };
        let markets = markets(case);
        let result = match case["family"].as_str().unwrap() {
            "inverse" => inverse_get_ticker_symbols_for_public_stream(&markets, subscription),
            "spot" => spot_get_ticker_symbols_for_public_stream(&markets, subscription),
            _ => panic!("unknown variant"),
        };
        assert_eq!(
            result,
            strings(&case["result"]),
            "VARIANT native ticker result"
        );
        assert_eq!(
            calls,
            serde_json::from_value::<Vec<Vec<String>>>(case["calls"].clone()).unwrap(),
            "VARIANT native ticker delegation"
        );
    }
    assert_eq!(
        f["settles"].as_array().unwrap().len(),
        104,
        "VARIANT independent settle floor"
    );
    for case in f["settles"].as_array().unwrap() {
        let mut logs = Vec::new();
        let host_error = ExchangeError {
            name: "HostError".into(),
            ..ExchangeError::new("host logging failed")
        };
        let emit = |message: &str| {
            logs.push(message.to_owned());
            if case["hostThrows"] == true {
                Err(host_error.clone())
            } else {
                Ok(())
            }
        };
        let category = case["category"].as_str().unwrap();
        let status = input(case);
        let result = match case["family"].as_str().unwrap() {
            "inverse" => inverse_get_settle_coins_for_category(category, status.as_ref(), emit),
            "hyro" => hyro_get_settle_coins_for_category(category, emit)
                .map(|coins| coins.into_iter().map(Some).collect()),
            _ => panic!("unknown variant"),
        };
        if let Some(expected) = case["result"]["ok"].as_array() {
            let actual = result.unwrap();
            assert_eq!(actual.len(), expected.len(), "VARIANT native settle count");
            for (actual, expected) in actual.into_iter().zip(expected) {
                if expected["undefined"] == true {
                    assert!(actual.is_none(), "VARIANT native undefined settle coin");
                } else {
                    assert_eq!(actual, expected.as_str(), "VARIANT native settle coin");
                }
            }
        } else {
            let error = result.unwrap_err();
            assert_eq!(
                error.name,
                case["result"]["error"].as_str().unwrap(),
                "VARIANT native settle error name"
            );
            assert_eq!(
                error.message,
                case["result"]["message"].as_str().unwrap(),
                "VARIANT native settle error message"
            );
            if case["hostThrows"] == true {
                assert_eq!(case["result"]["marker"], 37);
                assert_eq!(error, host_error, "VARIANT native host error retained");
            }
        }
        assert_eq!(
            logs,
            strings(&case["logs"]),
            "VARIANT native settle logging"
        );
    }
    assert_eq!(
        f["numeric"].as_array().unwrap().len(),
        29,
        "VARIANT independent numeric floor"
    );
    for case in f["numeric"].as_array().unwrap() {
        let value = input(case);
        assert_eq!(
            hyro_is_finite_numeric(value.as_ref()),
            case["result"].as_bool().unwrap(),
            "VARIANT native finite numeric"
        );
    }
}
