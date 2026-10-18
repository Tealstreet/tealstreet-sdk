//! Public SDK scheduling through NativeRuntime, signed HTTP and the production binding.
#![cfg(not(target_arch = "wasm32"))]
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{exchange::Exchange, js_runtime::client, native::native_runtime};
use safe_cex_types::ExchangeName;
use safe_cex_venue_asterdex::native_exchange::{AsterDEXDependencies, AsterDEXExchange};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::{Read, Write},
    net::TcpListener,
    rc::Rc,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

fn source() -> Value {
    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/eager_sdk_source.cjs"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn venue(options: Value) -> (Rc<AsterDEXExchange>, Rc<DefaultStore>) {
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

#[test]
fn validation_diagnostic_runs_before_the_public_call_returns_its_waiter() {
    let source = source();
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let (venue, _) = venue(json!({"key":"","secret":"","tuning":{"consoleLogging":false}}));
        let logs = Rc::new(RefCell::new(vec![]));
        let seen = logs.clone();
        venue.base().on(
            "log",
            Rc::new(move |_, value| {
                seen.borrow_mut()
                    .push(String::from_utf16(&value.to_js_string()?).unwrap());
                Ok(())
            }),
        );
        let pending = venue.validate_account();
        assert_eq!(
            json!(*logs.borrow()),
            source["prefix"],
            "ASTER-SDK-PREFIX must execute actual validation before polling"
        );
        let error = pending.await.unwrap_err();
        assert_eq!(error.message, "Missing required credentials: API key");
        assert_eq!(json!(*logs.borrow()), source["prefix"]);
        venue.dispose().unwrap();
    });
}

fn validation_source(setup: bool) -> Value {
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
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = String::from_utf8(output.stdout).unwrap();
    serde_json::from_str(output.lines().last().unwrap()).unwrap()
}

fn assert_validation_error(url: &str, source: Value) {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let (venue, _) = venue(json!({"key":"synthetic-key","secret":"synthetic-secret","overrideProxyUrl":url,"tuning":{"consoleLogging":false}}));
        let logs = Rc::new(RefCell::new(vec![]));
        let seen = logs.clone();
        venue.base().on("log", Rc::new(move |_, value| {
            let message = String::from_utf16(&value.to_js_string()?).unwrap();
            if message.contains("Failed to get account:") || message.contains("Account validation failed:") {
                seen.borrow_mut().push(message);
            }
            Ok(())
        }));
        let error = tokio::time::timeout(Duration::from_secs(10), venue.validate_account()).await.unwrap().unwrap_err();
        assert_eq!(error.name, source["name"], "ASTER-VALIDATION-ERROR source Error class");
        assert_eq!(error.code.as_ref().map(|value| value.to_display()), source["code"].as_str().map(str::to_owned), "ASTER-VALIDATION-ERROR source code");
        assert_eq!(error.is_axios_error, source["isAxiosError"].as_bool().unwrap(), "ASTER-VALIDATION-ERROR Axios marker");
        assert_eq!(error.message, source["message"], "ASTER-VALIDATION-ERROR enhanced message");
        assert_eq!(json!(*logs.borrow()), source["logs"], "ASTER-VALIDATION-ERROR nested diagnostics");
        venue.dispose().unwrap();
    });
}

#[test]
fn validation_setup_error_retains_the_source_axios_envelope() {
    assert_validation_error("unsupported://fixture", validation_source(true));
}

#[test]
fn validation_http_error_keeps_the_source_replacement_error() {
    let source = validation_source(false);
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
    assert_validation_error(&url, source);
    server.join().unwrap();
}

async fn request(receiver: &mpsc::Receiver<(String, String)>) -> (String, String) {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            match receiver.try_recv() {
                Ok(value) => return value,
                Err(mpsc::TryRecvError::Empty) => {
                    tokio::time::sleep(Duration::from_millis(5)).await
                }
                Err(error) => panic!("loopback request channel: {error}"),
            }
        }
    })
    .await
    .expect("ASTER-SDK-DROP discarded waiter must still send real signed HTTP")
}

#[test]
fn dropped_typed_and_unit_waiters_keep_signed_http_and_store_continuations_alive() {
    let source = source();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (sent, received) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(6);
        for _ in 0..3 {
            let (mut socket, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("loopback accept: {error}"),
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
            let header_end = bytes
                .windows(4)
                .position(|value| value == b"\r\n\r\n")
                .unwrap()
                + 4;
            let header = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < header_end + length {
                let count = socket.read(&mut buffer).unwrap();
                assert_ne!(count, 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            let mut parts = header.lines().next().unwrap().split_whitespace();
            let method = parts.next().unwrap().to_lowercase();
            let target = parts.next().unwrap();
            assert!(
                target.contains("signature="),
                "actual venue signer: {target}"
            );
            assert!(
                header
                    .to_lowercase()
                    .contains("x-mbx-apikey: synthetic-key")
            );
            let path = format!(
                "/fapi/{}",
                target
                    .split_once("/fapi/")
                    .unwrap()
                    .1
                    .split('?')
                    .next()
                    .unwrap()
            );
            sent.send((method, path.clone())).unwrap();
            if path.ends_with("positionSide/dual") {
                held.recv_timeout(Duration::from_secs(4)).unwrap();
            }
            let body = if path.ends_with("account") {
                r#"{"accountAlias":"native-account"}"#
            } else {
                "[]"
            };
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        }
    });
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let (venue, store) = venue(json!({"key":"synthetic-key","secret":"synthetic-secret","overrideProxyUrl":url,"tuning":{"consoleLogging":false}}));
        drop(venue.get_account());
        drop(venue.change_position_mode(true, None));
        let first = request(&received).await;
        let second = request(&received).await;
        assert!(!store.options().unwrap().is_hedged, "held HTTP must suspend the source Store update");
        release.send(()).unwrap();
        let third = request(&received).await;
        assert_eq!(json!([first, second, third]), source["requests"]);
        assert_eq!(store.options().unwrap().is_hedged, source["hedged"].as_bool().unwrap());
        venue.dispose().unwrap();
    });
    server.join().unwrap();
}
