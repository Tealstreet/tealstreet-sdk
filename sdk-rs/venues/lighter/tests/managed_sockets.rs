//! The real venue on the engine's managed sockets: reconnects, replay,
//! silence expiry and the grouped TP/SL stream proof.
mod support;
use safe_cex_engine::model::{Interval, PlaceOrderOpts};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, LogEvent, Notice, Outcome, SocketChannel, SocketId,
};
use safe_cex_venue_lighter::Lighter;
use support::*;

fn sent(actions: &[Action], socket: SocketId) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket => Some(text.clone()),
            _ => None,
        })
        .collect()
}
fn connected(actions: &[Action], channel: SocketChannel) -> Option<SocketId> {
    actions.iter().find_map(|a| match a {
        Action::WsConnect {
            socket,
            channel: Some(c),
            ..
        } if *c == channel => Some(*socket),
        _ => None,
    })
}
fn closed(actions: &[Action], socket: SocketId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
}
fn logged(actions: &[Action], text: &str) -> bool {
    actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::Event(LogEvent { message, .. })) if message == text),
    )
}
fn find(actions: &[Action], path: &str) -> Option<HttpRequest> {
    actions.iter().find_map(|a| match a {
        Action::Http(r) if r.url.contains(path) => Some(r.clone()),
        _ => None,
    })
}
/// The auth token of the `account_all_orders` subscription in `texts`.
fn auth_token(texts: &[String]) -> String {
    let frame = texts
        .iter()
        .find(|t| t.contains(r#""channel":"account_all_orders/718895""#))
        .expect("authenticated orders subscription");
    let value: serde_json::Value = serde_json::from_str(frame).unwrap();
    value["auth"].as_str().expect("auth token").to_owned()
}
/// Close `socket` at `now`, fire the engine's retry and return the new
/// socket of `channel` with the time it connected.
fn drop_and_reconnect(
    e: &mut Engine<Lighter>,
    timers: &mut Timers,
    socket: SocketId,
    channel: SocketChannel,
    now: f64,
) -> (SocketId, f64, Vec<Action>) {
    e.handle(now, Event::WsClosed { socket, code: 1006 });
    let mut actions = drain_at(e, timers, now);
    // The engine's jittered backoff is at most its 1 s base on a first retry.
    actions.extend(advance(e, timers, now + 1_000.0));
    let next = connected(&actions, channel).expect("managed reconnect");
    (next, now + 1_000.0, actions)
}

#[test]
fn dropped_private_feed_reopens_with_a_fresh_auth_token_and_resyncs_orders() {
    let (mut e, feeds, mut timers) = ready_feeds(options(), false);
    let first = feeds.private;
    e.handle(NOW, Event::WsOpened { socket: first });
    let opened = drain(&mut e, &mut timers);
    let first_token = auth_token(&sent(&opened, first));
    for (path, body) in [("/account?", ACCOUNT), ("accountActiveOrders", OPEN)] {
        let read = find(&opened, path).expect("open resync read");
        reply_at(&mut e, NOW, &read, 200, body);
    }

    let (next, at, retired) = drop_and_reconnect(
        &mut e,
        &mut timers,
        first,
        SocketChannel::Private,
        NOW + 1.0,
    );
    assert!(logged(&retired, "Lighter socket closed; reconnecting"));
    e.handle(at, Event::WsOpened { socket: next });
    let reopened = drain_at(&mut e, &mut timers, at);
    let texts = sent(&reopened, next);
    assert_eq!(
        texts[0],
        r#"{"type":"subscribe","channel":"account_all/718895"}"#
    );
    let token = auth_token(&texts);
    assert!(!token.is_empty());
    assert_ne!(token, first_token, "each reopen signs a fresh token");
    assert!(
        find(&reopened, "accountActiveOrders").is_some(),
        "the reopen resyncs open orders"
    );
    assert!(find(&reopened, "/account?").is_some());
    // The retired epoch's frames are refused; the new one's are applied.
    message(
        &mut e,
        first,
        at,
        r#"{"channel":"account_all_orders:718895","orders":{"1":[{"order_index":"77","market_id":1,"status":"open","initial_base_amount":"1","price":"80"}]}}"#,
    );
    assert!(e.store().order("77").is_none());
    message(
        &mut e,
        next,
        at,
        r#"{"channel":"account_all_orders:718895","orders":{"1":[{"order_index":"77","market_id":1,"status":"open","initial_base_amount":"1","price":"80"}]}}"#,
    );
    assert!(e.store().order("77").is_some());
}

#[test]
fn a_private_frame_after_the_silence_bound_is_refused_and_the_feed_reconnects() {
    let (mut e, feeds, mut timers) = ready_feeds(options(), true);
    let socket = feeds.private;
    // No timer has fired yet: expiry is enforced at ingress, not only by the timer.
    e.handle(
        NOW + 180_001.0,
        Event::WsMessage {
            socket,
            text: r#"{"channel":"account_all_orders:718895","orders":{"1":[{"order_index":"78","market_id":1,"status":"open","initial_base_amount":"1","price":"80"}]}}"#,
        },
    );
    let actions = drain_at(&mut e, &mut timers, NOW + 180_001.0);
    assert!(e.store().order("78").is_none(), "an expired epoch is mute");
    assert!(closed(&actions, socket));
    let retried = advance(&mut e, &mut timers, NOW + 181_001.0);
    assert!(connected(&retried, SocketChannel::Private).is_some());
}

#[test]
fn try_again_later_close_on_the_public_feed_waits_thirty_seconds() {
    let (mut e, feeds, mut timers) = ready_feeds(options(), true);
    let at = NOW + 1_000.0;
    e.handle(
        at,
        Event::WsClosed {
            socket: feeds.public,
            code: 1013,
        },
    );
    drain_at(&mut e, &mut timers, at);
    let early = advance(&mut e, &mut timers, at + 29_999.0);
    assert!(
        connected(&early, SocketChannel::Public).is_none(),
        "{early:?}"
    );
    let retry = advance(&mut e, &mut timers, at + 30_000.0);
    assert!(
        connected(&retry, SocketChannel::Public).is_some(),
        "{retry:?}"
    );
}

#[test]
fn dropped_public_feed_replays_each_current_subscription_once_then_pings() {
    let (mut e, feeds, mut timers) = ready_feeds(options(), true);
    for command in [
        Command::SubscribeOrderBook {
            symbol: "ETH".into(),
        },
        Command::SubscribeCandles {
            symbol: "ETH".into(),
            interval: Interval::M1,
        },
        Command::SubscribeCandles {
            symbol: "ETH".into(),
            interval: Interval::M5,
        },
    ] {
        e.handle(NOW, Event::Command(command));
    }
    let actions = drain(&mut e, &mut timers);
    let seed = find(&actions, "orderBookOrders").expect("book seed");
    reply_at(
        &mut e,
        NOW,
        &seed,
        200,
        r#"{"code":200,"bids":[],"asks":[]}"#,
    );
    let live = sent(&actions, feeds.public);
    assert_eq!(
        live.iter()
            .filter(|t| t.contains(r#""channel":"trade/1""#))
            .count(),
        1,
        "{live:?}"
    );

    let (next, at, retired) = drop_and_reconnect(
        &mut e,
        &mut timers,
        feeds.public,
        SocketChannel::Public,
        NOW + 1_000.0,
    );
    assert!(find(&retired, "orderBookOrders").is_some(), "books resync");
    e.handle(at, Event::WsOpened { socket: next });
    let texts = sent(&drain_at(&mut e, &mut timers, at), next);
    let subscriptions = [
        "market_stats/all",
        "ticker/1",
        "ticker/2",
        "order_book/1",
        "candle/1/1m",
        "candle/1/5m",
        "trade/1",
    ];
    for channel in subscriptions {
        let frame = format!(r#"{{"type":"subscribe","channel":"{channel}"}}"#);
        assert_eq!(
            texts.iter().filter(|t| **t == frame).count(),
            1,
            "{channel}: {texts:?}"
        );
    }
    assert_eq!(texts.len(), subscriptions.len() + 1, "{texts:?}");
    assert_eq!(texts.last().unwrap(), r#"{"type":"ping"}"#);
    // A book gap re-subscribes that one channel on the live epoch.
    message(
        &mut e,
        next,
        at,
        r#"{"channel":"order_book:1","type":"subscribed/order_book","order_book":{"nonce":5,"bids":[{"price":"99","size":"1"}],"asks":[{"price":"101","size":"1"}]}}"#,
    );
    message(
        &mut e,
        next,
        at,
        r#"{"channel":"order_book:1","type":"update/order_book","order_book":{"nonce":7,"begin_nonce":6,"bids":[],"asks":[]}}"#,
    );
    assert_eq!(
        sent(&drain_at(&mut e, &mut timers, at), next),
        [
            r#"{"type":"unsubscribe","channel":"order_book/1"}"#,
            r#"{"type":"subscribe","channel":"order_book/1"}"#,
        ]
    );
}

#[test]
fn private_silence_retirement_during_a_pending_group_fails_it_unknown() {
    let mut opts = options();
    opts.testnet = true;
    let (mut e, feeds, mut timers) = ready_feeds(opts, true);
    // The stream stays silent; the group is placed just before its 180 s bound.
    let start = NOW + 172_000.0;
    advance(&mut e, &mut timers, start);
    let grouped: PlaceOrderOpts = serde_json::from_value(serde_json::json!({
        "symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90,
        "stopLoss":80,"takeProfit":100
    }))
    .unwrap();
    e.handle(
        start,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: vec![grouped],
        }),
    );
    let nonce = find(&drain_at(&mut e, &mut timers, start), "nextNonce").expect("nonce read");
    reply_at(&mut e, start, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let send = find(&drain_at(&mut e, &mut timers, start), "sendTx").expect("submission");
    reply_at(
        &mut e,
        start,
        &send,
        200,
        r#"{"code":200,"tx_hash":"transaction-hash"}"#,
    );
    let mut out = drain_at(&mut e, &mut timers, start);
    out.extend(advance(&mut e, &mut timers, NOW + 179_999.0));
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
    );
    // Retired by the engine's silence deadline, before the group's own 10 s bound.
    out.extend(advance(&mut e, &mut timers, NOW + 180_500.0));
    assert!(closed(&out, feeds.private));
    let failed: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error.clone()),
            _ => None,
        })
        .collect();
    let [error] = &failed[..] else {
        panic!("one terminal before the proof bound: {failed:?}");
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
    );
}
