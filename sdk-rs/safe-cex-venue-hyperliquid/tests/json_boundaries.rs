use safe_cex_venue_hyperliquid::{
    json_boundaries::*, json_models::*, json_values::*, spot_json::JsString,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{collections::VecDeque, rc::Rc};
trait Native {
    fn native(&self) -> Value;
}
fn bits(value: &f64) -> Value {
    serde_json::json!(format!("{:016x}", value.to_bits()))
}
impl Native for JsString {
    fn native(&self) -> Value {
        serde_json::json!(self.0)
    }
}
impl<T: Native> Native for JsonObject<T> {
    fn native(&self) -> Value {
        serde_json::json!([
            "object",
            self.0
                .iter()
                .map(|(k, v)| serde_json::json!([k.0, v.native()]))
                .collect::<Vec<_>>()
        ])
    }
}
impl Native for JsonValue {
    fn native(&self) -> Value {
        match self {
            Self::Null => serde_json::json!(["null"]),
            Self::Bool(v) => serde_json::json!(["bool", v]),
            Self::Number(v) => serde_json::json!(["number", bits(v)]),
            Self::String(v) => serde_json::json!(["string", v.0]),
            Self::Array(v) => {
                serde_json::json!(["array", v.iter().map(Native::native).collect::<Vec<_>>()])
            }
            Self::Object(v) => v.native(),
        }
    }
}
impl Native for JsonContainer {
    fn native(&self) -> Value {
        match self {
            Self::Object(v) => v.native(),
            Self::Array(v) => {
                serde_json::json!(["array", v.iter().map(Native::native).collect::<Vec<_>>()])
            }
        }
    }
}
include!("fixtures/json-model-native.rs");
include!("fixtures/json-model-dispatch.rs");
fn source(script: &str) -> Value {
    let output = std::process::Command::new("node")
        .arg(format!("{}/scripts/{script}", env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "JSONBOUND source {script}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn check_model<T: DeserializeOwned + Native>(row: &Value) {
    let name = row["name"].as_str().unwrap();
    let raw = row["input"].as_str().unwrap();
    if row["accepted"] == true {
        let v = serde_json::from_str::<T>(raw)
            .unwrap_or_else(|e| panic!("JSONMODEL native accepted {name}: {e}"));
        assert_eq!(
            v.native(),
            row["native"],
            "JSONMODEL native {name} from_str mode {}",
            row["mode"]
        );
        let v = serde_json::from_slice::<T>(raw.as_bytes()).unwrap();
        assert_eq!(
            v.native(),
            row["native"],
            "JSONMODEL native {name} from_slice mode {}",
            row["mode"]
        );
    } else {
        assert!(
            serde_json::from_str::<T>(raw).is_err(),
            "JSONMODEL reject {name}"
        );
        assert!(
            serde_json::from_slice::<T>(raw.as_bytes()).is_err(),
            "JSONMODEL slice reject {name}"
        );
    }
}
#[test]
fn native_eight_models_match_actual_strict_source() {
    const ROOTS: [&str; 8] = [
        "AllMids",
        "UserRateLimit",
        "OrderStatus",
        "HyperliquidFastAssetCtx",
        "HyperliquidFastAssetCtxs",
        "PrePlacementEntry",
        "PendingOrderEntry",
        "AgentWalletCreationResult",
    ];
    let src = source("json-models-oracle.cjs");
    assert_eq!(src["roots"], serde_json::json!(ROOTS));
    assert_eq!(src["iterationStop"], 70);
    let cases = src["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 71, "JSONMODEL independent case floor");
    assert_eq!(
        cases.iter().filter(|r| r["accepted"] == true).count(),
        39,
        "JSONMODEL32accepted plus7witnesses"
    );
    let mut failures = Vec::new();
    for name in ROOTS {
        let rows: Vec<_> = cases.iter().filter(|r| r["name"] == name).collect();
        assert!(rows.len() >= 8, "JSONMODEL independent root floor {name}");
        if std::panic::catch_unwind(|| {
            for row in rows {
                model_dispatch(row);
            }
        })
        .is_err()
        {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "JSONMODEL named native failures:{failures:?}"
    );
}

#[derive(Debug, Clone)]
struct HostValue(Rc<HostKind>);
#[derive(Debug)]
enum HostKind {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(JsString),
    Array(Vec<HostValue>),
    Object(Vec<(JsString, HostValue)>),
    Opaque(String),
}
impl HostValue {
    fn from_native(value: &Value) -> Self {
        let units = |v: &Value| {
            JsString(
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u16)
                    .collect(),
            )
        };
        let kind = match value[0].as_str().unwrap() {
            "undefined" => HostKind::Undefined,
            "null" => HostKind::Null,
            "bool" => HostKind::Bool(value[1].as_bool().unwrap()),
            "number" => HostKind::Number(f64::from_bits(
                u64::from_str_radix(value[1].as_str().unwrap(), 16).unwrap(),
            )),
            "string" => HostKind::String(units(&value[1])),
            "array" => HostKind::Array(
                value[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Self::from_native)
                    .collect(),
            ),
            "object" => HostKind::Object(
                value[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| (units(&v[0]), Self::from_native(&v[1])))
                    .collect(),
            ),
            "opaque" => HostKind::Opaque(value[1].as_str().unwrap().into()),
            other => panic!("unknown native host tag {other}"),
        };
        Self(Rc::new(kind))
    }
}
impl Native for HostValue {
    fn native(&self) -> Value {
        match self.0.as_ref() {
            HostKind::Undefined => serde_json::json!(["undefined"]),
            HostKind::Null => serde_json::json!(["null"]),
            HostKind::Bool(v) => serde_json::json!(["bool", v]),
            HostKind::Number(v) => serde_json::json!(["number", bits(v)]),
            HostKind::String(v) => serde_json::json!(["string", v.0]),
            HostKind::Array(v) => {
                serde_json::json!(["array", v.iter().map(Native::native).collect::<Vec<_>>()])
            }
            HostKind::Object(v) => serde_json::json!([
                "object",
                v.iter()
                    .map(|(k, v)| serde_json::json!([k.0, v.native()]))
                    .collect::<Vec<_>>()
            ]),
            HostKind::Opaque(v) => serde_json::json!(["opaque", v]),
        }
    }
}
struct Host {
    calls: VecDeque<Value>,
    reads: Vec<String>,
}
impl Host {
    fn take(&mut self, name: &str, arg: Option<&[u16]>) -> Result<HostValue, usize> {
        self.reads.push(name.into());
        let call = self
            .calls
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected host call {name}"));
        assert_eq!(call["name"], name, "JSONBOUND fresh host operation");
        if let Some(arg) = arg {
            assert_eq!(
                call["arg"],
                serde_json::json!(arg),
                "JSONBOUND native parser input units"
            );
        }
        if let Some(error) = call["error"].as_u64() {
            return Err(error as usize);
        }
        Ok(HostValue::from_native(&call["result"]))
    }
}
impl JsPayloadHost for Host {
    type Value = HostValue;
    type Error = usize;
    fn string_units<'a>(&self, v: &'a HostValue) -> Option<&'a [u16]> {
        match v.0.as_ref() {
            HostKind::String(s) => Some(&s.0),
            _ => None,
        }
    }
    fn is_object(&self, v: &HostValue) -> bool {
        matches!(v.0.as_ref(), HostKind::Object(_) | HostKind::Array(_))
            || matches!(v.0.as_ref(),HostKind::Opaque(kind) if kind=="revoked")
    }
    fn is_array(&mut self, v: &HostValue) -> Result<bool, usize> {
        assert_eq!(
            v.native(),
            self.calls.front().unwrap()["arg"],
            "JSONBOUND native classifier receiver"
        );
        let result = self.take("isArray", None)?;
        match result.0.as_ref() {
            HostKind::Bool(value) => Ok(*value),
            other => panic!("bad host Array.isArray result {other:?}"),
        }
    }
    fn is_nullish(&self, v: &HostValue) -> bool {
        matches!(v.0.as_ref(), HostKind::Undefined | HostKind::Null)
    }
    fn undefined(&self) -> HostValue {
        HostValue(Rc::new(HostKind::Undefined))
    }
    fn parse_json(&mut self, text: &[u16]) -> Result<HostValue, usize> {
        self.take("parse", Some(text))
    }
    fn get(&mut self, v: &HostValue, p: &str) -> Result<HostValue, usize> {
        assert_eq!(
            v.native(),
            self.calls.front().unwrap()["arg"],
            "JSONBOUND native property receiver {p}"
        );
        self.take(&format!("get:{p}"), None)
    }
    fn json_stringify(&mut self, input: &HostValue) -> Result<Option<JsString>, usize> {
        assert_eq!(
            input.native(),
            self.calls.front().unwrap()["arg"],
            "JSONBOUND native stringify receiver"
        );
        let v = self.take("stringify", None)?;
        match v.0.as_ref() {
            HostKind::Undefined => Ok(None),
            HostKind::String(s) => Ok(Some(s.clone())),
            other => panic!("bad host stringify result {other:?}"),
        }
    }
    fn to_string(&mut self, input: &HostValue) -> Result<JsString, usize> {
        assert_eq!(
            input.native(),
            self.calls.front().unwrap()["arg"],
            "JSONBOUND native String receiver"
        );
        let v = self.take("string", None)?;
        match v.0.as_ref() {
            HostKind::String(s) => Ok(s.clone()),
            other => panic!("bad host String result {other:?}"),
        }
    }
}
fn check_boundary(row: &Value) {
    let name = row["name"].as_str().unwrap();
    let input = HostValue::from_native(&row["input"]);
    let scope = JsString(
        row["scope"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u16)
            .collect(),
    );
    let script = row["calls"].as_array().unwrap().clone();
    let mut host = Host {
        calls: script.clone().into(),
        reads: Vec::new(),
    };
    let mut identity = None;
    let result: Result<Value, usize> = match name {
        "parseJsonObject" | "parseHyperliquidExchangePayload" => {
            let result = if name == "parseJsonObject" {
                parse_json_object(&mut host, &input)
            } else {
                parse_hyperliquid_exchange_payload(&mut host, &input)
            };
            result.map(|v| {
                identity = v.as_ref().map(|v| Rc::ptr_eq(&v.0, &input.0));
                v.as_ref()
                    .map(Native::native)
                    .unwrap_or_else(|| serde_json::json!(["undefined"]))
            })
        }
        "maskHyperliquidAddress" => Ok(mask_hyperliquid_address(&host, &input)
            .map(|v| serde_json::json!(["string", v.0]))
            .unwrap_or_else(|| serde_json::json!(["undefined"]))),
        "stringifyDiagnosticPayload" => stringify_diagnostic_payload(&mut host, &input)
            .map(|v| serde_json::json!(["string", v.0])),
        "formatHyperliquidWsDiagnostic" => {
            format_hyperliquid_ws_diagnostic(&mut host, &scope, &input)
                .map(|v| serde_json::json!(["string", v.0]))
        }
        "shouldEmitHyperliquidWsSendDiagnostic" => {
            should_emit_hyperliquid_ws_send_diagnostic(&mut host, &input)
                .map(|v| serde_json::json!(["bool", v]))
        }
        "shouldEmitHyperliquidWsReceiveDiagnostic" => {
            should_emit_hyperliquid_ws_receive_diagnostic(&mut host, &scope, &input)
                .map(|v| serde_json::json!(["bool", v]))
        }
        other => panic!("unknown boundary {other}"),
    };
    if let Some(expected) = row["error"].as_u64() {
        assert_eq!(
            result.unwrap_err(),
            expected as usize,
            "JSONBOUND native opaque error {name}"
        );
    } else {
        assert_eq!(
            result.unwrap(),
            row["native"],
            "JSONBOUND native result {name} {}",
            row["label"]
        );
    }
    if row["input"][0] == "object" && identity.is_some() {
        assert_eq!(
            identity,
            Some(row["sameInput"].as_bool().unwrap()),
            "JSONBOUND native object identity {name}"
        );
    }
    assert!(
        host.calls.is_empty(),
        "JSONBOUND unused host operations {name}"
    );
    assert_eq!(
        host.reads,
        script
            .iter()
            .map(|v| v["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        "JSONBOUND native read order {name}"
    );
}
#[test]
fn native_seven_boundary_leaves_match_actual_hosted_source() {
    const FUNCTIONS: [&str; 7] = [
        "parseJsonObject",
        "stringifyDiagnosticPayload",
        "formatHyperliquidWsDiagnostic",
        "shouldEmitHyperliquidWsSendDiagnostic",
        "shouldEmitHyperliquidWsReceiveDiagnostic",
        "maskHyperliquidAddress",
        "parseHyperliquidExchangePayload",
    ];
    let src = source("json-boundaries-oracle.cjs");
    assert_eq!(src["functions"], serde_json::json!(FUNCTIONS));
    assert_eq!(src["iterationStop"], 214);
    let cases = src["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 215, "JSONBOUND independent floor");
    let mut failures = Vec::new();
    for name in FUNCTIONS {
        let rows: Vec<_> = cases.iter().filter(|r| r["name"] == name).collect();
        assert!(rows.len() >= 18, "JSONBOUND independent per-function floor");
        if std::panic::catch_unwind(|| {
            for row in rows {
                check_boundary(row);
            }
        })
        .is_err()
        {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "JSONBOUND named native failures:{failures:?}"
    );
}
#[test]
fn native_dictionary_keys_values_arrays_and_record_pairs() {
    let map: AllMids = serde_json::from_str(
        r#"{"10":"ten","2":"two","01":"zero","__proto__":"own","\ud800":false,"\ud800":"\udfff"}"#,
    )
    .unwrap();
    assert_eq!(
        map.0
            .iter()
            .map(|(key, _)| key.0.clone())
            .collect::<Vec<_>>(),
        vec![
            vec![50],
            vec![49, 48],
            vec![48, 49],
            "__proto__".encode_utf16().collect(),
            vec![0xd800]
        ],
        "JSONMODEL manual dictionary key order"
    );
    assert_eq!(
        map.0[4].1.0,
        vec![0xdfff],
        "JSONMODEL manual lone last value"
    );
    let rate: UserRateLimit = serde_json::from_str("[-0,1e400]").unwrap();
    match rate {
        JsonContainer::Array(v) => {
            assert!(
                matches!(v[0],JsonValue::Number(n) if n.to_bits()==(-0.0f64).to_bits()),
                "JSONMODEL manual native array zero"
            );
            assert!(
                matches!(v[1],JsonValue::Number(n) if n.to_bits()==f64::INFINITY.to_bits()),
                "JSONMODEL manual native array overflow"
            );
        }
        other => panic!("wrong native container {other:?}"),
    };
    let entry: PrePlacementEntry =
        serde_json::from_str(r#"{"type":"limit","price":91.25,"amount":17.5,"timestamp":-0}"#)
            .unwrap();
    assert_eq!(
        entry.r#type,
        CacheOrderType::Limit,
        "JSONMODEL manual order enum"
    );
    assert_eq!(
        entry.price.to_bits(),
        91.25f64.to_bits(),
        "JSONMODEL manual price bits"
    );
    assert_eq!(
        entry.amount.to_bits(),
        17.5f64.to_bits(),
        "JSONMODEL manual amount bits"
    );
    assert_eq!(
        entry.timestamp.to_bits(),
        (-0.0f64).to_bits(),
        "JSONMODEL manual timestamp bits"
    );
}
