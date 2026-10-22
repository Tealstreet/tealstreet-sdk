#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::{Action, Command, Engine, Event};
use safe_cex_venue_okx::Okx;

#[test]
fn seeds_contract_markets_and_tickers() {
    let (engine, _, _) = public(Options::default());
    let m = &engine.store().markets()[0];
    assert_eq!(m.symbol, "BTCUSDT");
    assert_eq!(m.contract_size, Some(0.01));
    assert_eq!(m.precision.amount, 0.01);
    assert_eq!(m.limits.amount.min, 0.0001);
    assert_eq!(engine.store().tickers()[0].mark, 40000.0);
}

#[test]
fn accepts_sequence_reset_keepalive_and_recovers_gap_once() {
    let (mut e, socket, _) = public(Options::default());
    e.handle(
        4.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    actions(&mut e);
    ws(
        &mut e,
        socket,
        r#"{"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"snapshot","data":[{"seqId":100,"prevSeqId":-1,"bids":[["39999","5","0","1"]],"asks":[["40001","7","0","1"]]}]}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(39999.0, 0.05)]);
    ws(
        &mut e,
        socket,
        r#"{"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"update","data":[{"seqId":10,"prevSeqId":100,"bids":[["39999","0","0","0"]],"asks":[]}]}"#,
    );
    assert!(e.book("BTCUSDT").unwrap().bids().is_empty());
    ws(
        &mut e,
        socket,
        r#"{"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"update","data":[{"seqId":10,"prevSeqId":10,"bids":[],"asks":[]}]}"#,
    );
    assert!(actions(&mut e).is_empty());
    let gap = r#"{"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"update","data":[{"seqId":12,"prevSeqId":11,"bids":[],"asks":[]}]}"#;
    ws(&mut e, socket, gap);
    assert!(e.book("BTCUSDT").is_none());
    assert_eq!(
        actions(&mut e)
            .iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        2
    );
    ws(&mut e, socket, gap);
    assert!(actions(&mut e).is_empty());
}

#[test]
fn envelope_failure_never_marks_empty_markets_loaded() {
    for body in [
        r#"{"code":"50113","msg":"Invalid Sign","data":[]}"#,
        r#"{"data":[],"code":"50113","msg":"Invalid Sign"}"#,
        r#"{"code":"0","data":[]"#,
        r#"{"code":"0","data":[,]}"#,
        r#"{"code":"0","data":[{},]}"#,
    ] {
        let mut e = crate::support::engine(Options::default());
        e.start(0.0);
        let request = actions(&mut e).remove(0);
        reply(&mut e, &request, body, 1.0);
        assert!(!e.store().loaded().markets, "{body}");
        assert!(
            actions(&mut e)
                .iter()
                .all(|a| !matches!(a, Action::Http(_)))
        );
    }
}

#[test]
fn candles_on_business_and_all_sockets_watch_silence() {
    let (mut e, _, socket) = public(Options::default());
    e.handle(
        4.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::Mo1,
        }),
    );
    assert!(actions(&mut e).iter().any(|a| matches!(a, Action::WsSend { socket: s, text } if *s == socket && text.contains("candle1Mutc"))));
    e.handle(
        4.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            from_ms: None,
            to_ms: None,
            limit: Some(10),
        }),
    );
    let a = actions(&mut e).remove(0);
    reply(
        &mut e,
        &a,
        &envelope(r#"["1700000000000","1","3","0.5","2","9","10","20","1"]"#),
        5.0,
    );
    assert!(actions(&mut e).iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::CommandDone { result: s, .. }) if s.contains("\"volume\":20.0"))));
}

type Timers = Vec<(safe_cex_engine::TimerId, f64)>;

fn track(e: &mut Engine<Okx>, timers: &mut Timers) -> Vec<Action> {
    let out = actions(e);
    for a in &out {
        match a {
            Action::SetTimer { timer, at_ms } => timers.push((*timer, *at_ms)),
            Action::ClearTimer { timer } => timers.retain(|(t, _)| t != timer),
            _ => {}
        }
    }
    out
}

fn fire_until(e: &mut Engine<Okx>, timers: &mut Timers, until: f64) -> Vec<Action> {
    let mut out = Vec::new();
    while let Some((timer, at)) = timers
        .iter()
        .copied()
        .filter(|(_, at)| *at <= until)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        timers.retain(|(t, _)| *t != timer);
        e.handle(at, Event::TimerFired { timer });
        out.extend(track(e, timers));
    }
    out
}

#[test]
fn every_open_socket_pings_times_out_and_reconnects() {
    let mut e = crate::support::engine(Options::default());
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(MARKET), 1.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(TICKER), 2.0);
    let mut timers = Timers::new();
    let sockets: Vec<_> = track(&mut e, &mut timers)
        .iter()
        .filter_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(*socket)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(sockets.len(), 2);
    for socket in &sockets {
        e.handle(3.0, Event::WsOpened { socket: *socket });
    }
    track(&mut e, &mut timers);
    // Unanswered pings retire both sockets one 10s cadence after the last pong.
    let out = fire_until(&mut e, &mut timers, 20_002.0);
    assert!(!out.iter().any(|a| matches!(a, Action::WsClose { .. })));
    let out = fire_until(&mut e, &mut timers, 20_003.0);
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a, Action::WsClose { .. }))
            .count(),
        2
    );
    let out = fire_until(&mut e, &mut timers, 21_003.0);
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        2
    );
    // Late frames from retired sockets cannot refresh the replacement.
    for socket in sockets {
        ws(&mut e, socket, "pong");
    }
    assert!(actions(&mut e).is_empty());
}

#[test]
fn malformed_levels_force_resnapshot_and_valid_depth_ignores_stale_trade() {
    let (mut e, socket, _) = public(Options::default());
    e.handle(
        4.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    actions(&mut e);
    ws(
        &mut e,
        socket,
        r#"{"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"snapshot","data":[{"seqId":100,"bids":[["40002","5","0","1"]],"asks":[["40003","7","0","1"]]}]}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().best_bid(), Some(40002.0));
    ws(
        &mut e,
        socket,
        r#"{"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"update","data":[{"seqId":101,"prevSeqId":100,"bids":[["40002"]],"asks":[]}]}"#,
    );
    assert!(e.book("BTCUSDT").is_none());
    assert_eq!(
        actions(&mut e)
            .iter()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        2
    );
    ws(
        &mut e,
        socket,
        r#"{"arg":{"channel":"books","instId":"BTC-USDT-SWAP"},"action":"snapshot","data":[{"seqId":102,"bids":[["40001","8","0","1"]],"asks":[]}]}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(40001.0, 0.08)]);
}

#[test]
fn malformed_supported_instrument_is_not_committed() {
    let mut e = crate::support::engine(Options::default());
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(
        &mut e,
        &request,
        &envelope(r#"{"ctType":"linear","settleCcy":"USDT"}"#),
        1.0,
    );
    assert!(!e.store().loaded().markets);
}

#[test]
fn variants_use_entity_hosts_and_demo_header() {
    for (eea, testnet, host, ws_host) in [
        (false, false, "www.okx.com", "ws.okx.com"),
        (true, false, "eea.okx.com", "wseea.okx.com"),
        (false, true, "openapi.okx.com", "wspap.okx.com"),
        (true, true, "eea.okx.com", "wseeapap.okx.com"),
    ] {
        let mut e = crate::support::engine(Options {
            eea,
            testnet,
            ..Options::default()
        });
        e.start(0.0);
        let request = actions(&mut e).remove(0);
        let Action::Http(r) = &request else { panic!() };
        assert!(r.url.starts_with(&format!("https://{host}/")));
        assert_eq!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "x-simulated-trading" && v == "1"),
            testnet
        );
        reply(&mut e, &request, &envelope(MARKET), 1.0);
        if eea {
            let request = actions(&mut e).remove(0);
            assert!(is_get(&request, "instruments?instType=FUTURES"));
            reply(&mut e, &request, &envelope(""), 1.5);
        }
        let request = actions(&mut e).remove(0);
        reply(&mut e, &request, &envelope(TICKER), 2.0);
        if eea {
            let request = actions(&mut e).remove(0);
            assert!(is_get(&request, "tickers?instType=FUTURES"));
            reply(&mut e, &request, &envelope(""), 2.5);
        }
        assert_eq!(actions(&mut e).iter().filter(|a| matches!(a, Action::WsConnect { url, .. } if url.starts_with(&format!("wss://{ws_host}/")))).count(), 2);
    }
}

#[test]
fn blank_known_ticker_seeds_zero_and_startup_continues() {
    let mut e = crate::support::engine(Options::default());
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(MARKET), 1.0);
    let request = actions(&mut e).remove(0);
    reply(
        &mut e,
        &request,
        &envelope(r#"{"instId":"BTC-USDT-SWAP","bidPx":"","askPx":"","volCcy24h":"","vol24h":""}"#),
        2.0,
    );
    assert!(e.store().loaded().tickers);
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!((t.bid, t.ask, t.last, t.volume), (0.0, 0.0, 0.0, 0.0));
    assert!(
        actions(&mut e)
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}

#[test]
fn failed_ticker_read_retries_with_backoff_keeping_the_committed_catalog() {
    let mut e = crate::support::engine(Options::default());
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(MARKET), 1.0);
    let mut now = 2.0;
    for delay in [1_000.0, 2_000.0, 4_000.0] {
        let request = actions(&mut e).remove(0);
        assert!(is_get(&request, "/api/v5/market/tickers?instType=SWAP"));
        reply(
            &mut e,
            &request,
            r#"{"code":"50011","msg":"Too Many Requests","data":[]}"#,
            now,
        );
        assert!(e.store().loaded().markets && !e.store().loaded().tickers);
        let timer = retry_timer(&actions(&mut e), now, delay).expect("backoff timer");
        now += delay;
        e.handle(now, Event::TimerFired { timer });
    }
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(TICKER), now);
    assert!(e.store().loaded().tickers);
}

#[test]
fn candle_reads_use_ts_bounds_limit_and_historical_fallback() {
    let (mut e, _, _) = public(Options::default());
    e.handle(
        1_800_000_000_000.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    // The clock jumps far past startup, so an engine startup notice may precede the read.
    let request = actions(&mut e)
        .into_iter()
        .find(|action| matches!(action, Action::Http(_)))
        .unwrap();
    assert!(
        matches!(&request, Action::Http(r) if r.url.ends_with("bar=1m&limit=300&before=1799982000000&after=1800000000000"))
    );
    reply(&mut e, &request, &envelope(""), 1_800_000_000_001.0);
    let fallback = actions(&mut e).remove(0);
    assert!(
        matches!(&fallback, Action::Http(r) if r.url.contains("/history-candles?") && r.url.contains("&limit=100&"))
    );
    reply(&mut e, &fallback, &envelope(""), 1_800_000_000_002.0);
    assert!(actions(&mut e).iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::CommandDone { result: s, .. }) if s == "[]")));
}

#[test]
fn month_candle_bounds_clamp_day_in_leap_february() {
    let (mut e, _, _) = public(Options::default());
    e.handle(
        1_711_886_400_000.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::Mo1,
            limit: Some(1),
            from_ms: None,
            to_ms: Some(1_711_886_400_000.0),
        }),
    );
    let out = actions(&mut e);
    assert!(out.iter().any(|a| matches!(a, Action::Http(r) if r.url.ends_with("bar=1Mutc&limit=1&before=1709208000000&after=1711886400000"))));
}

#[test]
fn business_candle_stream_emits_a_candle_frame() {
    let (mut e, _, socket) = public(Options::default());
    e.take_frame(4.0);
    e.handle(
        5.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    ws(
        &mut e,
        socket,
        r#"{"arg":{"channel":"candle1m","instId":"BTC-USDT-SWAP"},"data":[["1700000000000","1","3","0.5","2","9","10","20","1"]]}"#,
    );
    let frame = e.take_market_frame(6.0).unwrap();
    assert_eq!(u16::from_le_bytes(frame[16..18].try_into().unwrap()), 6);
    assert_eq!(u32::from_le_bytes(frame[24..28].try_into().unwrap()), 1);
}

#[test]
fn public_topics_keep_their_order_and_cap_across_market_boundaries() {
    let mut e = crate::support::engine(Options::default());
    e.start(0.0);
    let market_rows = (0..101)
        .map(|i| MARKET.replace("BTC", &format!("COIN{i}")))
        .collect::<Vec<_>>()
        .join(",");
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(&market_rows), 1.0);
    let ticker_rows = (0..101)
        .map(|i| TICKER.replace("BTC", &format!("COIN{i}")))
        .collect::<Vec<_>>()
        .join(",");
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(&ticker_rows), 2.0);
    let socket = actions(&mut e)
        .into_iter()
        .find_map(|action| match action {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(3.0, Event::WsOpened { socket });
    let batches: Vec<serde_json::Value> = actions(&mut e)
        .into_iter()
        .filter_map(|action| match action {
            Action::WsSend { text, .. } if text != "ping" => {
                Some(serde_json::from_str(&text).unwrap())
            }
            _ => None,
        })
        .collect();
    assert_eq!(batches.len(), 6);
    let mut topics = Vec::new();
    for batch in batches {
        assert_eq!(batch["op"], "subscribe");
        let args = batch["args"].as_array().unwrap();
        assert!(!args.is_empty() && args.len() <= 100);
        topics.extend(args.iter().cloned());
    }
    let expected: Vec<_> = [
        "tickers",
        "mark-price",
        "index-tickers",
        "open-interest",
        "funding-rate",
    ]
    .into_iter()
    .flat_map(|channel| {
        (0..101).map(move |i| {
            let id = if channel == "index-tickers" {
                format!("COIN{i}-USDT")
            } else {
                format!("COIN{i}-USDT-SWAP")
            };
            serde_json::json!({"channel": channel, "instId": id})
        })
    })
    .collect();
    assert_eq!(topics, expected);
}

#[test]
fn price_channels_follow_last_duplicate_symbol_in_reordered_ticker_seed() {
    let mut e = crate::support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    let eth = MARKET
        .replace("BTC-USDT-SWAP", "ETH-USDT-SWAP")
        .replace(r#""ctValCcy":"BTC""#, r#""ctValCcy":"ETH""#);
    let alias = MARKET.replace("BTC-USDT-SWAP", "BTCUSDT-SWAP");
    let missing = MARKET.replace("BTC-USDT-SWAP", "SOL-USDT-SWAP");
    reply(
        &mut e,
        &request,
        &envelope(&[MARKET, &eth, &alias, &missing].join(",")),
        1.0,
    );
    let request = actions(&mut e).remove(0);
    let eth_ticker = TICKER.replace("BTC-USDT-SWAP", "ETH-USDT-SWAP");
    let alias_ticker = TICKER.replace("BTC-USDT-SWAP", "BTCUSDT-SWAP");
    reply(
        &mut e,
        &request,
        &envelope(&[eth_ticker.as_str(), alias_ticker.as_str(), TICKER].join(",")),
        2.0,
    );
    let socket = actions(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(3.0, Event::WsOpened { socket });
    actions(&mut e);
    for message in [
        r#"{"arg":{"channel":"tickers"},"data":[{"instId":"BTC-USDT-SWAP","bidPx":"11","askPx":"13","last":"12","open24h":"8","volCcy24h":"4","vol24h":"5"}]}"#,
        r#"{"arg":{"channel":"mark-price"},"data":[{"instId":"BTC-USDT-SWAP","markPx":"14"}]}"#,
        r#"{"arg":{"channel":"mark-price"},"data":[{"instId":"BTCUSDT-SWAP","markPx":"15"},{"instId":"ETH-USDT-SWAP","markPx":"88"},{"instId":"SOL-USDT-SWAP","markPx":"99"},{"instId":"UNKNOWN-SWAP","markPx":"1"}]}"#,
        r#"{"arg":{"channel":"index-tickers"},"data":[{"instId":"BTC-USDT","idxPx":"16"}]}"#,
        r#"{"arg":{"channel":"open-interest"},"data":[{"instId":"BTCUSDT-SWAP","oiCcy":"17"}]}"#,
        r#"{"arg":{"channel":"funding-rate"},"data":[{"instId":"BTC-USDT-SWAP","fundingRate":"0.002","fundingTime":"1800"}]}"#,
    ] {
        ws(&mut e, socket, message);
    }
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(
        (&*t.id, t.bid, t.ask, t.last, t.percentage),
        ("BTC-USDT-SWAP", 11.0, 13.0, 12.0, 50.0)
    );
    assert_eq!(
        (
            t.mark,
            t.index,
            t.open_interest,
            t.funding_rate,
            t.funding_time,
            t.volume,
            t.quote_volume
        ),
        (15.0, 16.0, 17.0, 0.002, 1800.0, 4.0, 5.0)
    );
    // Like TS, a catalogue market missing from the REST ticker seed stays absent.
    assert_eq!(e.store().tickers().len(), 3);
    assert!(e.store().ticker("SOLUSDT").is_none());
    assert!(e.store().ticker("UNKNOWN").is_none());
    assert_eq!(
        e.store().tickers()[1].mark,
        40000.0,
        "earlier duplicate ticker must remain untouched"
    );
    assert_eq!(e.store().ticker("ETHUSDT").unwrap().mark, 88.0);
}

#[test]
fn null_ticker_quote_seeds_zero_like_a_blank() {
    let mut e = crate::support::engine(Options::default());
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(MARKET), 1.0);
    let request = actions(&mut e).remove(0);
    reply(
        &mut e,
        &request,
        &envelope(
            r#"{"instId":"BTC-USDT-SWAP","last":null,"open24h":null,"bidPx":"39999.9","askPx":null,"volCcy24h":null,"vol24h":"5"}"#,
        ),
        2.0,
    );
    assert!(e.store().loaded().tickers);
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(
        (t.bid, t.ask, t.last, t.quote_volume),
        (39999.9, 0.0, 0.0, 5.0)
    );
}

#[test]
fn all_undecodable_ticker_rows_retry_startup() {
    let mut e = crate::support::engine(Options::default());
    e.start(0.0);
    let request = actions(&mut e).remove(0);
    reply(&mut e, &request, &envelope(MARKET), 1.0);
    let request = actions(&mut e).remove(0);
    reply(
        &mut e,
        &request,
        &envelope(r#"{"instId":"BTC-USDT-SWAP","last":40000}"#),
        2.0,
    );
    assert!(!e.store().loaded().tickers);
    let out = actions(&mut e);
    assert!(out.iter().all(|a| !matches!(a, Action::WsConnect { .. })));
    let timer = retry_timer(&out, 2.0, 1_000.0).expect("startup retry timer");
    e.handle(1_002.0, Event::TimerFired { timer });
    let request = actions(&mut e).remove(0);
    assert!(is_get(&request, "/api/v5/market/tickers?instType=SWAP"));
    reply(&mut e, &request, &envelope(TICKER), 1_003.0);
    assert!(e.store().loaded().tickers);
}

#[test]
fn null_live_price_fields_keep_the_last_good_values() {
    let (mut e, socket, _) = public(Options::default());
    for message in [
        r#"{"arg":{"channel":"index-tickers"},"data":[{"instId":"BTC-USDT","idxPx":"16"}]}"#,
        r#"{"arg":{"channel":"funding-rate"},"data":[{"instId":"BTC-USDT-SWAP","fundingRate":"0.002","fundingTime":"1800"}]}"#,
        r#"{"arg":{"channel":"open-interest"},"data":[{"instId":"BTC-USDT-SWAP","oiCcy":"17"}]}"#,
        r#"{"arg":{"channel":"mark-price"},"data":[{"instId":"BTC-USDT-SWAP","markPx":null}]}"#,
        r#"{"arg":{"channel":"index-tickers"},"data":[{"instId":"BTC-USDT","idxPx":null}]}"#,
        r#"{"arg":{"channel":"funding-rate"},"data":[{"instId":"BTC-USDT-SWAP","fundingRate":null,"fundingTime":null}]}"#,
        r#"{"arg":{"channel":"open-interest"},"data":[{"instId":"BTC-USDT-SWAP","oiCcy":null}]}"#,
    ] {
        ws(&mut e, socket, message);
    }
    let t = e.store().ticker("BTCUSDT").unwrap();
    assert_eq!(
        (
            t.mark,
            t.index,
            t.funding_rate,
            t.funding_time,
            t.open_interest
        ),
        (40000.0, 16.0, 0.002, 1800.0, 17.0)
    );
}
