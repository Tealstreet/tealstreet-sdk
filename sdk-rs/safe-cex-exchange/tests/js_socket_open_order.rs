#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    js_runtime::{
        client, globals,
        promise::Promise,
        scope::ScopeShared,
        sockets::base_websocket,
        value::{Js, Thrown},
    },
    native::native_runtime,
    runtime::ExchangeRuntime,
};
use std::{cell::RefCell, process::Command, rc::Rc, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Owner(BaseExchange, ExchangeCapabilities);
impl Exchange for Owner {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.0
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.1
    }
}

#[test]
fn native_open_prefix_precedes_buffered_first_frame_without_waiting_for_async_listener() {
    let output = Command::new("node")
        .args([
            "-e",
            r#"
(async()=>{
  const rows=[];
  for(const asynchronous of [false,true]) {
    const socket=new EventTarget(),trace=[];
    socket.addEventListener('open', asynchronous ? async()=>{
      trace.push('open-prefix');await new Promise(r=>setTimeout(r,25));trace.push('open-complete');
    } : ()=>trace.push('open-prefix'));
    socket.addEventListener('message',()=>trace.push('message'));
    socket.dispatchEvent(new Event('open'));socket.dispatchEvent(new Event('message'));
    await new Promise(r=>setTimeout(r,40));rows.push(trace);
  }
  console.log(JSON.stringify(rows));
})().catch(e=>{console.error(e);process.exitCode=1});
"#,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Vec<String>> = serde_json::from_slice(&output.stdout).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        for headers in [false, true] {
            for asynchronous in [false, true] {
                let actual = native_open_trace(headers, asynchronous).await;
                assert_eq!(
                    actual,
                    expected[usize::from(asynchronous)],
                    "headers={headers}, async={asynchronous}"
                );
            }
        }
    });
}

async fn native_open_trace(headers: bool, asynchronous: bool) -> Vec<String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::task::spawn_local(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(stream.read_u8().await.unwrap());
            assert!(request.len() < 8192, "bounded websocket handshake");
        }
        let request = String::from_utf8(request).unwrap();
        let key = request
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("sec-websocket-key")
                    .then(|| value.trim())
            })
            .unwrap();
        let accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes());
        let mut packet = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        ).into_bytes();
        // One packet makes the first frame available when the upgrade completes.
        packet.extend_from_slice(b"\x81\x07welcome");
        stream.write_all(&packet).await.unwrap();
        released.await.unwrap();
    });
    let runtime = native_runtime();
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    let http = client::rest_http_host(
        runtime.clone(),
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
    );
    let shared = Rc::new(ScopeShared::new(
        runtime.clone(),
        http,
        store.clone(),
        Js::object(vec![]),
        (),
    ));
    globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
    let owner: Rc<dyn Exchange> = Rc::new(Owner(
        BaseExchange::new(
            safe_cex_types::ExchangeName::Kucoin,
            serde_json::from_value(serde_json::json!({"key":"", "secret":""})).unwrap(),
            store,
            runtime.clone(),
            ExchangeSettings::default(),
        ),
        ExchangeCapabilities::default(),
    ));
    *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&owner));
    let socket = base_websocket(&shared.sockets, Js::object(vec![]));
    let options = if headers {
        Js::object(vec![(
            "headers",
            Js::object(vec![("X-Witness", Js::str("open-order"))]),
        )])
    } else {
        Js::Undefined
    };
    let ws = socket
        .call_method(
            "createWebsocket",
            vec![Js::str(&url), Js::Undefined, options],
        )
        .unwrap();
    let trace = Rc::new(RefCell::new(Vec::<String>::new()));
    let opened = trace.clone();
    let runtime: Rc<dyn ExchangeRuntime> = runtime;
    ws.call_method(
        "addEventListener",
        vec![
            Js::str("open"),
            Js::function("onOpen", move |_, _| {
                opened.borrow_mut().push("open-prefix".into());
                if !asynchronous {
                    return Ok(Js::Undefined);
                }
                let trace = opened.clone();
                let sleeping = runtime.clone();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        sleeping.sleep(25).await.map_err(Thrown::from)?;
                        trace.borrow_mut().push("open-complete".into());
                        Ok(Js::Undefined)
                    }),
                )))
            }),
        ],
    )
    .unwrap();
    let messages = trace.clone();
    ws.call_method(
        "addEventListener",
        vec![
            Js::str("message"),
            Js::function("onMessage", move |_, _| {
                messages.borrow_mut().push("message".into());
                Ok(Js::Undefined)
            }),
        ],
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while trace.borrow().len() < if asynchronous { 3 } else { 2 } {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("native socket must deliver open and buffered message");
    let actual = trace.borrow().clone();
    socket.call_method("dispose", vec![]).unwrap();
    release.send(()).unwrap();
    server.await.unwrap();
    actual
}
