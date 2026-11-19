#![allow(dead_code)]
mod support;
use safe_cex_engine::{Action, Engine, Event, SocketId};
use safe_cex_venue_okx::Okx;
use support::*;

const RVN: &str = "RVN-USDT-SWAP";

fn subscribed() -> (Engine<Okx>, SocketId, SocketId) {
    let mut e = engine(FixtureOptions::default());
    e.start(0.0);
    let request = take_http(&mut prepared(&mut e, 0.0));
    let markets = format!("{MARKET},{}", MARKET.replace("BTC", "RVN"));
    reply(&mut e, &request, &envelope(&markets), 1.0);
    let request = take_http(&mut prepared(&mut e, 1.0));
    let tickers = format!("{TICKER},{}", TICKER.replace("BTC", "RVN"));
    reply(&mut e, &request, &envelope(&tickers), 2.0);
    let sockets = prepared(&mut e, 2.0);
    let socket = |suffix: &str| {
        sockets
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. } if url.ends_with(suffix) => Some(*socket),
                _ => None,
            })
            .unwrap()
    };
    let public = socket("/public");
    let business = socket("/business");
    e.handle(3.0, Event::WsOpened { socket: public });
    e.handle(3.0, Event::WsOpened { socket: business });
    let out = prepared(&mut e, 3.0);
    assert!(has_topic(&out, "subscribe", "funding-rate", RVN));
    (e, public, business)
}

fn has_topic(out: &[Action], op: &str, channel: &str, id: &str) -> bool {
    out.iter().any(|a| match a {
        Action::WsSend { text, .. } => {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
                return false;
            };
            value["op"] == op
                && value["args"].as_array().is_some_and(|args| {
                    args.iter()
                        .any(|arg| arg["channel"] == channel && arg["instId"] == id)
                })
        }
        _ => false,
    })
}

fn reconnect(e: &mut Engine<Okx>, socket: SocketId) -> Vec<Action> {
    e.handle(10.0, Event::WsClosed { socket, code: 1006 });
    let out = prepared(e, 10.0);
    let out = release_paced(e, 10.0, out);
    let socket = out
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(*socket),
            _ => None,
        })
        .expect("replacement public socket");
    e.handle(6_000.0, Event::WsOpened { socket });
    prepared(e, 6_000.0)
}

fn missing_topic_refreshes_and_stays_retired(error: &str) {
    let (mut e, public, _) = subscribed();
    ws(&mut e, public, error);
    let mut out = prepared(&mut e, 4.0);
    assert!(
        out.iter()
            .any(|a| is_get(a, "/api/v5/public/instruments?instType=SWAP")),
        "exact missing public topic must refresh instruments: {out:?}"
    );
    assert!(has_topic(&out, "unsubscribe", "funding-rate", RVN));
    let request = take_http(&mut out);
    reply(&mut e, &request, &envelope(MARKET), 5.0);
    prepared(&mut e, 5.0);
    // Cached rows remain usable by financial state; only this stream is retired.
    assert!(e.store().market_by_id(RVN).is_some());
    e.handle(
        6.0,
        Event::WsMessage {
            socket: public,
            text: error,
        },
    );
    assert!(
        prepared(&mut e, 6.0)
            .iter()
            .all(|a| !matches!(a, Action::Http(_)))
    );
    let replay = reconnect(&mut e, public);
    assert!(!has_topic(&replay, "subscribe", "funding-rate", RVN));
    assert!(has_topic(
        &replay,
        "subscribe",
        "funding-rate",
        "BTC-USDT-SWAP"
    ));
    assert!(has_topic(&replay, "subscribe", "tickers", RVN));
}

#[test]
fn missing_public_topic_with_arg_refreshes_and_is_not_replayed() {
    missing_topic_refreshes_and_stays_retired(
        r#"{"event":"error","code":"60018","msg":"Wrong URL or channel doesn't exist. Please check the URL and channel","arg":{"channel":"funding-rate","instId":"RVN-USDT-SWAP"}}"#,
    );
}

#[test]
fn missing_public_topic_from_exact_native_message_is_not_replayed() {
    missing_topic_refreshes_and_stays_retired(
        r#"{"event":"error","code":"60018","msg":"funding-rate subscribe for RVN-USDT-SWAP doesn't exist"}"#,
    );
}

#[test]
fn unrelated_or_unbound_errors_preserve_desired_topics_without_refresh() {
    for error in [
        r#"{"event":"error","code":"60012","msg":"funding-rate subscribe for RVN-USDT-SWAP doesn't exist","arg":{"channel":"funding-rate","instId":"RVN-USDT-SWAP"}}"#,
        r#"{"event":"error","code":"60018","msg":"channel doesn't exist"}"#,
        r#"{"event":"error","code":"60018","msg":"channel doesn't exist","arg":{"channel":"funding-rate","instId":"UNKNOWN-USDT-SWAP"}}"#,
        r#"{"event":"error","code":"60018","msg":"prefix funding-rate subscribe for RVN-USDT-SWAP doesn't exist suffix"}"#,
    ] {
        let (mut e, public, _) = subscribed();
        ws(&mut e, public, error);
        let out = prepared(&mut e, 4.0);
        assert!(out.iter().all(|a| !matches!(a, Action::Http(_))), "{error}");
        assert!(!has_topic(&out, "unsubscribe", "funding-rate", RVN));
        assert!(has_topic(
            &reconnect(&mut e, public),
            "subscribe",
            "funding-rate",
            RVN
        ));
    }
}

#[test]
fn business_error_cannot_retire_a_public_topic() {
    let (mut e, public, business) = subscribed();
    ws(
        &mut e,
        business,
        r#"{"event":"error","code":"60018","msg":"funding-rate subscribe for RVN-USDT-SWAP doesn't exist","arg":{"channel":"funding-rate","instId":"RVN-USDT-SWAP"}}"#,
    );
    assert!(
        prepared(&mut e, 4.0)
            .iter()
            .all(|a| !matches!(a, Action::Http(_)))
    );
    assert!(has_topic(
        &reconnect(&mut e, public),
        "subscribe",
        "funding-rate",
        RVN
    ));
}
