use super::{BingxDependencies, BingxExchange};
use crate::listen_key::{ListenKeyHost, PrivateListener};
use futures_util::SinkExt;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client,
        value::{from_typed, to_retained_input},
    },
    native::native_runtime,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::{ExchangeName, ExchangeOptions, OrderStatus};
use std::{
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
#[path = "retained_observe.rs"]
mod observation;

const FRAME: &str = r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":"retained-order","s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"NEW","q":"3","z":"0","p":"120","sp":"0","f":"GTC","ro":false,"opaque":{"\ud802":{"v":"\udc01"},"positive":1e999,"negative":-1e999,"zero":-0,"text":"\ud800","array":[1e999,-0,"\udfff"]}}}"#;

#[test]
fn native_private_socket_and_store_keep_opaque_bingx_info() {
    let mut source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private_events_oracle.cjs"
        ))
        .arg("--retained-info")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    source
        .stdin
        .take()
        .unwrap()
        .write_all(FRAME.as_bytes())
        .unwrap();
    let source = source.wait_with_output().unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let (release, held) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::task::spawn_local(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            socket
                .send(tokio_tungstenite::tungstenite::Message::Text(FRAME.into()))
                .await
                .unwrap();
            held.await.unwrap();
        });
        let runtime = native_runtime();
        let http = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let store = Rc::new(DefaultStore::new());
        let options: ExchangeOptions =
            serde_json::from_value(serde_json::json!({"key":"synthetic","secret":"synthetic"}))
                .unwrap();
        let venue = BingxExchange::new(
            ExchangeName::Bingx,
            options,
            store.clone(),
            BingxDependencies {
                runtime,
                http,
                local_wall_time_to_utc: Rc::new(|v| v),
                external_public_data: None,
            },
        )
        .unwrap();
        let socket = ListenKeyHost::create_socket(venue.host.as_ref(), &url).unwrap();
        ListenKeyHost::add_listener(venue.host.as_ref(), socket, PrivateListener::Message).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while store.orders().is_empty() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("actual BingX private frame must commit its retained order");
        let orders = store.orders();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].id, "retained-order");
        assert_eq!(orders[0].symbol, "BTCUSDT");
        assert_eq!(orders[0].status, OrderStatus::Open);
        assert_eq!(orders[0].amount.to_bits(), 3f64.to_bits());
        let order = from_typed(&orders[0]).unwrap();
        let info = to_retained_input(&order.get("info").unwrap()).unwrap();
        assert_eq!(
            observation::observe(&info),
            expected,
            "native BingX opaque info"
        );
        drop(orders);
        venue.dispose().unwrap();
        release.send(()).unwrap();
        server.await.unwrap();
    });
}
