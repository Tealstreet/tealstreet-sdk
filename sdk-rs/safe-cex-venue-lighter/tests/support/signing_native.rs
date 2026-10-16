use num_bigint::BigInt;
use safe_cex_venue_lighter::{http_transactions::*, transaction_client_types::*};
use serde_json::{Value, json};
pub fn num(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["number"].as_str().unwrap(), 16).unwrap())
}
pub fn big(v: &Value) -> BigInt {
    v["bigint"].as_str().unwrap().parse().unwrap()
}
pub fn text(v: &Value) -> Vec<u16> {
    v["utf16"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
pub fn missing(v: Option<&Value>) -> bool {
    v.is_none_or(|v| v.get("undefined") == Some(&json!(true)))
}
pub fn opt_big(v: &Value, k: &str) -> Option<BigInt> {
    v.get(k).filter(|v| !missing(Some(v))).map(big)
}
pub fn opt_num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).filter(|v| !missing(Some(v))).map(num)
}
pub fn opt_bool(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}
pub fn n(v: f64) -> Value {
    json!({"number":format!("{:016x}",v.to_bits())})
}
pub fn b(v: &BigInt) -> Value {
    json!({"bigint":v.to_string()})
}
pub fn t(v: &[u16]) -> Value {
    json!({"utf16":v})
}
pub fn optional(v: Option<Value>) -> Value {
    v.unwrap_or_else(|| json!({"undefined":true}))
}
pub fn ob(v: &Option<BigInt>) -> Value {
    optional(v.as_ref().map(b))
}
pub fn on(v: Option<f64>) -> Value {
    optional(v.map(n))
}
pub fn boolean(v: Option<bool>) -> Value {
    optional(v.map(|v| json!(v)))
}
pub fn kind(v: &Value) -> TransactionKind {
    match String::from_utf16(&text(v)).unwrap().as_str() {
        "create_order" => TransactionKind::CreateOrder,
        "cancel_order" => TransactionKind::CancelOrder,
        "transfer" => TransactionKind::Transfer,
        "withdrawal" => TransactionKind::Withdrawal,
        "update_leverage" => TransactionKind::UpdateLeverage,
        "approve_integrator" => TransactionKind::ApproveIntegrator,
        "create_grouped_orders" => TransactionKind::CreateGroupedOrders,
        _ => panic!("unknown source literal"),
    }
}
pub fn group_kind(v: &Value) -> LighterGroupedOrderType {
    match num(v) {
        1.0 => LighterGroupedOrderType::OneTriggersTheOther,
        2.0 => LighterGroupedOrderType::OneCancelsTheOther,
        3.0 => LighterGroupedOrderType::OneTriggersOco,
        _ => panic!("unknown source literal"),
    }
}
pub fn order(v: &Value) -> CreateOrderParams {
    let order_type = v.get("orderType").filter(|v| !missing(Some(v))).map(|v| {
        match String::from_utf16(&text(v)).unwrap().as_str() {
            "limit" => CreateOrderKind::Limit,
            "market" => CreateOrderKind::Market,
            "stop_loss" => CreateOrderKind::StopLoss,
            "stop_loss_limit" => CreateOrderKind::StopLossLimit,
            "take_profit" => CreateOrderKind::TakeProfit,
            "take_profit_limit" => CreateOrderKind::TakeProfitLimit,
            _ => panic!("unknown source literal"),
        }
    });
    let time_in_force = v.get("timeInForce").filter(|v| !missing(Some(v))).map(|v| {
        match String::from_utf16(&text(v)).unwrap().as_str() {
            "GoodTillCancel" => CreateOrderTimeInForce::GoodTillCancel,
            "ImmediateOrCancel" => CreateOrderTimeInForce::ImmediateOrCancel,
            "FillOrKill" => CreateOrderTimeInForce::FillOrKill,
            "PostOnly" => CreateOrderTimeInForce::PostOnly,
            _ => panic!("unknown source literal"),
        }
    });
    CreateOrderParams {
        order_id: big(&v["orderId"]),
        market_id: num(&v["marketId"]),
        is_buy: v["isBuy"].as_bool().unwrap(),
        price: big(&v["price"]),
        amount: big(&v["amount"]),
        post_only: opt_bool(v, "postOnly"),
        reduce_only: opt_bool(v, "reduceOnly"),
        order_type,
        trigger_price: opt_big(v, "triggerPrice"),
        order_expiry: opt_big(v, "orderExpiry"),
        expired_at: opt_big(v, "expiredAt"),
        time_in_force,
        integrator_account_index: opt_big(v, "integratorAccountIndex"),
        integrator_maker_fee: opt_num(v, "integratorMakerFee"),
        integrator_taker_fee: opt_num(v, "integratorTakerFee"),
    }
}
pub fn group(v: &Value) -> CreateGroupedOrdersParams {
    CreateGroupedOrdersParams {
        grouping_type: group_kind(&v["groupingType"]),
        orders: v["orders"].as_array().unwrap().iter().map(order).collect(),
        integrator_account_index: opt_big(v, "integratorAccountIndex"),
        integrator_maker_fee: opt_num(v, "integratorMakerFee"),
        integrator_taker_fee: opt_num(v, "integratorTakerFee"),
    }
}
pub fn header(v: &Value) -> TransactionHeader {
    TransactionHeader {
        data: text(&v["data"]),
        signature: text(&v["signature"]),
        account_index: big(&v["accountIndex"]),
        api_key_index: num(&v["apiKeyIndex"]),
        nonce: big(&v["nonce"]),
        expired_at: opt_big(v, "expiredAt"),
    }
}
pub fn group_detail(v: &Value) -> LighterGroupedOrderDetails {
    LighterGroupedOrderDetails {
        order_id: big(&v["orderId"]),
        market_id: num(&v["marketId"]),
        is_buy: v["isBuy"].as_bool().unwrap(),
        price: big(&v["price"]),
        amount: big(&v["amount"]),
        reduce_only: opt_bool(v, "reduceOnly"),
        order_type_value: opt_num(v, "orderTypeValue"),
        time_in_force_value: opt_num(v, "timeInForceValue"),
        trigger_price: opt_big(v, "triggerPrice"),
        order_expiry: opt_big(v, "orderExpiry"),
    }
}
pub fn create_detail(v: &Value) -> CreateOrderDetails {
    CreateOrderDetails {
        order_id: big(&v["orderId"]),
        market_id: num(&v["marketId"]),
        is_buy: v["isBuy"].as_bool().unwrap(),
        price: big(&v["price"]),
        amount: big(&v["amount"]),
        post_only: opt_bool(v, "postOnly"),
        reduce_only: opt_bool(v, "reduceOnly"),
        order_type_value: opt_num(v, "orderTypeValue"),
        time_in_force_value: opt_num(v, "timeInForceValue"),
        trigger_price: opt_big(v, "triggerPrice"),
        order_expiry: opt_big(v, "orderExpiry"),
        integrator_account_index: opt_big(v, "integratorAccountIndex"),
        integrator_maker_fee: opt_num(v, "integratorMakerFee"),
        integrator_taker_fee: opt_num(v, "integratorTakerFee"),
    }
}
pub fn transaction(v: &Value) -> AnyTxInfo {
    let header = header(v);
    match kind(&v["type"]) {
        TransactionKind::CreateOrder => AnyTxInfo::CreateOrder(L2CreateOrderTxInfo {
            header,
            order_details: create_detail(&v["orderDetails"]),
        }),
        TransactionKind::CreateGroupedOrders => {
            let d = &v["groupedOrderDetails"];
            AnyTxInfo::CreateGroupedOrders(L2CreateGroupedOrdersTxInfo {
                header,
                grouped_order_details: GroupedOrderDetails {
                    grouping_type: group_kind(&d["groupingType"]),
                    orders: d["orders"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(group_detail)
                        .collect(),
                    integrator_account_index: opt_big(d, "integratorAccountIndex"),
                    integrator_maker_fee: opt_num(d, "integratorMakerFee"),
                    integrator_taker_fee: opt_num(d, "integratorTakerFee"),
                },
            })
        }
        TransactionKind::CancelOrder => {
            let d = &v["cancelDetails"];
            AnyTxInfo::CancelOrder(L2CancelOrderTxInfo {
                header,
                cancel_details: CancelDetails {
                    order_id: big(&d["orderId"]),
                    market_id: num(&d["marketId"]),
                    expired_at: opt_big(d, "expiredAt"),
                },
            })
        }
        TransactionKind::Transfer => {
            let d = &v["transferDetails"];
            AnyTxInfo::Transfer(L2TransferTxInfo {
                header,
                transfer_details: TransferDetails {
                    to_account_index: big(&d["toAccountIndex"]),
                    amount: big(&d["amount"]),
                    token_id: num(&d["tokenId"]),
                },
            })
        }
        TransactionKind::UpdateLeverage => {
            let d = &v["leverageDetails"];
            AnyTxInfo::UpdateLeverage(L2UpdateLeverageTxInfo {
                header,
                leverage_details: LeverageDetails {
                    market_id: num(&d["marketId"]),
                    initial_margin_fraction: num(&d["initialMarginFraction"]),
                    margin_mode: MarginMode::new(num(&d["marginMode"])).unwrap(),
                    expired_at: opt_big(d, "expiredAt"),
                },
            })
        }
        TransactionKind::ApproveIntegrator => {
            let d = &v["approveIntegratorDetails"];
            AnyTxInfo::ApproveIntegrator(L2ApproveIntegratorTxInfo {
                header,
                approve_integrator_details: ApproveIntegratorDetails {
                    integrator_account_index: big(&d["integratorAccountIndex"]),
                    max_perps_taker_fee: num(&d["maxPerpsTakerFee"]),
                    max_perps_maker_fee: num(&d["maxPerpsMakerFee"]),
                    max_spot_taker_fee: num(&d["maxSpotTakerFee"]),
                    max_spot_maker_fee: num(&d["maxSpotMakerFee"]),
                    approval_expiry: big(&d["approvalExpiry"]),
                    expired_at: opt_big(d, "expiredAt"),
                    l1_signature: d.get("l1Signature").filter(|v| !missing(Some(v))).map(text),
                },
            })
        }
        TransactionKind::Withdrawal => AnyTxInfo::Withdrawal(L2WithdrawalTxInfo { header }),
    }
}
// Native projections read every field; no native struct Serialize is involved.
pub fn header_native(kind: TransactionKind, h: &TransactionHeader) -> Value {
    json!({"type":t(&kind.as_str().encode_utf16().collect::<Vec<_>>()),"data":t(&h.data),"signature":t(&h.signature),"accountIndex":b(&h.account_index),"apiKeyIndex":n(h.api_key_index),"nonce":b(&h.nonce),"expiredAt":ob(&h.expired_at)})
}
pub fn grouped_native(v: &LighterGroupedOrderDetails) -> Value {
    json!({"orderId":b(&v.order_id),"marketId":n(v.market_id),"isBuy":v.is_buy,"price":b(&v.price),"amount":b(&v.amount),"reduceOnly":boolean(v.reduce_only),"orderTypeValue":on(v.order_type_value),"timeInForceValue":on(v.time_in_force_value),"triggerPrice":ob(&v.trigger_price),"orderExpiry":ob(&v.order_expiry)})
}
pub fn create_native(v: &CreateOrderDetails) -> Value {
    json!({"orderId":b(&v.order_id),"marketId":n(v.market_id),"isBuy":v.is_buy,"price":b(&v.price),"amount":b(&v.amount),"postOnly":boolean(v.post_only),"reduceOnly":boolean(v.reduce_only),"orderTypeValue":on(v.order_type_value),"timeInForceValue":on(v.time_in_force_value),"triggerPrice":ob(&v.trigger_price),"orderExpiry":ob(&v.order_expiry),"integratorAccountIndex":ob(&v.integrator_account_index),"integratorMakerFee":on(v.integrator_maker_fee),"integratorTakerFee":on(v.integrator_taker_fee)})
}
pub fn tx_native(v: &AnyTxInfo) -> Value {
    let mut root = header_native(v.kind(), v.header());
    match v {
        AnyTxInfo::CreateOrder(v) => root["orderDetails"] = create_native(&v.order_details),
        AnyTxInfo::CreateGroupedOrders(v) => {
            let d = &v.grouped_order_details;
            root["groupedOrderDetails"] = json!({"groupingType":n(d.grouping_type.value()),"orders":d.orders.iter().map(grouped_native).collect::<Vec<_>>(),"integratorAccountIndex":ob(&d.integrator_account_index),"integratorMakerFee":on(d.integrator_maker_fee),"integratorTakerFee":on(d.integrator_taker_fee)});
        }
        AnyTxInfo::CancelOrder(v) => {
            let d = &v.cancel_details;
            root["cancelDetails"] = json!({"orderId":b(&d.order_id),"marketId":n(d.market_id),"expiredAt":ob(&d.expired_at)});
        }
        AnyTxInfo::Transfer(v) => {
            let d = &v.transfer_details;
            root["transferDetails"] = json!({"toAccountIndex":b(&d.to_account_index),"amount":b(&d.amount),"tokenId":n(d.token_id)});
        }
        AnyTxInfo::UpdateLeverage(v) => {
            let d = &v.leverage_details;
            root["leverageDetails"] = json!({"marketId":n(d.market_id),"initialMarginFraction":n(d.initial_margin_fraction),"marginMode":n(d.margin_mode.value()),"expiredAt":ob(&d.expired_at)});
        }
        AnyTxInfo::ApproveIntegrator(v) => {
            let d = &v.approve_integrator_details;
            root["approveIntegratorDetails"] = json!({"integratorAccountIndex":b(&d.integrator_account_index),"maxPerpsTakerFee":n(d.max_perps_taker_fee),"maxPerpsMakerFee":n(d.max_perps_maker_fee),"maxSpotTakerFee":n(d.max_spot_taker_fee),"maxSpotMakerFee":n(d.max_spot_maker_fee),"approvalExpiry":b(&d.approval_expiry),"expiredAt":ob(&d.expired_at),"l1Signature":optional(d.l1_signature.as_ref().map(|v|t(v)))});
        }
        AnyTxInfo::Withdrawal(_) => {}
    }
    root
}
pub fn identity(native: &Value, source: &Value, label: &str) {
    if let Some(fields) = native.as_object() {
        if fields.contains_key("undefined") {
            assert!(missing(Some(source)), "{label} optional native absence");
            return;
        }
        if fields.contains_key("utf16")
            || fields.contains_key("number")
            || fields.contains_key("bigint")
        {
            assert_eq!(native, source, "{label} native leaf identity");
            return;
        }
        for (k, v) in fields {
            identity(
                v,
                source.get(k).unwrap_or(&json!({"undefined":true})),
                &format!("{label}.{k}"),
            );
        }
    } else if let Some(items) = native.as_array() {
        let source = source.as_array().unwrap();
        assert_eq!(
            items.len(),
            source.len(),
            "{label} native source array identity"
        );
        for (i, v) in items.iter().enumerate() {
            identity(v, &source[i], &format!("{label}[{i}]"));
        }
    } else {
        assert_eq!(native, source, "{label} native scalar identity");
    }
}
