use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str, js_from_value, js_from_value_ref};
use safe_cex_venue_lighter::{
    sdk_common::{ApiResponse, Balance, PaginatedResponse},
    sdk_protocol_types::ResultCode,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;
fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/sdk-generic-types-oracle.cjs"
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
fn shape<T: DeserializeOwned + Serialize>(name: &str, complete: Value, optional: &[&str]) {
    let parsed: T = serde_json::from_value(complete.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        complete,
        "{name} roundtrip"
    );
    for key in complete.as_object().unwrap().keys() {
        let mut value = complete.clone();
        value.as_object_mut().unwrap().shift_remove(key);
        assert_eq!(
            serde_json::from_value::<T>(value).is_ok(),
            optional.contains(&key.as_str()),
            "{name}.{key} required"
        );
    }
    let positional = Value::Array(complete.as_object().unwrap().values().cloned().collect());
    for bad in [
        positional,
        json!([]),
        json!(null),
        json!(false),
        json!("record"),
        json!(1),
    ] {
        assert!(
            serde_json::from_value::<T>(bad).is_err(),
            "{name} rejects non-record"
        );
    }
}
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct NoDefault {
    value: f64,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for NoDefault {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { value });
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_generic_types_match_live_typescript() {
    let oracle = oracle_capture();
    assert_eq!(
        oracle["schemas"]["ApiResponse"],
        json!({"parameters":["T"],"fields":[
            {"name":"code","type":"ResultCode","optional":false},{"name":"message","type":"string","optional":false},
            {"name":"data","type":"T","optional":true},{"name":"error","type":"string","optional":true},{"name":"timestamp","type":"number","optional":false}
        ]}),
        "ApiResponse generic schema"
    );
    assert_eq!(
        oracle["schemas"]["PaginatedResponse"],
        json!({"parameters":["T"],"fields":[
            {"name":"items","type":"T[]","optional":false},{"name":"total","type":"number","optional":false},
            {"name":"page","type":"number","optional":false},{"name":"pageSize","type":"number","optional":false},
            {"name":"totalPages","type":"number","optional":false},{"name":"hasNext","type":"boolean","optional":false},{"name":"hasPrev","type":"boolean","optional":false}
        ]}),
        "PaginatedResponse generic schema"
    );
    let value = ApiResponse {
        code: ResultCode::SUCCESS,
        message: "fixture".into(),
        data: Some(NoDefault { value: 16777217.25 }),
        error: Some("error".into()),
        timestamp: 16777217.25,
    };
    let _: &ResultCode = &value.code;
    let _: &String = &value.message;
    let _: &Option<NoDefault> = &value.data;
    let _: &Option<String> = &value.error;
    let _: &f64 = &value.timestamp;
    let complete = oracle["identity"]["api"].clone();
    let decoded: ApiResponse<NoDefault> = serde_json::from_value(complete.clone()).unwrap();
    assert_eq!(
        decoded.code.0,
        complete["code"].as_f64().unwrap(),
        "ApiResponse.code native identity"
    );
    assert_eq!(
        decoded.message, complete["message"],
        "ApiResponse.message native identity"
    );
    assert_eq!(
        decoded.error.as_deref(),
        complete["error"].as_str(),
        "ApiResponse.error native identity"
    );
    assert_eq!(
        decoded.data.as_ref().unwrap().value,
        complete["data"]["value"].as_f64().unwrap(),
        "ApiResponse.data native identity"
    );
    assert_eq!(
        decoded.timestamp,
        complete["timestamp"].as_f64().unwrap(),
        "ApiResponse.timestamp native identity"
    );
    shape::<ApiResponse<NoDefault>>("ApiResponse", complete.clone(), &["data", "error"]);
    assert!(
        ApiResponse::<NoDefault>::deserialize(Value::Array(
            complete.as_object().unwrap().values().cloned().collect()
        ))
        .is_err(),
        "ApiResponse qualified shape"
    );
    for key in ["message", "error", "data", "code", "timestamp"] {
        for bad in [json!(null), json!([]), json!(false)] {
            let mut value = complete.clone();
            value[key] = bad;
            assert!(
                serde_json::from_value::<ApiResponse<NoDefault>>(value).is_err(),
                "ApiResponse.{key} rejects invalid field"
            );
        }
    }
    let mut nullable = complete.clone();
    nullable["data"] = json!(null);
    let parsed: ApiResponse<Option<f64>> = serde_json::from_value(nullable.clone()).unwrap();
    assert_eq!(parsed.data, Some(None), "present nullable T is not absent");
    assert!(
        serde_json::to_value(parsed)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("data")
    );
    nullable.as_object_mut().unwrap().shift_remove("data");
    let parsed: ApiResponse<Option<f64>> = serde_json::from_value(nullable).unwrap();
    assert_eq!(parsed.data, None, "missing nullable T stays absent");
    assert!(
        !serde_json::to_value(parsed)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("data")
    );
    let value = PaginatedResponse {
        items: vec![NoDefault { value: 16777217.25 }],
        total: 16777217.25,
        page: 16777217.25,
        page_size: 16777217.25,
        total_pages: 16777217.25,
        has_next: true,
        has_prev: false,
    };
    let _: &Vec<NoDefault> = &value.items;
    let _: &f64 = &value.total;
    let _: &f64 = &value.page;
    let _: &f64 = &value.page_size;
    let _: &f64 = &value.total_pages;
    let _: &bool = &value.has_next;
    let _: &bool = &value.has_prev;
    let complete = oracle["identity"]["page"].clone();
    let decoded: PaginatedResponse<NoDefault> = serde_json::from_value(complete.clone()).unwrap();
    for (field, actual) in [
        ("total", decoded.total),
        ("page", decoded.page),
        ("pageSize", decoded.page_size),
        ("totalPages", decoded.total_pages),
    ] {
        assert_eq!(
            actual,
            complete[field].as_f64().unwrap(),
            "PaginatedResponse.{field} native identity"
        );
    }
    assert_eq!(
        decoded
            .items
            .iter()
            .map(|item| item.value)
            .collect::<Vec<_>>(),
        vec![303.75, 404.5],
        "PaginatedResponse.items native identity"
    );
    for (next, prev) in [(true, false), (false, true), (false, false), (true, true)] {
        let mut input = complete.clone();
        input["hasNext"] = json!(next);
        input["hasPrev"] = json!(prev);
        let decoded: PaginatedResponse<NoDefault> = serde_json::from_value(input).unwrap();
        assert_eq!(
            decoded.has_next, next,
            "PaginatedResponse.hasNext native identity"
        );
        assert_eq!(
            decoded.has_prev, prev,
            "PaginatedResponse.hasPrev native identity"
        );
    }
    shape::<PaginatedResponse<NoDefault>>("PaginatedResponse", complete.clone(), &[]);
    assert!(
        PaginatedResponse::<NoDefault>::deserialize(Value::Array(
            complete.as_object().unwrap().values().cloned().collect()
        ))
        .is_err(),
        "PaginatedResponse qualified shape"
    );
    for key in complete.as_object().unwrap().keys() {
        let mut value = complete.clone();
        value[key] = Value::Null;
        assert!(
            serde_json::from_value::<PaginatedResponse<NoDefault>>(value).is_err(),
            "PaginatedResponse.{key} rejects null"
        );
    }
    let mut absent_nested = complete.clone();
    absent_nested["items"][0] = json!({});
    assert!(
        serde_json::from_value::<PaginatedResponse<NoDefault>>(absent_nested).is_err(),
        "generic payload required field retained"
    );
    assert_eq!(
        oracle["numeric"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["raw"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "-0",
            "-0.0",
            "-0e0",
            "0",
            "1e3",
            "1e400",
            "-1e400",
            "-1e-4000",
            "16777217.25"
        ],
        "numeric witness lexeme inventory"
    );
    assert_eq!(
        oracle["numeric"][0]["bits"], "8000000000000000",
        "lexical negative-zero source bits"
    );
    for witness in oracle["numeric"].as_array().unwrap() {
        let raw = witness["raw"].as_str().unwrap();
        let bits = witness["bits"].as_str().unwrap();
        for key in ["code", "timestamp", "data"] {
            let mut v = json!({"code":0,"message":"fixture","data":1,"timestamp":1});
            v[key] = json!("__NUMBER__");
            let raw = serde_json::to_string(&v)
                .unwrap()
                .replace("\"__NUMBER__\"", raw);
            let parsed: ApiResponse<f64> =
                serde_json::from_value(serde_json::from_str::<Value>(&raw).unwrap()).unwrap();
            let actual = match key {
                "code" => parsed.code.0,
                "timestamp" => parsed.timestamp,
                "data" => parsed.data.unwrap(),
                _ => unreachable!(),
            };
            assert_eq!(
                format!("{:016x}", actual.to_bits()),
                bits,
                "ApiResponse.{key} Value-first numeric bits"
            );
            let input: Value = serde_json::from_str(&raw).unwrap();
            for parsed in [
                js_from_str::<ApiResponse<f64>>(&raw),
                js_from_value(input.clone()),
                js_from_value_ref(&input),
            ] {
                let parsed = parsed.unwrap();
                let number = match key {
                    "code" => parsed.code.0,
                    "timestamp" => parsed.timestamp,
                    "data" => parsed.data.unwrap(),
                    _ => unreachable!(),
                };
                assert_eq!(
                    format!("{:016x}", number.to_bits()),
                    bits,
                    "ApiResponse.{key} canonical metadata bits"
                );
            }
        }
        let page_raw = format!(
            r#"{{"items":[{raw}],"total":1,"page":1,"pageSize":1,"totalPages":1,"hasNext":false,"hasPrev":false}}"#
        );
        let page_value: Value = serde_json::from_str(&page_raw).unwrap();
        for page in [
            PaginatedResponse::<f64>::deserialize(&page_value).unwrap(),
            serde_json::from_value::<PaginatedResponse<f64>>(page_value).unwrap(),
        ] {
            assert_eq!(
                format!("{:016x}", page.items[0].to_bits()),
                bits,
                "PaginatedResponse numeric payload bits"
            );
        }
        for key in ["total", "page", "pageSize", "totalPages"] {
            let mut v = complete.clone();
            v[key] = json!("__NUMBER__");
            let raw = serde_json::to_string(&v)
                .unwrap()
                .replace("\"__NUMBER__\"", raw);
            let parsed: PaginatedResponse<NoDefault> =
                serde_json::from_value(serde_json::from_str::<Value>(&raw).unwrap()).unwrap();
            let actual = match key {
                "total" => parsed.total,
                "page" => parsed.page,
                "pageSize" => parsed.page_size,
                "totalPages" => parsed.total_pages,
                _ => unreachable!(),
            };
            assert_eq!(
                format!("{:016x}", actual.to_bits()),
                bits,
                "PaginatedResponse.{key} Value-first numeric bits"
            );
            let input: Value = serde_json::from_str(&raw).unwrap();
            for parsed in [
                js_from_str::<PaginatedResponse<NoDefault>>(&raw),
                js_from_value(input.clone()),
                js_from_value_ref(&input),
            ] {
                let parsed = parsed.unwrap();
                let number = match key {
                    "total" => parsed.total,
                    "page" => parsed.page,
                    "pageSize" => parsed.page_size,
                    "totalPages" => parsed.total_pages,
                    _ => unreachable!(),
                };
                assert_eq!(
                    format!("{:016x}", number.to_bits()),
                    bits,
                    "PaginatedResponse.{key} canonical metadata bits"
                );
            }
        }
    }
    assert_eq!(
        json!(
            oracle["strings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w["units"].clone())
                .collect::<Vec<_>>()
        ),
        json!([
            [112, 108, 97, 105, 110],
            [233],
            [0xd83d, 0xdc69],
            [0xfeff],
            [0x85],
            [0xd800],
            [0xdfff],
            [120, 0xd800, 121]
        ]),
        "ApiResponse UTF16 witness inventory"
    );
    for witness in oracle["strings"].as_array().unwrap() {
        let api =
            js_from_str::<ApiResponse<JsNumberInput>>(witness["apiPayload"].as_str().unwrap());
        let page = js_from_str::<PaginatedResponse<JsNumberInput>>(
            witness["pagePayload"].as_str().unwrap(),
        );
        if witness["representable"].as_bool().unwrap() {
            for payload in [api.unwrap().data.unwrap(), page.unwrap().items.remove(0)] {
                let decoded: String = payload.decode().unwrap();
                assert_eq!(
                    json!(decoded.encode_utf16().collect::<Vec<_>>()),
                    witness["units"],
                    "retained generic payload scalar Unicode"
                );
            }
        } else {
            let expected: Vec<u16> = witness["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                .collect();
            for payload in [api.unwrap().data.unwrap(), page.unwrap().items.remove(0)] {
                let raw: safe_cex_types::retained_json::RetainedJsonInput =
                    payload.clone().decode().unwrap();
                assert_eq!(
                    raw.as_text().unwrap(),
                    expected,
                    "retained generic payload exact units"
                );
                let typed: String = payload.decode().unwrap();
                assert_eq!(
                    typed,
                    String::from_utf16_lossy(&expected),
                    "generic typed String replacement"
                );
            }
        }
        let raw = witness["encoded"].as_str().unwrap();
        let result =
            serde_json::from_str::<Value>(raw).and_then(serde_json::from_value::<ApiResponse<f64>>);
        if witness["representable"].as_bool().unwrap() {
            let value = result.unwrap();
            for s in [&value.message, value.error.as_ref().unwrap()] {
                assert_eq!(
                    json!(s.encode_utf16().collect::<Vec<_>>()),
                    witness["units"],
                    "ApiResponse Unicode retention"
                );
            }
        } else {
            assert!(result.is_err(), "ApiResponse upstream Value refusal");
            let _: ApiResponse<f64> = typed_utf16::decode(raw);
        }
    }
    // Nested typed payload decoders must retain a lexical negative zero too.
    let raw = r#"{"code":0,"message":"fixture","timestamp":0,"data":{"symbol":"X","tokenAddress":"X","available":"1","locked":"0","total":"1","decimals":-0}}"#;
    let parsed: ApiResponse<Balance> =
        serde_json::from_value(serde_json::from_str::<Value>(raw).unwrap()).unwrap();
    assert_eq!(
        parsed.data.unwrap().decimals.to_bits(),
        (-0.0_f64).to_bits(),
        "nested typed payload signed zero"
    );
    let raw = r#"{"items":[{"symbol":"X","tokenAddress":"X","available":"1","locked":"0","total":"1","decimals":-0}],"total":1,"page":1,"pageSize":1,"totalPages":1,"hasNext":false,"hasPrev":false}"#;
    let input: Value = serde_json::from_str(raw).unwrap();
    for page in [
        PaginatedResponse::<Balance>::deserialize(&input).unwrap(),
        serde_json::from_value::<PaginatedResponse<Balance>>(input).unwrap(),
    ] {
        assert_eq!(
            page.items[0].decimals.to_bits(),
            (-0.0_f64).to_bits(),
            "PaginatedResponse nested typed payload signed zero"
        );
    }
    let native: ApiResponse<Value> =
        serde_json::from_value(json!({"code":0,"message":"fixture","timestamp":0,"data":-0.0}))
            .unwrap();
    assert_eq!(
        native.data.unwrap().as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits(),
        "ApiResponse native Value payload zero"
    );
    let native: PaginatedResponse<Value> = serde_json::from_value(json!({"items":[-0.0],"total":1,"page":1,"pageSize":1,"totalPages":1,"hasNext":false,"hasPrev":false})).unwrap();
    assert_eq!(
        native.items[0].as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits(),
        "PaginatedResponse native Value payload zero"
    );
    // Legacy serde Value payloads retain their diagnostic witness. Canonical
    // whole-entry ingress below uses JsNumberInput and must preserve the bits.
    for witness in oracle["numeric"].as_array().unwrap() {
        let lexeme = witness["raw"].as_str().unwrap();
        let source_bits = witness["bits"].as_str().unwrap();
        let expected = if lexeme == "-0" {
            "0000000000000000"
        } else {
            source_bits
        };
        let api_raw = format!(r#"{{"code":0,"message":"fixture","timestamp":0,"data":{lexeme}}}"#);
        let page_raw = format!(
            r#"{{"items":[{lexeme}],"total":1,"page":1,"pageSize":1,"totalPages":1,"hasNext":false,"hasPrev":false}}"#
        );
        let api_input: Value = serde_json::from_str(&api_raw).unwrap();
        let page_input: Value = serde_json::from_str(&page_raw).unwrap();
        for input in [&api_input["data"], &page_input["items"][0]] {
            let value: f64 = serde_json::from_value(input.clone()).unwrap();
            assert_eq!(
                format!("{:016x}", value.to_bits()),
                source_bits,
                "generic payload input bits intact"
            );
        }
        for (path, api, page) in [
            (
                "raw",
                js_from_str::<ApiResponse<JsNumberInput>>(&api_raw),
                js_from_str::<PaginatedResponse<JsNumberInput>>(&page_raw),
            ),
            (
                "owned",
                js_from_value::<ApiResponse<JsNumberInput>>(api_input.clone()),
                js_from_value::<PaginatedResponse<JsNumberInput>>(page_input.clone()),
            ),
            (
                "borrowed",
                js_from_value_ref::<ApiResponse<JsNumberInput>>(&api_input),
                js_from_value_ref::<PaginatedResponse<JsNumberInput>>(&page_input),
            ),
        ] {
            let api = api.unwrap();
            let page = page.unwrap();
            for (name, payload) in [
                ("ApiResponse", api.data.unwrap()),
                ("PaginatedResponse", page.items[0].clone()),
            ] {
                let decoded: f64 = payload.decode().unwrap();
                assert_eq!(
                    format!("{:016x}", decoded.to_bits()),
                    source_bits,
                    "{name} {path} retained payload repaired bits {lexeme}"
                );
            }
        }
        let borrowed_api = ApiResponse::<Value>::deserialize(&api_input).unwrap();
        let borrowed_page = PaginatedResponse::<Value>::deserialize(&page_input).unwrap();
        for (name, value) in [
            ("ApiResponse", borrowed_api.data.unwrap()),
            ("PaginatedResponse", borrowed_page.items[0].clone()),
        ] {
            let value: f64 = serde_json::from_value(value).unwrap();
            assert_eq!(
                format!("{:016x}", value.to_bits()),
                expected,
                "{name} borrowed generic Value lexical-zero finding"
            );
        }
        let api: ApiResponse<Value> = serde_json::from_value(api_input).unwrap();
        let page: PaginatedResponse<Value> = serde_json::from_value(page_input).unwrap();
        for (name, value) in [
            ("ApiResponse", api.data.unwrap()),
            ("PaginatedResponse", page.items[0].clone()),
        ] {
            let value: f64 = serde_json::from_value(value).unwrap();
            assert_eq!(
                format!("{:016x}", value.to_bits()),
                expected,
                "{name} generic Value lexical-zero finding"
            );
        }
        let api: ApiResponse<Value> = serde_json::from_str(&api_raw).unwrap();
        let page: PaginatedResponse<Value> = serde_json::from_str(&page_raw).unwrap();
        for (name, value) in [
            ("ApiResponse", api.data.unwrap()),
            ("PaginatedResponse", page.items[0].clone()),
        ] {
            let value: f64 = serde_json::from_value(value).unwrap();
            assert_eq!(
                format!("{:016x}", value.to_bits()),
                source_bits,
                "{name} generic Value direct raw bits"
            );
        }
    }
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_generic_source_controls_fail_by_name() {
    for (mutation, name) in [
        (
            "inherited-number",
            "ApiResponse unsupported interface heritage",
        ),
        ("data-required", "ApiResponse generic schema"),
        ("items-scalar", "PaginatedResponse generic schema"),
        ("timestamp-string", "ApiResponse generic schema"),
        ("erase-negative-zero", "numeric witness lexeme inventory"),
    ] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_generic_types_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_GENERIC_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.status.code(), Some(101), "{log}");
        assert!(log.contains(name), "{log}");
    }
}
#[test]
#[ignore = "requires Node"]
fn sdk_generic_requires_selected_compiler() {
    let out = Command::new("node")
        .arg(script())
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/absent-generic-compiler"
            ),
        )
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("MODULE_NOT_FOUND"));
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_generic_ingress_depth_cost_matches_live_acceptance() {
    let oracle = oracle_capture();
    let cases = oracle["depthCases"].as_array().unwrap();
    assert_eq!(
        cases
            .iter()
            .map(|v| v["depth"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![0, 2, 124, 125, 126, 127, 128, 140],
        "generic source depth inventory"
    );
    fn depth_leaf(mut value: &Value, depth: usize, label: &str) {
        for _ in 0..depth {
            assert_eq!(value.as_array().unwrap().len(), 1, "{label} array width");
            value = &value[0];
        }
        assert_eq!(
            value.as_f64().unwrap().to_bits(),
            17.0_f64.to_bits(),
            "{label} leaf bits"
        );
    }
    for case in cases {
        let depth = case["depth"].as_u64().unwrap() as usize;
        // Construct an already-parsed value without introducing the raw parser's limit.
        let mut payload = json!(17);
        for _ in 0..depth {
            payload = Value::Array(vec![payload]);
        }
        let mut api = json!({"code":0,"message":"fixture","timestamp":0});
        api["data"] = payload.clone();
        let mut page =
            json!({"total":1,"page":1,"pageSize":1,"totalPages":1,"hasNext":false,"hasPrev":false});
        page["items"] = Value::Array(vec![payload]);
        for value in [
            ApiResponse::<Value>::deserialize(api.clone()).unwrap(),
            ApiResponse::<Value>::deserialize(&api).unwrap(),
        ] {
            depth_leaf(
                value.data.as_ref().unwrap(),
                depth,
                "ApiResponse legacy accepted payload",
            );
        }
        for value in [
            PaginatedResponse::<Value>::deserialize(page.clone()).unwrap(),
            PaginatedResponse::<Value>::deserialize(&page).unwrap(),
        ] {
            depth_leaf(
                &value.items[0],
                depth,
                "PaginatedResponse legacy accepted payload",
            );
        }
        for (entry, result) in [
            (
                "raw",
                js_from_str::<ApiResponse<JsNumberInput>>(case["api"].as_str().unwrap()),
            ),
            (
                "owned",
                js_from_value::<ApiResponse<JsNumberInput>>(api.clone()),
            ),
            (
                "borrowed",
                js_from_value_ref::<ApiResponse<JsNumberInput>>(&api),
            ),
        ] {
            assert_eq!(
                result.is_ok(),
                depth <= 126,
                "ApiResponse {entry} depth {depth} acceptance cost"
            );
            if let Err(error) = &result {
                assert!(
                    error.to_string().contains("recursion limit exceeded"),
                    "ApiResponse {entry} depth {depth} explicit depth refusal: {error}"
                );
            }
            if let Ok(record) = result {
                depth_leaf(
                    &serde_json::to_value(record.data.unwrap()).unwrap(),
                    depth,
                    "ApiResponse retained payload",
                );
            }
        }
        for (entry, result) in [
            (
                "raw",
                js_from_str::<PaginatedResponse<JsNumberInput>>(case["page"].as_str().unwrap()),
            ),
            (
                "owned",
                js_from_value::<PaginatedResponse<JsNumberInput>>(page.clone()),
            ),
            (
                "borrowed",
                js_from_value_ref::<PaginatedResponse<JsNumberInput>>(&page),
            ),
        ] {
            assert_eq!(
                result.is_ok(),
                depth <= 125,
                "PaginatedResponse {entry} depth {depth} acceptance cost"
            );
            if let Err(error) = &result {
                assert!(
                    error.to_string().contains("recursion limit exceeded"),
                    "PaginatedResponse {entry} depth {depth} explicit depth refusal: {error}"
                );
            }
            if let Ok(record) = result {
                depth_leaf(
                    &serde_json::to_value(&record.items[0]).unwrap(),
                    depth,
                    "PaginatedResponse retained payload",
                );
            }
        }
    }
}
