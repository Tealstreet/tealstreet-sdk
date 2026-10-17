#[path = "support/field_identity.rs"]
mod identity;
// Direct native observations: never serialize a whole record to prove identity.
use safe_cex_venue_woo::types::*;
use serde_json::Value;

fn check_accountresponse(value: &AccountResponse, input: &Value) {
    assert_eq!(
        value.application_id,
        input["applicationId"].as_str().unwrap(),
        "AccountResponse.application_id native field identity"
    );
    assert_eq!(
        value.account,
        input["account"].as_str().unwrap(),
        "AccountResponse.account native field identity"
    );
    assert_eq!(
        value.alias,
        input["alias"].as_str().unwrap(),
        "AccountResponse.alias native field identity"
    );
    assert_eq!(
        value.account_mode,
        input["accountMode"].as_str().unwrap(),
        "AccountResponse.account_mode native field identity"
    );
    assert_eq!(
        value.position_mode,
        input["positionMode"].as_str().unwrap(),
        "AccountResponse.position_mode native field identity"
    );
    assert_eq!(
        value.leverage.to_bits(),
        input["leverage"].as_f64().unwrap().to_bits(),
        "AccountResponse.leverage native field identity"
    );
    assert_eq!(
        value.taker_fee_rate.to_bits(),
        input["takerFeeRate"].as_f64().unwrap().to_bits(),
        "AccountResponse.taker_fee_rate native field identity"
    );
    assert_eq!(
        value.maker_fee_rate.to_bits(),
        input["makerFeeRate"].as_f64().unwrap().to_bits(),
        "AccountResponse.maker_fee_rate native field identity"
    );
    assert_eq!(
        value.interest_rate.to_bits(),
        input["interestRate"].as_f64().unwrap().to_bits(),
        "AccountResponse.interest_rate native field identity"
    );
    assert_eq!(
        value.futures_taker_fee_rate.to_bits(),
        input["futuresTakerFeeRate"].as_f64().unwrap().to_bits(),
        "AccountResponse.futures_taker_fee_rate native field identity"
    );
    assert_eq!(
        value.futures_maker_fee_rate.to_bits(),
        input["futuresMakerFeeRate"].as_f64().unwrap().to_bits(),
        "AccountResponse.futures_maker_fee_rate native field identity"
    );
    assert_eq!(
        value.otpauth,
        input["otpauth"].as_bool().unwrap(),
        "AccountResponse.otpauth native field identity"
    );
    assert_eq!(
        value.margin_ratio.to_bits(),
        input["marginRatio"].as_f64().unwrap().to_bits(),
        "AccountResponse.margin_ratio native field identity"
    );
    assert_eq!(
        value.open_margin_ratio.to_bits(),
        input["openMarginRatio"].as_f64().unwrap().to_bits(),
        "AccountResponse.open_margin_ratio native field identity"
    );
    assert_eq!(
        value.initial_margin_ratio.to_bits(),
        input["initialMarginRatio"].as_f64().unwrap().to_bits(),
        "AccountResponse.initial_margin_ratio native field identity"
    );
    assert_eq!(
        value.maintenance_margin_ratio.to_bits(),
        input["maintenanceMarginRatio"].as_f64().unwrap().to_bits(),
        "AccountResponse.maintenance_margin_ratio native field identity"
    );
    assert_eq!(
        value.total_collateral.to_bits(),
        input["totalCollateral"].as_f64().unwrap().to_bits(),
        "AccountResponse.total_collateral native field identity"
    );
    assert_eq!(
        value.free_collateral.to_bits(),
        input["freeCollateral"].as_f64().unwrap().to_bits(),
        "AccountResponse.free_collateral native field identity"
    );
    assert_eq!(
        value.total_account_value.to_bits(),
        input["totalAccountValue"].as_f64().unwrap().to_bits(),
        "AccountResponse.total_account_value native field identity"
    );
    assert_eq!(
        value.total_vault_value.to_bits(),
        input["totalVaultValue"].as_f64().unwrap().to_bits(),
        "AccountResponse.total_vault_value native field identity"
    );
    assert_eq!(
        value.total_staking_value.to_bits(),
        input["totalStakingValue"].as_f64().unwrap().to_bits(),
        "AccountResponse.total_staking_value native field identity"
    );
    assert_eq!(
        value.referrer_id,
        input["referrerID"].as_str().unwrap(),
        "AccountResponse.referrer_id native field identity"
    );
    assert_eq!(
        value.account_type,
        input["accountType"].as_str().unwrap(),
        "AccountResponse.account_type native field identity"
    );
    assert_eq!(
        value.total_launchpad_vault_value.to_bits(),
        input["totalLaunchpadVaultValue"]
            .as_f64()
            .unwrap()
            .to_bits(),
        "AccountResponse.total_launchpad_vault_value native field identity"
    );
    assert_eq!(
        value.total_earn_value.to_bits(),
        input["totalEarnValue"].as_f64().unwrap().to_bits(),
        "AccountResponse.total_earn_value native field identity"
    );
}

fn check_placeorderresponse(value: &PlaceOrderResponse, input: &Value) {
    assert_eq!(
        value.success,
        input["success"].as_bool().unwrap(),
        "PlaceOrderResponse.success native field identity"
    );
    assert_eq!(
        value.timestamp,
        input["timestamp"].as_str().unwrap(),
        "PlaceOrderResponse.timestamp native field identity"
    );
    assert_eq!(
        value.order_id.to_bits(),
        input["order_id"].as_f64().unwrap().to_bits(),
        "PlaceOrderResponse.order_id native field identity"
    );
    assert_eq!(
        value.order_type,
        input["order_type"].as_str().unwrap(),
        "PlaceOrderResponse.order_type native field identity"
    );
    assert_eq!(
        value.order_price.to_bits(),
        input["order_price"].as_f64().unwrap().to_bits(),
        "PlaceOrderResponse.order_price native field identity"
    );
    assert_eq!(
        value.order_quantity.to_bits(),
        input["order_quantity"].as_f64().unwrap().to_bits(),
        "PlaceOrderResponse.order_quantity native field identity"
    );
    assert_eq!(
        value.order_amount.to_bits(),
        input["order_amount"].as_f64().unwrap().to_bits(),
        "PlaceOrderResponse.order_amount native field identity"
    );
    assert_eq!(
        value.client_order_id.to_bits(),
        input["client_order_id"].as_f64().unwrap().to_bits(),
        "PlaceOrderResponse.client_order_id native field identity"
    );
}

fn check_placealgoorderresponsedatarowsitem(
    value: &PlaceAlgoOrderResponseDataRowsItem,
    input: &Value,
) {
    match &value.order_id {
        PlaceAlgoOrderResponseDataRowsItemOrderId::Number(n) => assert_eq!(
            n.to_bits(),
            input["orderId"].as_f64().unwrap().to_bits(),
            "PlaceAlgoOrderResponseDataRowsItem.order_id native field identity"
        ),
        PlaceAlgoOrderResponseDataRowsItemOrderId::String(t) => assert_eq!(
            t,
            input["orderId"].as_str().unwrap(),
            "PlaceAlgoOrderResponseDataRowsItem.order_id native field identity"
        ),
    }
}

fn check_placealgoorderresponsedata(value: &PlaceAlgoOrderResponseData, input: &Value) {
    assert_eq!(
        value.rows.len(),
        input["rows"].as_array().unwrap().len(),
        "PlaceAlgoOrderResponseData.rows native field identity"
    );
    for (actual, expected) in value.rows.iter().zip(input["rows"].as_array().unwrap()) {
        check_placealgoorderresponsedatarowsitem(actual, expected);
    }
}

fn check_placealgoorderresponse(value: &PlaceAlgoOrderResponse, input: &Value) {
    assert_eq!(
        value.success,
        input["success"].as_bool().unwrap(),
        "PlaceAlgoOrderResponse.success native field identity"
    );
    assert_eq!(
        value.timestamp.to_bits(),
        input["timestamp"].as_f64().unwrap().to_bits(),
        "PlaceAlgoOrderResponse.timestamp native field identity"
    );
    check_placealgoorderresponsedata(&value.data, &input["data"]);
}

fn check_getfuturesleverageresponsedatadetailsitem(
    value: &GetFuturesLeverageResponseDataDetailsItem,
    input: &Value,
) {
    assert_eq!(
        value.position_side,
        input["position_side"].as_str().unwrap(),
        "GetFuturesLeverageResponseDataDetailsItem.position_side native field identity"
    );
    assert_eq!(
        value.leverage,
        input["leverage"].as_str().unwrap(),
        "GetFuturesLeverageResponseDataDetailsItem.leverage native field identity"
    );
}

fn check_getfuturesleverageresponsedata(value: &GetFuturesLeverageResponseData, input: &Value) {
    assert_eq!(
        value.symbol,
        input["symbol"].as_str().unwrap(),
        "GetFuturesLeverageResponseData.symbol native field identity"
    );
    assert_eq!(
        value.margin_mode,
        input["margin_mode"].as_str().unwrap(),
        "GetFuturesLeverageResponseData.margin_mode native field identity"
    );
    assert_eq!(
        value.default_margin_mode.as_deref(),
        input["default_margin_mode"].as_str(),
        "GetFuturesLeverageResponseData.default_margin_mode native field identity"
    );
    assert_eq!(
        value.position_mode,
        input["position_mode"].as_str().unwrap(),
        "GetFuturesLeverageResponseData.position_mode native field identity"
    );
    assert_eq!(
        value.details.len(),
        input["details"].as_array().unwrap().len(),
        "GetFuturesLeverageResponseData.details native field identity"
    );
    for (actual, expected) in value
        .details
        .iter()
        .zip(input["details"].as_array().unwrap())
    {
        check_getfuturesleverageresponsedatadetailsitem(actual, expected);
    }
}

fn check_getfuturesleverageresponse(value: &GetFuturesLeverageResponse, input: &Value) {
    assert_eq!(
        value.success,
        input["success"].as_bool().unwrap(),
        "GetFuturesLeverageResponse.success native field identity"
    );
    check_getfuturesleverageresponsedata(&value.data, &input["data"]);
    assert_eq!(
        value.timestamp.to_bits(),
        input["timestamp"].as_f64().unwrap().to_bits(),
        "GetFuturesLeverageResponse.timestamp native field identity"
    );
}

fn check_getklineresponsedatarowsitem(value: &GetKlineResponseDataRowsItem, input: &Value) {
    assert_eq!(
        value.open.to_bits(),
        input["open"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataRowsItem.open native field identity"
    );
    assert_eq!(
        value.close.to_bits(),
        input["close"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataRowsItem.close native field identity"
    );
    assert_eq!(
        value.low.to_bits(),
        input["low"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataRowsItem.low native field identity"
    );
    assert_eq!(
        value.high.to_bits(),
        input["high"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataRowsItem.high native field identity"
    );
    assert_eq!(
        value.volume.to_bits(),
        input["volume"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataRowsItem.volume native field identity"
    );
    assert_eq!(
        value.amount.to_bits(),
        input["amount"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataRowsItem.amount native field identity"
    );
    assert_eq!(
        value.symbol,
        input["symbol"].as_str().unwrap(),
        "GetKlineResponseDataRowsItem.symbol native field identity"
    );
    assert_eq!(
        value.r#type,
        input["type"].as_str().unwrap(),
        "GetKlineResponseDataRowsItem.r#type native field identity"
    );
    assert_eq!(
        value.start_timestamp.to_bits(),
        input["start_timestamp"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataRowsItem.start_timestamp native field identity"
    );
    assert_eq!(
        value.end_timestamp.to_bits(),
        input["end_timestamp"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataRowsItem.end_timestamp native field identity"
    );
}

fn check_getklineresponsedatameta(value: &GetKlineResponseDataMeta, input: &Value) {
    assert_eq!(
        value.total.to_bits(),
        input["total"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataMeta.total native field identity"
    );
    assert_eq!(
        value.records_per_page.to_bits(),
        input["records_per_page"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataMeta.records_per_page native field identity"
    );
    assert_eq!(
        value.current_page.to_bits(),
        input["current_page"].as_f64().unwrap().to_bits(),
        "GetKlineResponseDataMeta.current_page native field identity"
    );
}

fn check_getklineresponsedata(value: &GetKlineResponseData, input: &Value) {
    assert_eq!(
        value.rows.len(),
        input["rows"].as_array().unwrap().len(),
        "GetKlineResponseData.rows native field identity"
    );
    for (actual, expected) in value.rows.iter().zip(input["rows"].as_array().unwrap()) {
        check_getklineresponsedatarowsitem(actual, expected);
    }
    check_getklineresponsedatameta(&value.meta, &input["meta"]);
}

fn check_getklineresponse(value: &GetKlineResponse, input: &Value) {
    assert_eq!(
        value.success,
        input["success"].as_bool().unwrap(),
        "GetKlineResponse.success native field identity"
    );
    check_getklineresponsedata(&value.data, &input["data"]);
    assert_eq!(
        value.timestamp.to_bits(),
        input["timestamp"].as_f64().unwrap().to_bits(),
        "GetKlineResponse.timestamp native field identity"
    );
}

#[test]
fn legacy_native_field_identity_through_outer_record() {
    let cases: Vec<Value> = serde_json::from_str(&identity::capture("types")).unwrap();
    for case in cases.iter().filter(|case| case["accepted"] == true) {
        let input = &case["input"];
        match case["name"].as_str().unwrap() {
            "ACCOUNT_RESPONSE" => check_accountresponse(
                &serde_json::from_value::<AccountResponse>(input.clone()).unwrap(),
                input,
            ),
            "PLACE_ORDER_RESPONSE" => check_placeorderresponse(
                &serde_json::from_value::<PlaceOrderResponse>(input.clone()).unwrap(),
                input,
            ),
            "PLACE_ALGO_ORDER_RESPONSE" => check_placealgoorderresponse(
                &serde_json::from_value::<PlaceAlgoOrderResponse>(input.clone()).unwrap(),
                input,
            ),
            "GET_FUTURES_LEVERAGE_RESPONSE" => check_getfuturesleverageresponse(
                &serde_json::from_value::<GetFuturesLeverageResponse>(input.clone()).unwrap(),
                input,
            ),
            "GET_KLINE_RESPONSE" => check_getklineresponse(
                &serde_json::from_value::<GetKlineResponse>(input.clone()).unwrap(),
                input,
            ),
            name => panic!("unmapped native identity record {name}"),
        }
    }
}
