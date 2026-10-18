//! externalPublicData on the production native host: Binance takes markets, tickers
//! and order books from a local public-data gateway socket instead of venue REST.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
use support::*;

fn gateway_peer(h: &Harness) -> Option<tokio::sync::mpsc::UnboundedSender<String>> {
    h.peers
        .borrow()
        .iter()
        .find(|p| p.path == "/gateway")
        .map(|p| p.push.clone())
}
fn subscribed(h: &Harness, channel: &str) -> bool {
    h.received.borrow().iter().any(|frame| {
        let value: serde_json::Value = serde_json::from_str(frame).unwrap_or_default();
        value["type"] == "subscribe"
            && value["exchangeId"] == "binance"
            && value["channel"] == channel
    })
}

#[test]
fn gateway_type_audit_uses_channel_fields_without_envelope_validation() {
    run_local(async {
        let options = json!({"key":"","secret":"","publicDataOnly":true,
            "externalPublicData":{"type":"websocket","url":"ws://gateway.local/gateway",
                "exchangeId":"binance","authToken":"audit-token"}});
        let h = Rc::new(binance_harness_with(Rc::new(|_| None), options).await);
        let gateway = h.clone();
        tokio::task::spawn_local(async move {
            eventually("audit gateway subscriptions", || {
                subscribed(&gateway, "tickers") && subscribed(&gateway, "market")
            })
            .await;
            let push = gateway_peer(&gateway).unwrap();
            let market = json!({"id":"GWUSDT","symbol":"GWUSDT","base":"GW","quote":"USDT","active":true,
                "precision":{"amount":0.1,"price":0.01},"limits":{"amount":{"min":0.1,"max":100},"leverage":{"min":1,"max":20}}});
            let ticker = json!({"id":"GWUSDT","symbol":"GWUSDT","bid":9.9,"ask":10.1,"last":10,"mark":10,"index":10,
                "percentage":1,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":1,"quoteVolume":10});
            push.send(
                json!({"type":"subscribed","exchangeId":"binance","channel":"auth"}).to_string(),
            )
            .unwrap();
            push.send(
                json!({"type":"future-type","exchangeId":"binance","channel":"future-channel"})
                    .to_string(),
            )
            .unwrap();
            push.send(json!({"type":"data","exchangeId":"binance","channel":"future-channel","data":null}).to_string()).unwrap();
            for (channel, data) in [("market", market), ("tickers", ticker)] {
                let frame = json!({"type":"data","exchangeId":"binance","channel":channel,"data":[data],
                    "symbol":null,"interval":{"unused":true},"error":17,"requestId":false,"extra":{"keep":"unused"}});
                assert!(
                    serde_json::from_value::<safe_cex_types::gateway::GatewayMessage>(
                        frame.clone()
                    )
                    .is_err(),
                    "GATEWAY_ENVELOPE declaration refuses unused fields for {channel}"
                );
                push.send(frame.to_string()).unwrap();
            }
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), h.exchange.start())
            .await
            .expect("GATEWAY_ENVELOPE raw ingress must not require the strict envelope decoder")
            .unwrap();
        assert_eq!(
            h.store.markets().unwrap().len(),
            1,
            "GATEWAY_ENVELOPE market admitted"
        );
        assert_eq!(h.store.markets().unwrap()[0].symbol, "GWUSDT");
        assert_eq!(
            h.store.tickers().unwrap().len(),
            1,
            "GATEWAY_ENVELOPE ticker admitted"
        );
        assert_eq!(
            h.store.tickers().unwrap()[0].last.to_bits(),
            10.0_f64.to_bits()
        );
        assert!(
            h.seen.borrow().is_empty(),
            "GATEWAY_ENVELOPE no venue REST fallback"
        );
        let sent: Vec<serde_json::Value> = h
            .received
            .borrow()
            .iter()
            .map(|frame| serde_json::from_str(frame).unwrap())
            .collect();
        assert_eq!(
            sent[0],
            json!({"type":"subscribe","exchangeId":"binance","channel":"auth","data":{"token":"audit-token"}}),
            "GATEWAY_AUTH actual native socket sends the source auth extension"
        );
        let auth: safe_cex_types::gateway::GatewayMessage =
            serde_json::from_value(sent[0].clone()).unwrap();
        assert_eq!(
            auth.channel,
            Some(safe_cex_types::gateway::GatewayChannel::Auth)
        );
        h.exchange.dispose().unwrap();
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public-data.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            source["gatewayTypeAudit"],
            json!({"auth":true,"markets":1,"tickers":1,"last":10})
        );
    });
}

#[test]
fn binance_starts_on_a_public_data_gateway_over_websocket() {
    run_local(async {
        let options = json!({"key":"native-key","secret":SECRET,
            "externalPublicData":{"type":"websocket","url":"ws://gateway.local/gateway","exchangeId":"binance"}});
        let h = Rc::new(binance_harness_with(Rc::new(|_| None), options).await);

        // The gateway answers each channel once the provider has subscribed to it.
        let gateway = h.clone();
        tokio::task::spawn_local(async move {
            eventually("provider subscribes to tickers and markets", || {
                subscribed(&gateway, "tickers") && subscribed(&gateway, "market")
            })
            .await;
            let push = gateway_peer(&gateway).expect("gateway socket");
            let market = json!({"id":"GWUSDT","symbol":"GWUSDT","base":"GW","quote":"USDT","active":true,
                "precision":{"amount":0.1,"price":0.01},"limits":{"amount":{"min":0.1,"max":100},"leverage":{"min":1,"max":20}}});
            let ticker = json!({"id":"GWUSDT","symbol":"GWUSDT","bid":9.9,"ask":10.1,"last":10.0,"mark":10.0,"index":10.0,
                "percentage":1.0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":1,"quoteVolume":10});
            push.send(
                json!({"type":"data","exchangeId":"binance","channel":"market","data":[market]})
                    .to_string(),
            )
            .unwrap();
            push.send(
                json!({"type":"data","exchangeId":"binance","channel":"tickers","data":[ticker]})
                    .to_string(),
            )
            .unwrap();
        });

        h.exchange
            .start()
            .await
            .expect("start with externalPublicData");
        assert_eq!(
            h.store
                .markets()
                .unwrap()
                .iter()
                .map(|m| m.symbol.clone())
                .collect::<Vec<_>>(),
            ["GWUSDT"],
            "markets came from the gateway"
        );
        assert!(
            h.store
                .tickers()
                .unwrap()
                .iter()
                .any(|t| t.symbol == "GWUSDT" && t.last == 10.0),
            "tickers came from the gateway"
        );
        let rest: Vec<_> = h.seen.borrow().iter().map(|s| s.path.clone()).collect();
        assert!(
            !rest
                .iter()
                .any(|p| p == "/fapi/v1/exchangeInfo" || p == "/fapi/v1/ticker/24hr"),
            "no venue public REST: {rest:?}"
        );
        assert!(
            rest.iter().any(|p| p == "/fapi/v1/listenKey"),
            "private startup still runs over HTTP"
        );

        let books = Rc::new(RefCell::new(vec![]));
        let sink = books.clone();
        let cleanup = h
            .exchange
            .listen_order_book(
                "GWUSDT".into(),
                Rc::new(move |book| sink.borrow_mut().push(book)),
            )
            .expect("listenOrderBook through the gateway");
        eventually("provider subscribes to the order book", || {
            h.received
                .borrow()
                .iter()
                .any(|f| f.contains("\"orderbook\"") && f.contains("GWUSDT"))
        })
        .await;
        let push = gateway_peer(&h).unwrap();
        push.send(json!({"type":"data","exchangeId":"binance","channel":"orderbook","symbol":"GWUSDT",
            "data":{"bids":[{"price":9.9,"amount":2,"total":2}],"asks":[{"price":10.1,"amount":3,"total":3}]}}).to_string()).unwrap();
        eventually("gateway order book reaches the listener", || {
            !books.borrow().is_empty()
        })
        .await;
        assert_eq!(books.borrow()[0].bids[0].price, 9.9);
        cleanup().expect("unsubscribe");
        h.exchange.dispose().expect("dispose");
    });
}

fn subscribed_as(h: &Harness, exchange: &str, channel: &str) -> bool {
    h.received.borrow().iter().any(|frame| {
        let value: serde_json::Value = serde_json::from_str(frame).unwrap_or_default();
        value["type"] == "subscribe"
            && value["exchangeId"] == exchange
            && value["channel"] == channel
    })
}

/// The js_runtime-bound venues get the same gateway provider through
/// `ExternalPublicDataFactory`, called with the exchange's own base.
#[test]
fn okx_starts_on_a_public_data_gateway_over_websocket() {
    run_local(starts_on_gateway(safe_cex_types::ExchangeName::Okx, "okx"));
}

#[test]
fn kraken_starts_on_a_public_data_gateway_over_websocket() {
    run_local(starts_on_gateway(
        safe_cex_types::ExchangeName::Kraken,
        "kraken",
    ));
}

#[test]
fn hyperliquid_starts_on_a_public_data_gateway_over_websocket() {
    run_local(starts_on_gateway(
        safe_cex_types::ExchangeName::Hyperliquid,
        "hyperliquid",
    ));
}

#[test]
fn hyperliquidspot_starts_on_a_public_data_gateway_over_websocket() {
    run_local(starts_on_gateway(
        safe_cex_types::ExchangeName::Hyperliquidspot,
        "hyperliquidspot",
    ));
}

async fn starts_on_gateway(name: safe_cex_types::ExchangeName, exchange_id: &'static str) {
    let options = json!({"key":"","secret":"","publicDataOnly":true,
            "externalPublicData":{"type":"websocket","url":"ws://gateway.local/gateway","exchangeId":exchange_id}});
    let h = Rc::new(
        venue_harness(
            name,
            Rc::new(|_| None),
            options,
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await,
    );
    let gateway = h.clone();
    tokio::task::spawn_local(async move {
        eventually("provider subscribes to tickers and markets", || {
            subscribed_as(&gateway, exchange_id, "tickers")
                && subscribed_as(&gateway, exchange_id, "market")
        })
        .await;
        let push = gateway_peer(&gateway).expect("gateway socket");
        let market = json!({"id":"GW-USDT-SWAP","symbol":"GWUSDT","base":"GW","quote":"USDT","active":true,
                "precision":{"amount":1,"price":0.01},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":20}}});
        let ticker = json!({"id":"GW-USDT-SWAP","symbol":"GWUSDT","bid":9.9,"ask":10.1,"last":10.0,"mark":10.0,"index":10.0,
                "percentage":1.0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":1,"quoteVolume":10});
        for (channel, data) in [("market", market), ("tickers", ticker)] {
            push.send(
                json!({"type":"data","exchangeId":exchange_id,"channel":channel,"data":[data]})
                    .to_string(),
            )
            .unwrap();
        }
    });
    h.exchange
        .start()
        .await
        .expect("start with externalPublicData");
    assert_eq!(
        h.store
            .markets()
            .unwrap()
            .iter()
            .map(|m| m.symbol.clone())
            .collect::<Vec<_>>(),
        ["GWUSDT"],
        "markets came from the gateway"
    );
    assert!(
        h.store
            .tickers()
            .unwrap()
            .iter()
            .any(|t| t.symbol == "GWUSDT" && t.last == 10.0),
        "tickers came from the gateway"
    );
    let rest: Vec<_> = h.seen.borrow().iter().map(|s| s.path.clone()).collect();
    assert!(
        rest.is_empty(),
        "gateway startup makes no venue REST request: {rest:?}"
    );
    h.exchange.dispose().expect("dispose");
}
