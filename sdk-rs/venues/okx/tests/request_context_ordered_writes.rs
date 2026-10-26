#![allow(dead_code)]
mod support;

use safe_cex_engine::model::{Interval, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts};
use safe_cex_engine::request_context::{HttpAction, Preparation};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Response};
use safe_cex_venue_okx::Okx;
use support::{prepared, reply, trading_engine};

const OWNER: u32 = 41;

fn order(amount: u32) -> PlaceOrderOpts {
    serde_json::from_str(&format!(
        r#"{{"symbol":"BTCUSDT","type":"market","side":"buy","amount":{amount}}}"#
    ))
    .unwrap()
}

fn stored(id: &str) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Sell,
        price: 38000.,
        amount: 1.,
        filled: 0.,
        remaining: 1.,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

fn place(engine: &mut Engine<Okx>, orders: Vec<PlaceOrderOpts>) {
    engine.handle(
        5.,
        Event::Command(Command::PlaceOrders {
            command: OWNER,
            orders,
        }),
    );
}

fn post(output: &[Action]) -> &Action {
    output
        .iter()
        .find(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post))
        .expect("native physical write")
}

fn small_lots(engine: &mut Engine<Okx>) {
    let mut markets = engine.store().markets().to_vec();
    markets[0].limits.amount.max = 1.;
    markets[0].precision.amount = 1.;
    engine.store_mut().set_markets(markets);
}

#[test]
fn write_requires_managed_route_preparation_before_provider_or_release() {
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let output: Vec<_> = engine.drain_actions().collect();
    assert!(
        output.iter().any(|a| matches!(a,
            Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. })
        )),
        "write must begin with genuine managed preparation"
    );
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
}

fn fill_capacity(engine: &mut Engine<Okx>) -> Vec<Preparation> {
    let mut held = Vec::new();
    for command in 1000..1600 {
        engine.handle(
            5.,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTCUSDT".into(),
                interval: Interval::M1,
                limit: Some(1),
                from_ms: None,
                to_ms: None,
            }),
        );
        let output: Vec<_> = engine.drain_actions().collect();
        if output.iter().any(|a| {
            matches!(a,
                Action::Notice(Notice::CommandFailed { command: id, error })
                    if *id == command && error.kind == Some(ErrorKind::Internal)
            )
        }) {
            assert!(!held.is_empty());
            return held;
        }
        held.push(
            output
                .iter()
                .find_map(|a| match a {
                    Action::ManagedHttp(action) => match action.as_ref() {
                        HttpAction::Prepare { preparation, .. } => Some(*preparation),
                        _ => None,
                    },
                    _ => None,
                })
                .expect("live peer preparation"),
        );
    }
    panic!("did not reach actual shared capacity")
}

#[test]
fn admission_capacity_refusal_precedes_optimistic_store_change() {
    let mut engine = trading_engine();
    engine.store_mut().upsert_order(stored("123"));
    let peers = fill_capacity(&mut engine);
    engine.handle(
        5.,
        Event::Command(Command::CancelOrders {
            command: OWNER,
            ids: vec!["123".into()],
        }),
    );
    let output: Vec<_> = engine.drain_actions().collect();
    assert!(peers.iter().all(|p| engine.http_preparation_pending(*p)));
    assert_eq!(
        engine.store().order("123").unwrap().status,
        OrderStatus::Open,
        "refused admission cannot publish optimistic Canceling"
    );
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: OWNER, error })
                    if error.kind == Some(ErrorKind::Internal)
                        && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
            ))
            .count(),
        1
    );
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::Sign { .. }
            | Action::Notice(Notice::CommandProgress { command: OWNER, .. })
    )));
}

#[test]
fn native_per_input_split_maximum_and_maximum_plus_one() {
    for amount in [10_000, 10_001] {
        let mut engine = trading_engine();
        small_lots(&mut engine);
        place(&mut engine, vec![order(amount)]);
        let output = prepared(&mut engine, 5.);
        if amount == 10_000 {
            let Action::Http(request) = post(&output) else {
                unreachable!()
            };
            let rows: Vec<serde_json::Value> =
                serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(rows.len(), 20);
            assert!(rows.iter().all(|row| row["sz"] == "1"));
        } else {
            assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
            assert!(output.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { command: OWNER, error })
                    if error.message == "order exceeds safe split bounds"
            )));
        }
    }
}

#[test]
fn native_split_bound_applies_per_input_not_to_whole_command() {
    let mut engine = trading_engine();
    small_lots(&mut engine);
    place(&mut engine, vec![order(10_000), order(10_000)]);
    let output = prepared(&mut engine, 5.);
    post(&output);
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: OWNER, .. })
    )));
}

fn complete_max_bracketed_inputs(input_count: u32) {
    let mut engine = trading_engine();
    small_lots(&mut engine);
    let orders = (0..input_count)
        .map(|_| {
            let mut input = order(10_000);
            input.stop_loss = Some(36000.);
            input.take_profit = Some(42000.);
            input
        })
        .collect();
    place(&mut engine, orders);
    let count = input_count * 30_000;
    let mut output = prepared(&mut engine, 5.);
    for step in 0..count {
        let write = output
            .iter()
            .position(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post))
            .expect("next native protective write");
        let Action::Http(request) = &output[write] else {
            unreachable!()
        };
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| *name == "OK-ACCESS-KEY" && value == "test-key")
        );
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        let identity = match step % 3 {
            0 => {
                assert!(request.url.ends_with("/batch-orders"));
                assert_eq!(body.as_array().unwrap().len(), 1);
                assert_eq!(body[0]["sz"], "1");
                body[0]["clOrdId"].as_str().unwrap()
            }
            index => {
                assert!(request.url.ends_with("/order-algo"));
                assert_eq!(body["sz"], "1");
                assert_eq!(body["reduceOnly"], true);
                let price = if index == 1 {
                    "slTriggerPx"
                } else {
                    "tpTriggerPx"
                };
                assert_eq!(body[price], if index == 1 { "36000" } else { "42000" });
                body["algoClOrdId"].as_str().unwrap()
            }
        };
        let receipts: Vec<_> = output[..write]
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandProgress {
                    command: OWNER,
                    receipts,
                }) => Some(receipts),
                _ => None,
            })
            .flatten()
            .collect();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].symbol, "BTCUSDT");
        assert_eq!(receipts[0].identity, identity);
        let field = if step % 3 == 0 { "ordId" } else { "algoId" };
        let response =
            format!(r#"{{"code":"0","data":[{{"{field}":"{step}","sCode":"0","sMsg":""}}]}}"#);
        let now = 6. + f64::from(step);
        reply(&mut engine, &output[write], &response, now);
        output = prepared(&mut engine, now);
        assert!(!output.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: OWNER, .. })
        )));
        if step + 1 != count {
            assert!(!output.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandDone { command: OWNER, .. })
            )));
        }
    }
    let results: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: OWNER,
                result,
            }) => Some(result),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 1);
    let ids: Vec<String> = serde_json::from_str(results[0]).unwrap();
    assert_eq!(ids.len(), count as usize);
    assert!(ids.iter().enumerate().all(|(i, id)| *id == i.to_string()));
    assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn native_max_bracketed_input_completes_all_thirty_thousand_steps() {
    complete_max_bracketed_inputs(1);
}

#[test]
fn native_two_max_bracketed_inputs_complete_all_sixty_thousand_steps() {
    complete_max_bracketed_inputs(2);
}

#[test]
fn native_maximum_plain_split_returns_all_ten_thousand_accepted_ids() {
    let mut engine = trading_engine();
    small_lots(&mut engine);
    place(&mut engine, vec![order(10_000)]);
    let mut output = prepared(&mut engine, 5.);
    for batch in 0..500 {
        let Action::Http(request) = post(&output) else {
            unreachable!()
        };
        let payload: Vec<serde_json::Value> =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.len(), 20);
        let rows: Vec<_> = (batch * 20..batch * 20 + 20)
            .map(|id| format!(r#"{{"ordId":"{id}","sCode":"0","sMsg":""}}"#))
            .collect();
        let now = 6. + f64::from(batch);
        reply(
            &mut engine,
            post(&output),
            &format!(r#"{{"code":"0","data":[{}]}}"#, rows.join(",")),
            now,
        );
        output = prepared(&mut engine, now);
        assert!(!output.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: OWNER, .. })
        )));
    }
    let results: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: OWNER,
                result,
            }) => Some(result),
            _ => None,
        })
        .collect();
    assert_eq!(results.len(), 1);
    let ids: Vec<String> = serde_json::from_str(results[0]).unwrap();
    assert_eq!(ids.len(), 10_000);
    assert!(ids.iter().enumerate().all(|(i, id)| *id == i.to_string()));
    assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn native_code_two_all_refused_cancels_report_each_row_continue_then_finish_empty() {
    let mut engine = trading_engine();
    let ids: Vec<_> = (1..=21).map(|i| i.to_string()).collect();
    for id in &ids {
        engine.store_mut().upsert_order(stored(id));
    }
    engine.handle(
        5.,
        Event::Command(Command::CancelOrders {
            command: OWNER,
            ids,
        }),
    );
    let first = prepared(&mut engine, 5.);
    let rows = [r#"{"sCode":"51400","sMsg":"Order does not exist"}"#; 20].join(",");
    reply(
        &mut engine,
        post(&first),
        &format!(r#"{{"code":"2","data":[{rows}]}}"#),
        6.,
    );
    let second = prepared(&mut engine, 6.);
    assert_eq!(second.iter().filter(|a| matches!(a,
        Action::Notice(Notice::CommandError { command: OWNER, message }) if message.contains("51400")
    )).count(), 20);
    reply(
        &mut engine,
        post(&second),
        r#"{"code":"2","data":[{"sCode":"51400","sMsg":"Order does not exist"}]}"#,
        7.,
    );
    let output = prepared(&mut engine, 7.);
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(
                a,
                Action::Notice(Notice::CommandError { command: OWNER, .. })
            ))
            .count(),
        1
    );
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(a,
                Action::Notice(Notice::CommandDone { command: OWNER, result }) if result == "[]"
            ))
            .count(),
        1
    );
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: OWNER, .. })
    )));
}

#[test]
fn native_code_one_all_refused_cancel_stops_and_restores_optimistic_status() {
    let mut engine = trading_engine();
    let ids: Vec<_> = (1..=21).map(|i| i.to_string()).collect();
    for id in &ids {
        engine.store_mut().upsert_order(stored(id));
    }
    engine.handle(
        5.,
        Event::Command(Command::CancelOrders {
            command: OWNER,
            ids: ids.clone(),
        }),
    );
    let first = prepared(&mut engine, 5.);
    assert!(
        ids.iter()
            .all(|id| engine.store().order(id).unwrap().status == OrderStatus::Canceling)
    );
    let rows = [r#"{"sCode":"51400","sMsg":"Order does not exist"}"#; 20].join(",");
    reply(
        &mut engine,
        post(&first),
        &format!(r#"{{"code":"1","msg":"All operations failed","data":[{rows}]}}"#),
        6.,
    );
    let output = prepared(&mut engine, 6.);
    assert!(
        ids.iter()
            .all(|id| engine.store().order(id).unwrap().status == OrderStatus::Open)
    );
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    let errors: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: OWNER,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::Rejected));
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { command: OWNER, .. })
    )));
}

#[test]
fn native_not_ready_refusal_precedes_invalid_planning_result() {
    let mut engine = support::engine(support::FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Default::default()
    });
    assert!(
        engine.store().market("BTCUSDT").is_none(),
        "planning would fail because the requested market is absent"
    );
    place(&mut engine, vec![order(1)]);
    let output = prepared(&mut engine, 5.);
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    let errors: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: OWNER,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "OKX account is not ready");
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
}

#[test]
fn native_bracket_continues_and_finishes_after_caller_receipt_deadline() {
    let mut engine = trading_engine();
    let mut input = order(1);
    input.stop_loss = Some(36000.);
    input.take_profit = Some(42000.);
    engine
        .handle_command(
            5.,
            Command::PlaceOrders {
                command: OWNER,
                orders: vec![input],
            },
            Some(1),
        )
        .expect("native command dispatch");
    let mut output = prepared(&mut engine, 5.);
    for step in 0..3 {
        let field = if step == 0 { "ordId" } else { "algoId" };
        let response =
            format!(r#"{{"code":"0","data":[{{"{field}":"{step}","sCode":"0","sMsg":""}}]}}"#);
        reply(&mut engine, post(&output), &response, 7.);
        output = prepared(&mut engine, 7.);
        assert!(!output.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: OWNER, .. })
        )));
    }
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(a,
                Action::Notice(Notice::CommandDone { command: OWNER, result })
                    if result == r#"["0","1","2"]"#
            ))
            .count(),
        1
    );
}

#[test]
fn native_keeps_duplicate_ids_and_releases_successors_without_step_wait() {
    let mut engine = trading_engine();
    let mut input = order(1);
    input.stop_loss = Some(36000.);
    input.take_profit = Some(42000.);
    place(&mut engine, vec![input]);
    let mut output = prepared(&mut engine, 5.);
    for step in 0..3 {
        let field = if step == 0 { "ordId" } else { "algoId" };
        let response =
            format!(r#"{{"code":"0","data":[{{"{field}":"repeated","sCode":"0","sMsg":""}}]}}"#);
        reply(&mut engine, post(&output), &response, 6.);
        output = prepared(&mut engine, 6.);
        if step < 2 {
            post(&output);
            assert!(
                !output.iter().any(|a| matches!(a,
                    Action::SetTimer { at_ms, .. } if *at_ms <= 6.
                )),
                "the next physical call released without firing a wait timer"
            );
        }
    }
    assert_eq!(
        output
            .iter()
            .filter(|a| matches!(a,
                Action::Notice(Notice::CommandDone { command: OWNER, result })
                    if result == r#"["repeated","repeated","repeated"]"#
            ))
            .count(),
        1
    );
}

#[test]
fn native_50004_stops_protective_chain_and_starts_reconciliation_without_retry() {
    let mut engine = trading_engine();
    let mut entry = order(1);
    entry.stop_loss = Some(36000.);
    entry.take_profit = Some(42000.);
    place(&mut engine, vec![entry]);
    let output = prepared(&mut engine, 5.);
    let Action::Http(request) = post(&output) else {
        unreachable!()
    };
    engine.handle(
        6.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":"50004","msg":"timeout","data":[]}"#,
            },
        },
    );
    let output = prepared(&mut engine, 6.);
    assert!(!output.iter().any(|a| matches!(a,
        Action::Http(r) if r.method == safe_cex_engine::Method::Post
    )));
    assert!(output.iter().any(|a| matches!(a,
        Action::Http(r) if r.url.contains("orders-pending")
    )));
    assert!(output.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandFailed { command: OWNER, error })
            if error.outcome == Some(safe_cex_engine::Outcome::Unknown)
    )));
}

// ---- Source-stage REDs (dazzy3, plan review of dbf746ef6bf) ----

/// Releases through the fixture host, recording whether any POST write was a
/// managed preparation rather than a legacy signed request.
fn release(engine: &mut Engine<Okx>, now: f64) -> (Vec<Action>, bool) {
    use safe_cex_engine::request_context::{Claim, Prepared};
    engine.prepare_dispatch(now);
    let (mut out, mut managed_write) = (Vec::new(), false);
    loop {
        let actions: Vec<_> = engine.drain_actions().collect();
        if actions.is_empty() {
            return (out, managed_write);
        }
        for action in actions {
            let Action::ManagedHttp(action) = action else {
                out.push(action);
                continue;
            };
            match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    managed_write |= request.method == safe_cex_engine::Method::Post;
                    let id = request.id.0;
                    let prepared = Prepared {
                        id,
                        expires_at: None,
                    };
                    engine.http_prepared(now, preparation, prepared);
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    if let Ok(Claim::Released(_)) = engine.claim_http(now, permit) {
                        out.push(Action::Http(request));
                    }
                }
                _ => {}
            }
        }
    }
}

fn failed(output: &[Action], command: u32) -> Vec<&safe_cex_engine::VenueError> {
    output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_managed_code_one_cancel_fails_the_plan_without_continuing() {
    let mut engine = trading_engine();
    let ids: Vec<_> = (1..=21).map(|i| i.to_string()).collect();
    for id in &ids {
        engine.store_mut().upsert_order(stored(id));
    }
    engine.handle(
        5.,
        Event::Command(Command::CancelOrders {
            command: OWNER,
            ids: ids.clone(),
        }),
    );
    let (first, managed) = release(&mut engine, 5.);
    assert!(managed, "the cancel batch is a managed Plan call");
    let rows = [r#"{"sCode":"51400","sMsg":"Order does not exist"}"#; 20].join(",");
    reply(
        &mut engine,
        post(&first),
        &format!(r#"{{"code":"1","msg":"All operations failed","data":[{rows}]}}"#),
        6.,
    );
    let (output, continued) = release(&mut engine, 6.);
    assert!(!continued, "code 1 never continues to the sibling batch");
    assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        ids.iter()
            .all(|id| engine.store().order(id).unwrap().status == OrderStatus::Open),
        "native optimistic status is restored"
    );
    let errors = failed(&output, OWNER);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::Rejected));
}

#[test]
fn a_dependent_replacement_never_runs_after_its_cancel_is_refused() {
    let mut engine = trading_engine();
    // A standalone stop with no limit price amends as cancel-then-replace.
    let mut stop = stored("stop");
    stop.kind = OrderType::StopLoss;
    engine.store_mut().upsert_order(stop);
    engine.handle(
        5.,
        Event::Command(Command::AmendOrder {
            command: OWNER,
            id: "stop".into(),
            price: Some(37000.),
            amount: None,
        }),
    );
    let (first, managed) = release(&mut engine, 5.);
    assert!(managed, "the prerequisite cancel is a managed Plan call");
    let cancel = post(&first);
    assert!(
        matches!(cancel, Action::Http(r) if r.url.contains("cancel")),
        "the cancel goes first: {cancel:?}"
    );
    reply(
        &mut engine,
        cancel,
        r#"{"code":"1","msg":"All operations failed","data":[{"sCode":"51400","sMsg":"Order does not exist"}]}"#,
        6.,
    );
    let (output, replaced) = release(&mut engine, 6.);
    assert!(!replaced, "no replacement is prepared after the refusal");
    assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
    let errors = failed(&output, OWNER);
    assert_eq!(errors.len(), 1, "the cancel's refusal ends the command");
}

#[test]
fn a_lost_create_fences_cancel_all_until_the_reconcile_snapshot() {
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let (first, managed) = release(&mut engine, 5.);
    assert!(managed, "the create is a managed Plan call");
    let Action::Http(write) = post(&first) else {
        unreachable!()
    };
    let write = write.id;
    engine.handle(
        6.,
        Event::HttpFailed {
            request: write,
            error: "lost",
        },
    );
    let (output, _) = release(&mut engine, 6.);
    assert_eq!(
        failed(&output, OWNER)[0].outcome,
        Some(safe_cex_engine::Outcome::Unknown)
    );
    let cancel_all = |engine: &mut Engine<Okx>, command, now| {
        engine.handle(
            now,
            Event::Command(Command::CancelSymbolOrders {
                command,
                symbol: "BTCUSDT".into(),
            }),
        );
        let (output, _) = release(engine, now);
        failed(&output, command).first().map(|e| e.message.clone())
    };
    // Answer the account reads issued after the loss, including the order
    // snapshot's successive pages, until none remain.
    let mut reads: Vec<_> = output
        .into_iter()
        .filter(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get))
        .collect();
    assert_eq!(
        cancel_all(&mut engine, 50, 7.).as_deref(),
        Some("Placement outcome is uncertain; reconcile before cancel-all")
    );
    let (later, _) = release(&mut engine, 8.);
    reads.extend(
        later
            .into_iter()
            .filter(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get)),
    );
    assert!(
        reads
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/trade/orders-pending"))),
        "a fresh reconcile read"
    );
    let mut now = 9.;
    while let Some(read) = reads.pop() {
        let body = if matches!(&read, Action::Http(r) if r.url.contains("account-position-risk")) {
            support::envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            support::envelope("")
        };
        reply(&mut engine, &read, &body, now);
        let (next, _) = release(&mut engine, now);
        reads.extend(
            next.into_iter().filter(
                |a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get),
            ),
        );
        now += 0.01;
    }
    assert_ne!(
        cancel_all(&mut engine, 51, 10.).as_deref(),
        Some("Placement outcome is uncertain; reconcile before cancel-all"),
        "the fresh snapshot clears the fence"
    );
}

#[test]
fn an_unknown_placement_refuses_a_retry_on_its_symbol_until_reconciled() {
    let refuse = |engine: &mut Engine<Okx>, action: &Action, status, body, now| {
        let Action::Http(r) = action else {
            panic!("expected request")
        };
        engine.handle(
            now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    };
    let failures = |output: &[Action]| -> Vec<safe_cex_engine::VenueError> {
        output
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error.clone()),
                _ => None,
            })
            .collect()
    };
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let first = prepared(&mut engine, 5.);
    // A lost 503 answer: the order may exist, so the placement seals Unknown.
    refuse(
        &mut engine,
        post(&first),
        503,
        r#"{"code":"1","msg":"busy"}"#,
        6.,
    );
    let sealed = prepared(&mut engine, 6.);
    assert!(
        failures(&sealed)
            .iter()
            .any(|error| error.outcome == Some(safe_cex_engine::Outcome::Unknown)),
        "the placement seals Unknown"
    );
    // A user retry on the same symbol must not send a second placement.
    engine.handle(
        7.,
        Event::Command(Command::PlaceOrders {
            command: OWNER + 1,
            orders: vec![order(1)],
        }),
    );
    let retry = prepared(&mut engine, 7.);
    assert!(
        !retry
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post)),
        "no second placement while the first is Unknown"
    );
    assert!(
        failures(&retry).iter().any(|error| {
            error.outcome == Some(safe_cex_engine::Outcome::NotSent)
                && error.legacy_text().contains("uncertain")
        }),
        "the retry is refused NotSent with the reason"
    );
}
