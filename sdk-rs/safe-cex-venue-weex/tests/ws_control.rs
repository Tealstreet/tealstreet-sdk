use safe_cex_venue_weex::{
    selection::get_desired_ticker_topics,
    signing::sign_weex_message,
    subscriptions::{WeexSubscriptionMethod, WeexSubscriptionRuntime, WeexSubscriptionState},
    ws_control::*,
};
use serde_json::Value;
use std::{path::Path, process::Command};

#[derive(Clone, Debug, PartialEq)]
enum HeaderValue {
    Undefined,
    String(Vec<u16>),
}

fn units(value: &Value) -> Vec<u16> {
    serde_json::from_value(value.clone()).unwrap()
}
fn strings(value: &Value) -> Vec<Vec<u16>> {
    serde_json::from_value(value.clone()).unwrap()
}
fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn header(value: &Value) -> HeaderValue {
    match value["kind"].as_str().unwrap() {
        "undefined" => HeaderValue::Undefined,
        "string" => HeaderValue::String(units(&value["units"])),
        other => panic!("unexpected header token {other}"),
    }
}
fn assert_header(native: &HeaderValue, expected: &Value, label: &str) {
    match native {
        HeaderValue::Undefined => assert_eq!(expected["kind"], "undefined", "{label}"),
        HeaderValue::String(value) => {
            assert_eq!(expected["kind"], "string", "{label}");
            assert_eq!(*value, units(&expected["units"]), "{label}");
        }
    }
}

struct AuthHost {
    time: f64,
    credentials: Value,
    failures: Value,
    mutate_time: bool,
    events: Vec<String>,
    signs: Vec<(HeaderValue, Vec<u16>)>,
}
fn time_ms(host: &mut AuthHost) -> Result<f64, String> {
    host.events.push("time".into());
    if host.failures["time"] == true {
        return Err("TypeError".into());
    }
    if host.mutate_time {
        host.credentials["key"] =
            serde_json::json!({"kind":"string","units":[67,72,65,78,71,69,68]});
    }
    Ok(host.time)
}
fn credential(host: &mut AuthHost, name: &str) -> Result<HeaderValue, String> {
    host.events.push(format!("credential:{name}"));
    if host.failures[name] == true {
        return Err("RangeError".into());
    }
    Ok(header(&host.credentials[name]))
}
fn sign(host: &mut AuthHost, secret: HeaderValue, message: &[u16]) -> Result<HeaderValue, String> {
    host.events.push("sign".into());
    host.signs.push((secret.clone(), message.to_vec()));
    match secret {
        HeaderValue::String(secret) => Ok(HeaderValue::String(
            sign_weex_message(&secret, message).encode_utf16().collect(),
        )),
        HeaderValue::Undefined => Err("TypeError".into()),
    }
}

struct Packet {
    method: String,
    params: Option<Vec<Vec<u16>>>,
    id: f64,
}
struct ControlHost {
    performance: f64,
    failures: Value,
    dispose_performance: bool,
    dispose_send: bool,
    events: Vec<String>,
    packets: Vec<Packet>,
    logs: Vec<Vec<u16>>,
    snapshots: Vec<f64>,
}
fn performance(
    host: &mut ControlHost,
    state: &mut WeexWebsocketControlState,
) -> Result<f64, String> {
    host.events.push("performance".into());
    if host.failures["performance"] == true {
        return Err("TypeError".into());
    }
    if host.dispose_performance {
        state.is_disposed = true;
    }
    Ok(host.performance)
}
fn send_logged(
    host: &mut ControlHost,
    state: &mut WeexWebsocketControlState,
    method: &str,
    params: Option<&[&str]>,
    id: f64,
) -> Result<(), String> {
    host.events.push("packet".into());
    host.packets.push(Packet {
        method: method.into(),
        params: params.map(|items| {
            items
                .iter()
                .map(|item| item.encode_utf16().collect())
                .collect()
        }),
        id,
    });
    if host.failures["json"] == true {
        return Err("TypeError".into());
    }
    host.events.push("sendWithLog".into());
    if host.dispose_send {
        state.is_disposed = true;
    }
    if host.failures["send"] == true {
        return Err("RangeError".into());
    }
    Ok(())
}
fn snapshot(
    host: &mut ControlHost,
    _: &mut WeexWebsocketControlState,
    limit: f64,
) -> Result<(), String> {
    host.events.push("snapshot".into());
    host.snapshots.push(limit);
    if host.failures["snapshot"] == true {
        return Err("RangeError".into());
    }
    Ok(())
}
fn emit_control(host: &mut ControlHost, _: &mut WeexWebsocketControlState) -> Result<(), String> {
    host.events.push("emit".into());
    if host.failures["emit"] == true {
        return Err("TypeError".into());
    }
    Ok(())
}
fn warning(
    host: &mut ControlHost,
    _: &mut WeexWebsocketControlState,
    message: &[u16],
) -> Result<(), String> {
    host.events.push("warning".into());
    host.logs.push(message.to_vec());
    if host.failures["log"] == true {
        return Err("RangeError".into());
    }
    Ok(())
}
fn assert_result(result: &Result<(), String>, expected: &Value, label: &str) {
    match result {
        Ok(()) => assert_eq!(expected["unit"].as_bool(), Some(true), "{label}"),
        Err(error) => assert_eq!(Some(error.as_str()), expected["error"].as_str(), "{label}"),
    }
}
fn assert_packets(native: &[Packet], expected: &Value, label: &str) {
    let expected = expected.as_array().unwrap();
    assert_eq!(native.len(), expected.len(), "{label}::packet_count");
    for (packet, expected) in native.iter().zip(expected) {
        assert_eq!(
            packet.method,
            expected["method"].as_str().unwrap(),
            "{label}::method"
        );
        let params = expected["params"].as_array().map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap().encode_utf16().collect::<Vec<_>>())
                .collect::<Vec<_>>()
        });
        assert_eq!(packet.params, params, "{label}::params");
        assert_eq!(
            packet.id.to_bits(),
            number(&expected["id"]).to_bits(),
            "{label}::packet_id"
        );
    }
}

struct OpenHost {
    available: Vec<Vec<u16>>,
    active: Vec<Vec<u16>>,
    failures: Value,
    dispose_send: bool,
    events: Vec<String>,
    packets: Vec<Packet>,
}
fn open_now(host: &mut OpenHost, _: &mut WeexSubscriptionState) -> Result<f64, String> {
    host.events.push("now".into());
    if host.failures["clock"] == true {
        return Err("TypeError".into());
    }
    Ok(1000.0)
}
fn open_desired(
    host: &mut OpenHost,
    state: &mut WeexSubscriptionState,
) -> Result<Vec<Vec<u16>>, String> {
    host.events.push("desired".into());
    if host.failures["desired"] == true {
        return Err("TypeError".into());
    }
    Ok(get_desired_ticker_topics(
        &host.available,
        &host.active,
        state.topics.len(),
    ))
}
fn open_send(
    host: &mut OpenHost,
    state: &mut WeexSubscriptionState,
    method: WeexSubscriptionMethod,
    topics: &[Vec<u16>],
    id: f64,
) -> Result<(), String> {
    host.events.push("packet".into());
    host.packets.push(Packet {
        method: match method {
            WeexSubscriptionMethod::Subscribe => "SUBSCRIBE",
            WeexSubscriptionMethod::Unsubscribe => "UNSUBSCRIBE",
        }
        .into(),
        params: Some(topics.to_vec()),
        id,
    });
    host.events.push("send".into());
    if host.dispose_send {
        state.is_disposed = true;
    }
    if host.failures["send"] == true {
        return Err("RangeError".into());
    }
    Ok(())
}
fn open_warning(_: &mut OpenHost, _: &mut WeexSubscriptionState, _: &str) -> Result<(), String> {
    Err("unexpected budget warning".into())
}
fn open_emit(host: &mut OpenHost, _: &mut WeexSubscriptionState) -> Result<(), String> {
    host.events.push("emit".into());
    if host.failures["emit"] == true {
        return Err("TypeError".into());
    }
    Ok(())
}

#[test]
fn ws_control_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ws-control-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut traces: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();
    let auth_ids = [
        "basic",
        "zero",
        "negative_zero",
        "nan",
        "infinity",
        "large_exponent",
        "small_exponent",
        "unicode",
        "nul",
        "missing_key",
        "missing_passphrase",
        "missing_secret",
        "clock_error",
        "key_error",
        "secret_error",
        "mutate_time",
    ];
    let auth = corpus["auth"].as_array().unwrap();
    assert_eq!(auth.len(), 16, "ws_control::auth_count");
    for (index, case) in auth.iter().enumerate() {
        let id = auth_ids[index];
        assert_eq!(case["id"], id, "ws_control::auth_identity");
        let input = &case["input"];
        let mut host = AuthHost {
            time: number(&input["time"]),
            credentials: input["credentials"].clone(),
            failures: input["failures"].clone(),
            mutate_time: input["mutateTime"].as_bool().unwrap(),
            events: Vec::new(),
            signs: Vec::new(),
        };
        let runtime = WeexAuthHeaderRuntime {
            time_ms,
            credential,
            string: |_, units| HeaderValue::String(units.to_vec()),
            sign,
        };
        match create_auth_headers(&mut host, &runtime) {
            Ok(headers) => {
                assert_eq!(case["result"]["error"].as_str(), None, "auth/{id}::result");
                let expected = case["result"]["headers"].as_array().unwrap();
                assert_eq!(expected.len(), 5, "auth/{id}::header_count");
                let names = [
                    "User-Agent",
                    "ACCESS-KEY",
                    "ACCESS-PASSPHRASE",
                    "ACCESS-TIMESTAMP",
                    "ACCESS-SIGN",
                ];
                for (index, (name, value)) in headers.iter().enumerate() {
                    assert_eq!(*name, names[index], "auth/{id}::native_header_name");
                    assert_eq!(
                        expected[index]["name"], names[index],
                        "auth/{id}::source_header_name"
                    );
                    assert_header(
                        value,
                        &expected[index]["value"],
                        &format!("auth/{id}::header_value"),
                    );
                }
            }
            Err(error) => assert_eq!(
                Some(error.as_str()),
                case["result"]["error"].as_str(),
                "auth/{id}::result"
            ),
        }
        let signs = case["signInputs"].as_array().unwrap();
        assert_eq!(host.signs.len(), signs.len(), "auth/{id}::sign_count");
        for ((secret, message), expected) in host.signs.iter().zip(signs) {
            assert_header(
                secret,
                &expected["secret"],
                &format!("auth/{id}::sign_secret"),
            );
            assert_eq!(
                *message,
                units(&expected["message"]),
                "auth/{id}::sign_message"
            );
        }
        traces.push((
            format!("auth/{id}"),
            host.events,
            serde_json::from_value(case["events"].clone()).unwrap(),
        ));
    }
    let control_ids = [
        "subscribe/basic",
        "subscribe/negative_zero",
        "subscribe/infinity",
        "subscribe/json_error",
        "subscribe/send_error",
        "subscribe/disposed",
        "log/default",
        "log/empty",
        "log/utf16",
        "log/already",
        "log/error",
        "log/repeated",
        "private_open/basic",
        "private_open/disposed",
        "private_open/json_error",
        "private_open/snapshot_error",
        "private_open/emit_error",
        "private_open/dispose_during_send",
        "private_ping/basic",
        "private_ping/disposed",
        "private_ping/performance_error",
        "private_ping/json_error",
        "private_ping/send_error",
        "private_ping/dispose_during_performance",
        "public_ping/basic",
        "public_ping/disposed",
        "public_ping/performance_error",
        "public_ping/json_error",
        "public_ping/send_error",
        "public_ping/dispose_during_performance",
    ];
    let control = corpus["control"].as_array().unwrap();
    assert_eq!(control.len(), 30, "ws_control::control_count");
    for (index, case) in control.iter().enumerate() {
        let id = control_ids[index];
        assert_eq!(case["id"], id, "ws_control::control_identity");
        let input = &case["input"];
        let mut state = WeexWebsocketControlState {
            is_disposed: input["disposed"].as_bool().unwrap(),
            subscription_id: number(&input["subscriptionId"]),
            ping_at: number(&input["pingAt"]),
            has_logged_unsupported: input["logged"].as_bool().unwrap(),
        };
        let mut host = ControlHost {
            performance: number(&input["performance"]),
            failures: input["failures"].clone(),
            dispose_performance: input["disposeDuringPerformance"].as_bool().unwrap(),
            dispose_send: input["disposeDuringSend"].as_bool().unwrap(),
            events: Vec::new(),
            packets: Vec::new(),
            logs: Vec::new(),
            snapshots: Vec::new(),
        };
        let runtime = WeexWebsocketControlRuntime {
            performance_now: performance,
            send_with_log: send_logged,
            snapshot,
            emit_reconnected: emit_control,
            log_warning: warning,
        };
        let reason = (!input["reason"].is_null()).then(|| units(&input["reason"]));
        let expected_results = case["results"].as_array().unwrap();
        let mut results = Vec::new();
        for _ in 0..if id == "log/repeated" { 2 } else { 1 } {
            let result = match case["name"].as_str().unwrap() {
                "subscribe" => private_subscribe(&mut host, &mut state, &runtime),
                "logUnsupportedTransport" => {
                    log_unsupported_transport(&mut host, &mut state, reason.as_deref(), &runtime)
                }
                "onOpen" => private_on_open(&mut host, &mut state, &runtime),
                "ping" if case["publicClass"] == true => {
                    public_ping(&mut host, &mut state, &runtime)
                }
                "ping" => private_ping(&mut host, &mut state, &runtime),
                other => panic!("unexpected entry {other}"),
            };
            let failed = result.is_err();
            results.push(result);
            if failed {
                break;
            }
        }
        assert_eq!(results.len(), expected_results.len(), "{id}::result_count");
        for (native, expected) in results.iter().zip(expected_results) {
            assert_result(native, expected, &format!("{id}::result"));
        }
        let expected = &case["state"];
        assert_eq!(
            state.is_disposed,
            expected["disposed"].as_bool().unwrap(),
            "{id}::disposed"
        );
        assert_eq!(
            state.subscription_id.to_bits(),
            number(&expected["subscriptionId"]).to_bits(),
            "{id}::id"
        );
        assert_eq!(
            state.ping_at.to_bits(),
            number(&expected["pingAt"]).to_bits(),
            "{id}::ping_at"
        );
        assert_eq!(
            state.has_logged_unsupported,
            expected["logged"].as_bool().unwrap(),
            "{id}::logged"
        );
        assert_packets(&host.packets, &case["packets"], id);
        let logs = case["logs"].as_array().unwrap();
        assert_eq!(host.logs.len(), logs.len(), "{id}::log_count");
        for (native, expected) in host.logs.iter().zip(logs) {
            assert_eq!(*native, units(&expected["message"]), "{id}::message");
            assert_eq!(expected["severity"], "warning", "{id}::warning_capability");
        }
        let snapshots: Vec<u64> = case["snapshots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| number(value).to_bits())
            .collect();
        assert_eq!(
            host.snapshots
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            snapshots,
            "{id}::snapshot_limit"
        );
        if id == "subscribe/basic" {
            assert_eq!(
                host.packets[0].params,
                Some(
                    ["account", "positions", "orders", "fill"]
                        .into_iter()
                        .map(|value| value.encode_utf16().collect())
                        .collect()
                ),
                "ws_control::independent_private_channels"
            );
        }
        if id == "log/repeated" {
            assert_eq!(host.logs.len(), 1, "ws_control::independent_log_once");
        }
        traces.push((
            id.into(),
            host.events,
            serde_json::from_value(case["events"].clone()).unwrap(),
        ));
    }
    let open_ids = [
        "basic",
        "disposed",
        "desired_error",
        "send_error",
        "clock_error",
        "emit_error",
        "duplicates",
        "dispose_during_send",
    ];
    let open = corpus["open"].as_array().unwrap();
    assert_eq!(open.len(), 8, "ws_control::open_count");
    for (index, case) in open.iter().enumerate() {
        let id = open_ids[index];
        assert_eq!(case["id"], id, "ws_control::open_identity");
        let input = &case["input"];
        let mut state = WeexSubscriptionState {
            topics: strings(&input["topics"]),
            ticker_topics: strings(&input["tickers"]),
            is_connected: true,
            is_disposed: input["disposed"].as_bool().unwrap(),
            operation_timestamps: input["timestamps"]
                .as_array()
                .unwrap()
                .iter()
                .map(number)
                .collect(),
            has_logged_limit: input["logged"].as_bool().unwrap(),
            subscription_id: number(&input["subscriptionId"]),
        };
        let mut host = OpenHost {
            available: strings(&input["available"]),
            active: strings(&input["active"]),
            failures: input["failures"].clone(),
            dispose_send: input["disposeDuringSend"].as_bool().unwrap(),
            events: Vec::new(),
            packets: Vec::new(),
        };
        let runtime = WeexSubscriptionRuntime {
            now: open_now,
            desired_topics: open_desired,
            log_warning: open_warning,
            send: open_send,
        };
        let result = public_on_open(&mut host, &mut state, &runtime, open_emit);
        assert_result(
            &result,
            &case["result"],
            &format!("public_open/{id}::result"),
        );
        let expected = &case["state"];
        assert_eq!(
            state.is_disposed,
            expected["disposed"].as_bool().unwrap(),
            "public_open/{id}::disposed"
        );
        assert_eq!(
            state.subscription_id.to_bits(),
            number(&expected["subscriptionId"]).to_bits(),
            "public_open/{id}::id"
        );
        assert_eq!(
            state.topics,
            strings(&expected["topics"]),
            "public_open/{id}::topics"
        );
        assert_eq!(
            state.ticker_topics,
            strings(&expected["tickers"]),
            "public_open/{id}::tickers"
        );
        let stamps: Vec<u64> = expected["timestamps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| number(value).to_bits())
            .collect();
        assert_eq!(
            state
                .operation_timestamps
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            stamps,
            "public_open/{id}::timestamps"
        );
        assert_eq!(
            state.has_logged_limit,
            expected["logged"].as_bool().unwrap(),
            "public_open/{id}::logged"
        );
        assert_packets(
            &host.packets,
            &case["packets"],
            &format!("public_open/{id}"),
        );
        if id == "basic" {
            assert_eq!(
                state.operation_timestamps.len(),
                2,
                "ws_control::independent_reopened_budget"
            );
            assert_eq!(
                host.packets.len(),
                2,
                "ws_control::independent_reopened_sends"
            );
        }
        traces.push((
            format!("public_open/{id}"),
            host.events,
            serde_json::from_value(case["events"].clone()).unwrap(),
        ));
    }
    assert_eq!(traces.len(), 54, "ws_control::independent_total_cases");
    for (id, native, expected) in traces {
        assert_eq!(native, expected, "{id}::effects");
    }
}
