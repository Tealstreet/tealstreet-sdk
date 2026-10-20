mod support;
use safe_cex_engine::{Command, Event, Venue};
use safe_cex_venue_bitget::{Options, Product};
use support::*;

#[test]
fn streamed_startup_and_partial_ticker_fields() {
    let mut h = Harness::public();
    let m = &h.engine.store().markets()[0];
    assert_eq!(
        (m.id.as_str(), m.precision.price, m.limits.amount.max),
        ("BTCUSDT", 0.1, 120.0)
    );
    h.message(r#"{"arg":{"channel":"ticker"},"data":[{"instId":"BTCUSDT","lastPr":"101","bidPr":"","change24h":"0.02","nextFundingTime":"123"}]}"#);
    let t = h.engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!(
        (t.bid, t.last, t.mark, t.percentage, t.funding_time),
        (99.9, 101.0, 100.0, 2.0, 123.0)
    );
}

#[test]
fn books_wait_for_snapshot_recover_gap_and_ignore_duplicates() {
    let mut h = Harness::public();
    h.book();
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"seq":10,"bids":[["99","1"]]}]}"#);
    assert!(h.engine.book("BTCUSDT").is_none());
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"snapshot","data":[{"seq":10,"bids":[["99","1"],["98","2"]],"asks":[["101","3"]]}]}"#);
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"pseq":9,"seq":11,"bids":[["99","0"],["100","4"]]}]}"#);
    assert_eq!(
        h.engine.book("BTCUSDT").unwrap().bids(),
        [(100.0, 4.0), (98.0, 2.0)]
    );
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"pseq":9,"seq":11,"bids":[["100","9"]]}]}"#);
    assert_eq!(h.engine.book("BTCUSDT").unwrap().bids()[0].1, 4.0);
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"pseq":12,"seq":13}]}"#);
    assert!(h.engine.book("BTCUSDT").is_none());
    assert!(
        h.sent
            .iter()
            .any(|(_, t)| t.contains("unsubscribe") && t.contains("books"))
    );
}

#[test]
fn checksum_failure_never_publishes_corrupt_book() {
    let mut h = Harness::public();
    h.book();
    // CRC32 of "99:1:101:3", independently calculated with zlib.
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"snapshot","data":[{"bids":[["99","1"]],"asks":[["101","3"]],"checksum":-1715721270}]}"#);
    assert!(h.engine.book("BTCUSDT").is_some(), "{:?}", h.logs);
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"bids":[["99","2"]],"checksum":1}]}"#);
    assert!(h.engine.book("BTCUSDT").is_none());
}

#[test]
fn refcounts_and_preconnect_unsubscribe_do_not_revive() {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.book();
    h.book();
    h.command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    h.command(Command::UnsubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    h.load_public();
    assert!(!h.sent.iter().any(|(_, t)| t.contains("books")));
    assert_eq!(h.engine.venue().books().count(), 0);
}

#[test]
fn candle_volume_is_quote_volume_and_rest_errors_fail_command() {
    let mut h = Harness::public();
    h.command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: "6h".into(),
    });
    assert!(h.sent.last().unwrap().1.contains("candle6Hutc"));
    h.command(Command::FetchCandles {
        command: 1,
        symbol: "BTCUSDT".into(),
        interval: "1m".into(),
        limit: Some(2),
        from_ms: Some(1000.0),
        to_ms: Some(1_000_000.0),
    });
    let req = h.take("/candles");
    assert!(req.url.contains("endTime=121000"));
    h.reply(
        req.id,
        200,
        r#"{"code":"00000","data":[["1000","1","3","1","2","10","20"]]}"#,
    );
    assert!(h.done[0].1.as_ref().unwrap().contains("\"volume\":20.0"));
    h.command(Command::FetchCandles {
        command: 2,
        symbol: "BTCUSDT".into(),
        interval: "1m".into(),
        limit: None,
        from_ms: None,
        to_ms: None,
    });
    h.respond(
        "/candles",
        200,
        r#"{"code":"429","msg":"limited","data":null}"#,
    );
    assert!(h.done[1].1.is_err());
}

#[test]
fn watchdog_covers_every_socket_and_old_socket_frames_are_ignored() {
    let mut h = Harness::public();
    let old = h.sockets.clone();
    h.now += 75000.0;
    for (timer, _) in h.timers.clone() {
        h.event(Event::TimerFired { timer });
    }
    assert_eq!(h.closed.len(), 3);
    for (timer, _) in h.timers.clone() {
        h.event(Event::TimerFired { timer });
    }
    assert_eq!(h.sockets.len(), 6);
    h.event(Event::WsMessage {
        socket: old[0].0,
        text: r#"{"arg":{"channel":"ticker"},"data":[{"instId":"BTCUSDT","lastPr":"999"}]}"#,
    });
    assert_eq!(h.engine.store().tickers()[0].last, 100.0);
}

#[test]
fn usdc_testnet_product_stays_consistent() {
    let mut h = Harness::started(Options {
        public_data_only: true,
        product: Product::Usdc,
        testnet: true,
        ..Options::default()
    });
    let r = h.take("/contracts");
    assert!(r.url.ends_with("productType=susdc-futures"));
    h.reply(r.id, 200, &CONTRACTS.replace("USDT", "SUSDC"));
    h.respond("/tickers", 200, &TICKERS.replace("USDT", "SUSDC"));
    let socket = h.sockets[0].0;
    h.event(Event::WsOpened { socket });
    assert!(h.sent[0].1.contains("SUSDC-FUTURES"));
}

#[test]
fn malformed_contract_does_not_mark_markets_loaded() {
    let mut h = Harness::started(Options::default());
    h.respond("/contracts", 200, r#"{"code":"00000","data":[{}]}"#);
    assert!(!h.engine.store().loaded().markets);
    assert!(h.sockets.is_empty());
}

#[test]
fn string_sequences_are_exact_above_double_precision_and_missing_metadata_fails_closed() {
    let mut h = Harness::public();
    h.book();
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"snapshot","data":[{"seq":"9007199254740993","bids":[["99","1"]],"asks":[]}]}"#);
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"pseq":"9007199254740993","seq":"9007199254740994","bids":[["99","2"]]}]}"#);
    assert_eq!(h.engine.book("BTCUSDT").unwrap().bids(), [(99.0, 2.0)]);
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"snapshot","data":[{}]}"#);
    assert!(h.engine.book("BTCUSDT").is_none());
}

#[test]
fn malformed_required_candle_number_fails_instead_of_emitting_zero() {
    let mut h = Harness::public();
    h.command(Command::FetchCandles {
        command: 1,
        symbol: "BTCUSDT".into(),
        interval: "1m".into(),
        limit: None,
        from_ms: None,
        to_ms: None,
    });
    h.respond(
        "/candles",
        200,
        r#"{"code":"00000","data":[["1000","bad","3","1","2","10","20"]]}"#,
    );
    assert!(h.done[0].1.is_err());
}

#[test]
fn connecting_socket_watchdog_waits_for_open_and_incomplete_snapshot_is_rejected() {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.respond("/contracts", 200, CONTRACTS);
    h.now += 5000.0;
    for (timer, _) in h.timers.clone() {
        h.event(Event::TimerFired { timer });
    }
    assert!(h.sent.is_empty());
    let mut h = Harness::public();
    h.book();
    h.message(
        r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"snapshot","data":[{"seq":1}]}"#,
    );
    assert!(h.engine.book("BTCUSDT").is_none());
}

#[test]
fn tick_sizes_decode_decimal_contract_steps_without_power_drift() {
    for (place, step, expected) in [(5, 1, 1e-5), (7, 1, 1e-7), (9, 5, 5e-9)] {
        let mut h = Harness::started(Options {
            public_data_only: true,
            ..Options::default()
        });
        h.respond(
            "/contracts",
            200,
            &CONTRACTS
                .replace(
                    "\"pricePlace\":\"1\"",
                    &format!("\"pricePlace\":\"{place}\""),
                )
                .replace(
                    "\"priceEndStep\":\"1\"",
                    &format!("\"priceEndStep\":\"{step}\""),
                ),
        );
        assert_eq!(
            h.engine.store().market("BTCUSDT").unwrap().precision.price,
            expected
        );
    }
}
#[test]
fn public_envelope_accepts_data_before_arg_and_preserves_partial_ticker_fields() {
    let mut h = Harness::public();
    h.message(r#"{"data":[{"instId":"BTCUSDT","lastPr":"101"}],"action":"snapshot","arg":{"instType":"USDT-FUTURES","channel":"ticker","instId":"BTCUSDT"}}"#);
    let ticker = h.engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!(ticker.last, 101.0);
    assert_eq!(ticker.bid, 99.9);
}

#[test]
fn malformed_book_data_shape_invalidates_a_previously_synced_book() {
    for data in ["{}", "\"bad\"", "[true]"] {
        let mut h = Harness::public();
        h.book();
        h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"snapshot","data":[{"seq":10,"bids":[["99","1"]],"asks":[["101","1"]]}]}"#);
        assert!(h.engine.book("BTCUSDT").is_some());
        h.message(&format!(
            r#"{{"arg":{{"channel":"books","instId":"BTCUSDT"}},"action":"update","data":{data}}}"#
        ));
        assert!(h.engine.book("BTCUSDT").is_none());
    }
}

#[test]
fn ticker_row_storage_preserves_empty_and_multiple_rows_and_escaped_envelopes() {
    let mut h = Harness::public();
    h.message(r#"{"arg":{"channel":"ticker"},"action":"snap\u0073hot","data":[]}"#);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 100.0);
    h.message(r#"{"arg":{"channel":"ticker"},"data":[{"instId":"BTCUSDT","lastPr":"101"},{"instId":"BTCUSDT","lastPr":"102"}]}"#);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 102.0);
}

#[test]
fn shared_public_socket_urls_keep_independent_feed_identities() {
    let h = Harness::public();
    let feeds: std::collections::HashSet<_> = h
        .socket_contexts
        .iter()
        .filter(|(_, url, _)| url == "wss://ws.bitget.com/v2/ws/public")
        .map(|(_, _, context)| context.as_ref().expect("feed context").feed)
        .collect();
    assert_eq!(
        feeds,
        std::collections::HashSet::from(["primary", "secondary-1", "secondary-2"])
    );
    assert!(
        h.socket_contexts
            .iter()
            .all(|(_, _, context)| !context.as_ref().unwrap().recover)
    );
}
