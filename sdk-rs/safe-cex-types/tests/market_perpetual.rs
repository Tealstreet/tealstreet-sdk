use safe_cex_types::Market;
use serde_json::{Value, json};

fn market() -> Value {
    json!({"id":"btc_perp", "symbol":"BTC_PERP", "base":"BTC", "quote":"USDT",
        "active":true, "precision":{"amount":0.001,"price":0.1},
        "limits":{"amount":{"min":0.001,"max":10},"leverage":{"min":1,"max":100}}})
}

#[test]
fn market_perpetual_native_boolean_and_absence() {
    let mut decoded = Vec::new();
    for expected in [None, Some(true), Some(false)] {
        let mut input = market();
        if let Some(value) = expected {
            input["isPerpetual"] = json!(value);
        }
        let native: Market = serde_json::from_value(input).unwrap();
        assert_eq!(
            native.is_perpetual, expected,
            "MARKET native perpetual boolean"
        );
        assert_eq!(native.is_spot, None, "MARKET independent spot absence");
        decoded.push((native, expected));
    }
    let mut null = market();
    null["isPerpetual"] = Value::Null;
    assert!(
        serde_json::from_value::<Market>(null).is_err(),
        "MARKET present null refusal"
    );
    for (native, expected) in decoded {
        let wire = serde_json::to_value(native).unwrap();
        assert_eq!(
            wire.get("isPerpetual"),
            expected.map(|b| json!(b)).as_ref(),
            "MARKET wire perpetual presence"
        );
    }
}
