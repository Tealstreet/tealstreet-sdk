mod support;
use safe_cex_engine::{Action, Event, HttpRequest, Response, SocketId};
use support::*;
fn regular(id: &str, price: u32, brackets: bool) -> String {
    format!(
        r#"{{"symbol":"BTCUSDT","orderId":"{id}","side":"SELL","orderType":"LIMIT","qty":"3","tradeQty":"1","price":"{price}","effect":"GTC"{}}}"#,
        if brackets {
            r#", "tpPrice":"80","slPrice":"110""#
        } else {
            ""
        }
    )
}
fn protection(id: &str, tp: u32, both: bool) -> String {
    format!(
        r#"{{"symbol":"BTCUSDT","id":"{id}","positionId":"position-1","tpPrice":"{tp}","tpQty":"0.01","tpOrderType":"MARKET"{}}}"#,
        if both {
            r#", "slPrice":"110","slQty":"0.01","slOrderType":"LIMIT","slOrderPrice":"123","slStopType":"MARK_PRICE""#
        } else {
            ""
        }
    )
}
fn body(rows: &[String], tpsl: bool) -> String {
    let rows = rows.join(",");
    if tpsl {
        format!(r#"{{"code":0,"data":[{rows}]}}"#)
    } else {
        format!(r#"{{"code":0,"data":{{"orderList":[{rows}]}}}}"#)
    }
}
fn reads(e: &mut TestEngine, socket: SocketId, now: f64) -> Vec<HttpRequest> {
    e.handle(now, Event::WsMessage { socket, text: r#"{"ch":"order","data":{"event":"SYNC","symbol":"BTCUSDT","orderId":"refresh-only"}}"# });
    let (timer, at) = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == now + 1000.0 => Some((timer, at_ms)),
            _ => None,
        })
        .next_back()
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    let reads = requests(e);
    assert_eq!(reads.len(), 2);
    reads
}
fn answer(e: &mut TestEngine, r: safe_cex_engine::RequestId, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn refresh(e: &mut TestEngine, socket: SocketId, now: f64, regular: &[String], tpsl: &[String]) {
    for r in reads(e, socket, now) {
        let tpsl_read = r.url.contains("/tpsl/");
        answer(
            e,
            r.id,
            now + 1001.0,
            &body(if tpsl_read { tpsl } else { regular }, tpsl_read),
        );
    }
}
fn regular_seed() -> Vec<String> {
    vec![
        regular("A", 100, true),
        regular("B", 200, false),
        regular("C", 300, false),
    ]
}
fn protection_seed() -> Vec<String> {
    vec![
        protection("P", 90, true),
        protection("Q", 70, false),
        protection("R", 60, false),
    ]
}
fn seeded() -> (TestEngine, SocketId) {
    let (mut e, socket) = startup();
    refresh(&mut e, socket, 1000.0, &regular_seed(), &protection_seed());
    e.drain_actions().for_each(drop);
    assert!(e.store().order("A__tp").is_some());
    assert!(e.store().order("P__position-1__sl").is_some());
    (e, socket)
}
#[test]
fn identified_regular_rejection_keeps_parent_and_brackets_while_healthy_peers_advance() {
    let (mut e, socket) = seeded();
    let bad = regular("A", 999, true).replace("\"qty\":\"3\"", "\"qty\":{}");
    refresh(
        &mut e,
        socket,
        3000.0,
        &[bad, regular("B", 250, false)],
        &protection_seed(),
    );
    assert_eq!(e.store().order("A").unwrap().price, 100.0);
    assert!(e.store().order("A__tp").is_some());
    assert!(e.store().order("A__sl").is_some());
    assert_eq!(e.store().order("B").unwrap().price, 250.0);
    assert!(e.store().order("C").is_none());
}
#[test]
fn identified_tpsl_rejection_keeps_both_native_siblings_while_other_raws_advance() {
    let (mut e, socket) = seeded();
    let bad = protection("P", 999, true).replace("\"tpQty\":\"0.01\"", "\"tpQty\":{}");
    refresh(
        &mut e,
        socket,
        3000.0,
        &regular_seed(),
        &[bad, protection("Q", 75, false)],
    );
    assert_eq!(e.store().order("P__position-1__tp").unwrap().price, 90.0);
    assert_eq!(e.store().order("P__position-1__sl").unwrap().price, 110.0);
    assert_eq!(e.store().order("Q").unwrap().price, 75.0);
    assert!(e.store().order("R").is_none());
    assert!(e.store().order("A__tp").is_some());
}
#[test]
fn unknown_regular_neighbor_holds_absence_but_cannot_resurrect_omitted_brackets() {
    let (mut e, socket) = seeded();
    refresh(
        &mut e,
        socket,
        3000.0,
        &["{}".into(), regular("A", 125, false)],
        &protection_seed(),
    );
    assert_eq!(e.store().order("A").unwrap().price, 125.0);
    assert!(e.store().order("A__tp").is_none());
    assert!(e.store().order("A__sl").is_none());
    assert!(e.store().order("B").is_some());
    assert!(e.store().order("C").is_some());
    assert!(e.store().order("P__position-1__sl").is_some());
}

#[test]
fn unknown_tpsl_neighbor_does_not_preserve_old_virtual_siblings_of_an_admitted_raw() {
    let (mut e, socket) = seeded();
    refresh(
        &mut e,
        socket,
        3000.0,
        &regular_seed(),
        &["{}".into(), protection("P", 95, false)],
    );
    assert!(e.store().order("P__position-1__tp").is_none());
    assert!(e.store().order("P__position-1__sl").is_none());
    assert_eq!(e.store().order("P").unwrap().price, 95.0);
    assert!(e.store().order("Q").is_some());
    assert!(e.store().order("R").is_some());
    e.drain_actions().for_each(drop);
    refresh(
        &mut e,
        socket,
        5000.0,
        &regular_seed(),
        &[
            "{}".into(),
            r#"{"symbol":"BTCUSDT","id":"P","positionId":"position-1"}"#.into(),
        ],
    );
    assert!(e.store().order("P").is_none());
    assert!(e.store().order("Q").is_some());
}

#[test]
fn known_raw_rejection_quarantines_duplicates_in_both_endpoint_scopes_and_orders() {
    for tpsl in [false, true] {
        for bad_first in [false, true] {
            let (mut e, socket) = seeded();
            let (bad, good, peer) = if tpsl {
                (
                    protection("P", 999, true).replace("\"tpQty\":\"0.01\"", "\"tpQty\":{}"),
                    protection("P", 999, false),
                    protection("Q", 75, false),
                )
            } else {
                (
                    regular("A", 999, true).replace("\"qty\":\"3\"", "\"qty\":{}"),
                    regular("A", 999, false),
                    regular("B", 250, false),
                )
            };
            let mut rows = if bad_first {
                vec![bad, good]
            } else {
                vec![good, bad]
            };
            rows.push(peer);
            let regular_rows = regular_seed();
            let protection_rows = protection_seed();
            refresh(
                &mut e,
                socket,
                3000.0,
                if tpsl { &regular_rows } else { &rows },
                if tpsl { &rows } else { &protection_rows },
            );
            if tpsl {
                assert_eq!(e.store().order("P__position-1__tp").unwrap().price, 90.0);
                assert_eq!(e.store().order("Q").unwrap().price, 75.0);
            } else {
                assert_eq!(e.store().order("A").unwrap().price, 100.0);
                assert!(e.store().order("A__sl").is_some());
                assert_eq!(e.store().order("B").unwrap().price, 250.0);
            }
        }
    }
}

#[test]
fn numeric_raw_identity_is_retained_despite_an_unrelated_bad_numeric_field() {
    let (mut e, socket) = seeded();
    let numeric = regular("123", 100, true).replace("\"orderId\":\"123\"", "\"id\":123");
    refresh(
        &mut e,
        socket,
        3000.0,
        &[numeric.clone(), regular("B", 200, false)],
        &protection_seed(),
    );
    e.drain_actions().for_each(drop);
    refresh(
        &mut e,
        socket,
        5000.0,
        &[
            numeric.replace("\"qty\":\"3\"", "\"qty\":{}"),
            regular("B", 250, false),
        ],
        &protection_seed(),
    );
    assert_eq!(e.store().order("123").unwrap().price, 100.0);
    assert!(e.store().order("123__sl").is_some());
    assert_eq!(e.store().order("B").unwrap().price, 250.0);
}

#[test]
fn retained_tpsl_family_keeps_native_cancel_and_amend_execution_metadata() {
    use safe_cex_engine::Command;
    for amend in [false, true] {
        let (mut e, socket) = seeded();
        let bad = protection("P", 999, true).replace("\"tpQty\":\"0.01\"", "\"tpQty\":{}");
        refresh(
            &mut e,
            socket,
            3000.0,
            &regular_seed(),
            &[bad, protection("Q", 75, false)],
        );
        e.drain_actions().for_each(drop);
        let command = if amend {
            Command::AmendOrder {
                command: 91,
                id: "P__position-1__sl".into(),
                price: Some(140.0),
                amount: None,
            }
        } else {
            Command::CancelOrders {
                command: 91,
                ids: vec!["P__position-1__tp".into()],
            }
        };
        e.handle(4200.0, Event::Command(command));
        let r = requests(&mut e).remove(0);
        let bytes = r.body.as_deref().unwrap();
        assert!(bytes.contains(r#""orderId":"P""#), "{bytes}");
        if amend {
            assert!(r.url.ends_with("/tpsl/modify_order"));
            for field in [
                r#""slPrice":"140""#,
                r#""tpPrice":"90""#,
                r#""slOrderType":"LIMIT""#,
                r#""slOrderPrice":"123""#,
                r#""tpQty":"0.01""#,
                r#""slQty":"0.01""#,
            ] {
                assert!(bytes.contains(field), "{bytes} missing{field}");
            }
        } else {
            assert!(r.url.ends_with("/tpsl/cancel_order"));
        }
    }
}

#[test]
fn rejection_coverage_overflow_preserves_every_cached_raw_family() {
    for tpsl in [false, true] {
        let (mut e, socket) = seeded();
        let previous = e.store().orders().to_vec();
        let mut rows: Vec<_> = (0..4097)
            .map(|i| format!(r#"{{"symbol":"BTCUSDT","orderId":"BAD{i}","qty":{{}}}}"#))
            .collect();
        rows.push(if tpsl {
            protection("Q", 999, false)
        } else {
            regular("B", 999, false)
        });
        let regular_rows = regular_seed();
        let protection_rows = protection_seed();
        refresh(
            &mut e,
            socket,
            3000.0,
            if tpsl { &regular_rows } else { &rows },
            if tpsl { &rows } else { &protection_rows },
        );
        // The unaffected sibling scope receives a clean snapshot; compare relevant prices/groups.
        assert_eq!(
            e.store().order("A").unwrap().price,
            previous.iter().find(|o| o.id == "A").unwrap().price
        );
        assert_eq!(e.store().order("B").unwrap().price, 200.0);
        assert_eq!(e.store().order("Q").unwrap().price, 70.0);
        assert!(e.store().order("A__sl").is_some());
        assert!(e.store().order("P__position-1__sl").is_some());
    }
}

fn initial_halves() -> (
    TestEngine,
    Vec<HttpRequest>,
    Vec<(safe_cex_engine::TimerId, f64)>,
) {
    let mut e = support::engine(support::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..support::Options::default()
    });
    e.start(0.0);
    let mut halves = Vec::new();
    let mut repairs = Vec::new();
    loop {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
        if actions.is_empty() {
            break;
        }
        for a in actions {
            match a {
                Action::WsConnect { socket, .. } => e.handle(0.0, Event::WsOpened { socket }),
                Action::Http(r) if r.url.contains("get_pending_orders") => halves.push(r),
                Action::Http(r) => {
                    let bytes = if r.url.ends_with("/trading_pairs") {
                        PAIRS
                    } else if r.url.ends_with("/tickers") {
                        TICKERS
                    } else if r.url.contains("get_pending_positions") {
                        POSITIONS
                    } else {
                        ACCOUNT
                    };
                    answer(&mut e, r.id, 0.0, bytes);
                }
                Action::SetTimer { timer, at_ms } if at_ms == 1000.0 => {
                    repairs.push((timer, at_ms))
                }
                Action::ClearTimer { timer } => repairs.retain(|(pending, _)| *pending != timer),
                _ => {}
            }
        }
    }
    assert_eq!(halves.len(), 2);
    assert!(!e.store().loaded().orders);
    (e, halves, repairs)
}
#[test]
fn partial_initial_halves_never_load_and_recovery_requires_both_current_serials() {
    for bad_tpsl in [false, true] {
        for bad_first in [false, true] {
            let (mut e, mut halves, mut repairs) = initial_halves();
            halves.sort_by_key(|r| r.url.contains("/tpsl/") != bad_tpsl);
            if !bad_first {
                halves.reverse();
            }
            let mut retry_at = 0.0;
            for (index, r) in halves.into_iter().enumerate() {
                let tpsl = r.url.contains("/tpsl/");
                let now = 100.0 + index as f64;
                let rows = if tpsl == bad_tpsl {
                    vec!["{}".into()]
                } else if tpsl {
                    protection_seed()
                } else {
                    regular_seed()
                };
                answer(&mut e, r.id, now, &body(&rows, tpsl));
                if tpsl == bad_tpsl {
                    retry_at = now + 1000.0;
                }
                assert!(!e.store().loaded().orders);
            }
            // Keep the earlier bootstrap repair if it was already armed; a bad
            // half must not postpone that deadline. Replay timer cancellation.
            for action in e.drain_actions() {
                match action {
                    Action::SetTimer { timer, at_ms } if at_ms >= 1000.0 && at_ms <= retry_at => {
                        repairs.push((timer, at_ms))
                    }
                    Action::ClearTimer { timer } => {
                        repairs.retain(|(pending, _)| *pending != timer)
                    }
                    _ => {}
                }
            }
            let (timer, at) = repairs
                .into_iter()
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("one coalesced initial repair");
            retry_at = at;
            e.handle(retry_at, Event::TimerFired { timer });
            let retry = requests(&mut e);
            assert_eq!(retry.len(), 2);
            for (index, r) in retry.into_iter().enumerate() {
                let tpsl = r.url.contains("/tpsl/");
                answer(&mut e, r.id, retry_at + 1.0, &body(&[], tpsl));
                if index == 0 {
                    assert!(
                        !e.store().loaded().orders,
                        "old staged serial cannot stand in for the second current half"
                    );
                }
            }
            assert!(e.store().loaded().orders);
            assert!(e.store().orders().is_empty());
        }
    }
}
#[test]
fn partial_owned_order_refresh_cannot_advance_a_write_preflight() {
    use safe_cex_engine::{Command, Method, Notice};
    let (mut e, _) = seeded();
    let order=serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"buy","price":85000,"amount":0,"tpTriggerType":"mark_price"}"#).unwrap();
    e.handle(
        3000.0,
        Event::Command(Command::PlaceOrders {
            command: 92,
            orders: vec![order],
        }),
    );
    let positions = requests(&mut e).remove(0);
    assert!(positions.url.contains("get_pending_positions"));
    answer(&mut e, positions.id, 3100.0, POSITIONS);
    let reads = requests(&mut e);
    let orders = reads
        .into_iter()
        .find(|r| r.url.contains("/trade/get_pending_orders"))
        .unwrap();
    let bad = regular("A", 999, true).replace("\"qty\":\"3\"", "\"qty\":{}");
    answer(
        &mut e,
        orders.id,
        3200.0,
        &body(&[bad, regular("B", 250, false)], false),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.method==Method::Post))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 92, .. })))
    );
    assert_eq!(e.store().order("B").unwrap().price, 250.0);
}
#[test]
fn failed_envelopes_never_publish_partially_parsed_healthy_peers() {
    for kind in ["native", "http", "truncated"] {
        let (mut e, socket) = seeded();
        let r = reads(&mut e, socket, 3000.0)
            .into_iter()
            .find(|r| !r.url.contains("/tpsl/"))
            .unwrap();
        let bytes = body(&[regular("B", 999, false)], false);
        let bytes = if kind == "native" {
            bytes.replace("\"code\":0", "\"code\":10001")
        } else if kind == "truncated" {
            bytes[..bytes.len() - 2].into()
        } else {
            bytes
        };
        e.handle(
            4100.0,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: if kind == "http" { 503 } else { 200 },
                    headers: "",
                    body: &bytes,
                },
            },
        );
        assert_eq!(e.store().order("B").unwrap().price, 200.0);
        assert!(e.store().order("A__sl").is_some());
    }
}

#[test]
fn unidentified_row_holds_absence_only_in_its_own_endpoint_scope() {
    for tpsl in [false, true] {
        let (mut e, socket) = seeded();
        let unknown = vec!["{}".into()];
        let empty = vec![];
        refresh(
            &mut e,
            socket,
            3000.0,
            if tpsl { &empty } else { &unknown },
            if tpsl { &unknown } else { &empty },
        );
        assert_eq!(e.store().order("A").is_some(), !tpsl);
        assert_eq!(e.store().order("A__sl").is_some(), !tpsl);
        assert_eq!(e.store().order("P__position-1__sl").is_some(), tpsl);
        assert_eq!(e.store().order("Q").is_some(), tpsl);
    }
}
fn fill_quantities(e: &mut TestEngine, now: f64) -> Vec<f64> {
    let Some(frame) = e.take_market_frame(now) else {
        return vec![];
    };
    let Some((_, payload)) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::FILLS)
    else {
        return vec![];
    };
    let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let values = (4 + count * 12).next_multiple_of(8);
    (0..count)
        .map(|i| {
            f64::from_le_bytes(
                payload[values + i * 16 + 8..values + i * 16 + 16]
                    .try_into()
                    .unwrap(),
            )
        })
        .collect()
}
#[test]
fn held_bad_raw_does_not_consume_fill_baseline_while_healthy_peer_baseline_advances() {
    let (mut e, socket) = seeded();
    e.take_market_frame(2101.0);
    let bad = regular("A", 999, true).replace("\"qty\":\"3\"", "\"qty\":{}");
    let healthy = regular("B", 250, false).replace("\"tradeQty\":\"1\"", "\"tradeQty\":\"2\"");
    refresh(&mut e, socket, 3000.0, &[bad, healthy], &protection_seed());
    assert!(
        fill_quantities(&mut e, 4101.0).is_empty(),
        "REST repair does not emit executions"
    );
    for (now, text) in [
        (
            4200.0,
            r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"A","dealAmount":"2","averagePrice":"150"}}"#,
        ),
        (
            4300.0,
            r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"B","dealAmount":"3","averagePrice":"250"}}"#,
        ),
    ] {
        e.handle(now, Event::WsMessage { socket, text });
        assert_eq!(fill_quantities(&mut e, now), [1.0]);
    }
}
