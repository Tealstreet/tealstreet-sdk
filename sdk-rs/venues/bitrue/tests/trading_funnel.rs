//! B2 originals (docs/plans/RUST_BITRUE_REQUEST_FUNNEL.md, controls 7-15):
//! trading writes on Preflight roots and serial or complete Plans.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Command, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, ReadCredentials, VenueError,
};
use support::Options;
use support::trading::*;

const ORDER: &str = "2334151155877155281";

fn keyed_with_read() -> Harness {
    Harness::with(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "read-material".into(),
            passphrase: None,
        }),
        ..Options::default()
    })
}
fn api_key(request: &HttpRequest) -> &str {
    request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("X-CH-APIKEY"))
        .map(|(_, value)| value.as_str())
        .expect("a signed request")
}
fn failure(h: &Harness, command: u32) -> Option<&VenueError> {
    h.notices.iter().find_map(|n| match n {
        Notice::CommandFailed { command: id, error } if *id == command => Some(error),
        _ => None,
    })
}
fn terminals(h: &Harness, command: u32) -> usize {
    h.notices
        .iter()
        .filter(|n| {
            matches!(n, Notice::CommandFailed { command: id, .. } | Notice::CommandDone { command: id, .. } if *id == command)
        })
        .count()
}
fn fail(h: &mut Harness, request: HttpRequest) {
    h.handle(
        h.now,
        Event::HttpFailed {
            request: request.id,
            error: "connection reset",
        },
    );
    h.collect();
}
fn position(h: &mut Harness) {
    h.e.store_mut().set_positions(vec![Position {
        id: "E-BTC-USDT".into(),
        symbol: "E-BTC-USDT".into(),
        side: PositionSide::Long,
        entry_price: 2000.0,
        notional: 20.0,
        leverage: 20.0,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: 0.0,
        realized_pnl: None,
        contracts: 0.01,
        liquidation_price: 0.0,
        margin_size: None,
        last_update: None,
    }]);
}
fn leverage(command: u32) -> Command {
    Command::SetLeverage {
        command,
        symbol: "E-BTC-USDT".into(),
        leverage: 10.0,
        side: None,
    }
}
fn level_edits(h: &Harness) -> usize {
    h.requests
        .iter()
        .filter(|r| r.method == Method::Post && r.url.ends_with("/fapi/v2/level_edit"))
        .count()
}

/// 7. A parked command whose caller wait expires never writes: at its wake
/// the expiry is its one NotSent terminal.
#[test]
fn an_expired_parked_command_never_writes() {
    let mut h = Harness::new();
    h.order(0.0);
    h.amend();
    let inspect = h.take(Method::Get, "/fapi/v2/order?");
    h.e.handle_command(h.now, leverage(8), Some(50)).unwrap();
    h.collect();
    assert_eq!(level_edits(&h), 0, "parked behind the amend");
    h.now += 1_000.0;
    h.reply(inspect, &detail("FILLED", 0.01, ""));
    let error = failure(&h, 8).expect("the parked command expired");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(level_edits(&h), 0, "an expired command never writes");
    assert_eq!(terminals(&h, 8), 1);
}

/// 7. A dequeued command re-plans against the current Store: a refusal there
/// is NotSent with no request.
#[test]
fn a_dequeue_re_plan_refusal_is_not_sent() {
    let mut h = Harness::new();
    h.order(0.0);
    h.amend();
    let inspect = h.take(Method::Get, "/fapi/v2/order?");
    let mut other = h.e.store().order(ORDER).unwrap().clone();
    other.id = "222".into();
    h.e.store_mut().upsert_order(other);
    h.command(Command::AmendOrder {
        command: 8,
        id: "222".into(),
        price: Some(1980.0),
        amount: None,
    });
    assert!(failure(&h, 8).is_none(), "parked behind the amend");
    // Its order closes while command 8 is parked.
    h.e.store_mut().set_order_status("222", OrderStatus::Closed);
    h.reply(inspect, &detail("FILLED", 0.01, ""));
    let error = failure(&h, 8).expect("the re-plan refused");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(h.writes(), 0);
}

/// 8 (D3). Inspect and Verify are in-job reads: they sign with the Plan's
/// Primary credential even with a read key configured.
#[test]
fn amend_reads_sign_with_the_primary_inside_the_write_job() {
    let mut h = keyed_with_read();
    h.order(0.004);
    h.amend();
    let inspect = h.take(Method::Get, "/fapi/v2/order?");
    assert_eq!(api_key(&inspect), "replay-key");
    h.reply(inspect, &detail("PARTIALLY_FILLED", 0.004, ""));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    assert_eq!(api_key(&cancel), "replay-key");
    h.reply(
        cancel,
        &format!(r#"{{"code":"0","data":{{"orderId":{ORDER}}}}}"#),
    );
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    assert_eq!(api_key(&verify), "replay-key");
}

/// 8 (D3). Native's three Read retries chain per call, and a fourth is never
/// sent; the job then fails NotSent with no write.
#[test]
fn an_inspect_chains_three_retries_and_never_a_fourth() {
    let mut h = Harness::new();
    h.order(0.0);
    h.amend();
    for _ in 0..4 {
        let inspect = h.take(Method::Get, "/fapi/v2/order?");
        fail(&mut h, inspect);
        let due: Vec<_> = h.timers.drain(..).collect();
        for (timer, at) in due {
            h.handle(at, Event::TimerFired { timer });
        }
        h.collect();
    }
    assert!(
        !h.requests.iter().any(|r| r.url.contains("/fapi/v2/order?")),
        "a fourth retry is never sent"
    );
    assert_eq!(
        failure(&h, 7).and_then(|e| e.outcome),
        Some(Outcome::NotSent)
    );
    assert_eq!(h.writes(), 0);
}

/// 8. A Verify that does not prove the cancel prepares no replacement; the
/// terminal keeps the accepted cancel as known-partial evidence.
#[test]
fn an_unproven_verify_prepares_no_replacement() {
    let mut h = Harness::new();
    h.order(0.004);
    h.amend();
    let inspect = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(inspect, &detail("PARTIALLY_FILLED", 0.004, ""));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    h.reply(
        cancel,
        &format!(r#"{{"code":"0","data":{{"orderId":{ORDER}}}}}"#),
    );
    let verify = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(verify, &detail("PARTIALLY_FILLED", 0.004, ""));
    assert!(
        !h.requests
            .iter()
            .any(|r| r.method == Method::Post && r.url.ends_with("/fapi/v2/order")),
        "no replacement without final cancellation proof"
    );
    // The accepted cancel is a known effect: the journal's partial contract.
    let error = failure(&h, 7).expect("one terminal");
    assert_eq!(error.outcome, None);
    assert_eq!(
        error.partial.as_ref().map(|p| p.order_ids.clone()),
        Some(vec![ORDER.to_owned()])
    );
    assert_eq!(terminals(&h, 7), 1);
}

/// 10. A -1022 on a write resends once with the same client order ID.
#[test]
fn a_signature_refusal_resends_a_write_once_with_its_client_id() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}]"#);
    let first = h.take(Method::Post, "/fapi/v2/order");
    let client = body(&first)["clientOrderId"].clone();
    h.e.handle(
        h.now,
        Event::HttpResponse {
            request: first.id,
            response: safe_cex_engine::Response {
                status: 400,
                headers: "",
                body: r#"{"code":-1022,"msg":"Signature for this request is not valid."}"#,
            },
        },
    );
    h.collect();
    let second = h.take(Method::Post, "/fapi/v2/order");
    assert_eq!(body(&second)["clientOrderId"], client);
    h.e.handle(
        h.now,
        Event::HttpResponse {
            request: second.id,
            response: safe_cex_engine::Response {
                status: 400,
                headers: "",
                body: r#"{"code":-1022,"msg":"Signature for this request is not valid."}"#,
            },
        },
    );
    h.collect();
    assert!(
        !h.requests.iter().any(|r| r.url.ends_with("/fapi/v2/order")),
        "a second -1022 does not resend again"
    );
    assert_eq!(terminals(&h, 7), 1);
}

/// 10. A malformed 2xx write is Unknown, never unclassified.
#[test]
fn a_malformed_write_answer_is_unknown() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}]"#);
    let create = h.take(Method::Post, "/fapi/v2/order");
    h.reply(create, r#"{"code":"0","data":{}}"#);
    assert_eq!(
        failure(&h, 7).and_then(|e| e.outcome),
        Some(Outcome::Unknown)
    );
}

/// 11 (Q1). A cancel batch continues past an Unknown leg and skips a child the
/// parent's ACK closed. The text stays native's; the outcome is Unknown with
/// the confirmed IDs as partial evidence.
#[test]
fn a_cancel_batch_continues_past_an_unknown_leg_and_skips_a_closed_child() {
    let mut h = Harness::new();
    h.order(0.0);
    let mut other = h.e.store().order(ORDER).unwrap().clone();
    other.id = "222".into();
    h.e.store_mut().upsert_order(other);
    let mut child = h.e.store().order(ORDER).unwrap().clone();
    child.id = "333".into();
    child.parent_id = Some(ORDER.into());
    child.kind = OrderType::StopLoss;
    h.e.store_mut().upsert_order(child);
    h.command(Command::CancelOrders {
        command: 7,
        ids: vec!["222".into(), ORDER.into(), "333".into()],
    });
    let first = h.take(Method::Post, "/fapi/v2/cancel");
    assert_eq!(body(&first)["orderId"], "222");
    fail(&mut h, first);
    let parent = h.take(Method::Post, "/fapi/v2/cancel");
    assert_eq!(body(&parent)["orderId"], ORDER);
    h.reply(
        parent,
        &format!(r#"{{"code":"0","data":{{"orderId":{ORDER}}}}}"#),
    );
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.ends_with("/fapi/v2/cancel")),
        "the parent's ACK closed its child"
    );
    let error = failure(&h, 7).expect("one terminal");
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(
        error.message.contains("canceledOrderIds"),
        "{}",
        error.message
    );
    assert_eq!(
        error.partial.as_ref().map(|p| p.order_ids.clone()),
        Some(vec![ORDER.to_owned()])
    );
    assert_eq!(terminals(&h, 7), 1);
}

/// 12. A failure after an accepted placement keeps the accepted ID.
#[test]
fn a_failure_after_an_accepted_placement_keeps_its_id() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000},{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":1990}]"#);
    let first = h.take(Method::Post, "/fapi/v2/order");
    h.reply(first, r#"{"code":"0","data":{"orderId":111}}"#);
    let second = h.take(Method::Post, "/fapi/v2/order");
    fail(&mut h, second);
    let error = failure(&h, 7).expect("one terminal");
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(
        error.partial.as_ref().map(|p| p.order_ids.clone()),
        Some(vec!["111".to_owned()])
    );
}

/// 13 (D4). The protective census is the job's own final Read: Primary-signed,
/// a clean one completes, a failed one is Unknown with the accepted IDs.
#[test]
fn the_protective_census_is_the_jobs_final_primary_read() {
    for clean in [true, false] {
        let mut h = keyed_with_read();
        h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"stopLoss":1800}]"#);
        let create = h.take(Method::Post, "/fapi/v2/order");
        h.requests.clear();
        h.reply(create, r#"{"code":"0","data":{"orderId":999}}"#);
        let census = h.take(Method::Get, "/fapi/v2/openOrders");
        assert_eq!(api_key(&census), "replay-key", "the census signs Primary");
        if clean {
            h.reply(census, r#"{"code":"0","data":[]}"#);
            assert_eq!(h.result(), Some("[]"));
        } else {
            h.reply(census, r#"{"code":"-1","msg":"denied"}"#);
            let error = failure(&h, 7).expect("a failed census");
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert_eq!(
                error.partial.as_ref().map(|p| p.order_ids.clone()),
                Some(vec!["999".to_owned()])
            );
        }
        assert_eq!(terminals(&h, 7), 1);
    }
}

/// 14. A lost protective create fences cancel-all until a Primary-signed
/// reconcile snapshot clears it.
#[test]
fn a_lost_protective_create_fences_cancel_all_until_a_primary_reconcile() {
    let mut h = keyed_with_read();
    while let Some(index) = h
        .requests
        .iter()
        .position(|r| r.url.contains("/fapi/v2/openOrders"))
    {
        let startup = h.requests.remove(index);
        h.reply(startup, r#"{"code":"0","data":[]}"#);
    }
    position(&mut h);
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"stop_market","side":"sell","amount":0.001,"price":1800}]"#);
    let create = h.take(Method::Post, "/fapi/v2/order");
    h.requests.clear();
    fail(&mut h, create);
    assert_eq!(
        failure(&h, 7).and_then(|e| e.outcome),
        Some(Outcome::Unknown)
    );
    h.command(Command::CancelSymbolOrders {
        command: 8,
        symbol: "E-BTC-USDT".into(),
    });
    assert!(failure(&h, 8).is_some_and(|e| e.message.contains("uncertain")));
    // Answer reads that predate the seal; the reconcile is Primary-signed.
    let reconcile = loop {
        let Some(read) = h
            .requests
            .iter()
            .find(|r| r.url.contains("/fapi/v2/openOrders"))
            .cloned()
        else {
            // The order queue paces its next read on a timer.
            h.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
            let (timer, at) = h.timers.remove(0);
            h.handle(at, Event::TimerFired { timer });
            h.collect();
            continue;
        };
        h.requests.retain(|r| r.id != read.id);
        if api_key(&read) == "replay-key" {
            break read;
        }
        h.reply(read, r#"{"code":"-1","msg":"busy"}"#);
    };
    h.reply(reconcile, r#"{"code":"0","data":[]}"#);
    h.command(Command::CancelSymbolOrders {
        command: 9,
        symbol: "E-BTC-USDT".into(),
    });
    assert!(
        failure(&h, 9).is_none(),
        "a clean reconcile releases cancel-all"
    );
}

/// 15. A 503 on a write restores its rows, gives one terminal, and the next
/// queued command proceeds.
#[test]
fn a_503_restores_rows_and_the_next_queued_command_proceeds() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}]"#);
    let create = h.take(Method::Post, "/fapi/v2/order");
    h.command(leverage(8));
    assert_eq!(level_edits(&h), 0, "parked behind the placement");
    h.e.handle(
        h.now,
        Event::HttpResponse {
            request: create.id,
            response: safe_cex_engine::Response {
                status: 503,
                headers: "",
                body: "Service Unavailable",
            },
        },
    );
    h.collect();
    let error = failure(&h, 7).expect("one terminal");
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_ne!(error.kind, Some(ErrorKind::InvalidInput));
    assert!(
        !h.e.store()
            .orders()
            .iter()
            .any(|o| o.status == OrderStatus::Creating),
        "Creating rows are restored"
    );
    assert_eq!(terminals(&h, 7), 1);
    assert_eq!(level_edits(&h), 1, "the next queued command proceeds");
}
