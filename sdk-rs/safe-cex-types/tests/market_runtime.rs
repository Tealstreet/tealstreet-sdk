use safe_cex_types::{Market, MarketFees, MarketRuntime};
use serde_json::json;

#[test]
fn market_runtime_fields_are_optional_and_share_the_market_wire_object() {
    let original = json!({"id":"A","symbol":"A","base":"A","quote":"B","active":true,
        "precision":{"amount":0.1,"price":0.2},
        "limits":{"amount":{"min":0.0,"max":1.0},"leverage":{"min":1.0,"max":2.0}}});
    let mut market: Market = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(market.runtime, MarketRuntime::default());
    assert_eq!(serde_json::to_value(&market).unwrap(), original);
    market.runtime = MarketRuntime {
        kind: Some("spot".into()),
        spot: Some(true),
        swap: Some(false),
        future: Some(false),
        contract: Some(false),
        linear: Some(false),
        inverse: Some(false),
        fees: Some(MarketFees {
            maker: -0.0001,
            taker: 0.0003,
        }),
    };
    let encoded = serde_json::to_value(&market).unwrap();
    assert_eq!(encoded["type"], "spot");
    assert_eq!(encoded["spot"], true);
    for name in ["swap", "future", "contract", "linear", "inverse"] {
        assert_eq!(
            encoded[name], false,
            "present false runtime property {name}"
        );
    }
    assert_eq!(market.runtime.fees.as_ref().unwrap().maker, -0.0001);
    assert_eq!(market.runtime.fees.as_ref().unwrap().taker, 0.0003);
    assert_eq!(encoded["fees"], json!({"maker":-0.0001,"taker":0.0003}));
    assert!(encoded.get("runtime").is_none());
    assert_eq!(serde_json::from_value::<Market>(encoded).unwrap(), market);
    assert!(serde_json::from_value::<MarketRuntime>(json!([])).is_err());
    assert!(serde_json::from_value::<MarketRuntime>(json!({"spot": null})).is_err());
}

#[test]
fn runtime_fees_require_an_object_with_both_numeric_fields() {
    for value in [
        json!([]),
        json!({"maker": 1}),
        json!({"maker": null, "taker": 2}),
        json!({"maker": "1", "taker": 2}),
    ] {
        assert!(serde_json::from_value::<MarketFees>(value).is_err());
    }
    assert!(serde_json::from_value::<MarketRuntime>(json!({"fees": null})).is_err());
}
