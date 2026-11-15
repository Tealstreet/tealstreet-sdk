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

#[test]
fn trailing_replacement_refusal_restores_exact_native_parameters_once() {
    let mut e = native_engine(TRAILING, "move_order_stop");
    assert!(e.store().order("91_tsl").is_some());
    amend(&mut e, "91_tsl", Some(39940.), Some(5.));
    let out = prepared(&mut e, 5.);
    let cancel = post(&out, "/cancel-algos");
    reply(
        &mut e,
        &cancel,
        &envelope(r#"{"algoId":"91","sCode":"0"}"#),
        6.,
    );
    let replacement = post(&prepared(&mut e, 6.), "/order-algo");
    reply(
        &mut e,
        &replacement,
        r#"{"code":"1","data":[{"sCode":"51280","sMsg":"replacement refused"}]}"#,
        7.,
    );
    let restore_out = prepared(&mut e, 7.);
    let restore = post(&restore_out, "/order-algo");
    let b = body(&restore);
    assert_eq!(b["sz"], "4");
    assert_eq!(b["callbackSpread"], "50");
    assert_eq!(b["activePx"], "41000");
    let restored_client_id = b["algoClOrdId"].as_str().unwrap();
    assert_ne!(restored_client_id, "OldTrail91");
    assert_ne!(b["algoClOrdId"], body(&replacement)["algoClOrdId"]);
    assert_eq!(restored_client_id.len(), 32);
    assert!(
        restored_client_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric())
    );
    let mut exact = b.clone();
    exact.as_object_mut().unwrap().remove("algoClOrdId");
    assert_eq!(
        exact,
        serde_json::json!({
            "instId":"BTC-USDT-SWAP", "tdMode":"cross", "side":"sell",
            "posSide":"net", "ordType":"move_order_stop", "sz":"4",
            "reduceOnly":true, "activePx":"41000", "callbackSpread":"50"
        }),
        "every original placement parameter except client identity is exact"
    );
    assert!(
        restore_out.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandProgress { command: 20, receipts })
            if receipts.iter().any(|r| r.identity == restored_client_id)
        )),
        "fresh restore identity is published before dispatch"
    );
    assert_eq!(b["posSide"], "net");
    reply(
        &mut e,
        &restore,
        &envelope(r#"{"algoId":"92","sCode":"0"}"#),
        8.,
    );
    let out = prepared(&mut e, 8.);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.protection == Some(safe_cex_engine::error::Protection::Restored))));
    let restored = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => error.partial.as_ref(),
            _ => None,
        })
        .expect("restored native order identity accompanies the failed edit");
    assert_eq!(
        restored.order_ids,
        ["92"],
        "only the newly restored native identity is created-order evidence"
    );
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
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
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts:1561-1562,2375-2383.
fn unknown_exception_protective_cancel_replace() {
    let mut e = native_engine(TRAILING, "move_order_stop");
    amend(&mut e, "91_tsl", Some(39940.), Some(5.));
    let cancel = post(&prepared(&mut e, 5.), "/cancel-algos");
    reply(
        &mut e,
        &cancel,
        &envelope(r#"{"algoId":"91","sCode":"51412","sMsg":"uncertain cancel"}"#),
        6.,
    );
    let mut out = prepared(&mut e, 6.);
    for now in [7., 30000.] {
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get)),
            "no replacement, retry or restore"
        );
        let failures: Vec<_> = out
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 20, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1);
        assert_unknown_message(failures[0], ENGINE_UNKNOWN_TEXT);
        assert!(failures[0].message.contains("91"));
        assert!(failures[0].partial.is_none());
        assert!(failures[0].protection.is_none());
        let events: Vec<_> = out
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                    Some(event)
                }
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].message, ENGINE_UNKNOWN_TEXT);
        assert!(events[0].fields.iter().any(
            |(key, value)| *key == "command" && *value == safe_cex_engine::log::Value::Num(20.)
        ));
        assert!(
            events[0]
                .fields
                .iter()
                .any(|(key, value)| *key == "placedCount"
                    && *value == safe_cex_engine::log::Value::Num(0.))
        );
        assert!(
            events[0]
                .fields
                .iter()
                .any(|(key, value)| *key == "unknownRequest"
                    && value.as_str().is_some_and(|s| s.contains("91")))
        );
        reply(
            &mut e,
            &cancel,
            &envelope(r#"{"algoId":"91","sCode":"0"}"#),
            now,
        );
        out.extend(prepared(&mut e, now));
    }
}

#[test]
fn released_cancel_late_ack_still_places_and_does_not_recheck_the_expired_quote() {
    // TS okx.exchange.ts:1561-1562 awaits cancellation before the replacement placement.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | protective cancel/re-place
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
fn refused_restore_is_typed_unprotected_and_never_attempted_twice() {
    let mut e = native_engine(TRAILING, "move_order_stop");
    let replace = replacement(&mut e, 6.);
    let refusal = r#"{"code":"1","data":[{"sCode":"51280","sMsg":"trigger refused"}]}"#;
    reply(&mut e, &replace, refusal, 7.);
    let restore = post(&prepared(&mut e, 7.), "/order-algo");
    reply(&mut e, &restore, refusal, 8.);
    let out = prepared(&mut e, 8.);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.protection == Some(safe_cex_engine::error::Protection::Unprotected) && error.legacy_text().contains("unprotected"))));
    assert_eq!(
        e.store().order("91_tsl").unwrap().status,
        OrderStatus::Canceled
    );
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    reply(&mut e, &restore, refusal, 9.);
    assert!(
        !prepared(&mut e, 9.)
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
}

#[test]
fn unknown_replacement_or_restore_never_sends_another_stop_or_claims_protection() {
    for during_restore in [false, true] {
        let mut e = native_engine(TRAILING, "move_order_stop");
        let replace = replacement(&mut e, 6.);
        let uncertain = if during_restore {
            reply(
                &mut e,
                &replace,
                r#"{"code":"1","data":[{"sCode":"51280","sMsg":"trigger refused"}]}"#,
                7.,
            );
            post(&prepared(&mut e, 7.), "/order-algo")
        } else {
            replace
        };
        reply(
            &mut e,
            &uncertain,
            r#"{"code":"50004","msg":"unknown outcome"}"#,
            8.,
        );
        let out = prepared(&mut e, 8.);
        assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.protection.is_none() && error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
        );
    }
}

#[test]
fn amount_only_trailing_edit_preserves_ratio_and_activation_and_restores_original_size() {
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
    let restore = body(&post(&prepared(&mut e, 7.), "/order-algo"));
    assert_eq!(restore["callbackRatio"], "0.005");
    assert_eq!(restore["activePx"], "41000");
    assert_eq!(restore["sz"], "4");
}

#[test]
fn refused_ratio_price_edit_restores_the_original_callback_ratio_exactly() {
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
    let mut restore = body(&post(&prepared(&mut e, 7.), "/order-algo"));
    restore.as_object_mut().unwrap().remove("algoClOrdId");
    assert_eq!(
        restore,
        serde_json::json!({
            "instId":"BTC-USDT-SWAP", "tdMode":"cross", "side":"sell",
            "posSide":"net", "ordType":"move_order_stop", "sz":"4",
            "reduceOnly":true, "activePx":"41000", "callbackRatio":"0.005"
        }),
        "the restore puts back the original ratio, never the edited one"
    );
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
    // TS okx.exchange.ts:1561-1562; okx.api.ts:77-82: thrown cancel prevents re-place.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: okx | protective cancel/re-place
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
fn concurrent_edit_cancel_and_cancel_all_cannot_overlap_restore_custody() {
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
        let restore = post(&prepared(&mut e, 7.), "/order-algo");
        reply(
            &mut e,
            &restore,
            &envelope(r#"{"algoId":"92","sCode":"0"}"#),
            8.,
        );
        let _ = prepared(&mut e, 8.);
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
fn frame_fed_writer_reads_native_trailing_parameters_before_cancel_and_can_restore() {
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
    let restore = body(&post(&prepared(&mut e, 8.), "/order-algo"));
    assert_eq!(restore["sz"], "4");
    assert_eq!(restore["callbackSpread"], "50");
    assert_eq!(restore["activePx"], "41000");
    assert_ne!(restore["algoClOrdId"], "OldTrail91");
    assert_ne!(restore["algoClOrdId"], body(&replace)["algoClOrdId"]);
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
