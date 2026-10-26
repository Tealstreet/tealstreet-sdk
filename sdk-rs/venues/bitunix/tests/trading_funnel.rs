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
    assert!(!e.store().positions().is_empty(), "the newer snapshot stands");
    assert_eq!(
        failure(&actions, 61).and_then(|e| e.outcome),
        Some(Outcome::NotSent)
    );
    assert_eq!(writes(&http(&actions), "/tpsl/"), 0);
}
