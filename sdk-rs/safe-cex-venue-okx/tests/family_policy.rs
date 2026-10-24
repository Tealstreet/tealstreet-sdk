use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{OrderSide, OrderType, PositionMode};
use safe_cex_venue_okx::{
    family_policy::*,
    types::{OkxInstrumentType, OkxRegion},
};
use serde_json::{Value, json};

#[test]
fn eight_live_policy_rows_match_native_values_before_wire() {
    let endpoint = get_market_instruments_endpoint();
    let eu_private = get_eu_market_instruments_endpoint(false);
    let eu_public = get_eu_market_instruments_endpoint(true);
    let instruments = get_eu_instrument_types();
    let resync = business_resyncs_private_state_on_reopen();
    assert_eq!(
        endpoint, "/api/v5/public/instruments",
        "OKX-POLICY native standard endpoint"
    );
    assert_eq!(
        eu_private, "/api/v5/account/instruments",
        "OKX-POLICY native EU account endpoint"
    );
    assert_eq!(
        eu_public, "/api/v5/public/instruments",
        "OKX-POLICY native EU public endpoint"
    );
    assert_eq!(
        instruments,
        [OkxInstrumentType::Swap, OkxInstrumentType::Futures],
        "OKX-POLICY native EU instrument identity"
    );
    assert!(resync, "OKX-POLICY native business resync");
    assert_eq!(
        ORDER_STATS_TOPIC, "orders",
        "OKX-POLICY native order stats topic"
    );

    // Literal expectations are independent of the implementation and oracle artefact.
    let buy = [
        ["long", "long", "short"],
        ["long", "long", "short"],
        ["long", "long", "long"],
        ["short", "short", "short"],
        ["short", "short", "short"],
        ["short", "short", "short"],
    ];
    let sell = [
        ["short", "short", "long"],
        ["short", "short", "long"],
        ["short", "short", "short"],
        ["long", "long", "long"],
        ["long", "long", "long"],
        ["long", "long", "long"],
    ];
    let types = [
        OrderType::Market,
        OrderType::Limit,
        OrderType::Conditional,
        OrderType::StopLoss,
        OrderType::TakeProfit,
        OrderType::TrailingStopLoss,
    ];
    let mut native = vec![];
    for mode in [None, Some(PositionMode::OneWay), Some(PositionMode::Hedged)] {
        for (type_index, order_type) in types.into_iter().enumerate() {
            for side in [OrderSide::Buy, OrderSide::Sell] {
                for (reduce_index, reduce_only) in
                    [None, Some(false), Some(true)].into_iter().enumerate()
                {
                    let result = get_position_side(mode, order_type, side, reduce_only);
                    let expected = match mode {
                        None => Err(ExchangeError::new("Account position mode not loaded")),
                        Some(PositionMode::OneWay) => Ok(None),
                        Some(PositionMode::Hedged) => Ok(Some(if side == OrderSide::Buy {
                            buy[type_index][reduce_index]
                        } else {
                            sell[type_index][reduce_index]
                        })),
                    };
                    assert_eq!(
                        result, expected,
                        "OKX-POLICY native position side {:?}/{order_type:?}/{side:?}/{reduce_only:?}",
                        mode
                    );
                    native.push((mode, order_type, side, reduce_only, result));
                }
            }
        }
    }
    assert_eq!(native.len(), 108, "independent native closed-enum corpus");

    let private_urls = [
        [
            "wss://ws.okx.com/ws/v5/private",
            "wss://wspap.okx.com/ws/v5/private",
        ],
        [
            "wss://wseea.okx.com/ws/v5/private",
            "wss://wseeapap.okx.com/ws/v5/private",
        ],
    ];
    let business_urls = [
        [
            "wss://ws.okx.com/ws/v5/business",
            "wss://wspap.okx.com/ws/v5/business",
        ],
        [
            "wss://wseea.okx.com/ws/v5/business",
            "wss://wseeapap.okx.com/ws/v5/business",
        ],
    ];
    let mut sockets = vec![];
    for (region_index, region) in [OkxRegion::Okx, OkxRegion::Okxeu].into_iter().enumerate() {
        for testnet in [None, Some(false), Some(true)] {
            let net = usize::from(testnet == Some(true));
            let private = get_private_websocket_endpoint(region, testnet);
            let business = get_business_websocket_endpoint(region, testnet);
            assert_eq!(
                private, private_urls[region_index][net],
                "OKX-POLICY native private endpoint {region:?}/{testnet:?}"
            );
            assert_eq!(
                business, business_urls[region_index][net],
                "OKX-POLICY native business endpoint {region:?}/{testnet:?}"
            );
            sockets.push((region, testnet, private, business));
        }
    }
    assert_eq!(sockets.len(), 6, "independent six socket endpoint profiles");

    // All native assertions above run before the oracle and any wire comparison.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/family-policy-oracle.mjs"
        ))
        .output()
        .expect("Node required for maintained OKX source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected["sockets"].as_array().unwrap().len(),
        6,
        "independent source socket profiles"
    );
    for (index, (region, testnet, private, business)) in sockets.into_iter().enumerate() {
        let mut actual = json!({"region":region,"private":private,"business":business});
        if let Some(testnet) = testnet {
            actual["testnet"] = json!(testnet);
        }
        assert_eq!(
            actual, expected["sockets"][index],
            "live socket endpoint profile {index}"
        );
    }
    assert_eq!(expected["instruments"], json!(instruments));
    assert_eq!(expected["resync"], resync);
    assert_eq!(expected["stats"], ORDER_STATS_TOPIC);
    assert_eq!(
        expected["endpoints"].as_array().unwrap().len(),
        2,
        "independent endpoint corpus"
    );
    assert_eq!(
        expected["endpoints"],
        json!([
            {"publicDataOnly":false,"standard":endpoint,"eu":eu_private},
            {"publicDataOnly":true,"standard":endpoint,"eu":eu_public},
        ])
    );
    assert_eq!(
        expected["sides"].as_array().unwrap().len(),
        108,
        "independent source closed-enum corpus"
    );
    for (index, (mode, order_type, side, reduce_only, result)) in native.into_iter().enumerate() {
        let mut input = json!({"mode":mode,"type":order_type,"side":side});
        if let Some(reduce_only) = reduce_only {
            input["reduceOnly"] = json!(reduce_only);
        }
        assert_eq!(
            input, expected["sides"][index]["input"],
            "live policy input identity {index}"
        );
        match result {
            Ok(value) => assert_eq!(
                json!(value),
                expected["sides"][index]["ok"],
                "live policy output {index}"
            ),
            Err(error) => assert_eq!(
                json!({"message":error.message,"name":error.name}),
                expected["sides"][index]["error"],
                "live policy rejection {index}"
            ),
        }
    }
}
