use safe_cex_types::{MarginType, PositionMode};
use safe_cex_venue_weex::{
    order_result::{WeexOrderResultRuntime, parse_order_result},
    symbol_settings::{
        WeexDefaultSymbolConfig, WeexSymbolSettingsRuntime, get_or_fetch_symbol_config,
        get_symbol_config_from_cache,
    },
};
use serde_json::Value;
use std::{
    cell::RefCell,
    future::Future,
    path::Path,
    pin::Pin,
    process::Command,
    task::{Context, Poll, Waker},
};

#[derive(Debug, PartialEq)]
enum NativeValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(Vec<u16>),
    Object(Vec<u16>, bool),
    Record,
    Default(WeexDefaultSymbolConfig),
}
#[derive(Debug, PartialEq)]
struct NativeError {
    name: &'static str,
    units: Vec<u16>,
}
struct Host {
    input: Value,
    events: Vec<String>,
    emitted: Vec<NativeValue>,
    symbols: Vec<Vec<u16>>,
    mode: PositionMode,
    fetch_polls: usize,
}
fn units(value: &Value) -> Vec<u16> {
    serde_json::from_value(value.clone()).unwrap()
}
fn mode(value: &str) -> PositionMode {
    match value {
        "hedged" => PositionMode::Hedged,
        "oneway" => PositionMode::OneWay,
        other => panic!("unknown mode {other}"),
    }
}
fn decode(value: &Value) -> NativeValue {
    match value["kind"].as_str().unwrap_or("undefined") {
        "undefined" => NativeValue::Undefined,
        "null" => NativeValue::Null,
        "boolean" => NativeValue::Boolean(value["value"].as_bool().unwrap()),
        "number" => NativeValue::Number(value["value"].as_f64().unwrap()),
        "string" => NativeValue::String(units(&value["units"])),
        "object" => NativeValue::Object(units(&value["units"]), value["throws"] == true),
        "default" => NativeValue::Default(WeexDefaultSymbolConfig {
            margin_type: match value["marginType"].as_str().unwrap() {
                "cross" => MarginType::Cross,
                "isolated" => MarginType::Isolated,
                other => panic!("unknown margin {other}"),
            },
            position_mode: mode(value["positionMode"].as_str().unwrap()),
            cross_leverage: value["crossLeverage"].as_f64().unwrap(),
            isolated_long_leverage: value["isolatedLongLeverage"].as_f64().unwrap(),
            isolated_short_leverage: value["isolatedShortLeverage"].as_f64().unwrap(),
        }),
        other => panic!("unknown value {other}"),
    }
}
fn truthy(value: &NativeValue) -> bool {
    match value {
        NativeValue::Undefined | NativeValue::Null => false,
        NativeValue::Boolean(v) => *v,
        NativeValue::Number(v) => *v != 0.0 && !v.is_nan(),
        NativeValue::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn error(name: &'static str, message: &str) -> NativeError {
    NativeError {
        name,
        units: message.encode_utf16().collect(),
    }
}
fn string_value(value: &NativeValue, nullish_empty: bool) -> Result<Vec<u16>, NativeError> {
    let text = match value {
        NativeValue::Undefined => if nullish_empty { "" } else { "undefined" }.into(),
        NativeValue::Null => if nullish_empty { "" } else { "null" }.into(),
        NativeValue::Boolean(v) => v.to_string(),
        NativeValue::Number(v) => safe_cex_num::js_number_string(*v),
        NativeValue::String(v) | NativeValue::Object(v, false) => return Ok(v.clone()),
        NativeValue::Object(_, true) => return Err(error("SyntaxError", "coercion")),
        _ => panic!("unsupported coercion"),
    };
    Ok(text.encode_utf16().collect())
}
fn property(host: &mut Host, result: &NativeValue, key: &str) -> Result<NativeValue, NativeError> {
    if matches!(result, NativeValue::Null | NativeValue::Undefined) {
        return Ok(NativeValue::Undefined);
    }
    host.events.push(format!("get:{key}"));
    if host.input["fail"] == key {
        return Err(error("TypeError", key));
    }
    Ok(decode(&host.input["props"][key]))
}
fn string(_: &mut Host, value: &str) -> Result<NativeValue, NativeError> {
    Ok(NativeValue::String(value.encode_utf16().collect()))
}
fn emit(host: &mut Host, value: &NativeValue) -> Result<(), NativeError> {
    host.events.push("emit".into());
    host.emitted.push(match value {
        NativeValue::String(v) => NativeValue::String(v.clone()),
        NativeValue::Number(v) => NativeValue::Number(*v),
        _ => panic!("uncovered emitted value"),
    });
    if host.input["fail"] == "emit" {
        return Err(error("RangeError", "emit"));
    }
    Ok(())
}
fn new_error(_: &mut Host, value: &NativeValue) -> NativeError {
    match string_value(value, false) {
        Ok(units) => NativeError {
            name: "Error",
            units,
        },
        Err(err) => err,
    }
}
fn uuid(host: &mut Host) -> Result<NativeValue, NativeError> {
    host.events.push("uuid".into());
    if host.input["fail"] == "uuid" {
        return Err(error("RangeError", "uuid"));
    }
    Ok(decode(&host.input["uuid"]))
}
fn integer_string(_: &mut Host, value: &NativeValue) -> Result<Vec<u16>, NativeError> {
    string_value(value, true)
}
fn cached(host: &RefCell<Host>, symbol: &[u16]) -> Result<NativeValue, NativeError> {
    let mut host = host.borrow_mut();
    host.events.push("cached".into());
    host.symbols.push(symbol.to_vec());
    if host.input["fail"] == "cached" {
        return Err(error("TypeError", "cached"));
    }
    Ok(decode(&host.input["cached"]))
}
struct Fetch<'a> {
    host: &'a RefCell<Host>,
    pending: bool,
}
impl Future for Fetch<'_> {
    type Output = Result<(), NativeError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.host.borrow_mut().fetch_polls += 1;
        if self.pending {
            self.pending = false;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        let mut host = self.host.borrow_mut();
        host.events.push("fetch:end".into());
        if host.input["fail"] == "fetch" {
            return Poll::Ready(Err(error("RangeError", "fetch")));
        }
        host.mode = mode(host.input["afterMode"].as_str().unwrap());
        Poll::Ready(Ok(()))
    }
}
fn fetch<'a>(
    host: &'a RefCell<Host>,
    symbol: &'a [u16],
) -> Pin<Box<dyn Future<Output = Result<(), NativeError>> + 'a>> {
    {
        let mut host = host.borrow_mut();
        host.events.push("fetch:start".into());
        host.symbols.push(symbol.to_vec());
    }
    Box::pin(Fetch {
        host,
        pending: true,
    })
}
fn fetched(host: &RefCell<Host>, _: &(), symbol: &[u16]) -> Result<NativeValue, NativeError> {
    let mut host = host.borrow_mut();
    host.events.push("fetched".into());
    host.symbols.push(symbol.to_vec());
    if host.input["fail"] == "fetched" {
        return Err(error("TypeError", "fetched"));
    }
    Ok(decode(&host.input["fetched"]))
}
fn account_mode(host: &RefCell<Host>) -> Result<PositionMode, NativeError> {
    Ok(host.borrow().mode)
}
fn default_config(
    _: &RefCell<Host>,
    config: WeexDefaultSymbolConfig,
) -> Result<NativeValue, NativeError> {
    Ok(NativeValue::Default(config))
}
fn run<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
    }
}
fn assert_error<T: std::fmt::Debug>(actual: Result<T, NativeError>, expected: &Value, label: &str) {
    let actual = actual.unwrap_err();
    assert_eq!(
        actual.name,
        expected["name"].as_str().unwrap(),
        "{label}::native_error_name"
    );
    assert_eq!(
        actual.units,
        units(&expected["units"]),
        "{label}::native_error_message"
    );
}
#[test]
fn native_order_settings_match_typescript() {
    let oracle = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/order-settings-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "oracle failed: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let cases: Value = serde_json::from_slice(&oracle.stdout).unwrap();
    let parse_ids = [
        "order_id",
        "client_id",
        "fallback",
        "uuid",
        "null_result",
        "undefined_result",
        "numeric_id",
        "surrogate_id",
        "false_chain",
        "success_zero",
        "success_null",
        "reject_message",
        "reject_code",
        "reject_default",
        "emit_failure",
        "uuid_failure",
        "getter_failure",
        "object_id",
        "coercion_failure",
        "rejected_surrogate",
    ];
    let settings_ids = [
        "cache_hit",
        "cache_missing",
        "cache_false",
        "cached_return",
        "refetch",
        "fetched_return",
        "default_after_await",
        "falsy_default",
        "surrogate_symbol",
        "cache_failure",
        "fetch_failure",
        "fetched_failure",
    ];
    assert_eq!(
        cases["parse"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        parse_ids,
        "native_parse_case_floor"
    );
    assert_eq!(
        cases["settings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        settings_ids,
        "native_settings_case_floor"
    );
    let result_runtime = WeexOrderResultRuntime {
        optional_property: property,
        is_false: |v| *v == NativeValue::Boolean(false),
        truthy,
        string,
        emit_error: emit,
        new_error,
        uuid,
        integer_string,
    };
    let settings_runtime = WeexSymbolSettingsRuntime {
        cached,
        truthy,
        fetch,
        fetched,
        account_position_mode: account_mode,
        default_config,
    };
    let mut native_events = Vec::new();
    for case in cases["parse"].as_array().unwrap() {
        let input = &case["input"];
        let mut host = Host {
            input: input.clone(),
            events: vec![],
            emitted: vec![],
            symbols: vec![],
            mode: PositionMode::OneWay,
            fetch_polls: 0,
        };
        let result = match input["result"].as_str().unwrap() {
            "record" => NativeValue::Record,
            "null" => NativeValue::Null,
            _ => NativeValue::Undefined,
        };
        let actual = parse_order_result(
            &mut host,
            &result_runtime,
            &result,
            decode(&input["fallback"]),
        );
        if case.get("error").is_some() {
            assert_error(actual, &case["error"], "native_parse");
        } else {
            assert_eq!(
                actual.unwrap(),
                units(&case["output"]),
                "native_parse_output::{}",
                case["id"]
            );
        }
        assert_eq!(
            host.emitted,
            case["emitted"]
                .as_array()
                .unwrap()
                .iter()
                .map(decode)
                .collect::<Vec<_>>(),
            "native_parse_emitted::{}",
            case["id"]
        );
        native_events.push((host.events, case["events"].clone()));
    }
    for case in cases["settings"].as_array().unwrap() {
        let input = &case["input"];
        let host = RefCell::new(Host {
            input: input.clone(),
            events: vec![],
            emitted: vec![],
            symbols: vec![],
            mode: mode(input["mode"].as_str().unwrap()),
            fetch_polls: 0,
        });
        let symbol = units(&input["symbol"]);
        let expects_fetch = input["method"] != "cache"
            && input["fail"] != "cached"
            && (!truthy(&decode(&input["cached"])) || input["refetch"] == true);
        let actual = if input["method"] == "cache" {
            get_symbol_config_from_cache(&host, &settings_runtime, &symbol)
        } else {
            let future = get_or_fetch_symbol_config(
                &host,
                &settings_runtime,
                &symbol,
                input["refetch"] == true,
            );
            assert_eq!(
                host.borrow().events,
                if expects_fetch {
                    vec!["cached", "fetch:start"]
                } else {
                    vec!["cached"]
                },
                "native_settings_eager_prefix::{}",
                case["id"]
            );
            host.borrow_mut().mode = PositionMode::OneWay;
            run(future)
        };
        let host = host.into_inner();
        if case.get("error").is_some() {
            assert_error(actual, &case["error"], "native_settings");
        } else {
            assert_eq!(
                actual.unwrap(),
                decode(&case["output"]),
                "native_settings_output::{}",
                case["id"]
            );
        }
        assert_eq!(
            host.symbols,
            case["symbols"]
                .as_array()
                .unwrap()
                .iter()
                .map(units)
                .collect::<Vec<_>>(),
            "native_settings_symbols::{}",
            case["id"]
        );
        assert_eq!(
            host.fetch_polls,
            if expects_fetch { 2 } else { 0 },
            "native_settings_await_polls::{}",
            case["id"]
        );
        native_events.push((host.events, case["events"].clone()));
    }
    for (actual, expected) in native_events {
        assert_eq!(
            actual,
            expected
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
            "order_settings_effect_order"
        );
    }
}
