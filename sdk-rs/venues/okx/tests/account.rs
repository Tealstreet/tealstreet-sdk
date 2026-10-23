#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::{Action, Engine, Event, Level, Notice};
use safe_cex_venue_okx::Okx;

const POSITION: &str = r#"{"instId":"BTC-USDT-SWAP","pos":"5","posSide":"net","avgPx":"39000","lever":"10","upl":"50","liqPx":"30000","mgnMode":"cross","imr":"200","mmr":"20"}"#;
const ORDER: &str = r#"{"instId":"BTC-USDT-SWAP","ordId":"123","ordType":"limit","state":"live","side":"buy","sz":"4","accFillSz":"1","px":"38000","lever":"10","reduceOnly":"false","clOrdId":"caller123"}"#;

#[test]
fn periodic_policy_does_not_suppress_local_valuation_or_create_network_polls() {
    let mut e = crate::support::engine(Options {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Default::default()
    });
    e.start(0.);
    let market = actions(&mut e).remove(0);
    reply(&mut e, &market, &envelope(MARKET), 1.);
    // Account config is requested alongside the ticker seed once markets load.
    let mut initial = actions(&mut e);
    let ticker = initial.remove(0);
    reply(&mut e, &ticker, &envelope(TICKER), 2.);
    initial.extend(actions(&mut e));
    let config = initial
        .into_iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut e,
        &config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.,
    );
    let pending = actions(&mut e);
    let timer = pending
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 2003. => Some(*timer),
            _ => None,
        })
        .unwrap();
    complete(&mut e, pending, ORDER);
    let expected = e.store().balance().clone();
    for scale in [f64::INFINITY, 15., 0.25] {
        e.handle(100., Event::PollScaleChanged { scale });
        assert!(actions(&mut e).is_empty());
    }
    e.handle(
        100.,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    e.store_mut().set_balance(Default::default());
    e.handle(2003., Event::TimerFired { timer });
    assert_eq!(e.store().balance(), &expected);
    let after = actions(&mut e);
    assert!(!after.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        after
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 4003.))
    );
}

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
    let mut e = crate::support::engine(opts);
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
        let subscriptions = handoff_actions(&mut e, 4.);
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

fn handoff_actions(engine: &mut Engine<Okx>, now: f64) -> Vec<Action> {
    let mut output = actions(engine);
    let wakeups: Vec<_> = output
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == now => Some(*timer),
            _ => None,
        })
        .collect();
    for timer in wakeups {
        engine.handle(now, Event::TimerFired { timer });
        output.extend(actions(engine));
    }
    output
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
fn malformed_known_position_row_without_a_stored_copy_commits_flat() {
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
    assert!(e.store().loaded().positions && e.store().loaded().balance);
    assert!(e.store().positions().is_empty());
    let errors: Vec<_> = actions(&mut e)
        .into_iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event)) if event.level == Level::Error => {
                Some(event.message)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors, ["OKX skipped unmappable BTCUSDT position row"]);
}

/// Starts a second account sweep after `complete` and answers it with these bodies.
fn resweep(
    e: &mut Engine<Okx>,
    private: safe_cex_engine::SocketId,
    risk: &str,
    position: &str,
    orders: &str,
    now: f64,
) -> safe_cex_engine::SocketId {
    // A duplicate ACK on an already authenticated epoch must not resync.
    // Obtain a real replacement epoch to exercise the financial reducer.
    e.handle(
        now,
        Event::WsClosed {
            socket: private,
            code: 1006,
        },
    );
    let out = actions(e);
    let (timer, at) = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if *at_ms >= now + 750. && *at_ms <= now + 30_000. =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("private reconnect backoff");
    e.handle(at, Event::TimerFired { timer });
    let private = actions(e)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(socket),
            _ => None,
        })
        .expect("replacement private epoch");
    e.handle(at, Event::WsOpened { socket: private });
    actions(e);
    e.handle(
        at,
        Event::WsMessage {
            socket: private,
            text: r#"{"event":"login","code":"0"}"#,
        },
    );
    let mut requests: Vec<_> = handoff_actions(e, at)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    while let Some(a) = requests.pop() {
        let Action::Http(r) = &a else { continue };
        let body = if r.url.contains("account-position-risk") {
            envelope(risk)
        } else if r.url.contains("/positions?") {
            envelope(position)
        } else if r.url.contains("orders-pending") {
            envelope(orders)
        } else {
            envelope("")
        };
        reply(e, &a, &body, at + 1.0);
        requests.extend(
            actions(e)
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_))),
        );
    }
    private
}

#[test]
fn bad_position_row_keeps_its_stored_copy_while_the_balance_commits() {
    let (mut e, private, _, pending) = startup();
    complete(&mut e, pending, ORDER);
    let bad = POSITION.replace("\"pos\":\"5\"", "\"pos\":\"bad\"");
    resweep(
        &mut e,
        private,
        r#"{"balData":[{"ccy":"USDT","eq":"6000"}]}"#,
        &bad,
        ORDER,
        10.0,
    );
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].contracts, 0.05);
    assert_eq!(e.store().balance().total, 5950.0);
}

#[test]
fn bad_balance_currency_row_keeps_its_equity_while_positions_commit() {
    for risk in [
        r#"{"balData":[{"ccy":"USDT","eq":"bad"}]}"#,
        r#"{"balData":[{"ccy":"","eq":"6000"}]}"#,
    ] {
        let (mut e, private, _, pending) = startup();
        complete(&mut e, pending, ORDER);
        let grown = POSITION.replace("\"pos\":\"5\"", "\"pos\":\"6\"");
        resweep(&mut e, private, risk, &grown, ORDER, 10.0);
        assert_eq!(e.store().positions()[0].contracts, 0.06, "{risk}");
        assert_eq!(e.store().balance().total, 4950.0, "{risk}");
    }
}

#[test]
fn bad_order_row_keeps_its_stored_copy_and_new_orders_commit() {
    let fresh = ORDER
        .replace("\"ordId\":\"123\"", "\"ordId\":\"456\"")
        .replace("caller123", "caller456");
    for bad in [
        ORDER.replace("\"side\":\"buy\"", "\"side\":\"up\""),
        r#"{"instId":"BTC-USDT-SWAP","ordId":"123","sz":4}"#.to_string(),
        r#"{"ordId":123}"#.to_string(),
    ] {
        let (mut e, private, _, pending) = startup();
        complete(&mut e, pending, ORDER);
        let balance = r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#;
        resweep(
            &mut e,
            private,
            balance,
            POSITION,
            &format!("{bad},{fresh}"),
            10.0,
        );
        let mut ids: Vec<_> = e.store().orders().iter().map(|o| o.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["123", "456"], "{bad}");
        assert_eq!(e.store().order("123").unwrap().amount, 4.0);
    }
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
    e.handle(
        2_002.,
        Event::WsMessage {
            socket,
            text: r#"{"event":"login","code":"0","msg":""}"#,
        },
    );
    let fresh: Vec<_> = handoff_actions(&mut e, 2_002.)
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
            side: None,
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
                side: None,
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

#[test]
fn filtered_market_rows_are_skipped_and_the_rest_of_the_snapshot_commits() {
    let (mut e, _, _, mut pending) = startup();
    let filtered_position = POSITION.replace("BTC-USDT-SWAP", "BTC-USD-250926");
    let filtered_order = ORDER
        .replace("BTC-USDT-SWAP", "BTC-USD-250926")
        .replace("\"123\"", "\"456\"");
    let mut logs = vec![];
    while let Some(action) = pending.pop() {
        match &action {
            Action::Http(r) => {
                let body = if r.url.contains("account-position-risk") {
                    envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
                } else if r.url.contains("/positions?") {
                    envelope(&format!("{filtered_position},{POSITION}"))
                } else if r.url.contains("orders-pending") {
                    envelope(&format!("{filtered_order},{ORDER}"))
                } else {
                    envelope("")
                };
                reply(&mut e, &action, &body, 5.0);
                pending.extend(actions(&mut e));
            }
            Action::Notice(safe_cex_engine::Notice::Event(event)) => {
                logs.push(event.message.clone())
            }
            _ => {}
        }
    }
    let loaded = e.store().loaded();
    assert!(loaded.balance && loaded.positions && loaded.orders);
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].symbol, "BTCUSDT");
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(e.store().orders()[0].id, "123");
    assert_eq!(e.store().balance().total, 4950.0);
    assert!(
        logs.iter()
            .any(|log| log.contains("BTC-USD-250926 is not a listed market"))
    );
    assert!(!logs.iter().any(|log| log.contains("preserving")));
}

#[test]
fn partially_effective_algo_remains_visible_and_accepts_later_live_updates() {
    let (mut e, _, business, pending) = startup();
    complete(&mut e, pending, "");
    let algo = r#"{"instId":"BTC-USDT-SWAP","algoId":"algo123","ordType":"conditional","state":"partially_effective","side":"sell","sz":"4","accFillSz":"0","slTriggerPx":"36000","slOrdPx":"-1","tpTriggerPx":"42000","tpOrdPx":"-1","reduceOnly":"true"}"#;
    ws(
        &mut e,
        business,
        &format!(r#"{{"arg":{{"channel":"orders-algo"}},"data":[{algo}]}}"#),
    );
    assert!(e.store().order("algo123_sl").is_some());
    assert!(e.store().order("algo123_tp").is_some());
    ws(
        &mut e,
        business,
        &format!(
            r#"{{"arg":{{"channel":"orders-algo"}},"data":[{}]}}"#,
            algo.replace("partially_effective", "live")
                .replace("42000", "43000")
        ),
    );
    assert_eq!(e.store().order("algo123_tp").unwrap().price, 43000.0);
}

#[test]
fn undecodable_instrument_row_still_reports_its_exposure() {
    for bad in [
        r#"{"instId":"ETH-USDT-SWAP","instType":"SWAP","ctType":"linear","ctVal":1}"#,
        r#"{"instId":"ETH-USDT-SWAP","instType":"SWAP","ctType":null,"ctVal":"1"}"#,
    ] {
        reports_exposure_on_malformed_instrument(bad);
    }
}

fn reports_exposure_on_malformed_instrument(bad: &str) {
    let mut e = crate::support::engine(Options {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Options::default()
    });
    e.start(0.0);
    let a = actions(&mut e).remove(0);
    reply(&mut e, &a, &envelope(&format!("{MARKET},{bad}")), 1.0);
    // Private startup begins once markets load, alongside the ticker read.
    let out = actions(&mut e);
    let tickers = out
        .iter()
        .find(|a| is_get(a, "/api/v5/market/tickers?instType=SWAP"))
        .unwrap();
    let config = out
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap()
        .clone();
    reply(&mut e, tickers, &envelope(TICKER), 2.0);
    reply(
        &mut e,
        &config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.0,
    );
    let joined = actions(&mut e);
    let positions = joined
        .iter()
        .find(|a| is_get(a, "/api/v5/account/positions?instType=SWAP"))
        .unwrap();
    let risk = joined
        .iter()
        .find(|a| is_get(a, "/api/v5/account/account-position-risk?instType=SWAP"))
        .unwrap();
    reply(
        &mut e,
        positions,
        &envelope(&format!(
            "{},{POSITION}",
            POSITION.replace("BTC-USDT-SWAP", "ETH-USDT-SWAP")
        )),
        4.0,
    );
    let out = actions(&mut e);
    assert!(out.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::Event(event))
            if event.message.contains("ETH-USDT-SWAP has open exposure")
    )));
    reply(
        &mut e,
        risk,
        &envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#),
        5.0,
    );
    // Exposure on a malformed catalogue row holds the whole positions/balance join.
    assert!(!e.store().loaded().positions);
    assert!(!e.store().loaded().balance);
    assert!(e.store().positions().is_empty());
}

#[test]
fn partial_catalogue_is_reread_without_restarting_the_account() {
    let eth = MARKET
        .replace("BTC-USDT-SWAP", "ETH-USDT-SWAP")
        .replace("\"BTC\"", "\"ETH\"");
    let bad = eth.replace("\"lotSz\":\"0.01\"", "\"lotSz\":\"0\"");
    let mut e = crate::support::engine(Options {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Options::default()
    });
    e.start(0.0);
    let a = actions(&mut e).remove(0);
    reply(&mut e, &a, &envelope(&format!("{MARKET},{bad}")), 1.0);
    assert!(e.store().loaded().markets);
    let out = actions(&mut e);
    assert_eq!(
        out.iter()
            .filter(|a| is_get(a, "/api/v5/account/config"))
            .count(),
        1
    );
    let retry = retry_timer(&out, 1.0, 1_000.0).expect("partial catalogue schedules a re-read");
    e.handle(1001.0, Event::TimerFired { timer: retry });
    let reread = actions(&mut e)
        .into_iter()
        .find(|a| is_get(a, "/api/v5/public/instruments?instType=SWAP"))
        .unwrap();
    reply(
        &mut e,
        &reread,
        &envelope(&format!("{MARKET},{eth}")),
        1002.0,
    );
    assert!(e.store().market("ETHUSDT").is_some());
    let out = actions(&mut e);
    assert!(!out.iter().any(|a| is_get(a, "/api/v5/account/config")));
    // The complete re-read clears the retry instead of scheduling another.
    assert!(retry_timer(&out, 1002.0, 2_000.0).is_none());
}

#[test]
fn rejected_currency_row_holds_the_usd_balance_instead_of_double_counting() {
    let (mut e, private, _, pending) = startup();
    let mut pending = pending;
    while let Some(a) = pending.pop() {
        let Action::Http(r) = &a else { continue };
        let body = if r.url.contains("account-position-risk") {
            envelope(r#"{"balData":[{"ccy":"BTC","eq":"1"},{"ccy":"USDT","eq":"0"}]}"#)
        } else {
            envelope("")
        };
        reply(&mut e, &a, &body, 5.0);
        pending.extend(
            actions(&mut e)
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_))),
        );
    }
    let before = e.store().balance().clone();
    assert_eq!(before.total, 40_000.0);
    // The BTC was sold for USDT; the BTC row now fails to read.
    let private = resweep(
        &mut e,
        private,
        r#"{"balData":[{"ccy":"BTC","eq":"bad"},{"ccy":"USDT","eq":"40000"}]}"#,
        "",
        "",
        10.0,
    );
    assert_eq!(*e.store().balance(), before);
    let mut coins: Vec<_> = e
        .store()
        .currency_balances()
        .unwrap_or_default()
        .iter()
        .map(|c| (c.currency.clone(), c.balance.total))
        .collect();
    coins.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        coins,
        [("BTC".to_string(), 1.0), ("USDT".to_string(), 40_000.0)]
    );
    resweep(
        &mut e,
        private,
        r#"{"balData":[{"ccy":"BTC","eq":"0"},{"ccy":"USDT","eq":"40000"}]}"#,
        "",
        "",
        5000.0,
    );
    assert_eq!(e.store().balance().total, 40_000.0);
}

#[test]
fn undecodable_order_row_on_a_malformed_market_holds_the_order_snapshot() {
    let bad_market = r#"{"instId":"ETH-USDT-SWAP","instType":"SWAP","ctType":null,"ctVal":"1"}"#;
    for row in [
        r#"{"instId":"ETH-USDT-SWAP","ordId":"9","sz":4}"#,
        r#"{"instId":"ETH-USDT-SWAP","ordId":9,"state":null}"#,
    ] {
        let mut e = crate::support::engine(Options {
            key: Some("test-key".into()),
            secret: Some("test-secret".into()),
            passphrase: Some("test-passphrase".into()),
            ..Options::default()
        });
        e.start(0.0);
        let a = actions(&mut e).remove(0);
        reply(
            &mut e,
            &a,
            &envelope(&format!("{MARKET},{bad_market}")),
            1.0,
        );
        let out = actions(&mut e);
        let tickers = out
            .iter()
            .find(|a| is_get(a, "/api/v5/market/tickers?instType=SWAP"))
            .unwrap();
        reply(&mut e, tickers, &envelope(TICKER), 2.0);
        let config = out
            .iter()
            .find(|a| is_get(a, "/api/v5/account/config"))
            .unwrap();
        reply(
            &mut e,
            config,
            &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
            3.0,
        );
        let mut pending = actions(&mut e);
        let mut logs = vec![];
        while let Some(a) = pending.pop() {
            match &a {
                Action::Http(r) => {
                    let body = if r.url.contains("account-position-risk") {
                        envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
                    } else if r.url.contains("orders-pending") {
                        envelope(row)
                    } else {
                        envelope("")
                    };
                    reply(&mut e, &a, &body, 4.0);
                    pending.extend(actions(&mut e));
                }
                Action::Notice(Notice::Event(event)) => logs.push(event.message.clone()),
                _ => {}
            }
        }
        assert!(e.store().loaded().positions, "{row}");
        assert!(!e.store().loaded().orders, "{row}");
        assert!(
            logs.iter()
                .any(|l| l.contains("ETH-USDT-SWAP has open exposure")),
            "{row}: {logs:?}"
        );
    }
}

#[test]
fn undecodable_out_of_scope_order_row_fails_the_order_read() {
    let (mut e, _, _, pending) = startup();
    complete(
        &mut e,
        pending,
        r#"{"instId":"BTC-USDT-SWAP","instType":"FUTURES","ordId":"9","sz":4}"#,
    );
    assert!(e.store().loaded().positions);
    assert!(!e.store().loaded().orders);
}

#[test]
fn undecodable_out_of_scope_position_row_fails_the_position_read() {
    let (mut e, _, _, pending) = startup();
    for a in pending {
        if is_get(&a, "/api/v5/account/positions?instType=SWAP") {
            let row = r#"{"instId":"BTC-USDT-SWAP","instType":"FUTURES","pos":5}"#;
            reply(&mut e, &a, &envelope(row), 5.0);
        } else if is_get(&a, "/api/v5/account/account-position-risk?instType=SWAP") {
            let risk = r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#;
            reply(&mut e, &a, &envelope(risk), 5.0);
        }
    }
    assert!(!e.store().loaded().positions);
}

#[test]
fn market_maker_and_liquidity_program_orders_keep_their_ord_id() {
    for kind in ["mmp", "mmp_and_post_only", "elp"] {
        let (mut e, _, _, pending) = startup();
        complete(
            &mut e,
            pending,
            &ORDER.replace(r#""ordType":"limit""#, &format!(r#""ordType":"{kind}""#)),
        );
        let o = &e.store().orders()[0];
        assert_eq!(o.id, "123", "{kind}");
        assert_eq!(o.filled, 0.01, "{kind}");
        assert_eq!(o.client_order_id.as_deref(), Some("caller123"), "{kind}");
    }
}
