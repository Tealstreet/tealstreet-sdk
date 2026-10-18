mod support;
#[path = "support/unknown_depth.rs"]
mod unknown_depth;
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_lighter::sdk_protocol_types::ResultCode;
use safe_cex_venue_lighter::sdk_responses::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;
fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/sdk-responses-types-oracle.cjs"
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
impl NativeIdentity for Result {
    fn native_identity(&self) -> Value {
        json!({"code": self.code,"message": self.message})
    }
}
impl NativeIdentity for NextNonce {
    fn native_identity(&self) -> Value {
        json!({"account": self.account,"nonce": self.nonce,"pendingNonce": self.pending_nonce,"confirmedNonce": self.confirmed_nonce})
    }
}
impl NativeIdentity for ApiKeyInfo {
    fn native_identity(&self) -> Value {
        json!({"index": self.index,"publicKey": self.public_key,"permissions": self.permissions,"label": self.label,"isActive": self.is_active,"createdAt": self.created_at,"lastUsedAt": self.last_used_at})
    }
}
impl NativeIdentity for TxHash {
    fn native_identity(&self) -> Value {
        json!({"txHash": self.tx_hash,"nonce": self.nonce,"submittedAt": self.submitted_at})
    }
}
impl NativeIdentity for ServerTime {
    fn native_identity(&self) -> Value {
        json!({"serverTime": self.server_time,"timezone": self.timezone})
    }
}
impl NativeIdentity for RateLimit {
    fn native_identity(&self) -> Value {
        json!({"type": self.r#type,"interval": self.interval,"limit": self.limit,"current": self.current})
    }
}
impl NativeIdentity for DepositAddress {
    fn native_identity(&self) -> Value {
        json!({"token": self.token,"chain": self.chain,"address": self.address,"memo": self.memo,"minDeposit": self.min_deposit,"confirmations": self.confirmations})
    }
}
impl NativeIdentity for TransferHistoryItem {
    fn native_identity(&self) -> Value {
        json!({"transferId": self.transfer_id,"token": self.token,"amount": self.amount,"from": self.from,"to": self.to,"memo": self.memo,"txHash": self.tx_hash,"createdAt": self.created_at})
    }
}
impl NativeIdentity for MaintenanceWindow {
    fn native_identity(&self) -> Value {
        json!({"id": self.id,"title": self.title,"description": self.description,"startTime": self.start_time,"endTime": self.end_time,"affectedServices": self.affected_services})
    }
}
impl NativeIdentity for FeeTier {
    fn native_identity(&self) -> Value {
        json!({"tier": self.tier,"minVolume": self.min_volume,"feeRate": self.fee_rate})
    }
}
impl NativeIdentity for FundingRate {
    fn native_identity(&self) -> Value {
        json!({"symbol": self.symbol,"fundingRate": self.funding_rate,"nextFundingTime": self.next_funding_time,"fundingInterval": self.funding_interval,"timestamp": self.timestamp})
    }
}
impl NativeIdentity for AccountApiKeys {
    fn native_identity(&self) -> Value {
        json!({"account": self.account,"apiKeys": self.api_keys.iter().map(NativeIdentity::native_identity).collect::<Vec<_>>()})
    }
}
impl NativeIdentity for ExchangeInfo {
    fn native_identity(&self) -> Value {
        json!({"name": self.name,"version": self.version,"markets": self.markets,"tokens": self.tokens,"rateLimits": self.rate_limits.iter().map(NativeIdentity::native_identity).collect::<Vec<_>>(),"serverTime": self.server_time})
    }
}
impl NativeIdentity for ErrorResponse {
    fn native_identity(&self) -> Value {
        json!({"code": self.code,"message": self.message,"details": self.details,"fieldErrors": self.field_errors,"requestId": self.request_id,"timestamp": self.timestamp})
    }
}
impl NativeIdentity for FeeSchedule {
    fn native_identity(&self) -> Value {
        json!({"makerTiers": self.maker_tiers.iter().map(NativeIdentity::native_identity).collect::<Vec<_>>(),"takerTiers": self.taker_tiers.iter().map(NativeIdentity::native_identity).collect::<Vec<_>>(),"withdrawalFees": self.withdrawal_fees,"depositFees": self.deposit_fees})
    }
}
impl NativeIdentity for InsuranceFund {
    fn native_identity(&self) -> Value {
        json!({"totalSize": self.total_size,"balances": self.balances,"lastUpdate": self.last_update})
    }
}
impl NativeIdentity for HealthResponse {
    fn native_identity(&self) -> Value {
        json!({"status": health_status_name(self.status),"version": self.version,"timestamp": self.timestamp,"uptime": self.uptime,"database": self.database.map(connection_status_name),"websocket": self.websocket.map(connection_status_name)})
    }
}
impl NativeIdentity for ComponentStatus {
    fn native_identity(&self) -> Value {
        json!({"name": self.name,"status": component_state_name(self.status),"message": self.message,"lastCheck": self.last_check})
    }
}
impl NativeIdentity for AccountInfoResponse {
    fn native_identity(&self) -> Value {
        json!({"address": self.address,"accountIndex": self.account_index,"nonce": self.nonce,"tier": self.tier,"makerFeeRate": self.maker_fee_rate,"takerFeeRate": self.taker_fee_rate,"tradingVolume30d": self.trading_volume30d,"createdAt": self.created_at,"kycStatus": self.kyc_status.map(kyc_status_name)})
    }
}
impl NativeIdentity for WithdrawalHistoryItem {
    fn native_identity(&self) -> Value {
        json!({"withdrawalId": self.withdrawal_id,"token": self.token,"amount": self.amount,"fee": self.fee,"l1Address": self.l1_address,"txHash": self.tx_hash,"status": withdrawal_state_name(self.status),"createdAt": self.created_at,"completedAt": self.completed_at})
    }
}
impl NativeIdentity for DepositHistoryItem {
    fn native_identity(&self) -> Value {
        json!({"depositId": self.deposit_id,"token": self.token,"amount": self.amount,"txHash": self.tx_hash,"status": deposit_state_name(self.status),"confirmations": self.confirmations,"requiredConfirmations": self.required_confirmations,"createdAt": self.created_at,"completedAt": self.completed_at})
    }
}
impl NativeIdentity for SystemStatus {
    fn native_identity(&self) -> Value {
        json!({"status": system_state_name(self.status),"message": self.message,"components": self.components.iter().map(NativeIdentity::native_identity).collect::<Vec<_>>(),"scheduledMaintenance": self.scheduled_maintenance.as_ref().map(|rows| rows.iter().map(NativeIdentity::native_identity).collect::<Vec<_>>()),"lastUpdate": self.last_update})
    }
}

fn identity_inputs(oracle: &Value, name: &str, value: &Value, path: &str) -> Vec<(String, Value)> {
    let mut cases = Vec::new();
    for field in oracle["schemas"][name].as_array().unwrap() {
        let key = field["name"].as_str().unwrap();
        let kind = field["type"].as_str().unwrap();
        let label = format!("{path}.{key}");
        let alternatives = if let Some(literals) = oracle["unions"][name][key].as_array() {
            literals.clone()
        } else if kind.ends_with("[]") && kind != "string[]" {
            let child_name = &kind[..kind.len() - 2];
            for (index, child) in value[key].as_array().unwrap().iter().enumerate() {
                for (child_label, changed) in
                    identity_inputs(oracle, child_name, child, &format!("{label}[{index}]"))
                {
                    let mut input = value.clone();
                    input[key][index] = changed;
                    cases.push((child_label, input));
                }
            }
            Vec::new()
        } else {
            match kind {
                "string" => vec![json!(format!("identity:{label}"))],
                "boolean" => vec![json!(false), json!(true)],
                "number" | "ResultCode" => vec![json!(876543.25)],
                "string[]" => vec![json!([format!("{label}:first"), format!("{label}:second")])],
                "Record<string, string>" => vec![json!({"identity-key":format!("{label}:value")})],
                other => panic!("unclassified native identity {label}: {other}"),
            }
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
    matches!(kind, "number" | "ResultCode")
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
fn sdk_responses_records_match_live_typescript() {
    let oracle = oracle_capture();
    check::<Result>(
        &oracle,
        "Result",
        json!([{"name": "code", "type": "ResultCode", "optional": false}, {"name": "message", "type": "string", "optional": false}]),
        |v, key| match key {
            "code" => v.code.0,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: Result = serde_json::from_value(oracle["complete"]["Result"].clone()).unwrap();
    field_type!(typed.code, ResultCode, "Result.code");
    field_type!(typed.message, String, "Result.message");
    assert!(
        Result::deserialize(Value::Array(
            oracle["complete"]["Result"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "Result public qualified map-only decoder"
    );
    check::<NextNonce>(
        &oracle,
        "NextNonce",
        json!([{"name": "account", "type": "string", "optional": false}, {"name": "nonce", "type": "number", "optional": false}, {"name": "pendingNonce", "type": "number", "optional": false}, {"name": "confirmedNonce", "type": "number", "optional": false}]),
        |v, key| match key {
            "nonce" => v.nonce,
            "pendingNonce" => v.pending_nonce,
            "confirmedNonce" => v.confirmed_nonce,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: NextNonce = serde_json::from_value(oracle["complete"]["NextNonce"].clone()).unwrap();
    field_type!(typed.account, String, "NextNonce.account");
    field_type!(typed.nonce, f64, "NextNonce.nonce");
    field_type!(typed.pending_nonce, f64, "NextNonce.pendingNonce");
    field_type!(typed.confirmed_nonce, f64, "NextNonce.confirmedNonce");
    assert!(
        NextNonce::deserialize(Value::Array(
            oracle["complete"]["NextNonce"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "NextNonce public qualified map-only decoder"
    );
    check::<ApiKeyInfo>(
        &oracle,
        "ApiKeyInfo",
        json!([{"name": "index", "type": "number", "optional": false}, {"name": "publicKey", "type": "string", "optional": false}, {"name": "permissions", "type": "number", "optional": false}, {"name": "label", "type": "string", "optional": true}, {"name": "isActive", "type": "boolean", "optional": false}, {"name": "createdAt", "type": "number", "optional": false}, {"name": "lastUsedAt", "type": "number", "optional": true}]),
        |v, key| match key {
            "index" => v.index,
            "permissions" => v.permissions,
            "createdAt" => v.created_at,
            "lastUsedAt" => v.last_used_at.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: ApiKeyInfo =
        serde_json::from_value(oracle["complete"]["ApiKeyInfo"].clone()).unwrap();
    field_type!(typed.index, f64, "ApiKeyInfo.index");
    field_type!(typed.public_key, String, "ApiKeyInfo.publicKey");
    field_type!(typed.permissions, f64, "ApiKeyInfo.permissions");
    field_type!(typed.label, Option<String>, "ApiKeyInfo.label");
    field_type!(typed.is_active, bool, "ApiKeyInfo.isActive");
    field_type!(typed.created_at, f64, "ApiKeyInfo.createdAt");
    field_type!(typed.last_used_at, Option<f64>, "ApiKeyInfo.lastUsedAt");
    assert!(
        ApiKeyInfo::deserialize(Value::Array(
            oracle["complete"]["ApiKeyInfo"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "ApiKeyInfo public qualified map-only decoder"
    );
    check::<TxHash>(
        &oracle,
        "TxHash",
        json!([{"name": "txHash", "type": "string", "optional": false}, {"name": "nonce", "type": "number", "optional": false}, {"name": "submittedAt", "type": "number", "optional": false}]),
        |v, key| match key {
            "nonce" => v.nonce,
            "submittedAt" => v.submitted_at,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: TxHash = serde_json::from_value(oracle["complete"]["TxHash"].clone()).unwrap();
    field_type!(typed.tx_hash, String, "TxHash.txHash");
    field_type!(typed.nonce, f64, "TxHash.nonce");
    field_type!(typed.submitted_at, f64, "TxHash.submittedAt");
    assert!(
        TxHash::deserialize(Value::Array(
            oracle["complete"]["TxHash"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "TxHash public qualified map-only decoder"
    );
    check::<ServerTime>(
        &oracle,
        "ServerTime",
        json!([{"name": "serverTime", "type": "number", "optional": false}, {"name": "timezone", "type": "string", "optional": false}]),
        |v, key| match key {
            "serverTime" => v.server_time,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: ServerTime =
        serde_json::from_value(oracle["complete"]["ServerTime"].clone()).unwrap();
    field_type!(typed.server_time, f64, "ServerTime.serverTime");
    field_type!(typed.timezone, String, "ServerTime.timezone");
    assert!(
        ServerTime::deserialize(Value::Array(
            oracle["complete"]["ServerTime"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "ServerTime public qualified map-only decoder"
    );
    check::<RateLimit>(
        &oracle,
        "RateLimit",
        json!([{"name": "type", "type": "string", "optional": false}, {"name": "interval", "type": "number", "optional": false}, {"name": "limit", "type": "number", "optional": false}, {"name": "current", "type": "number", "optional": true}]),
        |v, key| match key {
            "interval" => v.interval,
            "limit" => v.limit,
            "current" => v.current.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: RateLimit = serde_json::from_value(oracle["complete"]["RateLimit"].clone()).unwrap();
    field_type!(typed.r#type, String, "RateLimit.type");
    field_type!(typed.interval, f64, "RateLimit.interval");
    field_type!(typed.limit, f64, "RateLimit.limit");
    field_type!(typed.current, Option<f64>, "RateLimit.current");
    assert!(
        RateLimit::deserialize(Value::Array(
            oracle["complete"]["RateLimit"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "RateLimit public qualified map-only decoder"
    );
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_responses_source_controls_fail_by_name() {
    for (mutation, name) in [
        ("inherited-number", "Result unsupported interface heritage"),
        ("result-code-number", "Result source schema"),
        ("key-number", "ApiKeyInfo source schema"),
        ("last-used-required", "ApiKeyInfo source schema"),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_responses_records_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_RESPONSES_MUTATION", mutation)
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
fn sdk_responses_missing_compiler_fails_loudly() {
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
fn sdk_response_leaf_records_match_live_typescript() {
    let oracle = oracle_capture();
    check::<DepositAddress>(
        &oracle,
        "DepositAddress",
        json!([{"name": "token", "type": "string", "optional": false}, {"name": "chain", "type": "string", "optional": false}, {"name": "address", "type": "string", "optional": false}, {"name": "memo", "type": "string", "optional": true}, {"name": "minDeposit", "type": "string", "optional": false}, {"name": "confirmations", "type": "number", "optional": false}]),
        |v, key| match key {
            "confirmations" => v.confirmations,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: DepositAddress =
        serde_json::from_value(oracle["complete"]["DepositAddress"].clone()).unwrap();
    field_type!(typed.token, String, "DepositAddress.token");
    field_type!(typed.chain, String, "DepositAddress.chain");
    field_type!(typed.address, String, "DepositAddress.address");
    field_type!(typed.memo, Option<String>, "DepositAddress.memo");
    field_type!(typed.min_deposit, String, "DepositAddress.minDeposit");
    field_type!(typed.confirmations, f64, "DepositAddress.confirmations");
    assert!(
        DepositAddress::deserialize(Value::Array(
            oracle["complete"]["DepositAddress"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "DepositAddress public qualified map-only decoder"
    );
    check::<TransferHistoryItem>(
        &oracle,
        "TransferHistoryItem",
        json!([{"name": "transferId", "type": "string", "optional": false}, {"name": "token", "type": "string", "optional": false}, {"name": "amount", "type": "string", "optional": false}, {"name": "from", "type": "string", "optional": false}, {"name": "to", "type": "string", "optional": false}, {"name": "memo", "type": "string", "optional": true}, {"name": "txHash", "type": "string", "optional": false}, {"name": "createdAt", "type": "number", "optional": false}]),
        |v, key| match key {
            "createdAt" => v.created_at,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: TransferHistoryItem =
        serde_json::from_value(oracle["complete"]["TransferHistoryItem"].clone()).unwrap();
    field_type!(typed.transfer_id, String, "TransferHistoryItem.transferId");
    field_type!(typed.token, String, "TransferHistoryItem.token");
    field_type!(typed.amount, String, "TransferHistoryItem.amount");
    field_type!(typed.from, String, "TransferHistoryItem.from");
    field_type!(typed.to, String, "TransferHistoryItem.to");
    field_type!(typed.memo, Option<String>, "TransferHistoryItem.memo");
    field_type!(typed.tx_hash, String, "TransferHistoryItem.txHash");
    field_type!(typed.created_at, f64, "TransferHistoryItem.createdAt");
    assert!(
        TransferHistoryItem::deserialize(Value::Array(
            oracle["complete"]["TransferHistoryItem"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "TransferHistoryItem public qualified map-only decoder"
    );
    check::<MaintenanceWindow>(
        &oracle,
        "MaintenanceWindow",
        json!([{"name": "id", "type": "string", "optional": false}, {"name": "title", "type": "string", "optional": false}, {"name": "description", "type": "string", "optional": false}, {"name": "startTime", "type": "number", "optional": false}, {"name": "endTime", "type": "number", "optional": false}, {"name": "affectedServices", "type": "string[]", "optional": false}]),
        |v, key| match key {
            "startTime" => v.start_time,
            "endTime" => v.end_time,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: MaintenanceWindow =
        serde_json::from_value(oracle["complete"]["MaintenanceWindow"].clone()).unwrap();
    field_type!(typed.id, String, "MaintenanceWindow.id");
    field_type!(typed.title, String, "MaintenanceWindow.title");
    field_type!(typed.description, String, "MaintenanceWindow.description");
    field_type!(typed.start_time, f64, "MaintenanceWindow.startTime");
    field_type!(typed.end_time, f64, "MaintenanceWindow.endTime");
    field_type!(
        typed.affected_services,
        Vec<String>,
        "MaintenanceWindow.affectedServices"
    );
    assert!(
        MaintenanceWindow::deserialize(Value::Array(
            oracle["complete"]["MaintenanceWindow"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "MaintenanceWindow public qualified map-only decoder"
    );
    check::<FeeTier>(
        &oracle,
        "FeeTier",
        json!([{"name": "tier", "type": "number", "optional": false}, {"name": "minVolume", "type": "string", "optional": false}, {"name": "feeRate", "type": "string", "optional": false}]),
        |v, key| match key {
            "tier" => v.tier,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: FeeTier = serde_json::from_value(oracle["complete"]["FeeTier"].clone()).unwrap();
    field_type!(typed.tier, f64, "FeeTier.tier");
    field_type!(typed.min_volume, String, "FeeTier.minVolume");
    field_type!(typed.fee_rate, String, "FeeTier.feeRate");
    assert!(
        FeeTier::deserialize(Value::Array(
            oracle["complete"]["FeeTier"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "FeeTier public qualified map-only decoder"
    );
    check::<FundingRate>(
        &oracle,
        "FundingRate",
        json!([{"name": "symbol", "type": "string", "optional": false}, {"name": "fundingRate", "type": "string", "optional": false}, {"name": "nextFundingTime", "type": "number", "optional": false}, {"name": "fundingInterval", "type": "number", "optional": false}, {"name": "timestamp", "type": "number", "optional": false}]),
        |v, key| match key {
            "nextFundingTime" => v.next_funding_time,
            "fundingInterval" => v.funding_interval,
            "timestamp" => v.timestamp,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: FundingRate =
        serde_json::from_value(oracle["complete"]["FundingRate"].clone()).unwrap();
    field_type!(typed.symbol, String, "FundingRate.symbol");
    field_type!(typed.funding_rate, String, "FundingRate.fundingRate");
    field_type!(typed.next_funding_time, f64, "FundingRate.nextFundingTime");
    field_type!(typed.funding_interval, f64, "FundingRate.fundingInterval");
    field_type!(typed.timestamp, f64, "FundingRate.timestamp");
    assert!(
        FundingRate::deserialize(Value::Array(
            oracle["complete"]["FundingRate"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "FundingRate public qualified map-only decoder"
    );
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_response_leaf_source_controls_fail_by_name() {
    for (mutation, name) in [
        ("deposit-amount-number", "DepositAddress source schema"),
        ("services-scalar", "MaintenanceWindow source schema"),
        ("funding-time-string", "FundingRate source schema"),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_response_leaf_records_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_RESPONSES_MUTATION", mutation)
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
fn sdk_response_container_records_match_live_typescript() {
    let oracle = oracle_capture();
    check::<AccountApiKeys>(
        &oracle,
        "AccountApiKeys",
        json!([{"name": "account", "type": "string", "optional": false}, {"name": "apiKeys", "type": "ApiKeyInfo[]", "optional": false}]),
        |_v, key| panic!("unexpected numeric field {key}"),
    );
    let typed: AccountApiKeys =
        serde_json::from_value(oracle["complete"]["AccountApiKeys"].clone()).unwrap();
    field_type!(typed.account, String, "AccountApiKeys.account");
    field_type!(typed.api_keys, Vec<ApiKeyInfo>, "AccountApiKeys.apiKeys");
    assert!(
        AccountApiKeys::deserialize(Value::Array(
            oracle["complete"]["AccountApiKeys"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "AccountApiKeys public qualified map-only decoder"
    );
    check::<ExchangeInfo>(
        &oracle,
        "ExchangeInfo",
        json!([{"name": "name", "type": "string", "optional": false}, {"name": "version", "type": "string", "optional": false}, {"name": "markets", "type": "string[]", "optional": false}, {"name": "tokens", "type": "string[]", "optional": false}, {"name": "rateLimits", "type": "RateLimit[]", "optional": false}, {"name": "serverTime", "type": "number", "optional": false}]),
        |_v, key| match key {
            "serverTime" => _v.server_time,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: ExchangeInfo =
        serde_json::from_value(oracle["complete"]["ExchangeInfo"].clone()).unwrap();
    field_type!(typed.name, String, "ExchangeInfo.name");
    field_type!(typed.version, String, "ExchangeInfo.version");
    field_type!(typed.markets, Vec<String>, "ExchangeInfo.markets");
    field_type!(typed.tokens, Vec<String>, "ExchangeInfo.tokens");
    field_type!(typed.rate_limits, Vec<RateLimit>, "ExchangeInfo.rateLimits");
    field_type!(typed.server_time, f64, "ExchangeInfo.serverTime");
    assert!(
        ExchangeInfo::deserialize(Value::Array(
            oracle["complete"]["ExchangeInfo"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "ExchangeInfo public qualified map-only decoder"
    );
}

fn check_nested_numbers<T: DeserializeOwned + Serialize + JsNumberOutput>(
    oracle: &Value,
    name: &str,
    field: &str,
    numeric_fields: &[&str],
    number: impl Fn(&T, &str) -> f64,
) {
    let witnesses: Vec<_> = oracle["nestedNumeric"][name]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["field"] == field)
        .collect();
    let expected: Vec<_> = numeric_fields
        .iter()
        .flat_map(|key| {
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
            .map(|raw| json!([field, key, raw]))
        })
        .collect();
    assert_eq!(
        witnesses
            .iter()
            .map(|w| json!([w["field"], w["childField"], w["raw"]]))
            .collect::<Vec<_>>(),
        expected,
        "{name} nested numeric inventory"
    );
    for witness in witnesses {
        let raw = witness["encoded"].as_str().unwrap();
        let value: Value = serde_json::from_str(raw).unwrap();
        let key = witness["childField"].as_str().unwrap();
        for (entry, record) in [
            ("bare raw", serde_json::from_str::<T>(raw).unwrap()),
            ("bare owned", T::deserialize(value.clone()).unwrap()),
            ("bare borrowed", T::deserialize(&value).unwrap()),
            ("shared raw", js_from_str::<T>(raw).unwrap()),
            ("shared owned", js_from_value::<T>(value.clone()).unwrap()),
            ("shared borrowed", js_from_value_ref::<T>(&value).unwrap()),
        ] {
            assert_eq!(
                format!("{:016x}", number(&record, key).to_bits()),
                witness["bits"].as_str().unwrap(),
                "{name}.{field}.{key} nested {entry} bits {}",
                witness["raw"]
            );
        }
    }
    let strings: Vec<_> = oracle["nestedStrings"][name]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["field"] == field)
        .collect();
    assert_eq!(
        strings
            .iter()
            .map(|w| w["units"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!([]),
            json!([112, 108, 97, 105, 110]),
            json!([233]),
            json!([0xd83d, 0xdc69]),
            json!([0xfeff]),
            json!([0x85]),
            json!([0xd800]),
            json!([0xdfff]),
            json!([120, 0xd800, 121])
        ],
        "{name} nested string inventory"
    );
    for witness in strings {
        let raw = witness["encoded"].as_str().unwrap();
        let result = js_from_str::<T>(raw);
        if witness["representable"] == true {
            let expected: Value = serde_json::from_str(raw).unwrap();
            assert_eq!(
                serde_json::to_value(result.unwrap()).unwrap(),
                expected,
                "{name} nested UTF16 retention"
            );
        } else {
            typed_utf16::assert_projection(raw, &result.unwrap());
        }
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_response_containers_retain_nested_numeric_bits() {
    let oracle = oracle_capture();
    check_nested_numbers::<AccountApiKeys>(
        &oracle,
        "AccountApiKeys",
        "apiKeys",
        &["index", "permissions", "createdAt", "lastUsedAt"],
        |v, key| match key {
            "index" => v.api_keys[0].index,
            "permissions" => v.api_keys[0].permissions,
            "createdAt" => v.api_keys[0].created_at,
            "lastUsedAt" => v.api_keys[0].last_used_at.unwrap(),
            _ => panic!("unexpected nested field {key}"),
        },
    );
    check_nested_numbers::<ExchangeInfo>(
        &oracle,
        "ExchangeInfo",
        "rateLimits",
        &["interval", "limit", "current"],
        |v, key| match key {
            "interval" => v.rate_limits[0].interval,
            "limit" => v.rate_limits[0].limit,
            "current" => v.rate_limits[0].current.unwrap(),
            _ => panic!("unexpected nested field {key}"),
        },
    );
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_response_container_source_controls_fail_by_name() {
    for (mutation, name) in [
        ("api-keys-scalar", "AccountApiKeys source schema"),
        ("rate-limits-scalar", "ExchangeInfo source schema"),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_response_container_records_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_RESPONSES_MUTATION", mutation)
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
fn sdk_response_dictionary_records_match_live_typescript() {
    let oracle = oracle_capture();
    check::<ErrorResponse>(
        &oracle,
        "ErrorResponse",
        json!([{"name": "code", "type": "ResultCode", "optional": false}, {"name": "message", "type": "string", "optional": false}, {"name": "details", "type": "string", "optional": true}, {"name": "fieldErrors", "type": "Record<string, string>", "optional": true}, {"name": "requestId", "type": "string", "optional": true}, {"name": "timestamp", "type": "number", "optional": false}]),
        |v, key| match key {
            "code" => v.code.0,
            "timestamp" => v.timestamp,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: ErrorResponse =
        serde_json::from_value(oracle["complete"]["ErrorResponse"].clone()).unwrap();
    field_type!(typed.code, ResultCode, "ErrorResponse.code");
    field_type!(typed.message, String, "ErrorResponse.message");
    field_type!(typed.details, Option<String>, "ErrorResponse.details");
    field_type!(
        typed.field_errors,
        Option<safe_cex_types::object_map::JsObjectMap<String>>,
        "ErrorResponse.fieldErrors"
    );
    field_type!(typed.request_id, Option<String>, "ErrorResponse.requestId");
    field_type!(typed.timestamp, f64, "ErrorResponse.timestamp");
    assert!(
        ErrorResponse::deserialize(Value::Array(
            oracle["complete"]["ErrorResponse"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "ErrorResponse public qualified map-only decoder"
    );
    check_dictionary::<ErrorResponse>(&oracle, "ErrorResponse", "fieldErrors", |v| {
        v.field_errors.as_ref().unwrap()
    });
    check::<FeeSchedule>(
        &oracle,
        "FeeSchedule",
        json!([{"name": "makerTiers", "type": "FeeTier[]", "optional": false}, {"name": "takerTiers", "type": "FeeTier[]", "optional": false}, {"name": "withdrawalFees", "type": "Record<string, string>", "optional": false}, {"name": "depositFees", "type": "Record<string, string>", "optional": false}]),
        |_v, key| panic!("unexpected numeric field {key}"),
    );
    let typed: FeeSchedule =
        serde_json::from_value(oracle["complete"]["FeeSchedule"].clone()).unwrap();
    field_type!(typed.maker_tiers, Vec<FeeTier>, "FeeSchedule.makerTiers");
    field_type!(typed.taker_tiers, Vec<FeeTier>, "FeeSchedule.takerTiers");
    field_type!(
        typed.withdrawal_fees,
        safe_cex_types::object_map::JsObjectMap<String>,
        "FeeSchedule.withdrawalFees"
    );
    field_type!(
        typed.deposit_fees,
        safe_cex_types::object_map::JsObjectMap<String>,
        "FeeSchedule.depositFees"
    );
    assert!(
        FeeSchedule::deserialize(Value::Array(
            oracle["complete"]["FeeSchedule"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "FeeSchedule public qualified map-only decoder"
    );
    check_dictionary::<FeeSchedule>(&oracle, "FeeSchedule", "withdrawalFees", |v| {
        &v.withdrawal_fees
    });
    check_dictionary::<FeeSchedule>(&oracle, "FeeSchedule", "depositFees", |v| &v.deposit_fees);
    check::<InsuranceFund>(
        &oracle,
        "InsuranceFund",
        json!([{"name": "totalSize", "type": "string", "optional": false}, {"name": "balances", "type": "Record<string, string>", "optional": false}, {"name": "lastUpdate", "type": "number", "optional": false}]),
        |v, key| match key {
            "lastUpdate" => v.last_update,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: InsuranceFund =
        serde_json::from_value(oracle["complete"]["InsuranceFund"].clone()).unwrap();
    field_type!(typed.total_size, String, "InsuranceFund.totalSize");
    field_type!(
        typed.balances,
        safe_cex_types::object_map::JsObjectMap<String>,
        "InsuranceFund.balances"
    );
    field_type!(typed.last_update, f64, "InsuranceFund.lastUpdate");
    assert!(
        InsuranceFund::deserialize(Value::Array(
            oracle["complete"]["InsuranceFund"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "InsuranceFund public qualified map-only decoder"
    );
    check_dictionary::<InsuranceFund>(&oracle, "InsuranceFund", "balances", |v| &v.balances);
}

fn check_dictionary<T: DeserializeOwned + Serialize + JsNumberOutput>(
    oracle: &Value,
    name: &str,
    field: &str,
    dictionary: impl Fn(&T) -> &safe_cex_types::object_map::JsObjectMap<String>,
) {
    let witnesses: Vec<_> = oracle["dictionaries"][name]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["field"] == field)
        .collect();
    assert_eq!(
        witnesses.len(),
        20,
        "{name}.{field} dictionary witness count"
    );
    for axis in ["key", "value"] {
        assert_eq!(
            witnesses
                .iter()
                .filter(|w| w["axis"] == axis)
                .map(|w| w["units"].clone())
                .collect::<Vec<_>>(),
            vec![
                json!([]),
                json!([112, 108, 97, 105, 110]),
                json!([233]),
                json!([0xd83d, 0xdc69]),
                json!([0xfeff]),
                json!([0x85]),
                json!([0xd800]),
                json!([0xdfff]),
                json!([120, 0xd800, 121])
            ],
            "{name}.{field} dictionary {axis} UTF16 inventory"
        );
    }
    for w in witnesses {
        let raw = w["encoded"].as_str().unwrap();
        if w["representable"] != true {
            typed_utf16::assert_projection(
                raw,
                &(serde_json::from_str::<T>(raw)).expect("typed surrogate record is retained"),
            );
            typed_utf16::assert_projection(
                raw,
                &(js_from_str::<T>(raw)).expect("typed surrogate record is retained"),
            );
            continue;
        }
        let value: Value = serde_json::from_str(raw).unwrap();
        let expected: Value = serde_json::from_str(w["raw"].as_str().unwrap()).unwrap();
        for (entry, decoded) in [
            ("bare raw", serde_json::from_str::<T>(raw).unwrap()),
            ("bare owned", T::deserialize(value.clone()).unwrap()),
            ("bare borrowed", T::deserialize(&value).unwrap()),
            ("shared raw", js_from_str::<T>(raw).unwrap()),
            ("shared owned", js_from_value::<T>(value.clone()).unwrap()),
            ("shared borrowed", js_from_value_ref::<T>(&value).unwrap()),
        ] {
            let map = dictionary(&decoded);
            assert_eq!(
                json!(map.keys().collect::<Vec<_>>()),
                w["keys"],
                "{name}.{field} {entry} dictionary key order"
            );
            for (key, val) in map {
                assert_eq!(
                    val,
                    expected[key].as_str().unwrap(),
                    "{name}.{field} {entry} dictionary value {key}"
                );
            }
            let encoded = serde_json::to_value(&decoded).unwrap();
            assert_eq!(
                json!(
                    encoded[field]
                        .as_object()
                        .unwrap()
                        .keys()
                        .collect::<Vec<_>>()
                ),
                w["keys"],
                "{name}.{field} {entry} emitted key order"
            );
        }
    }
    for bad in [
        json!(null),
        json!([]),
        json!(false),
        json!(1),
        json!("map"),
        json!({"key":null}),
        json!({"key":1}),
        json!({"key":false}),
        json!({"key":{}}),
        json!({"key":[]}),
    ] {
        let mut value = oracle["complete"][name].clone();
        value[field] = bad;
        assert!(
            T::deserialize(value).is_err(),
            "{name}.{field} invalid dictionary input"
        );
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_fee_schedule_retains_nested_tier_bits() {
    let oracle = oracle_capture();
    check_nested_numbers::<FeeSchedule>(&oracle, "FeeSchedule", "makerTiers", &["tier"], |v, _| {
        v.maker_tiers[0].tier
    });
    check_nested_numbers::<FeeSchedule>(&oracle, "FeeSchedule", "takerTiers", &["tier"], |v, _| {
        v.taker_tiers[0].tier
    });
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_response_dictionary_source_controls_fail_by_name() {
    for (mutation, name) in [
        ("error-fields-number", "ErrorResponse source schema"),
        ("fee-tiers-scalar", "FeeSchedule source schema"),
        ("fund-balances-number", "InsuranceFund source schema"),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_response_dictionary_records_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_RESPONSES_MUTATION", mutation)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(101), "{mutation}: {text}");
        assert!(text.contains(name), "{mutation}: {text}");
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_status_records_match_live_typescript() {
    let oracle = oracle_capture();
    check::<HealthResponse>(
        &oracle,
        "HealthResponse",
        json!([{"name": "status", "type": "\"healthy\" | \"degraded\" | \"unhealthy\"", "optional": false}, {"name": "version", "type": "string", "optional": false}, {"name": "timestamp", "type": "number", "optional": false}, {"name": "uptime", "type": "number", "optional": false}, {"name": "database", "type": "\"connected\" | \"disconnected\"", "optional": true}, {"name": "websocket", "type": "\"connected\" | \"disconnected\"", "optional": true}]),
        |v, key| match key {
            "timestamp" => v.timestamp,
            "uptime" => v.uptime,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: HealthResponse =
        serde_json::from_value(oracle["complete"]["HealthResponse"].clone()).unwrap();
    field_type!(typed.status, HealthStatus, "HealthResponse.status");
    field_type!(typed.version, String, "HealthResponse.version");
    field_type!(typed.timestamp, f64, "HealthResponse.timestamp");
    field_type!(typed.uptime, f64, "HealthResponse.uptime");
    field_type!(
        typed.database,
        Option<ConnectionStatus>,
        "HealthResponse.database"
    );
    field_type!(
        typed.websocket,
        Option<ConnectionStatus>,
        "HealthResponse.websocket"
    );
    assert!(
        HealthResponse::deserialize(Value::Array(
            oracle["complete"]["HealthResponse"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "HealthResponse public qualified map-only decoder"
    );
    check_status_field::<HealthResponse, HealthStatus>(&oracle, "HealthResponse", "status");
    for literal in oracle["unions"]["HealthResponse"]["status"]
        .as_array()
        .unwrap()
    {
        assert_eq!(
            health_status_name(serde_json::from_value(literal.clone()).unwrap()),
            literal.as_str().unwrap(),
            "HealthStatus exhaustive literal mapping"
        );
    }
    for literal in oracle["unions"]["HealthResponse"]["status"]
        .as_array()
        .unwrap()
    {
        assert!(
            HealthStatus::deserialize(json!({literal.as_str().unwrap():null})).is_err(),
            "HealthStatus qualified decoder rejects tagged object"
        );
    }
    check_status_field::<HealthResponse, ConnectionStatus>(&oracle, "HealthResponse", "database");
    for literal in oracle["unions"]["HealthResponse"]["database"]
        .as_array()
        .unwrap()
    {
        assert_eq!(
            connection_status_name(serde_json::from_value(literal.clone()).unwrap()),
            literal.as_str().unwrap(),
            "ConnectionStatus exhaustive literal mapping"
        );
    }
    for literal in oracle["unions"]["HealthResponse"]["database"]
        .as_array()
        .unwrap()
    {
        assert!(
            ConnectionStatus::deserialize(json!({literal.as_str().unwrap():null})).is_err(),
            "ConnectionStatus qualified decoder rejects tagged object"
        );
    }
    check_status_field::<HealthResponse, ConnectionStatus>(&oracle, "HealthResponse", "websocket");
    for literal in oracle["unions"]["HealthResponse"]["websocket"]
        .as_array()
        .unwrap()
    {
        assert!(
            ConnectionStatus::deserialize(json!({literal.as_str().unwrap():null})).is_err(),
            "ConnectionStatus qualified decoder rejects tagged object"
        );
    }
    check::<ComponentStatus>(
        &oracle,
        "ComponentStatus",
        json!([{"name": "name", "type": "string", "optional": false}, {"name": "status", "type": "\"operational\" | \"degraded\" | \"down\"", "optional": false}, {"name": "message", "type": "string", "optional": true}, {"name": "lastCheck", "type": "number", "optional": false}]),
        |v, key| match key {
            "lastCheck" => v.last_check,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: ComponentStatus =
        serde_json::from_value(oracle["complete"]["ComponentStatus"].clone()).unwrap();
    field_type!(typed.name, String, "ComponentStatus.name");
    field_type!(typed.status, ComponentState, "ComponentStatus.status");
    field_type!(typed.message, Option<String>, "ComponentStatus.message");
    field_type!(typed.last_check, f64, "ComponentStatus.lastCheck");
    assert!(
        ComponentStatus::deserialize(Value::Array(
            oracle["complete"]["ComponentStatus"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "ComponentStatus public qualified map-only decoder"
    );
    check_status_field::<ComponentStatus, ComponentState>(&oracle, "ComponentStatus", "status");
    for literal in oracle["unions"]["ComponentStatus"]["status"]
        .as_array()
        .unwrap()
    {
        assert_eq!(
            component_state_name(serde_json::from_value(literal.clone()).unwrap()),
            literal.as_str().unwrap(),
            "ComponentState exhaustive literal mapping"
        );
    }
    for literal in oracle["unions"]["ComponentStatus"]["status"]
        .as_array()
        .unwrap()
    {
        assert!(
            ComponentState::deserialize(json!({literal.as_str().unwrap():null})).is_err(),
            "ComponentState qualified decoder rejects tagged object"
        );
    }
}

fn health_status_name(value: HealthStatus) -> &'static str {
    match value {
        HealthStatus::Healthy => "healthy",
        HealthStatus::Degraded => "degraded",
        HealthStatus::Unhealthy => "unhealthy",
    }
}

fn connection_status_name(value: ConnectionStatus) -> &'static str {
    match value {
        ConnectionStatus::Connected => "connected",
        ConnectionStatus::Disconnected => "disconnected",
    }
}

fn component_state_name(value: ComponentState) -> &'static str {
    match value {
        ComponentState::Operational => "operational",
        ComponentState::Degraded => "degraded",
        ComponentState::Down => "down",
    }
}

fn check_status_field<T: DeserializeOwned + Serialize, U: DeserializeOwned + Serialize>(
    oracle: &Value,
    name: &str,
    field: &str,
) {
    let literals = &oracle["unions"][name][field];
    assert_eq!(
        json!(support::enum_inventory::<U>()),
        *literals,
        "{name}.{field} accepted union metadata"
    );
    support::assert_string_only::<U>(literals, &format!("{name}.{field}"));
    for literal in literals.as_array().unwrap() {
        let mut complete = oracle["complete"][name].clone();
        complete[field] = literal.clone();
        assert_eq!(
            serde_json::to_value(T::deserialize(complete.clone()).unwrap()).unwrap(),
            complete,
            "{name}.{field} accepted literal record"
        );
        let decoded: U = serde_json::from_str(&literal.to_string()).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            *literal,
            "{name}.{field} accepted literal output"
        );
        for bad in [
            json!({literal.as_str().unwrap():null}),
            json!({}),
            json!([]),
            json!(false),
            json!(1),
            json!(null),
            json!("unknown"),
        ] {
            let mut value = complete.clone();
            value[field] = bad;
            assert!(
                T::deserialize(value).is_err(),
                "{name}.{field} rejected union input"
            );
        }
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_status_source_controls_fail_by_name() {
    for (mutation, name) in [
        ("health-status-extra", "HealthResponse source schema"),
        ("connection-number", "HealthResponse source schema"),
        ("component-status-extra", "ComponentStatus source schema"),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_status_records_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_RESPONSES_MUTATION", mutation)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(101), "{mutation}: {text}");
        assert!(text.contains(name), "{mutation}: {text}");
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_remaining_response_records_match_live_typescript() {
    let oracle = oracle_capture();
    check::<AccountInfoResponse>(
        &oracle,
        "AccountInfoResponse",
        json!([{"name": "address", "type": "string", "optional": false}, {"name": "accountIndex", "type": "number", "optional": false}, {"name": "nonce", "type": "number", "optional": false}, {"name": "tier", "type": "string", "optional": true}, {"name": "makerFeeRate", "type": "string", "optional": false}, {"name": "takerFeeRate", "type": "string", "optional": false}, {"name": "tradingVolume30d", "type": "string", "optional": false}, {"name": "createdAt", "type": "number", "optional": false}, {"name": "kycStatus", "type": "\"none\" | \"pending\" | \"verified\" | \"rejected\"", "optional": true}]),
        |v, key| match key {
            "accountIndex" => v.account_index,
            "nonce" => v.nonce,
            "createdAt" => v.created_at,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: AccountInfoResponse =
        serde_json::from_value(oracle["complete"]["AccountInfoResponse"].clone()).unwrap();
    field_type!(typed.address, String, "AccountInfoResponse.address");
    field_type!(typed.account_index, f64, "AccountInfoResponse.accountIndex");
    field_type!(typed.nonce, f64, "AccountInfoResponse.nonce");
    field_type!(typed.tier, Option<String>, "AccountInfoResponse.tier");
    field_type!(
        typed.maker_fee_rate,
        String,
        "AccountInfoResponse.makerFeeRate"
    );
    field_type!(
        typed.taker_fee_rate,
        String,
        "AccountInfoResponse.takerFeeRate"
    );
    field_type!(
        typed.trading_volume30d,
        String,
        "AccountInfoResponse.tradingVolume30d"
    );
    field_type!(typed.created_at, f64, "AccountInfoResponse.createdAt");
    field_type!(
        typed.kyc_status,
        Option<KycStatus>,
        "AccountInfoResponse.kycStatus"
    );
    assert!(
        AccountInfoResponse::deserialize(Value::Array(
            oracle["complete"]["AccountInfoResponse"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "AccountInfoResponse public qualified map-only decoder"
    );
    check_status_field::<AccountInfoResponse, KycStatus>(
        &oracle,
        "AccountInfoResponse",
        "kycStatus",
    );
    for literal in oracle["unions"]["AccountInfoResponse"]["kycStatus"]
        .as_array()
        .unwrap()
    {
        assert_eq!(
            kyc_status_name(serde_json::from_value(literal.clone()).unwrap()),
            literal.as_str().unwrap(),
            "KycStatus exhaustive literal mapping"
        );
    }
    for literal in oracle["unions"]["AccountInfoResponse"]["kycStatus"]
        .as_array()
        .unwrap()
    {
        assert!(
            KycStatus::deserialize(json!({literal.as_str().unwrap():null})).is_err(),
            "KycStatus qualified decoder rejects tagged object"
        );
    }
    check::<WithdrawalHistoryItem>(
        &oracle,
        "WithdrawalHistoryItem",
        json!([{"name": "withdrawalId", "type": "string", "optional": false}, {"name": "token", "type": "string", "optional": false}, {"name": "amount", "type": "string", "optional": false}, {"name": "fee", "type": "string", "optional": false}, {"name": "l1Address", "type": "string", "optional": false}, {"name": "txHash", "type": "string", "optional": true}, {"name": "status", "type": "\"pending\" | \"processing\" | \"completed\" | \"failed\"", "optional": false}, {"name": "createdAt", "type": "number", "optional": false}, {"name": "completedAt", "type": "number", "optional": true}]),
        |v, key| match key {
            "createdAt" => v.created_at,
            "completedAt" => v.completed_at.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: WithdrawalHistoryItem =
        serde_json::from_value(oracle["complete"]["WithdrawalHistoryItem"].clone()).unwrap();
    field_type!(
        typed.withdrawal_id,
        String,
        "WithdrawalHistoryItem.withdrawalId"
    );
    field_type!(typed.token, String, "WithdrawalHistoryItem.token");
    field_type!(typed.amount, String, "WithdrawalHistoryItem.amount");
    field_type!(typed.fee, String, "WithdrawalHistoryItem.fee");
    field_type!(typed.l1_address, String, "WithdrawalHistoryItem.l1Address");
    field_type!(
        typed.tx_hash,
        Option<String>,
        "WithdrawalHistoryItem.txHash"
    );
    field_type!(
        typed.status,
        WithdrawalState,
        "WithdrawalHistoryItem.status"
    );
    field_type!(typed.created_at, f64, "WithdrawalHistoryItem.createdAt");
    field_type!(
        typed.completed_at,
        Option<f64>,
        "WithdrawalHistoryItem.completedAt"
    );
    assert!(
        WithdrawalHistoryItem::deserialize(Value::Array(
            oracle["complete"]["WithdrawalHistoryItem"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "WithdrawalHistoryItem public qualified map-only decoder"
    );
    check_status_field::<WithdrawalHistoryItem, WithdrawalState>(
        &oracle,
        "WithdrawalHistoryItem",
        "status",
    );
    for literal in oracle["unions"]["WithdrawalHistoryItem"]["status"]
        .as_array()
        .unwrap()
    {
        assert_eq!(
            withdrawal_state_name(serde_json::from_value(literal.clone()).unwrap()),
            literal.as_str().unwrap(),
            "WithdrawalState exhaustive literal mapping"
        );
    }
    for literal in oracle["unions"]["WithdrawalHistoryItem"]["status"]
        .as_array()
        .unwrap()
    {
        assert!(
            WithdrawalState::deserialize(json!({literal.as_str().unwrap():null})).is_err(),
            "WithdrawalState qualified decoder rejects tagged object"
        );
    }
    check::<DepositHistoryItem>(
        &oracle,
        "DepositHistoryItem",
        json!([{"name": "depositId", "type": "string", "optional": false}, {"name": "token", "type": "string", "optional": false}, {"name": "amount", "type": "string", "optional": false}, {"name": "txHash", "type": "string", "optional": false}, {"name": "status", "type": "\"pending\" | \"confirming\" | \"completed\" | \"failed\"", "optional": false}, {"name": "confirmations", "type": "number", "optional": false}, {"name": "requiredConfirmations", "type": "number", "optional": false}, {"name": "createdAt", "type": "number", "optional": false}, {"name": "completedAt", "type": "number", "optional": true}]),
        |v, key| match key {
            "confirmations" => v.confirmations,
            "requiredConfirmations" => v.required_confirmations,
            "createdAt" => v.created_at,
            "completedAt" => v.completed_at.unwrap(),
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: DepositHistoryItem =
        serde_json::from_value(oracle["complete"]["DepositHistoryItem"].clone()).unwrap();
    field_type!(typed.deposit_id, String, "DepositHistoryItem.depositId");
    field_type!(typed.token, String, "DepositHistoryItem.token");
    field_type!(typed.amount, String, "DepositHistoryItem.amount");
    field_type!(typed.tx_hash, String, "DepositHistoryItem.txHash");
    field_type!(typed.status, DepositState, "DepositHistoryItem.status");
    field_type!(typed.confirmations, f64, "DepositHistoryItem.confirmations");
    field_type!(
        typed.required_confirmations,
        f64,
        "DepositHistoryItem.requiredConfirmations"
    );
    field_type!(typed.created_at, f64, "DepositHistoryItem.createdAt");
    field_type!(
        typed.completed_at,
        Option<f64>,
        "DepositHistoryItem.completedAt"
    );
    assert!(
        DepositHistoryItem::deserialize(Value::Array(
            oracle["complete"]["DepositHistoryItem"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "DepositHistoryItem public qualified map-only decoder"
    );
    check_status_field::<DepositHistoryItem, DepositState>(&oracle, "DepositHistoryItem", "status");
    for literal in oracle["unions"]["DepositHistoryItem"]["status"]
        .as_array()
        .unwrap()
    {
        assert_eq!(
            deposit_state_name(serde_json::from_value(literal.clone()).unwrap()),
            literal.as_str().unwrap(),
            "DepositState exhaustive literal mapping"
        );
    }
    for literal in oracle["unions"]["DepositHistoryItem"]["status"]
        .as_array()
        .unwrap()
    {
        assert!(
            DepositState::deserialize(json!({literal.as_str().unwrap():null})).is_err(),
            "DepositState qualified decoder rejects tagged object"
        );
    }
    check::<SystemStatus>(
        &oracle,
        "SystemStatus",
        json!([{"name": "status", "type": "\"operational\" | \"degraded\" | \"maintenance\" | \"outage\"", "optional": false}, {"name": "message", "type": "string", "optional": true}, {"name": "components", "type": "ComponentStatus[]", "optional": false}, {"name": "scheduledMaintenance", "type": "MaintenanceWindow[]", "optional": true}, {"name": "lastUpdate", "type": "number", "optional": false}]),
        |v, key| match key {
            "lastUpdate" => v.last_update,
            _ => panic!("unexpected numeric field {key}"),
        },
    );
    let typed: SystemStatus =
        serde_json::from_value(oracle["complete"]["SystemStatus"].clone()).unwrap();
    field_type!(typed.status, SystemState, "SystemStatus.status");
    field_type!(typed.message, Option<String>, "SystemStatus.message");
    field_type!(
        typed.components,
        Vec<ComponentStatus>,
        "SystemStatus.components"
    );
    field_type!(
        typed.scheduled_maintenance,
        Option<Vec<MaintenanceWindow>>,
        "SystemStatus.scheduledMaintenance"
    );
    field_type!(typed.last_update, f64, "SystemStatus.lastUpdate");
    assert!(
        SystemStatus::deserialize(Value::Array(
            oracle["complete"]["SystemStatus"]
                .as_object()
                .unwrap()
                .values()
                .cloned()
                .collect()
        ))
        .is_err(),
        "SystemStatus public qualified map-only decoder"
    );
    check_status_field::<SystemStatus, SystemState>(&oracle, "SystemStatus", "status");
    for literal in oracle["unions"]["SystemStatus"]["status"]
        .as_array()
        .unwrap()
    {
        assert_eq!(
            system_state_name(serde_json::from_value(literal.clone()).unwrap()),
            literal.as_str().unwrap(),
            "SystemState exhaustive literal mapping"
        );
    }
    for literal in oracle["unions"]["SystemStatus"]["status"]
        .as_array()
        .unwrap()
    {
        assert!(
            SystemState::deserialize(json!({literal.as_str().unwrap():null})).is_err(),
            "SystemState qualified decoder rejects tagged object"
        );
    }
}

fn kyc_status_name(value: KycStatus) -> &'static str {
    match value {
        KycStatus::None => "none",
        KycStatus::Pending => "pending",
        KycStatus::Verified => "verified",
        KycStatus::Rejected => "rejected",
    }
}

fn withdrawal_state_name(value: WithdrawalState) -> &'static str {
    match value {
        WithdrawalState::Pending => "pending",
        WithdrawalState::Processing => "processing",
        WithdrawalState::Completed => "completed",
        WithdrawalState::Failed => "failed",
    }
}

fn deposit_state_name(value: DepositState) -> &'static str {
    match value {
        DepositState::Pending => "pending",
        DepositState::Confirming => "confirming",
        DepositState::Completed => "completed",
        DepositState::Failed => "failed",
    }
}

fn system_state_name(value: SystemState) -> &'static str {
    match value {
        SystemState::Operational => "operational",
        SystemState::Degraded => "degraded",
        SystemState::Maintenance => "maintenance",
        SystemState::Outage => "outage",
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_system_status_retains_nested_numeric_bits() {
    let oracle = oracle_capture();
    check_nested_numbers::<SystemStatus>(
        &oracle,
        "SystemStatus",
        "components",
        &["lastCheck"],
        |v, _| v.components[0].last_check,
    );
    check_nested_numbers::<SystemStatus>(
        &oracle,
        "SystemStatus",
        "scheduledMaintenance",
        &["startTime", "endTime"],
        |v, field| match field {
            "startTime" => v.scheduled_maintenance.as_ref().unwrap()[0].start_time,
            "endTime" => v.scheduled_maintenance.as_ref().unwrap()[0].end_time,
            _ => panic!("unexpected nested field {field}"),
        },
    );
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_remaining_response_source_controls_fail_by_name() {
    for (mutation, name) in [
        ("kyc-status-extra", "AccountInfoResponse source schema"),
        (
            "withdrawal-fee-number",
            "WithdrawalHistoryItem source schema",
        ),
        (
            "deposit-required-string",
            "DepositHistoryItem source schema",
        ),
        ("maintenance-required", "SystemStatus source schema"),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_remaining_response_records_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_RESPONSES_MUTATION", mutation)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(101), "{mutation}: {text}");
        assert!(text.contains(name), "{mutation}: {text}");
    }
}

fn collection_cardinality<T: DeserializeOwned + Serialize + JsNumberOutput + NativeIdentity>(
    name: &str,
    field: &str,
    native_len: impl Fn(&T) -> usize,
) {
    let oracle = oracle_capture();
    let mut input = oracle["complete"][name].clone();
    for (key, value) in input.as_object_mut().unwrap() {
        if key != field && value.is_array() {
            *value = json!([]);
        }
    }
    let expected = input[field].as_array().unwrap();
    assert_eq!(
        expected.len(),
        4,
        "SDK-CARDINALITY {name}.{field} source length"
    );
    let raw = serde_json::to_string(&input).unwrap();
    let paths = [
        (
            "bare raw",
            serde_json::from_str::<T>(&raw).map_err(|e| e.to_string()),
        ),
        (
            "bare owned",
            T::deserialize(input.clone()).map_err(|e| e.to_string()),
        ),
        (
            "bare borrowed",
            T::deserialize(&input).map_err(|e| e.to_string()),
        ),
        (
            "shared raw",
            js_from_str::<T>(&raw).map_err(|e| e.to_string()),
        ),
        (
            "shared owned",
            js_from_value::<T>(input.clone()).map_err(|e| e.to_string()),
        ),
        (
            "shared borrowed",
            js_from_value_ref::<T>(&input).map_err(|e| e.to_string()),
        ),
    ];
    let mut failures = Vec::new();
    let mut compared = 0;
    for (path, result) in paths {
        let decoded = result.unwrap_or_else(|error| {
            panic!("SDK-CARDINALITY {name}.{field} {path} must decode successfully: {error}")
        });
        let actual_len = native_len(&decoded);
        if actual_len != 4 {
            failures.push(format!("{path} native length {actual_len} != 4"));
        }
        let identity = decoded.native_identity();
        let actual = identity[field].as_array().unwrap();
        if actual != expected {
            failures.push(format!("{path} native collection identity"));
        }
        for (index, item) in actual.iter().enumerate() {
            compared += 1;
            if expected.get(index) != Some(item) {
                failures.push(format!("{path} native item {index}"));
            }
        }
    }
    if compared != 24 {
        failures.push(format!("native comparisons {compared} != 24"));
    }
    assert!(
        failures.is_empty(),
        "SDK-CARDINALITY {name}.{field}: {failures:?}"
    );
}

macro_rules! collection_cardinality_test {
    ($test:ident, $typ:ty, $name:literal, $field:literal, $len:expr) => {
        #[test]
        #[ignore = "requires Node and maintained TypeScript"]
        fn $test() {
            collection_cardinality::<$typ>($name, $field, $len);
        }
    };
}
collection_cardinality_test!(
    sdk_collection_services,
    MaintenanceWindow,
    "MaintenanceWindow",
    "affectedServices",
    |v: &MaintenanceWindow| v.affected_services.len()
);
collection_cardinality_test!(
    sdk_collection_api_keys,
    AccountApiKeys,
    "AccountApiKeys",
    "apiKeys",
    |v: &AccountApiKeys| v.api_keys.len()
);
collection_cardinality_test!(
    sdk_collection_markets,
    ExchangeInfo,
    "ExchangeInfo",
    "markets",
    |v: &ExchangeInfo| v.markets.len()
);
collection_cardinality_test!(
    sdk_collection_tokens,
    ExchangeInfo,
    "ExchangeInfo",
    "tokens",
    |v: &ExchangeInfo| v.tokens.len()
);
collection_cardinality_test!(
    sdk_collection_rate_limits,
    ExchangeInfo,
    "ExchangeInfo",
    "rateLimits",
    |v: &ExchangeInfo| v.rate_limits.len()
);
collection_cardinality_test!(
    sdk_collection_maker_tiers,
    FeeSchedule,
    "FeeSchedule",
    "makerTiers",
    |v: &FeeSchedule| v.maker_tiers.len()
);
collection_cardinality_test!(
    sdk_collection_taker_tiers,
    FeeSchedule,
    "FeeSchedule",
    "takerTiers",
    |v: &FeeSchedule| v.taker_tiers.len()
);
collection_cardinality_test!(
    sdk_collection_components,
    SystemStatus,
    "SystemStatus",
    "components",
    |v: &SystemStatus| v.components.len()
);
collection_cardinality_test!(
    sdk_collection_maintenance,
    SystemStatus,
    "SystemStatus",
    "scheduledMaintenance",
    |v: &SystemStatus| v.scheduled_maintenance.as_ref().unwrap().len()
);

use unknown_depth::typed_utf16;
