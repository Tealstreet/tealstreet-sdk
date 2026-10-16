use safe_cex_venue_lighter::signer_types::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::process::Command;
mod support;
fn number(v: &Value) -> f64 {
    match v.get("$number").and_then(Value::as_str) {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        Some(s) => panic!("unknown number {s}"),
        None => v.as_f64().unwrap(),
    }
}
fn fields(expected: &Value, values: &[(&str, &str, bool)], name: &str) {
    let actual: Vec<_> = values
        .iter()
        .map(|(name, typ, optional)| json!({"name":name,"type":typ,"optional":optional}))
        .collect();
    assert_eq!(json!(actual), *expected, "{name} field schema");
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn lighter_signer_request_types_match_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/signer-types-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["fields"].as_array().unwrap().len(),
        32,
        "TYPE-CARDINALITY fields source cases"
    );
    for name in ["groupingType", "marginMode"] {
        assert_eq!(
            oracle["accepted"][name].as_array().unwrap().len(),
            21,
            "TYPE-CARDINALITY accepted {name} source cases"
        );
    }

    let unions = &oracle["unions"];
    macro_rules! strings {($typ:ty,$key:literal)=>{{
        support::assert_string_only::<$typ>(&unions[$key], stringify!($typ));
        assert_eq!(json!(support::enum_inventory::<$typ>()),unions[$key],"{} accepted inventory",stringify!($typ));
        for value in unions[$key].as_array().unwrap(){
            let parsed=<$typ>::deserialize(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(),*value,"{} accepted literal",stringify!($typ));
            for wrong in [json!({value.as_str().unwrap():null}),json!({value.as_str().unwrap():{}}),json!({value.as_str().unwrap():[]})]{
                assert!(<$typ>::deserialize(wrong).is_err(),"{} concrete decoder rejects object",stringify!($typ));
            }
        }
    }}}
    strings!(LighterSignerProvider, "provider");
    strings!(LighterSignerOrderType, "orderType");
    strings!(LighterSignerTimeInForce, "timeInForce");
    let provider = match LighterSignerProvider::TealstreetSdk {
        LighterSignerProvider::TealstreetSdk => "tealstreet-sdk",
    };
    assert_eq!(json!([provider]), unions["provider"]);
    let kinds = [
        LighterSignerOrderType::Limit,
        LighterSignerOrderType::Market,
        LighterSignerOrderType::StopLoss,
        LighterSignerOrderType::TakeProfit,
    ]
    .map(|kind| match kind {
        LighterSignerOrderType::Limit => "limit",
        LighterSignerOrderType::Market => "market",
        LighterSignerOrderType::StopLoss => "stop_loss",
        LighterSignerOrderType::TakeProfit => "take_profit",
    });
    assert_eq!(json!(kinds), unions["orderType"]);
    let tifs = [
        LighterSignerTimeInForce::GoodTillCancel,
        LighterSignerTimeInForce::ImmediateOrCancel,
        LighterSignerTimeInForce::FillOrKill,
        LighterSignerTimeInForce::PostOnly,
    ]
    .map(|tif| match tif {
        LighterSignerTimeInForce::GoodTillCancel => "GoodTillCancel",
        LighterSignerTimeInForce::ImmediateOrCancel => "ImmediateOrCancel",
        LighterSignerTimeInForce::FillOrKill => "FillOrKill",
        LighterSignerTimeInForce::PostOnly => "PostOnly",
    });
    assert_eq!(json!(tifs), unions["timeInForce"]);
    let groups = [
        LighterSignerGroupingType::OneTriggersOther,
        LighterSignerGroupingType::OneCancelsOther,
        LighterSignerGroupingType::OneTriggersOneCancelsOther,
    ];
    assert_eq!(
        serde_json::to_value(groups).unwrap(),
        unions["groupingType"]
    );
    for group in groups {
        let expected = match group {
            LighterSignerGroupingType::OneTriggersOther => 1,
            LighterSignerGroupingType::OneCancelsOther => 2,
            LighterSignerGroupingType::OneTriggersOneCancelsOther => 3,
        };
        assert_eq!(serde_json::to_value(group).unwrap(), json!(expected));
    }
    for case in oracle["accepted"]["groupingType"].as_array().unwrap() {
        let value = &case["value"];
        let accepted = if value.is_number() || value.get("$number").is_some() {
            let n = number(value);
            let parsed =
                LighterSignerGroupingType::deserialize(serde::de::value::F64Deserializer::<
                    serde::de::value::Error,
                >::new(n));
            if let Ok(parsed) = parsed {
                assert_eq!(
                    serde_json::to_value(parsed).unwrap().as_f64().unwrap(),
                    n,
                    "groupingType decoded numeric literal {case}"
                );
                let concrete = LighterSignerGroupingType::deserialize(json!(n)).unwrap();
                assert_eq!(
                    serde_json::to_value(concrete).unwrap().as_f64().unwrap(),
                    n,
                    "groupingType concrete decoded numeric literal {case}"
                );
            }
            parsed.is_ok()
        } else {
            LighterSignerGroupingType::deserialize(value.clone()).is_ok()
        };
        assert_eq!(
            accepted,
            case["expected"].as_bool().unwrap(),
            "groupingType accepted set {case}"
        );
    }
    for case in oracle["accepted"]["marginMode"].as_array().unwrap() {
        let value = &case["value"];
        let accepted = if value.is_number() || value.get("$number").is_some() {
            let n = number(value);
            let parsed = LighterSignerMarginMode::deserialize(serde::de::value::F64Deserializer::<
                serde::de::value::Error,
            >::new(n));
            assert_eq!(
                LighterSignerMarginMode::try_from(n).is_ok(),
                case["expected"].as_bool().unwrap(),
                "marginMode public TryFrom {case}"
            );
            if let Ok(parsed) = parsed {
                let concrete = LighterSignerMarginMode::deserialize(json!(n)).unwrap();
                assert_eq!(
                    f64::from(concrete).to_bits(),
                    n.to_bits(),
                    "marginMode concrete numeric roundtrip"
                );
                assert_eq!(
                    f64::from(parsed).to_bits(),
                    n.to_bits(),
                    "marginMode preserves signed zero"
                );
                assert_eq!(
                    serde_json::to_value(parsed)
                        .unwrap()
                        .as_f64()
                        .unwrap()
                        .to_bits(),
                    n.to_bits(),
                    "marginMode serialization preserves signed zero"
                );
            }
            parsed.is_ok()
        } else {
            LighterSignerMarginMode::deserialize(value.clone()).is_ok()
        };
        assert_eq!(
            accepted,
            case["expected"].as_bool().unwrap(),
            "marginMode accepted set {case}"
        );
    }
    fields(
        &oracle["schemas"]["LighterCreateOrderRequest"],
        &[
            ("orderId", "bigint", false),
            ("marketId", "number", false),
            ("isBuy", "boolean", false),
            ("price", "bigint", false),
            ("amount", "bigint", false),
            ("postOnly", "boolean", false),
            ("reduceOnly", "boolean", false),
            (
                "orderType",
                "'limit' | 'market' | 'stop_loss' | 'take_profit'",
                false,
            ),
            ("triggerPrice", "bigint", true),
            (
                "timeInForce",
                "'GoodTillCancel' | 'ImmediateOrCancel' | 'FillOrKill' | 'PostOnly'",
                true,
            ),
            ("integratorAccountIndex", "bigint", true),
            ("integratorMakerFee", "number", true),
            ("integratorTakerFee", "number", true),
        ],
        "LighterCreateOrderRequest",
    );
    fields(
        &oracle["schemas"]["LighterCreateGroupedOrdersRequest"],
        &[
            ("groupingType", "1 | 2 | 3", false),
            ("orders", "LighterCreateOrderRequest[]", false),
            ("integratorAccountIndex", "bigint", true),
            ("integratorMakerFee", "number", true),
            ("integratorTakerFee", "number", true),
        ],
        "LighterCreateGroupedOrdersRequest",
    );
    fields(
        &oracle["schemas"]["LighterCancelOrderRequest"],
        &[("orderId", "bigint", false), ("marketId", "number", false)],
        "LighterCancelOrderRequest",
    );
    fields(
        &oracle["schemas"]["LighterUpdateLeverageRequest"],
        &[
            ("marketId", "number", false),
            ("leverage", "number", false),
            ("marginMode", "0 | 1", false),
        ],
        "LighterUpdateLeverageRequest",
    );
    fields(
        &oracle["schemas"]["LighterApproveIntegratorRequest"],
        &[
            ("integratorAccountIndex", "bigint", false),
            ("maxPerpsTakerFee", "number", false),
            ("maxPerpsMakerFee", "number", false),
            ("maxSpotTakerFee", "number", false),
            ("maxSpotMakerFee", "number", false),
            ("approvalExpiry", "bigint", false),
            ("l1Signature", "string", true),
        ],
        "LighterApproveIntegratorRequest",
    );
    for case in oracle["fields"].as_array().unwrap() {
        let n = number(&case["number"]);
        let big = num_bigint::BigInt::parse_bytes(case["bigint"].as_str().unwrap().as_bytes(), 10)
            .unwrap();
        let present = case["present"].as_bool().unwrap();
        let flag = case["seed"].as_u64().unwrap() % 2 == 0;
        let order_type = LighterSignerOrderType::StopLoss;
        let tif = LighterSignerTimeInForce::PostOnly;
        let grouping = LighterSignerGroupingType::OneCancelsOther;
        let margin = LighterSignerMarginMode::try_from(-0.0).unwrap();
        let value = LighterCreateOrderRequest {
            order_id: big.clone(),
            market_id: n,
            is_buy: flag,
            price: big.clone(),
            amount: big.clone(),
            post_only: flag,
            reduce_only: flag,
            order_type,
            trigger_price: present.then(|| big.clone()),
            time_in_force: present.then_some(tif),
            integrator_account_index: present.then(|| big.clone()),
            integrator_maker_fee: present.then_some(n),
            integrator_taker_fee: present.then_some(n),
        };
        let order = value.clone();
        let LighterCreateOrderRequest {
            order_id,
            market_id,
            is_buy,
            price,
            amount,
            post_only,
            reduce_only,
            order_type,
            trigger_price,
            time_in_force,
            integrator_account_index,
            integrator_maker_fee,
            integrator_taker_fee,
        } = value;
        assert_eq!(
            order_id.to_string(),
            big.to_string(),
            "LighterCreateOrderRequest.orderId bigint"
        );
        assert_eq!(
            market_id.to_bits(),
            n.to_bits(),
            "LighterCreateOrderRequest.marketId binary64"
        );
        assert_eq!(is_buy, flag);
        assert_eq!(
            price.to_string(),
            big.to_string(),
            "LighterCreateOrderRequest.price bigint"
        );
        assert_eq!(
            amount.to_string(),
            big.to_string(),
            "LighterCreateOrderRequest.amount bigint"
        );
        assert_eq!(post_only, flag);
        assert_eq!(reduce_only, flag);
        assert_eq!(order_type, LighterSignerOrderType::StopLoss);
        assert_eq!(
            trigger_price.map(|v| v.to_string()),
            present.then(|| big.to_string()),
            "LighterCreateOrderRequest.triggerPrice bigint"
        );
        assert_eq!(time_in_force, present.then_some(tif));
        assert_eq!(
            integrator_account_index.map(|v| v.to_string()),
            present.then(|| big.to_string()),
            "LighterCreateOrderRequest.integratorAccountIndex bigint"
        );
        assert_eq!(
            integrator_maker_fee.map(f64::to_bits),
            present.then(|| n.to_bits()),
            "LighterCreateOrderRequest.integratorMakerFee binary64"
        );
        assert_eq!(
            integrator_taker_fee.map(f64::to_bits),
            present.then(|| n.to_bits()),
            "LighterCreateOrderRequest.integratorTakerFee binary64"
        );
        let value = LighterCreateGroupedOrdersRequest {
            grouping_type: grouping,
            orders: vec![order.clone()],
            integrator_account_index: present.then(|| big.clone()),
            integrator_maker_fee: present.then_some(n),
            integrator_taker_fee: present.then_some(n),
        };
        let LighterCreateGroupedOrdersRequest {
            grouping_type,
            orders,
            integrator_account_index,
            integrator_maker_fee,
            integrator_taker_fee,
        } = value;
        assert_eq!(grouping_type, grouping);
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].order_id, big);
        assert_eq!(
            integrator_account_index.map(|v| v.to_string()),
            present.then(|| big.to_string()),
            "LighterCreateGroupedOrdersRequest.integratorAccountIndex bigint"
        );
        assert_eq!(
            integrator_maker_fee.map(f64::to_bits),
            present.then(|| n.to_bits()),
            "LighterCreateGroupedOrdersRequest.integratorMakerFee binary64"
        );
        assert_eq!(
            integrator_taker_fee.map(f64::to_bits),
            present.then(|| n.to_bits()),
            "LighterCreateGroupedOrdersRequest.integratorTakerFee binary64"
        );
        let value = LighterCancelOrderRequest {
            order_id: big.clone(),
            market_id: n,
        };
        let LighterCancelOrderRequest {
            order_id,
            market_id,
        } = value;
        assert_eq!(
            order_id.to_string(),
            big.to_string(),
            "LighterCancelOrderRequest.orderId bigint"
        );
        assert_eq!(
            market_id.to_bits(),
            n.to_bits(),
            "LighterCancelOrderRequest.marketId binary64"
        );
        let value = LighterUpdateLeverageRequest {
            market_id: n,
            leverage: n,
            margin_mode: margin,
        };
        let LighterUpdateLeverageRequest {
            market_id,
            leverage,
            margin_mode,
        } = value;
        assert_eq!(
            market_id.to_bits(),
            n.to_bits(),
            "LighterUpdateLeverageRequest.marketId binary64"
        );
        assert_eq!(
            leverage.to_bits(),
            n.to_bits(),
            "LighterUpdateLeverageRequest.leverage binary64"
        );
        assert_eq!(f64::from(margin_mode).to_bits(), (-0.0f64).to_bits());
        let value = LighterApproveIntegratorRequest {
            integrator_account_index: big.clone(),
            max_perps_taker_fee: n,
            max_perps_maker_fee: n,
            max_spot_taker_fee: n,
            max_spot_maker_fee: n,
            approval_expiry: big.clone(),
            l1_signature: present.then(|| "\u{feff}0xsignature".to_owned()),
        };
        let LighterApproveIntegratorRequest {
            integrator_account_index,
            max_perps_taker_fee,
            max_perps_maker_fee,
            max_spot_taker_fee,
            max_spot_maker_fee,
            approval_expiry,
            l1_signature,
        } = value;
        assert_eq!(
            integrator_account_index.to_string(),
            big.to_string(),
            "LighterApproveIntegratorRequest.integratorAccountIndex bigint"
        );
        assert_eq!(
            max_perps_taker_fee.to_bits(),
            n.to_bits(),
            "LighterApproveIntegratorRequest.maxPerpsTakerFee binary64"
        );
        assert_eq!(
            max_perps_maker_fee.to_bits(),
            n.to_bits(),
            "LighterApproveIntegratorRequest.maxPerpsMakerFee binary64"
        );
        assert_eq!(
            max_spot_taker_fee.to_bits(),
            n.to_bits(),
            "LighterApproveIntegratorRequest.maxSpotTakerFee binary64"
        );
        assert_eq!(
            max_spot_maker_fee.to_bits(),
            n.to_bits(),
            "LighterApproveIntegratorRequest.maxSpotMakerFee binary64"
        );
        assert_eq!(
            approval_expiry.to_string(),
            big.to_string(),
            "LighterApproveIntegratorRequest.approvalExpiry bigint"
        );
        assert_eq!(
            l1_signature.as_deref(),
            present.then_some("\u{feff}0xsignature")
        );
    }
}

#[test]
fn native_record_clone_retains_field_identity() {
    macro_rules! identity {
        ($typ:ident, {$($field:ident: $value:expr),+}) => {{
            let source = $typ { $($field: $value),+ };
            let actual = source.clone();
            $(assert_eq!(actual.$field, source.$field, concat!(stringify!($typ), ".", stringify!($field), " native clone identity"));)+
            actual
        }};
    }
    for flags in 0..8 {
        let order = identity!(LighterCreateOrderRequest, {order_id: num_bigint::BigInt::from(1001u64), market_id: 102.25, is_buy: flags & 1 != 0, price: num_bigint::BigInt::from(1004u64), amount: num_bigint::BigInt::from(1005u64), post_only: flags & 2 != 0, reduce_only: flags & 4 != 0, order_type: LighterSignerOrderType::StopLoss, trigger_price: Some(num_bigint::BigInt::from(1009u64)), time_in_force: Some(LighterSignerTimeInForce::PostOnly), integrator_account_index: Some(num_bigint::BigInt::from(1011u64)), integrator_maker_fee: Some(112.25), integrator_taker_fee: Some(113.25)});
        identity!(LighterCreateGroupedOrdersRequest, {grouping_type: LighterSignerGroupingType::OneCancelsOther, orders: vec![order.clone()], integrator_account_index: Some(num_bigint::BigInt::from(1003u64)), integrator_maker_fee: Some(104.25), integrator_taker_fee: Some(105.25)});
        identity!(LighterCancelOrderRequest, {order_id: num_bigint::BigInt::from(1001u64), market_id: 102.25});
        identity!(LighterUpdateLeverageRequest, {market_id: 101.25, leverage: 102.25, margin_mode: LighterSignerMarginMode::try_from(-0.0).unwrap()});
        identity!(LighterApproveIntegratorRequest, {integrator_account_index: num_bigint::BigInt::from(1001u64), max_perps_taker_fee: 102.25, max_perps_maker_fee: 103.25, max_spot_taker_fee: 104.25, max_spot_maker_fee: 105.25, approval_expiry: num_bigint::BigInt::from(1006u64), l1_signature: Some("identity:LighterApproveIntegratorRequest.l1_signature".to_owned())});
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn source_cardinality_controls_fail_by_name() {
    for (field, assertion) in [
        ("fields", "TYPE-CARDINALITY fields source cases"),
        (
            "groupingType",
            "TYPE-CARDINALITY accepted groupingType source cases",
        ),
        (
            "marginMode",
            "TYPE-CARDINALITY accepted marginMode source cases",
        ),
    ] {
        for mode in ["empty", "subset"] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "lighter_signer_request_types_match_live_typescript",
                    "--ignored",
                    "--nocapture",
                ])
                .env(
                    "LIGHTER_TYPE_CARDINALITY_MUTATION",
                    format!("{field}:{mode}"),
                )
                .output()
                .unwrap();
            let log = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.status.code(), Some(101), "{log}");
            assert!(log.contains(assertion), "{log}");
        }
    }
}
