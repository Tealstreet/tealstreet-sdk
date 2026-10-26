//! Originals for docs/plans/RUST_BITUNIX_TRADING_FUNNEL.md: trading on
//! Preflight roots and serial Plans, concurrent under job-scoped order locks.
mod support;
use safe_cex_engine::{
    Action, Command, Event, HttpRequest, Notice, Outcome, ReadCredentials, VenueError,
};
use support::*;

fn keyed_with_read() -> (TestEngine, safe_cex_engine::SocketId) {
    startup_with_engine(engine(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-material".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "read-material".into(),
            passphrase: None,
        }),
        ..Options::default()
    }))
}
fn prepared(e: &mut TestEngine, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn http(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect()
}
fn api_key(request: &HttpRequest) -> &str {
    request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("api-key"))
        .map(|(_, value)| value.as_str())
        .expect("a signed request")
}
fn failure(actions: &[Action], command: u32) -> Option<VenueError> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
            Some(error.clone())
        }
        _ => None,
    })
}
fn answer(e: &mut TestEngine, now: f64, request: &HttpRequest, body: &str) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    prepared(e, now)
}
fn leverage(command: u32, value: f64) -> Command {
    Command::SetLeverage {
        command,
        symbol: "BTCUSDT".into(),
        leverage: value,
        side: None,
    }
}
fn writes(requests: &[HttpRequest], path: &str) -> usize {
    requests.iter().filter(|r| r.url.contains(path)).count()
}

/// Control 3: a validation refusal is the command's single NotSent terminal.
#[test]
fn a_validation_refusal_is_one_not_sent_terminal() {
    let (mut e, _) = startup();
    e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command: 31,
            symbol: "NOPEUSDT".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let actions = prepared(&mut e, 2000.0);
    assert!(http(&actions).is_empty());
    let error = failure(&actions, 31).expect("one terminal");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

/// Control 4: a placement's prerequisite read is Primary-signed inside the
/// command, even with a read key configured; a failed one sends no write.
#[test]
fn prerequisite_reads_sign_primary_and_a_failed_one_sends_no_write() {
    let (mut e, _) = keyed_with_read();
    e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 41,
            orders: vec![serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"stop_market","side":"buy","price":100000,"amount":0}"#,
            )
            .unwrap()],
        }),
    );
    let requests = http(&prepared(&mut e, 2000.0));
    let read = requests
        .iter()
        .find(|r| r.url.contains("get_pending_positions"))
        .expect("the prerequisite position read");
    assert_eq!(api_key(read), "replay-key", "in-job reads sign Primary");
    let actions = answer(&mut e, 2001.0, read, r#"{"code":-1,"msg":"busy"}"#);
    let requests = http(&actions);
    assert_eq!(
        writes(&requests, "/tpsl/"),
        0,
        "no write after a failed prerequisite"
    );
    assert_eq!(writes(&requests, "place_order"), 0);
    assert_eq!(
        failure(&actions, 41).and_then(|e| e.outcome),
        Some(Outcome::NotSent)
    );
}

/// Control 5: a refused later chunk keeps the earlier chunk's accepted IDs.
#[test]
fn a_refused_later_chunk_keeps_the_accepted_ids() {
    let (mut e, _) = startup();
    let order = || -> safe_cex_engine::model::PlaceOrderOpts {
        serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500.04,"amount":0.0021}"#,
        )
        .unwrap()
    };
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 51,
            orders: (0..6).map(|_| order()).collect(),
        }),
    );
    let first = http(&prepared(&mut e, 1.0)).remove(0);
    assert!(first.url.ends_with("batch_order"));
    let actions = answer(
        &mut e,
        2.0,
        &first,
        r#"{"code":0,"data":{"successList":[{"orderId":"1"},{"orderId":"2"},{"orderId":"3"},{"orderId":"4"},{"orderId":"5"}],"failureList":[]}}"#,
    );
    // The chunk gap is a Plan timer; an order refresh may share its instant.
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms >= 1000.0 => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    let gap = timers.first().expect("the 1000 ms chunk gap").1;
    let mut requests = Vec::new();
    for (timer, at) in &timers {
        e.handle(*at, Event::TimerFired { timer: *timer });
        requests.extend(http(&prepared(&mut e, *at)));
    }
    let timer = (timers[0].0, gap);
    let second = requests
        .into_iter()
        .find(|r| r.url.ends_with("place_order"))
        .expect("the second chunk");
    let actions = answer(
        &mut e,
        timer.1 + 1.0,
        &second,
        r#"{"code":10002,"msg":"Parameter error"}"#,
    );
    let error = failure(&actions, 51).expect("one terminal");
    assert_eq!(
        error.partial.as_ref().map(|p| p.order_ids.clone()),
        Some(
            vec!["1", "2", "3", "4", "5"]
                .into_iter()
                .map(String::from)
                .collect()
        )
    );
}

/// Control 9: the order lock holds an order for the whole job; a second
/// mutation of it is refused before any write.
#[test]
fn a_second_mutation_of_a_held_order_is_refused_before_any_write() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 91,
            id: "one".into(),
            price: Some(99.0),
            amount: None,
        }),
    );
    let first = http(&prepared(&mut e, 1.0));
    assert_eq!(first.len(), 1, "the first amend writes");
    e.handle(
        2.0,
        Event::Command(Command::CancelOrders {
            command: 92,
            ids: vec!["one".into()],
        }),
    );
    let actions = prepared(&mut e, 2.0);
    assert!(
        http(&actions).is_empty(),
        "no interleaved write on a held order"
    );
    assert!(failure(&actions, 92).is_some());
}

/// Control 9b (dazzy3 b): a second leverage change on the same symbol is
/// refused while one is live, and a placement still proceeds concurrently.
#[test]
fn settings_serialize_per_symbol_while_placements_proceed() {
    let (mut e, _) = startup();
    e.handle(1.0, Event::Command(leverage(93, 5.0)));
    let first = http(&prepared(&mut e, 1.0));
    assert_eq!(writes(&first, "change_leverage"), 1);
    e.handle(2.0, Event::Command(leverage(94, 7.0)));
    e.handle(
        2.0,
        Event::Command(Command::PlaceOrders {
            command: 95,
            orders: vec![serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500.04,"amount":0.0021}"#).unwrap()],
        }),
    );
    let actions = prepared(&mut e, 2.0);
    let requests = http(&actions);
    assert_eq!(
        writes(&requests, "change_leverage"),
        0,
        "the second leverage is refused, not queued"
    );
    assert!(failure(&actions, 94).is_some_and(|e| e.outcome == Some(Outcome::NotSent)));
    assert_eq!(
        writes(&requests, "place_order"),
        1,
        "placements stay concurrent"
    );
}

/// Control 9d: a cancel is never held behind another job's 1000 ms
/// batch-place gap (why there is no FIFO).
#[test]
fn a_cancel_is_not_held_behind_a_batch_place_gap() {
    let (mut e, _) = startup();
    let order = || -> safe_cex_engine::model::PlaceOrderOpts {
        serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500.04,"amount":0.0021}"#,
        )
        .unwrap()
    };
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 96,
            orders: (0..6).map(|_| order()).collect(),
        }),
    );
    let first = http(&prepared(&mut e, 1.0)).remove(0);
    answer(
        &mut e,
        2.0,
        &first,
        r#"{"code":0,"data":{"successList":[{"orderId":"1"},{"orderId":"2"},{"orderId":"3"},{"orderId":"4"},{"orderId":"5"}],"failureList":[]}}"#,
    );
    e.handle(
        3.0,
        Event::Command(Command::CancelOrders {
            command: 97,
            ids: vec!["one".into()],
        }),
    );
    let requests = http(&prepared(&mut e, 3.0));
    assert_eq!(
        writes(&requests, "cancel_orders"),
        1,
        "the cancel goes out inside the gap"
    );
}

/// Control 9c (dazzy3 c): two jobs' prerequisite reads answer out of order;
/// the older snapshot never overwrites the newer, and its job sends no write.
#[test]
fn an_older_prerequisite_snapshot_never_overwrites_a_newer_one() {
    let (mut e, _) = startup();
    let stop = || -> safe_cex_engine::model::PlaceOrderOpts {
        serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"stop_market","side":"buy","price":100000,"amount":0}"#,
        )
        .unwrap()
    };
    for command in [61, 62] {
        e.handle(
            2000.0,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![stop()],
            }),
        );
    }
    let reads: Vec<_> = http(&prepared(&mut e, 2000.0))
        .into_iter()
        .filter(|r| r.url.contains("get_pending_positions"))
        .collect();
    assert_eq!(reads.len(), 2, "each job reads positions");
    answer(&mut e, 2001.0, &reads[1], POSITIONS);
    let actions = answer(&mut e, 2002.0, &reads[0], r#"{"code":0,"data":[]}"#);
    assert!(
        !e.store().positions().is_empty(),
        "the newer snapshot stands"
    );
    assert_eq!(
        failure(&actions, 61).and_then(|e| e.outcome),
        Some(Outcome::NotSent)
    );
    assert_eq!(writes(&http(&actions), "/tpsl/"), 0);
}

fn post(requests: &[HttpRequest], path: &str) -> Option<HttpRequest> {
    requests
        .iter()
        .find(|r| r.method == safe_cex_engine::Method::Post && r.url.contains(path))
        .cloned()
}
/// Answers every prerequisite GET until a write goes out; returns the writes.
fn through_reads(
    e: &mut TestEngine,
    mut now: f64,
    mut requests: Vec<HttpRequest>,
) -> Vec<HttpRequest> {
    for _ in 0..10 {
        let writes: Vec<_> = requests
            .iter()
            .filter(|r| r.method == safe_cex_engine::Method::Post)
            .cloned()
            .collect();
        if !writes.is_empty() {
            return writes;
        }
        let mut next = Vec::new();
        for r in requests {
            now += 1.0;
            let body = if r.url.contains("get_pending_positions") {
                POSITIONS
            } else if r.url.contains("/tpsl/") {
                TPSL
            } else {
                ORDERS
            };
            next.extend(http(&answer(e, now, &r, body)));
        }
        requests = next;
    }
    panic!("no write followed the reads")
}

/// Control 6: "position does not exist" re-reads and re-places once, as
/// continuations; a second refusal ends the job with the refusal kept.
#[test]
fn position_does_not_exist_re_reads_and_re_places_once() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 81,
            orders: vec![serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500.04,"amount":0.0021}"#,
            )
            .unwrap()],
        }),
    );
    let first = post(&http(&prepared(&mut e, 1.0)), "place_order").expect("the place");
    let refused = r#"{"code":20003,"msg":"Position does not exist"}"#;
    let actions = answer(&mut e, 2.0, &first, refused);
    assert!(
        failure(&actions, 81).is_none(),
        "native recover, not a terminal"
    );
    let reads = http(&actions);
    assert!(
        reads
            .iter()
            .any(|r| r.url.contains("get_pending_positions")),
        "the re-read is the job's own Read call"
    );
    assert_eq!(api_key(&reads[0]), "replay-key");
    let writes = through_reads(&mut e, 3.0, reads);
    let again = post(&writes, "place_order").expect("the one re-place");
    let actions = answer(&mut e, 20.0, &again, refused);
    assert_eq!(
        failure(&actions, 81).and_then(|e| e.outcome),
        Some(Outcome::Rejected)
    );
    assert_eq!(writes_of(&actions, "place_order"), 0, "never a third place");
}
fn writes_of(actions: &[Action], path: &str) -> usize {
    http(actions)
        .iter()
        .filter(|r| r.method == safe_cex_engine::Method::Post && r.url.contains(path))
        .count()
}

/// Control 9 (a): a command arriving inside the amend fallback (after its
/// cancel is sent, and again before its replacement) is refused by the lock.
#[test]
fn the_order_lock_holds_across_the_amend_fallback() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 85,
            id: "one__tp".into(),
            price: Some(79.0),
            amount: None,
        }),
    );
    let amend = http(&prepared(&mut e, 1.0)).remove(0);
    let actions = answer(
        &mut e,
        2.0,
        &amend,
        r#"{"code":20003,"msg":"The order does not support this operation"}"#,
    );
    let cancel = post(&http(&actions), "cancel_orders").expect("the fallback cancel");
    e.handle(
        3.0,
        Event::Command(Command::CancelOrders {
            command: 86,
            ids: vec!["one".into()],
        }),
    );
    let actions = prepared(&mut e, 3.0);
    assert_eq!(
        writes_of(&actions, "/"),
        0,
        "no write between cancel and replacement"
    );
    assert!(failure(&actions, 86).is_some());
    let actions = answer(
        &mut e,
        4.0,
        &cancel,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    assert!(
        http(&actions)
            .iter()
            .any(|r| r.url.contains("get_order_detail")),
        "the final-state read precedes any replacement"
    );
    e.handle(
        5.0,
        Event::Command(Command::AmendOrder {
            command: 87,
            id: "one".into(),
            price: Some(81.0),
            amount: None,
        }),
    );
    let actions = prepared(&mut e, 5.0);
    assert_eq!(writes_of(&actions, "/"), 0);
    assert!(failure(&actions, 87).is_some());
}

/// Control 9b (b): margin against leverage on one symbol and mode against
/// mode are refused while one is live.
#[test]
fn settings_scopes_cover_margin_with_leverage_and_mode_with_mode() {
    let (mut e, _) = startup();
    e.store_mut().set_positions(vec![]);
    e.handle(1.0, Event::Command(leverage(101, 5.0)));
    e.handle(
        1.0,
        Event::Command(Command::SetPositionMode {
            command: 102,
            hedged: true,
            symbol: None,
        }),
    );
    let first = http(&prepared(&mut e, 1.0));
    assert_eq!(writes(&first, "change_leverage"), 1);
    assert_eq!(writes(&first, "change_position_mode"), 1);
    e.handle(
        2.0,
        Event::Command(Command::SetMarginType {
            command: 103,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    e.handle(
        2.0,
        Event::Command(Command::SetPositionMode {
            command: 104,
            hedged: false,
            symbol: None,
        }),
    );
    let actions = prepared(&mut e, 2.0);
    assert_eq!(writes_of(&actions, "/"), 0);
    for command in [103, 104] {
        assert_eq!(
            failure(&actions, command).and_then(|e| e.outcome),
            Some(Outcome::NotSent)
        );
    }
}

/// Control 9b (M1): an Unknown leverage change holds its symbol until a fresh
/// settings read lands; a second change is refused until then, then admitted.
#[test]
fn an_unknown_leverage_change_holds_the_symbol_until_a_fresh_read() {
    let (mut e, _) = startup();
    e.handle(1.0, Event::Command(leverage(111, 5.0)));
    let write = post(&http(&prepared(&mut e, 1.0)), "change_leverage").expect("the write");
    e.handle(
        2.0,
        Event::HttpFailed {
            request: write.id,
            error: "lost settings ACK",
        },
    );
    let actions = prepared(&mut e, 2.0);
    assert_eq!(
        failure(&actions, 111).and_then(|e| e.outcome),
        Some(Outcome::Unknown)
    );
    let read = http(&actions)
        .into_iter()
        .find(|r| r.url.contains("get_leverage_margin_mode"))
        .expect("the hold's fresh settings read");
    e.handle(3.0, Event::Command(leverage(112, 10.0)));
    let actions = prepared(&mut e, 3.0);
    assert_eq!(writes(&http(&actions), "change_leverage"), 0);
    assert_eq!(
        failure(&actions, 112).and_then(|e| e.outcome),
        Some(Outcome::NotSent)
    );
    answer(
        &mut e,
        4.0,
        &read,
        r#"{"code":0,"data":{"leverage":5,"marginMode":"CROSS"}}"#,
    );
    e.handle(5.0, Event::Command(leverage(113, 10.0)));
    assert_eq!(
        writes(&http(&prepared(&mut e, 5.0)), "change_leverage"),
        1,
        "admitted once the fresh read lands"
    );
}

/// L1 (dazzy3): the amend fallback's replacement is fenced as a placement, so
/// a lost replacement ACK leaves cancel-all refused until a fresh census.
#[test]
fn a_lost_fallback_replacement_fences_cancel_all() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 121,
            id: "one__tp".into(),
            price: Some(79.0),
            amount: None,
        }),
    );
    let amend = http(&prepared(&mut e, 1.0)).remove(0);
    let actions = answer(
        &mut e,
        2.0,
        &amend,
        r#"{"code":20003,"msg":"The order does not support this operation"}"#,
    );
    let cancel = post(&http(&actions), "cancel_orders").expect("the fallback cancel");
    let actions = answer(
        &mut e,
        3.0,
        &cancel,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    let detail = http(&actions)
        .into_iter()
        .find(|r| r.url.contains("get_order_detail"))
        .expect("the final-state read");
    let actions = answer(
        &mut e,
        4.0,
        &detail,
        r#"{"code":0,"data":{"orderId":"one","symbol":"BTCUSDT","status":"CANCELED","qty":"3","tradeQty":"1"}}"#,
    );
    let replacement = post(&http(&actions), "place_order").expect("the replacement");
    e.handle(
        5.0,
        Event::HttpFailed {
            request: replacement.id,
            error: "lost replacement ACK",
        },
    );
    let actions = prepared(&mut e, 5.0);
    assert_eq!(
        failure(&actions, 121).and_then(|e| e.outcome),
        Some(Outcome::Unknown)
    );
    e.handle(
        6.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 122,
            symbol: "BTCUSDT".into(),
        }),
    );
    let actions = prepared(&mut e, 6.0);
    // The engine defers cancel-all behind the uncertain placement.
    assert_eq!(
        writes_of(&actions, "cancel_orders"),
        0,
        "cancel-all stays fenced"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 122, .. })))
    );
}

/// P1 money path (#2279): an Unknown placement fences its symbol. A retry is
/// refused NotSent, a reconcile read started before the seal does not release
/// it, and the fresh read `reconcile_uncertain` starts does.
#[test]
fn an_unknown_placement_fences_retries_until_a_fresh_reconcile() {
    let (mut e, s) = startup();
    let place = |command: u32| {
        Command::PlaceOrders {
        command,
        orders: vec![serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500.04,"amount":0.0021}"#).unwrap()],
    }
    };
    let reads = |actions: &[Action]| -> Vec<HttpRequest> {
        http(actions)
            .into_iter()
            .filter(|r| r.url.ends_with("get_pending_orders"))
            .collect()
    };
    // A stale reconcile: a private order event's census, still in flight.
    e.handle(
        100.0,
        Event::WsMessage {
            socket: s,
            text: r#"{"ch":"order","data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"other","side":"BUY","type":"LIMIT","qty":"1","price":"90000","dealAmount":"0"}}"#,
        },
    );
    let timer = prepared(&mut e, 100.0)
        .into_iter()
        .filter_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1100.0,
            } => Some(timer),
            _ => None,
        })
        .next_back()
        .expect("the private event schedules its census");
    e.handle(1100.0, Event::TimerFired { timer });
    let stale = reads(&prepared(&mut e, 1100.0));
    assert_eq!(stale.len(), 2, "regular and tpsl census reads");

    e.handle(1200.0, Event::Command(place(201)));
    let write = post(&http(&prepared(&mut e, 1200.0)), "place_order").expect("the placement");
    e.handle(
        1300.0,
        Event::HttpFailed {
            request: write.id,
            error: "lost placement ACK",
        },
    );
    let sealed = prepared(&mut e, 1300.0);
    assert_eq!(
        failure(&sealed, 201).and_then(|e| e.outcome),
        Some(Outcome::Unknown)
    );
    // reconcile_uncertain supersedes the in-flight census with a fresh one.
    let fresh = reads(&sealed);
    assert_eq!(fresh.len(), 2, "the fresh regular and tpsl census, at once");

    let refused = |e: &mut TestEngine, command: u32, at: f64| {
        e.handle(at, Event::Command(place(command)));
        let actions = prepared(e, at);
        assert_eq!(writes_of(&actions, "place_order"), 0, "command {command}");
        let error = failure(&actions, command).expect("a refusal");
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert!(
            error.legacy_text().contains("uncertain"),
            "{}",
            error.legacy_text()
        );
    };
    refused(&mut e, 202, 1400.0);

    // The census started before the seal answers: not proof, still fenced.
    for request in &stale {
        let body = if request.url.contains("/tpsl/") {
            TPSL
        } else {
            ORDERS
        };
        answer(&mut e, 1500.0, request, body);
    }
    refused(&mut e, 203, 1600.0);

    refused(&mut e, 204, 2350.0);
    for request in &fresh {
        let body = if request.url.contains("/tpsl/") {
            TPSL
        } else {
            ORDERS
        };
        answer(&mut e, 2400.0, request, body);
    }
    e.handle(2500.0, Event::Command(place(205)));
    assert_eq!(
        writes_of(&prepared(&mut e, 2500.0), "place_order"),
        1,
        "the fresh census releases the fence"
    );
}
