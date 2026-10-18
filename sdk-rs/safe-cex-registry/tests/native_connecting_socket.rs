//! Real NativeRuntime handshakes and the production Bitget secondary callback.
#![cfg(not(target_arch = "wasm32"))]
use futures_util::{SinkExt, StreamExt};
use safe_cex_exchange::websocket::{WsEvent, WsRuntime};
use safe_cex_exchange::{native::native_runtime, native_store::DefaultStore};
use safe_cex_registry::{native_host, rust_venues};
use safe_cex_transport::{RestAdapter, native::reqwest_adapter};
use safe_cex_types::ExchangeName;
use safe_cex_venue_bitget::secondary_public::BitgetSecondaryPublicWebsocket;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use tokio::{io::AsyncReadExt, net::TcpListener, sync::oneshot};

#[test]
fn bitget_market_change_observes_connecting_handle_during_real_upgrade() {
    let host = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&host, async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let (seen, request_seen) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let server = tokio::task::spawn_local(async move {
            let (mut tcp, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                let mut bytes = [0; 1024];
                let count = tcp.read(&mut bytes).await.unwrap();
                assert_ne!(count, 0, "real native upgrade request");
                request.extend_from_slice(&bytes[..count]);
            }
            assert!(request.starts_with(b"GET / HTTP/1.1\r\n"));
            seen.send(()).unwrap();
            released.await.unwrap();
            assert_eq!(tokio::time::timeout(Duration::from_secs(5), tcp.read(&mut [0; 1])).await.unwrap().unwrap(), 0,
                "CONNECTING disposal releases the physical upgrade");
        });
        let runtime = native_runtime();
        let parent_host = native_host::native_venue_host_with(
            runtime.clone(),
            Rc::new(|| {
                reqwest_adapter()
                    .map(|value| Arc::new(value) as Arc<dyn RestAdapter>)
                    .map_err(|error| {
                        safe_cex_exchange::runtime::ExchangeError::new(error.to_string())
                    })
            }),
            native_host::hyperliquid_rate_limiter(),
        );
        let parent = rust_venues::bitget_constructor(parent_host)
            .construct(
                ExchangeName::Bitget,
                serde_json::from_value(
                    json!({"key":"","secret":"","tuning":{"consoleLogging":false}}),
                )
                .unwrap(),
                Rc::new(DefaultStore::new()),
            )
            .unwrap();
        let socket = BitgetSecondaryPublicWebsocket::new(5001, &parent, runtime.clone(), 1.0);
        let logs = Rc::new(RefCell::new(Vec::new()));
        let observed = logs.clone();
        parent.base().on(
            "log",
            Rc::new(move |_, value| {
                observed
                    .borrow_mut()
                    .push(String::from_utf16(&value.to_js_string()?).unwrap());
                Ok(())
            }),
        );
        // Only the terminal URL is supplied; the actual native dial and Bitget
        // callback/Store/parent/log/reconnect implementations remain in use.
        socket.base().start_websocket(url, vec![], None);
        tokio::time::timeout(Duration::from_secs(5), request_seen)
            .await
            .unwrap()
            .unwrap();
        assert!(
            socket.base().has_websocket(),
            "CONNECTING handle is present before upgrade"
        );
        socket
            .on_markets_changed(&["SKIP".into(), "NEW".into(), "OTHER".into()])
            .unwrap();
        let keys: Vec<String> = socket
            .topics()
            .borrow()
            .get("ticker")
            .map(|rows| rows.keys().cloned().collect())
            .unwrap_or_default();
        assert_eq!(
            keys,
            ["NEW"],
            "CONNECTING Bitget market callback retains the source-selected topic before upgrade"
        );
        assert!(
            !socket.base().is_connected(),
            "CONNECTING handle is not OPEN"
        );
        assert!(
            logs.borrow()
                .iter()
                .any(|message| message.contains("state: CONNECTING, stateCode: 0")),
            "CONNECTING BaseWebSocket.send observes the actual wrapper state"
        );
        assert_eq!(
            socket.base().diagnostics().reconnect_count,
            1,
            "CONNECTING send invokes the existing reconnect prefix"
        );
        tokio::time::timeout(Duration::from_secs(5), async {
            while socket.base().diagnostics().on_close_count == 0 { tokio::task::yield_now().await; }
        }).await.unwrap();
        let diagnostics = socket.base().diagnostics();
        let actual = json!({
            "keys": keys,
            "warning": logs.borrow().iter().any(|message| message.contains("state: CONNECTING, stateCode: 0")),
            "reconnectCount":diagnostics.reconnect_count,
            "closeCount":diagnostics.on_close_count,
            "closeCode":diagnostics.last_close_code,
            "errorCount":logs.borrow().iter().filter(|message|message.contains("WebSocket error:")).count(),
        });
        assert_eq!(actual, source()["markets"], "CONNECTING callback effects equal maintained Bitget/BaseWebSocket");
        assert!(logs.borrow().iter().any(|message| message.contains("WebSocket error: error")), "CONNECTING source ErrorEvent type fallback");
        socket.base().dispose();
        parent.dispose().unwrap();
        release.send(()).unwrap();
        server.await.unwrap();
    });
}

fn source() -> &'static Value {
    static SOURCE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .args([
                "--import",
                "tsx",
                "sdk-rs/safe-cex-registry/tests/connecting_socket_source.cjs",
            ])
            .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "CONNECTING maintained source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let row = stdout
            .lines()
            .find_map(|line| line.strip_prefix("CONNECTING_RESULT "))
            .unwrap();
        serde_json::from_str(row).unwrap()
    })
}

async fn held_upgrade() -> (String, oneshot::Receiver<()>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (seen, received) = oneshot::channel();
    let server = tokio::task::spawn_local(async move {
        let (mut tcp, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
            let mut bytes = [0; 1024];
            let count = tcp.read(&mut bytes).await.unwrap();
            assert_ne!(count, 0);
            request.extend_from_slice(&bytes[..count]);
        }
        seen.send(()).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), tcp.read(&mut [0; 1]))
                .await
                .unwrap()
                .unwrap(),
            0,
            "CONNECTING cancellation releases pending physical I/O"
        );
    });
    (url, received, server)
}

#[test]
fn connecting_send_close_matches_source_and_wakes_registered_receiver() {
    let host = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&host, async {
        let (url, received, server) = held_upgrade().await;
        let runtime = native_runtime();
        let socket = runtime
            .dial(url, vec![], Default::default(), false)
            .await
            .unwrap();
        assert_eq!(
            socket.ready_state(),
            0,
            "CONNECTING native handle returned before upgrade"
        );
        let error = socket.send("while-connecting").unwrap_err();
        assert_eq!(
            json!({"name":error.name,"message":error.message}),
            source()["rawClose"]["sendError"],
            "CONNECTING direct send has source InvalidStateError"
        );
        tokio::time::timeout(Duration::from_secs(5), received)
            .await
            .unwrap()
            .unwrap();
        let event = socket.next_event();
        let (deliver, delivered) = oneshot::channel();
        let receiver = tokio::task::spawn_local(async move {
            deliver.send(event.await.unwrap()).ok();
        });
        tokio::task::yield_now().await;
        socket.close();
        socket.close(); // Repeated close must not restart a failed handshake.
        assert_eq!(
            json!(socket.ready_state()),
            source()["rawClose"]["immediate"]
        );
        let event = tokio::time::timeout(Duration::from_secs(5), delivered)
            .await
            .unwrap()
            .unwrap();
        // WsEvent carries the normalized source error message (empty message
        // falls back to ErrorEvent.type in the actual BaseWebSocket listener).
        assert_eq!(source()["rawClose"]["events"][0], json!(["error", ""]));
        assert!(
            matches!(event,WsEvent::Error(message) if message == "error"),
            "CONNECTING close dispatches source Error first"
        );
        let close = socket.next_event().await.unwrap();
        assert!(
            matches!(close,WsEvent::Close{code:Some(1006),reason} if reason.is_empty()),
            "CONNECTING close dispatches source Close1006 second"
        );
        assert_eq!(json!(socket.ready_state()), source()["rawClose"]["ended"]);
        let mut after = socket.next_event();
        assert!(
            after
                .as_mut()
                .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
                .is_pending(),
            "CONNECTING closed stream never emits Open"
        );
        assert!(socket.send("after-close").is_ok());
        receiver.await.unwrap();
        server.await.unwrap();
    });
}

#[test]
fn connecting_terminate_and_final_drop_cancel_upgrade_without_late_events() {
    let host = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&host, async {
        for terminate in [false, true] {
            let (url, received, server) = held_upgrade().await;
            let runtime = native_runtime();
            let socket = runtime
                .dial(url, vec![], Default::default(), false)
                .await
                .unwrap();
            assert_eq!(socket.ready_state(), 0);
            tokio::time::timeout(Duration::from_secs(5), received)
                .await
                .unwrap()
                .unwrap();
            // The waiter survives its last socket handle, but owns no handshake.
            let mut event = socket.next_event();
            assert!(
                event
                    .as_mut()
                    .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
                    .is_pending()
            );
            if terminate {
                socket.terminate();
            }
            drop(socket);
            server.await.unwrap();
            assert!(
                event
                    .as_mut()
                    .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
                    .is_pending(),
                "CONNECTING teardown suppresses late lifecycle events"
            );
        }
    });
}

#[test]
fn connecting_handle_survives_upgrade_and_delivers_one_open_then_real_frames() {
    let host = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&host, async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}",listener.local_addr().unwrap());
        let server = tokio::task::spawn_local(async move {
            let (tcp,_) = listener.accept().await.unwrap();
            let mut websocket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let message = websocket.next().await.unwrap().unwrap();
            assert_eq!(message.to_text().unwrap(),"real-frame");
            websocket.send(tokio_tungstenite::tungstenite::Message::text("echo")).await.unwrap();
            websocket.close(None).await.unwrap();
        });
        let runtime = native_runtime();
        let socket = runtime.dial(url,vec![],Default::default(),false).await.unwrap();
        let retained = socket.clone();
        assert_eq!(socket.ready_state(),0);
        assert!(matches!(tokio::time::timeout(Duration::from_secs(5),socket.next_event()).await.unwrap().unwrap(),WsEvent::Open));
        assert!(Rc::ptr_eq(&socket,&retained),"CONNECTING upgrade retains handle identity");
        assert_eq!(socket.ready_state(),1);
        socket.send("real-frame").unwrap();
        assert!(matches!(socket.next_event().await.unwrap(),WsEvent::Message(safe_cex_transport::socket::SocketFrame::Text(text)) if text == "echo"),"CONNECTING upgrade emits one Open then frames");
        assert!(matches!(socket.next_event().await.unwrap(),WsEvent::Close{..}));
        assert!(socket.next_event().as_mut().poll(&mut std::task::Context::from_waker(std::task::Waker::noop())).is_pending());
        server.await.unwrap();
    });
}
