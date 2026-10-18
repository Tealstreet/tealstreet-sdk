//! A normal public Type::deserialize call must not resolve to a permissive
//! inherent remote decoder instead of the string-required trait implementation.
use safe_cex_venue_bybit::{order_params::*, rate_limits::*, types::*};
use serde::Deserialize;

#[test]
fn public_qualified_decoders_reject_object_tags_and_tuple_params() {
    macro_rules! rejects {
        ($ty:ty, $json:literal) => {
            assert!(
                <$ty>::deserialize(&mut serde_json::Deserializer::from_str($json)).is_err(),
                "public qualified {} decoder accepted {}",
                stringify!($ty),
                $json
            );
        };
    }
    rejects!(BybitTransactionType, r#"{"TRADE":null}"#);
    rejects!(BybitTransactionAccountType, r#"{"UNIFIED":null}"#);
    rejects!(BybitTransactionCategory, r#"{"linear":null}"#);
    rejects!(BybitVipLevel, r#"{"No VIP":null}"#);
    rejects!(BybitProductType, r#"{"spot":null}"#);
    rejects!(BybitAccountType, r#"{"utaPro":null}"#);
    rejects!(BybitBboSideType, r#"{"Queue":null}"#);
    rejects!(BybitOrderExecutionParams, r#"["Queue","1","Percent","2"]"#);
}

#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn public_record_decoders_reject_positional_arrays() {
    use safe_cex_venue_bybit::{fill_event::BybitExecutionAmounts, response::BybitResponseMeta};
    let output = std::process::Command::new("node")
        .args([
            concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/check-shapes.mjs"),
            "--records",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    macro_rules! record {
        ($ty:ty) => {
            let map = records[stringify!($ty)].clone();
            assert!(
                <$ty>::deserialize(map.clone()).is_ok(),
                "{} valid map",
                stringify!($ty)
            );
            let sequence =
                serde_json::Value::Array(map.as_object().unwrap().values().cloned().collect());
            for value in [sequence, serde_json::json!([]), serde_json::Value::Null] {
                assert!(
                    <$ty>::deserialize(value.clone()).is_err(),
                    "public {} decoder accepted {}",
                    stringify!($ty),
                    value
                );
            }
        };
    }
    record!(BybitApiKeyInfo);
    record!(BybitApiKeyPermissions);
    record!(BybitTransactionRecord);
    record!(BybitTransactionHistoryOptions);
    record!(BybitRateLimitRow);
    record!(BybitRateLimitAccountRates);
    record!(BybitResponseMeta);
    record!(BybitExecutionAmounts);
}

#[test]
fn response_metadata_optional_fields_are_not_nullable() {
    use safe_cex_venue_bybit::response::BybitResponseMeta;
    use serde_json::json;
    let base = json!({"isSuccess":true,"hasExplicitStatus":false});
    assert!(BybitResponseMeta::deserialize(base.clone()).is_ok());
    for (field, good, bad) in [
        (
            "retCode",
            vec![json!(0), json!(1.5)],
            vec![json!(null), json!("0"), json!([])],
        ),
        (
            "retMsg",
            vec![json!(""), json!("ok")],
            vec![json!(null), json!(0), json!([])],
        ),
        (
            "rawResponse",
            vec![json!({}), json!([]), json!([1, null])],
            vec![json!(null), json!(17), json!("x"), json!(true)],
        ),
    ] {
        for (values, accepted) in [(good, true), (bad, false)] {
            for value in values {
                let mut input = base.clone();
                input[field] = value;
                assert_eq!(
                    BybitResponseMeta::deserialize(input.clone()).is_ok(),
                    accepted,
                    "{input}"
                );
            }
        }
    }
}

#[test]
fn public_decoders_preserve_native_variant_identity() {
    // Native matches defeat a decoder/serializer permutation that round-trips.
    macro_rules! identity {
        ($ty:ty, $wire:literal, $variant:path) => {
            assert!(
                matches!(
                    <$ty>::deserialize(serde_json::Value::String($wire.into())).unwrap(),
                    $variant
                ),
                "native variant identity: {} {}",
                stringify!($ty),
                $wire
            );
        };
    }
    identity!(BybitTransactionType, "TRADE", BybitTransactionType::Trade);
    identity!(
        BybitTransactionType,
        "SETTLEMENT",
        BybitTransactionType::Settlement
    );
    identity!(
        BybitTransactionType,
        "DELIVERY",
        BybitTransactionType::Delivery
    );
    identity!(
        BybitTransactionType,
        "TRANSFER",
        BybitTransactionType::Transfer
    );
    identity!(
        BybitTransactionType,
        "AIRDROP",
        BybitTransactionType::Airdrop
    );
    identity!(
        BybitTransactionType,
        "LIQUIDATION",
        BybitTransactionType::Liquidation
    );
    identity!(BybitTransactionType, "ADL", BybitTransactionType::Adl);
    identity!(
        BybitTransactionType,
        "BLOCK_TRADE",
        BybitTransactionType::BlockTrade
    );
    identity!(BybitTransactionType, "BONUS", BybitTransactionType::Bonus);
    identity!(
        BybitTransactionType,
        "FEE_REFUND",
        BybitTransactionType::FeeRefund
    );
    identity!(
        BybitTransactionType,
        "INTEREST",
        BybitTransactionType::Interest
    );
    identity!(
        BybitTransactionType,
        "CURRENCY_BUY",
        BybitTransactionType::CurrencyBuy
    );
    identity!(
        BybitTransactionType,
        "CURRENCY_SELL",
        BybitTransactionType::CurrencySell
    );
    identity!(
        BybitTransactionType,
        "AUTO_REPAYMENT",
        BybitTransactionType::AutoRepayment
    );
    identity!(
        BybitTransactionType,
        "AUTO_BORROW",
        BybitTransactionType::AutoBorrow
    );
    identity!(
        BybitTransactionType,
        "MANUAL_REPAYMENT",
        BybitTransactionType::ManualRepayment
    );
    identity!(
        BybitTransactionType,
        "SPOT_REPAY_COLLATERAL",
        BybitTransactionType::SpotRepayCollateral
    );
    identity!(
        BybitTransactionType,
        "CUSTODY_NETWORK",
        BybitTransactionType::CustodyNetwork
    );
    identity!(
        BybitTransactionType,
        "CUSTODY_SETTLE",
        BybitTransactionType::CustodySettle
    );
    identity!(
        BybitTransactionType,
        "CUSTODY",
        BybitTransactionType::Custody
    );
    identity!(
        BybitTransactionType,
        "CUSTODY_AUTO",
        BybitTransactionType::CustodyAuto
    );
    identity!(
        BybitTransactionType,
        "CUSTODY_MANUAL",
        BybitTransactionType::CustodyManual
    );
    identity!(
        BybitTransactionType,
        "PREMARKET_TRANSFER_IN",
        BybitTransactionType::PremarketTransferIn
    );
    identity!(
        BybitTransactionType,
        "PREMARKET_TRANSFER_OUT",
        BybitTransactionType::PremarketTransferOut
    );
    identity!(
        BybitTransactionType,
        "PREMARKET_DELIVERY",
        BybitTransactionType::PremarketDelivery
    );
    identity!(
        BybitTransactionType,
        "PREMARKET_ROLLBACK",
        BybitTransactionType::PremarketRollback
    );
    identity!(
        BybitTransactionType,
        "NONE_C2C_TRANSFER",
        BybitTransactionType::NoneC2cTransfer
    );
    identity!(
        BybitTransactionAccountType,
        "UNIFIED",
        BybitTransactionAccountType::Unified
    );
    identity!(
        BybitTransactionAccountType,
        "CONTRACT",
        BybitTransactionAccountType::Contract
    );
    identity!(
        BybitTransactionCategory,
        "linear",
        BybitTransactionCategory::Linear
    );
    identity!(
        BybitTransactionCategory,
        "inverse",
        BybitTransactionCategory::Inverse
    );
    identity!(
        BybitTransactionCategory,
        "option",
        BybitTransactionCategory::Option
    );
    identity!(
        BybitTransactionCategory,
        "spot",
        BybitTransactionCategory::Spot
    );
    identity!(BybitVipLevel, "No VIP", BybitVipLevel::NoVIP);
    identity!(BybitVipLevel, "VIP-1", BybitVipLevel::VIP1);
    identity!(BybitVipLevel, "VIP-2", BybitVipLevel::VIP2);
    identity!(BybitVipLevel, "VIP-3", BybitVipLevel::VIP3);
    identity!(BybitVipLevel, "VIP-4", BybitVipLevel::VIP4);
    identity!(BybitVipLevel, "VIP-5", BybitVipLevel::VIP5);
    identity!(BybitVipLevel, "VIP-Supreme", BybitVipLevel::VIPSupreme);
    identity!(BybitVipLevel, "PRO-1", BybitVipLevel::PRO1);
    identity!(BybitVipLevel, "PRO-2", BybitVipLevel::PRO2);
    identity!(BybitVipLevel, "PRO-3", BybitVipLevel::PRO3);
    identity!(BybitVipLevel, "PRO-4", BybitVipLevel::PRO4);
    identity!(BybitVipLevel, "PRO-5", BybitVipLevel::PRO5);
    identity!(BybitVipLevel, "PRO-6", BybitVipLevel::PRO6);
    identity!(BybitProductType, "futures", BybitProductType::Futures);
    identity!(BybitProductType, "option", BybitProductType::Option);
    identity!(BybitProductType, "spot", BybitProductType::Spot);
    identity!(
        BybitAccountType,
        "classicAndUTA",
        BybitAccountType::ClassicAndUTA
    );
    identity!(BybitAccountType, "utaPro", BybitAccountType::UTAPro);
    identity!(BybitBboSideType, "Queue", BybitBboSideType::Queue);
    identity!(
        BybitBboSideType,
        "Counterparty",
        BybitBboSideType::Counterparty
    );
}

#[test]
fn decoded_response_and_execution_fields_have_native_identity() {
    use safe_cex_venue_bybit::{fill_event::BybitExecutionAmounts, response::BybitResponseMeta};
    use serde_json::json;
    for (success, explicit) in [(true, false), (false, true)] {
        let decoded: BybitResponseMeta = serde_json::from_value(json!({"isSuccess":success,"hasExplicitStatus":explicit,"retCode":17,"retMsg":"identity","rawResponse":{"retCode":29}})).unwrap();
        assert_eq!(
            decoded.is_success, success,
            "native response is_success identity"
        );
        assert_eq!(
            decoded.has_explicit_status, explicit,
            "native response has_explicit_status identity"
        );
        assert_eq!(
            decoded.ret_code,
            Some(17.0),
            "native response ret_code identity"
        );
        assert_eq!(
            decoded.ret_msg.as_deref(),
            Some([105, 100, 101, 110, 116, 105, 116, 121].as_slice()),
            "native response ret_msg identity"
        );
        assert_eq!(
            decoded.raw_response,
            Some(json!({"retCode":29}).into()),
            "native response raw_response identity"
        );
    }
    let amounts: BybitExecutionAmounts =
        serde_json::from_value(json!({"price":13,"amount":23})).unwrap();
    assert_eq!(amounts.price, 13.0, "native execution price identity");
    assert_eq!(amounts.amount, 23.0, "native execution amount identity");
    let params: BybitOrderExecutionParams = serde_json::from_value(json!({"bboSideType":"Counterparty","bboLevel":"3","slippageToleranceType":"Percent","slippageTolerance":"7"})).unwrap();
    assert_eq!(
        params.bbo_side_type,
        Some(BybitBboSideType::Counterparty),
        "native params bbo side identity"
    );
    assert_eq!(
        params.bbo_level.as_deref(),
        Some("3"),
        "native params bbo level identity"
    );
    assert_eq!(
        params.slippage_tolerance_type.as_deref(),
        Some("Percent"),
        "native params slippage type identity"
    );
    assert_eq!(
        params.slippage_tolerance.as_deref(),
        Some("7"),
        "native params slippage value identity"
    );
}

#[test]
fn response_message_decoder_preserves_native_utf16_and_rejects_nontext() {
    use safe_cex_exchange::js_runtime::value::{from_retained_input, from_typed, to_typed};
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    use safe_cex_venue_bybit::response::BybitResponseMeta;

    for units in [vec![0xd800], vec![0xdc00], vec![0xd800, 120, 0xdc00]] {
        let data = Input::Object(vec![
            ("isSuccess".encode_utf16().collect(), Input::Bool(false)),
            (
                "hasExplicitStatus".encode_utf16().collect(),
                Input::Bool(true),
            ),
            (
                "retMsg".encode_utf16().collect(),
                Input::Text(units.clone()),
            ),
        ]);
        let decoded: BybitResponseMeta = to_typed(&from_retained_input(data)).unwrap();
        assert_eq!(decoded.ret_msg.as_deref(), Some(units.as_slice()));
        assert_eq!(
            from_typed(&decoded)
                .unwrap()
                .get("retMsg")
                .unwrap()
                .to_js_string()
                .unwrap()
                .as_ref(),
            units.as_slice()
        );
    }
    for invalid in [
        Input::Null,
        Input::Float(55296.),
        Input::Array(vec![Input::Float(55296.)]),
    ] {
        let data = Input::Object(vec![
            ("isSuccess".encode_utf16().collect(), Input::Bool(false)),
            (
                "hasExplicitStatus".encode_utf16().collect(),
                Input::Bool(true),
            ),
            ("retMsg".encode_utf16().collect(), invalid),
        ]);
        assert!(to_typed::<BybitResponseMeta>(&from_retained_input(data)).is_err());
    }
}
