#[path = "support/unknown_depth.rs"]
mod unknown_depth;
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_lighter::sdk_protocol_types::{OrderSide, OrderType, TimeInForce};
use safe_cex_venue_lighter::sdk_transactions::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;
fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/sdk-transactions-types-oracle.cjs"
    )
}
fn oracle_capture() -> Value {
    static CAPTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPTURE
        .get_or_init(|| {
            let output = Command::new("node").arg(script()).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            serde_json::from_slice(&output.stdout).unwrap()
        })
        .clone()
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
impl NativeIdentity for TransactOpts {
    fn native_identity(&self) -> Value {
        json!({"nonce": self.nonce,"expiry": self.expiry,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index,"gasPrice": self.gas_price,"gasLimit": self.gas_limit})
    }
}
impl NativeIdentity for CreateOrderTxReq {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"type": self.r#type,"side": self.side,"price": self.price,"quantity": self.quantity,"timeInForce": self.time_in_force,"reduceOnly": self.reduce_only,"postOnly": self.post_only,"stopPrice": self.stop_price,"clientOrderId": self.client_order_id,"nonce": self.nonce,"expiry": self.expiry,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index})
    }
}
impl NativeIdentity for CancelOrderTxReq {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"orderId": self.order_id,"nonce": self.nonce,"expiry": self.expiry,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index})
    }
}
impl NativeIdentity for TransferTxReq {
    fn native_identity(&self) -> Value {
        json!({"token": self.token,"recipient": self.recipient,"amount": self.amount,"memo": self.memo,"nonce": self.nonce,"expiry": self.expiry,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index})
    }
}
impl NativeIdentity for WithdrawTxReq {
    fn native_identity(&self) -> Value {
        json!({"token": self.token,"amount": self.amount,"l1Recipient": self.l1_recipient,"fastWithdraw": self.fast_withdraw,"nonce": self.nonce,"expiry": self.expiry,"accountIndex": self.account_index,"apiKeyIndex": self.api_key_index})
    }
}
impl NativeIdentity for UpdateApiKeyTxReq {
    fn native_identity(&self) -> Value {
        json!({"apiKeyIndex": self.api_key_index,"newPublicKey": self.new_public_key,"permissions": self.permissions,"nonce": self.nonce,"expiry": self.expiry,"accountIndex": self.account_index})
    }
}
impl NativeIdentity for TxSignature {
    fn native_identity(&self) -> Value {
        json!({"r": self.r,"s": self.s,"v": self.v})
    }
}
impl NativeIdentity for L2CreateOrderTxInfo {
    fn native_identity(&self) -> Value {
        json!({"txType": self.tx_type,"marketId": self.market_id,"orderType": self.order_type,"side": self.side,"price": self.price,"quantity": self.quantity,"timeInForce": self.time_in_force,"reduceOnly": self.reduce_only,"postOnly": self.post_only,"stopPrice": self.stop_price,"clientOrderId": self.client_order_id,"opts": self.opts .native_identity()})
    }
}
impl NativeIdentity for L2CancelOrderTxInfo {
    fn native_identity(&self) -> Value {
        json!({"txType": self.tx_type,"marketId": self.market_id,"orderId": self.order_id,"opts": self.opts .native_identity()})
    }
}
impl NativeIdentity for L2TransferTxInfo {
    fn native_identity(&self) -> Value {
        json!({"txType": self.tx_type,"tokenAddress": self.token_address,"recipient": self.recipient,"amount": self.amount,"memo": self.memo,"opts": self.opts .native_identity()})
    }
}
impl NativeIdentity for L2WithdrawTxInfo {
    fn native_identity(&self) -> Value {
        json!({"txType": self.tx_type,"tokenAddress": self.token_address,"amount": self.amount,"l1Recipient": self.l1_recipient,"fastWithdraw": self.fast_withdraw,"opts": self.opts .native_identity()})
    }
}
impl NativeIdentity for L2UpdateApiKeyTxInfo {
    fn native_identity(&self) -> Value {
        json!({"txType": self.tx_type,"apiKeyIndex": self.api_key_index,"newPublicKey": self.new_public_key,"permissions": self.permissions,"opts": self.opts .native_identity()})
    }
}
impl NativeIdentity for L2LiquidationTxInfo {
    fn native_identity(&self) -> Value {
        json!({"txType": self.tx_type,"liquidateeAccount": self.liquidatee_account,"marketId": self.market_id,"quantity": self.quantity,"opts": self.opts .native_identity()})
    }
}
impl NativeIdentity for L2DeleverageTxInfo {
    fn native_identity(&self) -> Value {
        json!({"txType": self.tx_type,"deleverageeAccount": self.deleveragee_account,"marketId": self.market_id,"quantity": self.quantity,"opts": self.opts .native_identity()})
    }
}
impl NativeIdentity for L2FundingFeeTxInfo {
    fn native_identity(&self) -> Value {
        json!({"txType": self.tx_type,"marketId": self.market_id,"fundingRate": self.funding_rate,"opts": self.opts .native_identity()})
    }
}

fn identity_inputs(oracle: &Value, name: &str, value: &Value, path: &str) -> Vec<(String, Value)> {
    let mut cases = Vec::new();
    for field in oracle["schemas"][name].as_array().unwrap() {
        let key = field["name"].as_str().unwrap();
        let kind = field["type"].as_str().unwrap();
        let label = format!("{path}.{key}");
        if key == "txType" {
            continue;
        }
        if kind == "TransactOpts" {
            for (child_label, changed) in
                identity_inputs(oracle, "TransactOpts", &value[key], &label)
            {
                let mut input = value.clone();
                input[key] = changed;
                cases.push((child_label, input));
            }
            continue;
        }
        let alternatives = match kind {
            "string" => vec![json!(format!("identity:{label}"))],
            "boolean" => vec![json!(false), json!(true)],
            kind if numeric_kind(kind) => vec![json!(876543.25)],
            other => panic!("unclassified native identity {label}: {other}"),
        };
        for replacement in alternatives {
            let mut input = value.clone();
            input[key] = replacement;
            cases.push((label.clone(), input));
        }
    }
    cases
}

fn numeric_kind(kind: &str) -> bool {
    matches!(kind, "number" | "OrderType" | "OrderSide" | "TimeInForce")
}

fn check<T: DeserializeOwned + Serialize + JsNumberOutput + NativeIdentity>(
    oracle: &Value,
    name: &str,
    expected: Value,
    number: impl Fn(&T, &str) -> f64,
) {
    assert_eq!(oracle["schemas"][name], expected, "{name} source schema");
    let complete = &oracle["complete"][name];
    unknown_depth::audit::<T>(name, complete.clone(), false);
    let parsed = T::deserialize(complete.clone()).unwrap();
    assert_eq!(
        parsed.native_identity(),
        *complete,
        "{name} native field identity"
    );
    for (label, input) in identity_inputs(oracle, name, complete, name) {
        let decoded = T::deserialize(input.clone()).unwrap();
        assert_eq!(
            decoded.native_identity(),
            input,
            "{label} native field identity"
        );
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
        let mut invalid = vec![json!(null)];
        if kind != "Record<string, string>" {
            invalid.push(json!({}));
        }
        if !kind.ends_with("[]") {
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
        .filter(|f| f["type"].as_str().unwrap().ends_with("[]"))
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
                "{name}.{key} rejects invalid array element"
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
                "-1e-4000",
                "16777217.25",
                "9007199254740993",
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
            (
                "bare raw",
                serde_json::from_str::<T>(w["encoded"].as_str().unwrap()).unwrap(),
            ),
            ("bare owned", T::deserialize(input.clone()).unwrap()),
            ("bare borrowed", T::deserialize(&input).unwrap()),
            (
                "raw",
                js_from_str::<T>(w["encoded"].as_str().unwrap()).unwrap(),
            ),
            ("owned", js_from_value::<T>(input.clone()).unwrap()),
            ("borrowed", js_from_value_ref::<T>(&input).unwrap()),
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
        let result = js_from_str::<T>(w["encoded"].as_str().unwrap());
        if w["representable"] == true {
            let value = serde_json::to_value(result.unwrap()).unwrap();
            for f in expected.as_array().unwrap().iter().filter(|f| {
                matches!(
                    f["type"].as_str(),
                    Some("string" | "string[]" | "Record<string, string>")
                )
            }) {
                let field = &value[f["name"].as_str().unwrap()];
                let values = if f["type"] == "Record<string, string>" {
                    field.as_object().unwrap().values().collect::<Vec<_>>()
                } else if f["type"] == "string[]" {
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
            typed_utf16::assert_projection(w["encoded"].as_str().unwrap(), &result.unwrap());
        }
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_transaction_records_match_live_typescript() {
    let oracle = oracle_capture();
    check::<TransactOpts>(
        &oracle,
        "TransactOpts",
        json!([{"name": "nonce", "type": "number", "optional": false}, {"name": "expiry", "type": "number", "optional": false}, {"name": "accountIndex", "type": "number", "optional": false}, {"name": "apiKeyIndex", "type": "number", "optional": false}, {"name": "gasPrice", "type": "string", "optional": true}, {"name": "gasLimit", "type": "number", "optional": true}]),
        |v, key| match key {
            "nonce" => v.nonce,
            "expiry" => v.expiry,
            "accountIndex" => v.account_index,
            "apiKeyIndex" => v.api_key_index,
            "gasLimit" => v.gas_limit.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: TransactOpts =
        serde_json::from_value(oracle["complete"]["TransactOpts"].clone()).unwrap();
    field_type!(typed.nonce, f64, "TransactOpts.nonce");
    field_type!(typed.expiry, f64, "TransactOpts.expiry");
    field_type!(typed.account_index, f64, "TransactOpts.accountIndex");
    field_type!(typed.api_key_index, f64, "TransactOpts.apiKeyIndex");
    field_type!(typed.gas_price, Option<String>, "TransactOpts.gasPrice");
    field_type!(typed.gas_limit, Option<f64>, "TransactOpts.gasLimit");
    assert!(
        TransactOpts::deserialize(json!([])).is_err(),
        "TransactOpts qualified map guard"
    );
    check::<CreateOrderTxReq>(
        &oracle,
        "CreateOrderTxReq",
        json!([{"name": "symbol", "type": "string", "optional": false}, {"name": "type", "type": "OrderType", "optional": false}, {"name": "side", "type": "OrderSide", "optional": false}, {"name": "price", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "timeInForce", "type": "TimeInForce", "optional": true}, {"name": "reduceOnly", "type": "boolean", "optional": true}, {"name": "postOnly", "type": "boolean", "optional": true}, {"name": "stopPrice", "type": "string", "optional": true}, {"name": "clientOrderId", "type": "string", "optional": true}, {"name": "nonce", "type": "number", "optional": true}, {"name": "expiry", "type": "number", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}]),
        |v, key| match key {
            "type" => v.r#type.0,
            "side" => v.side.0,
            "timeInForce" => v.time_in_force.unwrap().0,
            "nonce" => v.nonce.unwrap(),
            "expiry" => v.expiry.unwrap(),
            "accountIndex" => v.account_index.unwrap(),
            "apiKeyIndex" => v.api_key_index.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: CreateOrderTxReq =
        serde_json::from_value(oracle["complete"]["CreateOrderTxReq"].clone()).unwrap();
    field_type!(typed.symbol, String, "CreateOrderTxReq.symbol");
    field_type!(typed.r#type, OrderType, "CreateOrderTxReq.type");
    field_type!(typed.side, OrderSide, "CreateOrderTxReq.side");
    field_type!(typed.price, String, "CreateOrderTxReq.price");
    field_type!(typed.quantity, String, "CreateOrderTxReq.quantity");
    field_type!(
        typed.time_in_force,
        Option<TimeInForce>,
        "CreateOrderTxReq.timeInForce"
    );
    field_type!(
        typed.reduce_only,
        Option<bool>,
        "CreateOrderTxReq.reduceOnly"
    );
    field_type!(typed.post_only, Option<bool>, "CreateOrderTxReq.postOnly");
    field_type!(
        typed.stop_price,
        Option<String>,
        "CreateOrderTxReq.stopPrice"
    );
    field_type!(
        typed.client_order_id,
        Option<String>,
        "CreateOrderTxReq.clientOrderId"
    );
    field_type!(typed.nonce, Option<f64>, "CreateOrderTxReq.nonce");
    field_type!(typed.expiry, Option<f64>, "CreateOrderTxReq.expiry");
    field_type!(
        typed.account_index,
        Option<f64>,
        "CreateOrderTxReq.accountIndex"
    );
    field_type!(
        typed.api_key_index,
        Option<f64>,
        "CreateOrderTxReq.apiKeyIndex"
    );
    assert!(
        CreateOrderTxReq::deserialize(json!([])).is_err(),
        "CreateOrderTxReq qualified map guard"
    );
    check::<CancelOrderTxReq>(
        &oracle,
        "CancelOrderTxReq",
        json!([{"name": "symbol", "type": "string", "optional": false}, {"name": "orderId", "type": "string", "optional": false}, {"name": "nonce", "type": "number", "optional": true}, {"name": "expiry", "type": "number", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}]),
        |v, key| match key {
            "nonce" => v.nonce.unwrap(),
            "expiry" => v.expiry.unwrap(),
            "accountIndex" => v.account_index.unwrap(),
            "apiKeyIndex" => v.api_key_index.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: CancelOrderTxReq =
        serde_json::from_value(oracle["complete"]["CancelOrderTxReq"].clone()).unwrap();
    field_type!(typed.symbol, String, "CancelOrderTxReq.symbol");
    field_type!(typed.order_id, String, "CancelOrderTxReq.orderId");
    field_type!(typed.nonce, Option<f64>, "CancelOrderTxReq.nonce");
    field_type!(typed.expiry, Option<f64>, "CancelOrderTxReq.expiry");
    field_type!(
        typed.account_index,
        Option<f64>,
        "CancelOrderTxReq.accountIndex"
    );
    field_type!(
        typed.api_key_index,
        Option<f64>,
        "CancelOrderTxReq.apiKeyIndex"
    );
    assert!(
        CancelOrderTxReq::deserialize(json!([])).is_err(),
        "CancelOrderTxReq qualified map guard"
    );
    check::<TransferTxReq>(
        &oracle,
        "TransferTxReq",
        json!([{"name": "token", "type": "string", "optional": false}, {"name": "recipient", "type": "string", "optional": false}, {"name": "amount", "type": "string", "optional": false}, {"name": "memo", "type": "string", "optional": true}, {"name": "nonce", "type": "number", "optional": true}, {"name": "expiry", "type": "number", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}]),
        |v, key| match key {
            "nonce" => v.nonce.unwrap(),
            "expiry" => v.expiry.unwrap(),
            "accountIndex" => v.account_index.unwrap(),
            "apiKeyIndex" => v.api_key_index.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: TransferTxReq =
        serde_json::from_value(oracle["complete"]["TransferTxReq"].clone()).unwrap();
    field_type!(typed.token, String, "TransferTxReq.token");
    field_type!(typed.recipient, String, "TransferTxReq.recipient");
    field_type!(typed.amount, String, "TransferTxReq.amount");
    field_type!(typed.memo, Option<String>, "TransferTxReq.memo");
    field_type!(typed.nonce, Option<f64>, "TransferTxReq.nonce");
    field_type!(typed.expiry, Option<f64>, "TransferTxReq.expiry");
    field_type!(
        typed.account_index,
        Option<f64>,
        "TransferTxReq.accountIndex"
    );
    field_type!(
        typed.api_key_index,
        Option<f64>,
        "TransferTxReq.apiKeyIndex"
    );
    assert!(
        TransferTxReq::deserialize(json!([])).is_err(),
        "TransferTxReq qualified map guard"
    );
    check::<WithdrawTxReq>(
        &oracle,
        "WithdrawTxReq",
        json!([{"name": "token", "type": "string", "optional": false}, {"name": "amount", "type": "string", "optional": false}, {"name": "l1Recipient", "type": "string", "optional": true}, {"name": "fastWithdraw", "type": "boolean", "optional": true}, {"name": "nonce", "type": "number", "optional": true}, {"name": "expiry", "type": "number", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}, {"name": "apiKeyIndex", "type": "number", "optional": true}]),
        |v, key| match key {
            "nonce" => v.nonce.unwrap(),
            "expiry" => v.expiry.unwrap(),
            "accountIndex" => v.account_index.unwrap(),
            "apiKeyIndex" => v.api_key_index.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: WithdrawTxReq =
        serde_json::from_value(oracle["complete"]["WithdrawTxReq"].clone()).unwrap();
    field_type!(typed.token, String, "WithdrawTxReq.token");
    field_type!(typed.amount, String, "WithdrawTxReq.amount");
    field_type!(
        typed.l1_recipient,
        Option<String>,
        "WithdrawTxReq.l1Recipient"
    );
    field_type!(
        typed.fast_withdraw,
        Option<bool>,
        "WithdrawTxReq.fastWithdraw"
    );
    field_type!(typed.nonce, Option<f64>, "WithdrawTxReq.nonce");
    field_type!(typed.expiry, Option<f64>, "WithdrawTxReq.expiry");
    field_type!(
        typed.account_index,
        Option<f64>,
        "WithdrawTxReq.accountIndex"
    );
    field_type!(
        typed.api_key_index,
        Option<f64>,
        "WithdrawTxReq.apiKeyIndex"
    );
    assert!(
        WithdrawTxReq::deserialize(json!([])).is_err(),
        "WithdrawTxReq qualified map guard"
    );
    check::<UpdateApiKeyTxReq>(
        &oracle,
        "UpdateApiKeyTxReq",
        json!([{"name": "apiKeyIndex", "type": "number", "optional": false}, {"name": "newPublicKey", "type": "string", "optional": false}, {"name": "permissions", "type": "number", "optional": false}, {"name": "nonce", "type": "number", "optional": true}, {"name": "expiry", "type": "number", "optional": true}, {"name": "accountIndex", "type": "number", "optional": true}]),
        |v, key| match key {
            "apiKeyIndex" => v.api_key_index,
            "permissions" => v.permissions,
            "nonce" => v.nonce.unwrap(),
            "expiry" => v.expiry.unwrap(),
            "accountIndex" => v.account_index.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: UpdateApiKeyTxReq =
        serde_json::from_value(oracle["complete"]["UpdateApiKeyTxReq"].clone()).unwrap();
    field_type!(typed.api_key_index, f64, "UpdateApiKeyTxReq.apiKeyIndex");
    field_type!(
        typed.new_public_key,
        String,
        "UpdateApiKeyTxReq.newPublicKey"
    );
    field_type!(typed.permissions, f64, "UpdateApiKeyTxReq.permissions");
    field_type!(typed.nonce, Option<f64>, "UpdateApiKeyTxReq.nonce");
    field_type!(typed.expiry, Option<f64>, "UpdateApiKeyTxReq.expiry");
    field_type!(
        typed.account_index,
        Option<f64>,
        "UpdateApiKeyTxReq.accountIndex"
    );
    assert!(
        UpdateApiKeyTxReq::deserialize(json!([])).is_err(),
        "UpdateApiKeyTxReq qualified map guard"
    );
    check::<TxSignature>(
        &oracle,
        "TxSignature",
        json!([{"name": "r", "type": "string", "optional": false}, {"name": "s", "type": "string", "optional": false}, {"name": "v", "type": "number", "optional": false}]),
        |v, key| match key {
            "v" => v.v,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: TxSignature =
        serde_json::from_value(oracle["complete"]["TxSignature"].clone()).unwrap();
    field_type!(typed.r, String, "TxSignature.r");
    field_type!(typed.s, String, "TxSignature.s");
    field_type!(typed.v, f64, "TxSignature.v");
    assert!(
        TxSignature::deserialize(json!([])).is_err(),
        "TxSignature qualified map guard"
    );
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_transaction_source_controls_fail_by_name() {
    for (mutation, name) in [
        (
            "inherited-number",
            "TransactOpts unsupported interface heritage",
        ),
        ("nonce-string", "TransactOpts source schema"),
        ("order-wrapper", "CreateOrderTxReq source schema"),
        ("gas-required", "TransactOpts source schema"),
        (
            "missing-enum-imports",
            "transaction source program diagnostics must be empty: 6",
        ),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_transaction_records_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_TRANSACTIONS_MUTATION", mutation)
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
fn sdk_transaction_missing_compiler_fails_loudly() {
    let out = Command::new("node")
        .arg(script())
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            "/nonexistent/lighter-sdk-responses-compiler",
        )
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("MODULE_NOT_FOUND"));
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_l2_records_match_live_typescript() {
    let oracle = oracle_capture();
    check_tag::<L2CreateOrderTxType>(&oracle, "L2CreateOrderTxInfo", 14);
    check::<L2CreateOrderTxInfo>(
        &oracle,
        "L2CreateOrderTxInfo",
        json!([{"name": "txType", "type": "typeof import('./constants').TxType.L2CreateOrder", "optional": false}, {"name": "marketId", "type": "string", "optional": false}, {"name": "orderType", "type": "OrderType", "optional": false}, {"name": "side", "type": "OrderSide", "optional": false}, {"name": "price", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "timeInForce", "type": "TimeInForce", "optional": false}, {"name": "reduceOnly", "type": "boolean", "optional": false}, {"name": "postOnly", "type": "boolean", "optional": false}, {"name": "stopPrice", "type": "string", "optional": true}, {"name": "clientOrderId", "type": "string", "optional": true}, {"name": "opts", "type": "TransactOpts", "optional": false}]),
        |v, key| match key {
            "orderType" => v.order_type.0,
            "side" => v.side.0,
            "timeInForce" => v.time_in_force.0,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: L2CreateOrderTxInfo =
        serde_json::from_value(oracle["complete"]["L2CreateOrderTxInfo"].clone()).unwrap();
    field_type!(
        typed.tx_type,
        L2CreateOrderTxType,
        "L2CreateOrderTxInfo.txType"
    );
    field_type!(typed.market_id, String, "L2CreateOrderTxInfo.marketId");
    field_type!(typed.order_type, OrderType, "L2CreateOrderTxInfo.orderType");
    field_type!(typed.side, OrderSide, "L2CreateOrderTxInfo.side");
    field_type!(typed.price, String, "L2CreateOrderTxInfo.price");
    field_type!(typed.quantity, String, "L2CreateOrderTxInfo.quantity");
    field_type!(
        typed.time_in_force,
        TimeInForce,
        "L2CreateOrderTxInfo.timeInForce"
    );
    field_type!(typed.reduce_only, bool, "L2CreateOrderTxInfo.reduceOnly");
    field_type!(typed.post_only, bool, "L2CreateOrderTxInfo.postOnly");
    field_type!(
        typed.stop_price,
        Option<String>,
        "L2CreateOrderTxInfo.stopPrice"
    );
    field_type!(
        typed.client_order_id,
        Option<String>,
        "L2CreateOrderTxInfo.clientOrderId"
    );
    field_type!(typed.opts, TransactOpts, "L2CreateOrderTxInfo.opts");
    match typed.tx_type {
        L2CreateOrderTxType::Value => (),
    };
    assert!(
        L2CreateOrderTxInfo::deserialize(json!([])).is_err(),
        "L2CreateOrderTxInfo qualified map guard"
    );
    assert!(
        L2CreateOrderTxType::deserialize(json!({"Value":null})).is_err(),
        "L2CreateOrderTxInfo qualified literal guard"
    );
    check_opts::<L2CreateOrderTxInfo>(&oracle, "L2CreateOrderTxInfo", |v| &v.opts);
    check_tag::<L2CancelOrderTxType>(&oracle, "L2CancelOrderTxInfo", 15);
    check::<L2CancelOrderTxInfo>(
        &oracle,
        "L2CancelOrderTxInfo",
        json!([{"name": "txType", "type": "typeof import('./constants').TxType.L2CancelOrder", "optional": false}, {"name": "marketId", "type": "string", "optional": false}, {"name": "orderId", "type": "string", "optional": false}, {"name": "opts", "type": "TransactOpts", "optional": false}]),
        |_v, key| panic!("unexpected numeric field {key}"),
    );
    let typed: L2CancelOrderTxInfo =
        serde_json::from_value(oracle["complete"]["L2CancelOrderTxInfo"].clone()).unwrap();
    field_type!(
        typed.tx_type,
        L2CancelOrderTxType,
        "L2CancelOrderTxInfo.txType"
    );
    field_type!(typed.market_id, String, "L2CancelOrderTxInfo.marketId");
    field_type!(typed.order_id, String, "L2CancelOrderTxInfo.orderId");
    field_type!(typed.opts, TransactOpts, "L2CancelOrderTxInfo.opts");
    match typed.tx_type {
        L2CancelOrderTxType::Value => (),
    };
    assert!(
        L2CancelOrderTxInfo::deserialize(json!([])).is_err(),
        "L2CancelOrderTxInfo qualified map guard"
    );
    assert!(
        L2CancelOrderTxType::deserialize(json!({"Value":null})).is_err(),
        "L2CancelOrderTxInfo qualified literal guard"
    );
    check_opts::<L2CancelOrderTxInfo>(&oracle, "L2CancelOrderTxInfo", |v| &v.opts);
    check_tag::<L2TransferTxType>(&oracle, "L2TransferTxInfo", 16);
    check::<L2TransferTxInfo>(
        &oracle,
        "L2TransferTxInfo",
        json!([{"name": "txType", "type": "typeof import('./constants').TxType.L2Transfer", "optional": false}, {"name": "tokenAddress", "type": "string", "optional": false}, {"name": "recipient", "type": "string", "optional": false}, {"name": "amount", "type": "string", "optional": false}, {"name": "memo", "type": "string", "optional": true}, {"name": "opts", "type": "TransactOpts", "optional": false}]),
        |_v, key| panic!("unexpected numeric field {key}"),
    );
    let typed: L2TransferTxInfo =
        serde_json::from_value(oracle["complete"]["L2TransferTxInfo"].clone()).unwrap();
    field_type!(typed.tx_type, L2TransferTxType, "L2TransferTxInfo.txType");
    field_type!(typed.token_address, String, "L2TransferTxInfo.tokenAddress");
    field_type!(typed.recipient, String, "L2TransferTxInfo.recipient");
    field_type!(typed.amount, String, "L2TransferTxInfo.amount");
    field_type!(typed.memo, Option<String>, "L2TransferTxInfo.memo");
    field_type!(typed.opts, TransactOpts, "L2TransferTxInfo.opts");
    match typed.tx_type {
        L2TransferTxType::Value => (),
    };
    assert!(
        L2TransferTxInfo::deserialize(json!([])).is_err(),
        "L2TransferTxInfo qualified map guard"
    );
    assert!(
        L2TransferTxType::deserialize(json!({"Value":null})).is_err(),
        "L2TransferTxInfo qualified literal guard"
    );
    check_opts::<L2TransferTxInfo>(&oracle, "L2TransferTxInfo", |v| &v.opts);
    check_tag::<L2WithdrawTxType>(&oracle, "L2WithdrawTxInfo", 17);
    check::<L2WithdrawTxInfo>(
        &oracle,
        "L2WithdrawTxInfo",
        json!([{"name": "txType", "type": "typeof import('./constants').TxType.L2Withdraw", "optional": false}, {"name": "tokenAddress", "type": "string", "optional": false}, {"name": "amount", "type": "string", "optional": false}, {"name": "l1Recipient", "type": "string", "optional": true}, {"name": "fastWithdraw", "type": "boolean", "optional": true}, {"name": "opts", "type": "TransactOpts", "optional": false}]),
        |_v, key| panic!("unexpected numeric field {key}"),
    );
    let typed: L2WithdrawTxInfo =
        serde_json::from_value(oracle["complete"]["L2WithdrawTxInfo"].clone()).unwrap();
    field_type!(typed.tx_type, L2WithdrawTxType, "L2WithdrawTxInfo.txType");
    field_type!(typed.token_address, String, "L2WithdrawTxInfo.tokenAddress");
    field_type!(typed.amount, String, "L2WithdrawTxInfo.amount");
    field_type!(
        typed.l1_recipient,
        Option<String>,
        "L2WithdrawTxInfo.l1Recipient"
    );
    field_type!(
        typed.fast_withdraw,
        Option<bool>,
        "L2WithdrawTxInfo.fastWithdraw"
    );
    field_type!(typed.opts, TransactOpts, "L2WithdrawTxInfo.opts");
    match typed.tx_type {
        L2WithdrawTxType::Value => (),
    };
    assert!(
        L2WithdrawTxInfo::deserialize(json!([])).is_err(),
        "L2WithdrawTxInfo qualified map guard"
    );
    assert!(
        L2WithdrawTxType::deserialize(json!({"Value":null})).is_err(),
        "L2WithdrawTxInfo qualified literal guard"
    );
    check_opts::<L2WithdrawTxInfo>(&oracle, "L2WithdrawTxInfo", |v| &v.opts);
    check_tag::<L2UpdateApiKeyTxType>(&oracle, "L2UpdateApiKeyTxInfo", 18);
    check::<L2UpdateApiKeyTxInfo>(
        &oracle,
        "L2UpdateApiKeyTxInfo",
        json!([{"name": "txType", "type": "typeof import('./constants').TxType.L2UpdateApiKey", "optional": false}, {"name": "apiKeyIndex", "type": "number", "optional": false}, {"name": "newPublicKey", "type": "string", "optional": false}, {"name": "permissions", "type": "number", "optional": false}, {"name": "opts", "type": "TransactOpts", "optional": false}]),
        |v, key| match key {
            "apiKeyIndex" => v.api_key_index,
            "permissions" => v.permissions,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: L2UpdateApiKeyTxInfo =
        serde_json::from_value(oracle["complete"]["L2UpdateApiKeyTxInfo"].clone()).unwrap();
    field_type!(
        typed.tx_type,
        L2UpdateApiKeyTxType,
        "L2UpdateApiKeyTxInfo.txType"
    );
    field_type!(typed.api_key_index, f64, "L2UpdateApiKeyTxInfo.apiKeyIndex");
    field_type!(
        typed.new_public_key,
        String,
        "L2UpdateApiKeyTxInfo.newPublicKey"
    );
    field_type!(typed.permissions, f64, "L2UpdateApiKeyTxInfo.permissions");
    field_type!(typed.opts, TransactOpts, "L2UpdateApiKeyTxInfo.opts");
    match typed.tx_type {
        L2UpdateApiKeyTxType::Value => (),
    };
    assert!(
        L2UpdateApiKeyTxInfo::deserialize(json!([])).is_err(),
        "L2UpdateApiKeyTxInfo qualified map guard"
    );
    assert!(
        L2UpdateApiKeyTxType::deserialize(json!({"Value":null})).is_err(),
        "L2UpdateApiKeyTxInfo qualified literal guard"
    );
    check_opts::<L2UpdateApiKeyTxInfo>(&oracle, "L2UpdateApiKeyTxInfo", |v| &v.opts);
    check_tag::<L2LiquidationTxType>(&oracle, "L2LiquidationTxInfo", 19);
    check::<L2LiquidationTxInfo>(
        &oracle,
        "L2LiquidationTxInfo",
        json!([{"name": "txType", "type": "typeof import('./constants').TxType.L2Liquidation", "optional": false}, {"name": "liquidateeAccount", "type": "string", "optional": false}, {"name": "marketId", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "opts", "type": "TransactOpts", "optional": false}]),
        |_v, key| panic!("unexpected numeric field {key}"),
    );
    let typed: L2LiquidationTxInfo =
        serde_json::from_value(oracle["complete"]["L2LiquidationTxInfo"].clone()).unwrap();
    field_type!(
        typed.tx_type,
        L2LiquidationTxType,
        "L2LiquidationTxInfo.txType"
    );
    field_type!(
        typed.liquidatee_account,
        String,
        "L2LiquidationTxInfo.liquidateeAccount"
    );
    field_type!(typed.market_id, String, "L2LiquidationTxInfo.marketId");
    field_type!(typed.quantity, String, "L2LiquidationTxInfo.quantity");
    field_type!(typed.opts, TransactOpts, "L2LiquidationTxInfo.opts");
    match typed.tx_type {
        L2LiquidationTxType::Value => (),
    };
    assert!(
        L2LiquidationTxInfo::deserialize(json!([])).is_err(),
        "L2LiquidationTxInfo qualified map guard"
    );
    assert!(
        L2LiquidationTxType::deserialize(json!({"Value":null})).is_err(),
        "L2LiquidationTxInfo qualified literal guard"
    );
    check_opts::<L2LiquidationTxInfo>(&oracle, "L2LiquidationTxInfo", |v| &v.opts);
    check_tag::<L2DeleverageTxType>(&oracle, "L2DeleverageTxInfo", 20);
    check::<L2DeleverageTxInfo>(
        &oracle,
        "L2DeleverageTxInfo",
        json!([{"name": "txType", "type": "typeof import('./constants').TxType.L2Deleverage", "optional": false}, {"name": "deleverageeAccount", "type": "string", "optional": false}, {"name": "marketId", "type": "string", "optional": false}, {"name": "quantity", "type": "string", "optional": false}, {"name": "opts", "type": "TransactOpts", "optional": false}]),
        |_v, key| panic!("unexpected numeric field {key}"),
    );
    let typed: L2DeleverageTxInfo =
        serde_json::from_value(oracle["complete"]["L2DeleverageTxInfo"].clone()).unwrap();
    field_type!(
        typed.tx_type,
        L2DeleverageTxType,
        "L2DeleverageTxInfo.txType"
    );
    field_type!(
        typed.deleveragee_account,
        String,
        "L2DeleverageTxInfo.deleverageeAccount"
    );
    field_type!(typed.market_id, String, "L2DeleverageTxInfo.marketId");
    field_type!(typed.quantity, String, "L2DeleverageTxInfo.quantity");
    field_type!(typed.opts, TransactOpts, "L2DeleverageTxInfo.opts");
    match typed.tx_type {
        L2DeleverageTxType::Value => (),
    };
    assert!(
        L2DeleverageTxInfo::deserialize(json!([])).is_err(),
        "L2DeleverageTxInfo qualified map guard"
    );
    assert!(
        L2DeleverageTxType::deserialize(json!({"Value":null})).is_err(),
        "L2DeleverageTxInfo qualified literal guard"
    );
    check_opts::<L2DeleverageTxInfo>(&oracle, "L2DeleverageTxInfo", |v| &v.opts);
    check_tag::<L2FundingFeeTxType>(&oracle, "L2FundingFeeTxInfo", 21);
    check::<L2FundingFeeTxInfo>(
        &oracle,
        "L2FundingFeeTxInfo",
        json!([{"name": "txType", "type": "typeof import('./constants').TxType.L2FundingFee", "optional": false}, {"name": "marketId", "type": "string", "optional": false}, {"name": "fundingRate", "type": "string", "optional": false}, {"name": "opts", "type": "TransactOpts", "optional": false}]),
        |_v, key| panic!("unexpected numeric field {key}"),
    );
    let typed: L2FundingFeeTxInfo =
        serde_json::from_value(oracle["complete"]["L2FundingFeeTxInfo"].clone()).unwrap();
    field_type!(
        typed.tx_type,
        L2FundingFeeTxType,
        "L2FundingFeeTxInfo.txType"
    );
    field_type!(typed.market_id, String, "L2FundingFeeTxInfo.marketId");
    field_type!(typed.funding_rate, String, "L2FundingFeeTxInfo.fundingRate");
    field_type!(typed.opts, TransactOpts, "L2FundingFeeTxInfo.opts");
    match typed.tx_type {
        L2FundingFeeTxType::Value => (),
    };
    assert!(
        L2FundingFeeTxInfo::deserialize(json!([])).is_err(),
        "L2FundingFeeTxInfo qualified map guard"
    );
    assert!(
        L2FundingFeeTxType::deserialize(json!({"Value":null})).is_err(),
        "L2FundingFeeTxInfo qualified literal guard"
    );
    check_opts::<L2FundingFeeTxInfo>(&oracle, "L2FundingFeeTxInfo", |v| &v.opts);
}

fn check_tag<T: DeserializeOwned + Serialize>(oracle: &Value, name: &str, expected: u8) {
    assert_eq!(
        oracle["tags"][name],
        json!(expected),
        "{name}.txType source literal"
    );
    for raw in [
        expected.to_string(),
        format!("{expected}.0"),
        format!("{expected}e0"),
    ] {
        let parsed: T = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            json!(expected),
            "{name} literal numeric spellings"
        );
    }
    for n in (-1..=30).map(f64::from).chain([
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        f64::from(expected) + 0.25,
    ]) {
        let result =
            T::deserialize(serde::de::value::F64Deserializer::<serde::de::value::Error>::new(n));
        assert_eq!(
            result.is_ok(),
            n == f64::from(expected),
            "{name}.txType accepts only its literal: {n:?}"
        );
        if let Ok(value) = result {
            assert_eq!(
                serde_json::to_value(value).unwrap(),
                json!(expected),
                "{name} F64 literal roundtrip"
            );
        }
    }
    for bad in [
        json!(null),
        json!(false),
        json!("14"),
        json!([]),
        json!({}),
        json!({"Value":null}),
    ] {
        assert!(
            T::deserialize(bad).is_err(),
            "{name}.txType rejects nonnumber"
        );
    }
}
fn check_opts<T: DeserializeOwned + Serialize + JsNumberOutput>(
    oracle: &Value,
    name: &str,
    opts: impl Fn(&T) -> &TransactOpts,
) {
    unknown_depth::audit::<T>(name, oracle["complete"][name].clone(), true);
    let witnesses = oracle["nestedNumbers"][name].as_array().unwrap();
    assert_eq!(witnesses.len(), 50, "{name} nested numeric count");
    let actual: Vec<_> = witnesses
        .iter()
        .map(|w| json!([w["field"], w["raw"]]))
        .collect();
    let expected: Vec<_> = oracle["numeric"]["TransactOpts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| json!([w["field"], w["raw"]]))
        .collect();
    assert_eq!(actual, expected, "{name} nested numeric inventory");
    for w in witnesses {
        let raw = w["encoded"].as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        for (path, decoded) in [
            ("bare raw", serde_json::from_str::<T>(raw).unwrap()),
            ("bare owned", T::deserialize(value.clone()).unwrap()),
            ("bare borrowed", T::deserialize(&value).unwrap()),
            ("shared raw", js_from_str::<T>(raw).unwrap()),
            ("shared owned", js_from_value::<T>(value.clone()).unwrap()),
            ("shared borrowed", js_from_value_ref::<T>(&value).unwrap()),
        ] {
            let v = opts(&decoded);
            let number = match w["field"].as_str().unwrap() {
                "nonce" => v.nonce,
                "expiry" => v.expiry,
                "accountIndex" => v.account_index,
                "apiKeyIndex" => v.api_key_index,
                "gasLimit" => v.gas_limit.unwrap(),
                other => panic!("unexpected opts field {other}"),
            };
            assert_eq!(
                format!("{:016x}", number.to_bits()),
                w["bits"].as_str().unwrap(),
                "{name}.opts.{} {path} numeric bits",
                w["field"]
            );
        }
    }
    let strings = oracle["nestedStrings"][name].as_array().unwrap();
    assert_eq!(strings.len(), 9, "{name} nested Unicode inventory");
    for w in strings {
        let raw = w["encoded"].as_str().unwrap();
        let bare = serde_json::from_str::<T>(raw);
        let shared = js_from_str::<T>(raw);
        if w["representable"] == true {
            for decoded in [bare.unwrap(), shared.unwrap()] {
                assert_eq!(
                    json!(
                        opts(&decoded)
                            .gas_price
                            .as_ref()
                            .unwrap()
                            .encode_utf16()
                            .collect::<Vec<_>>()
                    ),
                    w["units"],
                    "{name}.opts.gasPrice UTF16 retention"
                );
            }
        } else {
            typed_utf16::assert_projection(raw, &bare.unwrap());
            typed_utf16::assert_projection(raw, &shared.unwrap());
        }
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_l2_source_controls_fail_by_name() {
    for (mutation, name) in [
        (
            "lost-const",
            "L2CreateOrderTxInfo.txType single numeric literal",
        ),
        (
            "l2-wide-tag",
            "L2CreateOrderTxInfo.txType single numeric literal",
        ),
        ("tag-value", "L2CreateOrderTxInfo.txType source literal"),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_l2_records_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_TRANSACTIONS_MUTATION", mutation)
            .output()
            .unwrap();
        let message = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(101), "{mutation}: {message}");
        assert!(message.contains(name), "{mutation}: {message}");
    }
}

fn union_name(value: &L2TxInfo) -> &'static str {
    match value {
        L2TxInfo::CreateOrder(_) => "L2CreateOrderTxInfo",
        L2TxInfo::CancelOrder(_) => "L2CancelOrderTxInfo",
        L2TxInfo::Transfer(_) => "L2TransferTxInfo",
        L2TxInfo::Withdraw(_) => "L2WithdrawTxInfo",
        L2TxInfo::UpdateApiKey(_) => "L2UpdateApiKeyTxInfo",
        L2TxInfo::Liquidation(_) => "L2LiquidationTxInfo",
        L2TxInfo::Deleverage(_) => "L2DeleverageTxInfo",
        L2TxInfo::FundingFee(_) => "L2FundingFeeTxInfo",
    }
}
fn union_string<'a>(value: &'a L2TxInfo, key: &str) -> &'a str {
    match (value, key) {
        (L2TxInfo::CreateOrder(v), "marketId") => &v.market_id,
        (L2TxInfo::CreateOrder(v), "price") => &v.price,
        (L2TxInfo::CreateOrder(v), "quantity") => &v.quantity,
        (L2TxInfo::CreateOrder(v), "stopPrice") => v.stop_price.as_deref().unwrap(),
        (L2TxInfo::CreateOrder(v), "clientOrderId") => v.client_order_id.as_deref().unwrap(),
        (L2TxInfo::CancelOrder(v), "marketId") => &v.market_id,
        (L2TxInfo::CancelOrder(v), "orderId") => &v.order_id,
        (L2TxInfo::Transfer(v), "tokenAddress") => &v.token_address,
        (L2TxInfo::Transfer(v), "recipient") => &v.recipient,
        (L2TxInfo::Transfer(v), "amount") => &v.amount,
        (L2TxInfo::Transfer(v), "memo") => v.memo.as_deref().unwrap(),
        (L2TxInfo::Withdraw(v), "tokenAddress") => &v.token_address,
        (L2TxInfo::Withdraw(v), "amount") => &v.amount,
        (L2TxInfo::Withdraw(v), "l1Recipient") => v.l1_recipient.as_deref().unwrap(),
        (L2TxInfo::UpdateApiKey(v), "newPublicKey") => &v.new_public_key,
        (L2TxInfo::Liquidation(v), "liquidateeAccount") => &v.liquidatee_account,
        (L2TxInfo::Liquidation(v), "marketId") => &v.market_id,
        (L2TxInfo::Liquidation(v), "quantity") => &v.quantity,
        (L2TxInfo::Deleverage(v), "deleverageeAccount") => &v.deleveragee_account,
        (L2TxInfo::Deleverage(v), "marketId") => &v.market_id,
        (L2TxInfo::Deleverage(v), "quantity") => &v.quantity,
        (L2TxInfo::FundingFee(v), "marketId") => &v.market_id,
        (L2TxInfo::FundingFee(v), "fundingRate") => &v.funding_rate,
        _ => panic!("unclassified union string {key}"),
    }
}
fn union_number(value: &L2TxInfo, key: &str, nested: bool) -> f64 {
    let opts = match value {
        L2TxInfo::CreateOrder(v) => &v.opts,
        L2TxInfo::CancelOrder(v) => &v.opts,
        L2TxInfo::Transfer(v) => &v.opts,
        L2TxInfo::Withdraw(v) => &v.opts,
        L2TxInfo::UpdateApiKey(v) => &v.opts,
        L2TxInfo::Liquidation(v) => &v.opts,
        L2TxInfo::Deleverage(v) => &v.opts,
        L2TxInfo::FundingFee(v) => &v.opts,
    };
    if nested {
        return match key {
            "nonce" => opts.nonce,
            "expiry" => opts.expiry,
            "accountIndex" => opts.account_index,
            "apiKeyIndex" => opts.api_key_index,
            "gasLimit" => opts.gas_limit.unwrap(),
            _ => panic!("unknown numeric field {key}"),
        };
    }
    match (value, key) {
        (L2TxInfo::CreateOrder(v), "orderType") => v.order_type.0,
        (L2TxInfo::CreateOrder(v), "side") => v.side.0,
        (L2TxInfo::CreateOrder(v), "timeInForce") => v.time_in_force.0,
        (L2TxInfo::UpdateApiKey(v), "apiKeyIndex") => v.api_key_index,
        (L2TxInfo::UpdateApiKey(v), "permissions") => v.permissions,
        _ => panic!("unknown numeric field {key}"),
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_l2_union_matches_live_typescript() {
    let oracle = oracle_capture();
    let names = [
        "L2CreateOrderTxInfo",
        "L2CancelOrderTxInfo",
        "L2TransferTxInfo",
        "L2WithdrawTxInfo",
        "L2UpdateApiKeyTxInfo",
        "L2LiquidationTxInfo",
        "L2DeleverageTxInfo",
        "L2FundingFeeTxInfo",
    ];
    assert_eq!(
        oracle["variants"],
        json!(names),
        "L2TxInfo source variant inventory"
    );
    for name in names {
        let base = oracle["complete"][name].clone();
        let parsed = L2TxInfo::deserialize(base.clone()).unwrap();
        assert_eq!(union_name(&parsed), name, "{name} selected variant");
        for field in oracle["schemas"][name].as_array().unwrap() {
            if field["type"] == "string" {
                let key = field["name"].as_str().unwrap();
                assert_eq!(
                    union_string(&parsed, key),
                    base[key].as_str().unwrap(),
                    "{name}.{key} native string identity"
                );
            }
        }
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            base,
            "{name} union roundtrip"
        );
        unknown_depth::audit::<L2TxInfo>(name, base.clone(), false);
        unknown_depth::audit::<L2TxInfo>(name, base.clone(), true);
        for field in oracle["schemas"][name].as_array().unwrap() {
            let key = field["name"].as_str().unwrap();
            if field["type"] == "boolean" {
                for bit in [false, true] {
                    let mut input = base.clone();
                    input[key] = json!(bit);
                    let decoded = L2TxInfo::deserialize(input.clone()).unwrap();
                    let native = match (&decoded, key) {
                        (L2TxInfo::CreateOrder(v), "reduceOnly") => v.reduce_only,
                        (L2TxInfo::CreateOrder(v), "postOnly") => v.post_only,
                        (L2TxInfo::Withdraw(v), "fastWithdraw") => v.fast_withdraw.unwrap(),
                        _ => panic!("unclassified union boolean {name}.{key}"),
                    };
                    assert_eq!(native, bit, "{name}.{key} native boolean identity {bit}");
                    assert_eq!(
                        serde_json::to_value(decoded).unwrap(),
                        input,
                        "{name}.{key} independent boolean roundtrip {bit}"
                    );
                }
            }
            for bad in [json!(null), json!([]), json!({})] {
                let mut input = base.clone();
                input[key] = bad;
                assert!(
                    L2TxInfo::deserialize(input).is_err(),
                    "{name}.{key} union rejects null/wrong type"
                );
            }
        }
        for field in oracle["schemas"][name].as_array().unwrap() {
            let mut input = base.clone();
            input
                .as_object_mut()
                .unwrap()
                .shift_remove(field["name"].as_str().unwrap());
            assert_eq!(
                L2TxInfo::deserialize(input).is_ok(),
                field["optional"].as_bool().unwrap(),
                "{name} union required field {}",
                field["name"]
            );
        }
        for invalid in [
            json!(null),
            json!([]),
            json!({"CreateOrder":base}),
            json!("CreateOrder"),
        ] {
            assert!(
                L2TxInfo::deserialize(invalid).is_err(),
                "{name} union invalid shape"
            );
        }
        for tag in [-1, 0, 13, 22, 28, 99] {
            let mut input = base.clone();
            input["txType"] = json!(tag);
            assert!(
                L2TxInfo::deserialize(input).is_err(),
                "{name} union rejects tag {tag}"
            );
        }
        for category in ["strings", "nestedStrings"] {
            for w in oracle[category][name].as_array().unwrap() {
                let raw = w["encoded"].as_str().unwrap();
                for (path, result) in [
                    (
                        "bare",
                        serde_json::from_str::<L2TxInfo>(raw).map_err(|e| e.to_string()),
                    ),
                    (
                        "shared",
                        js_from_str::<L2TxInfo>(raw).map_err(|e| e.to_string()),
                    ),
                ] {
                    assert!(result.is_ok(), "{name} {category} {path} union Unicode");
                    if let Ok(value) = result {
                        if w["representable"] != true {
                            typed_utf16::assert_projection(raw, &value);
                            continue;
                        }
                        assert_eq!(
                            serde_json::to_value(value).unwrap(),
                            serde_json::from_str::<Value>(raw).unwrap(),
                            "{name} {category} {path} Unicode roundtrip"
                        );
                    }
                }
            }
        }
        for (category, nested) in [("numeric", false), ("nestedNumbers", true)] {
            for w in oracle[category][name].as_array().unwrap() {
                let raw = w["encoded"].as_str().unwrap();
                let value: Value = serde_json::from_str(raw).unwrap();
                let expected = u64::from_str_radix(w["bits"].as_str().unwrap(), 16).unwrap();
                let cases = [
                    serde_json::from_str::<L2TxInfo>(raw).unwrap(),
                    L2TxInfo::deserialize(value.clone()).unwrap(),
                    L2TxInfo::deserialize(&value).unwrap(),
                    js_from_str::<L2TxInfo>(raw).unwrap(),
                    js_from_value::<L2TxInfo>(value.clone()).unwrap(),
                    js_from_value_ref::<L2TxInfo>(&value).unwrap(),
                ];
                for (path, v) in cases.iter().enumerate() {
                    assert_eq!(union_name(v), name, "{name} numeric variant path {path}");
                    assert_eq!(
                        union_number(v, w["field"].as_str().unwrap(), nested).to_bits(),
                        expected,
                        "{name} {} {} union bits path {path}",
                        w["field"],
                        w["raw"]
                    );
                }
            }
        }
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_l2_union_source_control_fails_by_name() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "sdk_l2_union_matches_live_typescript",
            "--ignored",
            "--nocapture",
        ])
        .env("LIGHTER_SDK_TRANSACTIONS_MUTATION", "union-member")
        .output()
        .unwrap();
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(101), "{message}");
    assert!(
        message.contains("L2TxInfo source variant inventory"),
        "{message}"
    );
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_signed_transaction_matches_live_typescript() {
    let oracle = oracle_capture();
    assert_eq!(
        oracle["signedSchema"],
        json!([
            {"name":"tx","type":"L2TxInfo","optional":false},
            {"name":"signature","type":"TxSignature","optional":false},
            {"name":"signer","type":"string","optional":false}
        ]),
        "SignedTx source schema"
    );
    assert_eq!(
        oracle["signedCases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].clone())
            .collect::<Vec<_>>(),
        json!([
            "L2CreateOrderTxInfo",
            "L2CancelOrderTxInfo",
            "L2TransferTxInfo",
            "L2WithdrawTxInfo",
            "L2UpdateApiKeyTxInfo",
            "L2LiquidationTxInfo",
            "L2DeleverageTxInfo",
            "L2FundingFeeTxInfo"
        ])
        .as_array()
        .unwrap()
        .clone(),
        "SignedTx variant inventory"
    );
    for (key, count) in [
        ("signedNumeric", 530),
        ("signedStrings", 216),
        ("signedDepth", 144),
        ("signedUnknownStrings", 216),
    ] {
        assert_eq!(
            oracle[key].as_array().unwrap().len(),
            count,
            "SignedTx {key} witness inventory"
        );
    }
    for case in oracle["signedCases"].as_array().unwrap() {
        let base = case["value"].clone();
        let parsed = SignedTx::deserialize(base.clone()).unwrap();
        assert_eq!(
            parsed.signer,
            base["signer"].as_str().unwrap(),
            "SignedTx.signer native identity"
        );
        assert_eq!(
            parsed.signature.r,
            base["signature"]["r"].as_str().unwrap(),
            "SignedTx.signature.r native identity"
        );
        assert_eq!(
            parsed.signature.s,
            base["signature"]["s"].as_str().unwrap(),
            "SignedTx.signature.s native identity"
        );
        for field in oracle["schemas"][case["name"].as_str().unwrap()]
            .as_array()
            .unwrap()
        {
            if field["type"] == "string" {
                let key = field["name"].as_str().unwrap();
                assert_eq!(
                    union_string(&parsed.tx, key),
                    base["tx"][key].as_str().unwrap(),
                    "SignedTx.tx.{key} native string identity"
                );
            }
        }
        field_type!(parsed.tx, L2TxInfo, "SignedTx.tx");
        field_type!(parsed.signature, TxSignature, "SignedTx.signature");
        assert_eq!(union_name(&parsed.tx), case["name"].as_str().unwrap());
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            base,
            "SignedTx complete roundtrip"
        );
        for field in oracle["schemas"][case["name"].as_str().unwrap()]
            .as_array()
            .unwrap()
        {
            if field["type"] == "boolean" {
                for flag in [false, true] {
                    let mut input = base.clone();
                    let key = field["name"].as_str().unwrap();
                    input["tx"][key] = json!(flag);
                    let parsed = SignedTx::deserialize(input.clone()).unwrap();
                    let native = match (&parsed.tx, key) {
                        (L2TxInfo::CreateOrder(v), "reduceOnly") => v.reduce_only,
                        (L2TxInfo::CreateOrder(v), "postOnly") => v.post_only,
                        (L2TxInfo::Withdraw(v), "fastWithdraw") => v.fast_withdraw.unwrap(),
                        _ => panic!("unclassified SignedTx bool {key}"),
                    };
                    assert_eq!(native, flag, "SignedTx.tx.{key} native boolean identity");
                    assert_eq!(
                        serde_json::to_value(parsed).unwrap(),
                        input,
                        "SignedTx.tx.{key} independent boolean roundtrip {flag}"
                    );
                }
            }
        }
        unknown_depth::audit::<SignedTx>("SignedTx", base.clone(), false);
        for key in ["tx", "signature", "signer"] {
            let mut missing = base.clone();
            missing.as_object_mut().unwrap().shift_remove(key);
            assert!(
                SignedTx::deserialize(missing).is_err(),
                "SignedTx.{key} required"
            );
            for bad in [json!(null), json!([]), json!(false), json!(42)] {
                let mut invalid = base.clone();
                invalid[key] = bad;
                assert!(
                    SignedTx::deserialize(invalid).is_err(),
                    "SignedTx.{key} wrong kind"
                );
            }
        }
    }
    #[derive(Deserialize)]
    struct IgnoreExtra {}
    for w in oracle["signedDepth"].as_array().unwrap() {
        let raw = w["encoded"].as_str().unwrap();
        assert!(
            serde_json::from_str::<IgnoreExtra>(raw).is_ok(),
            "SignedTx ordinary serde discards deep extra"
        );
        let base = oracle["signedCases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == w["name"])
            .unwrap()["value"]
            .clone();
        let mut value = base;
        let mut cursor = &mut value;
        for key in w["path"].as_array().unwrap() {
            cursor = &mut cursor[key.as_str().unwrap()];
        }
        let mut extra = json!(17);
        for _ in 0..w["depth"].as_u64().unwrap() {
            extra = Value::Array(vec![extra]);
        }
        cursor["extra"] = extra;
        let accepted =
            w["depth"].as_u64().unwrap() <= 126 - w["path"].as_array().unwrap().len() as u64;
        for (path, result) in [
            (
                "bare raw",
                serde_json::from_str::<SignedTx>(raw).map_err(|e| e.to_string()),
            ),
            (
                "bare owned",
                SignedTx::deserialize(value.clone()).map_err(|e| e.to_string()),
            ),
            (
                "bare borrowed",
                SignedTx::deserialize(&value).map_err(|e| e.to_string()),
            ),
            (
                "shared raw",
                js_from_str::<SignedTx>(raw).map_err(|e| e.to_string()),
            ),
            (
                "shared owned",
                js_from_value::<SignedTx>(value.clone()).map_err(|e| e.to_string()),
            ),
            (
                "shared borrowed",
                js_from_value_ref::<SignedTx>(&value).map_err(|e| e.to_string()),
            ),
        ] {
            assert_eq!(
                result.is_ok(),
                accepted,
                "SignedTx {} {} depth {} {path}",
                w["name"],
                w["path"],
                w["depth"]
            );
            if let Err(e) = result {
                assert!(e.contains("recursion limit"), "SignedTx depth refusal {e}");
            }
        }
    }
    for w in oracle["signedUnknownStrings"].as_array().unwrap() {
        let raw = w["encoded"].as_str().unwrap();
        assert!(
            serde_json::from_str::<IgnoreExtra>(raw).is_ok(),
            "SignedTx ordinary serde ignores unknown Unicode"
        );
        for result in [
            serde_json::from_str::<SignedTx>(raw).map_err(|e| e.to_string()),
            js_from_str::<SignedTx>(raw).map_err(|e| e.to_string()),
        ] {
            assert!(
                result.is_ok(),
                "SignedTx {} {} unknown Unicode",
                w["name"],
                w["path"]
            );
            typed_utf16::retained(raw);
        }
    }
    for w in oracle["signedStrings"].as_array().unwrap() {
        let raw = w["encoded"].as_str().unwrap();
        for result in [
            serde_json::from_str::<SignedTx>(raw).map_err(|e| e.to_string()),
            js_from_str::<SignedTx>(raw).map_err(|e| e.to_string()),
        ] {
            assert!(
                result.is_ok(),
                "SignedTx {} {} Unicode",
                w["name"],
                w["field"]
            );
            if let Ok(v) = result {
                if w["representable"] != true {
                    typed_utf16::assert_projection(raw, &v);
                    continue;
                }
                assert_eq!(
                    serde_json::to_value(v).unwrap(),
                    serde_json::from_str::<Value>(raw).unwrap(),
                    "SignedTx Unicode roundtrip"
                );
            }
        }
    }
    for w in oracle["signedNumeric"].as_array().unwrap() {
        let raw = w["encoded"].as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        let expected = u64::from_str_radix(w["bits"].as_str().unwrap(), 16).unwrap();
        let cases = [
            serde_json::from_str::<SignedTx>(raw).unwrap(),
            SignedTx::deserialize(value.clone()).unwrap(),
            SignedTx::deserialize(&value).unwrap(),
            js_from_str::<SignedTx>(raw).unwrap(),
            js_from_value::<SignedTx>(value.clone()).unwrap(),
            js_from_value_ref::<SignedTx>(&value).unwrap(),
        ];
        for (path, v) in cases.iter().enumerate() {
            let n = if w["location"] == "signature" {
                v.signature.v
            } else {
                union_number(&v.tx, w["field"].as_str().unwrap(), w["location"] == "opts")
            };
            assert_eq!(
                n.to_bits(),
                expected,
                "SignedTx {} {} {} bits path {path}",
                w["name"],
                w["field"],
                w["raw"]
            );
        }
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_signed_source_controls_fail_by_name() {
    for (mutation, name) in [
        ("signed-signature-string", "SignedTx source schema"),
        ("signed-heritage", "SignedTx unsupported interface heritage"),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_signed_transaction_matches_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_TRANSACTIONS_MUTATION", mutation)
            .output()
            .unwrap();
        let message = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(101), "{message}");
        assert!(message.contains(name), "{message}");
    }
}

fn receipt_paths(raw: &str) -> Vec<Result<TxReceipt, String>> {
    receipt_paths_with_value(raw, serde_json::from_str(raw).unwrap())
}
fn receipt_paths_with_value(raw: &str, value: Value) -> Vec<Result<TxReceipt, String>> {
    vec![
        serde_json::from_str(raw).map_err(|e| e.to_string()),
        TxReceipt::deserialize(value.clone()).map_err(|e| e.to_string()),
        TxReceipt::deserialize(&value).map_err(|e| e.to_string()),
        js_from_str(raw).map_err(|e| e.to_string()),
        js_from_value(value.clone()).map_err(|e| e.to_string()),
        js_from_value_ref(&value).map_err(|e| e.to_string()),
    ]
}
#[derive(Deserialize, Serialize)]
struct ReceiptNumericPayload {
    value: f64,
    other: f64,
}
impl JsNumberOutput for ReceiptNumericPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { value, other });
    }
}
#[derive(Deserialize, Serialize)]
struct ReceiptIdentityPayload {
    left: f64,
    right: f64,
    nested: Vec<ReceiptLabel>,
}
#[derive(Deserialize, Serialize)]
struct ReceiptLabel {
    label: String,
    other: String,
}
impl JsNumberOutput for ReceiptIdentityPayload {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { left, right, nested });
    }
}
impl JsNumberOutput for ReceiptLabel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { label, other });
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_receipt_matches_live_source() {
    use safe_cex_num::numeric_ingress::JsNumberInput;
    use safe_cex_venue_lighter::sdk_protocol_types::TxTypeValue;
    let oracle = oracle_capture();
    assert_eq!(
        oracle["receiptSchema"],
        json!([
            {"name":"txHash","type":"string","optional":false},
            {"name":"txType","type":"TxTypeValue","optional":false},
            {"name":"status","type":"number","optional":false},
            {"name":"blockNumber","type":"number","optional":true},
            {"name":"blockTimestamp","type":"number","optional":true},
            {"name":"gasUsed","type":"string","optional":true},
            {"name":"error","type":"string","optional":true},
            {"name":"data","type":"any","optional":true}
        ]),
        "TxReceipt source schema"
    );
    for (key, count) in [
        ("receiptNumbers", 30),
        ("receiptPayloadNumbers", 20),
        ("receiptStrings", 45),
        ("receiptDepth", 6),
    ] {
        assert_eq!(
            oracle[key].as_array().unwrap().len(),
            count,
            "TxReceipt {key} inventory"
        );
    }
    let lexemes = [
        "-0",
        "-0.0",
        "-0e0",
        "0",
        "1e3",
        "1e400",
        "-1e400",
        "-1e-4000",
        "16777217.25",
        "9007199254740993",
    ];
    let numeric_inventory: Vec<_> = ["status", "blockNumber", "blockTimestamp"]
        .into_iter()
        .flat_map(|field| lexemes.map(|raw| json!([field, raw])))
        .collect();
    assert_eq!(
        oracle["receiptNumbers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| json!([row["field"], row["raw"]]))
            .collect::<Vec<_>>(),
        numeric_inventory,
        "TxReceipt numeric field/lexeme inventory"
    );
    let payload_inventory: Vec<_> = lexemes
        .into_iter()
        .flat_map(|raw| ["root", "nested"].map(|location| json!([location, raw])))
        .collect();
    assert_eq!(
        oracle["receiptPayloadNumbers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| json!([row["location"], row["raw"]]))
            .collect::<Vec<_>>(),
        payload_inventory,
        "TxReceipt payload location/lexeme inventory"
    );
    let units: [Vec<u16>; 9] = [
        vec![],
        "plain".encode_utf16().collect(),
        vec![0xe9],
        vec![0xd83d, 0xdc69],
        vec![0xfeff],
        vec![0x85],
        vec![0xd800],
        vec![0xdfff],
        vec![120, 0xd800, 121],
    ];
    let string_inventory: Vec<_> = ["txHash", "gasUsed", "error", "data", "extra"]
        .into_iter()
        .flat_map(|field| units.iter().map(move |units| json!([field, units])))
        .collect();
    assert_eq!(
        oracle["receiptStrings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| json!([row["field"], row["units"]]))
            .collect::<Vec<_>>(),
        string_inventory,
        "TxReceipt string field/UTF16 inventory"
    );
    assert_eq!(
        oracle["receiptDepth"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["depth"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!(0),
            json!(2),
            json!(125),
            json!(126),
            json!(127),
            json!(140)
        ],
        "TxReceipt depth inventory"
    );
    assert_eq!(
        oracle["receiptNonJson"],
        json!([
            {"kind":"undefined","own":true},{"kind":"bigint","own":true},
            {"kind":"function","own":true},{"kind":"symbol","own":true}
        ]),
        "TxReceipt.data non-JSON any domain remains a FINDING"
    );
    let base = &oracle["receiptBase"];
    assert_eq!(
        oracle["receiptTags"],
        json!([14, 15, 16, 17, 18, 19, 20, 21, 28]),
        "TxReceipt.txType source literals"
    );
    for tag in oracle["receiptTags"].as_array().unwrap() {
        let mut input = base.clone();
        input["txType"] = tag.clone();
        let receipt = TxReceipt::deserialize(input).unwrap();
        assert_eq!(
            u64::from(receipt.tx_type as u8),
            tag.as_u64().unwrap(),
            "TxReceipt.txType native literal identity"
        );
    }
    for tag in [
        json!(0),
        json!(13),
        json!(22),
        json!(29),
        json!(14.5),
        json!("14"),
        json!(null),
        json!({}),
    ] {
        let mut input = base.clone();
        input["txType"] = tag;
        assert!(
            TxReceipt::deserialize(input).is_err(),
            "TxReceipt.txType rejected literal"
        );
    }

    for value in receipt_paths(&base.to_string()) {
        let value = value.unwrap();
        field_type!(value.tx_hash, String, "TxReceipt.txHash");
        field_type!(value.status, f64, "TxReceipt.status");
        field_type!(value.block_number, Option<f64>, "TxReceipt.blockNumber");
        field_type!(
            value.block_timestamp,
            Option<f64>,
            "TxReceipt.blockTimestamp"
        );
        field_type!(value.gas_used, Option<String>, "TxReceipt.gasUsed");
        field_type!(value.error, Option<String>, "TxReceipt.error");
        field_type!(value.tx_type, TxTypeValue, "TxReceipt.txType");
        field_type!(value.data, Option<JsNumberInput>, "TxReceipt.data");
        assert_eq!(
            value.tx_hash, base["txHash"],
            "TxReceipt.txHash native identity"
        );
        assert!(
            matches!(value.tx_type, TxTypeValue::L2CreateOrder),
            "TxReceipt.txType native identity"
        );
        assert_eq!(
            value.status.to_bits(),
            7.25_f64.to_bits(),
            "TxReceipt.status native identity"
        );
        assert_eq!(
            value.block_number.unwrap().to_bits(),
            31.5_f64.to_bits(),
            "TxReceipt.blockNumber native identity"
        );
        assert_eq!(
            value.block_timestamp.unwrap().to_bits(),
            47.75_f64.to_bits(),
            "TxReceipt.blockTimestamp native identity"
        );
        assert_eq!(
            value.gas_used.as_deref(),
            Some("gas-used"),
            "TxReceipt.gasUsed native identity"
        );
        assert_eq!(
            value.error.as_deref(),
            Some("receipt-error"),
            "TxReceipt.error native identity"
        );
        let payload = value
            .data
            .unwrap()
            .decode::<ReceiptIdentityPayload>()
            .unwrap();
        assert_eq!(payload.left, 11.0, "TxReceipt.data.left native identity");
        assert_eq!(payload.right, 23.0, "TxReceipt.data.right native identity");
        assert_eq!(
            payload.nested.len(),
            1,
            "TxReceipt.data.nested native length"
        );
        assert_eq!(
            payload.nested[0].label, "payload-label",
            "TxReceipt.data.nested.label native identity"
        );
        assert_eq!(
            payload.nested[0].other, "payload-other",
            "TxReceipt.data.nested.other native identity"
        );
    }
    for field in oracle["receiptSchema"].as_array().unwrap() {
        let key = field["name"].as_str().unwrap();
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().shift_remove(key);
        let parsed = TxReceipt::deserialize(missing);
        assert_eq!(
            parsed.is_ok(),
            field["optional"].as_bool().unwrap(),
            "TxReceipt.{key} optionality"
        );
        if let Ok(parsed) = parsed {
            assert!(
                serde_json::to_value(parsed).unwrap().get(key).is_none(),
                "TxReceipt.{key} absence"
            );
        }
        if key != "data" {
            let mut invalid = base.clone();
            invalid[key] = Value::Null;
            assert!(
                TxReceipt::deserialize(invalid).is_err(),
                "TxReceipt.{key} rejects null"
            );
        }
    }
    for payload in [
        Value::Null,
        json!(false),
        json!(0),
        json!(""),
        json!([]),
        json!({}),
        json!([1, 2, 1]),
    ] {
        let mut input = base.clone();
        input["data"] = payload.clone();
        for value in receipt_paths(&input.to_string()) {
            let data = value.unwrap().data.expect("present data, including null");
            match &payload {
                Value::Null => assert!(
                    data.decode::<Option<bool>>().unwrap().is_none(),
                    "TxReceipt.data null retained"
                ),
                Value::Bool(expected) => assert_eq!(
                    data.decode::<bool>().unwrap(),
                    *expected,
                    "TxReceipt.data bool"
                ),
                Value::Number(expected) => assert_eq!(
                    data.decode::<f64>().unwrap(),
                    expected.as_f64().unwrap(),
                    "TxReceipt.data number"
                ),
                Value::String(expected) => assert_eq!(
                    data.decode::<String>().unwrap(),
                    *expected,
                    "TxReceipt.data string"
                ),
                Value::Array(expected) => assert_eq!(
                    data.decode::<Vec<f64>>().unwrap(),
                    expected
                        .iter()
                        .map(|v| v.as_f64().unwrap())
                        .collect::<Vec<_>>(),
                    "TxReceipt.data array"
                ),
                Value::Object(_) => assert!(
                    data.decode::<std::collections::BTreeMap<String, bool>>()
                        .unwrap()
                        .is_empty(),
                    "TxReceipt.data empty object"
                ),
            }
        }
    }
    for row in oracle["receiptNumbers"].as_array().unwrap() {
        let expected = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        for (path, value) in receipt_paths(row["encoded"].as_str().unwrap())
            .into_iter()
            .enumerate()
        {
            let value = value.unwrap();
            let number = match row["field"].as_str().unwrap() {
                "status" => value.status,
                "blockNumber" => value.block_number.unwrap(),
                "blockTimestamp" => value.block_timestamp.unwrap(),
                other => panic!("unknown receipt numeric {other}"),
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "TxReceipt {} bits path {path}",
                row["field"]
            );
        }
    }
    for row in oracle["receiptPayloadNumbers"].as_array().unwrap() {
        let expected = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        for (path, value) in receipt_paths(row["encoded"].as_str().unwrap())
            .into_iter()
            .enumerate()
        {
            let data = value.unwrap().data.unwrap();
            let number = if row["location"] == "root" {
                data.decode::<f64>().unwrap()
            } else {
                let payload = data.decode::<ReceiptNumericPayload>().unwrap();
                assert_eq!(payload.other, 13.0, "TxReceipt.data other native identity");
                payload.value
            };
            assert_eq!(
                number.to_bits(),
                expected,
                "TxReceipt.data {} bits path {path}",
                row["location"]
            );
        }
    }
    for row in oracle["receiptStrings"].as_array().unwrap() {
        let raw = row["encoded"].as_str().unwrap();
        for value in [
            serde_json::from_str::<TxReceipt>(raw),
            js_from_str::<TxReceipt>(raw),
        ] {
            assert!(value.is_ok(), "TxReceipt {} Unicode", row["field"]);
            typed_utf16::retained(raw);
            if let Ok(value) = value {
                let units: Vec<u16> = serde_json::from_value(row["units"].clone()).unwrap();
                let expected = String::from_utf16_lossy(&units);
                if row["field"] == "data" {
                    let raw: safe_cex_types::retained_json::RetainedJsonInput =
                        value.data.as_ref().unwrap().clone().decode().unwrap();
                    assert_eq!(
                        raw.as_text().unwrap(),
                        units,
                        "TxReceipt.data retained units"
                    );
                }
                let actual = match row["field"].as_str().unwrap() {
                    "txHash" => value.tx_hash,
                    "gasUsed" => value.gas_used.unwrap(),
                    "error" => value.error.unwrap(),
                    "data" => value.data.unwrap().decode::<String>().unwrap(),
                    "extra" => continue,
                    field => panic!("unknown receipt string field {field}"),
                };
                assert_eq!(
                    actual, expected,
                    "TxReceipt {} positive Unicode native identity",
                    row["field"]
                );
            }
        }
    }
    for row in oracle["receiptDepth"].as_array().unwrap() {
        let depth = row["depth"].as_u64().unwrap();
        let mut value = base.clone();
        let mut payload = json!(17);
        for _ in 0..depth {
            payload = Value::Array(vec![payload]);
        }
        value["data"] = payload;
        for (path, result) in receipt_paths_with_value(row["encoded"].as_str().unwrap(), value)
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                result.is_ok(),
                depth <= 126,
                "TxReceipt.data depth {depth} path {path}"
            );
            match result {
                Err(error) => assert!(
                    error.contains("recursion limit"),
                    "TxReceipt depth cause {error}"
                ),
                Ok(value) => {
                    let mut retained = value.data.unwrap();
                    for level in 0..depth {
                        let mut items = retained.decode::<Vec<JsNumberInput>>().unwrap();
                        assert_eq!(
                            items.len(),
                            1,
                            "TxReceipt.data depth {depth} level {level} path {path} native cardinality"
                        );
                        retained = items.pop().unwrap();
                    }
                    assert_eq!(
                        retained.decode::<f64>().unwrap().to_bits(),
                        17.0_f64.to_bits(),
                        "TxReceipt.data depth {depth} path {path} native leaf"
                    );
                }
            }
        }
    }
    unknown_depth::audit::<TxReceipt>("TxReceipt", base.clone(), false);
    for invalid in [
        json!(null),
        json!([]),
        Value::Array(base.as_object().unwrap().values().cloned().collect()),
        json!(true),
        json!(1),
        json!("receipt"),
    ] {
        assert!(
            TxReceipt::deserialize(invalid).is_err(),
            "TxReceipt requires object"
        );
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_receipt_source_controls_fail_by_name() {
    for (mutation, name) in [
        ("receipt-status-string", "TxReceipt source schema"),
        (
            "receipt-heritage",
            "TxReceipt unsupported interface heritage",
        ),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_receipt_matches_live_source",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_TRANSACTIONS_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(log.contains(name), "{log}");
    }
}

use unknown_depth::typed_utf16;
