use super::support;
use safe_cex_exchange::socket_lifecycle::SocketSlot;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::ExchangeName;
use serde_json::json;

#[path = "../../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

pub fn run(name: ExchangeName, source_name: &str, count: usize) {
    let source = ticker_coercion::source(source_name);
    assert_eq!(source["cases"].as_array().unwrap().len(), count);
    let dual = matches!(name, ExchangeName::Bybit | ExchangeName::Bybitdemo);
    let mut options = support::options();
    options.enable_inverse_markets = Some(dual);
    let h = support::harness_as(name.clone(), options, |_| {});
    let markets = ["USDT", "USD"].into_iter().flat_map(|quote| {
        ["BTC", "ETH", "ADA"].into_iter().map(move |base| {
            let symbol = format!("{base}{quote}");
            serde_json::from_value(json!({"id":symbol,"symbol":symbol,"base":base,"quote":quote,"active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}}})).unwrap()
        })
    }).collect();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let primary = if name == ExchangeName::Bybitinverse {
        SocketSlot::PublicSecondary
    } else {
        SocketSlot::Public
    };
    let slots = if dual {
        vec![SocketSlot::Public, SocketSlot::PublicSecondary]
    } else {
        vec![primary]
    };
    for slot in slots {
        let socket = h
            .venue()
            .base()
            .get_socket(slot)
            .expect("constructed public slot");
        socket.connect_and_subscribe(false).unwrap();
        h.runtime.pump();
        assert!(socket.is_connected(), "production public socket must open");
    }
    for case in source["cases"].as_array().unwrap() {
        let inverse = name == ExchangeName::Bybitinverse || case["channel"] == "inverse";
        let seed = if inverse { "bybitinverse" } else { source_name };
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                tickers: Some(ticker_coercion::seed(seed)),
                positions: Some(vec![]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let socket = h
            .venue()
            .base()
            .get_socket(if inverse {
                SocketSlot::PublicSecondary
            } else {
                SocketSlot::Public
            })
            .unwrap();
        for frame in case["frames"].as_array().unwrap() {
            socket
                .on_message(&SocketFrame::Text(frame.as_str().unwrap().into()))
                .unwrap();
            h.runtime.pump();
        }
        ticker_coercion::assert_native(&h.store.tickers().unwrap(), case);
    }
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    h.venue().dispose().unwrap();
}
