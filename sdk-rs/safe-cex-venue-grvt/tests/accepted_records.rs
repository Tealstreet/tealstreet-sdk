//! Direct object-only decoder witnesses with complete ordered counterexamples.
use safe_cex_venue_grvt::records::*;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
#[path = "support/enum_identity.rs"]
mod enum_identity;
#[path = "support/record_identity.rs"]
mod record_identity;
use record_identity::NativeIdentity;

fn check<T: DeserializeOwned + NativeIdentity>(name: &str, observe: impl Fn(T) -> Value) {
    let fixtures: Value = serde_json::from_str(include_str!("accepted_record_cases.json")).unwrap();
    let row = &fixtures[name];
    assert!(row.is_object(), "GRVT MAP missing reviewed owner {name}");
    for (key, index) in [("sample", 0), ("second", 1)] {
        let input = &row[key];
        let raw = input.to_string();
        let value: T = serde_json::from_str(&raw).unwrap();
        value.assert_native(input, name);
        assert_eq!(
            observe(value),
            row["nativeOutputs"][index],
            "GRVT MAP {name} native fields/presence/bits via text"
        );
        let value: T = serde_json::from_value(input.clone()).unwrap();
        value.assert_native(input, name);
        assert_eq!(
            observe(value),
            row["nativeOutputs"][index],
            "GRVT MAP {name} native fields/presence/bits via value"
        );
        let value = <T as Deserialize>::deserialize(input).unwrap();
        value.assert_native(input, name);
        assert_eq!(
            observe(value),
            row["nativeOutputs"][index],
            "GRVT MAP {name} borrowed native fields/presence/bits"
        );
        let mut decoder = serde_json::Deserializer::from_str(&raw);
        let value = <T as Deserialize>::deserialize(&mut decoder).unwrap();
        decoder.end().unwrap();
        value.assert_native(input, name);
        assert_eq!(
            observe(value),
            row["nativeOutputs"][index],
            "GRVT MAP {name} qualified native fields/presence/bits"
        );
    }
    let ordered = &row["completePositionalArray"];
    assert_eq!(
        row["orderedFields"].as_array().unwrap().len(),
        ordered.as_array().unwrap().len(),
        "GRVT MAP {name} complete reviewed field order"
    );
    let raw = ordered.to_string();
    if let Ok(value) = serde_json::from_str::<T>(&raw) {
        // A control may count only after every decoded native field agrees with
        // the valid object: wrongly ordered string siblings must fail separately.
        value.assert_native(&row["sample"], name);
        assert_eq!(
            observe(value),
            row["nativeOutputs"][0],
            "GRVT MAP {name} counterexample native values"
        );
        panic!("GRVT MAP {name} accepted complete positional array via text");
    }
    assert!(
        serde_json::from_value::<T>(ordered.clone()).is_err(),
        "GRVT MAP {name} accepted complete positional array via value"
    );
    assert!(
        <T as Deserialize>::deserialize(ordered).is_err(),
        "GRVT MAP {name} accepted complete positional array via borrowed value"
    );
    let mut decoder = serde_json::Deserializer::from_str(&raw);
    assert!(
        <T as Deserialize>::deserialize(&mut decoder).is_err(),
        "GRVT MAP {name} accepted complete positional array via qualified trait"
    );
    for invalid in [
        Value::Null,
        json!([]),
        json!(false),
        json!(0),
        json!("text"),
        json!({}),
    ] {
        assert!(
            serde_json::from_value::<T>(invalid.clone()).is_err(),
            "GRVT MAP {name} invalid input accepted: {invalid}"
        );
    }
}

#[test]
fn grvt_instrument_requires_object_shape() {
    check::<GrvtInstrument>(
        "GrvtInstrument",
        |d| json!({"ok":true,"id":d.instrument_id,"hash":d.instrument_hash,"base":d.base,"quote":d.quote,"perpetual":matches!(d.kind,safe_cex_venue_grvt::types::GrvtAssetKind::Perpetual),"active":d.is_active}),
    );
}

#[test]
fn grvt_ticker_requires_object_shape() {
    check::<GrvtTicker>(
        "GrvtTicker",
        |d| json!({"ok":true,"id":d.instrument_id,"bid":d.bid_price,"ask":d.ask_price,"funding_present":d.funding_rate.is_some(),"funding":d.funding_rate}),
    );
}

#[test]
fn grvt_order_book_level_requires_object_shape() {
    check::<GrvtOrderBookLevel>(
        "GrvtOrderBookLevel",
        |d| json!({"ok":true,"price":d.p,"size":d.s,"orders":d.n}),
    );
}

#[test]
fn grvt_order_book_requires_object_shape() {
    check::<GrvtOrderBook>(
        "GrvtOrderBook",
        |d| json!({"ok":true,"id":d.instrument_id,"bid_count":d.bids.len(),"ask_count":d.asks.len(),"first_bid":d.bids.first().map(|v|v.p.clone())}),
    );
}

#[test]
fn grvt_trade_requires_object_shape() {
    check::<GrvtTrade>(
        "GrvtTrade",
        |d| json!({"ok":true,"id":d.trade_id,"price":d.price,"size":d.size,"buy":matches!(d.side,safe_cex_venue_grvt::types::GrvtOrderSide::Buy)}),
    );
}

#[test]
fn grvt_kline_requires_object_shape() {
    check::<GrvtKline>(
        "GrvtKline",
        |d| json!({"ok":true,"time":d.timestamp,"open":d.open,"high":d.high,"low":d.low,"close":d.close,"volume":d.volume}),
    );
}

#[test]
fn grvt_position_requires_object_shape() {
    check::<GrvtPosition>(
        "GrvtPosition",
        |d| json!({"ok":true,"account":d.sub_account_id,"size":d.size,"entry":d.entry_price,"realized":d.realized_pnl,"margin":d.margin}),
    );
}

#[test]
fn grvt_balance_requires_object_shape() {
    check::<GrvtBalance>(
        "GrvtBalance",
        |d| json!({"ok":true,"currency":d.currency,"total":d.total,"available":d.available,"reserved":d.reserved}),
    );
}

#[test]
fn grvt_account_summary_requires_object_shape() {
    check::<GrvtAccountSummary>(
        "GrvtAccountSummary",
        |d| json!({"ok":true,"account":d.sub_account_id,"balances":d.balances.len(),"liquidatable":d.is_liquidatable,"collateral":d.total_collateral}),
    );
}

#[test]
fn grvt_order_requires_object_shape() {
    check::<GrvtOrder>(
        "GrvtOrder",
        |d| json!({"ok":true,"id":d.order_id,"client_present":d.client_order_id.is_some(),"client":d.client_order_id,"market":matches!(d.r#type,safe_cex_venue_grvt::types::GrvtOrderType::Market),"filled":matches!(d.status,safe_cex_venue_grvt::types::GrvtOrderStatus::Filled),"buy":matches!(d.side,safe_cex_venue_grvt::types::GrvtOrderSide::Buy),"post_only":d.post_only,"reduce_only":d.reduce_only,"expiry_present":d.expires_at.is_some()}),
    );
}

#[test]
fn grvt_auth_request_requires_object_shape() {
    check::<GrvtAuthRequest>(
        "GrvtAuthRequest",
        |d| json!({"ok":true,"key":d.api_key,"signature":d.signature,"nonce":d.nonce,"expiry":d.expires_at}),
    );
}

#[test]
fn grvt_auth_response_requires_object_shape() {
    check::<GrvtAuthResponse>(
        "GrvtAuthResponse",
        |d| json!({"ok":true,"token":d.session_token,"account":d.account_id,"sub_account":d.sub_account_id,"expiry":d.expires_at}),
    );
}

#[test]
fn grvt_fill_requires_object_shape() {
    check::<GrvtFill>(
        "GrvtFill",
        |d| json!({"ok":true,"fill":d.fill_id,"order":d.order_id,"fee":d.fee,"currency":d.fee_currency,"maker":matches!(d.liquidity,safe_cex_venue_grvt::types::GrvtLiquidity::Maker)}),
    );
}

#[test]
fn order_leg_requires_object_shape() {
    check::<OrderLeg>(
        "OrderLeg",
        |d| json!({"ok":true,"instrument":d.instrument,"size":d.size,"limit_present":d.limit_price.is_some(),"limit":d.limit_price,"buying":d.is_buying_asset}),
    );
}

#[test]
fn grvt_builder_authorization_result_requires_object_shape() {
    check::<GrvtBuilderAuthorizationResult>(
        "GrvtBuilderAuthorizationResult",
        |d| json!({"ok":true,"success":d.success,"message":d.message}),
    );
}

#[test]
fn grvt_instrument_lite_requires_object_shape() {
    check::<GrvtInstrumentLite>(
        "GrvtInstrumentLite",
        |d| json!({"ok":true,"id":d.i,"base":d.b,"quote":d.q,"bd_bits":d.bd.map(|v|format!("{:016x}",v.to_bits())),"qd_bits":d.qd.map(|v|format!("{:016x}",v.to_bits())),"fi_bits":d.fi.map(|v|format!("{:016x}",v.to_bits())),"active":d.ia,"venues":d.v.as_ref().map(|v|v.len())}),
    );
}

#[test]
fn grvt_signature_requires_object_shape() {
    check::<GrvtSignature>(
        "GrvtSignature",
        |d| json!({"ok":true,"r":d.r,"s":d.s,"signer":d.signer,"v_bits":format!("{:016x}",d.v.to_bits()),"nonce_bits":format!("{:016x}",d.nonce.to_bits())}),
    );
}

#[test]
fn order_payload_requires_object_shape() {
    check::<OrderPayload>(
        "OrderPayload",
        |d| json!({"ok":true,"account":d.sub_account_id,"market":d.is_market,"post_only":d.post_only,"reduce_only":d.reduce_only,"legs":d.legs.len(),"nonce_bits":format!("{:016x}",d.nonce.to_bits())}),
    );
}

#[test]
fn grvt_builder_check_result_requires_object_shape() {
    check::<GrvtBuilderCheckResult>(
        "GrvtBuilderCheckResult",
        |d| json!({"ok":true,"authorized":d.authorized,"fee_present":d.max_fee_rate.is_some(),"fee":d.max_fee_rate,"raw_bits":d.max_fee_rate_raw.map(|v|format!("{:016x}",v.to_bits()))}),
    );
}

#[test]
fn grvt_builder_info_requires_object_shape() {
    check::<GrvtBuilderInfo>(
        "GrvtBuilderInfo",
        |d| json!({"ok":true,"builder":d.builder,"fee":d.builder_fee,"raw_bits":format!("{:016x}",d.builder_fee_int.to_bits())}),
    );
}

#[test]
fn grvt_web_socket_subscription_params_requires_object_shape() {
    check::<GrvtWebSocketSubscriptionParams>(
        "GrvtWebSocketSubscriptionParams",
        |d| json!({"ok":true,"channel":d.channel,"instrument_present":d.instrument_id.is_some(),"instrument":d.instrument_id,"interval":d.interval}),
    );
}

#[test]
fn grvt_web_socket_subscription_requires_object_shape() {
    check::<GrvtWebSocketSubscription>(
        "GrvtWebSocketSubscription",
        |d| json!({"ok":true,"v2":matches!(d.jsonrpc,safe_cex_venue_grvt::types::GrvtJsonRpcVersion::V2),"id_bits":format!("{:016x}",d.id.to_bits()),"subscribe":matches!(d.method,safe_cex_venue_grvt::types::GrvtSubscriptionMethod::Subscribe),"channel":d.params.channel,"instrument_present":d.params.instrument_id.is_some()}),
    );
}

#[test]
fn grvt_web_socket_error_requires_object_shape() {
    check::<GrvtWebSocketError>(
        "GrvtWebSocketError",
        |d| json!({"ok":true,"code_bits":format!("{:016x}",d.code.to_bits()),"message":d.message,"data_present":d.data.is_some()}),
    );
}

#[test]
fn grvt_web_socket_message_requires_object_shape() {
    check::<GrvtWebSocketMessage>(
        "GrvtWebSocketMessage",
        |d| json!({"ok":true,"v2":matches!(d.jsonrpc,safe_cex_venue_grvt::types::GrvtJsonRpcVersion::V2),"method":d.method,"params_present":d.params.is_some(),"result_present":d.result.is_some(),"error_present":d.error.is_some(),"error_code_bits":d.error.as_ref().map(|v|format!("{:016x}",v.code.to_bits()))}),
    );
}

#[test]
fn grvt_trigger_metadata_requires_object_shape() {
    check::<GrvtTriggerMetadata>(
        "GrvtTriggerMetadata",
        |d| json!({"ok":true,"stop_loss":matches!(d.r#type,safe_cex_venue_grvt::types::GrvtTriggerKind::StopLoss),"price_bits":d.trigger_price.map(|v|format!("{:016x}",v.to_bits())),"mark":matches!(d.trigger_type,Some(safe_cex_types::OrderStopTriggerType::MarkPrice)),"close":d.close,"split":d.is_split_position}),
    );
}
