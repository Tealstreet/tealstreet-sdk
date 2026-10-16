use safe_cex_num::order::*;
use safe_cex_types::{Market, PositionSide};
use serde_json::{Value, json};

fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().expect("native number bits"), 16).unwrap())
}
fn optional(value: &Value) -> Option<f64> {
    (!value.is_null()).then(|| number(value))
}
fn assert_number(actual: f64, expected: &Value, label: &str) {
    let expected = number(expected);
    if expected.is_nan() {
        assert!(actual.is_nan(), "ORDER native {label} NaN");
    } else {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "ORDER native {label} bits"
        );
    }
}
fn market() -> Market {
    serde_json::from_value(json!({"id":"m","symbol":"BTC","base":"BTC","quote":"USDT","active":true,
        "precision":{"amount":0.1,"price":0.01},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}})).unwrap()
}
#[test]
fn order_arithmetic_matches_native_source_values() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_policy.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ORDER source execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    // Independent feeder dimensions: 15*9*3*2 + 8*8*2*8*8 + 7*9*2*6 + 3*7.
    assert_eq!(rows.len(), 9779, "ORDER source cardinality");
    let mut counts = [0; 4];
    for row in &rows {
        let op = row["op"].as_str().unwrap();
        match op {
            "roundToStep" | "roundMarketOrderSize" | "roundCloseSize" => {
                let value = number(&row["value"]);
                let step = number(&row["step"]);
                let actual = match op {
                    "roundToStep" => round_to_step(value, step),
                    "roundMarketOrderSize" => {
                        round_market_order_size(value, step, row["reduceOnly"].as_bool().unwrap())
                    }
                    _ => round_close_size(value, step),
                };
                assert_number(actual, &row["expected"], op);
                counts[0] += 1;
            }
            "breakevenPrice" => {
                let inputs = BreakevenInputs {
                    entry_price: number(&row["entryPrice"]),
                    size: number(&row["size"]),
                    side: if row["side"] == "long" {
                        PositionSide::Long
                    } else {
                        PositionSide::Short
                    },
                    banked_pnl: optional(&row["bankedPnl"]),
                    fees: optional(&row["fees"]),
                };
                assert_number(breakeven_price(&inputs), &row["expected"], "breakevenPrice");
                counts[1] += 1;
            }
            "notionalToNativeSize" => {
                let mut market = market();
                market.contract_size = optional(&row["contractSize"]);
                market.is_inverse = row["isInverse"].as_bool();
                assert_number(
                    notional_to_native_size(
                        number(&row["notional"]),
                        &market,
                        number(&row["price"]),
                    ),
                    &row["expected"],
                    "notionalToNativeSize",
                );
                counts[2] += 1;
            }
            "shouldRoundSizeDown" => {
                let mut market = market();
                market.is_spot = row["isSpot"].as_bool();
                assert_eq!(
                    should_round_size_down(&market, row["exchangeName"].as_str()),
                    row["expected"].as_bool().unwrap(),
                    "ORDER native shouldRoundSizeDown"
                );
                counts[3] += 1;
            }
            _ => panic!("ORDER unknown source operation {op}"),
        }
    }
    assert_eq!(counts, [810, 8192, 756, 21], "ORDER compared cardinality");
}
