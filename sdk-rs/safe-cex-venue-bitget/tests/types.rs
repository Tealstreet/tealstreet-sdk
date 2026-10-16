use safe_cex_venue_bitget::{protocol::*, types::*};
use serde_json::{Value, json};
fn table<T: serde::Serialize>(entries: &[(&str, T)]) -> Value {
    let map: serde_json::Map<String, Value> = entries
        .iter()
        .map(|(key, value)| ((*key).to_owned(), serde_json::to_value(value).unwrap()))
        .collect();
    assert_eq!(map.len(), entries.len(), "duplicate protocol table key");
    Value::Object(map)
}
fn constants() -> Value {
    json!({
        "RECV_WINDOW": RECV_WINDOW,
        "BROKER_ID": BROKER_ID,
        "BASE_URL": BASE_URL,
        "BASE_WS_URL": BASE_WS_URL,
        "BASE_WS_PRIVATE_URL": BASE_WS_PRIVATE_URL,
        "BASE_WS_PRIVATE_UTA_URL": BASE_WS_PRIVATE_UTA_URL,
        "ENDPOINTS": ENDPOINTS.iter().map(|entry| (entry.name.to_owned(), serde_json::to_value(entry.path).unwrap())).collect::<serde_json::Map<String, Value>>(),
        "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
        "INTERVAL": table(INTERVAL),
        "POSITION_SIDE": table(POSITION_SIDE),
        "ORDER_STATUS": table(ORDER_STATUS),
        "ORDER_TYPE": table(ORDER_TYPE),
        "ORDER_SIDE": table(ORDER_SIDE),
        "TIME_IN_FORCE": table(TIME_IN_FORCE),
    })
}
// Read derive metadata through either enum entry or the strict string decoder's
// unknown_variant error. The latter retains aliases without exposing private wire types.
#[derive(Debug)]
struct EnumInventoryError(Option<&'static [&'static str]>);
impl std::fmt::Display for EnumInventoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("enum inventory")
    }
}
impl std::error::Error for EnumInventoryError {}
impl serde::de::Error for EnumInventoryError {
    fn custom<T: std::fmt::Display>(_message: T) -> Self {
        Self(None)
    }
    fn unknown_variant(_variant: &str, expected: &'static [&'static str]) -> Self {
        Self(Some(expected))
    }
}
struct EnumInventory;
impl<'de> serde::Deserializer<'de> for EnumInventory {
    type Error = EnumInventoryError;
    fn deserialize_enum<V: serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(EnumInventoryError(Some(variants)))
    }
    fn deserialize_string<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_string("__bitget_serde_inventory_unknown__".into())
    }
    fn deserialize_str<V: serde::de::Visitor<'de>>(
        self,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        visitor.visit_borrowed_str("__bitget_serde_inventory_unknown__")
    }
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(EnumInventoryError(None))
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any
    }
}
fn check_enum<T: serde::de::DeserializeOwned>(name: &str, oracle: &Value, variants: &[&str]) {
    let mut actual = variants.to_vec();
    actual.sort();
    let mut expected: Vec<&str> = oracle["acceptedSets"][name]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    expected.sort();
    assert_eq!(actual, expected, "serde accepted-set inventory {name}");
    let mut probes = vec![
        "".to_owned(),
        "constructor".into(),
        "__proto__".into(),
        "toString".into(),
        "unknown".into(),
    ];
    for value in &expected {
        probes.extend([
            value.to_string(),
            value.to_lowercase(),
            value.to_uppercase(),
            format!(" {value}"),
            format!("{value}\u{feff}"),
            format!("{value}\u{85}"),
        ]);
    }
    for value in &expected {
        for rejected in [
            json!({(*value):null}),
            json!([value]),
            json!(null),
            json!(false),
            json!(0),
        ] {
            assert!(
                serde_json::from_value::<T>(rejected.clone()).is_err(),
                "enum rejects non-string {name}: {rejected}"
            );
        }
    }
    for value in probes {
        assert_eq!(
            serde_json::from_value::<T>(json!(value)).is_ok(),
            expected.contains(&value.as_str()),
            "enum accepted-set {name}: {value:?}"
        );
    }
}
fn check_derived_enum<T: serde::de::DeserializeOwned>(name: &str, oracle: &Value) {
    let error = T::deserialize(EnumInventory)
        .err()
        .expect("unknown inventory input must reject");
    check_enum::<T>(name, oracle, error.0.expect("serde enum metadata"));
}
fn qualified_record_rejections(oracle: &Value) {
    use serde::Deserialize;
    macro_rules! record {
        ($ty:ident, $name:literal, $pointer:literal) => {{
            let full = oracle["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["name"] == $name && c["label"] == "full")
                .unwrap();
            let object = full["value"].pointer($pointer).unwrap().clone();
            let sequence = Value::Array(object.as_object().unwrap().values().cloned().collect());
            assert!(
                $ty::deserialize(sequence).is_err(),
                "qualified record {} rejects complete positional sequence",
                stringify!($ty)
            );
            assert!(
                $ty::deserialize(json!([])).is_err(),
                "qualified record {} rejects empty sequence",
                stringify!($ty)
            );
            assert!(
                $ty::deserialize(object).is_ok(),
                "qualified record {} accepts source object",
                stringify!($ty)
            );
        }};
    }
    record!(
        FetchSingleAccountResponseData,
        "FETCH_SINGLE_ACCOUNT_RESPONSE",
        "/data"
    );
    record!(
        FetchSingleAccountResponse,
        "FETCH_SINGLE_ACCOUNT_RESPONSE",
        ""
    );
    record!(
        FetchUtaAccountAssetsResponseDataAssetsItem,
        "FETCH_UTA_ACCOUNT_ASSETS_RESPONSE",
        "/data/assets/0"
    );
    record!(
        FetchUtaAccountAssetsResponseData,
        "FETCH_UTA_ACCOUNT_ASSETS_RESPONSE",
        "/data"
    );
    record!(
        FetchUtaAccountAssetsResponse,
        "FETCH_UTA_ACCOUNT_ASSETS_RESPONSE",
        ""
    );
    record!(
        FetchUtaAccountSettingsResponseDataSymbolConfigListItem,
        "FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE",
        "/data/symbolConfigList/0"
    );
    record!(
        FetchUtaAccountSettingsResponseData,
        "FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE",
        "/data"
    );
    record!(
        FetchUtaAccountSettingsResponse,
        "FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE",
        ""
    );
    record!(BitgetUtaPosition, "BITGET_UTA_POSITION", "");
    record!(
        FetchUtaPositionsResponseData,
        "FETCH_UTA_POSITIONS_RESPONSE",
        "/data"
    );
    record!(
        FetchUtaPositionsResponse,
        "FETCH_UTA_POSITIONS_RESPONSE",
        ""
    );
    record!(
        BitgetUtaOrderFeeDetailItem,
        "BITGET_UTA_ORDER",
        "/feeDetail/0"
    );
    record!(BitgetUtaOrder, "BITGET_UTA_ORDER", "");
    record!(
        FetchUtaOrdersResponseData,
        "FETCH_UTA_ORDERS_RESPONSE",
        "/data"
    );
    record!(FetchUtaOrdersResponse, "FETCH_UTA_ORDERS_RESPONSE", "");
    record!(
        FetchPositionsV2ResponseDataItem,
        "FETCH_POSITIONS_V2_RESPONSE",
        "/data/0"
    );
    record!(FetchPositionsV2Response, "FETCH_POSITIONS_V2_RESPONSE", "");
}
fn check_numeric_ingress(oracle: &Value) {
    let cases = oracle["numericIngress"].as_array().unwrap();
    assert_eq!(cases.len(), 90, "Bitget numeric ingress field inventory");
    macro_rules! probe {
        ($ty:ty, $case:expr) => {{
            let case=$case;
            let name=case["name"].as_str().unwrap();
            let lexeme=case["lexeme"].as_str().unwrap();
            let text=case["raw"].as_str().unwrap();
            let expected=u64::from_str_radix(case["bits"].as_str().unwrap(),16).unwrap();
            assert_eq!(case["valid"],true,"TS numeric ingress {name} {lexeme}");
            let value:Value=serde_json::from_str(text).unwrap();
            let raw=serde_json::from_str::<$ty>(text);
            let borrowed=<$ty as serde::Deserialize>::deserialize(&value).unwrap();
            assert_eq!(borrowed.request_time.to_bits(),expected,"borrowed numeric ingress {name}.requestTime {lexeme}");
            let parsed=serde_json::from_value::<$ty>(value.clone()).unwrap();
            assert_eq!(parsed.request_time.to_bits(),expected,"parsed numeric ingress {name}.requestTime {lexeme}");
            assert_eq!(raw.unwrap_or_else(|error| panic!("raw numeric ingress {name}.requestTime {lexeme}: {error}")).request_time.to_bits(),expected,"raw numeric ingress {name}.requestTime {lexeme}");
            let mut native=value;
            native["requestTime"]=json!(-0.0);
            assert_eq!(serde_json::from_value::<$ty>(native.clone()).unwrap().request_time.to_bits(),(-0.0f64).to_bits(),"native numeric ingress {name}.requestTime");
            assert_eq!(<$ty as serde::Deserialize>::deserialize(&native).unwrap().request_time.to_bits(),(-0.0f64).to_bits(),"borrowed native numeric ingress {name}.requestTime");
            // Unknown fields stay on the original Value decoder path, independently
            // of the declared timestamp's scalar ingress operation.
            let mut with_extra:Value=serde_json::from_str(text).unwrap();
            with_extra["unknownNumeric"] = json!({"nested":serde_json::from_str::<Value>(lexeme).unwrap()});
            let mut early=serde_json::Map::new();
            early.insert("earlyUnknown".into(),serde_json::from_str::<Value>("{\"negative\":-0,\"overflow\":1e400}").unwrap());
            early.extend(with_extra.as_object().unwrap().clone());
            let with_extra=Value::Object(early);
            let extra_raw=with_extra.to_string();
            let decoded=serde_json::from_str::<$ty>(&extra_raw).unwrap();
            assert_eq!(safe_cex_num::numeric_ingress::js_value_as_f64(&decoded.extra["unknownNumeric"]["nested"]).unwrap().to_bits(),expected,"unknown numeric extra {name} {lexeme}");
            assert_eq!(decoded.request_time.to_bits(),expected,"numeric extras before requestTime {name} {lexeme}");
            assert_eq!(safe_cex_num::numeric_ingress::js_value_as_f64(&decoded.extra["earlyUnknown"]["negative"]).unwrap().to_bits(),(-0.0f64).to_bits(),"early extra negative zero {name}");
            assert_eq!(safe_cex_num::numeric_ingress::js_value_as_f64(&decoded.extra["earlyUnknown"]["overflow"]).unwrap().to_bits(),f64::INFINITY.to_bits(),"early extra overflow {name}");
            if lexeme=="-0" {
                for (label,invalid) in [
                    ("null",Value::Null),("boolean",json!(true)),("array",json!([])),
                    ("object",json!({})),("numeric-string",json!("1e400")),
                    ("number-token",json!({"$serde_json::private::Number":"1e400"})),
                    ("raw-token",json!({"$serde_json::private::RawValue":"1e400"})),
                ] {
                    let mut invalid_record:Value=serde_json::from_str(text).unwrap();
                    // Explicit object construction matters: parsing a private
                    // Number token through Value first can promote the object.
                    invalid_record["requestTime"]=invalid;
                    assert!(serde_json::from_str::<$ty>(&invalid_record.to_string()).is_err(),"raw numeric rejection {name} {label}");
                    assert!(serde_json::from_value::<$ty>(invalid_record.clone()).is_err(),"owned numeric rejection {name} {label}");
                    assert!(<$ty as serde::Deserialize>::deserialize(&invalid_record).is_err(),"borrowed numeric rejection {name} {label}");
                }
            }


        }};
    }
    for case in cases {
        match case["name"].as_str().unwrap() {
            "FETCH_UTA_ACCOUNT_ASSETS_RESPONSE" => probe!(FetchUtaAccountAssetsResponse, case),
            "FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE" => probe!(FetchUtaAccountSettingsResponse, case),
            "FETCH_UTA_POSITIONS_RESPONSE" => probe!(FetchUtaPositionsResponse, case),
            "FETCH_UTA_ORDERS_RESPONSE" => probe!(FetchUtaOrdersResponse, case),
            "FETCH_POSITIONS_V2_RESPONSE" => probe!(FetchPositionsV2Response, case),
            name => panic!("uncovered numeric ingress {name}"),
        }
    }
}
fn check(oracle: Value) {
    check_numeric_ingress(&oracle);
    qualified_record_rejections(&oracle);
    // The private derived category metadata is audited by its crate-unit test.
    // The public Deserialize operation must reject object forms, including qualified calls.
    use serde::Deserialize;
    for value in oracle["acceptedSets"]["BITGET_UTA_CATEGORY"]
        .as_array()
        .unwrap()
    {
        assert!(
            BitgetUtaCategory::deserialize(json!({value.as_str().unwrap():null})).is_err(),
            "BITGET_UTA_CATEGORY qualified decode rejects object {value}"
        );
    }
    check_derived_enum::<safe_cex_types::OrderSide>("OrderSide", &oracle);
    check_derived_enum::<safe_cex_types::OrderStatus>("OrderStatus", &oracle);
    check_derived_enum::<safe_cex_types::OrderType>("OrderType", &oracle);
    check_derived_enum::<safe_cex_types::OrderTimeInForce>("OrderTimeInForce", &oracle);
    check_derived_enum::<safe_cex_types::PositionSide>("PositionSide", &oracle);
    check_derived_enum::<safe_cex_types::PositionMode>("PositionMode", &oracle);
    check_derived_enum::<safe_cex_types::OrderStopTriggerType>("OrderStopTriggerType", &oracle);

    for case in oracle["utf16Cases"].as_array().unwrap() {
        assert_eq!(
            case["valid"], true,
            "TS accepts lone UTF-16 surrogate in {}.{}",
            case["name"], case["field"]
        );
        assert!(
            serde_json::from_str::<Value>(case["json"].as_str().unwrap()).is_err(),
            "Rust UTF-8 boundary changed: {}",
            case["name"]
        );
    }
    let generations: serde_json::Map<String, Value> = ENDPOINTS
        .iter()
        .map(|entry| {
            (
                entry.name.to_owned(),
                serde_json::to_value(entry.generation).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        generations.len(),
        ENDPOINTS.len(),
        "duplicate endpoint names"
    );
    for (name, expected) in oracle["generations"].as_object().unwrap() {
        assert_eq!(&generations[name], expected, "API generation {name}");
    }
    let actual = constants();
    assert_eq!(
        actual.as_object().unwrap().len(),
        oracle["constants"].as_object().unwrap().len(),
        "exported constants inventory"
    );
    for (name, expected) in oracle["constants"].as_object().unwrap() {
        assert_eq!(&actual[name], expected, "protocol constant {name}");
    }
    for case in oracle["cases"].as_array().unwrap() {
        let value = case["value"].clone();
        let result: Result<Value, String> = match case["name"].as_str().unwrap() {
            "FETCH_SINGLE_ACCOUNT_RESPONSE" => {
                serde_json::from_value::<FetchSingleAccountResponse>(value)
                    .map(|v| serde_json::to_value(v).unwrap())
                    .map_err(|e| e.to_string())
            }
            "BITGET_UTA_CATEGORY" => serde_json::from_value::<BitgetUtaCategory>(value)
                .map(|v| serde_json::to_value(v).unwrap())
                .map_err(|e| e.to_string()),
            "FETCH_UTA_ACCOUNT_ASSETS_RESPONSE" => {
                serde_json::from_value::<FetchUtaAccountAssetsResponse>(value)
                    .map(|v| serde_json::to_value(v).unwrap())
                    .map_err(|e| e.to_string())
            }
            "FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE" => {
                serde_json::from_value::<FetchUtaAccountSettingsResponse>(value)
                    .map(|v| serde_json::to_value(v).unwrap())
                    .map_err(|e| e.to_string())
            }
            "BITGET_UTA_POSITION" => serde_json::from_value::<BitgetUtaPosition>(value)
                .map(|v| serde_json::to_value(v).unwrap())
                .map_err(|e| e.to_string()),
            "FETCH_UTA_POSITIONS_RESPONSE" => {
                serde_json::from_value::<FetchUtaPositionsResponse>(value)
                    .map(|v| serde_json::to_value(v).unwrap())
                    .map_err(|e| e.to_string())
            }
            "BITGET_UTA_ORDER" => serde_json::from_value::<BitgetUtaOrder>(value)
                .map(|v| serde_json::to_value(v).unwrap())
                .map_err(|e| e.to_string()),
            "FETCH_UTA_ORDERS_RESPONSE" => serde_json::from_value::<FetchUtaOrdersResponse>(value)
                .map(|v| serde_json::to_value(v).unwrap())
                .map_err(|e| e.to_string()),
            "FETCH_POSITIONS_V2_RESPONSE" => {
                serde_json::from_value::<FetchPositionsV2Response>(value)
                    .map(|v| serde_json::to_value(v).unwrap())
                    .map_err(|e| e.to_string())
            }
            name => panic!("uncovered TS type {name}"),
        };
        assert_eq!(
            result.is_ok(),
            case["valid"].as_bool().unwrap(),
            "{} {}: {:?}",
            case["name"],
            case["label"],
            result
        );
        if let Ok(value) = result {
            assert_eq!(
                value, case["value"],
                "wire roundtrip {} {}",
                case["name"], case["label"]
            );
        }
    }
}
#[test]
fn frozen_types_and_protocol() {
    check(serde_json::from_str(include_str!("types-oracle.json")).unwrap());
}
#[test]
#[ignore = "requires Node and the isolated TypeScript compiler"]
fn live_types_and_protocol() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/types-oracle.cjs"
        ))
        .output()
        .expect("Node oracle must execute");
    assert!(
        output.status.success(),
        "TS oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).expect("TS oracle JSON");
    check(oracle.clone());
    assert_eq!(
        oracle,
        serde_json::from_str::<Value>(include_str!("types-oracle.json")).unwrap(),
        "maintained TS oracle snapshot drift"
    );
}

#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_types_refuse_inherited_required_fields() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/types-oracle.cjs"
        ))
        .arg("--probe-interface-heritage")
        .output()
        .expect("Node heritage control must execute");
    assert!(
        !output.status.success()
            && String::from_utf8_lossy(&output.stderr).contains(
                "Bitget type inventory refuses interface heritage: FETCH_POSITIONS_V2_RESPONSE"
            ),
        "Bitget inherited required numeric field must fail closed by declaration name: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[path = "support/type_identity.rs"]
mod type_identity;
