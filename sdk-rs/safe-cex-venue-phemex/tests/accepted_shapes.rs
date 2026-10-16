//! Independent complete-record witnesses for the selected accepted-set owners.
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_phemex::types::{
    PhemexAccount, PhemexApiResponse, PhemexPosition, PositionsResponse,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

const ACCOUNT: &str = r#"{"userID":-0,"accountId":17.5,"currency":"USDT","accountBalanceRv":"123.25","totalUsedBalanceRv":"9.75","bonusBalanceRv":"1.125","status":2.5}"#;
const ACCOUNT_ARRAY: &str = r#"[-0,17.5,"USDT","123.25","9.75","1.125",2.5]"#;
const POSITION: &str = r#"{
"userID":-0,"accountID":17.5,"symbol":"BTCUSDT","currency":"USDT","side":"Buy",
"positionStatus":"Normal","crossMargin":true,"leverageRr":"lev","initMarginReqRr":"init",
"maintMarginReqRr":"maint","riskLimitRv":"risk","size":"size","valueRv":"value",
"avgEntryPriceRp":"entry-rp","avgEntryPrice":"entry","posCostRv":"cost",
"assignedPosBalanceRv":"assigned","bankruptCommRv":"bank-comm","bankruptPriceRp":"bank-price",
"positionMarginRv":"margin","liquidationPriceRp":"liq","deleveragePercentileRr":"deleverage",
"buyValueToCostRr":"buy-cost","sellValueToCostRr":"sell-cost","markPriceRp":"mark",
"estimatedOrdLossRv":"loss","usedBalanceRv":"used","cumClosedPnlRv":"closed",
"cumFundingFeeRv":"funding","cumTransactFeeRv":"fee","transactTimeNs":31.5,
"takerFeeRateRr":"taker","makerFeeRateRr":"maker","term":2.75,"lastTermEndTimeNs":-0,
"lastFundingTimeNs":-13.5,"curTermRealisedPnlRv":"realised","execSeq":3.25,
"posSide":"Long","posMode":"Hedged","buyLeavesValueRv":"buy-leaves","sellLeavesValueRv":"sell-leaves"}"#;
// Complete Serde declaration order, with every field independently valid.
const POSITION_ARRAY: &str = r#"[-0,17.5,"BTCUSDT","USDT","Buy","Normal",true,
"lev","init","maint","risk","size","value","entry-rp","entry","cost","assigned",
"bank-comm","bank-price","margin","liq","deleverage","buy-cost","sell-cost","mark",
"loss","used","closed","funding","fee",31.5,"taker","maker",2.75,-0,-13.5,
"realised",3.25,"Long","Hedged","buy-leaves","sell-leaves"]"#;

fn account(value: &PhemexAccount) {
    assert_eq!(value.user_id.to_bits(), 0x8000_0000_0000_0000);
    assert_eq!(value.account_id.to_bits(), 0x4031_8000_0000_0000);
    assert_eq!(value.currency, "USDT");
    assert_eq!(value.account_balance_rv, "123.25");
    assert_eq!(value.total_used_balance_rv, "9.75");
    assert_eq!(value.bonus_balance_rv, "1.125");
    assert_eq!(value.status.to_bits(), 0x4004_0000_0000_0000);
}
fn position(value: &PhemexPosition) {
    assert_eq!(value.user_id.to_bits(), 0x8000_0000_0000_0000);
    assert_eq!(value.account_id.to_bits(), 0x4031_8000_0000_0000);
    assert_eq!(value.symbol, "BTCUSDT");
    assert_eq!(value.currency, "USDT");
    assert_eq!(value.side, "Buy");
    assert_eq!(value.position_status, "Normal");
    assert!(value.cross_margin);
    assert_eq!(value.leverage_rr, "lev");
    assert_eq!(value.init_margin_req_rr, "init");
    assert_eq!(value.maint_margin_req_rr, "maint");
    assert_eq!(value.risk_limit_rv, "risk");
    assert_eq!(value.size, "size");
    assert_eq!(value.value_rv, "value");
    assert_eq!(value.avg_entry_price_rp, "entry-rp");
    assert_eq!(value.avg_entry_price, "entry");
    assert_eq!(value.pos_cost_rv, "cost");
    assert_eq!(value.assigned_pos_balance_rv, "assigned");
    assert_eq!(value.bankrupt_comm_rv, "bank-comm");
    assert_eq!(value.bankrupt_price_rp, "bank-price");
    assert_eq!(value.position_margin_rv, "margin");
    assert_eq!(value.liquidation_price_rp, "liq");
    assert_eq!(value.deleverage_percentile_rr, "deleverage");
    assert_eq!(value.buy_value_to_cost_rr, "buy-cost");
    assert_eq!(value.sell_value_to_cost_rr, "sell-cost");
    assert_eq!(value.mark_price_rp, "mark");
    assert_eq!(value.estimated_ord_loss_rv, "loss");
    assert_eq!(value.used_balance_rv, "used");
    assert_eq!(value.cum_closed_pnl_rv, "closed");
    assert_eq!(value.cum_funding_fee_rv, "funding");
    assert_eq!(value.cum_transact_fee_rv, "fee");
    assert_eq!(value.transact_time_ns.to_bits(), 0x403f_8000_0000_0000);
    assert_eq!(value.taker_fee_rate_rr, "taker");
    assert_eq!(value.maker_fee_rate_rr, "maker");
    assert_eq!(value.term.to_bits(), 0x4006_0000_0000_0000);
    assert_eq!(value.last_term_end_time_ns.to_bits(), 0x8000_0000_0000_0000);
    assert_eq!(value.last_funding_time_ns.to_bits(), 0xc02b_0000_0000_0000);
    assert_eq!(value.cur_term_realised_pnl_rv, "realised");
    assert_eq!(value.exec_seq.to_bits(), 0x400a_0000_0000_0000);
    assert_eq!(value.pos_side, "Long");
    assert_eq!(value.pos_mode, "Hedged");
    assert_eq!(value.buy_leaves_value_rv, "buy-leaves");
    assert_eq!(value.sell_leaves_value_rv, "sell-leaves");
}
fn positive<T: DeserializeOwned + Serialize + JsNumberOutput>(raw: &str, inspect: impl Fn(&T)) {
    let value: Value = serde_json::from_str(raw).unwrap();
    for native in [
        serde_json::from_str::<T>(raw).unwrap(),
        <T as Deserialize>::deserialize(value.clone()).unwrap(),
        <T as Deserialize>::deserialize(&value).unwrap(),
        js_from_str::<T>(raw).unwrap(),
        js_from_value::<T>(value.clone()).unwrap(),
        js_from_value_ref::<T>(&value).unwrap(),
    ] {
        inspect(&native);
    }
}
fn rejects<T: DeserializeOwned + Serialize + JsNumberOutput>(array: &str, label: &str) {
    for raw in [array, "[]", "null", "true", "17", "\"text\""] {
        let value: Value = serde_json::from_str(raw).unwrap();
        assert!(
            serde_json::from_str::<T>(raw).is_err(),
            "{label}: text {raw}"
        );
        assert!(
            <T as Deserialize>::deserialize(value.clone()).is_err(),
            "{label}: owned {raw}"
        );
        assert!(
            <T as Deserialize>::deserialize(&value).is_err(),
            "{label}: borrowed {raw}"
        );
        assert!(
            js_from_str::<T>(raw).is_err(),
            "{label}: numeric text {raw}"
        );
        assert!(
            js_from_value::<T>(value.clone()).is_err(),
            "{label}: numeric owned {raw}"
        );
        assert!(
            js_from_value_ref::<T>(&value).is_err(),
            "{label}: numeric borrowed {raw}"
        );
    }
}
#[test]
fn account_native_fields_and_complete_array_boundary() {
    positive(ACCOUNT, account);
    rejects::<PhemexAccount>(ACCOUNT_ARRAY, "ACCOUNT complete valid array");
}
#[test]
fn position_native_fields_and_complete_array_boundary() {
    positive(POSITION, position);
    rejects::<PhemexPosition>(POSITION_ARRAY, "POSITION complete valid array");
}
#[test]
fn positions_native_fields_and_complete_array_boundary() {
    let object = format!("{{\"account\":{ACCOUNT},\"positions\":[{POSITION}]}}");
    positive(&object, |value: &PositionsResponse| {
        account(&value.account);
        assert_eq!(value.positions.len(), 1);
        position(&value.positions[0]);
    });
    rejects::<PositionsResponse>(
        &format!("[{ACCOUNT},[{POSITION}]]"),
        "POSITIONS complete valid array",
    );
    for raw in [
        format!("{{\"account\":{ACCOUNT_ARRAY},\"positions\":[{POSITION}]}}"),
        format!("{{\"account\":{ACCOUNT},\"positions\":[{POSITION_ARRAY}]}}"),
    ] {
        assert!(
            js_from_str::<PositionsResponse>(&raw).is_err(),
            "nested record array"
        );
    }
}
#[test]
fn envelope_native_fields_and_complete_array_boundary() {
    let object = format!(
        "{{\"code\":-0,\"msg\":\"ready\",\"account\":{ACCOUNT},\"positions\":[{POSITION}]}}"
    );
    positive(&object, |value: &PhemexApiResponse<PositionsResponse>| {
        assert_eq!(value.code.to_bits(), 0x8000_0000_0000_0000);
        assert_eq!(value.msg, "ready");
        account(&value.payload.account);
        assert_eq!(value.payload.positions.len(), 1);
        position(&value.payload.positions[0]);
    });
    let payload = format!("{{\"account\":{ACCOUNT},\"positions\":[{POSITION}]}}");
    rejects::<PhemexApiResponse<PositionsResponse>>(
        &format!("[-0,\"ready\",{payload}]"),
        "ENVELOPE complete valid array",
    );
}

#[test]
fn envelope_streams_borrowed_payload_and_interleaved_metadata() {
    // Deserialize remains independent of Serialize/JsNumberOutput on T, and can
    // borrow directly from the original text rather than an owned Content map.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Borrowed<'a> {
        label: &'a str,
        reading: f64,
    }
    let raw = r#"{"reading":17.5,"code":-0,"label":"hello","msg":"ready"}"#;
    let value: PhemexApiResponse<Borrowed<'_>> = serde_json::from_str(raw).unwrap();
    assert_eq!(value.code.to_bits(), 0x8000_0000_0000_0000);
    assert_eq!(value.msg, "ready");
    assert_eq!(value.payload.reading.to_bits(), 0x4031_8000_0000_0000);
    assert_eq!(value.payload.label, "hello");
    let borrowed = value.payload.label.as_ptr() as usize;
    let start = raw.as_ptr() as usize;
    assert!((start..start + raw.len()).contains(&borrowed));
}

#[test]
fn envelope_retains_required_and_duplicate_metadata_checks() {
    #[derive(Deserialize)]
    struct Payload {}
    for raw in [
        r#"{"code":0,"code":1,"msg":"ready"}"#,
        r#"{"code":0,"msg":"ready","msg":"again"}"#,
        r#"{"msg":"ready"}"#,
        r#"{"code":0}"#,
        r#"{"code":null,"msg":"ready"}"#,
        r#"{"code":0,"msg":null}"#,
    ] {
        assert!(
            serde_json::from_str::<PhemexApiResponse<Payload>>(raw).is_err(),
            "required/duplicate metadata: {raw}"
        );
    }
}

#[test]
fn envelope_reads_late_metadata_after_a_custom_payload_finishes_early() {
    struct Early;
    impl<'de> Deserialize<'de> for Early {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct EarlyVisitor;
            impl<'de> serde::de::Visitor<'de> for EarlyVisitor {
                type Value = Early;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("an object")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    _: A,
                ) -> Result<Self::Value, A::Error> {
                    Ok(Early)
                }
            }
            d.deserialize_map(EarlyVisitor)
        }
    }
    let raw = r#"{"unused":{"skip":[1,2]},"msg":"late","code":3.5}"#;
    let value: PhemexApiResponse<Early> = serde_json::from_str(raw).unwrap();
    assert_eq!(value.code.to_bits(), 0x400c_0000_0000_0000);
    assert_eq!(value.msg, "late");
}
