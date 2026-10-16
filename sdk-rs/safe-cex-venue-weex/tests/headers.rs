use safe_cex_venue_weex::headers::{
    WeexHeaderRuntime, WeexHeaderValueKind, get_header, get_header_entries,
    get_weex_rate_limit_retry_delay_ms,
};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

#[derive(Debug)]
enum Native {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(Vec<u16>),
    Object(usize),
    Function(usize),
    Symbol(usize),
    BigInt(String),
}

#[derive(Debug)]
struct HostError {
    name: String,
    message: String,
}

struct Host<'a> {
    corpus: &'a Value,
    trace: Vec<Value>,
}

fn units(value: &Value) -> Vec<u16> {
    serde_json::from_value(value.clone()).unwrap()
}
fn number(bits: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(bits.as_str().unwrap(), 16).unwrap())
}
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
fn error(message: &str) -> HostError {
    HostError {
        name: "Error".into(),
        message: message.into(),
    }
}
fn identity(value: &Native) -> usize {
    match value {
        Native::Object(id) | Native::Function(id) | Native::Symbol(id) => *id,
        _ => panic!("object identity: {value:?}"),
    }
}
fn kind(value: &Native) -> WeexHeaderValueKind {
    use WeexHeaderValueKind as K;
    match value {
        Native::Undefined => K::Undefined,
        Native::Null => K::Null,
        Native::Boolean(_) => K::Boolean,
        Native::Number(_) => K::Number,
        Native::String(_) => K::String,
        Native::Object(_) => K::Object,
        Native::Function(_) => K::Function,
        Native::Symbol(_) => K::Symbol,
        Native::BigInt(_) => K::BigInt,
    }
}
impl Host<'_> {
    fn value(&self, id: usize) -> Native {
        let spec = &self.corpus["specs"][id];
        match spec["kind"].as_str().unwrap() {
            "undefined" => Native::Undefined,
            "null" => Native::Null,
            "boolean" => Native::Boolean(spec["value"].as_bool().unwrap()),
            "number" => Native::Number(number(&spec["bits"])),
            "string" => Native::String(units(&spec["units"])),
            "object" => Native::Object(id),
            "function" => Native::Function(id),
            "symbol" => Native::Symbol(id),
            "bigint" => Native::BigInt(spec["text"].as_str().unwrap().into()),
            other => panic!("kind {other}"),
        }
    }
    fn property(&self, id: usize, name: &[u16]) -> Option<usize> {
        let spec = &self.corpus["specs"][id];
        if let Some(found) = spec["props"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| units(&p[0]) == name)
        {
            Some(found[1].as_u64().unwrap() as usize)
        } else {
            spec.get("proto")
                .and_then(Value::as_u64)
                .and_then(|p| self.property(p as usize, name))
        }
    }
    fn has(&mut self, value: &Native, name: &[u16]) -> Result<bool, HostError> {
        let id = identity(value);
        self.trace.push(json!(["has", id, name]));
        Ok(self.property(id, name).is_some())
    }
    fn get(&mut self, value: &Native, name: &[u16]) -> Result<Native, HostError> {
        let id = identity(value);
        self.trace.push(json!(["get", id, name]));
        if self.corpus["specs"][id]
            .get("getError")
            .and_then(Value::as_str)
            .is_some_and(|key| key.encode_utf16().eq(name.iter().copied()))
        {
            return Err(error("getter sentinel"));
        }
        Ok(self
            .property(id, name)
            .map_or(Native::Undefined, |p| self.value(p)))
    }
    fn invoke(
        &mut self,
        method: &Native,
        receiver: &Native,
        name: Option<&[u16]>,
    ) -> Result<Native, HostError> {
        let id = identity(method);
        self.trace
            .push(json!(["call", id, identity(receiver), name]));
        let spec = &self.corpus["specs"][id];
        if let Some(message) = spec.get("error").and_then(Value::as_str) {
            return Err(error(message));
        }
        if let Some(key) = spec.get("receiverKey").and_then(Value::as_str) {
            let key: Vec<u16> = key.encode_utf16().collect();
            return self.get(receiver, &key);
        }
        Ok(self.value(spec["result"].as_u64().unwrap() as usize))
    }
    fn entries(&mut self, value: &Native) -> Result<Vec<(Vec<u16>, Native)>, HostError> {
        let id = identity(value);
        self.trace.push(json!(["entries", id]));
        Ok(self.corpus["specs"][id]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (units(&p[0]), self.value(p[1].as_u64().unwrap() as usize)))
            .collect())
    }
    fn case(&self, name: &[u16], field: &str) -> Vec<u16> {
        let spec = self.corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| units(&c["input"]) == name)
            .unwrap();
        units(&spec[field])
    }
    fn number(&mut self, value: &Native) -> Result<f64, HostError> {
        Ok(match value {
            Native::Undefined => f64::NAN,
            Native::Null => 0.0,
            Native::Boolean(value) => f64::from(u8::from(*value)),
            Native::Number(value) => *value,
            Native::String(value) => {
                String::from_utf16(value).map_or(f64::NAN, |s| safe_cex_num::number_from_string(&s))
            }
            Native::BigInt(value) => safe_cex_num::number_from_string(value),
            Native::Symbol(_) => {
                return Err(HostError {
                    name: "TypeError".into(),
                    message: "Cannot convert a Symbol value to a number".into(),
                });
            }
            Native::Object(id) => {
                self.trace.push(json!(["number", id]));
                let spec = &self.corpus["specs"][*id];
                if let Some(message) = spec.get("numberError").and_then(Value::as_str) {
                    return Err(error(message));
                }
                spec.get("numberResult").map_or(f64::NAN, |id| {
                    match self.value(id.as_u64().unwrap() as usize) {
                        Native::Number(value) => value,
                        other => panic!("numeric capability {other:?}"),
                    }
                })
            }
            Native::Function(_) => f64::NAN,
        })
    }
    fn maximum(&mut self, values: &[f64]) -> Result<f64, HostError> {
        self.trace.push(json!([
            "maximum",
            values.iter().copied().map(bits).collect::<Vec<_>>()
        ]));
        Ok(values
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, safe_cex_num::math_max))
    }
    fn now(&mut self) -> f64 {
        self.trace.push(json!(["clock"]));
        number(&self.corpus["clock"])
    }
    fn date(&mut self, name: &[u16]) -> f64 {
        self.trace.push(json!(["date", name]));
        number(
            &self.corpus["dates"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| units(&c["input"]) == name)
                .unwrap()["bits"],
        )
    }
}
fn runtime<'a>() -> WeexHeaderRuntime<Host<'a>, Native, HostError> {
    WeexHeaderRuntime {
        kind,
        undefined: || Native::Undefined,
        has_property: Host::has,
        get_property: Host::get,
        invoke: Host::invoke,
        entries: Host::entries,
        lowercase: |h, n| h.case(n, "lower"),
        uppercase: |h, n| h.case(n, "upper"),
        number_value: |v| match v {
            Native::Number(n) => *n,
            other => panic!("number {other:?}"),
        },
        string_value: |v| match v {
            Native::String(s) => s.clone(),
            other => panic!("string {other:?}"),
        },
        number: Host::number,
        maximum: Host::maximum,
        now_ms: Host::now,
        parse_date: Host::date,
    }
}
fn assert_native(value: &Native, expected: &Value, label: &str) {
    match value {
        Native::Undefined => assert_eq!(expected["kind"], "undefined", "{label}::kind"),
        Native::Null => assert_eq!(expected["kind"], "null", "{label}::kind"),
        Native::Boolean(value) => {
            assert_eq!(expected["kind"], "boolean", "{label}::kind");
            assert_eq!(
                *value,
                expected["value"].as_bool().unwrap(),
                "{label}::value"
            );
        }
        Native::Number(value) => {
            assert_eq!(expected["kind"], "number", "{label}::kind");
            assert_eq!(bits(*value), expected["bits"], "{label}::value");
        }
        Native::String(value) => {
            assert_eq!(expected["kind"], "string", "{label}::kind");
            assert_eq!(*value, units(&expected["units"]), "{label}::value");
        }
        Native::BigInt(value) => {
            assert_eq!(expected["kind"], "bigint", "{label}::kind");
            assert_eq!(value, expected["text"].as_str().unwrap(), "{label}::value");
        }
        Native::Object(id) | Native::Function(id) | Native::Symbol(id) => {
            let kind = match value {
                Native::Object(_) => "object",
                Native::Function(_) => "function",
                _ => "symbol",
            };
            assert_eq!(expected["kind"], kind, "{label}::kind");
            assert_eq!(
                *id as u64,
                expected["id"].as_u64().unwrap(),
                "{label}::value"
            );
        }
    }
}
fn assert_error(error: HostError, expected: &Value, label: &str) {
    assert_eq!(error.name, expected["error"]["name"], "{label}::error_name");
    assert_eq!(
        error.message, expected["error"]["message"],
        "{label}::error_message"
    );
}

#[test]
fn headers_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/headers-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let header_ids = [
        "lookup_mixed",
        "exact_false",
        "exact_zero",
        "upper_fallback",
        "method",
        "method_undefined",
        "inherited_receiver",
        "nonfunction_get",
        "getter_error",
        "method_error",
        "lone_name",
        "null",
        "function",
        "primitive",
    ];
    let entries_ids = [
        "plain",
        "serialized",
        "json_null",
        "json_string",
        "json_function",
        "json_error",
        "nonfunction_json",
        "array_order",
        "null",
    ];
    let delay_ids = [
        "fallback",
        "exhausted_priority",
        "observed_cap",
        "mixed_remaining",
        "bigint_weight",
        "object_weight",
        "coercion_error",
        "symbol_error",
        "ignored_coercion",
        "retry_first",
        "retry_nonfinite",
        "retry_date",
        "retry_zero",
        "retry_invalid",
        "retry_object",
        "huge_interval",
        "unicode_names",
        "nan_remaining",
    ];
    let expected: Vec<_> = [
        ("header", header_ids.as_slice()),
        ("entries", entries_ids.as_slice()),
        ("delay", delay_ids.as_slice()),
    ]
    .into_iter()
    .flat_map(|(op, ids)| ids.iter().map(move |id| (op, *id)))
    .collect();
    let results = corpus["results"].as_array().unwrap();
    assert_eq!(results.len(), 41, "headers::case_count");
    let (mut errors, mut compared) = (0, 0);
    let mut capability_counts = [0; 6];
    for (index, case) in results.iter().enumerate() {
        let (op, id) = expected[index];
        assert_eq!(case["op"], op, "headers::case_operation");
        assert_eq!(case["id"], id, "headers::case_identity");
        let label = format!("headers::{op}::{id}");
        let mut host = Host {
            corpus: &corpus,
            trace: Vec::new(),
        };
        let headers = host.value(case["input"].as_u64().unwrap() as usize);
        let runtime = runtime();
        let expected = &case["result"];
        match op {
            "header" => match get_header(&headers, &units(&case["name"]), &mut host, &runtime) {
                Ok(value) => assert_native(&value, &expected["value"], &label),
                Err(error) => {
                    errors += 1;
                    assert_error(error, expected, &label);
                }
            },
            "entries" => match get_header_entries(&headers, &mut host, &runtime) {
                Ok(values) => {
                    let expected = expected["entries"].as_array().unwrap();
                    assert_eq!(values.len(), expected.len(), "{label}::len");
                    for (index, (key, value)) in values.iter().enumerate() {
                        assert_eq!(*key, units(&expected[index][0]), "{label}::key_{index}");
                        assert_native(
                            value,
                            &expected[index][1],
                            &format!("{label}::entry_{index}"),
                        );
                    }
                }
                Err(error) => {
                    errors += 1;
                    assert_error(error, expected, &label);
                }
            },
            "delay" => match get_weex_rate_limit_retry_delay_ms(&headers, &mut host, &runtime) {
                Ok(value) => assert_native(&Native::Number(value), &expected["value"], &label),
                Err(error) => {
                    errors += 1;
                    assert_error(error, expected, &label);
                }
            },
            _ => panic!("unexpected operation {op}"),
        }
        for event in &host.trace {
            if let Some(index) = ["clock", "date", "maximum", "call", "entries", "number"]
                .iter()
                .position(|name| event[0].as_str() == Some(*name))
            {
                capability_counts[index] += 1;
            }
        }
        assert_eq!(
            host.trace,
            *case["trace"].as_array().unwrap(),
            "{label}::capability_trace"
        );
        compared += 1;
    }
    assert_eq!(errors, 5, "headers::error_count");
    assert_eq!(compared, 41, "headers::compared");
    assert_eq!(
        capability_counts,
        [18, 4, 11, 9, 19, 2],
        "headers::capability_counts"
    );
}
