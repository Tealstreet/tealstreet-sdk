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
    // TS okx.exchange.ts:1751-1763,2291-2295: entry chunks precede original-amount algos.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | trading.rs:669 per-lot TP/SL child validation
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
    let entry_batches = input_count * 500;
    let calls = entry_batches + input_count * 2;
    let mut output = prepared(&mut engine, 5.);
    let mut next_id = 0;
    for call in 0..calls {
        let write = output
            .iter()
            .position(|a| {
                matches!(a,
                    Action::Http(r) if r.method == safe_cex_engine::Method::Post
                )
            })
            .expect("next native write");
        let Action::Http(request) = &output[write] else {
            unreachable!()
        };
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().expect("payload")).expect("JSON");
        let identities: Vec<_> = if call < entry_batches {
            assert!(request.url.ends_with("/batch-orders"));
            let rows = body.as_array().expect("entry batch");
            assert_eq!(rows.len(), 20);
            rows.iter()
                .map(|row| {
                    assert_eq!(row["sz"], "1");
                    row["clOrdId"].as_str().expect("client identity")
                })
                .collect()
        } else {
            assert!(request.url.ends_with("/order-algo"));
            assert_eq!(body["sz"], "10000");
            assert_eq!(body["reduceOnly"], true);
            if call - entry_batches < input_count {
                assert_eq!(body["slTriggerPx"], "36000");
            } else {
                assert_eq!(body["tpTriggerPx"], "42000");
            }
            vec![body["algoClOrdId"].as_str().expect("client identity")]
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
        assert_eq!(receipts.len(), identities.len());
        for (receipt, identity) in receipts.iter().zip(&identities) {
            assert_eq!(receipt.symbol, "BTCUSDT");
            assert_eq!(receipt.identity, *identity);
        }
        let replies: Vec<_> = identities
            .iter()
            .map(|_| {
                let field = if call < entry_batches {
                    "ordId"
                } else {
                    "algoId"
                };
                let row = serde_json::json!({field:next_id.to_string(),"sCode":"0"});
                next_id += 1;
                row
            })
            .collect();
        let now = 6. + f64::from(call);
        reply(
            &mut engine,
            &output[write],
            &serde_json::json!({"code":"0","data":replies}).to_string(),
            now,
        );
        output = prepared(&mut engine, now);
        assert!(!output.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: OWNER, .. })
        )));
        if call + 1 != calls {
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
    let ids: Vec<String> = serde_json::from_str(results[0]).expect("result IDs");
    assert_eq!(
        ids,
        (0..next_id).map(|id| id.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(next_id, input_count * 10_002);
    assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn max_bracketed_input_batches_all_entry_lots_before_full_amount_children() {
    complete_max_bracketed_inputs(1);
}

#[test]
fn two_max_bracketed_inputs_batch_entries_before_grouped_stops_and_tps() {
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
fn legacy_cancel_envelope_selects_continue_or_stop() {
    // TS okx.exchange.ts:2355-2363,2375-2383 continues row errors; okx.api.ts:77-82 throws nonzero envelopes.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | native cancellation runtime row refusal
    for code in ["0", "1", "2"] {
        for row_code in ["51000", "51400"] {
            for algo in [false, true] {
                let mut engine = trading_engine();
                let mut ids = Vec::new();
                for index in 1..=21 {
                    let id = if algo {
                        format!("{index}_sl")
                    } else {
                        index.to_string()
                    };
                    let mut row = stored(&id);
                    if algo {
                        row.kind = OrderType::StopLoss;
                    }
                    engine.store_mut().upsert_order(row);
                    ids.push(id);
                }
                engine.handle(
                    5.,
                    Event::Command(Command::CancelOrders {
                        command: OWNER,
                        ids,
                    }),
                );
                let first = prepared(&mut engine, 5.);
                let Action::Http(request) = post(&first) else {
                    panic!("one native cancel")
                };
                let payload: Vec<serde_json::Value> =
                    serde_json::from_str(request.body.as_deref().expect("cancel payload"))
                        .expect("cancel rows");
                let replies: Vec<_> = payload
                    .iter()
                    .map(|row| {
                        let mut reply = row.clone();
                        reply["sCode"] = serde_json::json!(row_code);
                        reply["sMsg"] = serde_json::json!("scripted refusal");
                        reply
                    })
                    .collect();
                reply(
                    &mut engine,
                    post(&first),
                    &serde_json::json!({"code":code,"msg":"refused","data":replies}).to_string(),
                    6.,
                );
                let mut sequence = vec![request.clone()];
                let mut pending = prepared(&mut engine, 6.);
                let mut output = pending.clone();
                let mut now = 7.;
                loop {
                    let next: Vec<_> = pending
                        .iter()
                        .filter_map(|action| match action {
                            Action::Http(request)
                                if request.method != safe_cex_engine::Method::Get =>
                            {
                                Some(request.clone())
                            }
                            _ => None,
                        })
                        .collect();
                    if next.is_empty() {
                        break;
                    }
                    assert_eq!(code, "0", "TS throws before later cancellation I/O");
                    let [request] = next.as_slice() else {
                        panic!("serial cancel")
                    };
                    sequence.push(request.clone());
                    let payload: Vec<serde_json::Value> =
                        serde_json::from_str(request.body.as_deref().expect("cancel payload"))
                            .expect("cancel rows");
                    let acks: Vec<_> = payload
                        .into_iter()
                        .map(|mut row| {
                            row["sCode"] = serde_json::json!("0");
                            row
                        })
                        .collect();
                    reply(
                        &mut engine,
                        &Action::Http(request.clone()),
                        &serde_json::json!({"code":"0","data":acks}).to_string(),
                        now,
                    );
                    pending = prepared(&mut engine, now);
                    output.extend(pending.iter().cloned());
                    now += 1.;
                    assert!(sequence.len() <= 3);
                }
                let field = if algo { "algoId" } else { "ordId" };
                let expected_path = if algo {
                    "/cancel-algos"
                } else {
                    "/cancel-batch-orders"
                };
                let mut sent_ids = Vec::new();
                let mut batch_sizes = Vec::new();
                for request in sequence {
                    assert!(request.url.ends_with(expected_path));
                    let rows: Vec<serde_json::Value> =
                        serde_json::from_str(request.body.as_deref().expect("cancel payload"))
                            .expect("cancel rows");
                    batch_sizes.push(rows.len());
                    sent_ids.extend(
                        rows.iter()
                            .map(|row| row[field].as_str().expect("target").to_owned()),
                    );
                }
                let expected_sizes = match (code, algo) {
                    ("0", false) => vec![20, 1],
                    ("0", true) => vec![10, 10, 1],
                    (_, false) => vec![20],
                    (_, true) => vec![10],
                };
                assert_eq!(batch_sizes, expected_sizes);
                let sent_count = if code == "0" {
                    21
                } else if algo {
                    10
                } else {
                    20
                };
                assert_eq!(
                    sent_ids,
                    (1..=sent_count)
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                );
                assert_eq!(
                    output
                        .iter()
                        .filter(|action| matches!(
                            action,
                            Action::Notice(
                                Notice::CommandDone { command: OWNER, .. }
                                    | Notice::CommandFailed { command: OWNER, .. }
                            )
                        ))
                        .count(),
                    1
                );
                if code != "0" {
                    assert!(output.iter().any(|action| matches!(
                        action,
                        Action::Notice(Notice::CommandFailed { command: OWNER, .. })
                    )));
                } else {
                    assert!(output.iter().any(|action| matches!(
                        action,
                        Action::Notice(Notice::CommandDone { command: OWNER, .. })
                    )));
                }
            }
        }
    }
}

#[test]
fn native_code_zero_all_absent_cancels_continue_then_finish_empty_without_errors() {
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
    let rows = (1..=20)
        .map(|id| format!(r#"{{"ordId":"{id}","sCode":"51400","sMsg":"Order does not exist"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    reply(
        &mut engine,
        post(&first),
        &format!(r#"{{"code":"0","data":[{rows}]}}"#),
        6.,
    );
    let second = prepared(&mut engine, 6.);
    assert_eq!(second.iter().filter(|a| matches!(a,
        Action::Notice(Notice::CommandError { command: OWNER, message }) if message.contains("51400")
    )).count(), 0);
    reply(
        &mut engine,
        post(&second),
        r#"{"code":"0","data":[{"ordId":"21","sCode":"51400","sMsg":"Order does not exist"}]}"#,
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
        0
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
    let rows = (1..=20)
        .map(|id| format!(r#"{{"ordId":"{id}","sCode":"51000","sMsg":"Parameter error"}}"#))
        .collect::<Vec<_>>()
        .join(",");
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
    let rows = (1..=20)
        .map(|id| format!(r#"{{"ordId":"{id}","sCode":"51000","sMsg":"Parameter error"}}"#))
        .collect::<Vec<_>>()
        .join(",");
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
fn a_native_stop_amend_refusal_never_prepares_a_replacement() {
    let mut engine = trading_engine();
    // Native amendment keeps the original stop; a refusal cannot initiate replacement.
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
    assert!(managed, "the native amendment is a managed Plan call");
    let cancel = post(&first);
    assert!(
        matches!(cancel, Action::Http(r) if r.url.ends_with("/amend-algos")),
        "the native amend goes first: {cancel:?}"
    );
    reply(
        &mut engine,
        cancel,
        r#"{"code":"1","msg":"All operations failed","data":[{"algoId":"stop","sCode":"51400","sMsg":"Order does not exist"}]}"#,
        6.,
    );
    let (output, replaced) = release(&mut engine, 6.);
    assert!(!replaced, "no replacement is prepared after the refusal");
    assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
    let errors = failed(&output, OWNER);
    assert_eq!(errors.len(), 1, "the amendment refusal ends the command");
    assert_eq!(
        engine.store().order("stop").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
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
    // The reconcile read itself fails: the fence holds and the retry is refused again.
    let read = |output: &[Action]| {
        output
            .iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("orders-pending")))
            .cloned()
            .expect("a fresh open-orders read")
    };
    let first_read = read(&sealed);
    refuse(
        &mut engine,
        &first_read,
        503,
        r#"{"code":"1","msg":"busy"}"#,
        8.,
    );
    let after = prepared(&mut engine, 8.);
    engine.handle(
        9.,
        Event::Command(Command::PlaceOrders {
            command: OWNER + 2,
            orders: vec![order(1)],
        }),
    );
    let refused = prepared(&mut engine, 9.);
    assert!(
        !refused
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post)),
        "a failed reconcile read does not release the fence"
    );
    assert!(
        failures(&refused)
            .iter()
            .any(|error| error.outcome == Some(safe_cex_engine::Outcome::NotSent))
    );
    // The engine re-asks the venue 1 s after the seal; that fresh read releases the fence.
    let timer = sealed
        .iter()
        .chain(&retry)
        .chain(&after)
        .chain(&refused)
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 1_006. => Some(*timer),
            _ => None,
        })
        .next_back()
        .expect("the engine re-ask is armed 1 s after the seal");
    engine.handle(1_006., Event::TimerFired { timer });
    // Every order kind's page answers empty; the complete clean read releases.
    let mut output = prepared(&mut engine, 1_006.);
    while let Some(page) = output
        .iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.contains("-pending")))
        .cloned()
    {
        reply(&mut engine, &page, &support::envelope(""), 1_007.);
        output = prepared(&mut engine, 1_007.);
    }
    engine.handle(
        1_008.,
        Event::Command(Command::PlaceOrders {
            command: OWNER + 3,
            orders: vec![order(1)],
        }),
    );
    let admitted = prepared(&mut engine, 1_008.);
    post(&admitted);
    assert!(failures(&admitted).is_empty(), "the placement is admitted");
}

#[test]
fn engine_re_asks_join_the_running_fresh_read_until_it_releases() {
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let first = prepared(&mut engine, 5.);
    let Action::Http(write) = post(&first) else {
        unreachable!()
    };
    engine.handle(
        6.,
        Event::HttpResponse {
            request: write.id,
            response: Response {
                status: 503,
                headers: "",
                body: r#"{"code":"1","msg":"busy"}"#,
            },
        },
    );
    let mut seen: Vec<Action> = prepared(&mut engine, 6.);
    let reads = |output: &[Action]| {
        output
            .iter()
            .filter(|a| matches!(a, Action::Http(r) if r.url.contains("-pending")))
            .count()
    };
    assert_eq!(reads(&seen), 1, "the seal starts one fresh order read");
    let held = seen
        .iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.contains("-pending")))
        .cloned()
        .unwrap();
    // The read is slow: the engine's 1 s and 2 s re-asks join it instead of replacing it.
    for at in [1_006., 3_006.] {
        let timer = seen
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
                _ => None,
            })
            .next_back()
            .expect("an engine re-ask is armed");
        engine.handle(at, Event::TimerFired { timer });
        let output = prepared(&mut engine, at);
        assert_eq!(
            reads(&output),
            0,
            "a re-ask does not restart the running read"
        );
        seen.extend(output);
    }
    // The held read completes clean, page by page, and releases the fence.
    let mut page = Some(held);
    while let Some(request) = page {
        reply(&mut engine, &request, &support::envelope(""), 3_100.);
        page = prepared(&mut engine, 3_100.)
            .into_iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("-pending")));
    }
    engine.handle(
        3_200.,
        Event::Command(Command::PlaceOrders {
            command: OWNER + 1,
            orders: vec![order(1)],
        }),
    );
    post(&prepared(&mut engine, 3_200.));
}

const SIGNATURE_REJECTED: &str = r#"{"code":"50113","msg":"Invalid Sign","data":[]}"#;

fn refuse(engine: &mut Engine<Okx>, action: &Action, status: u16, body: &str, now: f64) {
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
}

fn failures(output: &[Action]) -> Vec<&safe_cex_engine::VenueError> {
    output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: OWNER,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect()
}

#[test]
fn a_signature_rejected_write_is_re_signed_and_resent_once() {
    // TS okx.api.ts:66-75; utils/signatureRejection.ts:19-40.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | signature refusal retry
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let first = prepared(&mut engine, 5.);
    refuse(&mut engine, post(&first), 401, SIGNATURE_REJECTED, 6.);
    let second = prepared(&mut engine, 6.);
    assert!(
        failures(&second).is_empty(),
        "the resend decides the outcome"
    );
    let (Action::Http(a), Action::Http(b)) = (post(&first), post(&second)) else {
        unreachable!()
    };
    assert_ne!(a.id, b.id, "a fresh physical attempt");
    assert_eq!(a.body, b.body, "the same native write");
    assert_eq!(a.url, b.url);
    assert!(a.url.ends_with("/batch-orders"));
    reply(
        &mut engine,
        post(&second),
        r#"{"code":"0","msg":"","data":[{"ordId":"777","clOrdId":"","sCode":"0","sMsg":""}]}"#,
        7.,
    );
    let output = prepared(&mut engine, 7.);
    assert!(!output.iter().any(|action| matches!(action,
        Action::Http(request) if request.method != safe_cex_engine::Method::Get
    )));
    assert!(output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { command: OWNER, result }) if result.contains("777")
    )));
}

#[test]
fn a_second_signature_rejection_fails_without_a_third_send() {
    // TS utils/signatureRejection.ts:19-40 bounds the resend.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | signature refusal retry
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let first = prepared(&mut engine, 5.);
    refuse(&mut engine, post(&first), 401, SIGNATURE_REJECTED, 6.);
    let second = prepared(&mut engine, 6.);
    refuse(&mut engine, post(&second), 401, SIGNATURE_REJECTED, 7.);
    let output = prepared(&mut engine, 7.);
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post)),
        "no third send"
    );
    let errors = failures(&output);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::Rejected));
}

#[test]
fn other_key_refusals_are_never_resent() {
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let first = prepared(&mut engine, 5.);
    refuse(
        &mut engine,
        post(&first),
        401,
        r#"{"code":"50111","msg":"Invalid OK-ACCESS-KEY","data":[]}"#,
        6.,
    );
    let output = prepared(&mut engine, 6.);
    assert!(!output.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(failures(&output).len(), 1);
}

#[test]
fn a_signature_code_on_an_unproven_status_is_never_resent() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at utils/signatureRejection.ts:19-40.
    // A 5xx proves nothing about the write, so 50113 in its body is no refusal.
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let first = prepared(&mut engine, 5.);
    refuse(&mut engine, post(&first), 503, SIGNATURE_REJECTED, 6.);
    let output = prepared(&mut engine, 6.);
    // Reconciliation reads may follow; the write itself is never sent again.
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post)),
        "no resend of an unproven write"
    );
    let failed = failures(&output);
    assert_eq!(failed.len(), 1);
    support::assert_unknown_message(failed[0], support::ENGINE_UNKNOWN_TEXT);
    assert!(
        failed
            .iter()
            .all(|error| error.outcome == Some(safe_cex_engine::Outcome::Unknown)),
        "an unproven write stays Unknown: {failed:?}"
    );
}

#[test]
fn unknown_after_a_signature_refusal_keeps_the_engine_http_sealing_text() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at utils/signatureRejection.ts:19-40.
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let first = prepared(&mut engine, 5.);
    refuse(&mut engine, post(&first), 401, SIGNATURE_REJECTED, 6.);
    let second = prepared(&mut engine, 6.);
    refuse(&mut engine, post(&second), 503, SIGNATURE_REJECTED, 7.);
    let output = prepared(&mut engine, 7.);
    assert!(!output.iter().any(|action| matches!(action,
        Action::Http(request) if request.method != safe_cex_engine::Method::Get
    )));
    let errors = failures(&output);
    assert_eq!(errors.len(), 1);
    support::assert_unknown_message(errors[0], support::ENGINE_HTTP_UNKNOWN_TEXT);
    let elements = errors[0].elements.as_ref().expect("recorded unknown row");
    assert_eq!(elements.len(), 1);
    assert!(matches!(&elements[0].result,
        safe_cex_engine::write_plan::ElementResult::Unknown { error }
            if error.message.contains(support::ENGINE_UNKNOWN_TEXT)
    ));
}

#[test]
fn unknown_after_a_cancel_refusal_keeps_the_engine_refusal_sealing_text() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at okx.exchange.ts:2355-2363.
    let mut engine = trading_engine();
    let ids: Vec<_> = (1..=21).map(|id| id.to_string()).collect();
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
    let rows: Vec<_> = (1..=20)
        .map(|id| {
            serde_json::json!({
                "ordId":id.to_string(), "sCode":if id == 1 { "51000" } else { "0" },
                "sMsg":if id == 1 { "Parameter error" } else { "" }
            })
        })
        .collect();
    reply(
        &mut engine,
        post(&first),
        &serde_json::json!({"code":"0","data":rows}).to_string(),
        6.,
    );
    let second = prepared(&mut engine, 6.);
    refuse(
        &mut engine,
        post(&second),
        503,
        r#"{"code":"50001","msg":"busy"}"#,
        7.,
    );
    let output = prepared(&mut engine, 7.);
    assert!(!output.iter().any(|action| matches!(action,
        Action::Http(request) if request.method != safe_cex_engine::Method::Get
    )));
    let errors = failures(&output);
    assert_eq!(errors.len(), 1);
    support::assert_unknown_message(errors[0], support::ENGINE_UNKNOWN_AFTER_REFUSAL);
}

/// OKX's census runs each page under its own context. A mark made mid-census
/// restarts it at once, so pages sent before the mark never report a release;
/// the census also releases only marks its first page saw (`order_read_epoch`).
#[test]
fn a_census_begun_before_a_mark_cannot_release_it_from_a_later_page() {
    use support::{FixtureOptions, MARKET, TICKER, envelope, is_get, take_http};
    let eth = |row: &str| row.replace("BTC", "ETH");
    let mut e = support::engine(FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..FixtureOptions::default()
    });
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(
        &mut e,
        &a,
        &envelope(&format!("{MARKET},{}", eth(MARKET))),
        1.0,
    );
    let mut out = prepared(&mut e, 1.0);
    let a = take_http(&mut out);
    reply(
        &mut e,
        &a,
        &envelope(&format!("{TICKER},{}", eth(TICKER))),
        2.0,
    );
    out.extend(prepared(&mut e, 2.0));
    let a = out
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap()
        .clone();
    reply(
        &mut e,
        &a,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.0,
    );
    // Answer every account read; with `hold`, keep the first open-orders page in flight.
    let drain = |e: &mut Engine<Okx>, first: Vec<Action>, at, hold: bool| {
        let (mut held, mut pending) = (None, first);
        while let Some(a) = pending.pop() {
            let Action::Http(r) = &a else { continue };
            if hold && held.is_none() && r.url.contains("/trade/orders-pending") {
                held = Some(a);
                continue;
            }
            let body = if r.url.contains("account-position-risk") {
                envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
            } else {
                envelope("")
            };
            reply(e, &a, &body, at);
            pending.extend(prepared(e, at));
        }
        held
    };
    let first = prepared(&mut e, 3.0);
    drain(&mut e, first, 4.0, false);
    let place = |e: &mut Engine<Okx>, command, symbol: &str, at| {
        e.handle(
            at,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![
                    serde_json::from_str(&format!(
                        r#"{{"symbol":"{symbol}","type":"market","side":"buy","amount":1}}"#
                    ))
                    .unwrap(),
                ],
            }),
        );
        prepared(e, at)
    };
    let answer = |e: &mut Engine<Okx>, sent: &[Action], status, body: &str, at| {
        let Action::Http(write) = post(sent) else {
            unreachable!()
        };
        e.handle(
            at,
            Event::HttpResponse {
                request: write.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    };
    // A lost BTC placement seals Unknown; its reconcile starts a census at once.
    let sent = place(&mut e, OWNER, "BTCUSDT", 5.);
    answer(&mut e, &sent, 503, r#"{"code":"1","msg":"busy"}"#, 6.);
    let census = prepared(&mut e, 6.);
    let held = drain(&mut e, census, 6.5, true).expect("the census's first page is in flight");
    // A lost ETH placement mid-census seals Unknown too; its reconcile restarts
    // the census at once, so the running one (its first page sent before the
    // ETH write) can no longer report.
    let sent = place(&mut e, OWNER + 1, "ETHUSDT", 7.);
    answer(&mut e, &sent, 503, r#"{"code":"1","msg":"busy"}"#, 8.);
    let restarted = prepared(&mut e, 8.);
    let fresh = restarted
        .iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.contains("/trade/orders-pending")))
        .cloned()
        .expect("the census restarts");
    // The stale census's pages answer clean and release nothing.
    let mut page = Some(held);
    while let Some(request) = page {
        reply(&mut e, &request, &envelope(""), 9.);
        page = prepared(&mut e, 9.)
            .into_iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("-pending")));
    }
    let retry = |e: &mut Engine<Okx>, command, symbol, at| {
        place(e, command, symbol, at)
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post))
    };
    assert!(
        !retry(&mut e, OWNER + 2, "ETHUSDT", 10.),
        "a census begun before the ETH write released it"
    );
    // The restarted census, every page sent after both writes, releases both.
    let mut page = Some(fresh);
    while let Some(request) = page {
        reply(&mut e, &request, &envelope(""), 11.);
        page = prepared(&mut e, 11.)
            .into_iter()
            .find(|a| matches!(a, Action::Http(r) if r.url.contains("-pending")));
    }
    assert!(retry(&mut e, OWNER + 3, "ETHUSDT", 12.));
}

#[test]
fn a_stale_regular_row_in_code_one_stops_before_the_algo_cancels() {
    let mut engine = trading_engine();
    engine.store_mut().upsert_order(stored("101"));
    engine.store_mut().upsert_order(Order {
        kind: OrderType::StopLoss,
        ..stored("202_sl")
    });
    engine.handle(
        5.,
        Event::Command(Command::CancelOrders {
            command: OWNER,
            ids: vec!["101".into(), "202_sl".into()],
        }),
    );
    let first = prepared(&mut engine, 5.);
    reply(
        &mut engine,
        post(&first),
        r#"{"code":"1","data":[{"ordId":"101","sCode":"51400","sMsg":"Order does not exist"}],"msg":"All operations failed"}"#,
        6.,
    );
    let second = prepared(&mut engine, 6.);
    assert!(!second.iter().any(|action| matches!(action,
        Action::Http(request) if request.method != safe_cex_engine::Method::Get
    )));
    assert!(second.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandFailed { command: OWNER, .. })
    )));
}

// OKX answers an accepted row with a non-empty sMsg; only sCode decides.
#[test]
fn accepted_rows_with_a_success_message_complete_and_send_the_next_lots() {
    let mut engine = trading_engine();
    small_lots(&mut engine);
    place(&mut engine, vec![order(30)]);
    let mut output = prepared(&mut engine, 5.);
    for (batch, size) in [(0, 20), (1, 10)] {
        let rows: Vec<_> = (0..size)
            .map(|row| format!(r#"{{"ordId":"{batch}-{row}","sCode":"0","sMsg":"Order placed"}}"#))
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
    let done = output.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone {
            command: OWNER,
            result,
        }) => Some(result),
        _ => None,
    });
    let ids: Vec<String> = serde_json::from_str(done.expect("the command completes")).unwrap();
    assert_eq!(ids.len(), 30);
}

#[test]
fn a_refused_row_still_fails_whatever_its_message() {
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let output = prepared(&mut engine, 5.);
    reply(
        &mut engine,
        post(&output),
        r#"{"code":"1","data":[{"sCode":"51008","sMsg":"Order failed. Insufficient balance."}]}"#,
        6.,
    );
    let output = prepared(&mut engine, 6.);
    assert!(output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: OWNER, .. })
    )));
}

#[test]
fn a_cancel_success_without_an_id_is_unknown() {
    let mut engine = trading_engine();
    engine.store_mut().upsert_order(Order {
        kind: OrderType::StopLoss,
        ..stored("202_sl")
    });
    engine.handle(
        5.,
        Event::Command(Command::CancelOrders {
            command: OWNER,
            ids: vec!["202_sl".into()],
        }),
    );
    let first = prepared(&mut engine, 5.);
    reply(
        &mut engine,
        post(&first),
        r#"{"code":"0","msg":"","data":[{"algoId":"","sCode":"0","sMsg":""}]}"#,
        6.,
    );
    let output = prepared(&mut engine, 6.);
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { command: OWNER, .. })
    )));
    let errors = failed(&output, OWNER);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
}

#[test]
fn a_documented_refusal_is_rejected_not_unknown() {
    for code in [54000, 59000, 50002, 50006, 50009, 50027] {
        let mut engine = trading_engine();
        place(&mut engine, vec![order(1)]);
        let output = prepared(&mut engine, 5.);
        reply(
            &mut engine,
            post(&output),
            &format!(r#"{{"code":"{code}","msg":"refused","data":[]}}"#),
            6.,
        );
        let output = prepared(&mut engine, 6.);
        assert!(output.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: OWNER, error }) if error.outcome == Some(safe_cex_engine::Outcome::Rejected)
        )), "{code}");
    }
}

#[test]
fn engine_ambiguity_429_signature_must_not_resend() {
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let first = prepared(&mut engine, 5.);
    refuse(&mut engine, post(&first), 429, SIGNATURE_REJECTED, 6.);
    let output = prepared(&mut engine, 6.);
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post)),
        "429 physically resent: {:?}",
        output
    );
}
#[test]
fn engine_ambiguity_503_signature_working_control_never_resends() {
    let mut engine = trading_engine();
    place(&mut engine, vec![order(1)]);
    let first = prepared(&mut engine, 5.);
    refuse(&mut engine, post(&first), 503, SIGNATURE_REJECTED, 6.);
    let output = prepared(&mut engine, 6.);
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post))
    );
    assert!(
        failures(&output)
            .iter()
            .all(|e| e.outcome == Some(safe_cex_engine::Outcome::Unknown))
    );
}
