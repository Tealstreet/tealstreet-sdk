//! Payloads from the official info/perpetuals and websocket/subscriptions docs.
//! https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Command, Engine, Event, RequestId, Response, SocketId};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

#[path = "../../../engine/tests/support/public_managed_headers.rs"]
mod public_managed_headers;

fn response(engine: &mut Engine<Hyperliquid>, request: RequestId, body: &str) {
    engine.handle(
        1000.0,
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
fn request(engine: &mut Engine<Hyperliquid>) -> RequestId {
    prepared_now(engine)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap()
}
fn start() -> (Engine<Hyperliquid>, SocketId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.0);
    let id = request(&mut e);
    response(
        &mut e,
        id,
        r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50},{"name":"ETH","szDecimals":4,"maxLeverage":50}]},[{"midPx":"97500.5","markPx":"97500.5","oraclePx":"97500","prevDayPx":"95000","dayNtlVlm":"195001","openInterest":"2","funding":"0.0001"},{"midPx":"3650.25","markPx":"3650.25","szDecimals":4}]]"#,
    );
    let id = request(&mut e);
    response(&mut e, id, "[null]");
    let socket = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1000.0, Event::WsOpened { socket });
    (e, socket)
}
fn message(e: &mut Engine<Hyperliquid>, socket: SocketId, text: &str) {
    e.handle(2000.0, Event::WsMessage { socket, text });
}
// fastAssetCtxs carries every HIP-3 dex's mids and marks, so a per-dex allMids is a duplicate
// against the 1,000-subscription per-IP cap.
#[test]
fn dex_mids_ride_fast_asset_contexts_not_a_per_dex_all_mids() {
    for (public_data_only, identity) in [(true, false), (true, true), (false, false), (false, true)]
    {
        let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
            public_data_only,
            application_id: identity.then(|| "0x0000000000000000000000000000000000000001".into()),
            ..Options::default()
        }));
        e.start(1000.0);
        let id = request(&mut e);
        response(&mut e, id, r#"[{"universe":[]},[]]"#);
        let id = request(&mut e);
        response(&mut e, id, r#"[null,{"name":"xyz"}]"#);
        let id = request(&mut e);
        response(
            &mut e,
            id,
            r#"[{"universe":[{"name":"xyz:FOO","szDecimals":2,"maxLeverage":10}]},[{"midPx":"50"}]]"#,
        );
        let socket = prepared_now(&mut e)
            .into_iter()
            .find_map(|action| match action {
                Action::WsConnect { socket, .. } => Some(socket),
                _ => None,
            })
            .unwrap();
        e.handle(1000.0, Event::WsOpened { socket });
        let types: Vec<_> = prepared_now(&mut e)
            .into_iter()
            .filter_map(|action| match action {
                Action::WsSend { text, .. } => {
                    let message: serde_json::Value = serde_json::from_str(&text).unwrap();
                    let subscription = &message["subscription"];
                    (subscription["user"].is_null())
                        .then(|| format!("{} {}", subscription["type"], subscription["dex"]))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            types,
            [r#""allMids" null"#, r#""fastAssetCtxs" null"#],
            "public={public_data_only}, identity={identity}"
        );
    }
}
#[test]
fn markets_units_and_compressed_partial_fields() {
    let (mut e, s) = start();
    assert_eq!(e.store().markets().len(), 2);
    let btc = e.store().ticker("BTC-PERP").unwrap();
    assert_eq!(
        (btc.volume, btc.quote_volume, btc.open_interest),
        (2.0, 195001.0, 195001.0)
    );
    message(
        &mut e,
        s,
        r#"{"channel":"allMids","data":{"mids":{"BTC":"98000"}}}"#,
    );
    message(
        &mut e,
        s,
        r#"{"channel":"fastAssetCtxs","data":"q1ZyCnFWsqpWyk0syg6oULJSsjQ3NTDQM1Wq1VFyDfFAkTI2MzXQMwJLVVRWWfmFuTiiyBuamOoZKdXWAgA="}"#,
    );
    let btc = e.store().ticker("BTC-PERP").unwrap();
    assert_eq!((btc.last, btc.mark), (98000.0, 97500.5));
    message(&mut e, s, r#"{"channel":"fastAssetCtxs","data":"broken"}"#);
    assert_eq!(e.store().ticker("BTC-PERP").unwrap().last, 98000.0);
}
#[test]
fn full_depth_bbo_overlay_stale_bbo_and_reconnect() {
    let (mut e, s) = start();
    e.handle(
        1000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    message(
        &mut e,
        s,
        r#"{"channel":"l2Book","data":{"coin":"BTC","time":100,"levels":[[{"px":"100","sz":"2","n":1},{"px":"99","sz":"3","n":1},{"px":"98","sz":"4","n":1}],[{"px":"101","sz":"5","n":1},{"px":"102","sz":"6","n":1}]]}}"#,
    );
    message(
        &mut e,
        s,
        r#"{"channel":"bbo","data":{"coin":"BTC","time":101,"bbo":[{"px":"99","sz":"7","n":1},{"px":"102","sz":"8","n":1}]}}"#,
    );
    let b = e.book("BTC-PERP").unwrap();
    assert_eq!(b.bids(), vec![(99.0, 7.0), (98.0, 4.0)]);
    assert_eq!(b.asks(), vec![(102.0, 8.0)]);
    message(
        &mut e,
        s,
        r#"{"channel":"bbo","data":{"coin":"BTC","time":99,"bbo":[null,null]}}"#,
    );
    assert_eq!(e.book("BTC-PERP").unwrap().bids().len(), 2);
    e.handle(
        3000.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    assert!(e.book("BTC-PERP").is_none());
}
#[test]
fn posted_candle_read_preserves_body_and_streams() {
    let (mut e, _) = start();
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        1000.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "BTC-PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: Some(60000.0),
        }),
    );
    let req = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        req.body.as_deref(),
        Some(
            r#"{"type":"candleSnapshot","req":{"coin":"BTC","interval":"1m","startTime":0,"endTime":60000}}"#
        )
    );
    public_managed_headers::assert_native_public_intent_survives_managed_release(&req);
    response(
        &mut e,
        req.id,
        r#"[{"t":0,"o":"1","h":"2","l":"1","c":"2","v":"3"}]"#,
    );
    assert!(prepared_now(&mut e).into_iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone { .. })
    )));
}
#[test]
fn silence_watchdog_closes_without_a_close_event() {
    let (mut e, _) = start();
    let timer = prepared_now(&mut e)
        .into_iter()
        .filter_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    e.handle(50000.0, Event::TimerFired { timer });
    assert!(
        prepared_now(&mut e)
            .into_iter()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
}
#[test]
fn incomplete_metadata_never_commits_and_schedules_a_read_retry() {
    for body in [
        "[]",
        r#"[{"universe":[]}]"#,
        r#"[[],{"universe":[]}]"#,
        r#"[{"universe":[{"name":"BTC","szDecimals":5}]},[]]"#,
    ] {
        let mut e =
            safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options::default()));
        e.start(0.0);
        let id = request(&mut e);
        response(&mut e, id, body);
        assert!(!e.store().loaded().markets);
        assert!(
            prepared_now(&mut e)
                .into_iter()
                .any(|a| matches!(a, Action::SetTimer { .. }))
        );
    }
}
#[test]
fn invalid_interval_is_a_command_error_and_limit_caps_at_1500() {
    let (mut e, _) = start();
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        1000.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "BTC-PERP".into(),
            interval: safe_cex_engine::model::Interval::S1,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    assert!(prepared_now(&mut e).into_iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandFailed { .. })
    )));
    e.handle(
        1000.0,
        Event::Command(Command::FetchCandles {
            command: 2,
            symbol: "BTC-PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2000),
            from_ms: Some(1.0),
            to_ms: Some(90_000_000.0),
        }),
    );
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert!(r.body.as_deref().unwrap().contains("\"startTime\":0"));
    response(
        &mut e,
        r.id,
        r#"[{"t":0,"o":"NaN","h":"2","l":"1","c":"2","v":"3"}]"#,
    );
    assert!(prepared_now(&mut e).into_iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandFailed { .. })
    )));
}

#[test]
fn malformed_nested_rows_keep_positional_native_asset_mapping_and_valid_peers() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options::default()));
    e.start(0.);
    // Skipped discovery rows log at Debug.
    e.set_log_level(safe_cex_engine::Level::Debug);
    let id = request(&mut e);
    response(
        &mut e,
        id,
        r#"[{"universe":[{}, {"name":"BTC","szDecimals":5}, {"name":"ETH","szDecimals":4}]},[{"midPx":"999"},{"midPx":"100","markPx":"100"},{"midPx":"bad"}]]"#,
    );
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().markets()[0].id, "BTC");
    assert_eq!(e.store().tickers()[0].last, 100.);
    assert!(prepared_now(&mut e).into_iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message == "Skipped malformed public discovery row")));
}

#[test]
fn base_metadata_failure_keeps_backing_off_until_a_valid_read_arrives() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options::default()));
    e.start(0.);
    for delay in [1000., 2000., 4000., 8000., 16000., 30000., 30000.] {
        let id = request(&mut e);
        e.handle(
            1000.,
            Event::HttpFailed {
                request: id,
                error: "offline",
            },
        );
        let (timer, at) = prepared_now(&mut e)
            .into_iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .unwrap();
        assert!(
            (0.75 * delay..=delay).contains(&(at - 1000.)),
            "{at} is not a jittered {delay} ms backoff"
        );
        e.handle(at, Event::TimerFired { timer });
        assert!(!e.store().loaded().markets);
    }
    let id = request(&mut e);
    response(
        &mut e,
        id,
        r#"[{"universe":[{"name":"BTC","szDecimals":5}]},[{"midPx":"100"}]]"#,
    );
    assert_eq!(e.store().markets().len(), 1);
}

#[test]
fn a_market_without_a_positive_step_is_skipped_as_malformed() {
    for sz in ["400", "-2147483648"] {
        let mut e =
            safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options::default()));
        e.start(0.);
        e.set_log_level(safe_cex_engine::Level::Debug);
        let id = request(&mut e);
        response(
            &mut e,
            id,
            &format!(
                r#"[{{"universe":[{{"name":"BTC","szDecimals":5}},{{"name":"ETH","szDecimals":{sz}}}]}},[{{"midPx":"100","markPx":"100"}},{{"midPx":"10","markPx":"10"}}]]"#
            ),
        );
        let ids: Vec<_> = e.store().markets().iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["BTC"], "szDecimals {sz}");
        assert!(prepared_now(&mut e).into_iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, ..})) if message == "Skipped malformed public discovery row")));
    }
}

/// Every timer currently armed after `actions`, by deadline.
fn armed(actions: Vec<Action>, timers: &mut Vec<(safe_cex_engine::TimerId, f64)>) {
    for action in actions {
        match action {
            Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
            Action::ClearTimer { timer } => timers.retain(|(t, _)| *t != timer),
            _ => {}
        }
    }
}

/// Fires due timers through `until`, answering each ping with a pong; true once the socket closed.
fn run_answering_pings(
    e: &mut Engine<Hyperliquid>,
    socket: SocketId,
    timers: &mut Vec<(safe_cex_engine::TimerId, f64)>,
    until: f64,
) -> bool {
    while let Some(&(timer, at)) = timers
        .iter()
        .filter(|(_, at)| *at <= until)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        timers.retain(|(t, _)| *t != timer);
        e.handle(at, Event::TimerFired { timer });
        let actions = safe_cex_engine::test_support::prepared_actions(e, at);
        if actions.iter().any(|a| matches!(a, Action::WsClose { .. })) {
            return true;
        }
        armed(actions, timers);
        e.handle(
            at,
            Event::WsMessage {
                socket,
                text: r#"{"channel":"pong"}"#,
            },
        );
        armed(
            safe_cex_engine::test_support::prepared_actions(e, at),
            timers,
        );
    }
    false
}

// 5b15cb / ebb012: an open socket answering every ping while its public streams carry nothing
// was never recycled, because any text frame counted as traffic.
#[test]
fn a_socket_answering_pings_but_carrying_no_public_data_is_recycled() {
    let (mut e, s) = start();
    let mut timers = Vec::new();
    armed(prepared_now(&mut e), &mut timers);
    message(
        &mut e,
        s,
        r#"{"channel":"allMids","data":{"mids":{"BTC":"98000"}}}"#,
    );
    armed(prepared_now(&mut e), &mut timers);
    assert!(
        !run_answering_pings(&mut e, s, &mut timers, 31_000.),
        "inside the window"
    );
    assert!(
        run_answering_pings(&mut e, s, &mut timers, 33_000.),
        "30 s of pongs alone"
    );
}

#[test]
fn public_data_keeps_a_pinged_socket() {
    let (mut e, s) = start();
    let mut timers = Vec::new();
    armed(prepared_now(&mut e), &mut timers);
    for second in (10..=120).step_by(10) {
        let at = f64::from(second) * 1_000.;
        assert!(!run_answering_pings(&mut e, s, &mut timers, at), "at {at}");
        e.handle(
            at,
            Event::WsMessage {
                socket: s,
                text: r#"{"channel":"allMids","data":{"mids":{"BTC":"98000"}}}"#,
            },
        );
        armed(
            safe_cex_engine::test_support::prepared_actions(&mut e, at),
            &mut timers,
        );
    }
}

fn notices(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(notice)) => {
                Some(notice.message.to_string())
            }
            _ => None,
        })
        .collect()
}

fn sent(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

// The per-IP cap names nothing; it answers the oldest subscription still unacknowledged.
#[test]
fn a_capped_subscription_is_named_and_not_resubscribed_silently() {
    let (mut e, s) = start();
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        1000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ETH-PERP".into(),
        }),
    );
    prepared_now(&mut e).into_iter().for_each(drop);
    for ack in [
        r#"{"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"allMids"}}}"#,
        r#"{"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"fastAssetCtxs"}}}"#,
        r#"{"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"l2Book","coin":"ETH"}}}"#,
    ] {
        message(&mut e, s, ack);
    }
    message(
        &mut e,
        s,
        r#"{"channel":"error","data":"Cannot subscribe to more than 1000 channels."}"#,
    );
    let actions = prepared_now(&mut e);
    assert_eq!(
        notices(&actions),
        ["Hyperliquid refused bbo ETH: Cannot subscribe to more than 1000 channels."]
    );
    assert_eq!(
        sent(&actions),
        [r#"{"method":"unsubscribe","subscription":{"type":"bbo","coin":"ETH"}}"#],
        "dropped from the desired set, not retried in place"
    );
    message(
        &mut e,
        s,
        r#"{"channel":"error","data":"Already unsubscribed: {\"type\":\"bbo\",\"coin\":\"ETH\"}"}"#,
    );
    assert!(
        notices(&prepared_now(&mut e)).is_empty(),
        "the venue's own echo is not news"
    );
}

#[test]
fn a_refusal_naming_its_subscription_drops_that_one_and_a_fresh_subscribe_retries_it() {
    let (mut e, s) = start();
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        1000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    prepared_now(&mut e).into_iter().for_each(drop);
    message(
        &mut e,
        s,
        r#"{"channel":"error","data":"Invalid subscription {\"method\":\"subscribe\",\"subscription\":{\"type\":\"l2Book\",\"coin\":\"BTC\"}}"}"#,
    );
    let actions = prepared_now(&mut e);
    assert_eq!(notices(&actions).len(), 1);
    assert!(
        notices(&actions)[0].starts_with("Hyperliquid refused l2Book BTC: Invalid subscription")
    );
    assert_eq!(
        sent(&actions),
        [r#"{"method":"unsubscribe","subscription":{"type":"l2Book","coin":"BTC"}}"#]
    );
    e.handle(
        3000.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        4000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    let again = sent(&prepared_now(&mut e));
    assert!(
        again.contains(
            &r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"BTC"}}"#.to_owned()
        ),
        "the user asked again: {again:?}"
    );
}

// A chart's forced reset repairs its candle: a refused subscribe left the desired set, so
// the engine would only answer "not subscribed". The venue sends it once more instead.
#[test]
fn a_repair_of_a_refused_candle_sends_its_subscribe_once_more() {
    let (mut e, s) = start();
    prepared_now(&mut e).into_iter().for_each(drop);
    let candle = safe_cex_engine::SubscriptionTopic::Candles {
        symbol: "BTC-PERP".into(),
        interval: safe_cex_engine::model::Interval::M1,
    };
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTC-PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    prepared_now(&mut e).into_iter().for_each(drop);
    message(
        &mut e,
        s,
        r#"{"channel":"error","data":"Invalid subscription {\"type\":\"candle\",\"coin\":\"BTC\",\"interval\":\"1m\"}"}"#,
    );
    assert_eq!(
        sent(&prepared_now(&mut e)),
        [
            r#"{"method":"unsubscribe","subscription":{"type":"candle","coin":"BTC","interval":"1m"}}"#
        ]
    );
    e.handle(
        2000.0,
        Event::Command(Command::RepairSubscription(candle.clone())),
    );
    let repaired = prepared_now(&mut e);
    assert_eq!(
        sent(&repaired),
        [r#"{"method":"subscribe","subscription":{"type":"candle","coin":"BTC","interval":"1m"}}"#]
    );
    assert!(notices(&repaired).is_empty(), "{:?}", notices(&repaired));
    // A topic nobody subscribed is still the engine's "not subscribed".
    e.handle(
        2000.0,
        Event::Command(Command::RepairSubscription(
            safe_cex_engine::SubscriptionTopic::OrderBook {
                symbol: "ETH-PERP".into(),
            },
        )),
    );
    assert!(
        notices(&prepared_now(&mut e))
            .iter()
            .any(|n| n.contains("is not subscribed"))
    );
}
