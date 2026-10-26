use safe_cex_types::{
    ExchangeOptions, Market, OptionRight, OptionTermsSettlement, OptionTermsStyle, PlaceOrderOpts,
    Position,
};
use serde_json::{Value, json};

fn market() -> Value {
    json!({"id":"BTC-27DEC26-100000-C", "symbol":"BTC-27DEC26-100000-C", "base":"BTC",
        "quote":"USDC", "active":true, "precision":{"amount":0.01,"price":5},
        "limits":{"amount":{"min":0.01,"max":500},"leverage":{"min":1,"max":1}}})
}

#[test]
fn market_option_terms_and_price_limit_decode_natively_and_round_trip() {
    let mut input = market();
    input["limits"]["price"] = json!({"min": 5});
    input["option"] = json!({"underlying":"BTC", "expiry":1798358400000.0_f64, "strike":100000,
        "right":"call", "style":"european", "settlement":"cash", "premiumAsset":"USDC",
        "settleAsset":"USDC"});
    let native: Market = serde_json::from_value(input.clone()).unwrap();
    let option = native.option.as_ref().expect("OPTION native terms");
    assert_eq!(option.underlying, "BTC", "OPTION native underlying");
    assert_eq!(
        option.expiry.to_bits(),
        1798358400000.0_f64.to_bits(),
        "OPTION native expiry"
    );
    assert_eq!(
        option.strike.to_bits(),
        100000.0_f64.to_bits(),
        "OPTION native strike"
    );
    assert_eq!(option.right, OptionRight::Call, "OPTION native right");
    assert_eq!(
        option.style,
        OptionTermsStyle::European,
        "OPTION native style"
    );
    assert_eq!(
        option.settlement,
        OptionTermsSettlement::Cash,
        "OPTION native settlement"
    );
    assert_eq!(option.premium_asset, "USDC", "OPTION native premium asset");
    assert_eq!(option.settle_asset, "USDC", "OPTION native settle asset");
    assert_eq!(
        native.limits.price.as_ref().map(|p| p.min.to_bits()),
        Some(5.0_f64.to_bits()),
        "OPTION native price limit"
    );
    // Number lexemes are not the claim (5 vs 5.0); field names, order and values are.
    let wire = serde_json::to_value(&native).unwrap();
    let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&wire), keys(&input), "OPTION wire market key order");
    assert_eq!(
        keys(&wire["option"]),
        keys(&input["option"]),
        "OPTION wire terms key order"
    );
    assert_eq!(
        keys(&wire["limits"]),
        ["amount", "leverage", "price"],
        "OPTION wire limits key order"
    );
    assert_eq!(
        serde_json::from_value::<Market>(wire).unwrap(),
        native,
        "OPTION wire decodes back to the same native market"
    );

    let plain: Market = serde_json::from_value(market()).unwrap();
    assert!(
        plain.option.is_none() && plain.limits.price.is_none(),
        "absent on every other market"
    );
    let wire = serde_json::to_value(plain).unwrap();
    assert!(
        wire.get("option").is_none() && wire["limits"].get("price").is_none(),
        "absent fields stay absent on the wire"
    );

    let mut put = market();
    put["option"] = input["option"].clone();
    put["option"]["right"] = json!("put");
    let put: Market = serde_json::from_value(put).unwrap();
    assert_eq!(
        put.option.unwrap().right,
        OptionRight::Put,
        "OPTION put right"
    );

    for (path, value) in [
        ("right", json!("straddle")),
        ("style", json!("american")),
        ("settlement", json!("physical")),
    ] {
        let mut bad = input.clone();
        bad["option"][path] = value;
        assert!(
            serde_json::from_value::<Market>(bad).is_err(),
            "OPTION refuses unknown {path}"
        );
    }
    let mut null = input.clone();
    null["option"] = Value::Null;
    assert!(
        serde_json::from_value::<Market>(null).is_err(),
        "OPTION present null refusal"
    );
}

#[test]
fn position_greeks_order_iv_and_market_toggles_decode_natively() {
    let position: Position = serde_json::from_value(json!({"id":"p", "symbol":"S", "side":"long",
        "entryPrice":1, "notional":2, "leverage":1, "unrealizedPnl":0, "contracts":1,
        "liquidationPrice":0,
        "option":{"greeks":{"delta":0.5,"gamma":0.01,"vega":3,"theta":-4,"units":"per-1-vol"}}}))
    .unwrap();
    let greeks = position
        .option
        .as_ref()
        .and_then(|o| o.greeks.as_ref())
        .unwrap();
    assert_eq!(
        [greeks.delta, greeks.gamma, greeks.vega, greeks.theta].map(f64::to_bits),
        [0.5_f64, 0.01, 3.0, -4.0].map(f64::to_bits),
        "GREEKS native numbers"
    );
    assert_eq!(greeks.units, "per-1-vol", "GREEKS native units");

    let order: PlaceOrderOpts = serde_json::from_value(json!({"symbol":"S", "type":"limit",
        "side":"buy", "amount":1, "impliedVolatility":0.45}))
    .unwrap();
    assert_eq!(
        order.implied_volatility.map(f64::to_bits),
        Some(0.45_f64.to_bits()),
        "ORDER native implied volatility"
    );

    let options: ExchangeOptions = serde_json::from_value(json!({"key":"k", "secret":"s",
        "enableOptionMarkets":true, "enableInverseMarkets":false}))
    .unwrap();
    assert_eq!(
        (
            options.enable_option_markets,
            options.enable_inverse_markets
        ),
        (Some(true), Some(false)),
        "OPTIONS native market toggles"
    );
}
