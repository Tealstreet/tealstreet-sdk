use safe_cex_venue_hyperliquid::{
    api_errors::{self, ApiErrorClass, ApiErrorHost},
    json_boundaries::JsPayloadHost,
    market_helpers::*,
    spot_json::JsString,
};
use serde_json::Value;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
fn number(value: f64) -> Value {
    serde_json::json!([
        "number",
        if value.is_nan() {
            "nan".into()
        } else {
            format!("{:016x}", value.to_bits())
        }
    ])
}
fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u16)
        .collect()
}
#[derive(Debug, Clone)]
struct NativeValue(Rc<Kind>);
#[derive(Debug)]
enum Kind {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(JsString),
    Array(Vec<NativeValue>),
    Object(Vec<(JsString, NativeValue)>),
    Opaque(String),
    Error(RefCell<ErrorState>),
}
#[derive(Debug)]
struct ErrorState {
    class: String,
    name: JsString,
    message: JsString,
    own_message: bool,
    own_name: bool,
    own_code: bool,
    code: NativeValue,
    code_attrs: Option<[bool; 3]>,
}
impl NativeValue {
    fn from_native(v: &Value) -> Self {
        let kind = match v[0].as_str().unwrap() {
            "undefined" => Kind::Undefined,
            "null" => Kind::Null,
            "bool" => Kind::Bool(v[1].as_bool().unwrap()),
            "number" => Kind::Number(if v[1] == "nan" {
                f64::NAN
            } else {
                f64::from_bits(u64::from_str_radix(v[1].as_str().unwrap(), 16).unwrap())
            }),
            "string" => Kind::String(JsString(units(&v[1]))),
            "array" => Kind::Array(
                v[1].as_array()
                    .unwrap()
                    .iter()
                    .map(Self::from_native)
                    .collect(),
            ),
            "object" => Kind::Object(
                v[1].as_array()
                    .unwrap()
                    .iter()
                    .map(|p| (JsString(units(&p[0])), Self::from_native(&p[1])))
                    .collect(),
            ),
            "opaque" => Kind::Opaque(v[1].as_str().unwrap().into()),
            "error" => Kind::Error(RefCell::new(ErrorState {
                class: v[1].as_str().unwrap().into(),
                name: JsString(units(&v[2])),
                message: JsString(units(&v[3])),
                own_message: v[4].as_bool().unwrap(),
                own_name: v[5].as_bool().unwrap(),
                own_code: v[6].as_bool().unwrap(),
                code: Self::from_native(&v[7]),
                code_attrs: if v[8].is_null() {
                    None
                } else {
                    Some([
                        v[8][0].as_bool().unwrap(),
                        v[8][1].as_bool().unwrap(),
                        v[8][2].as_bool().unwrap(),
                    ])
                },
            })),
            other => panic!("unknown native kind {other}"),
        };
        Self(Rc::new(kind))
    }
    fn native(&self) -> Value {
        match self.0.as_ref() {
            Kind::Undefined => serde_json::json!(["undefined"]),
            Kind::Null => serde_json::json!(["null"]),
            Kind::Bool(v) => serde_json::json!(["bool", v]),
            Kind::Number(v) => number(*v),
            Kind::String(v) => serde_json::json!(["string", v.0]),
            Kind::Array(v) => {
                serde_json::json!(["array", v.iter().map(Self::native).collect::<Vec<_>>()])
            }
            Kind::Object(v) => serde_json::json!([
                "object",
                v.iter()
                    .map(|(k, v)| serde_json::json!([k.0, v.native()]))
                    .collect::<Vec<_>>()
            ]),
            Kind::Opaque(v) => serde_json::json!(["opaque", v]),
            Kind::Error(v) => {
                let s = v.borrow();
                serde_json::json!([
                    "error",
                    s.class,
                    s.name.0,
                    s.message.0,
                    s.own_message,
                    s.own_name,
                    s.own_code,
                    s.code.native(),
                    s.code_attrs
                ])
            }
        }
    }
    fn numeric(&self) -> f64 {
        match self.0.as_ref() {
            Kind::Number(v) => *v,
            other => panic!("expected native number {other:?}"),
        }
    }
    fn text(&self) -> JsString {
        match self.0.as_ref() {
            Kind::String(v) => v.clone(),
            other => panic!("expected native string {other:?}"),
        }
    }
}
#[derive(Debug)]
enum Exception {
    Opaque {
        identity: usize,
        native: NativeValue,
    },
    Value(NativeValue),
}
impl Exception {
    fn native(&self) -> Value {
        match self {
            Self::Opaque { native, .. } | Self::Value(native) => native.native(),
        }
    }
}
struct Host {
    script: VecDeque<Value>,
    reads: Vec<String>,
    case: String,
    last_parsed: Option<NativeValue>,
    last_pair: Option<[NativeValue; 2]>,
}
impl Host {
    fn take_raw(&mut self, name: &str, arg: Value) -> Result<Value, Exception> {
        self.reads.push(name.into());
        let row = self
            .script
            .pop_front()
            .unwrap_or_else(|| panic!("HELPER unexpected host operation {name}"));
        assert_eq!(row["name"], name, "HELPER native operation order");
        assert_eq!(
            row["arg"], arg,
            "HELPER native receiver/argument {} {name}",
            self.case
        );
        if row["error"].is_object() {
            return Err(Exception::Opaque {
                identity: row["error"]["id"].as_u64().unwrap() as usize,
                native: NativeValue::from_native(&row["error"]["native"]),
            });
        }
        Ok(row["result"].clone())
    }
    fn take(&mut self, name: &str, arg: Value) -> Result<NativeValue, Exception> {
        self.take_raw(name, arg)
            .map(|v| NativeValue::from_native(&v))
    }
    fn assign(
        &mut self,
        target: &NativeValue,
        key: &str,
        value: &NativeValue,
        define: bool,
    ) -> Result<(), Exception> {
        let operation = if define {
            format!("define:{key}")
        } else {
            format!("set:{key}")
        };
        let result = self.take(
            &operation,
            serde_json::json!({"target":target.native(),"value":value.native()}),
        )?;
        assert!(matches!(result.0.as_ref(), Kind::Bool(true)));
        let Kind::Error(state) = target.0.as_ref() else {
            panic!("expected native Error target")
        };
        let mut state = state.borrow_mut();
        match key {
            "code" => {
                state.code = value.clone();
                state.own_code = true;
                if define || state.code_attrs.is_none() {
                    state.code_attrs = Some([true, true, true]);
                }
            }
            "name" => {
                state.name = value.text();
                state.own_name = true
            }
            other => panic!("unexpected error field {other}"),
        };
        Ok(())
    }
}
impl JsPayloadHost for Host {
    type Value = NativeValue;
    type Error = Exception;
    fn string_units<'a>(&self, v: &'a NativeValue) -> Option<&'a [u16]> {
        match v.0.as_ref() {
            Kind::String(s) => Some(&s.0),
            _ => None,
        }
    }
    fn is_object(&self, v: &NativeValue) -> bool {
        matches!(
            v.0.as_ref(),
            Kind::Object(_) | Kind::Array(_) | Kind::Error(_)
        )
    }
    fn is_array(&mut self, v: &NativeValue) -> Result<bool, Exception> {
        let result = self.take("isArray", v.native())?;
        match result.0.as_ref() {
            Kind::Bool(b) => Ok(*b),
            _ => panic!("invalid native array classification"),
        }
    }
    fn is_nullish(&self, v: &NativeValue) -> bool {
        matches!(v.0.as_ref(), Kind::Null | Kind::Undefined)
    }
    fn undefined(&self) -> NativeValue {
        NativeValue(Rc::new(Kind::Undefined))
    }
    fn parse_json(&mut self, text: &[u16]) -> Result<NativeValue, Exception> {
        let value = self.take("parse", serde_json::json!(text))?;
        self.last_parsed = Some(value.clone());
        Ok(value)
    }
    fn get(&mut self, v: &NativeValue, key: &str) -> Result<NativeValue, Exception> {
        self.take(&format!("get:{key}"), v.native())
    }
    fn json_stringify(&mut self, _v: &NativeValue) -> Result<Option<JsString>, Exception> {
        panic!("unused host stringify")
    }
    fn to_string(&mut self, v: &NativeValue) -> Result<JsString, Exception> {
        self.take("string", v.native()).map(|v| v.text())
    }
}
impl ApiErrorHost for Host {
    fn new_error(
        &mut self,
        class: ApiErrorClass,
        message: &NativeValue,
    ) -> Result<NativeValue, Exception> {
        let class = match class {
            ApiErrorClass::Core => "core",
            ApiErrorClass::Spot => "spot",
            ApiErrorClass::Intrinsic => "intrinsic",
        };
        self.take(
            "newError",
            serde_json::json!({"class":class,"message":message.native()}),
        )
    }
    fn define_field(
        &mut self,
        target: &NativeValue,
        key: &str,
        value: &NativeValue,
    ) -> Result<(), Exception> {
        self.assign(target, key, value, true)
    }
    fn set_property(
        &mut self,
        target: &NativeValue,
        key: &str,
        value: &NativeValue,
    ) -> Result<(), Exception> {
        self.assign(target, key, value, false)
    }
    fn literal(&self, text: &str) -> NativeValue {
        NativeValue(Rc::new(Kind::String(JsString(
            text.encode_utf16().collect(),
        ))))
    }
    fn truthy(&self, v: &NativeValue) -> bool {
        match v.0.as_ref() {
            Kind::Null | Kind::Undefined => false,
            Kind::Bool(b) => *b,
            Kind::Number(n) => *n != 0.0 && !n.is_nan(),
            Kind::String(s) => !s.0.is_empty(),
            _ => true,
        }
    }
    fn exception_from_value(&self, v: NativeValue) -> Exception {
        Exception::Value(v)
    }
}
impl MarketScalarHost for Host {
    fn number_value(&self, v: &NativeValue) -> Option<f64> {
        match v.0.as_ref() {
            Kind::Number(n) => Some(*n),
            _ => None,
        }
    }
    fn value_by_key(&mut self, v: &NativeValue, key: &str) -> Result<NativeValue, Exception> {
        self.take(&format!("v:{key}"), v.native())
    }
    fn parse_float(&mut self, text: &[u16]) -> Result<f64, Exception> {
        self.take("parseFloat", serde_json::json!(text))
            .map(|v| v.numeric())
    }
    fn lowercase(&mut self, text: &[u16]) -> Result<JsString, Exception> {
        self.take("lowercase", serde_json::json!(text))
            .map(|v| v.text())
    }
    fn number_to_string(&mut self, n: f64) -> Result<Option<JsString>, Exception> {
        let v = self.take("numberToString", number(n))?;
        Ok(match v.0.as_ref() {
            Kind::Undefined => None,
            Kind::String(s) => Some(s.clone()),
            _ => panic!("invalid formatter output"),
        })
    }
    fn precise_compare(
        &mut self,
        kind: PreciseComparison,
        left: &[u16],
        right: &str,
    ) -> Result<bool, Exception> {
        let op = match kind {
            PreciseComparison::Equal => "Equal",
            PreciseComparison::Greater => "Greater",
            PreciseComparison::Less => "Less",
        };
        let v = self.take(&format!("precise:{op}"), serde_json::json!([left, right]))?;
        match v.0.as_ref() {
            Kind::Bool(b) => Ok(*b),
            _ => panic!("invalid Precise result"),
        }
    }
    fn parse_to_int(&mut self, n: f64) -> Result<f64, Exception> {
        self.take("parseToInt", number(n)).map(|v| v.numeric())
    }
}
impl FastAssetCtxHost for Host {
    fn atob(&mut self, text: &[u16]) -> Result<JsString, Exception> {
        self.take("atob", serde_json::json!(text)).map(|v| v.text())
    }
    fn inflate_raw(&mut self, bytes: &[u8]) -> Result<Vec<u8>, Exception> {
        let v = self.take_raw("inflateRaw", serde_json::json!(bytes))?;
        assert_eq!(v[0], "bytes");
        Ok(v[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect())
    }
    fn decode_text(&mut self, bytes: &[u8]) -> Result<JsString, Exception> {
        self.take("decodeText", serde_json::json!(bytes))
            .map(|v| v.text())
    }
}
impl MetaTupleHost for Host {
    fn first_two(&mut self, data: &NativeValue) -> Result<[NativeValue; 2], Exception> {
        let result = self.take("firstTwo", data.native())?;
        let Kind::Array(values) = result.0.as_ref() else {
            panic!("bad iterator result")
        };
        assert_eq!(values.len(), 2);
        let pair = [values[0].clone(), values[1].clone()];
        self.last_pair = Some(pair.clone());
        Ok(pair)
    }
    fn type_of(&self, value: &NativeValue) -> &'static str {
        match value.0.as_ref() {
            Kind::Undefined => "undefined",
            Kind::Null | Kind::Array(_) | Kind::Object(_) | Kind::Error(_) => "object",
            Kind::Bool(_) => "boolean",
            Kind::Number(_) => "number",
            Kind::String(_) => "string",
            Kind::Opaque(kind) => match kind.as_str() {
                "function" => "function",
                "bigint" => "bigint",
                "symbol" => "symbol",
                _ => panic!("unknown typeof"),
            },
        }
    }
    fn implicit_string(&mut self, value: &NativeValue) -> Result<JsString, Exception> {
        self.take("implicitString", value.native())
            .map(|v| v.text())
    }
    fn log_warning(&mut self, message: &JsString) -> Result<(), Exception> {
        let value = self.take("warning", serde_json::json!([message.0, "warning"]))?;
        assert!(matches!(value.0.as_ref(), Kind::Undefined));
        Ok(())
    }
}
fn source() -> Value {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/error-market-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "HELPER source:{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn check(row: &Value) {
    let name = row["name"].as_str().unwrap();
    let args: Vec<_> = row["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(NativeValue::from_native)
        .collect();
    let script = row["calls"].as_array().unwrap().clone();
    let mut host = Host {
        script: script.clone().into(),
        reads: Vec::new(),
        case: name.into(),
        last_parsed: None,
        last_pair: None,
    };
    let mut decoded_identity = None;
    let mut tuple_identity = None;
    let result: Result<Value, Exception> = match name {
        "core.class" | "core.constructor" => {
            api_errors::core::HyperliquidAPIError::new(&mut host, &args[0], &args[1])
                .map(|e| e.native.native())
        }
        "spot.class" | "spot.constructor" => {
            api_errors::spot::HyperliquidAPIError::new(&mut host, &args[0], &args[1])
                .map(|e| e.native.native())
        }
        "core.handleApiError" => {
            api_errors::core::handle_api_error(&mut host, &args[0]).map(|never| match never {})
        }
        "spot.handleApiError" => {
            api_errors::spot::handle_api_error(&mut host, &args[0]).map(|never| match never {})
        }
        "symbolToCoin" => {
            symbol_to_coin(&mut host, &args[0].text()).map(|v| serde_json::json!(["string", v.0]))
        }
        "calculatePricePrecision" => calculate_price_precision(
            &mut host,
            args[0].numeric(),
            args[1].numeric(),
            args[2].numeric(),
        )
        .map(number),
        "parseHyperliquidVolumeFields" => {
            parse_hyperliquid_volume_fields(&mut host, &args[0], args[1].numeric()).map(|v| {
                serde_json::json!([
                    "object",
                    [
                        [
                            "volume".encode_utf16().collect::<Vec<_>>(),
                            number(v.volume)
                        ],
                        [
                            "quoteVolume".encode_utf16().collect::<Vec<_>>(),
                            number(v.quote_volume)
                        ]
                    ]
                ])
            })
        }
        "parseHyperliquidOpenInterestNotional" => {
            parse_hyperliquid_open_interest_notional(&mut host, &args[0], args[1].numeric())
                .map(number)
        }
        "decodeHyperliquidFastAssetCtxs" => decode_hyperliquid_fast_asset_ctxs(&mut host, &args[0])
            .map(|v| {
                decoded_identity = Some(Rc::ptr_eq(&v.0, &host.last_parsed.as_ref().unwrap().0));
                v.native()
            }),
        "parseMetaAndAssetCtxsResponse" => parse_meta_and_asset_ctxs_response(
            &mut host,
            &args[0],
            &args[1].text(),
            &args[2].text(),
        )
        .map(|value| match value {
            None => serde_json::json!(["null"]),
            Some(pair) => {
                let original = host.last_pair.as_ref().unwrap();
                tuple_identity = Some(
                    Rc::ptr_eq(&pair[0].0, &original[0].0)
                        && Rc::ptr_eq(&pair[1].0, &original[1].0),
                );
                serde_json::json!(["array", [pair[0].native(), pair[1].native()]])
            }
        }),
        other => panic!("unknown helper{other}"),
    };
    if row["error"].is_object() {
        let error = result.unwrap_err();
        assert_eq!(
            error.native(),
            row["error"]["native"],
            "HELPER native error {name} {}",
            row["label"]
        );
        if let Exception::Opaque { identity, .. } = error {
            assert_eq!(
                identity,
                row["error"]["id"].as_u64().unwrap() as usize,
                "HELPER native opaque error identity {name}"
            );
        }
    } else {
        assert_eq!(
            result.unwrap(),
            row["native"],
            "HELPER native result {name} {}",
            row["label"]
        );
    }
    if let Some(identity) = tuple_identity {
        assert_eq!(
            identity,
            row["sameMembers"].as_bool().unwrap(),
            "HELPER native tuple member identity"
        );
    }
    if let Some(identity) = decoded_identity {
        assert_eq!(
            identity,
            row["sameParsed"].as_bool().unwrap(),
            "HELPER native decoder object identity"
        );
    }
    assert!(
        host.script.is_empty(),
        "HELPER unused native operations {name}"
    );
    assert_eq!(
        host.reads,
        script
            .iter()
            .map(|r| r["name"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        "HELPER native operation sequence {name}"
    );
}
#[test]
fn native_twelve_new_symbols_match_actual_source() {
    const FUNCTIONS: [&str; 12] = [
        "core.class",
        "core.constructor",
        "core.handleApiError",
        "spot.class",
        "spot.constructor",
        "spot.handleApiError",
        "symbolToCoin",
        "calculatePricePrecision",
        "parseHyperliquidVolumeFields",
        "parseHyperliquidOpenInterestNotional",
        "decodeHyperliquidFastAssetCtxs",
        "parseMetaAndAssetCtxsResponse",
    ];
    let src = source();
    assert_eq!(src["functions"], serde_json::json!(FUNCTIONS));
    assert_eq!(src["iterationStop"], 274);
    let cases = src["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 275, "HELPER independent corpus floor");
    let mut failures = Vec::new();
    const EXPECTED: [usize; 12] = [14, 14, 26, 14, 14, 26, 12, 57, 26, 26, 21, 25];
    for (name, expected) in FUNCTIONS.into_iter().zip(EXPECTED) {
        let rows: Vec<_> = cases.iter().filter(|r| r["name"] == name).collect();
        assert_eq!(
            rows.len(),
            expected,
            "HELPER independent symbol floor{name}"
        );
        if std::panic::catch_unwind(|| {
            for row in rows {
                check(row);
            }
        })
        .is_err()
        {
            failures.push(name);
        }
    }
    assert!(
        failures.is_empty(),
        "HELPER named native failures:{failures:?}"
    );
}
#[test]
fn native_constructor_keeps_code_handle_and_utf16_message_slots() {
    let code = NativeValue::from_native(&serde_json::json!(["object", []]));
    let msg = NativeValue::from_native(&serde_json::json!(["string", [0xd800, 0x58]]));
    let base = serde_json::json!([
        "error",
        "core",
        "Error".encode_utf16().collect::<Vec<_>>(),
        [0xd800, 0x58],
        true,
        false,
        false,
        ["undefined"],
        null
    ]);
    let with_slot = serde_json::json!([
        "error",
        "core",
        "Error".encode_utf16().collect::<Vec<_>>(),
        [0xd800, 0x58],
        true,
        false,
        true,
        ["undefined"],
        [true, true, true]
    ]);
    let with_code = serde_json::json!([
        "error",
        "core",
        "Error".encode_utf16().collect::<Vec<_>>(),
        [0xd800, 0x58],
        true,
        false,
        true,
        code.native(),
        [true, true, true]
    ]);
    let rows = vec![
        serde_json::json!({"name":"newError","arg":{"class":"core","message":msg.native()},"result":base}),
        serde_json::json!({"name":"define:code","arg":{"target":base,"value":["undefined"]},"result":["bool",true]}),
        serde_json::json!({"name":"set:code","arg":{"target":with_slot,"value":code.native()},"result":["bool",true]}),
        serde_json::json!({"name":"set:name","arg":{"target":with_code,"value":["string","HyperliquidAPIError".encode_utf16().collect::<Vec<_>>()]},"result":["bool",true]}),
    ];
    let mut host = Host {
        script: rows.into(),
        reads: Vec::new(),
        case: "manual constructor".into(),
        last_parsed: None,
        last_pair: None,
    };
    let error = api_errors::core::HyperliquidAPIError::new(&mut host, &code, &msg).unwrap();
    let Kind::Error(state) = error.native.0.as_ref() else {
        panic!("native Error missing")
    };
    let state = state.borrow();
    assert!(
        Rc::ptr_eq(&state.code.0, &code.0),
        "HELPER manual native code identity"
    );
    assert_eq!(
        state.message.0,
        vec![0xd800, 0x58],
        "HELPER manual native message units"
    );
    assert_eq!(state.class, "core", "HELPER manual native class prototype");
    assert!(
        state.own_message && state.own_code && state.own_name,
        "HELPER manual native field presence"
    );
}
