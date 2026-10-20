mod support;
use safe_cex_engine::{Command, Event, Query};
use safe_cex_venue_binance::{Options, Variant};
use support::Harness;

#[test]
fn coin_m_public_trades_use_dapi_and_documented_base_qty() {
    let mut h = Harness::started(Options {
        variant: Variant::CoinM,
        public_data_only: true,
        ..Options::default()
    });
    h.requests.clear();
    h.event(Event::Command(Command::Query {
        command: 1,
        query: Query::PublicTrades {
            symbol: "BTCUSD_PERP".into(),
            limit: Some(2000),
        },
    }));
    let request = h.take("/dapi/v1/trades");
    assert!(request.url.starts_with("https://dapi.binance.com/"));
    assert!(request.url.contains("limit=1000"));
    assert!(!request.url.contains("signature"));
    h.reply(request.id, 200, "", r#"[{"id":456,"price":"10000","qty":"5","baseQty":"0.05","time":1700000000123,"isBuyerMaker":true}]"#);
    assert_eq!(
        h.done.last().unwrap().1.as_ref().unwrap(),
        r#"[{"id":"456","symbol":"BTCUSD_PERP","price":10000.0,"qty":5.0,"quoteQty":0.05,"timestamp":1700000000123.0,"side":"sell"}]"#
    );
}

#[test]
fn inverse_private_history_keeps_existing_ts_unsupported_empty_contract() {
    let mut h = Harness::started(Options {
        variant: Variant::CoinM,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.requests.clear();
    for query in [
        Query::Fills {
            symbol: Some("BTCUSD_PERP".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::OrderHistory {
            symbol: Some("BTCUSD_PERP".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Income {
            symbol: Some("BTCUSD_PERP".into()),
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    ] {
        h.event(Event::Command(Command::Query { command: 1, query }));
        assert_eq!(h.done.last().unwrap().1.as_ref().unwrap(), "[]");
    }
    assert!(h.requests.is_empty());
}
