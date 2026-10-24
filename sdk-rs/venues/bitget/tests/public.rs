use crate::support::FixtureOptions as Options;
mod support;
use safe_cex_engine::{Command, Event, Venue};
use safe_cex_venue_bitget::Product;
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
        interval: safe_cex_engine::model::Interval::H6,
    });
    assert!(h.sent.last().unwrap().1.contains("candle6Hutc"));
    h.command(Command::FetchCandles {
        command: 1,
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::M1,
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
        interval: safe_cex_engine::model::Interval::M1,
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
        interval: safe_cex_engine::model::Interval::M1,
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
fn ticker_indexes_follow_reordered_market_and_ticker_seeds_and_last_duplicates() {
    use safe_cex_engine::{Action, Cx, Engine, Response, SocketId, TimerId};
    use safe_cex_venue_bitget::{BitgetFutures, Pending};

    // Drive additional REST seed cycles through the production response handlers.
    struct Reloadable(BitgetFutures);
    impl Venue for Reloadable {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            self.0.start(cx);
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, _: Command) {
            self.0.start(cx);
        }
        fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
            self.0.on_open(cx, socket);
        }
        fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
            self.0.on_message(cx, socket, text);
        }
        fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
            self.0.on_close(cx, socket, code);
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, r: Response<'_>) {
            self.0.on_response(cx, pending, r);
        }
        fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
            self.0.on_item(cx, pending, item);
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
            self.0.on_request_failed(cx, pending, error);
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
            self.0.on_timer(cx, timer);
        }
        fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
            self.0.book(symbol)
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
            self.0.books()
        }
    }
    fn reply(engine: &mut Engine<Reloadable>, actions: Vec<Action>, path: &str, body: &str) {
        let request = actions
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.contains(path) => Some(r.id),
                _ => None,
            })
            .unwrap();
        engine.handle(
            1_000_000.0,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
    fn message(engine: &mut Engine<Reloadable>, socket: SocketId, text: &str) {
        engine.handle(1_000_000.0, Event::WsMessage { socket, text });
    }
    let btc: serde_json::Value = serde_json::from_str(CONTRACTS).unwrap();
    let btc = &btc["data"][0];
    let eth =
        serde_json::from_str::<serde_json::Value>(&btc.to_string().replace("BTC", "ETH")).unwrap();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Reloadable(BitgetFutures::new(
        safe_cex_venue_bitget::Options {
            public_data_only: true,
            ..Default::default()
        },
    )));
    engine.start(1_000_000.0);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    reply(
        &mut engine,
        actions,
        "/contracts",
        &serde_json::json!({"code":"00000","data":[btc,eth]}).to_string(),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    reply(
        &mut engine,
        actions,
        "/tickers",
        r#"{"code":"00000","data":[{"symbol":"ETHUSDT","lastPr":"200"},{"symbol":"BTCUSDT","lastPr":"100"},{"symbol":"BTCUSDT","lastPr":"110"}]}"#,
    );
    engine.drain_actions();
    message(
        &mut engine,
        socket,
        r#"{"arg":{"channel":"ticker"},"data":[{"instId":"BTCUSDT","lastPr":"111"},{"symbol":"ETHUSDT","lastPr":"201"},{"instId":"UNKNOWN","lastPr":"999"}]}"#,
    );
    assert_eq!(
        engine
            .store()
            .tickers()
            .iter()
            .map(|t| t.last)
            .collect::<Vec<_>>(),
        [201.0, 100.0, 111.0]
    );

    engine.handle(
        1_000_000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "reload".into(),
        }),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    reply(
        &mut engine,
        actions,
        "/contracts",
        &serde_json::json!({"code":"00000","data":[eth,btc]}).to_string(),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    // Markets have changed order while the previous ticker table is still live.
    message(
        &mut engine,
        socket,
        r#"{"arg":{"channel":"ticker"},"data":[{"instId":"BTCUSDT","lastPr":"112"}]}"#,
    );
    assert_eq!(engine.store().tickers()[2].last, 112.0);
    assert_eq!(engine.store().tickers()[0].last, 201.0);
    reply(
        &mut engine,
        actions,
        "/tickers",
        r#"{"code":"00000","data":[{"symbol":"BTCUSDT","lastPr":"300"}]}"#,
    );
    engine.drain_actions();
    assert_eq!(
        engine.store().ticker("BTCUSDT").unwrap().last,
        112.0,
        "late REST must preserve the intervening native quote"
    );
    message(
        &mut engine,
        socket,
        r#"{"arg":{"channel":"ticker"},"data":[{"instId":"ETHUSDT","lastPr":"999"},{"instId":"UNKNOWN","lastPr":"999"},{"symbol":"BTCUSDT","lastPr":"301"}]}"#,
    );
    assert_eq!(engine.store().tickers().len(), 2);
    assert_eq!(engine.store().ticker("BTCUSDT").unwrap().last, 301.0);
    assert_eq!(engine.store().ticker("ETHUSDT").unwrap().last, 999.0);
    assert!(engine.store().ticker("UNKNOWN").is_none());
}

#[test]
fn inverse_public_contracts_keep_native_precision_and_coin_product() {
    // https://www.bitget.com/docs/catalog/classic-contract-market/classic-contract-market
    for (testnet, product) in [(false, "coin-futures"), (true, "scoin-futures")] {
        let mut h = Harness::started(Options {
            public_data_only: true,
            product: Product::Inverse,
            testnet,
            ..Options::default()
        });
        assert!(
            h.requests[0]
                .url
                .ends_with(&format!("productType={product}"))
        );
        h.respond("/contracts", 200, r#"{"code":"00000","data":[{"symbol":"BTCUSD","baseCoin":"BTC","quoteCoin":"USD","symbolStatus":"normal","pricePlace":"1","priceEndStep":"5","sizeMultiplier":"1","minTradeNum":"1","maxTradeNum":"2000","maxOrderQty":"5000","maxLever":"125"},{"symbol":"ETHUSD","baseCoin":"ETH","quoteCoin":"USD","symbolStatus":"listed","pricePlace":"2","priceEndStep":"1","sizeMultiplier":"1","minTradeNum":"1"},{"symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001"}]}"#);
        h.respond("/tickers", 200, r#"{"code":"00000","data":[{"symbol":"BTCUSD","lastPr":"50000","bidPr":"49999","askPr":"50001"},{"symbol":"ETHUSD","lastPr":"2500"}]}"#);
        let markets = h.engine.store().markets();
        assert_eq!(markets.len(), 2);
        assert_eq!(markets[0].is_inverse, Some(true));
        assert_eq!(markets[0].precision.amount, 1.0);
        assert_eq!(markets[0].precision.price, 0.5);
        assert_eq!(markets[0].limits.amount.max, 2000.0);
        assert_eq!(markets[1].limits.leverage.max, 20.0);
        for (socket, _) in h.sockets.clone() {
            h.event(Event::WsOpened { socket });
        }
        assert!(
            h.sent
                .iter()
                .any(|(_, text)| text.contains(&product.to_uppercase()))
        );
        h.command(Command::SubscribeOrderBook {
            symbol: "BTCUSD".into(),
        });
        h.message(r#"{"arg":{"channel":"books","instId":"BTCUSD"},"action":"snapshot","data":[{"seq":1,"bids":[["49999","1000"]],"asks":[["50001","2000"]]}]}"#);
        assert_eq!(
            h.engine.book("BTCUSD").unwrap().bids()[0],
            (49999.0, 1000.0)
        );
    }
}

#[test]
fn shared_public_socket_urls_keep_independent_feed_identities() {
    let h = Harness::public();
    let feeds: std::collections::HashSet<_> = h
        .socket_contexts
        .iter()
        .filter(|(_, url, _)| url == "wss://ws.bitget.com/v2/ws/public")
        .map(|(_, _, context)| context.as_ref().expect("feed context").feed.as_str())
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
