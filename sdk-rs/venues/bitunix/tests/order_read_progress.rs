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
fn ws(e: &mut TestEngine, socket: SocketId, at: f64, topic: &str, row: &str) {
    let text = format!(r#"{{"ch":"{topic}","data":{row}}}"#);
    e.handle(
        at,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
}
fn finish(e: &mut TestEngine, pending: Vec<HttpRequest>, regular: &[String], tpsl: &[String]) {
    for r in pending {
        let protection = r.url.contains("/tpsl/");
        answer(
            e,
            r.id,
            4500.0,
            &body(if protection { tpsl } else { regular }, protection),
        );
    }
}
#[test]
fn newer_ws_families_survive_stale_rest_while_healthy_peers_advance() {
    for tpsl in [false, true] {
        for omitted in [false, true] {
            let (mut e, socket) = seeded();
            let pending = reads(&mut e, socket, 3000.0);
            if tpsl {
                ws(
                    &mut e,
                    socket,
                    4100.0,
                    "tpsl",
                    r#"{"event":"UPDATE","symbol":"BTCUSDT","orderId":"P","tpPrice":"95","tpQty":"0.02","slOrderType":"LIMIT","slOrderPrice":"321"}"#,
                );
            } else {
                ws(
                    &mut e,
                    socket,
                    4100.0,
                    "order",
                    r#"{"event":"UPDATE","symbol":"BTCUSDT","orderId":"A","price":"111","qty":"4","dealAmount":"1"}"#,
                );
            }
            let regular = if omitted {
                vec![regular("B", 250, false)]
            } else {
                vec![regular("A", 100, true), regular("B", 250, false)]
            };
            let protection = if omitted {
                vec![protection("Q", 75, false)]
            } else {
                vec![protection("P", 90, true), protection("Q", 75, false)]
            };
            finish(&mut e, pending, &regular, &protection);
            assert_eq!(e.store().order("B").unwrap().price, 250.0);
            assert_eq!(e.store().order("Q").unwrap().price, 75.0);
            assert!(e.store().order("C").is_none());
            assert!(e.store().order("R").is_none());
            if tpsl {
                assert_eq!(e.store().order("P__position-1__tp").unwrap().price, 95.0);
                assert_eq!(e.store().order("P__position-1__tp").unwrap().amount, 0.02);
                assert!(e.store().order("P__position-1__sl").is_some());
                e.drain_actions().for_each(drop);
                e.handle(
                    4600.0,
                    Event::Command(safe_cex_engine::Command::AmendOrder {
                        command: 501,
                        id: "P__position-1__sl".into(),
                        price: Some(140.0),
                        amount: None,
                    }),
                );
                let r = requests(&mut e).remove(0);
                let bytes = r.body.unwrap();
                for field in [
                    r#""tpPrice":"95""#,
                    r#""slOrderType":"LIMIT""#,
                    r#""slOrderPrice":"321""#,
                ] {
                    assert!(bytes.contains(field), "{bytes} missing {field}");
                }
            } else {
                assert_eq!(e.store().order("A").unwrap().price, 111.0);
                assert_eq!(e.store().order("A").unwrap().amount, 4.0);
                assert!(e.store().order("A__tp").is_some());
                assert!(e.store().order("A__sl").is_some());
            }
        }
    }
}
#[test]
fn malformed_ws_identity_coverage_preserves_only_its_scope_and_never_starves_peers() {
    for tpsl in [false, true] {
        for known in [false, true] {
            let (mut e, socket) = seeded();
            let pending = reads(&mut e, socket, 3000.0);
            let topic = if tpsl { "tpsl" } else { "order" };
            let id = if known {
                if tpsl { "P" } else { "A" }
            } else {
                ""
            };
            let malformed = format!(r#"{{"symbol":"BTCUSDT","orderId":"{id}","qty":{{}}}}"#);
            for at in [4100.0, 4200.0, 4300.0] {
                ws(&mut e, socket, at, topic, &malformed);
            }
            finish(
                &mut e,
                pending,
                &[regular("A", 999, true), regular("B", 250, false)],
                &[protection("P", 999, true), protection("Q", 75, false)],
            );
            assert_eq!(e.store().order("B").unwrap().price, 250.0);
            assert_eq!(e.store().order("Q").unwrap().price, 75.0);
            if tpsl {
                assert_eq!(
                    e.store().order("P__position-1__tp").unwrap().price,
                    if known { 90.0 } else { 999.0 }
                );
                assert_eq!(e.store().order("A").unwrap().price, 999.0);
                assert_eq!(e.store().order("R").is_some(), !known);
                assert!(e.store().order("C").is_none());
            } else {
                assert_eq!(
                    e.store().order("A").unwrap().price,
                    if known { 100.0 } else { 999.0 }
                );
                assert_eq!(e.store().order("P__position-1__tp").unwrap().price, 999.0);
                assert_eq!(e.store().order("C").is_some(), !known);
                assert!(e.store().order("R").is_none());
            }
        }
    }
}
#[test]
fn terminal_ws_observation_cannot_be_resurrected_by_older_rest() {
    for tpsl in [false, true] {
        for cached in [false, true] {
            let (mut e, socket) = seeded();
            let pending = reads(&mut e, socket, 3000.0);
            let id = if cached {
                if tpsl { "P" } else { "A" }
            } else {
                "newly-closed"
            };
            let filled = if cached && !tpsl { 1 } else { 0 };
            let row = format!(
                r#"{{"event":"CLOSE","symbol":"BTCUSDT","orderId":"{id}","orderStatus":"CANCELED","dealAmount":"{filled}"}}"#
            );
            ws(
                &mut e,
                socket,
                4100.0,
                if tpsl { "tpsl" } else { "order" },
                &row,
            );
            let regular = if tpsl {
                regular_seed()
            } else {
                vec![regular(id, 123, false), regular("B", 250, false)]
            };
            let protection = if tpsl {
                vec![protection(id, 123, true), protection("Q", 75, false)]
            } else {
                protection_seed()
            };
            finish(&mut e, pending, &regular, &protection);
            let key = if tpsl && cached {
                "P__position-1__tp"
            } else {
                id
            };
            if cached {
                assert_eq!(
                    e.store().order(key).unwrap().status,
                    safe_cex_engine::model::OrderStatus::Canceled
                );
            } else {
                assert!(!e.store().orders().iter().any(|o| o.id.starts_with(id)));
            }
            assert_eq!(
                e.store().order("B").unwrap().price,
                if tpsl { 200.0 } else { 250.0 }
            );
            assert_eq!(
                e.store().order("Q").unwrap().price,
                if tpsl { 75.0 } else { 70.0 }
            );
        }
    }
}

fn observed_fills(e: &mut TestEngine) -> Vec<(u32, f64, f64)> {
    let Some(frame) = e.take_market_frame(100.0) else {
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
            let side = 4 + count * 8 + i * 4;
            let value = values + i * 16;
            (
                u32::from_le_bytes(payload[side..side + 4].try_into().unwrap()),
                f64::from_le_bytes(payload[value..value + 8].try_into().unwrap()),
                f64::from_le_bytes(payload[value + 8..value + 16].try_into().unwrap()),
            )
        })
        .collect()
}

#[test]
fn missing_or_null_ws_data_holds_only_endpoint_absence_and_allows_healthy_replacements() {
    for tpsl in [false, true] {
        for missing in [false, true] {
            let (mut e, socket) = seeded();
            let pending = reads(&mut e, socket, 3000.0);
            let topic = if tpsl { "tpsl" } else { "order" };
            let text = if missing {
                format!(r#"{{"ch":"{topic}"}}"#)
            } else {
                format!(r#"{{"ch":"{topic}","data":null}}"#)
            };
            e.handle(
                4100.0,
                Event::WsMessage {
                    socket,
                    text: &text,
                },
            );
            finish(
                &mut e,
                pending,
                &[regular("A", 125, false), regular("B", 250, false)],
                &[protection("P", 95, false), protection("Q", 75, false)],
            );
            assert_eq!(e.store().order("A").unwrap().price, 125.0);
            assert!(e.store().order("A__tp").is_none());
            assert_eq!(e.store().order("P").unwrap().price, 95.0);
            assert!(e.store().order("P__position-1__sl").is_none());
            assert_eq!(e.store().order("C").is_some(), !tpsl);
            assert_eq!(e.store().order("R").is_some(), tpsl);
        }
    }
}
#[test]
fn superseded_rest_baseline_cannot_consume_a_later_ws_fill() {
    let (mut e, socket) = seeded();
    let pending = reads(&mut e, socket, 3000.0);
    ws(
        &mut e,
        socket,
        4100.0,
        "order",
        r#"{"event":"UPDATE","symbol":"BTCUSDT","orderId":"A","price":"100","qty":"3","dealAmount":"2"}"#,
    );
    assert_eq!(observed_fills(&mut e), vec![(1, 100.0, 1.0)]);
    let stale = regular("A", 100, true).replace("\"tradeQty\":\"1\"", "\"tradeQty\":\"999\"");
    finish(
        &mut e,
        pending,
        &[stale, regular("B", 250, false)],
        &protection_seed(),
    );
    assert_eq!(e.store().order("B").unwrap().price, 250.0);
    assert_eq!(e.store().order("A").unwrap().filled, 2.0);
    assert!(observed_fills(&mut e).is_empty());
    ws(
        &mut e,
        socket,
        4600.0,
        "order",
        r#"{"event":"UPDATE","symbol":"BTCUSDT","orderId":"A","price":"100","qty":"3","dealAmount":"3"}"#,
    );
    assert_eq!(observed_fills(&mut e), vec![(1, 100.0, 1.0)]);
}
#[test]
fn unknown_sparse_execution_gets_rest_metadata_without_consuming_its_pending_fill() {
    let (mut e, socket) = seeded();
    let pending = reads(&mut e, socket, 3000.0);
    e.set_log_level(safe_cex_engine::Level::Error);
    ws(
        &mut e,
        socket,
        4100.0,
        "order",
        r#"{"event":"UPDATE","symbol":"BTCUSDT","orderId":"Z","dealAmount":"2"}"#,
    );
    assert!(observed_fills(&mut e).is_empty());
    let events = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some(event)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, safe_cex_engine::Level::Warn);
    assert_eq!(events[0].message, "Bitunix fill metadata is not reconciled");
    assert_eq!(
        events[0].fields,
        vec![(
            "scope",
            safe_cex_engine::Value::Str("account/background".into())
        )]
    );
    let metadata = regular("Z", 120, false).replace("\"tradeQty\":\"1\"", "\"tradeQty\":\"2\"");
    finish(
        &mut e,
        pending,
        &[metadata, regular("B", 250, false)],
        &protection_seed(),
    );
    assert_eq!(e.store().order("Z").unwrap().price, 120.0);
    assert_eq!(e.store().order("B").unwrap().price, 250.0);
    ws(
        &mut e,
        socket,
        4600.0,
        "order",
        r#"{"event":"UPDATE","symbol":"BTCUSDT","orderId":"Z","dealAmount":"2"}"#,
    );
    assert_eq!(observed_fills(&mut e), vec![(1, 120.0, 2.0)]);
    ws(
        &mut e,
        socket,
        4700.0,
        "order",
        r#"{"event":"UPDATE","symbol":"BTCUSDT","orderId":"Z","dealAmount":"2"}"#,
    );
    assert!(observed_fills(&mut e).is_empty());
}

fn initial_halves() -> (TestEngine, SocketId, Vec<HttpRequest>) {
    use safe_cex_venue_bitunix::{BitunixFutures, Options};
    let mut e = safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Options::default()
    }));
    e.start(0.0);
    let mut halves = Vec::new();
    let mut private = None;
    loop {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
        if actions.is_empty() {
            break;
        }
        for a in actions {
            match a {
                Action::WsConnect { socket, url, .. } => {
                    if url.contains("/private/") {
                        private = Some(socket);
                    }
                    e.handle(0.0, Event::WsOpened { socket });
                }
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
                _ => {}
            }
        }
    }
    assert_eq!(halves.len(), 2);
    assert!(!e.store().loaded().orders);
    (e, private.unwrap(), halves)
}

#[test]
fn staged_initial_halves_keep_newer_ws_families_until_complete_pair_publication() {
    for tpsl_first in [false, true] {
        let (mut e, socket, mut halves) = initial_halves();
        let regular_rows = regular_seed();
        let protection_rows = protection_seed();
        halves.sort_by_key(|r| r.url.contains("/tpsl/") != tpsl_first);
        let first = halves.remove(0);
        answer(
            &mut e,
            first.id,
            100.0,
            &body(
                if tpsl_first {
                    &protection_rows
                } else {
                    &regular_rows
                },
                tpsl_first,
            ),
        );
        assert!(!e.store().loaded().orders);
        let row = if tpsl_first {
            protection("P", 95, true)
        } else {
            regular("A", 111, true)
        };
        let row = row.replacen("{", r#"{"event":"CREATE","#, 1);
        ws(
            &mut e,
            socket,
            200.0,
            if tpsl_first { "tpsl" } else { "order" },
            &row,
        );
        assert!(!e.store().loaded().orders);
        let other = halves.remove(0);
        answer(
            &mut e,
            other.id,
            300.0,
            &body(
                if tpsl_first {
                    &regular_rows
                } else {
                    &protection_rows
                },
                !tpsl_first,
            ),
        );
        assert!(
            e.store().loaded().orders,
            "valid complete pair progresses across WS activity"
        );
        assert_eq!(
            e.store()
                .order(if tpsl_first { "P__position-1__tp" } else { "A" })
                .unwrap()
                .price,
            if tpsl_first { 95.0 } else { 111.0 }
        );
        assert!(e.store().order("B").is_some());
        assert!(e.store().order("Q").is_some());
    }
}
#[test]
fn malformed_ws_after_staging_cannot_make_a_partial_initial_pair_ready() {
    for tpsl_first in [false, true] {
        for malformed in ["null", r#"{"orderId":"A","qty":{}}"#] {
            let (mut e, socket, mut halves) = initial_halves();
            let regular_rows = regular_seed();
            let protection_rows = protection_seed();
            halves.sort_by_key(|r| r.url.contains("/tpsl/") != tpsl_first);
            let first = halves.remove(0);
            answer(
                &mut e,
                first.id,
                100.0,
                &body(
                    if tpsl_first {
                        &protection_rows
                    } else {
                        &regular_rows
                    },
                    tpsl_first,
                ),
            );
            assert!(!e.store().loaded().orders);
            ws(
                &mut e,
                socket,
                200.0,
                if tpsl_first { "tpsl" } else { "order" },
                malformed,
            );
            let other = halves.remove(0);
            answer(
                &mut e,
                other.id,
                300.0,
                &body(
                    if tpsl_first {
                        &regular_rows
                    } else {
                        &protection_rows
                    },
                    !tpsl_first,
                ),
            );
            assert!(!e.store().loaded().orders);
            assert!(
                e.store().orders().is_empty(),
                "invalid pair publishes neither REST half"
            );
        }
    }
}
#[test]
fn overlap_coverage_overflow_holds_only_its_endpoint_and_emits_one_diagnostic() {
    for tpsl in [false, true] {
        for malformed in [false, true] {
            let (mut e, socket) = seeded();
            let pending = reads(&mut e, socket, 3000.0);
            for n in 0..4097 {
                let row = if malformed {
                    format!(r#"{{"orderId":"bad-{n}","qty":{{}}}}"#)
                } else {
                    format!(r#"{{"event":"CLOSE","symbol":"BTCUSDT","orderId":"closed-{n}"}}"#)
                };
                ws(
                    &mut e,
                    socket,
                    4100.0,
                    if tpsl { "tpsl" } else { "order" },
                    &row,
                );
            }
            let diagnostics=e.drain_actions().filter(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(event))if event.code == safe_cex_engine::log::code::SNAPSHOT_DISCARDED)).count();
            assert_eq!(diagnostics, 1);
            finish(
                &mut e,
                pending,
                &[regular("A", 999, true), regular("B", 250, false)],
                &[protection("P", 999, true), protection("Q", 75, false)],
            );
            assert_eq!(
                e.store().order("B").unwrap().price,
                if tpsl { 250.0 } else { 200.0 }
            );
            assert_eq!(
                e.store().order("Q").unwrap().price,
                if tpsl { 70.0 } else { 75.0 }
            );
            assert_eq!(e.store().order("C").is_some(), !tpsl);
            assert_eq!(e.store().order("R").is_some(), tpsl);
        }
    }
}

#[test]
fn partial_cancel_ack_fences_older_rest_before_the_error_exit() {
    use safe_cex_engine::model::OrderStatus;
    use safe_cex_engine::{Command, Method, Notice};
    let (mut e, socket) = seeded();
    e.handle(
        3000.0,
        Event::Command(Command::CancelOrders {
            command: 601,
            ids: vec!["A".into(), "B".into()],
        }),
    );
    let writes = requests(&mut e);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].method, Method::Post);
    let pending = reads(&mut e, socket, 3100.0);
    answer(
        &mut e,
        writes[0].id,
        4200.0,
        r#"{"code":0,"data":{"successList":[{"orderId":"A"}],"failureList":[{"orderId":"B","errorMsg":"cancel rejected"}]}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 601, .. })
    )));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.method==Method::Post))
    );
    assert_eq!(e.store().order("A").unwrap().status, OrderStatus::Canceled);
    assert_eq!(e.store().order("B").unwrap().status, OrderStatus::Open);
    finish(
        &mut e,
        pending,
        &[regular("A", 999, true), regular("B", 250, false)],
        &protection_seed(),
    );
    assert_eq!(
        e.store().order("A").unwrap().status,
        OrderStatus::Canceled,
        "stale REST must not reopen an accepted cancellation"
    );
    assert_eq!(
        e.store().order("B").unwrap().price,
        200.0,
        "whole command-crossing read remains stale"
    );
}
#[test]
fn snapshot_crossing_write_cleanup_cannot_resurrect_an_optimistic_order() {
    use safe_cex_engine::model::OrderStatus;
    use safe_cex_engine::{Command, Method};
    for succeeds in [false, true] {
        let (mut e, socket) = seeded();
        let opts=serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":120,"amount":0.01,"clientOrderId":"creating-caller"}"#).unwrap();
        e.handle(
            3000.0,
            Event::Command(Command::PlaceOrders {
                command: 602,
                orders: vec![opts],
            }),
        );
        let write = requests(&mut e).remove(0);
        assert_eq!(write.method, Method::Post);
        let creating = e
            .store()
            .orders()
            .iter()
            .find(|row| row.status == OrderStatus::Creating)
            .unwrap()
            .clone();
        let pending = reads(&mut e, socket, 3100.0);
        if succeeds {
            answer(
                &mut e,
                write.id,
                4200.0,
                r#"{"code":0,"data":{"successList":[{"orderId":"accepted-new"}],"failureList":[]}}"#,
            );
        } else {
            e.handle(
                4200.0,
                Event::HttpFailed {
                    request: write.id,
                    error: "response lost",
                },
            );
        }
        e.drain_actions().for_each(drop);
        assert!(e.store().order(&creating.id).is_none());
        finish(
            &mut e,
            pending,
            &[regular(&creating.id, 120, false), regular("B", 250, false)],
            &protection_seed(),
        );
        assert!(
            e.store().order(&creating.id).is_none(),
            "snapshot issued before cleanup cannot restore its temporary ID"
        );
        assert_eq!(e.store().order("B").unwrap().price, 200.0);
    }
}

#[test]
fn initial_newer_coverage_overflow_never_claims_complete_snapshot_authority() {
    for tpsl_first in [false, true] {
        for staged in [false, true] {
            let (mut e, socket, mut halves) = initial_halves();
            halves.sort_by_key(|r| r.url.contains("/tpsl/") != tpsl_first);
            let first = halves.remove(0);
            let regular_rows = regular_seed();
            let protection_rows = protection_seed();
            if staged {
                answer(
                    &mut e,
                    first.id,
                    100.0,
                    &body(
                        if tpsl_first {
                            &protection_rows
                        } else {
                            &regular_rows
                        },
                        tpsl_first,
                    ),
                );
            }
            for n in 0..4097 {
                let row = format!(
                    r#"{{"event":"CLOSE","symbol":"BTCUSDT","orderId":"initial-closed-{n}"}}"#
                );
                ws(
                    &mut e,
                    socket,
                    200.0,
                    if tpsl_first { "tpsl" } else { "order" },
                    &row,
                );
            }
            if !staged {
                answer(
                    &mut e,
                    first.id,
                    300.0,
                    &body(
                        if tpsl_first {
                            &protection_rows
                        } else {
                            &regular_rows
                        },
                        tpsl_first,
                    ),
                );
            }
            let other = halves.remove(0);
            answer(
                &mut e,
                other.id,
                400.0,
                &body(
                    if tpsl_first {
                        &regular_rows
                    } else {
                        &protection_rows
                    },
                    !tpsl_first,
                ),
            );
            assert!(
                !e.store().loaded().orders,
                "bounded coverage cannot prove REST-only peers were fully observed"
            );
            assert!(
                e.store().orders().is_empty(),
                "neither REST half publishes from an incomplete initial pair"
            );
        }
    }
}

#[test]
fn owned_read_crossing_partial_cancel_ack_neither_publishes_stale_rows_nor_approves_write() {
    use safe_cex_engine::model::OrderStatus;
    use safe_cex_engine::{Command, Method, Notice};
    let (mut e, _) = seeded();
    e.handle(
        3000.0,
        Event::Command(Command::CancelOrders {
            command: 701,
            ids: vec!["A".into(), "B".into()],
        }),
    );
    let cancel = requests(&mut e).remove(0);
    let opts=serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"buy","price":85000,"amount":0,"tpTriggerType":"mark_price"}"#).unwrap();
    e.handle(
        3100.0,
        Event::Command(Command::PlaceOrders {
            command: 702,
            orders: vec![opts],
        }),
    );
    let positions = requests(&mut e).remove(0);
    assert!(positions.url.contains("get_pending_positions"));
    answer(&mut e, positions.id, 3200.0, POSITIONS);
    let owned = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("/trade/get_pending_orders"))
        .expect("owned order prerequisite");
    answer(
        &mut e,
        cancel.id,
        3300.0,
        r#"{"code":0,"data":{"successList":[{"orderId":"A"}],"failureList":[{"orderId":"B","errorMsg":"cancel rejected"}]}}"#,
    );
    assert_eq!(e.store().order("A").unwrap().status, OrderStatus::Canceled);
    e.drain_actions().for_each(drop);
    answer(
        &mut e,
        owned.id,
        3400.0,
        &body(&[regular("A", 999, true), regular("B", 250, false)], false),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r)if r.method==Method::Post))
    );
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 702, .. })
    )));
    assert_eq!(
        e.store().order("A").unwrap().status,
        OrderStatus::Canceled,
        "strict response failure must also fence STORE publication"
    );
    assert_eq!(e.store().order("B").unwrap().price, 200.0);
}

#[test]
fn exhaustion_between_initial_halves_never_publishes_native_or_store_snapshot_rows() {
    for first_tpsl in [false, true] {
        let (mut e, socket, mut halves) = initial_halves();
        halves.sort_by_key(|r| r.url.contains("/tpsl/") != first_tpsl);
        let first = halves.remove(0);
        let regular_rows = regular_seed();
        let protection_rows = protection_seed();
        answer(
            &mut e,
            first.id,
            100.0,
            &body(
                if first_tpsl {
                    &protection_rows
                } else {
                    &regular_rows
                },
                first_tpsl,
            ),
        );
        assert!(!e.store().loaded().orders);
        safe_cex_engine::test_support::exhaust_order_snapshots(e.store_mut());
        e.drain_actions().for_each(drop);
        let second = halves.remove(0);
        answer(
            &mut e,
            second.id,
            200.0,
            &body(
                if first_tpsl {
                    &regular_rows
                } else {
                    &protection_rows
                },
                !first_tpsl,
            ),
        );
        assert!(!e.store().loaded().orders);
        assert!(e.store().orders().is_empty());
        assert!(!e.drain_actions().any(|action| matches!(
            action,
            Action::Notice(safe_cex_engine::Notice::AccountAuth { .. })
        )));
        ws(
            &mut e,
            socket,
            300.0,
            "order",
            r#"{"event":"UPDATE","symbol":"BTCUSDT","orderId":"A","side":"SELL","price":"100","qty":"3","dealAmount":"2"}"#,
        );
        assert_eq!(
            observed_fills(&mut e),
            vec![(1, 100.0, 2.0)],
            "rejected snapshot must not consume the native cumulative baseline"
        );
    }
}
