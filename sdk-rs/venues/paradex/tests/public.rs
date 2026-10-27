//! Payload examples follow api.prod.paradex.trade/v1 and the official Python SDK.
mod support;
use safe_cex_engine::{Action, Command, Event, PublicFeedPolicy, Response, SocketChannel};
use safe_cex_venue_paradex::Options;
use support::*;
const BTC: &str = "BTC-USD-PERP";

#[test]
fn sequential_streamed_market_seed_filters_options_spot_and_inactive() {
    let h = Harness::new();
    assert_eq!(h.e.store().markets().len(), 2);
    let m = h.e.store().market(BTC).unwrap();
    assert_eq!(m.precision.amount, 0.00001);
    assert_eq!(m.precision.price, 0.1);
    assert_eq!(m.limits.amount.min, 10.);
    assert_eq!(m.limits.leverage.max, 50.);
    assert_eq!(
        h.e.store()
            .market("ETH-USD-PERP")
            .unwrap()
            .limits
            .leverage
            .max,
        20.
    );
    let t = h.e.store().ticker(BTC).unwrap();
    assert_eq!(
        (t.bid, t.ask, t.last, t.mark, t.index),
        (99., 101., 100., 100.5, 100.2)
    );
    assert_eq!(
        (t.percentage, t.volume, t.quote_volume, t.open_interest),
        (2., 200., 200., 50.)
    );
    assert!(h.pending.is_empty());
    assert!(
        h.e.store().loaded().balance
            && h.e.store().loaded().positions
            && h.e.store().loaded().orders
    );
    assert_eq!(h.sent().len(), 5);
    assert!(h.sent().iter().any(|s| s.contains("bbo.BTC-USD-PERP")));
    assert!(h.sent().iter().any(|s| s.contains("trades.ETH-USD-PERP")));
    assert!(!h.sent().iter().any(|s| s.contains("order_book")));
}
#[test]
fn testnet_keeps_real_urls_and_public_role() {
    let mut h = Harness::empty(Options {
        public_data_only: true,
        testnet: true,
        ..Options::default()
    });
    assert_eq!(
        h.pending[0].url,
        "https://api.testnet.paradex.trade/v1/markets"
    );
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    assert!(h.actions.iter().any(|a|matches!(a,Action::WsConnect{url,channel:Some(SocketChannel::Public),..}if url=="wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2")));
}
#[test]
fn bbo_does_not_replace_trade_last_and_summary_matches_runtime() {
    let mut h = Harness::new();
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"bbo.BTC-USD-PERP","data":{"bid_price":"102","ask_price":"104","index_price":"103","mark_price":"999","last_price":"999"}}}"#);
    let t = h.e.store().ticker(BTC).unwrap();
    assert_eq!(
        (t.bid, t.ask, t.index, t.last, t.mark),
        (102., 104., 103., 100., 100.5)
    );
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"trades.BTC-USD-PERP","data":[{"price":"105","size":"0.2"},{"price":"106","size":"0.3"}]}}"#);
    assert_eq!(h.e.store().ticker(BTC).unwrap().mark, 106.);
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"markets_summary","data":[{"symbol":"BTC-USD-PERP","bid":"107","last_traded_price":"108","mark_price":"109","oracle_price":"110","price_change_rate_24h":"-0.03","volume_24h":"300","open_interest":"70","funding_rate":"-0.001"}]}}"#);
    let t = h.e.store().ticker(BTC).unwrap();
    assert_eq!(
        (
            t.bid,
            t.ask,
            t.last,
            t.mark,
            t.index,
            t.percentage,
            t.volume,
            t.quote_volume
        ),
        (107., 104., 108., 109., 110., -3., 300., 200.)
    );
}
#[test]
fn snapshot_books_replace_old_levels_and_accept_array_envelopes() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    assert!(
        h.sent()
            .last()
            .unwrap()
            .contains("order_book.BTC-USD-PERP.deltas")
    );
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"order_book.BTC-USD-PERP.deltas","data":{"market":"BTC-USD-PERP","update_type":"s","seq_no":10,"updates":[],"deletes":[],"inserts":[{"side":"BUY","price":"98","size":"2"},{"side":"BUY","price":"99","size":"3"},{"side":"SELL","price":"102","size":"4"}]}}}"#);
    assert_eq!(h.e.book(BTC).unwrap().bids(), [(99., 3.), (98., 2.)]);
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"order_book.BTC-USD-PERP.deltas","data":[{"update_type":"s","seq_no":99,"inserts":[{"side":"BUY","price":"97","size":"5"},{"side":"SELL","price":"103","size":"6"}],"updates":[],"deletes":[]}]}}"#);
    let b = h.e.book(BTC).unwrap();
    assert_eq!(b.bids(), [(97., 5.)]);
    assert_eq!(b.asks(), [(103., 6.)]);
    assert_eq!(h.e.store().ticker(BTC).unwrap().bid, 97.);
}
#[test]
fn independent_book_subscribers_and_reconnect_restore_interest() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    let n = h.sent().len();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    h.command(Command::UnsubscribeOrderBook { symbol: BTC.into() });
    assert_eq!(h.sent().len(), n);
    h.e.handle(
        h.now,
        Event::WsClosed {
            socket: h.public,
            code: 1006,
        },
    );
    h.collect();
    h.reconnect();
    h.public = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: h.public });
    h.collect();
    assert!(h.sent().last().unwrap().contains("order_book.BTC-USD-PERP"));
    h.command(Command::UnsubscribeOrderBook { symbol: BTC.into() });
    assert!(h.e.book(BTC).is_none());
    assert!(h.sent().last().unwrap().contains("unsubscribe"));
}
#[test]
fn failed_or_malformed_market_lists_do_not_load_and_retry() {
    for body in [
        r#"{"error":"UNAVAILABLE","message":"try later","results":[]}"#,
        r#"{"message":"missing list"}"#,
    ] {
        let mut h = Harness::empty(Options {
            public_data_only: true,
            ..Options::default()
        });
        h.reply("/markets", body);
        assert!(!h.e.store().loaded().markets);
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { .. }))
        );
        h.retry(1000.);
        assert_eq!(h.pending.len(), 1);
    }
}
#[test]
fn streamed_chunks_are_not_committed_until_complete() {
    let mut h = Harness::empty(Options {
        public_data_only: true,
        ..Options::default()
    });
    let r = h.take("/markets");
    h.e.handle(
        h.now,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    for chunk in MARKETS.as_bytes().chunks(11) {
        h.e.handle(
            h.now,
            Event::HttpChunk {
                request: r.id,
                data: chunk,
            },
        );
    }
    assert!(!h.e.store().loaded().markets);
    h.e.handle(h.now, Event::HttpEnd { request: r.id });
    h.collect();
    assert_eq!(h.e.store().markets().len(), 2);
    assert!(h.pending[0].url.ends_with("summary?market=ALL"));
}
#[test]
fn syntactically_malformed_rows_reject_streamed_seeds_even_when_filtered() {
    // The venue's identity parse is the only syntax check on these streams.
    for row in [
        r#"{"symbol":"BTC-USD-100000-C","asset_kind":"OPTION",}"#,
        r#"{"symbol":"BTC-USD-100000-C" "asset_kind":"OPTION"}"#,
        r#"{"symbol":"BTC-USD-100000-C","asset_kind":"OPTION","fee_config":{"x":tru}}"#,
        r#"{"symbol":"BTC-USD-PERP","asset_kind":"PERP","tags":[1,]}"#,
    ] {
        let markets = format!(
            r#"{{"results":[{row},{}]}}"#,
            &MARKETS[12..MARKETS.len() - 2]
        );
        for chunk in [7, 4096] {
            let mut h = Harness::empty(Options {
                public_data_only: true,
                ..Options::default()
            });
            let r = h.take("/markets");
            h.e.handle(
                h.now,
                Event::HttpStart {
                    request: r.id,
                    status: 200,
                    headers: "",
                },
            );
            for data in markets.as_bytes().chunks(chunk) {
                h.e.handle(
                    h.now,
                    Event::HttpChunk {
                        request: r.id,
                        data,
                    },
                );
            }
            h.e.handle(h.now, Event::HttpEnd { request: r.id });
            h.collect();
            assert!(!h.e.store().loaded().markets, "{row} chunk {chunk}");
            assert!(h.e.store().markets().is_empty(), "{row} chunk {chunk}");
        }
        let mut h = Harness::empty(Options {
            public_data_only: true,
            ..Options::default()
        });
        h.reply("/markets", MARKETS);
        let summary = format!(r#"{{"results":[{row}]}}"#);
        let r = h.take("summary");
        h.e.handle(
            h.now,
            Event::HttpStart {
                request: r.id,
                status: 200,
                headers: "",
            },
        );
        h.e.handle(
            h.now,
            Event::HttpChunk {
                request: r.id,
                data: summary.as_bytes(),
            },
        );
        h.e.handle(h.now, Event::HttpEnd { request: r.id });
        h.collect();
        assert!(!h.e.store().loaded().tickers, "{row}");
    }
}
#[test]
fn documented_klines_and_fallback_preserve_time_units() {
    let mut h = Harness::new();
    h.command(Command::FetchCandles {
        command: 1,
        symbol: BTC.into(),
        interval: safe_cex_engine::model::Interval::M1,
        limit: Some(2),
        from_ms: Some(120000.),
        to_ms: Some(240000.),
    });
    let r = h.take("/markets/klines");
    assert_eq!(
        r.url,
        "https://api.prod.paradex.trade/v1/markets/klines?symbol=BTC-USD-PERP&resolution=1&start_at=120000&end_at=240000&price_kind=last"
    );
    h.respond(r, 503, r#"{"error":"unavailable"}"#);
    let r = h.take("tradingview/history");
    assert!(r.url.contains("&from=120&to=240&countback=2000"));
    h.respond(
        r,
        200,
        r#"{"s":"ok","t":[120],"o":["1"],"h":["3"],"l":["0.5"],"c":["2"],"v":["4"]}"#,
    );
    assert!(h.result(1).unwrap().contains("\"timestamp\":120000.0"));
}
#[test]
fn wide_intervals_use_legacy_route_and_malformed_parallel_arrays_fail() {
    let mut h = Harness::new();
    h.command(Command::FetchCandles {
        command: 2,
        symbol: BTC.into(),
        interval: safe_cex_engine::model::Interval::H4,
        limit: None,
        from_ms: None,
        to_ms: None,
    });
    let r = h.take("tradingview/history");
    assert!(r.url.contains("resolution=240&countback=2000"));
    h.respond(
        r,
        200,
        r#"{"s":"ok","t":[1],"o":[],"h":[],"l":[],"c":[],"v":[]}"#,
    );
    assert!(h.result(2).is_err());
}
#[test]
fn missing_kline_list_uses_legacy_fallback() {
    let mut h = Harness::new();
    h.command(Command::FetchCandles {
        command: 3,
        symbol: BTC.into(),
        interval: safe_cex_engine::model::Interval::M5,
        limit: None,
        from_ms: None,
        to_ms: None,
    });
    h.reply("klines", r#"{"message":"no list"}"#);
    assert!(h.pending.iter().any(|r| r.url.contains("tradingview")));
}
#[test]
fn polling_is_reference_counted_and_old_generation_cannot_restart_it() {
    let mut h = Harness::new();
    let subscribe = || Command::SubscribeCandles {
        symbol: BTC.into(),
        interval: safe_cex_engine::model::Interval::M1,
    };
    let unsubscribe = || Command::UnsubscribeCandles {
        symbol: BTC.into(),
        interval: safe_cex_engine::model::Interval::M1,
    };
    h.command(subscribe());
    h.command(subscribe());
    assert_eq!(h.pending.len(), 1);
    let first = h.take("klines");
    h.command(unsubscribe());
    h.respond(first, 200, r#"{"results":[[1699999980000,1,2,0.5,1.5,3]]}"#);
    let at = h.now + 3000.;
    h.timer(at);
    let old = h.take("klines");
    h.command(unsubscribe());
    h.command(subscribe());
    let current = h.take("klines");
    let before = h.actions.len();
    h.respond(old, 200, r#"{"results":[[1699999980000,1,2,0.5,1.5,3]]}"#);
    let retired = &h.actions[before..];
    assert_eq!(
        retired.len(),
        1,
        "only the retired HTTP deadline is cleared"
    );
    assert!(matches!(retired[0], Action::ClearTimer { .. }));
    h.respond(current, 200, r#"{"results":[]}"#);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..}if *at_ms==h.now+3000.))
    );
}
#[test]
fn candle_subscriptions_accept_all_declared_nonsecond_timeframes() {
    for interval in [
        "1m", "3m", "5m", "15m", "30m", "1h", "2h", "3h", "4h", "5h", "6h", "7h", "8h", "9h",
        "10h", "11h", "12h", "24h", "1d", "2d", "3d", "1w", "1M",
    ] {
        let mut h = Harness::new();
        h.command(Command::SubscribeCandles {
            symbol: BTC.into(),
            interval: safe_cex_engine::model::Interval::parse(interval).unwrap(),
        });
        assert_eq!(h.pending.len(), 1, "{interval}");
    }
}
#[test]
fn malformed_wire_numbers_cannot_poison_ticker() {
    let mut h = Harness::new();
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"trades.BTC-USD-PERP","data":{"price":"NaN","size":"1"}}}"#);
    assert_eq!(h.e.store().ticker(BTC).unwrap().last, 100.);
}
#[test]
fn public_socket_watchdog_closes_silence_even_when_late_old_socket_responds() {
    let mut h = Harness::new();
    let initial = h.now;
    let watchdog = h.timer_at(initial + 5000.);
    h.now = initial + 60_000.;
    h.e.handle(h.now, Event::TimerFired { timer: watchdog });
    h.collect();
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==h.public))
    );
    let count = h.sent().len();
    h.ws(r#"{"type":"pong"}"#);
    assert_eq!(h.sent().len(), count);
}
#[test]
fn malformed_success_http_status_does_not_commit_ticker_seed() {
    let mut h = Harness::empty(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.reply("/markets", MARKETS);
    let r = h.take("summary");
    h.e.handle(
        h.now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"results":[{"symbol":"BTC-USD-PERP","last_traded_price":"Infinity"}]}"#,
            },
        },
    );
    h.collect();
    assert!(!h.e.store().loaded().tickers);
}

fn depth(h: &mut Harness, body: &str) {
    h.ws(&format!(r#"{{"jsonrpc":"2.0","method":"subscription","params":{{"channel":"order_book.BTC-USD-PERP.deltas","data":{body}}}}}"#));
}
#[test]
fn full_depth_keeps_more_than_fifteen_and_applies_every_delta_before_flush() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    let levels = (0..25)
        .map(|i| format!(r#"{{"side":"BUY","price":"{}","size":"1"}}"#, 100 - i))
        .collect::<Vec<_>>()
        .join(",");
    depth(
        &mut h,
        &format!(
            r#"{{"update_type":"s","seq_no":100,"inserts":[{levels}],"updates":[],"deletes":[]}}"#
        ),
    );
    assert_eq!(h.e.book(BTC).unwrap().bids().len(), 25);
    depth(
        &mut h,
        r#"{"update_type":"d","seq_no":101,"inserts":[{"side":"SELL","price":"102","size":"2"}],"updates":[{"side":"BUY","price":"100","size":"3"}],"deletes":[{"side":"BUY","price":"99","size":"0"}]}"#,
    );
    depth(
        &mut h,
        r#"{"update_type":"d","seq_no":102,"inserts":[],"updates":[{"side":"SELL","price":"102","size":"4"}],"deletes":[]}"#,
    );
    let b = h.e.book(BTC).unwrap();
    assert_eq!(b.bids().len(), 24);
    assert_eq!(b.bids()[0], (100., 3.));
    assert_eq!(b.asks(), [(102., 4.)]);
}
#[test]
fn full_depth_gap_waits_for_new_snapshot_and_stale_sequences_never_rewind() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    depth(
        &mut h,
        r#"{"update_type":"d","seq_no":1,"inserts":[{"side":"BUY","price":"99","size":"8"}],"updates":[],"deletes":[]}"#,
    );
    assert!(h.e.book(BTC).unwrap().is_empty());
    depth(
        &mut h,
        r#"{"update_type":"s","seq_no":10,"inserts":[{"side":"BUY","price":"99","size":"2"}],"updates":[],"deletes":[]}"#,
    );
    depth(
        &mut h,
        r#"{"update_type":"s","seq_no":9,"inserts":[],"updates":[],"deletes":[]}"#,
    );
    assert_eq!(h.e.book(BTC).unwrap().bids(), [(99., 2.)]);
    let count = h.sent().len();
    depth(
        &mut h,
        r#"{"update_type":"d","seq_no":12,"inserts":[],"updates":[],"deletes":[]}"#,
    );
    assert!(h.e.book(BTC).unwrap().is_empty());
    assert_eq!(h.sent().len(), count + 2);
    assert!(h.sent()[count].contains("unsubscribe"));
    depth(
        &mut h,
        r#"{"update_type":"s","seq_no":9,"inserts":[{"side":"BUY","price":"98","size":"50"}],"updates":[],"deletes":[]}"#,
    );
    assert!(h.e.book(BTC).unwrap().is_empty());
    depth(
        &mut h,
        r#"{"update_type":"d","seq_no":13,"inserts":[{"side":"BUY","price":"98","size":"5"}],"updates":[],"deletes":[]}"#,
    );
    assert!(h.e.book(BTC).unwrap().is_empty());
    depth(
        &mut h,
        r#"{"update_type":"s","seq_no":20,"inserts":[{"side":"BUY","price":"97","size":"1"}],"updates":[],"deletes":[]}"#,
    );
    assert_eq!(h.e.book(BTC).unwrap().bids(), [(97., 1.)]);
}
#[test]
fn malformed_depth_does_not_clear_or_partially_apply_valid_book() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    depth(
        &mut h,
        r#"{"update_type":"s","seq_no":10,"inserts":[{"side":"BUY","price":"99","size":"2"}],"updates":[],"deletes":[]}"#,
    );
    for body in [
        r#"{}"#,
        r#"{"update_type":"s","seq_no":11,"inserts":[],"updates":[]}"#,
        r#"{"update_type":"d","seq_no":11,"inserts":[{"side":"BUY","price":"98","size":"1"},{"side":"INVALID","price":"97","size":"1"}],"updates":[],"deletes":[]}"#,
    ] {
        depth(&mut h, body);
        assert_eq!(h.e.book(BTC).unwrap().bids(), [(99., 2.)]);
    }
}
#[test]
fn malformed_or_wrong_channel_ack_does_not_suppress_subscription_retry() {
    let mut h = Harness::new();
    #[derive(serde::Deserialize)]
    struct Rpc {
        id: u64,
        params: Channel,
    }
    #[derive(serde::Deserialize)]
    struct Channel {
        channel: String,
    }
    let requests: Vec<Rpc> = h
        .sent()
        .into_iter()
        .filter(|s| s.contains("subscribe"))
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let first = requests[0].id;
    let channel = requests[0].params.channel.clone();
    for r in &requests[1..] {
        h.ws(&format!(
            r#"{{"jsonrpc":"2.0","id":{},"result":{{"channel":"{}"}}}}"#,
            r.id, r.params.channel
        ));
    }
    h.ws(&format!(r#"{{"jsonrpc":"2.0","id":{first}}}"#));
    h.ws(&format!(
        r#"{{"jsonrpc":"2.0","id":{first},"result":{{"channel":"wrong"}}}}"#
    ));
    let initial = h.now;
    let watchdog = h.timer_at(initial + 5000.);
    h.now = initial + 10000.;
    h.e.handle(h.now, Event::TimerFired { timer: watchdog });
    h.collect();
    let n = h.sent().len();
    h.timer(initial + 11000.);
    assert_eq!(h.sent().len(), n + 1);
    assert!(h.sent().last().unwrap().contains(&channel));
}
#[test]
fn late_poll_cannot_overwrite_newer_trade_or_restart_removed_subscription() {
    let mut h = Harness::new();
    h.command(Command::SubscribeCandles {
        symbol: BTC.into(),
        interval: safe_cex_engine::model::Interval::M1,
    });
    let r = h.take("klines");
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"trades.BTC-USD-PERP","data":{"price":"111","size":"2","created_at":1700000000000}}}"#);
    assert!(h.e.take_market_frame(h.now).is_none());
    let startup = h.e.take_frame(h.now).unwrap();
    let sections: Vec<_> = safe_cex_engine::wire::sections(startup).collect();
    assert!(
        sections
            .iter()
            .any(|(kind, _)| *kind == safe_cex_engine::wire::section::MARKETS)
    );
    let candle = sections
        .iter()
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::CANDLES)
        .unwrap()
        .1;
    assert_eq!(u32::from_le_bytes(candle[..4].try_into().unwrap()), 1);
    let values: [f64; 6] = std::array::from_fn(|i| {
        f64::from_le_bytes(candle[16 + i * 8..24 + i * 8].try_into().unwrap())
    });
    assert_eq!(values, [1699999980000., 111., 111., 111., 111., 2.]);
    h.respond(r, 200, r#"{"results":[[1699999980000,100,100,100,100,1]]}"#);
    assert!(h.e.take_market_frame(h.now).is_none());
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"trades.BTC-USD-PERP","data":{"price":"112","size":"1","created_at":1699999900000}}}"#);
    assert!(h.e.take_market_frame(h.now).is_none());
}
#[test]
fn market_maker_and_taker_fees_survive_normalization() {
    let mut h = Harness::empty(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.reply("/markets",r#"{"results":[{"symbol":"BTC-USD-PERP","asset_kind":"PERP","order_size_increment":"0.001","price_tick_size":"0.1","maker_fee":"-0.0001","taker_fee":"0.0002"}]}"#);
    let m = h.e.store().market(BTC).unwrap();
    assert_eq!((m.maker_fee, m.taker_fee), (Some(-0.0001), Some(0.0002)));
}

fn binary_vector(name: &str) -> Vec<u8> {
    #[derive(serde::Deserialize)]
    struct Vectors {
        frames: std::collections::HashMap<String, String>,
    }
    let v: Vectors = serde_json::from_str(include_str!("sbe-public-vectors.json")).unwrap();
    v.frames[name]
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn independent_sbe_vectors_drive_ticker_and_full_depth_without_json_translation() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    for name in ["bbo", "trade", "summary", "snapshot", "delta"] {
        let data = binary_vector(name);
        h.e.handle(
            h.now,
            Event::WsBinary {
                socket: h.public,
                data: &data,
            },
        );
        h.collect();
    }
    let t = h.e.store().ticker(BTC).unwrap();
    assert_eq!(
        (
            t.last,
            t.mark,
            t.index,
            t.volume,
            t.open_interest,
            t.percentage
        ),
        (127., 125., 126., 300., 70., -3.)
    );
    assert_eq!(t.funding_rate, 0.000123456789);
    assert_eq!((t.bid, t.ask), (99., 110.));
    let b = h.e.book(BTC).unwrap();
    assert_eq!(b.bids().len(), 24);
    assert_eq!(b.bids()[0], (99., 3.));
    assert_eq!(b.asks(), [(110., 2.)]);
}
#[test]
fn sbe_declared_blocks_groups_nullability_and_unknown_versions_are_checked_before_mutation() {
    for mut data in [
        binary_vector("bbo"),
        binary_vector("snapshot"),
        binary_vector("trade"),
        binary_vector("summary"),
    ] {
        let mut h = Harness::new();
        data.pop();
        h.e.handle(
            h.now,
            Event::WsBinary {
                socket: h.public,
                data: &data,
            },
        );
        h.collect();
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a,Action::WsClose{socket}if *socket==h.public))
        );
        assert_eq!(h.e.store().ticker(BTC).unwrap().last, 100.);
    }
    let mut h = Harness::new();
    let mut data = binary_vector("trade");
    data[6] = 3;
    h.e.handle(
        h.now,
        Event::WsBinary {
            socket: h.public,
            data: &data,
        },
    );
    h.collect();
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==h.public))
    );
}

#[test]
fn nullable_summary_fields_keep_valid_prices_on_rest_and_json_updates() {
    let mut h = Harness::empty(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.reply("/markets", MARKETS);
    h.reply("summary", r#"{"results":[{"symbol":"BTC-USD-PERP","bid":null,"ask":"","funding_rate":null,"underlying_price":null,"last_traded_price":"100","mark_price":"101","volume_24h":"20"}]}"#);
    assert!(h.e.store().loaded().tickers);
    let ticker = h.e.store().ticker(BTC).unwrap();
    assert_eq!((ticker.bid, ticker.ask, ticker.funding_rate), (0., 0., 0.));
    assert_eq!(
        (ticker.last, ticker.mark, ticker.index, ticker.volume),
        (100., 101., 101., 20.)
    );
    h.public = h
        .actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: h.public });
    h.collect();
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"markets_summary","data":{"symbol":"BTC-USD-PERP","bid":null,"ask":null,"funding_rate":null,"last_traded_price":"102","mark_price":"103","volume_24h":"25"}}}"#);
    let ticker = h.e.store().ticker(BTC).unwrap();
    assert_eq!((ticker.last, ticker.mark, ticker.volume), (102., 103., 25.));
    assert_eq!(ticker.funding_rate, 0.);
}

#[test]
fn malformed_sbe_preserves_books_and_socket_while_real_gaps_resync_only_the_affected_book() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    h.command(Command::SubscribeOrderBook {
        symbol: "ETH-USD-PERP".into(),
    });
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"order_book.ETH-USD-PERP.deltas","data":{"market":"ETH-USD-PERP","update_type":"s","seq_no":10,"inserts":[{"side":"BUY","price":"9","size":"2"},{"side":"SELL","price":"11","size":"3"}],"updates":[],"deletes":[]}}}"#);
    let binary = |h: &mut Harness, data: &[u8]| {
        h.e.handle(
            h.now,
            Event::WsBinary {
                socket: h.public,
                data,
            },
        );
        h.collect();
    };
    let mut snapshot = binary_vector("snapshot");
    binary(&mut h, &snapshot);
    let bids = h.e.book(BTC).unwrap().bids().to_vec();
    let asks = h.e.book(BTC).unwrap().asks().to_vec();
    let mut delta = binary_vector("delta");
    let sent = h.sent().len();
    binary(&mut h, &delta[..delta.len() - 1]);
    assert_eq!(h.e.book(BTC).unwrap().bids(), bids);
    assert_eq!(h.e.book(BTC).unwrap().asks(), asks);
    assert_eq!(h.e.book("ETH-USD-PERP").unwrap().bids(), [(9., 2.)]);
    assert_eq!(h.sent().len(), sent);
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );

    binary(&mut h, &delta);
    assert_eq!(h.e.book(BTC).unwrap().bids()[0], (99., 3.));
    assert_eq!(h.e.book(BTC).unwrap().asks(), [(110., 2.)]);
    delta[16..24].copy_from_slice(&103i64.to_le_bytes());
    binary(&mut h, &delta);
    assert!(h.e.book(BTC).unwrap().is_empty());
    assert_eq!(h.e.book("ETH-USD-PERP").unwrap().bids(), [(9., 2.)]);
    assert_eq!(h.e.book("ETH-USD-PERP").unwrap().asks(), [(11., 3.)]);
    assert_eq!(h.sent().len(), sent + 2);
    assert!(h.sent()[sent..].iter().all(|text| text.contains(BTC)));
    assert!(h.sent()[sent].contains("unsubscribe"));
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );

    snapshot[16..24].copy_from_slice(&104i64.to_le_bytes());
    binary(&mut h, &snapshot);
    assert_eq!(h.e.book(BTC).unwrap().bids(), bids);
    assert_eq!(h.e.book(BTC).unwrap().asks(), asks);
}

#[test]
fn irrelevant_option_summaries_with_empty_or_null_numbers_cannot_block_perpetual_startup() {
    let mut h = Harness::empty(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.reply("/markets", MARKETS);
    h.reply("summary",r#"{"results":[{"symbol":"BTC-USD-PERP","last_traded_price":"100","mark_price":"101","volume_24h":"20"},{"symbol":"BTC-USD-23OCT26-82500-P","last_traded_price":"","bid":"","ask":"","funding_rate":null},{"symbol":"UNLISTED-USD-PERP","bid":""}]}"#);
    assert!(h.e.store().loaded().tickers);
    assert_eq!(h.e.store().tickers().len(), 1);
    assert_eq!(h.e.store().ticker(BTC).unwrap().last, 100.);
    assert!(h.actions.iter().any(|a| matches!(
        a,
        Action::WsConnect {
            channel: Some(SocketChannel::Public),
            ..
        }
    )));
}

#[test]
fn preseed_subscriptions_keep_reference_counts_and_cancelled_interest_stays_cancelled() {
    let mut h = Harness::empty(Options {
        public_data_only: true,
        ..Options::default()
    });
    for symbol in [BTC, BTC, "ETH-USD-PERP", "UNKNOWN-USD-PERP"] {
        h.command(Command::SubscribeOrderBook {
            symbol: symbol.into(),
        });
    }
    h.command(Command::UnsubscribeOrderBook { symbol: BTC.into() });
    h.command(Command::UnsubscribeOrderBook {
        symbol: "ETH-USD-PERP".into(),
    });
    h.command(Command::SubscribeCandles {
        symbol: BTC.into(),
        interval: safe_cex_engine::model::Interval::M1,
    });
    h.command(Command::SubscribeCandles {
        symbol: "ETH-USD-PERP".into(),
        interval: safe_cex_engine::model::Interval::M5,
    });
    h.command(Command::UnsubscribeCandles {
        symbol: "ETH-USD-PERP".into(),
        interval: safe_cex_engine::model::Interval::M5,
    });
    assert_eq!(h.pending.len(), 1);
    h.reply("/markets", MARKETS);
    assert_eq!(h.pending.len(), 2);
    let candles = h.take("klines");
    assert!(candles.url.contains("symbol=BTC-USD-PERP"));
    h.reply("summary", TICKERS);
    h.public = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: h.public });
    h.collect();
    let books = h
        .sent()
        .into_iter()
        .filter(|text| text.contains("order_book."))
        .collect::<Vec<_>>();
    assert_eq!(books.len(), 1);
    assert!(books[0].contains(BTC));
    assert!(h.e.book("UNKNOWN-USD-PERP").is_none());
    h.command(Command::UnsubscribeOrderBook { symbol: BTC.into() });
    assert!(h.e.book(BTC).is_none());
}

#[test]
fn idle_sbe_heartbeats_preserve_liveness_without_invalid_application_pings() {
    let mut h = Harness::new();
    for (index, channel) in [
        "bbo.BTC-USD-PERP",
        "bbo.ETH-USD-PERP",
        "trades.BTC-USD-PERP",
        "trades.ETH-USD-PERP",
        "markets_summary",
    ]
    .into_iter()
    .enumerate()
    {
        h.ws(&format!(
            r#"{{"jsonrpc":"2.0","id":{},"result":{{"channel":"{channel}"}}}}"#,
            index + 1
        ));
    }
    let start = h.now;
    let mut watchdog = h.timer_at(start + 5_000.);
    let mut heartbeat = vec![16, 0, 40, 0, 1, 0, 2, 0];
    heartbeat.extend_from_slice(&[0; 16]);
    for i in 1..=4 {
        h.now = start + f64::from(i) * 20_000.;
        h.e.handle(
            h.now,
            Event::WsBinary {
                socket: h.public,
                data: &heartbeat,
            },
        );
        h.e.handle(h.now, Event::TimerFired { timer: watchdog });
        h.collect();
        watchdog = h.timer_at(h.now + 5_000.);
    }
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == h.public))
    );
    assert!(!h.sent().iter().any(|text| text.contains("ping")));
    h.now += 59_000.;
    h.e.handle(
        h.now,
        Event::WsBinary {
            socket: h.public,
            data: &[0, 1, 2],
        },
    );
    h.collect();
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == h.public))
    );
    h.now += 1_000.;
    h.e.handle(h.now, Event::TimerFired { timer: watchdog });
    h.collect();
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == h.public))
    );
}

#[test]
fn market_selection_precedes_numeric_decoding_of_irrelevant_instruments() {
    let mut h = Harness::empty(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.reply("/markets", r#"{"results":[
        {"symbol":"BTC-USD-PERP","asset_kind":"PERP","status":"ACTIVE","order_size_increment":"0.001","price_tick_size":"0.1"},
        {"symbol":"BTC-USD-250101-100000-C","asset_kind":"OPTION","order_size_increment":"invalid","maker_fee":null},
        {"market":"ETH-USD-250101-100000-P","market_kind":"OPTION","price_tick_size":"","base_currency":[]},
        {"symbol":"CALL-C","step_size":"invalid"},
        {"symbol":"PUT-P","taker_fee":null},
        {"symbol":"USDC-USD-SPOT","asset_kind":"SPOT","delta1_cross_margin_params":{"imf_base":"invalid"}},
        {"symbol":"OLD-USD-PERP","asset_kind":"PERP","status":"INACTIVE","max_slippage":null},
        {"symbol":"EXPIRING-FUTURE","asset_kind":"FUTURE","min_notional":"invalid"},
        {"market":"ETH-USD-PERP","asset_kind":"perp","step_size":"0.01","tick_size":"0.001"}
    ]}"#);
    assert!(h.e.store().loaded().markets);
    assert_eq!(h.e.store().markets().len(), 2);
    assert_eq!(h.e.store().market(BTC).unwrap().precision.amount, 0.001);
    assert_eq!(
        h.e.store().market("ETH-USD-PERP").unwrap().precision.price,
        0.001
    );
    assert_eq!(h.pending.len(), 1);
    assert!(h.pending[0].url.ends_with("summary?market=ALL"));
}

#[test]
fn malformed_selected_perpetual_numbers_skip_only_the_bad_market_row() {
    for invalid in ["\"invalid\"", "null", "\"NaN\""] {
        let mut h = Harness::empty(Options {
            public_data_only: true,
            ..Options::default()
        });
        h.reply("/markets", &format!(r#"{{"results":[
            {{"symbol":"BTC-USD-PERP","asset_kind":"PERP","step_size":"0.001","price_tick_size":"0.1","price_bands_width":"5"}},
            {{"symbol":"ETH-USD-PERP","asset_kind":"PERP","status":"ACTIVE","price_tick_size":{invalid}}}
        ]}}"#));
        assert!(h.e.store().loaded().markets);
        assert_eq!(h.e.store().markets().len(), 1);
        assert_eq!(h.e.store().markets()[0].symbol, "BTC-USD-PERP");
        assert_eq!(h.pending.len(), 1);
        assert!(h.pending[0].url.ends_with("summary?market=ALL"));
    }
}

#[test]
fn json_book_arrays_apply_snapshot_and_every_delta_in_wire_order() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    let sent = h.sent().len();
    depth(
        &mut h,
        r#"[
        {"update_type":"s","seq_no":1,"inserts":[{"side":"BUY","price":"100","size":"1"}],"updates":[],"deletes":[]},
        {"update_type":"d","seq_no":2,"inserts":[],"updates":[{"side":"BUY","price":"100","size":"2"}],"deletes":[]},
        {"update_type":"d","seq_no":3,"inserts":[{"side":"SELL","price":"101","size":"3"}],"updates":[{"side":"BUY","price":"100","size":"4"}],"deletes":[]}
    ]"#,
    );
    assert_eq!(h.e.book(BTC).unwrap().bids(), [(100., 4.)]);
    assert_eq!(h.e.book(BTC).unwrap().asks(), [(101., 3.)]);
    depth(
        &mut h,
        r#"{"update_type":"d","seq_no":4,"inserts":[],"updates":[{"side":"BUY","price":"100","size":"5"}],"deletes":[]}"#,
    );
    assert_eq!(h.e.book(BTC).unwrap().bids(), [(100., 5.)]);
    assert_eq!(h.sent().len(), sent);
}

#[test]
fn malformed_json_book_array_does_not_partially_apply_or_advance_sequence() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    depth(
        &mut h,
        r#"{"update_type":"s","seq_no":10,"inserts":[{"side":"BUY","price":"100","size":"1"}],"updates":[],"deletes":[]}"#,
    );
    depth(
        &mut h,
        r#"[
        {"update_type":"d","seq_no":11,"inserts":[],"updates":[{"side":"BUY","price":"100","size":"2"}],"deletes":[]},
        {"update_type":"d","seq_no":12,"inserts":[],"updates":[{"side":"BUY","price":"100","size":"invalid"}],"deletes":[]}
    ]"#,
    );
    assert_eq!(h.e.book(BTC).unwrap().bids(), [(100., 1.)]);
    let sent = h.sent().len();
    depth(
        &mut h,
        r#"{"update_type":"d","seq_no":11,"inserts":[],"updates":[{"side":"BUY","price":"100","size":"3"}],"deletes":[]}"#,
    );
    assert_eq!(h.e.book(BTC).unwrap().bids(), [(100., 3.)]);
    assert_eq!(h.sent().len(), sent);
}

#[test]
fn json_book_array_gap_waits_for_snapshot_before_later_deltas() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    depth(
        &mut h,
        r#"{"update_type":"s","seq_no":10,"inserts":[{"side":"BUY","price":"100","size":"1"}],"updates":[],"deletes":[]}"#,
    );
    let sent = h.sent().len();
    depth(
        &mut h,
        r#"[
        {"update_type":"d","seq_no":12,"inserts":[],"updates":[{"side":"BUY","price":"100","size":"2"}],"deletes":[]},
        {"update_type":"d","seq_no":13,"inserts":[],"updates":[{"side":"BUY","price":"100","size":"3"}],"deletes":[]}
    ]"#,
    );
    assert!(h.e.book(BTC).unwrap().is_empty());
    assert_eq!(h.sent().len(), sent + 2);
    depth(
        &mut h,
        r#"[
        {"update_type":"s","seq_no":9,"inserts":[{"side":"BUY","price":"99","size":"9"}],"updates":[],"deletes":[]},
        {"update_type":"s","seq_no":10,"inserts":[{"side":"BUY","price":"100","size":"4"}],"updates":[],"deletes":[]},
        {"update_type":"d","seq_no":11,"inserts":[],"updates":[{"side":"BUY","price":"100","size":"5"}],"deletes":[]}
    ]"#,
    );
    assert_eq!(h.e.book(BTC).unwrap().bids(), [(100., 5.)]);
    assert_eq!(h.sent().len(), sent + 2);
}

#[test]
fn polls_cannot_rewind_a_bucket_or_quantity_known_before_the_request_started() {
    fn emitted(h: &mut Harness) -> Option<[f64; 6]> {
        let frame = h.e.take_market_frame(h.now)?;
        safe_cex_engine::wire::sections(frame).find_map(|(kind, payload)| {
            if kind != safe_cex_engine::wire::section::CANDLES {
                return None;
            }
            assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 1);
            let row = &payload[payload.len() - 48..];
            Some(std::array::from_fn(|i| {
                f64::from_le_bytes(row[i * 8..i * 8 + 8].try_into().unwrap())
            }))
        })
    }
    let mut h = Harness::new();
    let bucket = (h.now / 60_000.).floor() * 60_000.;
    h.command(Command::SubscribeCandles {
        symbol: BTC.into(),
        interval: safe_cex_engine::model::Interval::M1,
    });
    h.reply("klines", r#"{"results":[]}"#);
    assert!(h.e.take_market_frame(h.now).is_none());
    let startup = h.e.take_frame(h.now).unwrap();
    assert!(
        safe_cex_engine::wire::sections(startup)
            .any(|(kind, _)| kind == safe_cex_engine::wire::section::MARKETS)
    );
    h.ws(&format!(r#"{{"jsonrpc":"2.0","method":"subscription","params":{{"channel":"trades.BTC-USD-PERP","data":{{"price":"100","size":"2","created_at":{}}}}}}}"#, h.now));
    assert_eq!(emitted(&mut h), Some([bucket, 100., 100., 100., 100., 2.]));
    for (timestamp, price, volume, expected) in [
        (bucket - 60_000., 90., 99., None),
        (bucket, 90., 1., None),
        (bucket, 101., 3., Some([bucket, 101., 101., 101., 101., 3.])),
        (
            bucket + 60_000.,
            120.,
            1.,
            Some([bucket + 60_000., 120., 120., 120., 120., 1.]),
        ),
    ] {
        h.timer(h.now + 3_000.);
        h.reply(
            "klines",
            &format!(r#"{{"results":[[{timestamp},{price},{price},{price},{price},{volume}]]}}"#),
        );
        assert_eq!(emitted(&mut h), expected);
    }
}

#[test]
fn blank_book_size_is_rejected_atomically_and_explicit_zero_remains_a_delete() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    depth(
        &mut h,
        r#"{"update_type":"s","seq_no":100,"inserts":[{"side":"BUY","price":"100","size":"2"}],"updates":[],"deletes":[]}"#,
    );
    depth(
        &mut h,
        r#"{"update_type":"d","seq_no":101,"inserts":[],"updates":[{"side":"BUY","price":"100","size":""}],"deletes":[]}"#,
    );
    assert_eq!(h.e.book(BTC).unwrap().bids(), [(100., 2.)]);
    depth(
        &mut h,
        r#"{"update_type":"s","seq_no":102,"inserts":[{"side":"BUY","price":"100","size":"2"}],"updates":[],"deletes":[]}"#,
    );
    depth(
        &mut h,
        r#"{"update_type":"d","seq_no":103,"inserts":[],"updates":[{"side":"BUY","price":"100","size":"0"}],"deletes":[]}"#,
    );
    assert!(h.e.book(BTC).unwrap().bids().is_empty());
}

#[test]
fn late_rest_summary_keeps_early_live_bbo_without_suppressing_last_and_statistics() {
    let mut h = Harness::empty(Options {
        public_data_only: true,
        ..Default::default()
    });
    h.reply("/markets", MARKETS);
    h.public = h
        .actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: h.public });
    h.collect();
    h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"bbo.BTC-USD-PERP","data":{"market":"BTC-USD-PERP","bid_price":"110","ask_price":"112"}}}"#);
    assert!(!h.e.store().loaded().tickers);
    h.reply("summary", TICKERS);
    let ticker = h.e.store().ticker(BTC).unwrap();
    assert_eq!(
        (
            ticker.bid,
            ticker.ask,
            ticker.last,
            ticker.mark,
            ticker.volume
        ),
        (110., 112., 100., 100.5, 200.)
    );
    assert!(h.e.store().loaded().tickers);
}

#[path = "support/socket.rs"]
mod socket_checks;
#[test]
fn public_supervisor_bounds_handshake_and_backs_off_flapping_connections() {
    let h = Harness::new();
    socket_checks::flaps(h.e, h.public, h.now);
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut h = Harness::empty_with_policy(
        Options {
            public_data_only: true,
            ..Options::default()
        },
        PublicFeedPolicy::Shared,
    );
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    assert_eq!(h.e.store().markets().len(), 2, "public startup completed");
    let until = h.now + 600_000.;
    let (mut timers, mut sockets, mut fired) = (Vec::new(), Vec::new(), 0);
    loop {
        for action in std::mem::take(&mut h.actions) {
            match action {
                Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => timers.retain(|(t, _)| *t != timer),
                Action::WsConnect { .. } | Action::WsClose { .. } => sockets.push(action),
                _ => {}
            }
        }
        let Some(&(timer, at)) = timers
            .iter()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        else {
            break;
        };
        timers.retain(|(t, _)| *t != timer);
        h.now = at;
        h.e.handle(at, Event::TimerFired { timer });
        h.collect();
        fired += 1;
    }
    assert!(
        sockets.is_empty(),
        "no public connects or closes: {sockets:?}"
    );
    assert_eq!(fired, 0, "no supervision timers fire");
}

#[test]
fn a_market_without_a_positive_step_is_skipped_as_malformed() {
    for (valid, bad) in [
        (r#""price_tick_size":"0.01""#, r#""price_tick_size":"0""#),
        (
            r#""order_size_increment":"0.001""#,
            r#""order_size_increment":"-0.001""#,
        ),
        (
            r#""order_size_increment":"0.001""#,
            r#""order_size_increment":"""#,
        ),
    ] {
        let markets = MARKETS.replace(valid, bad);
        assert_ne!(markets, MARKETS);
        let mut h = Harness::empty(Options {
            public_data_only: true,
            ..Options::default()
        });
        h.reply("/markets", &markets);
        assert!(h.e.store().market("BTC-USD-PERP").is_some(), "{bad}");
        assert!(h.e.store().market("ETH-USD-PERP").is_none(), "{bad}");
    }
}

#[test]
fn rest_query_success_and_failure_leave_sbe_book_sequence_and_account_unchanged() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    let snapshot = binary_vector("snapshot");
    h.e.handle(
        h.now,
        Event::WsBinary {
            socket: h.public,
            data: &snapshot,
        },
    );
    h.collect();
    let before = (
        h.e.book(BTC).unwrap().bids().to_vec(),
        h.e.book(BTC).unwrap().asks().to_vec(),
        h.e.store().tickers().to_vec(),
        h.e.store().balance().clone(),
        h.e.store().positions().to_vec(),
        h.e.store().orders().to_vec(),
    );
    for (command, status, body) in [
        (7, 200, r#"{"bids":[["1","2"]],"asks":[],"seq_no":999999}"#),
        (
            8,
            401,
            r#"{"error":"INVALID_API_KEY","message":"invalid api key"}"#,
        ),
    ] {
        h.actions.clear();
        h.e.handle(
            h.now,
            Event::Command(Command::FetchOrderBook {
                command,
                symbol: BTC.into(),
                limit: Some(5),
            }),
        );
        let mut deadline = None;
        for action in safe_cex_engine::test_support::prepared_actions(&mut h.e, h.now) {
            match action {
                Action::SetTimer { timer, at_ms } => {
                    assert_eq!(at_ms, h.now + 20_000.0);
                    assert!(
                        deadline.replace(timer).is_none(),
                        "one original read deadline"
                    );
                }
                Action::Http(request) => {
                    h.receipt_trace.push(ReceiptTrace::Http(request.id));
                    h.pending.push(request);
                }
                action => h.actions.push(action),
            }
        }
        assert!(
            deadline.is_some(),
            "managed query retains its original deadline"
        );
        assert_eq!(h.pending.len(), 1);
        let request = h.take("/orderbook/");
        assert!(request.headers.is_empty());
        h.respond(request, status, body);
        if command == 7 {
            let result: serde_json::Value = serde_json::from_str(h.result(7).unwrap()).unwrap();
            assert_eq!(result["bids"][0]["total"], 2.0);
        } else {
            assert!(h.result(8).is_err());
        }
        assert!(h.pending.is_empty());
        assert!(!h.actions.iter().any(|a| matches!(
            a,
            Action::WsConnect { .. }
                | Action::WsClose { .. }
                | Action::SetTimer { .. }
                | Action::Notice(safe_cex_engine::Notice::AccountAuth { .. })
        )));
        assert_eq!(
            (
                h.e.book(BTC).unwrap().bids().to_vec(),
                h.e.book(BTC).unwrap().asks().to_vec(),
                h.e.store().tickers().to_vec(),
                h.e.store().balance().clone(),
                h.e.store().positions().to_vec(),
                h.e.store().orders().to_vec()
            ),
            before
        );
    }
    let delta = binary_vector("delta");
    h.e.handle(
        h.now,
        Event::WsBinary {
            socket: h.public,
            data: &delta,
        },
    );
    h.collect();
    assert_eq!(h.e.book(BTC).unwrap().bids()[0], (99.0, 3.0));
    assert_eq!(h.e.book(BTC).unwrap().asks(), [(110.0, 2.0)]);
}
