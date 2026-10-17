#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_exchange::{socket_lifecycle::SocketSlot, websocket::WsEvent};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::ExchangeName;
use serde_json::json;

fn ticker_volumes(name: ExchangeName, secondary: bool, swapped: bool) {
    let mut options = support::options();
    options.enable_inverse_markets = Some(secondary);
    let h = support::harness_as(name.clone(), options, |_| {});
    let symbol = if secondary || name == ExchangeName::Bybitinverse {
        "BTCUSD"
    } else {
        "BTCUSDT"
    };
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            tickers: Some(vec![
                serde_json::from_value(json!({
                    "id":symbol,"symbol":symbol,"bid":1,"ask":2,"last":3,"mark":4,
                    "index":5,"percentage":6,"openInterest":7,"fundingRate":8,
                    "fundingTime":9,"volume":10,"quoteVolume":11
                }))
                .unwrap(),
            ]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let socket = h
        .venue()
        .base()
        .get_socket(if secondary || name == ExchangeName::Bybitinverse {
            SocketSlot::PublicSecondary
        } else {
            SocketSlot::Public
        })
        .unwrap();
    socket.connect_and_subscribe(false).unwrap();
    h.runtime.pump();
    assert!(socket.is_connected(), "production public socket opened");
    let wire = h.runtime.sockets.borrow().last().unwrap().1.clone();
    for (data, expected) in [
        (
            json!({"symbol":symbol,"volume24h":"12000","turnover24h":"0.375"}),
            if swapped {
                (0.375, 12000.)
            } else {
                (12000., 0.375)
            },
        ),
        (
            json!({"symbol":symbol,"volume24h":"0","turnover24h":0}),
            if swapped { (0.375, 0.) } else { (0., 0.375) },
        ),
        (json!({"symbol":symbol,"turnover24h":"0"}), (0., 0.)),
    ] {
        wire.events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(
                json!({"topic":format!("tickers.{symbol}"),"data":data}).to_string(),
            )));
        h.runtime.pump();
        let tickers = h.store.tickers();
        assert_eq!(
            (tickers[0].volume, tickers[0].quote_volume),
            expected,
            "production {name:?} secondary={secondary} volume units and partial update truthiness"
        );
    }
    h.venue().dispose().unwrap();
}

#[test]
fn primary_inverse_socket_uses_coin_volume_and_contract_quote_volume() {
    ticker_volumes(ExchangeName::Bybit, true, true);
}

#[test]
fn demo_inverse_socket_uses_coin_volume_and_contract_quote_volume() {
    ticker_volumes(ExchangeName::Bybitdemo, true, true);
}

#[test]
fn primary_linear_socket_keeps_source_volume_units() {
    ticker_volumes(ExchangeName::Bybit, false, false);
    ticker_volumes(ExchangeName::Bybitdemo, false, false);
}

#[test]
fn dedicated_inverse_and_spot_keep_their_source_volume_units() {
    ticker_volumes(ExchangeName::Bybitinverse, false, false);
    ticker_volumes(ExchangeName::Bybitspot, false, false);
}
