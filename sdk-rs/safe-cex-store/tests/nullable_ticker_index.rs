use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::Ticker;
use serde_json::{Value, json};

fn nullable_index_ticker_fixture(index: Value) -> Value {
    json!({"id":"SOLV","symbol":"SOLVUSDT","bid":1.0,"ask":2.0,"last":3.0,"mark":4.0,
        "index":index,"percentage":6.0,"openInterest":7.0,"fundingRate":8.0,"fundingTime":9.0,
        "volume":10.0,"quoteVolume":11.0})
}

#[test]
fn null_index_is_a_required_nullable_number_through_canonical_ingress() {
    let input = nullable_index_ticker_fixture(Value::Null);
    let routes: [Ticker; 4] = [
        serde_json::from_value(input.clone()).unwrap(),
        js_from_str(&input.to_string()).unwrap(),
        js_from_value(input.clone()).unwrap(),
        js_from_value_ref(&input).unwrap(),
    ];
    for ticker in routes {
        assert_eq!(ticker.index, None);
        assert_eq!(ticker.mark, 4.0);
        assert_eq!(serde_json::to_value(ticker).unwrap(), input);
    }
    for invalid in [json!("1"), json!(false), json!([]), json!({})] {
        assert!(js_from_value::<Ticker>(nullable_index_ticker_fixture(invalid)).is_err());
    }
    let mut missing = input;
    missing.as_object_mut().unwrap().remove("index");
    assert!(
        js_from_value::<Ticker>(missing).is_err(),
        "index stays required"
    );
    for (token, expected) in [("-0", -0.0_f64), ("1e400", f64::INFINITY)] {
        let input = nullable_index_ticker_fixture(Value::Null)
            .to_string()
            .replace("\"index\":null", &format!("\"index\":{token}"));
        let ticker: Ticker = js_from_str(&input).unwrap();
        assert_eq!(ticker.index.unwrap().to_bits(), expected.to_bits());
    }
}
