//! The real NumericChanges consumer: constructed socket -> Store ticker patch.
use super::accepted_read_receivers::terminal::{harness, options};
use crate::{classes, lifecycle, stream_updates::NumericChanges};
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{ExchangeName, Ticker};
use std::{marker::PhantomData, rc::Rc};

fn ticker(symbol: &str) -> Ticker {
    Ticker {
        id: symbol.into(),
        symbol: symbol.into(),
        bid: 1.0,
        ask: 2.0,
        last: 3.0,
        mark: 4.0,
        index: Some(5.0),
        mid_price: None,
        percentage: 6.0,
        open_interest: 7.0,
        funding_rate: 8.0,
        funding_time: 9.0,
        volume: 10.0,
        quote_volume: 11.0,
        timestamp: None,
        info: None,
    }
}

#[test]
fn numeric_changes_borrowed_decoder_keeps_keys_order_duplicates_and_bits() {
    let _: PhantomData<fn(NumericChanges) -> NumericChanges> =
        PhantomData::<fn(Vec<(&'static str, f64)>) -> Vec<(&'static str, f64)>>;
    const RAW: &str = r#"[["bid",-0.0],["ask",19.25],["bid",-3.125]]"#;
    let decoded: NumericChanges = serde_json::from_str(RAW).unwrap();
    assert_eq!(decoded.len(), 3);
    for ((key, number), (expected_key, expected_number, offset)) in decoded.iter().zip([
        ("bid", -0.0_f64, 3),
        ("ask", 19.25, 16),
        ("bid", -3.125, 30),
    ]) {
        assert_eq!(*key, expected_key);
        assert_eq!(number.to_bits(), expected_number.to_bits());
        assert_eq!(key.as_ptr() as usize - RAW.as_ptr() as usize, offset);
    }
    assert!(serde_json::from_str::<NumericChanges>(r#"[["b\u0069d",1]]"#).is_err());
    assert!(
        <NumericChanges as serde::Deserialize>::deserialize(serde_json::json!([["bid", 1]]))
            .is_err()
    );
    assert!(
        <NumericChanges as serde::Deserialize>::deserialize(serde_json::json!([]))
            .unwrap()
            .is_empty()
    );
}

fn socket_patch(name: ExchangeName, raw: &str) -> Ticker {
    let futures = name == ExchangeName::Kraken;
    let symbol = if futures { "PF_XBTUSD" } else { "XBT/USD" };
    let h = harness(name, options(), |_| {});
    h.store
        .update(
            StoreUpdate {
                tickers: Some(vec![ticker(symbol)]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    let rt = &h.exchange.rt;
    let socket = if futures {
        classes::futures_public(rt, Rc::new(rt.this.clone())).unwrap()
    } else {
        classes::spot_public(rt, Rc::new(rt.this.clone())).unwrap()
    };
    assert!(socket.is_object());
    let host = rt.shared.extra.sockets.borrow().last().unwrap().clone();
    if futures {
        lifecycle::futures_public_connect(host).unwrap();
    } else {
        lifecycle::spot_public_connect(host).unwrap();
    }
    h.runtime.pump();
    h.runtime.advance(100.0);
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    let physical = h.runtime.sockets.borrow()[0].1.clone();
    physical
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(raw.into())));
    h.runtime.pump();
    let rows = h.store.tickers().unwrap();
    assert_eq!(rows.len(), 1);
    rows[0].clone()
}

#[test]
fn numeric_changes_reaches_real_futures_socket_store_with_each_key_and_bit() {
    let actual = socket_patch(
        ExchangeName::Kraken,
        r#"{"feed":"ticker","product_id":"PF_XBTUSD","bid":"-0","ask":"19.25","last":"17.5","mark":"21.75","index":"23.125","relative_funding_rate":"0.003125","next_funding_rate_time":"12345.5","open24h":"10"}"#,
    );
    for (actual, expected) in [
        (actual.bid, -0.0_f64),
        (actual.ask, 19.25),
        (actual.last, 17.5),
        (actual.mark, 21.75),
        (actual.index.unwrap(), 23.125),
        (actual.funding_rate, 0.003125),
        (actual.funding_time, 12345.5),
        (actual.percentage, 75.0),
        (actual.volume, 10.0),
        (actual.open_interest, 7.0),
    ] {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "Futures patch field bits"
        );
    }
}

#[test]
fn numeric_changes_reaches_real_spot_socket_store_with_each_key_and_bit() {
    let actual = socket_patch(
        ExchangeName::Krakenspot,
        r#"{"channel":"ticker","type":"snapshot","data":[{"symbol":"XBT/USD","bid":"13.25","ask":"19.25","last":"-0","volume":"31.5","change_pct":"-3.125"}]}"#,
    );
    for (actual, expected) in [
        (actual.bid, 13.25_f64),
        (actual.ask, 19.25),
        (actual.last, -0.0),
        (actual.mark, -0.0),
        (actual.index.unwrap(), -0.0),
        (actual.volume, 31.5),
        (actual.percentage, -3.125),
        (actual.funding_rate, 8.0),
    ] {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "Spot patch field bits"
        );
    }
}
