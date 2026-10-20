#![allow(dead_code)]
mod support;
use safe_cex_engine::{Action, Engine, Event};
use safe_cex_venue_okx::{Okx, Options};
use support::*;

const POSITION: &str = r#"{"instId":"BTC-USDT-SWAP","pos":"5","posSide":"net","avgPx":"39000","lever":"10","upl":"50","liqPx":"30000","mgnMode":"cross","imr":"200","mmr":"20"}"#;
const ORDER: &str = r#"{"instId":"BTC-USDT-SWAP","ordId":"123","ordType":"limit","state":"live","side":"buy","sz":"4","accFillSz":"1","px":"38000","lever":"10","reduceOnly":"false","clOrdId":"caller123"}"#;

fn startup() -> (
    Engine<Okx>,
    safe_cex_engine::SocketId,
    safe_cex_engine::SocketId,
    Vec<Action>,
) {
    let opts = Options {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Options::default()
    };
    let mut e = Engine::new(Okx::new(opts));
    e.start(0.0);
    let a = actions(&mut e).remove(0);
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let mut initial = actions(&mut e);
    let a = initial.remove(0);
    reply(&mut e, &a, &envelope(TICKER), 2.0);
    initial.extend(actions(&mut e));
    let business = initial
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/business") => Some(*socket),
            _ => None,
        })
        .unwrap();
    let config = initial
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut e,
        config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.0,
    );
    let pending = actions(&mut e);
    let private = pending
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    for socket in [private, business] {
        e.handle(4.0, Event::WsOpened { socket });
        let sent = actions(&mut e);
        assert!(sent.iter().any(|a| matches!(a, Action::WsSend { socket: s, text } if *s == socket && text.contains("\"op\":\"login\"") && text.contains("test-passphrase"))));
        ws(&mut e, socket, r#"{"event":"login","code":"0","msg":""}"#);
        let subscriptions = actions(&mut e);
        assert!(subscriptions.iter().any(|a| matches!(a, Action::WsSend { text, .. } if text.contains(if socket == business { "orders-algo" } else { "positions" }))));
    }
    (
        e,
        private,
        business,
        pending
            .into_iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .collect(),
    )
}

fn complete(e: &mut Engine<Okx>, mut requests: Vec<Action>, orders: &str) {
    while let Some(a) = requests.pop() {
        let Action::Http(r) = &a else {
            continue;
        };
        let body = if r.url.contains("account-position-risk") {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else if r.url.contains("/positions?") {
            envelope(POSITION)
        } else if r.url.contains("orders-pending") {
            envelope(orders)
        } else {
            envelope("")
        };
        reply(e, &a, &body, 5.0);
        requests.extend(
            actions(e)
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_))),
        );
    }
}

#[test]
fn authenticates_two_sockets_and_maps_contract_units_and_balance() {
    let (mut e, _, _, pending) = startup();
    complete(&mut e, pending, ORDER);
    assert!(
        e.store().loaded().balance && e.store().loaded().positions && e.store().loaded().orders
    );
    let p = &e.store().positions()[0];
    assert_eq!(p.contracts, 0.05);
    assert_eq!(p.notional, 2000.0);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 10.0);
    let o = &e.store().orders()[0];
    assert_eq!(o.amount, 4.0);
    assert_eq!(o.filled, 0.01);
    assert_eq!(o.remaining, 0.03);
    assert_eq!(o.client_order_id.as_deref(), Some("caller123"));
    assert_eq!(e.store().balance().total, 4950.0);
    assert_eq!(e.store().balance().used, 200.0);
    assert_eq!(e.store().balance().free, 4674.6);
}

#[test]
fn terminal_push_wins_over_delayed_snapshot_and_fills_dedupe() {
    let (mut e, private, _, pending) = startup();
    let live = format!(r#"{{"arg":{{"channel":"orders"}},"data":[{ORDER}]}}"#);
    ws(&mut e, private, &live);
    let filled = ORDER.replace("\"live\"", "\"filled\"").replace(
        "\"accFillSz\":\"1\"",
        "\"accFillSz\":\"4\",\"fillSz\":\"3\",\"fillPx\":\"38000\",\"tradeId\":\"987\"",
    );
    let frame = format!(r#"{{"arg":{{"channel":"orders"}},"data":[{filled}]}}"#);
    ws(&mut e, private, &frame);
    ws(&mut e, private, &frame);
    complete(&mut e, pending, ORDER);
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(
        e.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Closed
    );
    // FILLS section length identifies one execution, not duplicate delivery.
    let frame = e.take_frame(10.0).unwrap();
    let mut at = 16;
    let mut count = 0;
    while at < frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            count = u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap());
        }
        at += 8 + len;
    }
    assert_eq!(count, 1);
}

#[test]
fn position_tombstone_and_new_equity_win_over_inflight_rest() {
    let (mut e, private, _, pending) = startup();
    let zero = POSITION.replace("\"pos\":\"5\"", "\"pos\":\"0\"");
    ws(
        &mut e,
        private,
        &format!(r#"{{"arg":{{"channel":"positions"}},"data":[{zero}]}}"#),
    );
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"account"},"data":[{"totalEq":"6000"}]}"#,
    );
    complete(&mut e, pending, "");
    assert!(e.store().positions().is_empty());
    assert_eq!(e.store().balance().total, 6000.0);
}

#[test]
fn failed_account_snapshot_preserves_existing_state() {
    let (mut e, _, _, pending) = startup();
    for a in pending {
        if is_get(&a, "/api/v5/account/positions?instType=SWAP") {
            reply(
                &mut e,
                &a,
                r#"{"code":"50011","msg":"Rate limit","data":[]}"#,
                5.0,
            );
        } else {
            reply(&mut e, &a, &envelope(""), 5.0);
        }
    }
    assert!(!e.store().loaded().positions);
}

#[test]
fn stale_live_push_cannot_reopen_terminal_order() {
    let (mut e, private, _, pending) = startup();
    complete(&mut e, pending, ORDER);
    let canceled = ORDER.replace("\"live\"", "\"canceled\"");
    ws(
        &mut e,
        private,
        &format!(r#"{{"arg":{{"channel":"orders"}},"data":[{canceled}]}}"#),
    );
    ws(
        &mut e,
        private,
        &format!(r#"{{"arg":{{"channel":"orders"}},"data":[{ORDER}]}}"#),
    );
    assert_eq!(
        e.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
}

#[test]
fn empty_and_unknown_position_pushes_preserve_balance() {
    let (mut e, private, _, pending) = startup();
    complete(&mut e, pending, "");
    let before = e.store().balance().clone();
    ws(
        &mut e,
        private,
        r#"{"arg":{"channel":"positions"},"data":[]}"#,
    );
    let unknown = POSITION.replace("BTC-USDT-SWAP", "BTC-USD-SWAP");
    ws(
        &mut e,
        private,
        &format!(r#"{{"arg":{{"channel":"positions"}},"data":[{unknown}]}}"#),
    );
    assert_eq!(e.store().balance(), &before);
}

#[test]
fn rejected_login_closes_and_reconnects() {
    let (mut e, private, _, _) = startup();
    ws(
        &mut e,
        private,
        r#"{"event":"login","code":"60009","msg":"Login failed"}"#,
    );
    let out = actions(&mut e);
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::WsClose { socket } if *socket == private))
    );
    let timer = out
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(20_000.0, Event::TimerFired { timer });
    assert!(
        actions(&mut e)
            .iter()
            .any(|a| matches!(a, Action::WsConnect { url, .. } if url.ends_with("/private")))
    );
}

#[test]
fn malformed_known_position_is_a_failed_snapshot() {
    let (mut e, _, _, pending) = startup();
    for a in pending {
        if is_get(&a, "/api/v5/account/positions?instType=SWAP") {
            reply(
                &mut e,
                &a,
                &envelope(&POSITION.replace("\"pos\":\"5\"", "\"pos\":\"bad\"")),
                5.0,
            );
        } else if is_get(&a, "/api/v5/account/account-position-risk?instType=SWAP") {
            reply(
                &mut e,
                &a,
                &envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#),
                5.0,
            );
        }
    }
    assert!(!e.store().loaded().positions && !e.store().loaded().balance);
}

fn complete_fresh(e: &mut Engine<Okx>, mut requests: Vec<Action>, position: &str) {
    while let Some(a) = requests.pop() {
        let Action::Http(r) = &a else { continue };
        let body = if r.url.contains("account-position-risk") {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"6000"}]}"#)
        } else if r.url.contains("/positions?") {
            envelope(position)
        } else {
            envelope("")
        };
        reply(e, &a, &body, 30_000.0);
        requests.extend(
            actions(e)
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_))),
        );
    }
}

#[test]
fn authenticated_reopen_retires_pre_disconnect_reads_and_order_walk() {
    let (mut e, private, _, stale) = startup();
    e.handle(
        100.0,
        Event::WsClosed {
            socket: private,
            code: 1006,
        },
    );
    let out = actions(&mut e);
    let timer = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(2_000.0, Event::TimerFired { timer });
    let out = actions(&mut e);
    let socket = out
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
            _ => None,
        })
        .unwrap();
    e.handle(2_001.0, Event::WsOpened { socket });
    actions(&mut e);
    ws(&mut e, socket, r#"{"event":"login","code":"0","msg":""}"#);
    let fresh: Vec<_> = actions(&mut e)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    assert_eq!(fresh.len(), 3);
    complete_fresh(&mut e, fresh, "");
    assert!(e.store().loaded().orders && e.store().loaded().positions);
    assert!(e.store().orders().is_empty() && e.store().positions().is_empty());
    assert_eq!(e.store().balance().total, 6000.0);
    // The old page must not restart paging or replace the post-reconnect view.
    complete(&mut e, stale, ORDER);
    assert!(e.store().orders().is_empty() && e.store().positions().is_empty());
    assert_eq!(e.store().balance().total, 6000.0);
}

#[test]
fn settings_refresh_retires_position_join_without_discarding_inflight_orders() {
    use safe_cex_engine::Command;
    let (mut e, private, _, initial) = startup();
    complete(&mut e, initial, ORDER);
    // The SDK completes startup before commands; repeat authentication starts
    // another read sweep whose replies race the acknowledged settings write.
    ws(&mut e, private, r#"{"event":"login","code":"0"}"#);
    let stale = actions(&mut e)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    e.handle(
        5.0,
        Event::Command(Command::SetLeverage {
            command: 9,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
        }),
    );
    let out = actions(&mut e);
    let request = out.iter().find(|a| matches!(a, Action::Http(_))).unwrap();
    reply(
        &mut e,
        request,
        &envelope(r#"{"instId":"BTC-USDT-SWAP","lever":"20","mgnMode":"cross"}"#),
        6.0,
    );
    let fresh: Vec<_> = actions(&mut e)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    assert_eq!(fresh.len(), 2);
    assert!(
        !fresh
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("orders")))
    );
    // Retired position reads cannot overwrite the acknowledged leverage.
    complete(&mut e, stale, ORDER);
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    complete_fresh(
        &mut e,
        fresh,
        &POSITION.replace("\"lever\":\"10\"", "\"lever\":\"20\""),
    );
    assert_eq!(e.store().positions()[0].leverage, 20.0);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert_eq!(e.store().orders().len(), 1);
}

fn fill_count(frame: &[u8]) -> u32 {
    let mut at = 16;
    while at < frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            return u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap());
        }
        at += 8 + len;
    }
    0
}

#[test]
fn distinct_earlier_fill_survives_terminal_order_guard_and_deduplicates() {
    let (mut e, private, _, pending) = startup();
    complete(&mut e, pending, ORDER);
    let filled = ORDER.replace("\"live\"", "\"filled\"").replace(
        "\"accFillSz\":\"1\"",
        "\"accFillSz\":\"4\",\"fillSz\":\"2\",\"fillPx\":\"38000\",\"tradeId\":\"last\"",
    );
    let partial = ORDER.replace("\"live\"", "\"partially_filled\"").replace(
        "\"accFillSz\":\"1\"",
        "\"accFillSz\":\"2\",\"fillSz\":\"1\",\"fillPx\":\"37900\",\"tradeId\":\"earlier\"",
    );
    for row in [&filled, &filled, &partial, &partial] {
        ws(
            &mut e,
            private,
            &format!(r#"{{"arg":{{"channel":"orders"}},"data":[{row}]}}"#),
        );
    }
    assert_eq!(
        e.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Closed
    );
    assert_eq!(fill_count(e.take_frame(10.0).unwrap()), 2);
}

#[test]
fn invalid_execution_does_not_emit_or_consume_the_execution_identity() {
    let (mut e, private, _, pending) = startup();
    complete(&mut e, pending, ORDER);
    let valid = ORDER.replace("\"live\"", "\"filled\"").replace(
        "\"accFillSz\":\"1\"",
        "\"accFillSz\":\"4\",\"fillSz\":\"3\",\"fillPx\":\"38000\",\"tradeId\":\"987\"",
    );
    let invalid = valid.replace("\"fillPx\":\"38000\"", "\"fillPx\":\"bad\"");
    ws(
        &mut e,
        private,
        &format!(r#"{{"arg":{{"channel":"orders"}},"data":[{invalid}]}}"#),
    );
    assert_eq!(fill_count(e.take_frame(9.0).unwrap()), 0);
    ws(
        &mut e,
        private,
        &format!(r#"{{"arg":{{"channel":"orders"}},"data":[{valid}]}}"#),
    );
    assert_eq!(fill_count(e.take_frame(10.0).unwrap()), 1);
}

#[test]
fn malformed_risk_outer_rows_preserve_existing_atomic_snapshot() {
    for body in [
        envelope(""),
        envelope("{}"),
        envelope(r#"{"balData":null}"#),
        envelope(r#"{"balData":[]},{"balData":[]}"#),
        envelope(r#"{"balData":[{"ccy":"","eq":"5000"}]}"#),
    ] {
        let (mut e, _, _, pending) = startup();
        complete(&mut e, pending, ORDER);
        let balance = e.store().balance().clone();
        let positions = e.store().positions().to_vec();
        e.handle(
            6.0,
            Event::Command(safe_cex_engine::Command::SetLeverage {
                command: 901,
                symbol: "BTCUSDT".into(),
                leverage: 12.0,
            }),
        );
        let request = actions(&mut e)
            .into_iter()
            .find(|a| matches!(a, Action::Http(_)))
            .unwrap();
        reply(
            &mut e,
            &request,
            &envelope(r#"{"instId":"BTC-USDT-SWAP","lever":"12","mgnMode":"cross"}"#),
            7.0,
        );
        let requests = actions(&mut e);
        for a in requests
            .iter()
            .filter(|a| is_get(a, "/api/v5/account/positions?instType=SWAP"))
        {
            reply(
                &mut e,
                a,
                &envelope(&POSITION.replace("\"pos\":\"5\"", "\"pos\":\"6\"")),
                8.0,
            );
        }
        for a in requests
            .iter()
            .filter(|a| is_get(a, "/api/v5/account/account-position-risk?instType=SWAP"))
        {
            reply(&mut e, a, &body, 9.0);
        }
        assert_eq!(e.store().balance(), &balance);
        assert_eq!(e.store().positions(), positions);
        assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 12.0);
    }
}

#[test]
fn documented_empty_bal_data_is_a_valid_zero_balance() {
    let (mut e, _, _, pending) = startup();
    for a in pending {
        if is_get(&a, "/api/v5/account/positions?instType=SWAP") {
            reply(&mut e, &a, &envelope(""), 5.0);
        } else if is_get(&a, "/api/v5/account/account-position-risk?instType=SWAP") {
            reply(&mut e, &a, &envelope(r#"{"balData":[]}"#), 5.0);
        }
    }
    assert!(e.store().loaded().balance && e.store().loaded().positions);
    assert_eq!(e.store().balance().total, 0.0);
}
