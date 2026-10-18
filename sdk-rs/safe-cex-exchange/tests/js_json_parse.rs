//! Real JSON builtin and real HTTP response binding; inspect UTF-16 before JSON projection.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{client, globals, promise::await_js, scope::ScopeShared, value::Js},
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_json_parse_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn units(v: &Value) -> Vec<u16> {
    serde_json::from_value(v.clone()).unwrap()
}
fn observe(v: &Js) -> Value {
    match v {
        Js::Null => json!(["null"]),
        Js::Bool(v) => json!(["bool", v]),
        Js::Number(v) => json!(["number", format!("{:016x}", v.to_bits())]),
        Js::String(v) => json!(["string", &**v]),
        Js::Array(v) => json!([
            "array",
            v.borrow()
                .iter()
                .map(|v| observe(v.as_ref().unwrap()))
                .collect::<Vec<_>>()
        ]),
        Js::Object(_) => json!([
            "object",
            v.own_entries()
                .unwrap()
                .into_iter()
                .map(|(k, v)| json!([&*k, observe(&v)]))
                .collect::<Vec<_>>()
        ]),
        _ => panic!("unexpected JSON value {v:?}"),
    }
}
fn run(f: impl std::future::Future<Output = ()>) {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, f);
}
#[test]
fn json_builtin_retains_lone_surrogate_units_and_matches_native_parse() {
    let source = source();
    assert_eq!(
        source["cases"].as_array().unwrap().len(),
        200,
        "independent JSON source census"
    );
    run(async {
        let runtime = native_runtime();
        let http = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            runtime,
            http,
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        let json = shared.globals.get("JSON").unwrap();
        for case in source["cases"].as_array().unwrap() {
            let text = units(&case["text"]);
            let actual = json.call_method("parse", vec![Js::utf16(&text)]);
            if let Some(name) = case.get("error") {
                assert_eq!(
                    actual.unwrap_err().value.get("name").unwrap().to_display(),
                    name.as_str().unwrap(),
                    "invalid JSON {text:?}"
                );
            } else {
                assert_eq!(
                    observe(&actual.unwrap_or_else(|error| panic!(
                        "valid JSON units {text:?} refused: {error:?}"
                    ))),
                    case["value"],
                    "parsed native units {text:?}"
                );
            }
        }
    });
}
#[test]
fn real_http_json_response_retains_lone_surrogate_fields_and_rows() {
    let source = source();
    let body = source["rest"].as_str().unwrap().to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        let mut chunk = [0; 1024];
        while !request.windows(4).any(|b| b == b"\r\n\r\n") {
            let n = socket.read(&mut chunk).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&chunk[..n]);
        }
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    run(async {
        let runtime: std::rc::Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> = native_runtime();
        let host = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let xhr = client::create(
            host,
            Js::object(vec![
                ("baseURL", Js::str(&format!("http://{address}"))),
                ("timeout", Js::Number(2000.0)),
            ]),
        );
        let pending = xhr.call_method("get", vec![Js::str("/json")]).unwrap();
        let response = await_js(&runtime, pending).await.unwrap();
        assert_eq!(
            observe(&response.get("data").unwrap()),
            source["restValue"],
            "actual axios response must remain an object with every field and native UTF-16 units"
        );
    });
    server.join().unwrap();
}

#[test]
fn json_builtin_preserves_every_escaped_utf16_code_unit() {
    assert_eq!(
        source()["escapeCodeUnits"],
        65536,
        "actual Node escape census"
    );
    run(async {
        let runtime = native_runtime();
        let http = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            runtime,
            http,
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        let json = shared.globals.get("JSON").unwrap();
        for unit in 0..=u16::MAX {
            let text = format!("\"\\u{unit:04x}\"");
            let parsed = json
                .call_method("parse", vec![Js::str(&text)])
                .unwrap_or_else(|error| panic!("code unit {unit:04x}: {error:?}"));
            assert_eq!(
                parsed.to_js_string().unwrap().as_ref(),
                &[unit],
                "native JSON escape {unit:04x}"
            );
        }
    });
}
