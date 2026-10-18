use super::*;
use safe_cex_exchange::{
    js_runtime::{
        client::JsHttpHost,
        promise::{JsFuture, await_js},
        value::{Thrown, error_object},
    },
    native_store::DefaultStore,
    runtime::ExchangeRuntime,
};
use serde_json::json;

struct Host {
    runtime: Rc<dyn ExchangeRuntime>,
    original: Js,
}
impl JsHttpHost for Host {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let original = self.original.clone();
        original.define("config", config).unwrap();
        Box::pin(async move { Err(Thrown::new(original)) })
    }
}
#[test]
fn sync_position_mode_retains_native_axios_error_observations() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let runtime = safe_cex_exchange::native::native_runtime();
        let original = error_object("AxiosError", "native Aster rejection");
        original.define("isAxiosError", Js::Bool(true)).unwrap();
        original
            .define("code", Js::str("ERR_ASTER_REJECTED"))
            .unwrap();
        original
            .define(
                "stack",
                Js::str("AxiosError: native Aster rejection\n  at terminal-host"),
            )
            .unwrap();
        let cause = error_object("TypeError", "root cause");
        cause.define("number", Js::Number(f64::INFINITY)).unwrap();
        cause.define("missing", Js::Undefined).unwrap();
        original.define("cause", cause).unwrap();
        let host = Rc::new(Host {
            runtime: runtime.clone(),
            original: original.clone(),
        });
        let venue = AsterDEXExchange::new(
            ExchangeName::Asterdex,
            serde_json::from_value(
                json!({"key":"fixture","secret":"fixture","tuning":{"consoleLogging":false}}),
            )
            .unwrap(),
            Rc::new(DefaultStore::new()),
            AsterDEXDependencies {
                runtime: runtime.clone(),
                http: Rc::new(move |_| Ok(host.clone())),
                external_public_data: None,
            },
        )
        .unwrap();
        let pending = venue
            .rt
            .this
            .call_method("syncPositionMode", vec![])
            .unwrap();
        let error = await_js(&venue.rt.exchange_runtime(), pending)
            .await
            .unwrap_err();
        for field in ["name", "message", "code", "stack", "isAxiosError"] {
            assert!(
                error
                    .value
                    .get(field)
                    .unwrap()
                    .strict_equals(&original.get(field).unwrap()),
                "ASTER-NATIVE-ERROR must preserve {field}"
            );
        }
        assert!(
            !error
                .value
                .get("originalError")
                .unwrap()
                .strict_equals(&error.value),
            "approved originalError snapshot must not point back to the Error"
        );
        let snapshot = error.value.get("originalError").unwrap();
        assert!(matches!(&snapshot, Js::Object(object) if object.class == "Error"));
        assert!(!snapshot.has_own("originalError"));
        for field in [
            "name",
            "message",
            "code",
            "stack",
            "isAxiosError",
            "enhancedMessage",
        ] {
            assert!(
                snapshot
                    .get(field)
                    .unwrap()
                    .strict_equals(&error.value.get(field).unwrap()),
                "original Error snapshot preserves {field}"
            );
        }

        let native = error.native.as_ref().unwrap();
        assert!(native.is_axios_error);
        assert_eq!(
            native.stack.as_deref(),
            Some("AxiosError: native Aster rejection\n  at terminal-host")
        );
        let Js::Object(object) = &error.value else {
            panic!("native Error instance")
        };
        assert_eq!(object.class, "Error");
        let cause = error.value.get("cause").unwrap();
        assert_eq!(
            cause.get("name").unwrap().to_display(),
            "TypeError",
            "ASTER-ERROR-CAUSE native Error prototype fields must survive owned capture"
        );
        assert_eq!(cause.get("message").unwrap().to_display(), "root cause");
        assert_eq!(
            cause.get("number").unwrap().to_number().unwrap(),
            f64::INFINITY
        );
        assert!(cause.has_own("missing") && cause.get("missing").unwrap().is_undefined());
        assert!(
            error.value.get("response").unwrap().is_undefined(),
            "source network rejection has no response"
        );
        assert!(
            !error.value.strict_equals(&original),
            "accepted owned-error boundary snapshots identity"
        );
        venue.dispose().unwrap();
    });
}

#[test]
fn native_error_contract_matches_both_maintained_typescript_clients() {
    let result = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../safe-cex-exchange/tests/rest-rejection-source.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let text = String::from_utf8(result.stdout).unwrap();
    let result: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(result, json!({"asterdex":true,"woo":true}));
}

use safe_cex_exchange::js_runtime::client;
use safe_cex_exchange::native::native_runtime;
use serde_json::Value;
use std::{
    cell::RefCell,
    io::{Read, Write},
    net::TcpListener,
    sync::Arc,
    time::{Duration, Instant},
};

fn validation_venue(options: Value) -> (Rc<AsterDEXExchange>, Rc<DefaultStore>) {
    let runtime = native_runtime();
    let http = client::rest_http_host(
        runtime.clone(),
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
    );
    let store = Rc::new(DefaultStore::new());
    let venue = AsterDEXExchange::new(
        ExchangeName::Asterdex,
        serde_json::from_value(options).unwrap(),
        store.clone(),
        AsterDEXDependencies {
            runtime,
            http: Rc::new(move |_| Ok(http.clone())),
            external_public_data: None,
        },
    )
    .unwrap();
    (venue, store)
}

fn validation_source(setup: bool, balance: bool) -> Value {
    let mut command = std::process::Command::new("node");
    command
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/eager_sdk_source.cjs"),
            "--validation-error",
        ]);
    if setup {
        command.arg("--setup-error");
    }
    if balance {
        command.arg("--balance");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    serde_json::from_str(output.lines().last().unwrap()).unwrap()
}

fn assert_validation_error(url: &str, source: Value, balance: bool) {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let (venue, _) = validation_venue(json!({"key":"synthetic-key","secret":"synthetic-secret","overrideProxyUrl":url,"tuning":{"consoleLogging":false}}));
        let logs = Rc::new(RefCell::new(vec![]));
        let seen = logs.clone();
        venue.base().on("log", Rc::new(move |_, value| {
            let message = String::from_utf16(&value.to_js_string()?).unwrap();
            if message.contains("Failed to get account:") || message.contains("Account validation failed:") || message.contains("Failed to fetch balance:") {
                seen.borrow_mut().push(message);
            }
            Ok(())
        }));
        let error = tokio::time::timeout(Duration::from_secs(10), async { if balance { await_js(&venue.rt.exchange_runtime(), venue.rt.this.call_method("fetchBalance", vec![]).unwrap()).await.unwrap_err().to_exchange_error() } else { venue.validate_account().await.unwrap_err() } }).await.unwrap();
        assert_eq!(error.name, source["name"], "ASTER-VALIDATION-ERROR source Error class");
        assert_eq!(error.code.as_ref().map(|value| value.to_display()), source["code"].as_str().map(str::to_owned), "ASTER-VALIDATION-ERROR source code");
        assert_eq!(error.is_axios_error, source["isAxiosError"].as_bool().unwrap(), "ASTER-VALIDATION-ERROR Axios marker");
        assert!(error.retained_response_input().is_none(), "source replacement/setup Error has no response");
        assert_eq!(error.message, source["message"], "ASTER-VALIDATION-ERROR enhanced message");
        assert_eq!(json!(*logs.borrow()), source["logs"], "ASTER-VALIDATION-ERROR nested diagnostics");
        venue.dispose().unwrap();
    });
}

#[test]
fn validation_setup_error_retains_the_source_axios_envelope() {
    assert_validation_error(
        "unsupported://fixture",
        validation_source(true, false),
        false,
    );
}

#[test]
fn validation_http_error_keeps_the_source_replacement_error() {
    http_error(false);
}

#[test]
fn balance_http_error_keeps_the_source_replacement_error() {
    http_error(true);
}

fn http_error(balance: bool) {
    let source = validation_source(false, balance);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let (mut socket, _) = loop {
            match listener.accept() {
                Ok(value) => break value,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("loopback validation accept: {error}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = vec![];
        let mut buffer = [0; 4096];
        while !bytes.windows(4).any(|value| value == b"\r\n\r\n") {
            let count = socket.read(&mut buffer).unwrap();
            assert_ne!(count, 0);
            bytes.extend_from_slice(&buffer[..count]);
        }
        let header = String::from_utf8(bytes).unwrap();
        let target = header.lines().next().unwrap();
        assert!(target.contains("/fapi/v4/account?"), "{target}");
        assert!(target.contains("signature="), "actual signing: {target}");
        assert!(
            header
                .to_lowercase()
                .contains("x-mbx-apikey: synthetic-key")
        );
        let body = r#"{"code":-2015,"msg":"Invalid API-key, IP, or permissions for action."}"#;
        write!(socket, "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
    });
    assert_validation_error(&url, source, balance);
    server.join().unwrap();
}
