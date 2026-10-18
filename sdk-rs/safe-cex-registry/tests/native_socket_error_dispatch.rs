//! Native host dispatch against the maintained venues' actual WebSocket listeners.
#![cfg(not(target_arch = "wasm32"))]
use futures_util::SinkExt;
use safe_cex_exchange::socket_lifecycle::SocketSlot;
use safe_cex_registry::{native_host, rust_venues};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};
use tokio_tungstenite::tungstenite::{
    Message,
    protocol::{CloseFrame, frame::coding::CloseCode},
};

fn source(name: &str) -> Value {
    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/socket_error_source.cjs"),
            name,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let receipts: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("SOCKET_RESULT "))
        .collect();
    assert_eq!(receipts.len(), 1, "one source receipt: {stdout}");
    serde_json::from_str(receipts[0]).unwrap()
}

fn construct(name: ExchangeName) -> Rc<dyn safe_cex_exchange::exchange::Exchange> {
    let host = native_host::native_venue_host(native_host::hyperliquid_rate_limiter());
    let constructor = match name {
        ExchangeName::Asterdex => rust_venues::asterdex_constructor(host),
        ExchangeName::Bitunix => rust_venues::bitunix_constructor(host),
        _ => unreachable!(),
    };
    constructor
        .construct(
            name.clone(),
            serde_json::from_value(
                json!({"key":"", "secret":"", "tuning":{"consoleLogging":false}}),
            )
            .unwrap(),
            Rc::new(DefaultStore::new()),
        )
        .unwrap()
}

async fn observe(name: ExchangeName, rejected_handshake: bool, expected: &Value) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (release, released) = oneshot::channel();
    let (close, closed) = oneshot::channel();
    let server = tokio::task::spawn_local(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        if rejected_handshake {
            let mut tcp = tcp;
            let mut request = Vec::new();
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let mut bytes = [0; 1024];
                let length = tcp.read(&mut bytes).await.unwrap();
                assert_ne!(length, 0, "actual WebSocket upgrade request");
                request.extend_from_slice(&bytes[..length]);
            }
            assert!(request.starts_with(b"GET / HTTP/1.1\r\n"));
            tcp.write_all(
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
            tcp.shutdown().await.unwrap();
        } else {
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            closed.await.unwrap();
            socket
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::Away,
                    reason: "server close".into(),
                })))
                .await
                .unwrap();
            released.await.unwrap();
            return;
        }
        released.await.unwrap();
    });
    let venue = construct(name.clone());
    let logs = Rc::new(RefCell::new(Vec::new()));
    let seen = logs.clone();
    venue.base().on(
        "log",
        Rc::new(move |_, value| {
            seen.borrow_mut()
                .push(String::from_utf16(&value.to_js_string()?).unwrap());
            Ok(())
        }),
    );
    let socket = venue.base().get_socket(SocketSlot::Public).unwrap();
    socket.start_websocket(url, vec![], None);
    if !rejected_handshake {
        tokio::time::timeout(Duration::from_secs(5), async {
            while socket.diagnostics().last_connected_at == 0.0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("actual native open");
        socket
            .handle_web_socket_error("native transport failed")
            .unwrap();
        assert_eq!(
            socket.diagnostics().on_close_count,
            expected["afterError"].as_u64().unwrap() as usize
        );
        assert!(
            !logs
                .borrow()
                .iter()
                .any(|line| line.contains("Reconnecting in")),
            "error alone must not schedule reconnect"
        );
        close.send(()).unwrap();
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let done = if rejected_handshake {
                logs.borrow()
                    .iter()
                    .any(|line| line.contains("WebSocket error:"))
            } else {
                socket.diagnostics().on_close_count > 0
            };
            if done {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual native transport event");
    let health = socket.diagnostics();
    let actual = json!({
        "count": health.on_close_count,
        "lastCloseCode": health.last_close_code,
        "reconnectScheduled": logs.borrow().iter().any(|line| line.contains("Reconnecting in")),
    });
    venue.dispose().unwrap();
    release.send(()).unwrap();
    server.await.unwrap();
    let expected = if rejected_handshake {
        expected
    } else {
        &expected["closed"]
    };
    assert_eq!(
        actual, *expected,
        "{name:?}: independent error suppression and close recovery"
    );
}

fn check(name: ExchangeName, source_name: &str, failed_upgrade: bool) {
    let expected = source(source_name);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async {
        let key = if failed_upgrade {
            "failedUpgrade"
        } else {
            "live"
        };
        observe(name, failed_upgrade, &expected[key]).await;
    });
}

#[test]
fn aster_native_error_dispatch_stops_later_close_handler() {
    check(ExchangeName::Asterdex, "asterdex", false);
}
#[test]
fn bitunix_native_error_dispatch_stops_later_close_handler() {
    check(ExchangeName::Bitunix, "bitunix", false);
}
#[test]
fn aster_failed_upgrade_delivers_abnormal_close_and_reconnect() {
    check(ExchangeName::Asterdex, "asterdex", true);
}
#[test]
fn bitunix_failed_upgrade_delivers_abnormal_close_and_reconnect() {
    check(ExchangeName::Bitunix, "bitunix", true);
}

#[test]
fn native_invalid_url_does_not_deliver_close_or_schedule_reconnect() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async {
        for (name, source_name) in [(ExchangeName::Asterdex, "asterdex"), (ExchangeName::Bitunix, "bitunix")] {
            let expected = source(source_name);
            let venue = construct(name);
            let logs = Rc::new(RefCell::new(Vec::new()));
            let seen = logs.clone();
            venue.base().on("log", Rc::new(move |_, value| {
                seen.borrow_mut().push(String::from_utf16(&value.to_js_string()?).unwrap());
                Ok(())
            }));
            let socket = venue.base().get_socket(SocketSlot::Public).unwrap();
            socket.start_websocket("invalid".into(), vec![], None);
            tokio::time::timeout(Duration::from_secs(5), async {
                while !logs.borrow().iter().any(|line| line.contains("WebSocket error:")) {
                    tokio::task::yield_now().await;
                }
            }).await.expect("constructor failure reported");
            let actual = json!({
                "count": socket.diagnostics().on_close_count,
                "reconnectScheduled": logs.borrow().iter().any(|line| line.contains("Reconnecting in")),
            });
            venue.dispose().unwrap();
            assert_eq!(actual, expected["invalidURL"]);
        }
    });
}
