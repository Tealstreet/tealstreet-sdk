use safe_cex_num::sizing::*;
use safe_cex_types::{Balance, Market};
use serde_json::{Value, json};
fn equal(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            assert_eq!(a.as_f64().unwrap().to_bits(), b.as_f64().unwrap().to_bits())
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                equal(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect()
            );
            for (key, value) in a {
                equal(value, &b[key]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
fn number(value: &Value) -> f64 {
    match value.as_str().unwrap() {
        "NaN" => f64::NAN,
        "-0" => -0.0,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        v => v.parse().unwrap(),
    }
}
#[test]
#[ignore = "runs current TypeScript sizing and math sources"]
fn sizing_typescript_differential() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_sizing_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 3939);
    for row in rows {
        let op = row["op"].as_str().unwrap();
        let market: Option<Market> = if row["market"].is_null() {
            None
        } else {
            let mut v = json!({"id":"m","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}});
            v.as_object_mut()
                .unwrap()
                .extend(row["market"].as_object().unwrap().clone());
            Some(serde_json::from_value(v).unwrap())
        };
        let m = market.as_ref();
        let s = if row["size"].is_null() {
            0.0
        } else {
            number(&row["size"])
        };
        let p = row["price"].as_f64().unwrap_or(0.0);
        let entry = row["entry"].as_f64().unwrap_or(0.0);
        let stop = row["stop"].as_f64().unwrap_or(0.0);
        let btc = row["btc"].as_f64();
        let balance: Option<Balance> = if row["balance"].is_object() {
            Some(serde_json::from_value(row["balance"].clone()).unwrap())
        } else {
            None
        };
        let mut text = None;
        let mut object = None;
        let value = match op {
            "convertFromQuoteSizeToNative" => convert_from_quote_size_to_native(s, m.unwrap(), p),
            "convertFromBaseSizeToNative" => convert_from_base_size_to_native(s, m.unwrap(), p),
            "convertFromNativeSizeToQuote" => convert_from_native_size_to_quote(s, m.unwrap(), p),
            "convertFromNativeSizeToBase" => convert_from_native_size_to_base(s, m.unwrap(), p),
            "convertMarginToSizeCurrency" => convert_margin_to_size_currency(s, m.unwrap(), p, btc),
            "convertSizeToMarginCurrency" => convert_size_to_margin_currency(s, m.unwrap(), p, btc),
            "convertFromMarginToQuote" => convert_from_margin_to_quote(s, m.unwrap(), Some(p), btc),
            "getSizeCurrency" => {
                text = Some(get_size_currency(m));
                0.0
            }
            "getMarginCurrency" => {
                text = Some(get_margin_currency(m, "fallback"));
                0.0
            }
            "getContractType" => {
                text = Some(get_contract_type(m));
                0.0
            }
            "calculatePnlInMarginCurrency" => {
                calculate_pnl_in_margin_currency(s, entry, p, s > 0.0, m.unwrap(), btc)
            }
            "convertMarginRiskToNativeSize" => {
                convert_margin_risk_to_native_size(s, entry, stop, m.unwrap(), btc)
            }
            "calculatePositionAccountLeverage" => calculate_position_account_leverage(
                row["notionals"][0].as_f64().unwrap_or(0.0),
                row["balance"].as_f64().unwrap(),
            ),
            "calculateTotalAccountLeverage" => calculate_total_account_leverage(
                &serde_json::from_value::<Vec<f64>>(row["notionals"].clone()).unwrap(),
                row["balance"].as_f64().unwrap(),
            ),
            "calculateTotalNotional" => calculate_total_notional(
                &serde_json::from_value::<Vec<f64>>(row["notionals"].clone()).unwrap(),
            ),
            "getBalanceSlice" => {
                let selected = get_balance_slice(balance.as_ref(), m);
                assert_native_balance(
                    selected.and_then(BalanceSlice::as_balance),
                    &row["expected"],
                );
                object = Some(serde_json::to_value(selected).unwrap());
                0.0
            }
            "getTotalBalanceFromBalance" => get_total_balance_from_balance(balance.as_ref(), m),
            _ => panic!("{op}"),
        };
        if let Some(text) = text {
            assert_eq!(text, row["expected"].as_str().unwrap(), "{row}");
        } else if let Some(object) = object {
            equal(&object, &row["expected"]);
        } else {
            let expected = number(&row["expected"]);
            assert!(
                value.is_nan() && expected.is_nan() || value.to_bits() == expected.to_bits(),
                "{row}\nactual {value:?} expected {expected:?}"
            );
        }
    }
}

fn assert_native_balance(actual: Option<&Balance>, expected: &Value) {
    let Some(actual) = actual else {
        assert!(expected.is_null(), "SIZING native balance absence");
        return;
    };
    for (name, value) in [
        ("used", actual.used),
        ("free", actual.free),
        ("total", actual.total),
        ("upnl", actual.upnl),
    ] {
        assert_eq!(
            value.to_bits(),
            expected[name].as_f64().unwrap().to_bits(),
            "SIZING native balance field {name}"
        );
    }
    if let Some(currencies) = &actual.currencies {
        assert_eq!(
            currencies.len(),
            expected["currencies"].as_object().unwrap().len(),
            "SIZING native currencies count"
        );
        for (key, value) in currencies.iter() {
            assert_native_balance(value.as_balance(), &expected["currencies"][key]);
        }
    } else {
        assert!(
            expected.get("currencies").is_none(),
            "SIZING native currencies absence"
        );
    }
}

#[test]
fn balance_slice_preserves_available_locked_row_and_missing_total_default() {
    let balance: Balance = serde_json::from_value(json!({
        "used":6,"free":9,"total":15,"upnl":0,
        "currencies":{"USD":{"available":9,"locked":6}}
    }))
    .unwrap();
    let market: Market = serde_json::from_value(json!({"id":"m","symbol":"BTCUSD","base":"BTC","quote":"USD","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}})).unwrap();
    let selected = get_balance_slice(Some(&balance), Some(&market)).unwrap();
    assert!(selected.as_balance().is_none());
    let source_row = serde_json::to_value(selected).unwrap();
    assert_eq!(source_row, json!({"available":9.0,"locked":6.0}));
    assert_eq!(
        get_total_balance_from_balance(Some(&balance), Some(&market)),
        0.0
    );
    assert_eq!(get_total_balance_from_balance(Some(&balance), None), 15.0);
}
