#![allow(dead_code)]
mod support;

use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, Notice};
use safe_cex_venue_okx::Okx;
use support::*;

fn stored(id: &str, kind: OrderType) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind,
        side: OrderSide::Sell,
        price: if kind == OrderType::TakeProfit {
            42000.
        } else {
            36000.
        },
        amount: 2.,
        filled: 0.,
        remaining: 0.02,
        reduce_only: true,
        close: None,
        client_order_id: None,
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 4.,
        trigger_type: Some(StopTriggerType::MarkPrice),
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

fn amend(e: &mut Engine<Okx>, id: &str, price: Option<f64>, amount: Option<f64>) {
    e.handle(
        5.,
        Event::Command(Command::AmendOrder {
            command: 20,
            id: id.into(),
            price,
            amount,
        }),
    );
}

fn post(out: &[Action], suffix: &str) -> Action {
    out.iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.ends_with(suffix)))
        .unwrap_or_else(|| panic!("missing {suffix}: {out:?}"))
        .clone()
}

fn body(a: &Action) -> serde_json::Value {
    let Action::Http(r) = a else {
        panic!("not HTTP")
    };
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}

#[test]
fn standalone_stop_and_take_profit_modify_in_place_preserving_execution_and_reference() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        for reference in [
            StopTriggerType::MarkPrice,
            StopTriggerType::LastPrice,
            StopTriggerType::IndexPrice,
        ] {
            for execution in [-1., 35900.] {
                let mut e = trading_engine();
                let id = if kind == OrderType::StopLoss {
                    "91_sl"
                } else {
                    "91_tp"
                };
                let mut o = stored(id, kind);
                o.trigger_type = Some(reference);
                o.extra = Some(OrderExtra {
                    conditional_order_price: Some(execution),
                    ..Default::default()
                });
                e.store_mut().upsert_order(o.clone());
                amend(&mut e, id, Some(o.price + 10.), Some(3.145));
                let out = prepared(&mut e, 5.);
                let request = post(&out, "/amend-algos");
                let b = body(&request);
                let prefix = if kind == OrderType::StopLoss {
                    "Sl"
                } else {
                    "Tp"
                };
                let other = if kind == OrderType::StopLoss {
                    "Tp"
                } else {
                    "Sl"
                };
                assert_eq!(b["algoId"], "91");
                assert_eq!(b["cxlOnFail"], false);
                assert_eq!(b["newSz"], "3.15");
                assert_eq!(b[format!("new{prefix}OrdPx")], execution.to_string());
                assert_eq!(
                    b[format!("new{prefix}TriggerPxType")],
                    match reference {
                        StopTriggerType::MarkPrice => "mark",
                        StopTriggerType::LastPrice => "last",
                        StopTriggerType::IndexPrice => "index",
                    }
                );
                assert!(b.get(format!("new{other}TriggerPx")).is_none());
                assert!(
                    !out.iter()
                        .any(|a| matches!(a, Action::Http(r) if r.url.contains("cancel")))
                );
                reply(
                    &mut e,
                    &request,
                    r#"{"code":"1","data":[{"algoId":"91","sCode":"51280","sMsg":"trigger refused"}]}"#,
                    6.,
                );
                let out = prepared(&mut e, 6.);
                let refusal = out
                    .iter()
                    .find_map(|a| match a {
                        Action::Notice(Notice::CommandFailed { command: 20, error }) => Some(error),
                        _ => None,
                    })
                    .expect("typed native amendment refusal");
                assert_eq!(refusal.venue_code.as_deref(), Some("51280"));
                assert_eq!(refusal.outcome, Some(safe_cex_engine::Outcome::Rejected));
                assert!(refusal.message.contains("trigger refused"));
                assert!(refusal.message.contains("your stop is unchanged"));
                assert!(!out.iter().any(
                    |a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get)
                ));
                assert_eq!(e.store().order(id), Some(&o));
            }
        }
    }
}

#[test]
fn only_definitive_native_amend_refusals_claim_the_stop_is_unchanged() {
    for (status, response, definitive) in [
        (200, r#"{"code":"51280","msg":"trigger refused"}"#, true),
        (200, r#"{"code":"50004","msg":"execution unknown"}"#, false),
        (200, r#"{"code":"50013","msg":"system busy"}"#, false),
        (503, r#"{"code":"50013","msg":"system busy"}"#, false),
    ] {
        let mut e = trading_engine();
        let o = stored("91_sl", OrderType::StopLoss);
        e.store_mut().upsert_order(o.clone());
        amend(&mut e, &o.id, Some(o.price + 10.), None);
        let out = prepared(&mut e, 5.);
        let Action::Http(request) = post(&out, "/amend-algos") else {
            unreachable!();
        };
        e.handle(
            6.,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status,
                    headers: "",
                    body: response,
                },
            },
        );
        let out = prepared(&mut e, 6.);
        let error = out
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 20, error }) => Some(error),
                _ => None,
            })
            .expect("native amendment failure reaches caller");
        assert_eq!(error.message.contains("your stop is unchanged"), definitive);
        assert_eq!(
            error.outcome,
            Some(if definitive {
                safe_cex_engine::Outcome::Rejected
            } else {
                safe_cex_engine::Outcome::Unknown
            })
        );
        if definitive {
            assert_eq!(error.venue_code.as_deref(), Some("51280"));
            assert!(error.message.contains("trigger refused"));
            assert_eq!(e.store().order(&o.id), Some(&o));
        }
        assert!(
            !out.iter().any(|a| matches!(a,
                Action::Http(r) if r.method != safe_cex_engine::Method::Get
            )),
            "native amendment refusal never places or restores another stop"
        );
    }
}

#[test]
fn full_position_amend_never_replaces_its_zero_size_sentinel_with_new_size() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        let mut e = trading_engine();
        let id = if kind == OrderType::StopLoss {
            "91_sl"
        } else {
            "91_tp"
        };
        let mut o = stored(id, kind);
        o.amount = 0.;
        o.remaining = 0.;
        e.store_mut().upsert_order(o.clone());
        amend(&mut e, id, Some(o.price + 10.), Some(3.));
        let out = prepared(&mut e, 5.);
        let b = body(&post(&out, "/amend-algos"));
        assert!(
            b.get("newSz").is_none(),
            "full-position sentinel is unchanged"
        );
        let execution = if kind == OrderType::StopLoss {
            "newSlOrdPx"
        } else {
            "newTpOrdPx"
        };
        assert!(
            b.get(execution).is_none(),
            "close-all stops permit trigger edits, not execution-price edits (51523)"
        );
        assert_eq!(b["cxlOnFail"], false);
    }
}

const TRAILING: &str = r#"{"instId":"BTC-USDT-SWAP","algoId":"91","ordType":"move_order_stop","state":"live","side":"sell","sz":"4","tdMode":"cross","posSide":"net","reduceOnly":"true","algoClOrdId":"OldTrail91","callbackSpread":"50","callbackRatio":"","activePx":"41000","moveTriggerPx":"39500","last":"40000"}"#;

fn native_engine(row: &str, kind: &str) -> Engine<Okx> {
    let mut e = engine(FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Default::default()
    });
    e.start(0.);
    let a = take_http(&mut prepared(&mut e, 0.));
    reply(&mut e, &a, &envelope(MARKET), 1.);
    let mut out = prepared(&mut e, 1.);
    let ticker = take_http(&mut out);
    reply(&mut e, &ticker, &envelope(TICKER), 2.);
    out.extend(prepared(&mut e, 2.));
    let config = out
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut e,
        config,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.,
    );
    let mut pending = prepared(&mut e, 3.);
    while let Some(a) = pending.pop() {
        let Action::Http(r) = &a else { continue };
        let rows = if r.url.contains(&format!("ordType={kind}")) {
            row
        } else if r.url.contains("account-position-risk") {
            r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#
        } else {
            ""
        };
        reply(&mut e, &a, &envelope(rows), 4.);
        pending.extend(prepared(&mut e, 4.));
    }
    e
}

#[test]
fn native_trigger_protection_uses_trigger_fields_instead_of_conditional_fields() {
    let row = r#"{"instId":"BTC-USDT-SWAP","algoId":"91","ordType":"trigger","state":"live","side":"sell","sz":"2","reduceOnly":"true","triggerPx":"36000","triggerPxType":"last","ordPx":"35900"}"#;
    let mut e = native_engine(row, "trigger");
    assert!(e.store().order("91_sl").is_some());
    amend(&mut e, "91_sl", Some(36500.), None);
    let out = prepared(&mut e, 5.);
    let b = body(&post(&out, "/amend-algos"));
    assert_eq!(b["newTriggerPx"], "36500");
    assert_eq!(b["newOrdPx"], "35900");
    assert_eq!(b["newTriggerPxType"], "last");
    assert!(b.get("newSlTriggerPx").is_none());
}

fn failed_replacement(out: &[Action]) -> &safe_cex_engine::VenueError {
    assert!(
        out.iter()
            .all(|a| !matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get)),
        "no resend, cancel, suffix or automatic restoration"
    );
    let error = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 20, error }) => Some(error),
            _ => None,
        })
        .expect("replacement failure is terminal");
    let elements = error.elements.as_ref().expect("shared element journal");
    assert_eq!(
        elements.len(),
        2,
        "cancel and replacement must both be journaled"
    );
    assert_eq!(
        elements[0].result,
        safe_cex_engine::write_plan::ElementResult::Applied {
            effect: safe_cex_engine::write_plan::AppliedEffect::Cancel,
            target_order_id: Some("91".into()),
            position_key: None,
        }
    );
    assert!(
        matches!(&elements[1].result, safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(error) } if error.outcome == Some(safe_cex_engine::Outcome::Rejected))
    );
    assert_eq!(
        error.protection,
        Some(safe_cex_engine::error::Protection::Unprotected)
    );
    error
}

#[test]
fn trailing_replacement_refusal_reports_failure_without_restoring() {
    let mut e = native_engine(TRAILING, "move_order_stop");
    let replace = replacement(&mut e, 6.);
    let payload = body(&replace);
    assert_eq!(payload["sz"], "5");
    assert_ne!(payload["algoClOrdId"], "OldTrail91");
    reply(
        &mut e,
        &replace,
        r#"{"code":"1","data":[{"sCode":"51280","sMsg":"replacement refused"}]}"#,
        7.,
    );
    let out = prepared(&mut e, 7.);
    assert!(
        failed_replacement(&out)
            .legacy_text()
            .contains("unprotected")
    );
    assert_eq!(
        e.store().order("91_tsl").expect("old stop").status,
        OrderStatus::Canceled
    );
    assert!(
        prepared(&mut e, 8.)
            .iter()
            .all(|a| !matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
}

fn held_cancel(e: &mut Engine<Okx>) -> safe_cex_engine::request_context::Permit {
    held_cancel_at(e, 5.)
}
fn held_cancel_at(e: &mut Engine<Okx>, now: f64) -> safe_cex_engine::request_context::Permit {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    e.prepare_dispatch(now);
    loop {
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.is_empty(), "cancel reaches its actual final Bind");
        for action in actions {
            let Action::ManagedHttp(action) = action else {
                continue;
            };
            match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(request.url.ends_with("/cancel-algos"));
                    e.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert!(request.url.ends_with("/cancel-algos"));
                    return permit;
                }
                _ => {}
            }
        }
    }
}

#[test]
fn final_cancel_claim_rechecks_freshness_source_and_size_with_a_working_boundary() {
    use safe_cex_engine::request_context::Claim;
    for case in ["working", "stale", "crossed", "terminal", "filled", "size"] {
        let mut e = native_engine(TRAILING, "move_order_stop");
        amend(&mut e, "91_tsl", Some(39940.), Some(5.));
        let permit = held_cancel(&mut e);
        let now = if case == "stale" { 5003. } else { 5002. };
        match case {
            "crossed" => e.store_mut().update_ticker("BTCUSDT", |t| t.last = 39499.),
            "terminal" => {
                e.store_mut()
                    .set_order_status("91_tsl", OrderStatus::Closed);
            }
            "filled" => {
                let mut row = e.store().order("91_tsl").unwrap().clone();
                row.filled = 1.;
                e.store_mut().upsert_order(row);
            }
            "size" => {
                let mut market = e.store().market("BTCUSDT").unwrap().clone();
                market.limits.amount.max = 4.;
                e.store_mut().set_markets(vec![market]);
            }
            _ => {}
        }
        let released = matches!(e.claim_http(now, permit), Ok(Claim::Released(_)));
        assert_eq!(released, case == "working", "final claim {case}");
        if case != "working" {
            let out = prepared(&mut e, now);
            assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.outcome == Some(safe_cex_engine::Outcome::NotSent))));
            assert!(
                !out.iter().any(
                    |a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get)
                )
            );
        }
    }
}

fn replacement(e: &mut Engine<Okx>, ack_at: f64) -> Action {
    amend(e, "91_tsl", Some(39940.), Some(5.));
    let cancel = post(&prepared(e, 5.), "/cancel-algos");
    reply(
        e,
        &cancel,
        &envelope(r#"{"algoId":"91","sCode":"0"}"#),
        ack_at,
    );
    post(&prepared(e, ack_at), "/order-algo")
}

#[test]
fn released_cancel_late_ack_still_places_and_does_not_recheck_the_expired_quote() {
    let mut e = native_engine(TRAILING, "move_order_stop");
    let replace = replacement(&mut e, 10000.);
    reply(
        &mut e,
        &replace,
        &envelope(r#"{"algoId":"92","sCode":"0"}"#),
        10001.,
    );
    let out = prepared(&mut e, 10001.);
    assert!(
        out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 20, .. })))
    );
}

#[test]
fn duplicate_replacement_refusal_cannot_trigger_restoration_or_second_terminal() {
    let mut e = native_engine(TRAILING, "move_order_stop");
    let replace = replacement(&mut e, 6.);
    let refusal = r#"{"code":"1","data":[{"sCode":"51280","sMsg":"trigger refused"}]}"#;
    reply(&mut e, &replace, refusal, 7.);
    let out = prepared(&mut e, 7.);
    failed_replacement(&out);
    reply(&mut e, &replace, refusal, 8.);
    assert!(prepared(&mut e, 8.).iter().all(
        |a| !matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get)
            && !matches!(a, Action::Notice(Notice::CommandFailed { command: 20, .. }))
    ));
}

#[test]
fn unknown_replacement_never_sends_another_stop_or_claims_protection() {
    let mut e = native_engine(TRAILING, "move_order_stop");
    let replace = replacement(&mut e, 6.);
    reply(
        &mut e,
        &replace,
        r#"{"code":"50004","msg":"unknown outcome"}"#,
        7.,
    );
    let out = prepared(&mut e, 7.);
    assert!(
        out.iter()
            .all(|a| !matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    let error = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 20, error }) => Some(error),
            _ => None,
        })
        .expect("unknown replacement terminal");
    assert!(error.protection.is_none());
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    let elements = error.elements.as_ref().expect("shared journal");
    assert_eq!(elements.len(), 2);
    assert!(matches!(&elements[0].result,
        safe_cex_engine::write_plan::ElementResult::Applied {
            effect: safe_cex_engine::write_plan::AppliedEffect::Cancel,
            target_order_id: Some(target), ..
        } if target == "91"));
    assert!(matches!(
        elements[1].result,
        safe_cex_engine::write_plan::ElementResult::Unknown { .. }
    ));
}

#[test]
fn amount_only_trailing_edit_preserves_ratio_and_activation_without_restoring() {
    let row = TRAILING.replace(
        "\"callbackSpread\":\"50\",\"callbackRatio\":\"\"",
        "\"callbackSpread\":\"\",\"callbackRatio\":\"0.005\"",
    );
    let mut e = native_engine(&row, "move_order_stop");
    amend(&mut e, "91_tsl", None, Some(5.));
    let cancel = post(&prepared(&mut e, 5.), "/cancel-algos");
    reply(
        &mut e,
        &cancel,
        &envelope(r#"{"algoId":"91","sCode":"0"}"#),
        6.,
    );
    let replace = post(&prepared(&mut e, 6.), "/order-algo");
    let b = body(&replace);
    assert_eq!(b["callbackRatio"], "0.005");
    assert_eq!(b["activePx"], "41000");
    assert!(b.get("callbackSpread").is_none());
    reply(
        &mut e,
        &replace,
        r#"{"code":"1","data":[{"sCode":"51280","sMsg":"trigger refused"}]}"#,
        7.,
    );
    let out = prepared(&mut e, 7.);
    failed_replacement(&out);
}

#[test]
fn refused_ratio_price_edit_does_not_restore_the_original_stop() {
    let row = TRAILING.replace(
        "\"callbackSpread\":\"50\",\"callbackRatio\":\"\"",
        "\"callbackSpread\":\"\",\"callbackRatio\":\"0.005\"",
    );
    let mut e = native_engine(&row, "move_order_stop");
    amend(&mut e, "91_tsl", Some(39600.), None);
    let cancel = post(&prepared(&mut e, 5.), "/cancel-algos");
    reply(
        &mut e,
        &cancel,
        &envelope(r#"{"algoId":"91","sCode":"0"}"#),
        6.,
    );
    let replace = post(&prepared(&mut e, 6.), "/order-algo");
    let replaced = body(&replace);
    assert_eq!(
        replaced["callbackRatio"], "0.01",
        "the price edit re-derives the ratio"
    );
    assert!(replaced.get("callbackSpread").is_none());
    reply(
        &mut e,
        &replace,
        r#"{"code":"1","data":[{"sCode":"51280","sMsg":"replacement refused"}]}"#,
        7.,
    );
    let out = prepared(&mut e, 7.);
    failed_replacement(&out);
}

#[test]
fn unproven_trailing_and_dust_refuse_before_any_cancel() {
    let mut e = trading_engine();
    e.store_mut()
        .upsert_order(stored("91_tsl", OrderType::TrailingStopLoss));
    amend(&mut e, "91_tsl", Some(39940.), Some(5.));
    let out = prepared(&mut e, 5.);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    assert_eq!(e.store().order("91_tsl").unwrap().status, OrderStatus::Open);
    let mut e = native_engine(TRAILING, "move_order_stop");
    amend(&mut e, "91_tsl", Some(39940.), Some(0.001));
    let out = prepared(&mut e, 5.);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    assert_eq!(e.store().order("91_tsl").unwrap().status, OrderStatus::Open);
}

#[test]
fn dependent_cancel_refusal_never_replaces_or_restores() {
    let mut e = native_engine(TRAILING, "move_order_stop");
    amend(&mut e, "91_tsl", Some(39940.), Some(5.));
    let cancel = post(&prepared(&mut e, 5.), "/cancel-algos");
    reply(
        &mut e,
        &cancel,
        r#"{"code":"1","data":[{"algoId":"91","sCode":"51400","sMsg":"gone"}]}"#,
        6.,
    );
    let out = prepared(&mut e, 6.);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
}

#[test]
fn concurrent_edit_cancel_and_cancel_all_cannot_overlap_replacement_custody() {
    for second in [
        Command::AmendOrder {
            command: 21,
            id: "91_tsl".into(),
            price: Some(39930.),
            amount: Some(5.),
        },
        Command::CancelOrders {
            command: 21,
            ids: vec!["91_tsl".into()],
        },
        Command::CancelSymbolOrders {
            command: 21,
            symbol: "BTCUSDT".into(),
        },
    ] {
        let mut e = native_engine(TRAILING, "move_order_stop");
        let replace = replacement(&mut e, 6.);
        e.handle(6., Event::Command(second));
        let out = prepared(&mut e, 6.);
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 21, .. })))
        );
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
        );
        reply(
            &mut e,
            &replace,
            r#"{"code":"1","data":[{"sCode":"51280","sMsg":"trigger refused"}]}"#,
            7.,
        );
        let out = prepared(&mut e, 7.);
        failed_replacement(&out);
        e.store_mut()
            .upsert_order(stored("92_tsl", OrderType::TrailingStopLoss));
        e.handle(
            9.,
            Event::Command(Command::CancelOrders {
                command: 22,
                ids: vec!["92_tsl".into()],
            }),
        );
        post(&prepared(&mut e, 9.), "/cancel-algos");
    }
}

fn frame_writer(order: Order) -> Engine<Okx> {
    let mut e = engine(FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Default::default()
    });
    e.set_frame_fed().unwrap();
    e.start(0.);
    let market = take_http(&mut prepared(&mut e, 0.));
    reply(&mut e, &market, &envelope(MARKET), 1.);
    let _ = prepared(&mut e, 1.);
    e.store_mut().set_orders(vec![order]);
    e.store_mut().set_positions(vec![]);
    e.store_mut().set_tickers(vec![Ticker {
        id: "BTC-USDT-SWAP".into(),
        symbol: "BTCUSDT".into(),
        last: 40000.,
        mark: 40000.,
        index: 40000.,
        ..Default::default()
    }]);
    let frame = e.snapshot_frame(2.).to_vec();
    e.apply_frame(2., &frame, false).unwrap();
    let _ = prepared(&mut e, 2.);
    e
}

fn source_read(e: &mut Engine<Okx>) -> Action {
    let out = prepared(e, 5.);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    out.into_iter().find(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get && r.url.ends_with("/api/v5/trade/order-algo?algoId=91"))).expect("bounded native provenance read before mutation")
}

#[test]
fn frame_fed_writer_reads_native_trailing_parameters_and_never_rolls_back() {
    let mut row = stored("91_tsl", OrderType::TrailingStopLoss);
    row.amount = 0.;
    row.remaining = 0.;
    let mut e = frame_writer(row);
    amend(&mut e, "91_tsl", Some(39940.), Some(5.));
    let read = source_read(&mut e);
    reply(&mut e, &read, &envelope(TRAILING), 6.);
    let cancel = post(&prepared(&mut e, 6.), "/cancel-algos");
    reply(
        &mut e,
        &cancel,
        &envelope(r#"{"algoId":"91","sCode":"0"}"#),
        7.,
    );
    let replace = post(&prepared(&mut e, 7.), "/order-algo");
    reply(
        &mut e,
        &replace,
        r#"{"code":"1","data":[{"sCode":"51280","sMsg":"replacement refused"}]}"#,
        8.,
    );
    let out = prepared(&mut e, 8.);
    failed_replacement(&out);
}

#[test]
fn frame_fed_writer_uses_the_authoritative_native_trigger_kind() {
    let mut o = stored("91_sl", OrderType::StopLoss);
    o.trigger_type = Some(StopTriggerType::LastPrice);
    o.extra = Some(OrderExtra {
        conditional_order_price: Some(35900.),
        ..Default::default()
    });
    let mut e = frame_writer(o);
    amend(&mut e, "91_sl", Some(36500.), None);
    let read = source_read(&mut e);
    reply(
        &mut e,
        &read,
        &envelope(
            r#"{"instId":"BTC-USDT-SWAP","algoId":"91","ordType":"trigger","state":"live","side":"sell","sz":"2","reduceOnly":"true","triggerPx":"36000","triggerPxType":"last","ordPx":"35900"}"#,
        ),
        6.,
    );
    let out = prepared(&mut e, 6.);
    let b = body(&post(&out, "/amend-algos"));
    assert_eq!(b["newTriggerPx"], "36500");
    assert_eq!(b["newOrdPx"], "35900");
    assert_eq!(b["cxlOnFail"], false);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("cancel")))
    );
}

#[test]
fn failed_foreign_or_raced_source_read_never_cancels_the_original_protection() {
    for case in [
        "refused",
        "foreign",
        "terminal",
        "duplicate",
        "raced",
        "timeout",
    ] {
        let mut row = stored("91_tsl", OrderType::TrailingStopLoss);
        row.amount = 0.;
        row.remaining = 0.;
        let mut e = frame_writer(row);
        amend(&mut e, "91_tsl", Some(39940.), Some(5.));
        let read = source_read(&mut e);
        let response = match case {
            "refused" => r#"{"code":"50113","msg":"signature refused"}"#.into(),
            "foreign" => envelope(&TRAILING.replace("\"algoId\":\"91\"", "\"algoId\":\"99\"")),
            "terminal" => {
                envelope(&TRAILING.replace("\"state\":\"live\"", "\"state\":\"canceled\""))
            }
            "duplicate" => envelope(&format!("{TRAILING},{TRAILING}")),
            _ => envelope(TRAILING),
        };
        if case == "raced" {
            e.store_mut()
                .set_order_status("91_tsl", OrderStatus::Canceled);
        }
        let now = if case == "timeout" { 20006. } else { 6. };
        reply(&mut e, &read, &response, now);
        let out = prepared(&mut e, now);
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get)),
            "{case}"
        );
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 20, .. }))),
            "{case}"
        );
        if case == "refused" {
            assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.kind == Some(safe_cex_engine::ErrorKind::AuthInvalid) && error.venue_code.as_deref() == Some("50113"))));
        }
        if case != "raced" {
            assert_eq!(e.store().order("91_tsl").unwrap().status, OrderStatus::Open);
        }
    }
}

fn quiet_lookup(frame_fed: bool) -> (Engine<Okx>, Action) {
    let mut e = if frame_fed {
        let mut order = stored("91_tsl", OrderType::TrailingStopLoss);
        order.amount = 0.;
        order.remaining = 0.;
        frame_writer(order)
    } else {
        native_engine(TRAILING, "move_order_stop")
    };
    e.handle(
        10000.,
        Event::Command(Command::AmendOrder {
            command: 20,
            id: "91_tsl".into(),
            price: Some(39940.),
            amount: Some(5.),
        }),
    );
    let out = prepared(&mut e, 10000.);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    let source = post(&out, "/order-algo?algoId=91");
    // Algo `last` is historical: it must never become the fresh cancellation price.
    reply(
        &mut e,
        &source,
        &envelope(&TRAILING.replace("\"last\":\"40000\"", "\"last\":\"100\"")),
        10001.,
    );
    let out = prepared(&mut e, 10001.);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    let quote = post(&out, "/market/ticker?instId=BTC-USDT-SWAP");
    (e, quote)
}

#[test]
fn quiet_native_and_frame_fed_markets_obtain_fresh_quote_before_cancel_without_republishing_it() {
    for frame_fed in [false, true] {
        let (mut e, quote) = quiet_lookup(frame_fed);
        let tickers = e.store().tickers().to_vec();
        reply(&mut e, &quote, &envelope(TICKER), 10002.);
        let cancel = post(&prepared(&mut e, 10002.), "/cancel-algos");
        assert_eq!(
            e.store().tickers(),
            tickers,
            "source writer alone owns market data"
        );
        reply(
            &mut e,
            &cancel,
            &envelope(r#"{"algoId":"91","sCode":"0"}"#),
            20000.,
        );
        let out = prepared(&mut e, 20000.);
        assert_eq!(
            body(&post(&out, "/order-algo"))["callbackSpread"],
            "60",
            "the level converts against the read quote, never the algo row's last"
        );
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Get)),
            "no optional read after cancel"
        );
    }
}

#[test]
fn quote_read_is_owner_bound_and_its_final_claim_freshness_has_an_exact_working_boundary() {
    use safe_cex_engine::request_context::Claim;
    for now in [15002., 15003.] {
        let (mut e, quote) = quiet_lookup(true);
        reply(&mut e, &quote, &envelope(TICKER), 10002.);
        let permit = held_cancel_at(&mut e, 10004.);
        let claim = e.claim_http(now, permit);
        let error = claim.as_ref().err().copied();
        let released = matches!(claim, Ok(Claim::Released(_)));
        assert_eq!(
            released,
            now == 15002.,
            "fresh actual ticker at {now}; error {error:?}; actions {:?}",
            e.drain_actions().collect::<Vec<_>>()
        );
    }
}

#[test]
fn failed_foreign_expired_or_firing_quote_keeps_the_trailing_stop() {
    for case in ["malformed", "foreign", "expired", "firing"] {
        let (mut e, quote) = quiet_lookup(true);
        let response = match case {
            "malformed" => envelope(r#"{"instId":"BTC-USDT-SWAP","last":"NaN"}"#),
            "foreign" => envelope(r#"{"instId":"ETH-USDT-SWAP","last":"40000"}"#),
            "firing" => envelope(r#"{"instId":"BTC-USDT-SWAP","last":"39499"}"#),
            _ => envelope(TICKER),
        };
        let now = if case == "expired" { 15002. } else { 10002. };
        reply(&mut e, &quote, &response, now);
        let out = prepared(&mut e, now);
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get)),
            "{case}"
        );
        assert_eq!(
            e.store().order("91_tsl").unwrap().status,
            OrderStatus::Open,
            "{case}"
        );
    }
}
