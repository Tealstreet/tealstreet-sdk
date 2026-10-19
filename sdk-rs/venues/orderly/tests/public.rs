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
    assert!(!messages.iter().any(|m| m.contains("PERP_ETH_USDC")));
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
