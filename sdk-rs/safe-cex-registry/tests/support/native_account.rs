#![allow(dead_code)]
//! Real account-surface fixture: production registry and native HTTP/WS hosts.
pub use crate::support::run_local as run;
use crate::support::{Seen, WsPeer, eventually, serve_http, serve_ws};
use safe_cex_registry::native_account::{
    NativeAccount, NativeAccountSpec, native_account_call, open_native_account,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use tokio::net::TcpListener;

pub struct Account {
    pub account: Rc<NativeAccount>,
    pub seen: Rc<RefCell<Vec<Seen>>>,
    pub peers: Rc<RefCell<Vec<WsPeer>>>,
    pub received: Rc<RefCell<Vec<String>>>,
}
pub fn order() -> Value {
    json!({"orderId":77,"clientOrderId":"original","symbol":"BTCUSDT","side":"BUY","type":"LIMIT",
        "origQty":"1","price":"10","stopPrice":"0","status":"NEW","executedQty":"0",
        "reduceOnly":false,"positionSide":"BOTH","time":1})
}
pub fn options() -> Value {
    json!({"symbol":"BTCUSDT","side":"buy","type":"limit","amount":1,"price":10})
}
impl Account {
    pub async fn call(&self, method: &str, args: Value) -> Result<Value, Value> {
        native_account_call(
            self.account.clone(),
            method.into(),
            args.as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
        )
        .await
        .map(crate::support::boundary_json)
        .map_err(crate::support::boundary_json)
    }
    pub async fn ok(&self, method: &str, args: Value) -> Value {
        self.call(method, args)
            .await
            .unwrap_or_else(|error| panic!("{method}: {error}"))
    }
}
pub async fn opened(extra_options: Value) -> Account {
    let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", http.local_addr().unwrap());
    let gateway = format!("ws://{}/gateway", ws.local_addr().unwrap());
    let seen = Rc::new(RefCell::new(vec![]));
    tokio::task::spawn_local(serve_http(
        http,
        Rc::new(|request: &Seen| {
            match (
                request.method.as_str(),
                request.path.rsplit(".com").next().unwrap(),
            ) {
                ("POST", "/fapi/v1/order") => Some(json!({"orderId":9001})),
                ("GET", "/fapi/v1/openOrders") => Some(json!([order()])),
                ("GET", "/fapi/v1/openAlgoOrders") => Some(json!([])),
                ("DELETE", "/fapi/v1/allOpenOrders" | "/fapi/v1/algoOpenOrders") => Some(json!({})),
                _ => panic!("unexpected native HTTP: {request:?}"),
            }
        }),
        seen.clone(),
    ));
    let peers: Rc<RefCell<Vec<WsPeer>>> = Rc::default();
    let received = Rc::new(RefCell::new(vec![]));
    tokio::task::spawn_local(serve_ws(ws, peers.clone(), received.clone()));
    let snapshot_peers = peers.clone();
    let snapshot_received = received.clone();
    tokio::task::spawn_local(async move {
        let peers = snapshot_peers;
        let received = snapshot_received;
        eventually("gateway market/ticker subscriptions", || {
            received
                .borrow()
                .iter()
                .any(|frame| frame.contains("\"market\""))
                && received
                    .borrow()
                    .iter()
                    .any(|frame| frame.contains("\"tickers\""))
        })
        .await;
        let push = peers.borrow()[0].push.clone();
        let market = json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
            "precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":100},"leverage":{"min":1,"max":20}}});
        let ticker = json!({"id":"BTCUSDT","symbol":"BTCUSDT","bid":9.9,"ask":10.1,"last":10,"mark":10,"index":10,
            "percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":1,"quoteVolume":10});
        for (channel, data) in [("market", market), ("tickers", ticker)] {
            push.send(
                json!({"type":"data","exchangeId":"binance","channel":channel,"data":[data]})
                    .to_string(),
            )
            .unwrap();
        }
    });
    let mut options = json!({"key":"test-key","secret":"test-secret","publicDataOnly":true,"overrideProxyUrl":origin,
        "externalPublicData":{"type":"websocket","url":gateway,"exchangeId":"binance"}});
    options
        .as_object_mut()
        .unwrap()
        .extend(extra_options.as_object().unwrap().clone());
    let account = open_native_account(NativeAccountSpec {
        hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
        exchange: "binance".into(),
        options: options.into(),
        account_name: None,
        sign: None,
        proxy_token: None,
    })
    .unwrap();
    let account = Account {
        account,
        seen,
        peers,
        received,
    };
    account
}
pub async fn started() -> Account {
    let account = opened(json!({})).await;
    account.ok("start", json!([])).await;
    account
}
