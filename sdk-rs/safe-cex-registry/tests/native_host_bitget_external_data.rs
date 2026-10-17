//! Bitget gateway startup and source-equivalent direct-socket stream overrides.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use support::*;

fn frame(h: &Harness, kind: &str, exchange: &str, channel: &str) -> bool {
    h.received.borrow().iter().any(|text| {
        let value: Value = serde_json::from_str(text).unwrap();
        value["type"] == kind && value["exchangeId"] == exchange && value["channel"] == channel
    })
}
fn ticker(last: f64) -> Value {
    json!({"id":"GWUSDT","symbol":"GWUSDT","bid":9.9,"ask":10.1,"last":last,"mark":10.0,"index":10.0,
        "percentage":1.0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":1,"quoteVolume":10})
}
async fn verify(name: ExchangeName, exchange: &'static str) {
    let h = Rc::new(venue_harness(name, Rc::new(|_|None), json!({"key":"","secret":"","publicDataOnly":true,
        "externalPublicData":{"type":"websocket","url":"ws://gateway.local/gateway","exchangeId":exchange}}),None,safe_cex_exchange::native::native_runtime()).await);
    let gateway = h.clone();
    tokio::task::spawn_local(async move {
        eventually("Bitget market/ticker subscriptions", || {
            frame(&gateway, "subscribe", exchange, "market")
                && frame(&gateway, "subscribe", exchange, "tickers")
        })
        .await;
        let push = gateway
            .peers
            .borrow()
            .iter()
            .find(|p| p.path == "/gateway")
            .unwrap()
            .push
            .clone();
        let market = json!({"id":"GWUSDT","symbol":"GWUSDT","base":"GW","quote":"USDT","active":true,
            "precision":{"amount":0.1,"price":0.01},"limits":{"amount":{"min":0.1,"max":100},"leverage":{"min":1,"max":20}}});
        for (channel, data) in [("market", market), ("tickers", ticker(10.0))] {
            push.send(
                json!({"type":"data","exchangeId":exchange,"channel":channel,"data":[data]})
                    .to_string(),
            )
            .unwrap();
        }
    });
    h.exchange
        .start()
        .await
        .expect("Bitget starts with native external provider");
    assert_eq!(h.store.markets()[0].symbol, "GWUSDT");
    assert_eq!(h.store.markets()[0].precision.amount, 0.1);
    assert_eq!(h.store.tickers()[0].last, 10.0);
    {
        let loaded = h.store.loaded();
        assert!(
            loaded.markets && loaded.tickers && loaded.balance && loaded.positions && loaded.orders
        );
    }
    assert!(
        h.seen.borrow().is_empty(),
        "gateway public-only startup must avoid venue REST"
    );
    assert_eq!(
        h.peers
            .borrow()
            .iter()
            .map(|p| p.path.as_str())
            .collect::<Vec<_>>(),
        ["/gateway"],
        "only external gateway is dialed"
    );

    let books = Rc::new(RefCell::new(vec![]));
    let sink = books.clone();
    let clean_book = h
        .exchange
        .listen_order_book(
            "GWUSDT".into(),
            Rc::new(move |book| sink.borrow_mut().push(book)),
        )
        .unwrap();
    let candles = Rc::new(RefCell::new(vec![]));
    let sink = candles.clone();
    let clean_candle = h
        .exchange
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"GWUSDT","interval":"1m"})).unwrap(),
            Rc::new(move |candle| sink.borrow_mut().push(candle)),
        )
        .unwrap();
    let push = h
        .peers
        .borrow()
        .iter()
        .find(|p| p.path == "/gateway")
        .unwrap()
        .push
        .clone();
    let book=json!({"type":"data","exchangeId":exchange,"channel":"orderbook","symbol":"GWUSDT",
        "data":{"bids":[{"price":9.9,"amount":2,"total":2}],"asks":[{"price":10.1,"amount":3,"total":3}]}}).to_string();
    let candle=json!({"type":"data","exchangeId":exchange,"channel":"candle","symbol":"GWUSDT","interval":"1m",
        "data":{"timestamp":1700000000000.0,"open":9.0,"high":12.0,"low":8.0,"close":11.0,"volume":17.0}}).to_string();
    push.send(book.clone()).unwrap();
    push.send(candle.clone()).unwrap();
    // A later ticker on this same ordered socket proves both stream frames were
    // processed. Bitget's own stream overrides delegate to its disconnected
    // venue socket, unlike BaseExchange's gateway-aware defaults.
    push.send(
        json!({"type":"data","exchangeId":exchange,"channel":"tickers","data":[ticker(10.5)]})
            .to_string(),
    )
    .unwrap();
    eventually("gateway stream processing barrier", || {
        h.store.tickers()[0].last == 10.5
    })
    .await;
    let observation = json!({
        "bookSubscribed": frame(&h, "subscribe", exchange, "orderbook"),
        "candleSubscribed": frame(&h, "subscribe", exchange, "candle"),
        "books": books.borrow().len(),
        "candles": candles.borrow().len(),
    });
    assert_eq!(
        observation,
        source_stream_observation(),
        "Bitget actual TS stream routing"
    );
    clean_book().unwrap();
    clean_candle().unwrap();
    assert!(!frame(&h, "unsubscribe", exchange, "orderbook"));
    assert!(!frame(&h, "unsubscribe", exchange, "candle"));
    push.send(book).unwrap();
    push.send(candle).unwrap();
    // A later ticker on the same ordered socket is a processing barrier.
    push.send(
        json!({"type":"data","exchangeId":exchange,"channel":"tickers","data":[ticker(11.0)]})
            .to_string(),
    )
    .unwrap();
    eventually("gateway processing barrier", || {
        h.store.tickers()[0].last == 11.0
    })
    .await;
    assert!(
        books.borrow().is_empty(),
        "gateway book does not reach venue callback"
    );
    assert!(
        candles.borrow().is_empty(),
        "gateway candle does not reach venue callback"
    );
    assert!(h.seen.borrow().is_empty());
    assert_eq!(
        h.peers.borrow().len(),
        1,
        "streams do not dial a venue socket"
    );
    h.exchange.dispose().unwrap();
    assert!(h.exchange.base().is_disposed());
}
#[test]
fn bitget_uses_native_external_gateway() {
    run_local(verify(ExchangeName::Bitget, "bitget"));
}
#[test]
fn bitget_usdc_uses_native_external_gateway() {
    run_local(verify(ExchangeName::Bitgetusdc, "bitgetusdc"));
}
#[test]
fn bitget_inverse_uses_native_external_gateway() {
    run_local(verify(ExchangeName::Bitgetinverse, "bitgetinverse"));
}

fn source_stream_observation() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/bitget_external_streams.cjs"
        ))
        .output()
        .expect("execute actual Bitget source");
    assert!(
        output.status.success(),
        "Bitget stream source witness: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("Bitget source observation")
}
