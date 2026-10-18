//! Native host dispatch against the maintained venues' actual WebSocket listeners.
#![cfg(not(target_arch = "wasm32"))]
use futures_util::SinkExt;
use safe_cex_exchange::socket_lifecycle::SocketSlot;
use safe_cex_registry::{native_host, rust_venues};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, time::Duration};
use tokio::{io::AsyncWriteExt, net::TcpListener, sync::oneshot};
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

async fn observe(name: ExchangeName, malformed: bool, expected: &Value) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (release, released) = oneshot::channel();
    let server = tokio::task::spawn_local(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
        if malformed {
            socket.get_mut().write_all(&[0x83, 0x00]).await.unwrap();
        } else {
            socket
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::Away,
                    reason: "server close".into(),
                })))
                .await
                .unwrap();
        }
        released.await.unwrap();
    });
    let host = native_host::native_venue_host(native_host::hyperliquid_rate_limiter());
    let constructor = match name {
        ExchangeName::Asterdex => rust_venues::asterdex_constructor(host),
        ExchangeName::Bitunix => rust_venues::bitunix_constructor(host),
        _ => unreachable!(),
    };
    let venue = constructor
        .construct(
            name.clone(),
            serde_json::from_value(json!({"tuning":{"consoleLogging":false}})).unwrap(),
            Rc::new(DefaultStore::new()),
        )
        .unwrap();
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
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let done = if malformed {
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
    // Error logging happens inside the same synchronous dispatch as the venue hook.
    let count = socket.diagnostics().on_close_count;
    venue.dispose().unwrap();
    release.send(()).unwrap();
    server.await.unwrap();
    assert_eq!(
        count,
        expected.as_u64().unwrap() as usize,
        "{name:?}: base error listener suppresses later handlers; actual close still runs"
    );
    if malformed {
        assert!(
            !logs
                .borrow()
                .iter()
                .any(|line| line.contains("Reconnecting in")),
            "error alone must not schedule reconnect"
        );
    }
}

fn check(name: ExchangeName, source_name: &str) {
    let expected = source(source_name);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async {
        observe(name.clone(), true, &expected["afterError"]).await;
        observe(name, false, &expected["afterClose"]).await;
    });
}

#[test]
fn aster_native_error_dispatch_stops_later_close_handler() {
    check(ExchangeName::Asterdex, "asterdex");
}
#[test]
fn bitunix_native_error_dispatch_stops_later_close_handler() {
    check(ExchangeName::Bitunix, "bitunix");
}
