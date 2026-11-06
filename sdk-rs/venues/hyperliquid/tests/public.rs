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
/// The socket a batch opened: it connects with the first catalogue, not after discovery.
fn opened(actions: &[Action]) -> Option<SocketId> {
    actions.iter().find_map(|a| match a {
        Action::WsConnect { socket, .. } => Some(*socket),
        _ => None,
    })
}
fn http_id(actions: &[Action]) -> RequestId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
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
    let catalogue = prepared_now(&mut e);
    response(&mut e, http_id(&catalogue), "[null]");
    let socket = opened(&catalogue).expect("the socket opens with the main catalogue");
    assert!(opened(&prepared_now(&mut e)).is_none(), "one socket");
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
        let catalogue = prepared_now(&mut e);
        response(&mut e, http_id(&catalogue), r#"[null,{"name":"xyz"}]"#);
        let dexs = prepared_now(&mut e);
        response(
            &mut e,
            http_id(&dexs),
            r#"[{"universe":[{"name":"xyz:FOO","szDecimals":2,"maxLeverage":10}]},[{"midPx":"50"}]]"#,
        );
        let discovered = prepared_now(&mut e);
        let socket = opened(&catalogue)
            .or_else(|| opened(&dexs))
            .or_else(|| opened(&discovered))
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

// HL answers frames in the order sent, so an unnamed cap refusal answers the oldest frame
// still unanswered: here the candle, never the book that desired order lists ahead of it.
#[test]
fn an_unnamed_refusal_answers_the_oldest_unanswered_frame_not_desired_order() {
    let (mut e, s) = start();
    prepared_now(&mut e).into_iter().for_each(drop);
    for ack in [
        r#"{"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"allMids"}}}"#,
        r#"{"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"fastAssetCtxs"}}}"#,
    ] {
        message(&mut e, s, ack);
    }
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTC-PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    // The host takes the candle frame, so it goes out before the book is asked for.
    assert_eq!(sent(&prepared_now(&mut e)).len(), 1);
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ETH-PERP".into(),
        }),
    );
    prepared_now(&mut e).into_iter().for_each(drop);
    message(
        &mut e,
        s,
        r#"{"channel":"error","data":"Cannot subscribe to more than 1000 channels."}"#,
    );
    let actions = prepared_now(&mut e);
    assert_eq!(
        notices(&actions),
        ["Hyperliquid refused candle BTC 1m: Cannot subscribe to more than 1000 channels."]
    );
    assert_eq!(
        sent(&actions),
        [
            r#"{"method":"unsubscribe","subscription":{"type":"candle","coin":"BTC","interval":"1m"}}"#
        ],
        "the working book keeps its subscription"
    );
}

fn ack(e: &mut Engine<Hyperliquid>, socket: SocketId, at: f64, text: &str) {
    let subscription = text.trim_start_matches(r#"{"method":"subscribe","subscription":"#);
    let subscription = &subscription[..subscription.len() - 1];
    let reply = format!(
        r#"{{"channel":"subscriptionResponse","data":{{"method":"subscribe","subscription":{subscription}}}}}"#
    );
    e.handle(
        at,
        Event::WsMessage {
            socket,
            text: &reply,
        },
    );
}

fn reconnected_socket(actions: &[Action]) -> Option<SocketId> {
    actions.iter().find_map(|a| match a {
        Action::WsConnect { socket, .. } => Some(*socket),
        _ => None,
    })
}

// claude-fe7lgm's probe on #3151: a book asked for while reconnecting was encoded but never
// sent on the old socket, and Ready replays everything. The refusal must still answer the
// frame actually sent last, not allMids.
#[test]
fn a_reconnect_with_an_interim_proposal_keeps_the_refusal_on_the_frame_sent() {
    let (mut e, s) = start();
    prepared_now(&mut e).into_iter().for_each(drop);
    e.handle(
        2000.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let closed = prepared_now(&mut e);
    e.handle(
        2001.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ETH-PERP".into(),
        }),
    );
    let mut pending: Vec<Action> = closed;
    pending.extend(prepared_now(&mut e));
    let mut timers: Vec<_> = pending
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    timers.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut replay = Vec::new();
    let mut socket = None;
    let mut now = 2002.0;
    for (timer, due) in timers {
        now = due.max(now);
        e.handle(now, Event::TimerFired { timer });
        let batch = prepared_now(&mut e);
        if let Some(next) = reconnected_socket(&batch) {
            e.handle(now, Event::WsOpened { socket: next });
            replay = sent(&prepared_now(&mut e));
            socket = Some(next);
            break;
        }
    }
    let socket = socket.expect("reconnect");
    assert!(
        replay
            .iter()
            .any(|t| t.contains(r#""type":"l2Book","coin":"ETH""#)),
        "{replay:?}"
    );
    for text in &replay {
        ack(&mut e, socket, now + 1.0, text);
    }
    e.handle(
        now + 2.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTC-PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    assert_eq!(sent(&prepared_now(&mut e)).len(), 1, "the candle goes out");
    e.handle(
        now + 3.0,
        Event::WsMessage {
            socket,
            text: r#"{"channel":"error","data":"Cannot subscribe to more than 1000 channels."}"#,
        },
    );
    let actions = prepared_now(&mut e);
    assert_eq!(
        notices(&actions),
        ["Hyperliquid refused candle BTC 1m: Cannot subscribe to more than 1000 channels."]
    );
    assert_eq!(
        sent(&actions),
        [
            r#"{"method":"unsubscribe","subscription":{"type":"candle","coin":"BTC","interval":"1m"}}"#
        ],
        "allMids and the book keep their subscriptions"
    );
}

// A proposal made before the host takes the previous one's frames supersedes them: the
// engine drops the queued candle and re-encodes it after the book, in desired order. The
// refusal names what actually went out first (the book), never the superseded encode.
#[test]
fn a_superseded_revision_never_answers_for_frames_it_did_not_send() {
    let (mut e, s) = start();
    prepared_now(&mut e).into_iter().for_each(drop);
    for ack in [
        r#"{"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"allMids"}}}"#,
        r#"{"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"fastAssetCtxs"}}}"#,
    ] {
        message(&mut e, s, ack);
    }
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTC-PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    e.handle(
        2000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ETH-PERP".into(),
        }),
    );
    let wire = sent(&prepared_now(&mut e));
    assert!(
        wire[0].contains(r#""type":"l2Book""#),
        "the book goes out first: {wire:?}"
    );
    message(
        &mut e,
        s,
        r#"{"channel":"error","data":"Cannot subscribe to more than 1000 channels."}"#,
    );
    assert_eq!(
        notices(&prepared_now(&mut e)),
        ["Hyperliquid refused l2Book ETH: Cannot subscribe to more than 1000 channels."]
    );
}

// Open, close and reopen one book inside a round trip: the first subscribe's key has left
// the released ledger by the time the reopen is proposed, yet it was sent and HL answers
// it. The refusal after the four answers is the reopened l2Book's, not one frame later.
#[test]
fn a_book_reopened_within_a_round_trip_keeps_the_refusal_on_the_right_frame() {
    let (mut e, s) = start();
    prepared_now(&mut e).into_iter().for_each(drop);
    for ack in [
        r#"{"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"allMids"}}}"#,
        r#"{"channel":"subscriptionResponse","data":{"method":"subscribe","subscription":{"type":"fastAssetCtxs"}}}"#,
    ] {
        message(&mut e, s, ack);
    }
    let book = |subscribe: bool| {
        if subscribe {
            Command::SubscribeOrderBook {
                symbol: "ETH-PERP".into(),
            }
        } else {
            Command::UnsubscribeOrderBook {
                symbol: "ETH-PERP".into(),
            }
        }
    };
    for (subscribe, frames) in [(true, 2), (false, 2), (true, 2)] {
        e.handle(2000.0, Event::Command(book(subscribe)));
        assert_eq!(sent(&prepared_now(&mut e)).len(), frames);
    }
    // HL answers the first subscribe and unsubscribe pairs in order.
    for method in ["subscribe", "subscribe", "unsubscribe", "unsubscribe"] {
        let reply = format!(
            r#"{{"channel":"subscriptionResponse","data":{{"method":"{method}","subscription":{{"type":"l2Book","coin":"ETH"}}}}}}"#
        );
        message(&mut e, s, &reply);
    }
    message(
        &mut e,
        s,
        r#"{"channel":"error","data":"Cannot subscribe to more than 1000 channels."}"#,
    );
    assert_eq!(
        notices(&prepared_now(&mut e)),
        ["Hyperliquid refused l2Book ETH: Cannot subscribe to more than 1000 channels."]
    );
}

fn sent_texts(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

// 5b15cb: the socket waited for every HIP-3 dex's metadata, so a slow catalogue left
// the whole public stream silent. It now opens with the main catalogue, and each dex
// adds its topics as its metadata lands.
#[test]
fn the_public_stream_starts_before_hip3_discovery_finishes() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.0);
    let id = request(&mut e);
    response(
        &mut e,
        id,
        r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50}]},[{"midPx":"100","markPx":"100"}]]"#,
    );
    let catalogue = prepared_now(&mut e);
    let socket = opened(&catalogue).expect("the socket opens with the main catalogue");
    response(&mut e, http_id(&catalogue), r#"[null,{"name":"xyz"}]"#);
    let dex_meta = http_id(&prepared_now(&mut e));
    e.handle(1000.0, Event::WsOpened { socket });
    let streams = sent_texts(&prepared_now(&mut e));
    for kind in ["allMids", "fastAssetCtxs"] {
        assert!(
            streams
                .iter()
                .any(|t| t.contains(&format!("\"type\":\"{kind}\""))),
            "{kind} streams while xyz is still loading: {streams:?}"
        );
    }
    e.handle(
        1500.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "XYZ|FOO-PERP".into(),
        }),
    );
    assert!(
        sent_texts(&prepared_now(&mut e)).is_empty(),
        "xyz:FOO is not known yet"
    );
    e.handle(
        2000.0,
        Event::HttpResponse {
            request: dex_meta,
            response: Response {
                status: 200,
                headers: "",
                body: r#"[{"universe":[{"name":"xyz:FOO","szDecimals":2,"maxLeverage":10}]},[{"midPx":"50"}]]"#,
            },
        },
    );
    let added = sent_texts(&prepared_now(&mut e));
    assert!(
        added
            .iter()
            .any(|t| t.contains(r#""type":"l2Book","coin":"xyz:FOO""#)),
        "the dex's book joins the open socket: {added:?}"
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

#[test]
fn unchanged_fast_context_exports_its_real_receipt_time_for_shared_accounts() {
    use safe_cex_engine::model::StopTriggerType::MarkPrice;
    let (mut source, socket) = start();
    source.set_account_startup(false);
    source.take_frame(1_000.);
    let text = r#"{"channel":"fastAssetCtxs","data":"q1ZyCnFWsqpWyk0syg6oULJSsjQ3NTDQM1Wq1VFyDfFAkTI2MzXQMwJLVVRWWfmFuTiiyBuamOoZKdXWAgA="}"#;
    source.handle(2_000., Event::WsMessage { socket, text });
    source.take_frame(2_001.);
    source.handle(8_000., Event::WsMessage { socket, text });
    assert_eq!(
        source.store().reference_seen_at("BTC-PERP", MarkPrice),
        Some(8_000.)
    );
    let frame = source.take_frame(8_100.).expect("observation frame");
    assert!(
        !safe_cex_engine::wire::sections(frame).any(|(kind, _)| kind == 4),
        "same prices: no value delta"
    );
    let (_, bytes) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == 24)
        .expect("unchanged observed mark must reach the shared source");
    let times: Vec<_> = bytes
        .as_chunks::<8>()
        .0
        .iter()
        .map(|v| f64::from_le_bytes(*v))
        .collect();
    assert_eq!(times[2], 8_000.);
    assert_eq!(source.store().ticker("BTC-PERP").unwrap().mark, 97_500.5);
}
