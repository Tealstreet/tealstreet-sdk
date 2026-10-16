use safe_cex_venue_lighter::sdk_protocol_types::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::process::Command;
mod support;
fn number(value: f64) -> Value {
    json!({"$number":format!("{:016x}",value.to_bits())})
}
fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/sdk-protocol-types-oracle.cjs"
    )
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_protocol_types_match_live_typescript() {
    let output = Command::new("node").arg(script()).output().unwrap();
    assert!(
        output.status.success(),
        "SDK protocol type schema: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let oracle = &data["types"];
    assert_eq!(
        data["overflow"].as_array().unwrap().len(),
        5,
        "numeric overflow witness inventory"
    );
    assert_eq!(
        oracle
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "TxTypeValue",
            "OrderType",
            "TimeInForce",
            "OrderStatus",
            "TriggerStatus",
            "MarginMode",
            "GroupingType",
            "OrderSide",
            "WsMessageType",
            "WsChannel",
            "ResultCode"
        ],
        "SDK protocol type inventory"
    );
    macro_rules! numeric { ($typ:ty) => {{
        assert_eq!(oracle[stringify!($typ)]["numberAssignable"], json!(true), "{} number-typed input domain", stringify!($typ));
        for value in [-0.0,0.0,1.0,1.5,-1.0,12.0,999.0,1e100,f64::MIN_POSITIVE,f64::from_bits(1),f64::MAX,f64::NAN,f64::INFINITY,f64::NEG_INFINITY] {
            let parsed = <$typ>::deserialize(serde::de::value::F64Deserializer::<serde::de::value::Error>::new(value)).unwrap();
            assert_eq!(parsed.0.to_bits(),value.to_bits(),"{} number preservation",stringify!($typ));
            let serialized = serde_json::to_value(parsed).unwrap();
            if value.is_finite() {
                assert_eq!(serialized.as_f64().unwrap().to_bits(),value.to_bits(),"{} numeric serialization",stringify!($typ));
                let parsed = serde_json::from_value::<$typ>(json!(value)).unwrap();
                assert_eq!(parsed.0.to_bits(),value.to_bits(),"{} Value number preservation",stringify!($typ));
            } else { assert!(serialized.is_null(),"{} nonfinite JSON",stringify!($typ)); }
        }
        for witness in data["overflow"].as_array().unwrap() {
            let raw = witness["raw"].as_str().unwrap();
            // Diagnostic: direct typed parsing bypasses canonical Value-first ingress.
            // Flip this control when the separately routed shared serde repair lands.
            assert!(!raw.contains("e400") || serde_json::from_str::<$typ>(raw).is_err(),"{} raw parser bypass behavior changed",stringify!($typ));
            let value: Value = serde_json::from_str(raw).unwrap();
            let parsed = serde_json::from_value::<$typ>(value);
            assert!(parsed.is_ok(),"{} canonical Value-first overflow ingress",stringify!($typ));
            let parsed = parsed.unwrap();
            assert_eq!(format!("{:016x}",parsed.0.to_bits()),witness["bits"].as_str().unwrap(),"{} Value-first numeric bits match Node: {raw}",stringify!($typ));
        }
        for bad in [json!(null),json!(false),json!(true),json!("0"),json!([]),json!([0]),json!({"0":null}),json!({})] {
            assert!(<$typ>::deserialize(bad.clone()).is_err(),"{} rejects {bad}",stringify!($typ));
        }
    }}; }
    macro_rules! strings { ($typ:ty) => {{
        assert_eq!(oracle[stringify!($typ)]["numberAssignable"], json!(false));
        let expected = json!(oracle[stringify!($typ)]["members"].as_array().unwrap().iter().map(|entry| entry[1].clone()).collect::<Vec<_>>());
        assert_eq!(json!(support::enum_inventory::<$typ>()), expected, "{} accepted metadata", stringify!($typ));
        support::assert_string_only::<$typ>(&expected, stringify!($typ));
        for value in expected.as_array().unwrap() {
            let parsed = <$typ>::deserialize(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), *value, "{} literal roundtrip", stringify!($typ));
            for bad in [json!({value.as_str().unwrap():null}), json!({value.as_str().unwrap():[]}), json!({value.as_str().unwrap():{}})] {
                assert!(<$typ>::deserialize(bad).is_err(), "{} public qualified decoder", stringify!($typ));
            }
        }
    }}; }
    let variants = [
        TxTypeValue::L2CreateOrder,
        TxTypeValue::L2CancelOrder,
        TxTypeValue::L2Transfer,
        TxTypeValue::L2Withdraw,
        TxTypeValue::L2UpdateApiKey,
        TxTypeValue::L2Liquidation,
        TxTypeValue::L2Deleverage,
        TxTypeValue::L2FundingFee,
        TxTypeValue::L2CreateGroupedOrders,
    ];
    let exhaustive = variants.map(|value| match value {
        TxTypeValue::L2CreateOrder => 14,
        TxTypeValue::L2CancelOrder => 15,
        TxTypeValue::L2Transfer => 16,
        TxTypeValue::L2Withdraw => 17,
        TxTypeValue::L2UpdateApiKey => 18,
        TxTypeValue::L2Liquidation => 19,
        TxTypeValue::L2Deleverage => 20,
        TxTypeValue::L2FundingFee => 21,
        TxTypeValue::L2CreateGroupedOrders => 28,
    });
    assert_eq!(
        json!(exhaustive),
        oracle["TxTypeValue"]["literals"],
        "TxTypeValue exhaustive variants"
    );
    assert_eq!(
        json!(variants),
        oracle["TxTypeValue"]["literals"],
        "TxTypeValue serialization"
    );
    numeric!(OrderType);
    let members = [
        json!(["Limit", number(OrderType::LIMIT.0)]),
        json!(["Market", number(OrderType::MARKET.0)]),
        json!(["StopLoss", number(OrderType::STOP_LOSS.0)]),
        json!(["TakeProfit", number(OrderType::TAKE_PROFIT.0)]),
        json!(["TWAP", number(OrderType::TWAP.0)]),
    ];
    assert_eq!(
        json!(members),
        oracle["OrderType"]["members"],
        "OrderType members"
    );
    numeric!(TimeInForce);
    let members = [
        json!([
            "ImmediateOrCancel",
            number(TimeInForce::IMMEDIATE_OR_CANCEL.0)
        ]),
        json!(["GoodTillTime", number(TimeInForce::GOOD_TILL_TIME.0)]),
        json!(["PostOnly", number(TimeInForce::POST_ONLY.0)]),
    ];
    assert_eq!(
        json!(members),
        oracle["TimeInForce"]["members"],
        "TimeInForce members"
    );
    numeric!(OrderStatus);
    let members = [
        json!(["Pending", number(OrderStatus::PENDING.0)]),
        json!(["Open", number(OrderStatus::OPEN.0)]),
        json!(["PartiallyFilled", number(OrderStatus::PARTIALLY_FILLED.0)]),
        json!(["Filled", number(OrderStatus::FILLED.0)]),
        json!(["Cancelled", number(OrderStatus::CANCELLED.0)]),
        json!(["Rejected", number(OrderStatus::REJECTED.0)]),
        json!(["Expired", number(OrderStatus::EXPIRED.0)]),
    ];
    assert_eq!(
        json!(members),
        oracle["OrderStatus"]["members"],
        "OrderStatus members"
    );
    numeric!(TriggerStatus);
    let members = [
        json!(["Inactive", number(TriggerStatus::INACTIVE.0)]),
        json!(["Active", number(TriggerStatus::ACTIVE.0)]),
        json!(["Triggered", number(TriggerStatus::TRIGGERED.0)]),
        json!(["Cancelled", number(TriggerStatus::CANCELLED.0)]),
    ];
    assert_eq!(
        json!(members),
        oracle["TriggerStatus"]["members"],
        "TriggerStatus members"
    );
    numeric!(MarginMode);
    let members = [
        json!(["Cross", number(MarginMode::CROSS.0)]),
        json!(["Isolated", number(MarginMode::ISOLATED.0)]),
    ];
    assert_eq!(
        json!(members),
        oracle["MarginMode"]["members"],
        "MarginMode members"
    );
    numeric!(GroupingType);
    let members = [
        json!([
            "OneTriggersTheOther",
            number(GroupingType::ONE_TRIGGERS_THE_OTHER.0)
        ]),
        json!([
            "OneCancelsTheOther",
            number(GroupingType::ONE_CANCELS_THE_OTHER.0)
        ]),
        json!([
            "OneTriggersAOneCancelsTheOther",
            number(GroupingType::ONE_TRIGGERS_A_ONE_CANCELS_THE_OTHER.0)
        ]),
    ];
    assert_eq!(
        json!(members),
        oracle["GroupingType"]["members"],
        "GroupingType members"
    );
    numeric!(OrderSide);
    let members = [
        json!(["Buy", number(OrderSide::BUY.0)]),
        json!(["Sell", number(OrderSide::SELL.0)]),
    ];
    assert_eq!(
        json!(members),
        oracle["OrderSide"]["members"],
        "OrderSide members"
    );
    strings!(WsMessageType);
    let variants = [
        WsMessageType::Subscribe,
        WsMessageType::Unsubscribe,
        WsMessageType::Ping,
        WsMessageType::Pong,
        WsMessageType::Update,
        WsMessageType::Error,
    ];
    let members = variants.map(|value| match value {
        WsMessageType::Subscribe => json!(["Subscribe", value]),
        WsMessageType::Unsubscribe => json!(["Unsubscribe", value]),
        WsMessageType::Ping => json!(["Ping", value]),
        WsMessageType::Pong => json!(["Pong", value]),
        WsMessageType::Update => json!(["Update", value]),
        WsMessageType::Error => json!(["Error", value]),
    });
    assert_eq!(
        json!(members),
        oracle["WsMessageType"]["members"],
        "WsMessageType exhaustive members"
    );
    strings!(WsChannel);
    let variants = [
        WsChannel::OrderBook,
        WsChannel::Trades,
        WsChannel::Ticker,
        WsChannel::Orders,
        WsChannel::Positions,
        WsChannel::Balances,
    ];
    let members = variants.map(|value| match value {
        WsChannel::OrderBook => json!(["OrderBook", value]),
        WsChannel::Trades => json!(["Trades", value]),
        WsChannel::Ticker => json!(["Ticker", value]),
        WsChannel::Orders => json!(["Orders", value]),
        WsChannel::Positions => json!(["Positions", value]),
        WsChannel::Balances => json!(["Balances", value]),
    });
    assert_eq!(
        json!(members),
        oracle["WsChannel"]["members"],
        "WsChannel exhaustive members"
    );
    numeric!(ResultCode);
    let members = [
        json!(["Success", number(ResultCode::SUCCESS.0)]),
        json!(["InvalidParams", number(ResultCode::INVALID_PARAMS.0)]),
        json!(["AuthFailed", number(ResultCode::AUTH_FAILED.0)]),
        json!([
            "InsufficientBalance",
            number(ResultCode::INSUFFICIENT_BALANCE.0)
        ]),
        json!(["OrderNotFound", number(ResultCode::ORDER_NOT_FOUND.0)]),
        json!(["MarketNotFound", number(ResultCode::MARKET_NOT_FOUND.0)]),
        json!([
            "RateLimitExceeded",
            number(ResultCode::RATE_LIMIT_EXCEEDED.0)
        ]),
        json!(["InternalError", number(ResultCode::INTERNAL_ERROR.0)]),
        json!(["NonceTooLow", number(ResultCode::NONCE_TOO_LOW.0)]),
        json!(["NonceTooHigh", number(ResultCode::NONCE_TOO_HIGH.0)]),
        json!(["InvalidSignature", number(ResultCode::INVALID_SIGNATURE.0)]),
        json!(["TxExpired", number(ResultCode::TX_EXPIRED.0)]),
    ];
    assert_eq!(
        json!(members),
        oracle["ResultCode"]["members"],
        "ResultCode members"
    );
    let actual_constants: Vec<_> = include_str!("../src/sdk_protocol_types.rs")
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub const "))
        .map(|line| line.split(':').next().unwrap())
        .collect();
    assert_eq!(
        actual_constants,
        [
            "LIMIT",
            "MARKET",
            "STOP_LOSS",
            "TAKE_PROFIT",
            "TWAP",
            "IMMEDIATE_OR_CANCEL",
            "GOOD_TILL_TIME",
            "POST_ONLY",
            "PENDING",
            "OPEN",
            "PARTIALLY_FILLED",
            "FILLED",
            "CANCELLED",
            "REJECTED",
            "EXPIRED",
            "INACTIVE",
            "ACTIVE",
            "TRIGGERED",
            "CANCELLED",
            "CROSS",
            "ISOLATED",
            "ONE_TRIGGERS_THE_OTHER",
            "ONE_CANCELS_THE_OTHER",
            "ONE_TRIGGERS_A_ONE_CANCELS_THE_OTHER",
            "BUY",
            "SELL",
            "SUCCESS",
            "INVALID_PARAMS",
            "AUTH_FAILED",
            "INSUFFICIENT_BALANCE",
            "ORDER_NOT_FOUND",
            "MARKET_NOT_FOUND",
            "RATE_LIMIT_EXCEEDED",
            "INTERNAL_ERROR",
            "NONCE_TOO_LOW",
            "NONCE_TOO_HIGH",
            "INVALID_SIGNATURE",
            "TX_EXPIRED"
        ],
        "numeric enum associated constant inventory"
    );
    for value in oracle["TxTypeValue"]["literals"].as_array().unwrap() {
        let expected = value.as_f64().unwrap();
        let parsed = TxTypeValue::deserialize(value.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap().as_f64().unwrap(),
            expected,
            "TxTypeValue Value roundtrip"
        );
        let parsed = TxTypeValue::deserialize(serde::de::value::F64Deserializer::<
            serde::de::value::Error,
        >::new(expected))
        .unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap().as_f64().unwrap(),
            expected,
            "TxTypeValue f64 roundtrip"
        );
    }
    for bad in [
        json!(null),
        json!(false),
        json!(true),
        json!([]),
        json!({}),
        json!({"L2CreateOrder":null}),
        json!("14"),
        json!(0),
        json!(-1),
        json!(14.5),
        json!(22),
        json!(999),
    ] {
        assert!(
            TxTypeValue::deserialize(bad.clone()).is_err(),
            "TxTypeValue rejects {bad}"
        );
    }
    for bad in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        14.000000000000002,
    ] {
        assert!(
            TxTypeValue::deserialize(
                serde::de::value::F64Deserializer::<serde::de::value::Error>::new(bad)
            )
            .is_err(),
            "TxTypeValue rejects f64 {bad}"
        );
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_protocol_type_source_controls_fail_by_name() {
    for (mutation, witness) in [
        ("widen", "TxTypeValue must remain a closed literal union"),
        ("numeric", "OrderType members"),
        ("string", "WsMessageType accepted metadata"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_protocol_types_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_PROTOCOL_TYPE_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(log.contains(witness), "{log}");
    }
}
#[test]
#[ignore = "requires Node"]
fn sdk_protocol_types_require_selected_compiler() {
    let output = Command::new("node")
        .arg(script())
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/absent-protocol-types-compiler"
            ),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
}
