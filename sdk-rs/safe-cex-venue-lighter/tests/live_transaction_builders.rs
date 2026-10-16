use num_bigint::BigInt;
use safe_cex_venue_lighter::{transaction_builders::*, transaction_client_types::*};
use serde_json::{Value, json};
use std::{cell::Cell, process::Command};

fn capture() -> &'static Value {
    static CAPTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPTURE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/transaction-builders-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["contentHash"],
            "a54b6bbb656b67eb70b312ad28c820ed42e31d81fa339e2cd4dee1283ffd2ba9",
            "transaction builder live corpus content"
        );
        value
    })
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["number"].as_str().unwrap(), 16).unwrap())
}
fn bigint(v: &Value) -> BigInt {
    v["bigint"].as_str().unwrap().parse().unwrap()
}
fn text(v: &Value) -> Vec<u16> {
    v["utf16"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn missing(v: Option<&Value>) -> bool {
    v.is_none_or(|v| v.get("undefined") == Some(&json!(true)))
}
fn opt_big(v: &Value, key: &str) -> Option<BigInt> {
    v.get(key).filter(|v| !missing(Some(v))).map(bigint)
}
fn opt_num(v: &Value, key: &str) -> Option<f64> {
    v.get(key).filter(|v| !missing(Some(v))).map(number)
}
fn opt_bool(v: &Value, key: &str) -> Option<bool> {
    v.get(key).and_then(Value::as_bool)
}
fn opt_text(v: &Value, key: &str) -> Option<Vec<u16>> {
    v.get(key).filter(|v| !missing(Some(v))).map(text)
}
fn order_kind(v: Option<&Value>) -> Option<CreateOrderKind> {
    if missing(v) {
        return None;
    }
    Some(
        match String::from_utf16(&text(v.unwrap())).unwrap().as_str() {
            "limit" => CreateOrderKind::Limit,
            "market" => CreateOrderKind::Market,
            "stop_loss" => CreateOrderKind::StopLoss,
            "stop_loss_limit" => CreateOrderKind::StopLossLimit,
            "take_profit" => CreateOrderKind::TakeProfit,
            "take_profit_limit" => CreateOrderKind::TakeProfitLimit,
            _ => panic!("outside source literal union"),
        },
    )
}
fn tif(v: Option<&Value>) -> Option<CreateOrderTimeInForce> {
    if missing(v) {
        return None;
    }
    Some(
        match String::from_utf16(&text(v.unwrap())).unwrap().as_str() {
            "GoodTillCancel" => CreateOrderTimeInForce::GoodTillCancel,
            "ImmediateOrCancel" => CreateOrderTimeInForce::ImmediateOrCancel,
            "FillOrKill" => CreateOrderTimeInForce::FillOrKill,
            "PostOnly" => CreateOrderTimeInForce::PostOnly,
            _ => panic!("outside source literal union"),
        },
    )
}
// This test-only carrier decodes tagged Node-native values. Source bigint interfaces
// have no JSON wire codec; production parameter structs are native value records.
fn order(v: &Value) -> CreateOrderParams {
    CreateOrderParams {
        order_id: bigint(&v["orderId"]),
        market_id: number(&v["marketId"]),
        is_buy: v["isBuy"].as_bool().unwrap(),
        price: bigint(&v["price"]),
        amount: bigint(&v["amount"]),
        post_only: opt_bool(v, "postOnly"),
        reduce_only: opt_bool(v, "reduceOnly"),
        order_type: order_kind(v.get("orderType")),
        trigger_price: opt_big(v, "triggerPrice"),
        order_expiry: opt_big(v, "orderExpiry"),
        expired_at: opt_big(v, "expiredAt"),
        time_in_force: tif(v.get("timeInForce")),
        integrator_account_index: opt_big(v, "integratorAccountIndex"),
        integrator_maker_fee: opt_num(v, "integratorMakerFee"),
        integrator_taker_fee: opt_num(v, "integratorTakerFee"),
    }
}
fn group(v: &Value) -> CreateGroupedOrdersParams {
    CreateGroupedOrdersParams {
        grouping_type: match number(&v["groupingType"]) {
            1.0 => LighterGroupedOrderType::OneTriggersTheOther,
            2.0 => LighterGroupedOrderType::OneCancelsTheOther,
            3.0 => LighterGroupedOrderType::OneTriggersOco,
            _ => panic!("outside source literal union"),
        },
        orders: v["orders"].as_array().unwrap().iter().map(order).collect(),
        integrator_account_index: opt_big(v, "integratorAccountIndex"),
        integrator_maker_fee: opt_num(v, "integratorMakerFee"),
        integrator_taker_fee: opt_num(v, "integratorTakerFee"),
    }
}
fn approve(v: &Value) -> ApproveIntegratorParams {
    ApproveIntegratorParams {
        integrator_account_index: bigint(&v["integratorAccountIndex"]),
        max_perps_taker_fee: number(&v["maxPerpsTakerFee"]),
        max_perps_maker_fee: number(&v["maxPerpsMakerFee"]),
        max_spot_taker_fee: number(&v["maxSpotTakerFee"]),
        max_spot_maker_fee: number(&v["maxSpotMakerFee"]),
        approval_expiry: bigint(&v["approvalExpiry"]),
        nonce: opt_big(v, "nonce"),
        l1_signature: opt_text(v, "l1Signature"),
    }
}
fn n(v: f64) -> Value {
    json!({"number":format!("{:016x}",v.to_bits())})
}
fn b(v: &BigInt) -> Value {
    json!({"bigint":v.to_string()})
}
fn t(v: &str) -> Value {
    json!({"utf16":v.encode_utf16().collect::<Vec<_>>()})
}
fn optional(v: Option<Value>) -> Value {
    v.unwrap_or_else(|| json!({"undefined":true}))
}
fn ob(v: &Option<BigInt>) -> Value {
    optional(v.as_ref().map(b))
}
fn on(v: Option<f64>) -> Value {
    optional(v.map(n))
}
// These projections explicitly read native fields; they never serialize the structs.
fn order_native(v: &CreateOrderParams) -> Value {
    json!({"orderId":b(&v.order_id),"marketId":n(v.market_id),"isBuy":v.is_buy,"price":b(&v.price),"amount":b(&v.amount),"postOnly":optional(v.post_only.map(|v|json!(v))),"reduceOnly":optional(v.reduce_only.map(|v|json!(v))),"orderType":optional(v.order_type.map(|v|t(v.as_str()))),"triggerPrice":ob(&v.trigger_price),"orderExpiry":ob(&v.order_expiry),"expiredAt":ob(&v.expired_at),"timeInForce":optional(v.time_in_force.map(|v|t(v.as_str()))),"integratorAccountIndex":ob(&v.integrator_account_index),"integratorMakerFee":on(v.integrator_maker_fee),"integratorTakerFee":on(v.integrator_taker_fee)})
}
fn identity(native: &Value, source: &Value, label: &str) {
    for (field, value) in native.as_object().unwrap() {
        assert_eq!(
            value,
            source.get(field).unwrap_or(&json!({"undefined":true})),
            "{label}.{field} native field identity"
        );
    }
}
#[test]
#[ignore = "actual source Node oracle"]
fn transaction_parameter_native_identity() {
    let records = &capture()["recordInputs"];
    let getter_cases = capture()["getters"].as_array().unwrap();
    assert_eq!(getter_cases.len(), 4, "native getter independent inventory");
    for row in getter_cases {
        let config = &row["config"];
        let account = bigint(&config["accountIndex"]);
        let clock = || panic!("getters must never access clock");
        let context = TransactionBuildContext {
            chain_id: number(&config["chainId"]),
            account_index: &account,
            api_key_index: number(&config["apiKeyIndex"]),
            now_millis: &clock,
        };
        assert_eq!(
            get_account_index(&context),
            bigint(&row["accountIndex"]),
            "getAccountIndex native bigint"
        );
        assert_eq!(
            get_api_key_index(&context).to_bits(),
            number(&row["apiKeyIndex"]).to_bits(),
            "getApiKeyIndex native number bits"
        );
        assert_eq!(
            get_chain_id(&context).to_bits(),
            number(&row["chainId"]).to_bits(),
            "getChainId native number bits"
        );
    }

    for (name, count) in [
        ("CreateOrderParams", 2),
        ("CreateGroupedOrdersParams", 2),
        ("CancelOrderParams", 1),
        ("TransferParams", 2),
        ("UpdateLeverageParams", 1),
        ("ApproveIntegratorParams", 2),
    ] {
        let rows = records[name].as_array().unwrap();
        assert_eq!(rows.len(), count, "{name} native record inventory");
        for input in rows {
            let native = match name {
                "CreateOrderParams" => order_native(&order(input).clone()),
                "CreateGroupedOrdersParams" => {
                    let v = group(input).clone();
                    assert_eq!(
                        v.orders.len(),
                        if v.grouping_type == LighterGroupedOrderType::OneTriggersTheOther {
                            2
                        } else {
                            3
                        },
                        "grouped parameter native length"
                    );
                    for (index, item) in v.orders.iter().enumerate() {
                        identity(
                            &order_native(item),
                            &input["orders"][index],
                            "nested CreateOrderParams",
                        );
                    }
                    json!({"groupingType":n(v.grouping_type.value()),"integratorAccountIndex":ob(&v.integrator_account_index),"integratorMakerFee":on(v.integrator_maker_fee),"integratorTakerFee":on(v.integrator_taker_fee)})
                }
                "CancelOrderParams" => {
                    let v = CancelOrderParams {
                        order_id: bigint(&input["orderId"]),
                        market_id: number(&input["marketId"]),
                    }
                    .clone();
                    json!({"orderId":b(&v.order_id),"marketId":n(v.market_id)})
                }
                "TransferParams" => {
                    let v = TransferParams {
                        to_account_index: bigint(&input["toAccountIndex"]),
                        amount: bigint(&input["amount"]),
                        token_id: opt_num(input, "tokenId"),
                    }
                    .clone();
                    json!({"toAccountIndex":b(&v.to_account_index),"amount":b(&v.amount),"tokenId":on(v.token_id)})
                }
                "UpdateLeverageParams" => {
                    let v = UpdateLeverageParams {
                        market_id: number(&input["marketId"]),
                        leverage: number(&input["leverage"]),
                        margin_mode: MarginMode::new(number(&input["marginMode"])).unwrap(),
                    }
                    .clone();
                    json!({"marketId":n(v.market_id),"leverage":n(v.leverage),"marginMode":n(v.margin_mode.value())})
                }
                "ApproveIntegratorParams" => {
                    let v = approve(input).clone();
                    json!({"integratorAccountIndex":b(&v.integrator_account_index),"maxPerpsTakerFee":n(v.max_perps_taker_fee),"maxPerpsMakerFee":n(v.max_perps_maker_fee),"maxSpotTakerFee":n(v.max_spot_taker_fee),"maxSpotMakerFee":n(v.max_spot_maker_fee),"approvalExpiry":b(&v.approval_expiry),"nonce":ob(&v.nonce),"l1Signature":optional(v.l1_signature.map(|v|json!({"utf16":v})))})
                }
                _ => unreachable!(),
            };
            identity(&native, input, name);
        }
    }
    for (value, expected) in [
        (LighterGroupedOrderType::OneTriggersTheOther, 1.0),
        (LighterGroupedOrderType::OneCancelsTheOther, 2.0),
        (LighterGroupedOrderType::OneTriggersOco, 3.0),
    ] {
        assert_eq!(
            value.value(),
            expected,
            "LighterGroupedOrderType native literal"
        );
    }
    assert_eq!(
        MarginMode::new(-0.0).unwrap().value().to_bits(),
        (-0.0f64).to_bits(),
        "MarginMode native negative zero"
    );
    for value in [-1.0, 2.0, f64::NAN, f64::INFINITY] {
        assert!(MarginMode::new(value).is_none());
    }
}
fn error_identity(error: TransactionBuildError, expected: &Value) {
    assert_eq!(
        error.name,
        expected["name"].as_str().unwrap(),
        "builder native error name"
    );
    assert_eq!(
        error.message,
        expected["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
            .collect::<Vec<_>>(),
        "builder native error UTF16 message"
    );
}
fn metadata(native: &CreateOrderData, expected: &Value) {
    identity(
        &json!({"expiredAt":b(&native.expired_at),"orderTypeValue":n(native.order_type_value),"timeInForceValue":n(native.time_in_force_value),"triggerPrice":b(&native.trigger_price),"orderExpiry":b(&native.order_expiry),"integratorAccountIndex":ob(&native.integrator_account_index),"integratorMakerFee":on(native.integrator_maker_fee),"integratorTakerFee":on(native.integrator_taker_fee)}),
        expected,
        "CreateOrderData",
    );
}
fn utf8(v: &Value) -> String {
    String::from_utf16(&text(v)).unwrap()
}
fn words(hex: &str, count: usize) -> Vec<i64> {
    assert_eq!(hex.len(), count * 16, "preimage independent word count");
    (0..count)
        .map(|i| {
            let word = &hex[i * 16..i * 16 + 16];
            let mut bytes = [0u8; 8];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&word[index * 2..index * 2 + 2], 16).unwrap();
            }
            i64::from_le_bytes(bytes)
        })
        .collect()
}
fn assert_words(hex: &str, expected: &[i64], label: &str) {
    assert_eq!(
        words(hex, expected.len()),
        expected,
        "{label} decoded native words"
    );
}
fn attributes_expected(index: &BigInt, taker: f64, maker: f64) -> Vec<i64> {
    let mut values = vec![
        (1, i64::try_from(index).unwrap()),
        (2, taker as i64),
        (3, maker as i64),
    ];
    values.retain(|(_, v)| *v != 0);
    while values.len() < 4 {
        values.push((0, 0));
    }
    values.into_iter().flat_map(|(k, v)| [k, v]).collect()
}
#[test]
#[ignore = "actual source Node oracle"]
fn transaction_builder_native_results() {
    let rows = capture()["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 410, "builder case inventory: 311+9+4+38+29+19");
    let mut visited = std::collections::BTreeMap::new();
    for row in rows {
        let method = row["method"].as_str().unwrap();
        *visited.entry(method).or_insert(0usize) += 1;
        let params = &row["params"];
        let nonce = row
            .get("nonce")
            .map(bigint)
            .unwrap_or_else(|| BigInt::from(7));
        let config = &row["config"];
        let account = config
            .get("accountIndex")
            .map(bigint)
            .unwrap_or_else(|| BigInt::from(9007199254740993i64));
        let calls = Cell::new(0);
        let clock = || {
            let index = calls.get();
            calls.set(index + 1);
            1700000000000.0 + 7.0 * index as f64
        };
        let context = TransactionBuildContext {
            chain_id: config.get("chainId").map(number).unwrap_or(304.0),
            account_index: &account,
            api_key_index: config.get("apiKeyIndex").map(number).unwrap_or(13.0),
            now_millis: &clock,
        };
        let expected = &row["output"];
        let expected_ok = &expected["ok"];
        let result: Result<String, TransactionBuildError> = match method {
            "buildCreateOrderData" => {
                let input = order(params);
                identity(&order_native(&input), params, "builder CreateOrderParams");
                let options = &row["options"];
                let options = CreateOrderDataBuildOptions {
                    expired_at: opt_big(options, "expiredAt"),
                    default_order_expiry: opt_big(options, "defaultOrderExpiry"),
                };
                build_create_order_data(&context, &input, &nonce, &options).map(|native| {
                    metadata(&native, expected_ok);
                    let expected_words = [
                        context.chain_id as i64,
                        14,
                        i64::try_from(&nonce).unwrap(),
                        i64::try_from(&native.expired_at).unwrap(),
                        i64::try_from(&account).unwrap(),
                        context.api_key_index as i64,
                        input.market_id as i64,
                        i64::try_from(&input.order_id).unwrap(),
                        i64::try_from(&input.amount).unwrap(),
                        i64::try_from(&input.price).unwrap(),
                        if input.is_buy { 0 } else { 1 },
                        native.order_type_value as i64,
                        native.time_in_force_value as i64,
                        if input.reduce_only == Some(true) {
                            1
                        } else {
                            0
                        },
                        i64::try_from(&native.trigger_price).unwrap(),
                        i64::try_from(&native.order_expiry).unwrap(),
                    ];
                    assert_words(&native.message_hex, &expected_words, "create order");
                    if let Some(hex) = &native.l2_attributes_message_hex {
                        assert_words(
                            hex,
                            &attributes_expected(
                                native.integrator_account_index.as_ref().unwrap(),
                                native.integrator_taker_fee.unwrap(),
                                native.integrator_maker_fee.unwrap(),
                            ),
                            "create attributes",
                        );
                    }
                    let expected_attributes = expected_ok
                        .get("l2AttributesMessageHex")
                        .filter(|v| !missing(Some(v)))
                        .map(utf8);
                    assert_eq!(
                        native.l2_attributes_message_hex.as_deref(),
                        expected_attributes.as_deref(),
                        "native optional attribution hex"
                    );
                    native.message_hex
                })
            }
            "buildCreateOrderL2AttributesData" => build_create_order_l2_attributes_data(
                &bigint(&params["integratorAccountIndex"]),
                number(&params["integratorTakerFee"]),
                number(&params["integratorMakerFee"]),
            )
            .inspect(|hex| {
                assert_words(
                    hex,
                    &attributes_expected(
                        &bigint(&params["integratorAccountIndex"]),
                        number(&params["integratorTakerFee"]),
                        number(&params["integratorMakerFee"]),
                    ),
                    "attributes",
                )
            }),
            "buildCancelOrderData" => {
                let input = CancelOrderParams {
                    order_id: bigint(&params["orderId"]),
                    market_id: number(&params["marketId"]),
                };
                build_cancel_order_data(&context, &input, &nonce).map(|native| {
                    assert_eq!(
                        native.expired_at,
                        bigint(&expected_ok["expiredAt"]),
                        "cancel native expiry"
                    );
                    assert_words(
                        &native.message_hex,
                        &[
                            304,
                            15,
                            7,
                            1700000600000,
                            9007199254740993,
                            13,
                            17,
                            i64::try_from(&input.order_id).unwrap(),
                        ],
                        "cancel order",
                    );
                    native.message_hex
                })
            }
            "buildUpdateLeverageData" => {
                let input = UpdateLeverageParams {
                    market_id: number(&params["marketId"]),
                    leverage: number(&params["leverage"]),
                    margin_mode: MarginMode::new(number(&params["marginMode"])).unwrap(),
                };
                build_update_leverage_data(&context, &input, &nonce).map(|native| {
                    assert_eq!(
                        native.expired_at,
                        bigint(&expected_ok["expiredAt"]),
                        "leverage native expiry"
                    );
                    assert_eq!(
                        native.initial_margin_fraction.to_bits(),
                        number(&expected_ok["initialMarginFraction"]).to_bits(),
                        "leverage native fraction"
                    );
                    assert_words(
                        &native.message_hex,
                        &[
                            304,
                            20,
                            7,
                            1700000600000,
                            9007199254740993,
                            13,
                            input.market_id as i64,
                            native.initial_margin_fraction as i64,
                            input.margin_mode.value() as i64,
                        ],
                        "leverage",
                    );
                    native.message_hex
                })
            }
            "buildApproveIntegratorData" => {
                let input = approve(params);
                build_approve_integrator_data(&context, &input, &nonce).map(|native| {
                    assert_eq!(
                        native.expired_at,
                        bigint(&expected_ok["expiredAt"]),
                        "approval native expiry"
                    );
                    assert_words(
                        &native.message_hex,
                        &[
                            304,
                            45,
                            7,
                            1700000600000,
                            9007199254740993,
                            13,
                            i64::try_from(&input.integrator_account_index).unwrap(),
                            input.max_perps_taker_fee as i64,
                            input.max_perps_maker_fee as i64,
                            input.max_spot_taker_fee as i64,
                            input.max_spot_maker_fee as i64,
                            i64::try_from(&input.approval_expiry).unwrap(),
                        ],
                        "approval distinct fields",
                    );
                    native.message_hex
                })
            }
            "buildTransferData" => Ok(build_transfer_data(
                &context,
                &TransferParams {
                    to_account_index: bigint(&params["toAccountIndex"]),
                    amount: bigint(&params["amount"]),
                    token_id: opt_num(params, "tokenId"),
                },
                &nonce,
            )),
            _ => panic!("unknown method"),
        };
        assert_eq!(
            calls.get(),
            row["clockCalls"].as_u64().unwrap() as usize,
            "{method} clock calls"
        );
        match result {
            Ok(hex) => {
                let source = if matches!(
                    method,
                    "buildTransferData" | "buildCreateOrderL2AttributesData"
                ) {
                    utf8(expected_ok)
                } else {
                    utf8(&expected_ok["messageHex"])
                };
                assert_eq!(
                    hex, source,
                    "{method} native returned hex string {}",
                    row["name"]
                );
            }
            Err(error) => error_identity(error, &expected["error"]),
        }
    }
    assert_eq!(
        visited,
        std::collections::BTreeMap::from([
            ("buildCreateOrderData", 311),
            ("buildCreateOrderL2AttributesData", 9),
            ("buildCancelOrderData", 4),
            ("buildUpdateLeverageData", 38),
            ("buildApproveIntegratorData", 29),
            ("buildTransferData", 19)
        ]),
        "independent executed method totals"
    );
}
#[test]
#[ignore = "actual source Node oracle"]
fn transaction_group_validator_and_info() {
    let rows = capture()["validations"].as_array().unwrap();
    assert_eq!(rows.len(), 84, "group validation inventory: 25+25+31+3");
    for row in rows {
        let input = group(&row["params"]);
        match validate_create_grouped_orders_params(&input) {
            Ok(()) => assert_eq!(row["output"]["ok"], true, "group validator native success"),
            Err(error) => error_identity(error, &row["output"]["error"]),
        }
    }
    let info = &capture()["info"];
    let input = order(&info["params"]);
    let payload = &info["payload"];
    let payload = CreateOrderData {
        message_hex: String::new(),
        l2_attributes_message_hex: None,
        expired_at: BigInt::from(0),
        order_type_value: number(&payload["orderTypeValue"]),
        time_in_force_value: number(&payload["timeInForceValue"]),
        trigger_price: bigint(&payload["triggerPrice"]),
        order_expiry: bigint(&payload["orderExpiry"]),
        integrator_account_index: None,
        integrator_maker_fee: None,
        integrator_taker_fee: None,
    };
    let native = build_grouped_order_info_data(&input, &payload).unwrap();
    assert_words(
        &native,
        &[17, 301, 2413, 2401, 1, 3, 2, 1, 2399, 1702419200067],
        "grouped order distinct fields",
    );
    assert_eq!(
        native,
        utf8(&info["result"]),
        "grouped order native returned hex"
    );
}

#[test]
#[ignore = "actual source Node precedence control"]
fn transaction_validation_precedence_control() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "transaction_group_validator_and_info",
            "--include-ignored",
            "--nocapture",
        ])
        .env("LIGHTER_TRANSACTION_BUILDERS_MUTATION", "validator-order")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("transaction builder live corpus content"),
        "{text}"
    );
}
