use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Response, SocketId, Venue};
use safe_cex_venue_orderly::{Broker, Options, Orderly};

type Driver = Engine<Orderly>;
fn actions(e: &mut Driver) -> Vec<Action> {
    e.drain_actions().collect()
}
fn requests(e: &mut Driver) -> Vec<HttpRequest> {
    actions(e)
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
fn reply(e: &mut Driver, r: &HttpRequest, body: &str) {
    e.handle(
        1000.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn ws(e: &mut Driver, socket: SocketId, body: &str) {
    e.handle(2000.0, Event::WsMessage { socket, text: body });
}
fn ready() -> (Driver, SocketId) {
    ready_with(|_| {})
}
fn ready_with(before_seed: impl FnOnce(&mut Driver)) -> (Driver, SocketId) {
    let mut e = Engine::new(Orderly::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
    before_seed(&mut e);
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(r.url, "https://api-evm.orderly.org/v1/public/info");
    assert!(r.headers.is_empty());
    reply(
        &mut e,
        &r,
        r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","base_tick":0.00001,"quote_tick":0.1,"base_min":0.00001,"base_max":20,"base_imr":0.01},{"symbol":"PERP_ETH_USDC","base_tick":0.001,"quote_tick":0.01,"base_min":0.001,"base_max":100,"base_imr":0.02}]}}"#,
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","24h_open":50000,"24h_close":51000,"index_price":50999,"mark_price":51001,"open_interest":100,"last_funding_rate":0.0001,"next_funding_time":3000,"24h_volume":10,"24h_amount":500000}]}}"#,
    );
    let socket = actions(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, url } = a {
                assert_eq!(
                    url,
                    "wss://ws-evm.orderly.org/ws/stream/OqdphuyCtYWxwzhxyLLjOWNdFP7sQt8RPWzmb5xY"
                );
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1000.0, Event::WsOpened { socket });
    (e, socket)
}
fn subscribe(e: &mut Driver) {
    e.handle(
        1100.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDC".into(),
        }),
    );
}
fn delta(e: &mut Driver, s: SocketId, ts: u64, bids: &str) {
    ws(
        e,
        s,
        &format!(
            r#"{{"topic":"PERP_BTC_USDC@orderbookupdate","ts":{ts},"data":{{"prevTs":{},"bids":{bids},"asks":[]}}}}"#,
            ts - 100
        ),
    );
}
fn snapshot_id(e: &mut Driver) -> String {
    actions(e)
        .into_iter()
        .find_map(|a| {
            if let Action::WsSend { text, .. } = a {
                serde_json::from_str::<Request>(&text)
                    .ok()
                    .filter(|r| r.event == "request")
                    .map(|r| r.id)
            } else {
                None
            }
        })
        .unwrap()
}
#[derive(serde::Deserialize)]
struct Request {
    event: String,
    #[serde(default)]
    id: String,
}
fn snapshot(e: &mut Driver, s: SocketId, id: &str) {
    ws(
        e,
        s,
        &format!(
            r#"{{"id":"{id}","event":"request","success":true,"data":{{"symbol":"PERP_BTC_USDC","ts":1500,"bids":[[50000,1],[49999,2]],"asks":[[50001,1]]}}}}"#
        ),
    );
}

#[test]
fn seeds_and_routes_global_topics() {
    let (mut e, s) = ready();
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(
        e.store().market("ETHUSDC").unwrap().limits.leverage.max,
        50.0
    );
    let t = e.store().tickers().first().unwrap();
    assert_eq!(t.percentage, 2.0);
    assert!(e.store().loaded().orders && e.store().loaded().balance);
    for text in [
        r#"{"topic":"tickers","data":[{"symbol":"PERP_BTC_USDC","close":51002,"volume":12,"amount":612000}]}"#,
        r#"{"topic":"bbos","data":[{"symbol":"PERP_BTC_USDC","bid":51001,"ask":51003}]}"#,
        r#"{"topic":"markprices","data":[{"symbol":"PERP_BTC_USDC","price":51002.5}]}"#,
        r#"{"topic":"estfundingrate","data":[{"symbol":"PERP_BTC_USDC","fundingRate":"0.0002","fundingTs":"9000"}]}"#,
    ] {
        ws(&mut e, s, text);
    }
    let t = e.store().tickers().first().unwrap();
    assert_eq!(
        (t.last, t.bid, t.ask, t.mark, t.funding_rate, t.funding_time),
        (51002.0, 51001.0, 51003.0, 51002.5, 0.0002, 9000.0)
    );
    ws(&mut e, s, r#"{"event":"ping"}"#);
    assert!(
        actions(&mut e)
            .iter()
            .any(|a| matches!(a,Action::WsSend{text,..}if text=="{\"event\":\"pong\"}"))
    );
}
#[test]
fn full_depth_snapshot_buffers_then_applies_newer_updates() {
    let (mut e, s) = ready();
    subscribe(&mut e);
    delta(&mut e, s, 1400, "[[50000,9]]");
    let id = snapshot_id(&mut e);
    delta(&mut e, s, 1600, "[[50000,3],[49998,4]]");
    snapshot(&mut e, s, &id);
    assert_eq!(
        e.venue().book("BTCUSDC").unwrap().bids(),
        [(50000.0, 3.0), (49999.0, 2.0), (49998.0, 4.0)]
    );
    delta(&mut e, s, 1700, "[[49999,0]]");
    delta(&mut e, s, 1600, "[[50000,99]]");
    assert_eq!(
        e.venue().book("BTCUSDC").unwrap().bids(),
        [(50000.0, 3.0), (49998.0, 4.0)]
    );
}
#[test]
fn subscriptions_refcount_and_reject_old_snapshot_after_resubscribe() {
    let (mut e, s) = ready();
    subscribe(&mut e);
    delta(&mut e, s, 1400, "[]");
    let old = snapshot_id(&mut e);
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDC".into(),
        }),
    );
    e.handle(
        2000.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDC".into(),
        }),
    );
    assert!(
        !actions(&mut e)
            .iter()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
    e.handle(
        2000.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDC".into(),
        }),
    );
    actions(&mut e);
    subscribe(&mut e);
    delta(&mut e, s, 1600, "[]");
    let fresh = snapshot_id(&mut e);
    snapshot(&mut e, s, &old);
    assert!(e.venue().book("BTCUSDC").is_none());
    snapshot(&mut e, s, &fresh);
    assert!(e.venue().book("BTCUSDC").is_some());
}
#[test]
fn failed_snapshot_retries_without_another_delta_and_clears_on_reconnect() {
    let (mut e, s) = ready();
    subscribe(&mut e);
    delta(&mut e, s, 1400, "[]");
    let id = snapshot_id(&mut e);
    ws(
        &mut e,
        s,
        &format!(r#"{{"event":"request","id":"{id}","success":false,"message":"try again"}}"#),
    );
    let timer = actions(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(3000.0, Event::TimerFired { timer });
    let fresh = snapshot_id(&mut e);
    snapshot(&mut e, s, &fresh);
    e.handle(
        3000.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    assert!(e.venue().book("BTCUSDC").is_none());
    snapshot(&mut e, s, &fresh);
    assert!(e.venue().book("BTCUSDC").is_none());
}
#[test]
fn malformed_seed_and_candles_do_not_commit_partial_results() {
    let mut e = Engine::new(Orderly::new(Options::default()));
    e.start(1000.0);
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BAD_USDC","base_tick":"NaN","quote_tick":1,"base_min":1,"base_max":2}]}}"#,
    );
    assert!(e.store().markets().is_empty());
    let (mut e, s) = ready();
    e.handle(
        1000.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDC".into(),
            interval: "1m".into(),
        }),
    );
    actions(&mut e);
    ws(
        &mut e,
        s,
        r#"{"topic":"PERP_BTC_USDC@kline_1m","data":{"startTime":60000,"open":50000,"high":51000,"low":49000,"close":50500,"volume":2}}"#,
    );
    assert!(e.take_frame(2000.0).is_some());
}
#[test]
fn broker_tags_are_explicit_and_shared_where_expected() {
    for (b, tag) in [
        (Broker::Tealstreet, "TEALSTREET"),
        (Broker::KodiakFi, "TEALKODIAK"),
        (Broker::Mode, "TEALMODE"),
        (Broker::SalsaDex, "TEALSALSA"),
        (Broker::What, "TEALWHAT"),
        (Broker::WooFi, "TEALSTREET"),
    ] {
        assert_eq!(b.order_tag(), tag);
    }
}

#[test]
fn startup_subscriptions_are_retained_and_can_be_removed_before_seed() {
    let (mut e, s) = ready_with(|e| {
        for symbol in ["BTCUSDC", "ETHUSDC"] {
            e.handle(
                1000.0,
                Event::Command(Command::SubscribeOrderBook {
                    symbol: symbol.into(),
                }),
            );
            e.handle(
                1000.0,
                Event::Command(Command::SubscribeCandles {
                    symbol: symbol.into(),
                    interval: "1m".into(),
                }),
            );
        }
        e.handle(
            1000.0,
            Event::Command(Command::UnsubscribeOrderBook {
                symbol: "ETHUSDC".into(),
            }),
        );
        e.handle(
            1000.0,
            Event::Command(Command::UnsubscribeCandles {
                symbol: "ETHUSDC".into(),
                interval: "1m".into(),
            }),
        );
    });
    let out = actions(&mut e);
    let messages: Vec<_> = out
        .iter()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("PERP_BTC_USDC@orderbookupdate"))
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("PERP_BTC_USDC@kline_1m"))
    );
    assert!(!messages.iter().any(|m| {
        m.contains("PERP_ETH_USDC@orderbookupdate") || m.contains("PERP_ETH_USDC@kline_")
    }));
    let id = messages
        .iter()
        .find_map(|text| {
            serde_json::from_str::<Request>(text)
                .ok()
                .filter(|r| r.event == "request")
                .map(|r| r.id)
        })
        .unwrap();
    snapshot(&mut e, s, &id);
    assert!(e.book("BTCUSDC").is_some());
}

#[test]
fn malformed_live_depth_invalidates_and_wrong_symbol_snapshot_is_rejected() {
    let (mut e, s) = ready();
    subscribe(&mut e);
    let id = snapshot_id(&mut e);
    snapshot(&mut e, s, &id);
    actions(&mut e);
    ws(
        &mut e,
        s,
        r#"{"topic":"PERP_BTC_USDC@orderbookupdate","ts":1600,"data":{"bids":[[50000,9]]}}"#,
    );
    assert!(e.book("BTCUSDC").is_none());
    let timer = actions(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    delta(&mut e, s, 1700, "[[50000,2]]");
    assert!(e.book("BTCUSDC").is_none());
    e.handle(3000.0, Event::TimerFired { timer });
    let id = snapshot_id(&mut e);
    ws(
        &mut e,
        s,
        &format!(
            r#"{{"event":"request","id":"{id}","success":true,"data":{{"symbol":"PERP_ETH_USDC","ts":1600,"bids":[[50000,1]],"asks":[]}}}}"#
        ),
    );
    assert!(e.book("BTCUSDC").is_none());
}

#[test]
fn watchdog_closes_silent_socket_and_reconnects_with_quiet_snapshot_request() {
    let (mut e, s) = ready();
    let timer = actions(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(21000.0, Event::TimerFired { timer });
    let out = actions(&mut e);
    assert!(
        out.iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==s))
    );
    let timer = out
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    subscribe(&mut e);
    actions(&mut e);
    e.handle(22000.0, Event::TimerFired { timer });
    let socket = actions(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(22000.0, Event::WsOpened { socket });
    let id = snapshot_id(&mut e);
    snapshot(&mut e, socket, &id);
    assert!(e.book("BTCUSDC").is_some());
}

#[test]
fn candle_reads_reject_missing_timestamp_invalid_ohlc_and_failed_envelope() {
    let (mut e, _) = ready();
    actions(&mut e);
    for body in [
        r#"{"success":false,"data":{"rows":[]},"message":"rate limit"}"#,
        r#"{"success":true,"data":{"rows":[{"open":1,"high":2,"low":1,"close":2,"volume":0}]}}"#,
        r#"{"success":true,"data":{"rows":[{"start_timestamp":1000,"open":3,"high":2,"low":1,"close":2,"volume":0}]}}"#,
        r#"{"success":true,"data":{}}"#,
    ] {
        e.handle(
            1000.0,
            Event::Command(Command::FetchCandles {
                command: 42,
                symbol: "BTCUSDC".into(),
                interval: "1m".into(),
                limit: Some(5),
                from_ms: None,
                to_ms: None,
            }),
        );
        let r = requests(&mut e).pop().unwrap();
        reply(&mut e, &r, body);
        assert!(actions(&mut e).iter().any(|a| matches!(
            a,
            Action::Notice(safe_cex_engine::Notice::CommandDone {
                command: 42,
                result: Err(_)
            })
        )));
    }
}

#[test]
fn predecessor_gap_requires_a_fresh_snapshot() {
    let (mut e, s) = ready();
    subscribe(&mut e);
    let id = snapshot_id(&mut e);
    snapshot(&mut e, s, &id);
    delta(&mut e, s, 1600, "[[50000,2]]");
    assert!(e.book("BTCUSDC").is_some());
    delta(&mut e, s, 1800, "[[50000,8]]");
    assert!(e.book("BTCUSDC").is_none());
}

fn ready_sockets(markets: &str, tickers: &str) -> (Driver, Vec<SocketId>) {
    let mut e = Engine::new(Orderly::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(1000.0);
    for body in [markets, tickers] {
        let r = requests(&mut e).pop().unwrap();
        reply(&mut e, &r, body);
    }
    let sockets: Vec<_> = actions(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .collect();
    for socket in &sockets {
        e.handle(1000.0, Event::WsOpened { socket: *socket });
    }
    (e, sockets)
}
fn custom_ready(markets: &str, tickers: &str) -> (Driver, SocketId) {
    let (e, sockets) = ready_sockets(markets, tickers);
    (e, sockets[0])
}
fn market_row(native: &str) -> String {
    format!(
        r#"{{"symbol":"{native}","base_tick":0.01,"quote_tick":0.001,"base_min":0.01,"base_max":100,"base_imr":0.02}}"#
    )
}
fn ticker_row(native: &str) -> String {
    format!(r#"{{"symbol":"{native}","24h_open":1,"24h_close":1.1,"mark_price":1.11}}"#)
}
fn rows_envelope(rows: &[String]) -> String {
    format!(
        r#"{{"success":true,"data":{{"rows":[{}]}}}}"#,
        rows.join(",")
    )
}

#[test]
fn broker_listed_markets_keep_usdc_quote_unique_identity_and_native_requests() {
    let natives = [
        "PERP_XLM_USDC",
        "PERP_XLM_USDC_mythos",
        "PERP_XLM_USDC_other",
    ];
    let (mut e, socket) = custom_ready(
        &rows_envelope(&natives.map(market_row)),
        &rows_envelope(&natives.map(ticker_row)),
    );
    assert_eq!(e.store().markets().len(), 3);
    for (symbol, id) in [
        ("XLMUSDC", "XLM/USDC:USDC"),
        ("XLMUSDC_mythos", "XLM/USDC:USDC_mythos"),
        ("XLMUSDC_other", "XLM/USDC:USDC_other"),
    ] {
        let market = e.store().market(symbol).unwrap();
        assert_eq!(market.base, "XLM");
        assert_eq!(market.quote, "USDC");
        assert_eq!(market.id, id);
    }
    assert_eq!(e.store().tickers().len(), 3);
    actions(&mut e);
    e.handle(
        2000.0,
        Event::Command(Command::FetchCandles {
            command: 90,
            symbol: "XLMUSDC_mythos".into(),
            interval: "1m".into(),
            limit: Some(10),
            from_ms: None,
            to_ms: None,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.contains("symbol=PERP_XLM_USDC_mythos&type=1m"));
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "XLMUSDC_mythos".into(),
        }),
    );
    let out = actions(&mut e);
    assert!(out.iter().any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("PERP_XLM_USDC_mythos@orderbookupdate"))));
    assert!(out.iter().any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("\"symbol\":\"PERP_XLM_USDC_mythos\""))));
    ws(
        &mut e,
        socket,
        r#"{"topic":"tickers","data":[{"symbol":"PERP_XLM_USDC_mythos","close":1.2,"volume":3,"amount":3.6}]}"#,
    );
    assert_eq!(e.store().ticker("XLMUSDC_mythos").unwrap().last, 1.2);
    assert_eq!(e.store().ticker("XLMUSDC").unwrap().last, 1.1);
}

#[test]
fn funding_subscribes_per_market_and_parses_documented_single_object() {
    let natives = ["PERP_XLM_USDC", "PERP_XLM_USDC_mythos"];
    let (mut e, sockets) = ready_sockets(
        &rows_envelope(&natives.map(market_row)),
        &rows_envelope(&natives.map(ticker_row)),
    );
    let socket = sockets[1];
    #[derive(serde::Deserialize)]
    struct Subscription {
        event: String,
        #[serde(default)]
        topic: String,
    }
    let subscriptions: Vec<_> = actions(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => serde_json::from_str::<Subscription>(&text)
                .ok()
                .filter(|s| s.event == "subscribe")
                .map(|s| s.topic),
            _ => None,
        })
        .collect();
    assert_eq!(subscriptions.len(), 5);
    for topic in [
        "tickers",
        "bbos",
        "markprices",
        "PERP_XLM_USDC@estfundingrate",
        "PERP_XLM_USDC_mythos@estfundingrate",
    ] {
        assert!(subscriptions.iter().any(|s| s == topic));
    }
    assert!(!subscriptions.iter().any(|s| s == "estfundingrate"));
    ws(
        &mut e,
        socket,
        r#"{"topic":"PERP_XLM_USDC_mythos@estfundingrate","ts":1682242440000,"data":{"symbol":"PERP_XLM_USDC_mythos","fundingRate":0.00046875,"fundingTs":1682242440000}}"#,
    );
    let broker = e.store().ticker("XLMUSDC_mythos").unwrap();
    assert_eq!(broker.funding_rate, 0.00046875);
    assert_eq!(broker.funding_time, 1682242440000.0);
    assert_eq!(e.store().ticker("XLMUSDC").unwrap().funding_rate, 0.0);
    for data in [
        r#"{"symbol":"PERP_XLM_USDC","fundingRate":9,"fundingTs":1}"#,
        r#"{"symbol":"PERP_XLM_USDC_mythos","fundingRate":9,"fundingTs":-1}"#,
        r#"{"symbol":"PERP_XLM_USDC_mythos","fundingTs":1}"#,
    ] {
        ws(
            &mut e,
            socket,
            &format!(r#"{{"topic":"PERP_XLM_USDC_mythos@estfundingrate","data":{data}}}"#),
        );
        assert_eq!(
            e.store().ticker("XLMUSDC_mythos").unwrap().funding_rate,
            0.00046875
        );
        assert_eq!(e.store().ticker("XLMUSDC").unwrap().funding_rate, 0.0);
    }
}

#[test]
fn malformed_quotes_empty_broker_suffixes_and_duplicate_markets_fail_seed() {
    for natives in [
        vec!["PERP_XLM_USDT"],
        vec!["PERP_XLM_USDC_"],
        vec!["PERP__USDC"],
        vec!["PERP_XLM_USDC_mythos?other"],
        vec!["PERP_XLM_USDC", "PERP_XLM_USDC"],
    ] {
        let mut e = Engine::new(Orderly::new(Options {
            public_data_only: true,
            ..Default::default()
        }));
        e.start(1000.0);
        let r = requests(&mut e).pop().unwrap();
        let mut rows = vec![market_row("PERP_BTC_USDC")];
        rows.extend(natives.into_iter().map(market_row));
        reply(&mut e, &r, &rows_envelope(&rows));
        assert!(e.store().markets().is_empty());
        assert!(!e.store().loaded().markets);
        assert!(!actions(&mut e).iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[derive(serde::Deserialize)]
struct FundingSubscription {
    event: String,
    #[serde(default)]
    topic: String,
}
fn subscription_topics(actions: &[Action], socket: SocketId) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket => {
                serde_json::from_str::<FundingSubscription>(text)
                    .ok()
                    .filter(|s| s.event == "subscribe")
                    .map(|s| s.topic)
            }
            _ => None,
        })
        .collect()
}
fn many_market_sockets(count: usize) -> (Driver, Vec<SocketId>) {
    let natives: Vec<_> = (0..count)
        .map(|n| format!("PERP_ASSET{n:03}_USDC"))
        .collect();
    ready_sockets(
        &rows_envelope(&natives.iter().map(|n| market_row(n)).collect::<Vec<_>>()),
        &rows_envelope(&natives.iter().map(|n| ticker_row(n)).collect::<Vec<_>>()),
    )
}

#[test]
fn funding_shards_keep_main_socket_capacity_and_route_each_pong_to_its_sender() {
    let (mut e, sockets) = many_market_sockets(201);
    assert_eq!(sockets.len(), 4, "one main plus three funding sockets");
    let out = actions(&mut e);
    let main_topics = subscription_topics(&out, sockets[0]);
    assert_eq!(main_topics, ["tickers", "bbos", "markprices"]);
    let mut all = std::collections::HashSet::new();
    for (socket, count) in sockets[1..].iter().zip([90, 90, 21]) {
        let topics = subscription_topics(&out, *socket);
        assert_eq!(topics.len(), count);
        assert!(topics.iter().all(|t| t.ends_with("@estfundingrate")));
        for topic in topics {
            assert!(all.insert(topic));
        }
    }
    assert_eq!(all.len(), 201);
    for socket in [sockets[1], sockets[3]] {
        ws(&mut e, socket, r#"{"event":"ping"}"#);
        let out = actions(&mut e);
        assert_eq!(
            out.iter()
                .filter(
                    |a| matches!(a, Action::WsSend { text, .. } if text == "{\"event\":\"pong\"}")
                )
                .count(),
            1
        );
        assert!(out.iter().any(|a| matches!(a, Action::WsSend { socket: s, text } if *s == socket && text == "{\"event\":\"pong\"}")));
    }
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ASSET000USDC".into(),
        }),
    );
    let out = actions(&mut e);
    assert!(out.iter().any(|a| matches!(a, Action::WsSend { socket, text } if *socket == sockets[0] && text.contains("ASSET000_USDC@orderbookupdate"))));
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::WsSend { socket, .. } if *socket != sockets[0]))
    );
}

#[test]
fn funding_watchdog_reconnects_only_its_shard_and_ignores_late_old_socket_frames() {
    let (mut e, sockets) = many_market_sockets(201);
    let out = actions(&mut e);
    let original_topics = subscription_topics(&out, sockets[1]);
    let foreign_topic = subscription_topics(&out, sockets[2])[0].clone();
    let native = original_topics[0].strip_suffix("@estfundingrate").unwrap();
    let internal = native.strip_prefix("PERP_").unwrap().replace('_', "");
    let frame = |topic: &str, native: &str, rate| {
        format!(
            r#"{{"topic":"{topic}","data":{{"symbol":"{native}","fundingRate":{rate},"fundingTs":12345}}}}"#
        )
    };
    ws(
        &mut e,
        sockets[1],
        &frame(&original_topics[0], native, 0.001),
    );
    assert_eq!(e.store().ticker(&internal).unwrap().funding_rate, 0.001);
    let timers: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 4);
    e.handle(31000.0, Event::TimerFired { timer: timers[1] });
    let out = actions(&mut e);
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a, Action::WsClose { .. }))
            .count(),
        1
    );
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == sockets[1]))
    );
    let retry = out
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 32000.0,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(32000.0, Event::TimerFired { timer: retry });
    let fresh = actions(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    assert!(!sockets.contains(&fresh));
    e.handle(32000.0, Event::WsOpened { socket: fresh });
    let out = actions(&mut e);
    assert_eq!(subscription_topics(&out, fresh), original_topics);
    ws(&mut e, sockets[1], &frame(&original_topics[0], native, 0.9));
    assert_eq!(e.store().ticker(&internal).unwrap().funding_rate, 0.001);
    ws(&mut e, fresh, &frame(&original_topics[0], native, 0.002));
    assert_eq!(e.store().ticker(&internal).unwrap().funding_rate, 0.002);
    let foreign_native = foreign_topic.strip_suffix("@estfundingrate").unwrap();
    ws(&mut e, fresh, &frame(&foreign_topic, foreign_native, 0.9));
    let foreign_internal = foreign_native
        .strip_prefix("PERP_")
        .unwrap()
        .replace('_', "");
    assert_eq!(
        e.store().ticker(&foreign_internal).unwrap().funding_rate,
        0.0
    );
}
