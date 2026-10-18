use safe_cex_venue_lighter::sdk_orders::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;
fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/sdk-orders-types-oracle.cjs"
    )
}
macro_rules! field_type {
    ($field:expr, $expected:ty, $name:literal) => {
        assert_eq!(
            std::any::type_name_of_val(&$field),
            std::any::type_name::<$expected>(),
            concat!($name, " public field type")
        );
    };
}
trait NativeIdentity {
    fn native_identity(&self) -> Value;
}
impl NativeIdentity for CancelOrderParams {
    fn native_identity(&self) -> Value {
        json!({"orderId": self.order_id,"clientOrderId": self.client_order_id,"symbol": self.symbol,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index})
    }
}
impl NativeIdentity for CancelAllOrdersParams {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index})
    }
}
impl NativeIdentity for QueryFillsParams {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"orderId": self.order_id,"startTime": self.start_time,"endTime": self.end_time,"page": self.page,"pageSize": self.page_size})
    }
}
impl NativeIdentity for OrderStats {
    fn native_identity(&self) -> Value {
        json!({"totalOrders": self.total_orders,"activeOrders": self.active_orders,"filledOrders": self.filled_orders,"cancelledOrders": self.cancelled_orders,"totalVolume": self.total_volume,"totalFees": self.total_fees,"averageFillRate": self.average_fill_rate})
    }
}
impl NativeIdentity for OrderInfo {
    fn native_identity(&self) -> Value {
        json!({"orderId": self.order_id,"clientOrderId": self.client_order_id,"marketId": self.market_id,"symbol": self.symbol,"account": self.account,"type": self.r#type,"side": self.side,"price": self.price,"quantity": self.quantity,"filledQuantity": self.filled_quantity,"remainingQuantity": self.remaining_quantity,"averagePrice": self.average_price,"status": self.status,"timeInForce": self.time_in_force,"expiry": self.expiry,"stopPrice": self.stop_price,"triggerStatus": self.trigger_status,"reduceOnly": self.reduce_only,"postOnly": self.post_only,"fee": self.fee,"feeToken": self.fee_token,"createdAt": self.created_at,"updatedAt": self.updated_at,"txHash": self.tx_hash,"nonce": self.nonce,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index})
    }
}
impl NativeIdentity for Fill {
    fn native_identity(&self) -> Value {
        json!({"fillId": self.fill_id,"orderId": self.order_id,"tradeId": self.trade_id,"symbol": self.symbol,"price": self.price,"quantity": self.quantity,"quoteQuantity": self.quote_quantity,"fee": self.fee,"feeToken": self.fee_token,"isMaker": self.is_maker,"side": self.side,"timestamp": self.timestamp})
    }
}
impl NativeIdentity for CreateLimitOrderParams {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"side": self.side,"price": self.price,"quantity": self.quantity,"timeInForce": self.time_in_force,"postOnly": self.post_only,"reduceOnly": self.reduce_only,"clientOrderId": self.client_order_id,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index,"expiry": self.expiry})
    }
}
impl NativeIdentity for CreateMarketOrderParams {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"side": self.side,"quantity": self.quantity,"reduceOnly": self.reduce_only,"clientOrderId": self.client_order_id,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index})
    }
}
impl NativeIdentity for CreateStopLossOrderParams {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"side": self.side,"stopPrice": self.stop_price,"quantity": self.quantity,"price": self.price,"reduceOnly": self.reduce_only,"clientOrderId": self.client_order_id,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index,"expiry": self.expiry})
    }
}
impl NativeIdentity for CreateTakeProfitOrderParams {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"side": self.side,"takeProfitPrice": self.take_profit_price,"quantity": self.quantity,"price": self.price,"reduceOnly": self.reduce_only,"clientOrderId": self.client_order_id,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index,"expiry": self.expiry})
    }
}
impl NativeIdentity for QueryOrdersParams {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"status": self.status,"side": self.side,"type": self.r#type,"startTime": self.start_time,"endTime": self.end_time,"page": self.page,"pageSize": self.page_size})
    }
}
impl NativeIdentity for ModifyOrderParams {
    fn native_identity(&self) -> Value {
        json!({"orderId": self.order_id,"price": self.price,"quantity": self.quantity,"expiry": self.expiry,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index})
    }
}
impl NativeIdentity for BatchCancelOrdersParams {
    fn native_identity(&self) -> Value {
        json!({"orderIds": self.order_ids,"clientOrderIds": self.client_order_ids,"symbol": self.symbol,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index})
    }
}
fn numeric_kind(kind: &str) -> bool {
    matches!(
        kind,
        "number" | "OrderType" | "OrderSide" | "OrderStatus" | "TimeInForce" | "TriggerStatus"
    )
}
fn check<T: DeserializeOwned + Serialize + NativeIdentity>(
    oracle: &Value,
    name: &str,
    expected: Value,
    number: impl Fn(&T, &str) -> f64,
) {
    assert_eq!(oracle["schemas"][name], expected, "{name} source schema");
    let complete = &oracle["complete"][name];
    let parsed = T::deserialize(complete.clone()).unwrap();
    assert_eq!(
        parsed.native_identity(),
        *complete,
        "{name} native field identity"
    );
    for field in expected.as_array().unwrap() {
        let key = field["name"].as_str().unwrap();
        let values = match field["type"].as_str().unwrap() {
            "string" => vec![json!(format!("identity:{name}.{key}"))],
            "boolean" => vec![json!(false), json!(true)],
            "string[]" => vec![json!([
                format!("identity:{key}:first"),
                format!("identity:{key}:second")
            ])],
            kind if numeric_kind(kind) => vec![json!(876543.25)],
            kind => panic!("unclassified identity field {name}.{key}: {kind}"),
        };
        for value in values {
            let mut input = complete.clone();
            input[key] = value;
            let decoded = T::deserialize(input.clone()).unwrap();
            assert_eq!(
                decoded.native_identity(),
                input,
                "{name}.{key} native field identity"
            );
        }
    }
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        *complete,
        "{name} complete roundtrip"
    );
    if expected
        .as_array()
        .unwrap()
        .iter()
        .all(|f| f["optional"] == true)
    {
        let empty = T::deserialize(json!({})).unwrap();
        assert_eq!(
            serde_json::to_value(empty).unwrap(),
            json!({}),
            "{name} all-optional empty record"
        );
    }
    for field in expected.as_array().unwrap() {
        let key = field["name"].as_str().unwrap();
        let mut value = complete.clone();
        value.as_object_mut().unwrap().shift_remove(key);
        assert_eq!(
            T::deserialize(value).is_ok(),
            field["optional"].as_bool().unwrap(),
            "{name}.{key} required/optional"
        );
        if field["optional"].as_bool().unwrap() {
            let mut absent = complete.clone();
            absent.as_object_mut().unwrap().shift_remove(key);
            let decoded = T::deserialize(absent.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                absent,
                "{name}.{key} absent key remains absent"
            );
        }
        let kind = field["type"].as_str().unwrap();
        let mut invalid = vec![json!(null), json!({})];
        if kind != "string[]" {
            invalid.push(json!([]));
        }
        if kind != "boolean" {
            invalid.push(json!(false));
        }
        if !numeric_kind(kind) {
            invalid.push(json!(1));
        }
        if kind != "string" {
            invalid.push(json!("1"));
        }
        for bad in invalid {
            let mut value = complete.clone();
            value[key] = bad;
            assert!(
                T::deserialize(value).is_err(),
                "{name}.{key} rejects null/wrong type"
            );
        }
    }
    for field in expected
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["type"] == "string[]")
    {
        let key = field["name"].as_str().unwrap();
        let mut empty = complete.clone();
        empty[key] = json!([]);
        assert_eq!(
            serde_json::to_value(T::deserialize(empty.clone()).unwrap()).unwrap(),
            empty,
            "{name}.{key} empty array retained"
        );
        for bad in [json!(null), json!({}), json!([]), json!(1), json!(false)] {
            let mut value = complete.clone();
            value[key] = json!([bad]);
            assert!(
                T::deserialize(value).is_err(),
                "{name}.{key} rejects non-string array element"
            );
        }
    }
    for field in expected
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["type"] == "boolean")
    {
        for flag in [false, true] {
            let mut value = complete.clone();
            value[field["name"].as_str().unwrap()] = json!(flag);
            let decoded = T::deserialize(value.clone()).unwrap();
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                value,
                "{name}.{} boolean roundtrip",
                field["name"]
            );
        }
    }
    for bad in [
        json!(null),
        json!(false),
        json!(1),
        json!("record"),
        json!([]),
        Value::Array(complete.as_object().unwrap().values().cloned().collect()),
    ] {
        assert!(T::deserialize(bad).is_err(), "{name} object-only shape");
    }
    let witnesses = oracle["numeric"][name].as_array().unwrap();
    let expected_inventory: Vec<_> = expected
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| numeric_kind(f["type"].as_str().unwrap()))
        .flat_map(|f| {
            [
                "-0",
                "-0.0",
                "-0e0",
                "0",
                "1e3",
                "1e400",
                "-1e400",
                "16777217.25",
            ]
            .map(|raw| json!([f["name"], raw]))
        })
        .collect();
    assert_eq!(
        witnesses
            .iter()
            .map(|w| json!([w["field"], w["raw"]]))
            .collect::<Vec<_>>(),
        expected_inventory,
        "{name} numeric witness inventory"
    );
    for w in witnesses {
        let key = w["field"].as_str().unwrap();
        let input: Value = serde_json::from_str(w["encoded"].as_str().unwrap()).unwrap();
        let bits = w["bits"].as_str().unwrap();
        if w["raw"] == "-0" {
            assert_eq!(
                bits, "8000000000000000",
                "{name}.{key} negative source bits"
            );
        }
        for (path, parsed) in [
            ("owned", T::deserialize(input.clone()).unwrap()),
            ("borrowed", T::deserialize(&input).unwrap()),
        ] {
            assert_eq!(
                format!("{:016x}", number(&parsed, key).to_bits()),
                bits,
                "{name}.{key} {path} numeric bits"
            );
        }
        let mut native = complete.clone();
        native[key] = json!(-0.0);
        assert_eq!(
            number(&T::deserialize(native).unwrap(), key).to_bits(),
            (-0.0_f64).to_bits(),
            "{name}.{key} native signed zero"
        );
    }
    let strings = oracle["strings"][name].as_array().unwrap();
    assert_eq!(
        json!(
            strings
                .iter()
                .map(|w| w["units"].clone())
                .collect::<Vec<_>>()
        ),
        json!([
            [],
            [112, 108, 97, 105, 110],
            [233],
            [0xd83d, 0xdc69],
            [0xfeff],
            [0x85],
            [0xd800],
            [0xdfff],
            [120, 0xd800, 121]
        ]),
        "{name} UTF16 inventory"
    );
    for w in strings {
        let result = serde_json::from_str::<Value>(w["encoded"].as_str().unwrap())
            .and_then(serde_json::from_value::<T>);
        if w["representable"] == true {
            let value = serde_json::to_value(result.unwrap()).unwrap();
            for f in expected
                .as_array()
                .unwrap()
                .iter()
                .filter(|f| f["type"] == "string" || f["type"] == "string[]")
            {
                let field = &value[f["name"].as_str().unwrap()];
                let values = if f["type"] == "string[]" {
                    field.as_array().unwrap().iter().collect::<Vec<_>>()
                } else {
                    vec![field]
                };
                for item in values {
                    assert_eq!(
                        json!(item.as_str().unwrap().encode_utf16().collect::<Vec<_>>()),
                        w["units"],
                        "{name} UTF16 retention"
                    );
                }
            }
        } else {
            assert!(result.is_err(), "{name} upstream Value refusal");
            let _: T = typed_utf16::decode(w["encoded"].as_str().unwrap());
        }
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_orders_records_match_live_typescript() {
    let output = Command::new("node").arg(script()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    check::<CancelOrderParams>(
        &oracle,
        "CancelOrderParams",
        json!([{"name": "orderId", "type": "string", "optional": true}, {"name": "clientOrderId", "type": "string", "optional": true}, {"name": "symbol", "type": "string", "optional": false}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}]),
        |value, key| match key {
            "accountIndex" => value.account_index.unwrap(),
            "apiKeyIndex" => value.api_key_index.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        CancelOrderParams::deserialize(Value::Array(
            oracle["complete"]["CancelOrderParams"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "CancelOrderParams public qualified shape"
    );
    check::<CancelAllOrdersParams>(
        &oracle,
        "CancelAllOrdersParams",
        json!([{"name": "symbol", "type": "string", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}]),
        |value, key| match key {
            "accountIndex" => value.account_index.unwrap(),
            "apiKeyIndex" => value.api_key_index.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        CancelAllOrdersParams::deserialize(Value::Array(
            oracle["complete"]["CancelAllOrdersParams"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "CancelAllOrdersParams public qualified shape"
    );
    check::<QueryFillsParams>(
        &oracle,
        "QueryFillsParams",
        json!([{"name": "symbol", "type": "string", "optional": true}, {"name": "orderId", "type": "string", "optional": true}, {"name": "startTime", "type": "number", "optional": true}, {"name": "endTime", "type": "number", "optional": true}, {"name": "page", "type": "number", "optional": true}, {"name": "pageSize", "type": "number", "optional": true}]),
        |value, key| match key {
            "startTime" => value.start_time.unwrap(),
            "endTime" => value.end_time.unwrap(),
            "page" => value.page.unwrap(),
            "pageSize" => value.page_size.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        QueryFillsParams::deserialize(Value::Array(
            oracle["complete"]["QueryFillsParams"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "QueryFillsParams public qualified shape"
    );
    check::<OrderStats>(
        &oracle,
        "OrderStats",
        json!([{"name": "totalOrders", "type": "number", "optional": false}, {"name": "activeOrders", "type": "number", "optional": false}, {"name": "filledOrders", "type": "number", "optional": false}, {"name": "cancelledOrders", "type": "number", "optional": false}, {"name": "totalVolume", "type": "string", "optional": false}, {"name": "totalFees", "type": "string", "optional": false}, {"name": "averageFillRate", "type": "number", "optional": false}]),
        |value, key| match key {
            "totalOrders" => value.total_orders,
            "activeOrders" => value.active_orders,
            "filledOrders" => value.filled_orders,
            "cancelledOrders" => value.cancelled_orders,
            "averageFillRate" => value.average_fill_rate,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        OrderStats::deserialize(Value::Array(
            oracle["complete"]["OrderStats"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "OrderStats public qualified shape"
    );
    check::<OrderInfo>(
        &oracle,
        "OrderInfo",
        json!([{"name": "orderId", "type": "string", "optional": false}, {"name": "clientOrderId", "type": "string", "optional": true}, {"name": "marketId", "type": "string", "optional": false}, {"name": "symbol", "type": "string", "optional": false}, {"name": "account", "type": "string", "optional": false}, {"name": "type", "type": "OrderType", "optional": false}, {"name": "side", "type": "OrderSide", "optional": false}, {"name": "price", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "filledQuantity", "type": "string", "optional": false}, {"name": "remainingQuantity", "type": "string", "optional": false}, {"name": "averagePrice", "type": "string", "optional": false}, {"name": "status", "type": "OrderStatus", "optional": false}, {"name": "timeInForce", "type": "TimeInForce", "optional": false}, {"name": "expiry", "type": "number", "optional": false}, {"name": "stopPrice", "type": "string", "optional": true}, {"name": "triggerStatus", "type": "TriggerStatus", "optional": true}, {"name": "reduceOnly", "type": "boolean", "optional": false}, {"name": "postOnly", "type": "boolean", "optional": false}, {"name": "fee", "type": "string", "optional": false}, {"name": "feeToken", "type": "string", "optional": false}, {"name": "createdAt", "type": "number", "optional": false}, {"name": "updatedAt", "type": "number", "optional": false}, {"name": "txHash", "type": "string", "optional": true}, {"name": "nonce", "type": "number", "optional": false}, {"name": "accountIndex", "type": "number", "optional": false}, {"name": "apiKeyIndex", "type": "number", "optional": false}]),
        |value, key| match key {
            "type" => value.r#type.0,
            "side" => value.side.0,
            "status" => value.status.0,
            "timeInForce" => value.time_in_force.0,
            "expiry" => value.expiry,
            "triggerStatus" => value.trigger_status.unwrap().0,
            "createdAt" => value.created_at,
            "updatedAt" => value.updated_at,
            "nonce" => value.nonce,
            "accountIndex" => value.account_index,
            "apiKeyIndex" => value.api_key_index,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        OrderInfo::deserialize(Value::Array(
            oracle["complete"]["OrderInfo"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "OrderInfo public qualified shape"
    );
    check::<Fill>(
        &oracle,
        "Fill",
        json!([{"name": "fillId", "type": "string", "optional": false}, {"name": "orderId", "type": "string", "optional": false}, {"name": "tradeId", "type": "string", "optional": false}, {"name": "symbol", "type": "string", "optional": false}, {"name": "price", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "quoteQuantity", "type": "string", "optional": false}, {"name": "fee", "type": "string", "optional": false}, {"name": "feeToken", "type": "string", "optional": false}, {"name": "isMaker", "type": "boolean", "optional": false}, {"name": "side", "type": "OrderSide", "optional": false}, {"name": "timestamp", "type": "number", "optional": false}]),
        |value, key| match key {
            "side" => value.side.0,
            "timestamp" => value.timestamp,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        Fill::deserialize(Value::Array(
            oracle["complete"]["Fill"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "Fill public qualified shape"
    );
    let typed: OrderInfo = serde_json::from_value(oracle["complete"]["OrderInfo"].clone()).unwrap();
    field_type!(typed.order_id, String, "OrderInfo.orderId");
    field_type!(
        typed.client_order_id,
        Option<String>,
        "OrderInfo.clientOrderId"
    );
    field_type!(typed.market_id, String, "OrderInfo.marketId");
    field_type!(typed.symbol, String, "OrderInfo.symbol");
    field_type!(typed.account, String, "OrderInfo.account");
    field_type!(
        typed.r#type,
        safe_cex_venue_lighter::sdk_protocol_types::OrderType,
        "OrderInfo.type"
    );
    field_type!(
        typed.side,
        safe_cex_venue_lighter::sdk_protocol_types::OrderSide,
        "OrderInfo.side"
    );
    field_type!(typed.price, String, "OrderInfo.price");
    field_type!(typed.quantity, String, "OrderInfo.quantity");
    field_type!(typed.filled_quantity, String, "OrderInfo.filledQuantity");
    field_type!(
        typed.remaining_quantity,
        String,
        "OrderInfo.remainingQuantity"
    );
    field_type!(typed.average_price, String, "OrderInfo.averagePrice");
    field_type!(
        typed.status,
        safe_cex_venue_lighter::sdk_protocol_types::OrderStatus,
        "OrderInfo.status"
    );
    field_type!(
        typed.time_in_force,
        safe_cex_venue_lighter::sdk_protocol_types::TimeInForce,
        "OrderInfo.timeInForce"
    );
    field_type!(typed.expiry, f64, "OrderInfo.expiry");
    field_type!(typed.stop_price, Option<String>, "OrderInfo.stopPrice");
    field_type!(
        typed.trigger_status,
        Option<safe_cex_venue_lighter::sdk_protocol_types::TriggerStatus>,
        "OrderInfo.triggerStatus"
    );
    field_type!(typed.reduce_only, bool, "OrderInfo.reduceOnly");
    field_type!(typed.post_only, bool, "OrderInfo.postOnly");
    field_type!(typed.fee, String, "OrderInfo.fee");
    field_type!(typed.fee_token, String, "OrderInfo.feeToken");
    field_type!(typed.created_at, f64, "OrderInfo.createdAt");
    field_type!(typed.updated_at, f64, "OrderInfo.updatedAt");
    field_type!(typed.tx_hash, Option<String>, "OrderInfo.txHash");
    field_type!(typed.nonce, f64, "OrderInfo.nonce");
    field_type!(typed.account_index, f64, "OrderInfo.accountIndex");
    field_type!(typed.api_key_index, f64, "OrderInfo.apiKeyIndex");
    let typed: Fill = serde_json::from_value(oracle["complete"]["Fill"].clone()).unwrap();
    field_type!(typed.fill_id, String, "Fill.fillId");
    field_type!(typed.order_id, String, "Fill.orderId");
    field_type!(typed.trade_id, String, "Fill.tradeId");
    field_type!(typed.symbol, String, "Fill.symbol");
    field_type!(typed.price, String, "Fill.price");
    field_type!(typed.quantity, String, "Fill.quantity");
    field_type!(typed.quote_quantity, String, "Fill.quoteQuantity");
    field_type!(typed.fee, String, "Fill.fee");
    field_type!(typed.fee_token, String, "Fill.feeToken");
    field_type!(typed.is_maker, bool, "Fill.isMaker");
    field_type!(
        typed.side,
        safe_cex_venue_lighter::sdk_protocol_types::OrderSide,
        "Fill.side"
    );
    field_type!(typed.timestamp, f64, "Fill.timestamp");
    check::<CreateLimitOrderParams>(
        &oracle,
        "CreateLimitOrderParams",
        json!([{"name": "symbol", "type": "string", "optional": false}, {"name": "side", "type": "OrderSide", "optional": false}, {"name": "price", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "timeInForce", "type": "TimeInForce", "optional": true}, {"name": "postOnly", "type": "boolean", "optional": true}, {"name": "reduceOnly", "type": "boolean", "optional": true}, {"name": "clientOrderId", "type": "string", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}, {"name": "expiry", "type": "number", "optional": true}]),
        |value, key| match key {
            "side" => value.side.0,
            "timeInForce" => value.time_in_force.unwrap().0,
            "accountIndex" => value.account_index.unwrap(),
            "apiKeyIndex" => value.api_key_index.unwrap(),
            "expiry" => value.expiry.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        CreateLimitOrderParams::deserialize(Value::Array(
            oracle["complete"]["CreateLimitOrderParams"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "CreateLimitOrderParams public qualified shape"
    );
    let typed: CreateLimitOrderParams =
        serde_json::from_value(oracle["complete"]["CreateLimitOrderParams"].clone()).unwrap();
    field_type!(typed.symbol, String, "CreateLimitOrderParams.symbol");
    field_type!(
        typed.side,
        safe_cex_venue_lighter::sdk_protocol_types::OrderSide,
        "CreateLimitOrderParams.side"
    );
    field_type!(typed.price, String, "CreateLimitOrderParams.price");
    field_type!(typed.quantity, String, "CreateLimitOrderParams.quantity");
    field_type!(
        typed.time_in_force,
        Option<safe_cex_venue_lighter::sdk_protocol_types::TimeInForce>,
        "CreateLimitOrderParams.timeInForce"
    );
    field_type!(
        typed.post_only,
        Option<bool>,
        "CreateLimitOrderParams.postOnly"
    );
    field_type!(
        typed.reduce_only,
        Option<bool>,
        "CreateLimitOrderParams.reduceOnly"
    );
    field_type!(
        typed.client_order_id,
        Option<String>,
        "CreateLimitOrderParams.clientOrderId"
    );
    field_type!(
        typed.account_index,
        Option<f64>,
        "CreateLimitOrderParams.accountIndex"
    );
    field_type!(
        typed.api_key_index,
        Option<f64>,
        "CreateLimitOrderParams.apiKeyIndex"
    );
    field_type!(typed.expiry, Option<f64>, "CreateLimitOrderParams.expiry");
    check::<CreateMarketOrderParams>(
        &oracle,
        "CreateMarketOrderParams",
        json!([{"name": "symbol", "type": "string", "optional": false}, {"name": "side", "type": "OrderSide", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "reduceOnly", "type": "boolean", "optional": true}, {"name": "clientOrderId", "type": "string", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}]),
        |value, key| match key {
            "side" => value.side.0,
            "accountIndex" => value.account_index.unwrap(),
            "apiKeyIndex" => value.api_key_index.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        CreateMarketOrderParams::deserialize(Value::Array(
            oracle["complete"]["CreateMarketOrderParams"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "CreateMarketOrderParams public qualified shape"
    );
    let typed: CreateMarketOrderParams =
        serde_json::from_value(oracle["complete"]["CreateMarketOrderParams"].clone()).unwrap();
    field_type!(typed.symbol, String, "CreateMarketOrderParams.symbol");
    field_type!(
        typed.side,
        safe_cex_venue_lighter::sdk_protocol_types::OrderSide,
        "CreateMarketOrderParams.side"
    );
    field_type!(typed.quantity, String, "CreateMarketOrderParams.quantity");
    field_type!(
        typed.reduce_only,
        Option<bool>,
        "CreateMarketOrderParams.reduceOnly"
    );
    field_type!(
        typed.client_order_id,
        Option<String>,
        "CreateMarketOrderParams.clientOrderId"
    );
    field_type!(
        typed.account_index,
        Option<f64>,
        "CreateMarketOrderParams.accountIndex"
    );
    field_type!(
        typed.api_key_index,
        Option<f64>,
        "CreateMarketOrderParams.apiKeyIndex"
    );
    check::<CreateStopLossOrderParams>(
        &oracle,
        "CreateStopLossOrderParams",
        json!([{"name": "symbol", "type": "string", "optional": false}, {"name": "side", "type": "OrderSide", "optional": false}, {"name": "stopPrice", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "price", "type": "string", "optional": true}, {"name": "reduceOnly", "type": "boolean", "optional": true}, {"name": "clientOrderId", "type": "string", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}, {"name": "expiry", "type": "number", "optional": true}]),
        |value, key| match key {
            "side" => value.side.0,
            "accountIndex" => value.account_index.unwrap(),
            "apiKeyIndex" => value.api_key_index.unwrap(),
            "expiry" => value.expiry.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        CreateStopLossOrderParams::deserialize(Value::Array(
            oracle["complete"]["CreateStopLossOrderParams"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "CreateStopLossOrderParams public qualified shape"
    );
    let typed: CreateStopLossOrderParams =
        serde_json::from_value(oracle["complete"]["CreateStopLossOrderParams"].clone()).unwrap();
    field_type!(typed.symbol, String, "CreateStopLossOrderParams.symbol");
    field_type!(
        typed.side,
        safe_cex_venue_lighter::sdk_protocol_types::OrderSide,
        "CreateStopLossOrderParams.side"
    );
    field_type!(
        typed.stop_price,
        String,
        "CreateStopLossOrderParams.stopPrice"
    );
    field_type!(typed.quantity, String, "CreateStopLossOrderParams.quantity");
    field_type!(
        typed.price,
        Option<String>,
        "CreateStopLossOrderParams.price"
    );
    field_type!(
        typed.reduce_only,
        Option<bool>,
        "CreateStopLossOrderParams.reduceOnly"
    );
    field_type!(
        typed.client_order_id,
        Option<String>,
        "CreateStopLossOrderParams.clientOrderId"
    );
    field_type!(
        typed.account_index,
        Option<f64>,
        "CreateStopLossOrderParams.accountIndex"
    );
    field_type!(
        typed.api_key_index,
        Option<f64>,
        "CreateStopLossOrderParams.apiKeyIndex"
    );
    field_type!(
        typed.expiry,
        Option<f64>,
        "CreateStopLossOrderParams.expiry"
    );
    check::<CreateTakeProfitOrderParams>(
        &oracle,
        "CreateTakeProfitOrderParams",
        json!([{"name": "symbol", "type": "string", "optional": false}, {"name": "side", "type": "OrderSide", "optional": false}, {"name": "takeProfitPrice", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "price", "type": "string", "optional": true}, {"name": "reduceOnly", "type": "boolean", "optional": true}, {"name": "clientOrderId", "type": "string", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}, {"name": "expiry", "type": "number", "optional": true}]),
        |value, key| match key {
            "side" => value.side.0,
            "accountIndex" => value.account_index.unwrap(),
            "apiKeyIndex" => value.api_key_index.unwrap(),
            "expiry" => value.expiry.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        CreateTakeProfitOrderParams::deserialize(Value::Array(
            oracle["complete"]["CreateTakeProfitOrderParams"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "CreateTakeProfitOrderParams public qualified shape"
    );
    let typed: CreateTakeProfitOrderParams =
        serde_json::from_value(oracle["complete"]["CreateTakeProfitOrderParams"].clone()).unwrap();
    field_type!(typed.symbol, String, "CreateTakeProfitOrderParams.symbol");
    field_type!(
        typed.side,
        safe_cex_venue_lighter::sdk_protocol_types::OrderSide,
        "CreateTakeProfitOrderParams.side"
    );
    field_type!(
        typed.take_profit_price,
        String,
        "CreateTakeProfitOrderParams.takeProfitPrice"
    );
    field_type!(
        typed.quantity,
        String,
        "CreateTakeProfitOrderParams.quantity"
    );
    field_type!(
        typed.price,
        Option<String>,
        "CreateTakeProfitOrderParams.price"
    );
    field_type!(
        typed.reduce_only,
        Option<bool>,
        "CreateTakeProfitOrderParams.reduceOnly"
    );
    field_type!(
        typed.client_order_id,
        Option<String>,
        "CreateTakeProfitOrderParams.clientOrderId"
    );
    field_type!(
        typed.account_index,
        Option<f64>,
        "CreateTakeProfitOrderParams.accountIndex"
    );
    field_type!(
        typed.api_key_index,
        Option<f64>,
        "CreateTakeProfitOrderParams.apiKeyIndex"
    );
    field_type!(
        typed.expiry,
        Option<f64>,
        "CreateTakeProfitOrderParams.expiry"
    );
    check::<QueryOrdersParams>(
        &oracle,
        "QueryOrdersParams",
        json!([{"name": "symbol", "type": "string", "optional": true}, {"name": "status", "type": "OrderStatus", "optional": true}, {"name": "side", "type": "OrderSide", "optional": true}, {"name": "type", "type": "OrderType", "optional": true}, {"name": "startTime", "type": "number", "optional": true}, {"name": "endTime", "type": "number", "optional": true}, {"name": "page", "type": "number", "optional": true}, {"name": "pageSize", "type": "number", "optional": true}]),
        |value, key| match key {
            "status" => value.status.unwrap().0,
            "side" => value.side.unwrap().0,
            "type" => value.r#type.unwrap().0,
            "startTime" => value.start_time.unwrap(),
            "endTime" => value.end_time.unwrap(),
            "page" => value.page.unwrap(),
            "pageSize" => value.page_size.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        QueryOrdersParams::deserialize(Value::Array(
            oracle["complete"]["QueryOrdersParams"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "QueryOrdersParams public qualified shape"
    );
    let typed: QueryOrdersParams =
        serde_json::from_value(oracle["complete"]["QueryOrdersParams"].clone()).unwrap();
    field_type!(typed.symbol, Option<String>, "QueryOrdersParams.symbol");
    field_type!(
        typed.status,
        Option<safe_cex_venue_lighter::sdk_protocol_types::OrderStatus>,
        "QueryOrdersParams.status"
    );
    field_type!(
        typed.side,
        Option<safe_cex_venue_lighter::sdk_protocol_types::OrderSide>,
        "QueryOrdersParams.side"
    );
    field_type!(
        typed.r#type,
        Option<safe_cex_venue_lighter::sdk_protocol_types::OrderType>,
        "QueryOrdersParams.type"
    );
    field_type!(typed.start_time, Option<f64>, "QueryOrdersParams.startTime");
    field_type!(typed.end_time, Option<f64>, "QueryOrdersParams.endTime");
    field_type!(typed.page, Option<f64>, "QueryOrdersParams.page");
    field_type!(typed.page_size, Option<f64>, "QueryOrdersParams.pageSize");
    check::<ModifyOrderParams>(
        &oracle,
        "ModifyOrderParams",
        json!([{"name": "orderId", "type": "string", "optional": false}, {"name": "price", "type": "string", "optional": true}, {"name": "quantity", "type": "string", "optional": true}, {"name": "expiry", "type": "number", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}]),
        |value, key| match key {
            "expiry" => value.expiry.unwrap(),
            "accountIndex" => value.account_index.unwrap(),
            "apiKeyIndex" => value.api_key_index.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        ModifyOrderParams::deserialize(Value::Array(
            oracle["complete"]["ModifyOrderParams"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "ModifyOrderParams public qualified shape"
    );
    let typed: ModifyOrderParams =
        serde_json::from_value(oracle["complete"]["ModifyOrderParams"].clone()).unwrap();
    field_type!(typed.order_id, String, "ModifyOrderParams.orderId");
    field_type!(typed.price, Option<String>, "ModifyOrderParams.price");
    field_type!(typed.quantity, Option<String>, "ModifyOrderParams.quantity");
    field_type!(typed.expiry, Option<f64>, "ModifyOrderParams.expiry");
    field_type!(
        typed.account_index,
        Option<f64>,
        "ModifyOrderParams.accountIndex"
    );
    field_type!(
        typed.api_key_index,
        Option<f64>,
        "ModifyOrderParams.apiKeyIndex"
    );
    check::<BatchCancelOrdersParams>(
        &oracle,
        "BatchCancelOrdersParams",
        json!([
            {"name":"orderIds","type":"string[]","optional":true},
            {"name":"clientOrderIds","type":"string[]","optional":true},
            {"name":"symbol","type":"string","optional":true},
            {"name":"accountIndex","type":"number","optional":true},
            {"name":"apiKeyIndex","type":"number","optional":true}
        ]),
        |value, key| match key {
            "accountIndex" => value.account_index.unwrap(),
            "apiKeyIndex" => value.api_key_index.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    assert!(
        BatchCancelOrdersParams::deserialize(Value::Array(
            oracle["complete"]["BatchCancelOrdersParams"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "BatchCancelOrdersParams public qualified shape"
    );
    let typed: BatchCancelOrdersParams =
        serde_json::from_value(oracle["complete"]["BatchCancelOrdersParams"].clone()).unwrap();
    field_type!(
        typed.order_ids,
        Option<Vec<String>>,
        "BatchCancelOrdersParams.orderIds"
    );
    field_type!(
        typed.client_order_ids,
        Option<Vec<String>>,
        "BatchCancelOrdersParams.clientOrderIds"
    );
    field_type!(
        typed.symbol,
        Option<String>,
        "BatchCancelOrdersParams.symbol"
    );
    field_type!(
        typed.account_index,
        Option<f64>,
        "BatchCancelOrdersParams.accountIndex"
    );
    field_type!(
        typed.api_key_index,
        Option<f64>,
        "BatchCancelOrdersParams.apiKeyIndex"
    );
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_orders_source_controls_fail_by_name() {
    for (mutation, name) in [
        (
            "inherited-number",
            "CancelOrderParams unsupported interface heritage",
        ),
        ("required-order-id", "CancelOrderParams source schema"),
        ("stats-string", "OrderStats source schema"),
        ("order-type-string", "OrderInfo source schema"),
        (
            "cancel-array-scalar",
            "BatchCancelOrdersParams source schema",
        ),
        (
            "reduce-only-required",
            "CreateLimitOrderParams source schema",
        ),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_orders_records_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_ORDERS_MUTATION", mutation)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(101), "{mutation}");
        assert!(
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
            .contains(name),
            "{mutation}: {}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_orders_missing_compiler_fails_loudly() {
    let out = Command::new("node")
        .arg(script())
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            "/nonexistent/lighter-sdk-orders-compiler",
        )
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("MODULE_NOT_FOUND"));
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
