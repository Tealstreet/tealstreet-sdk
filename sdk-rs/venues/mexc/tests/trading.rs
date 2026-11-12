mod support;
use safe_cex_engine::model::{BybitBbo, OrderStatus, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, Notice, Response};
use support::*;
fn order() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":60,"price":59000}"#,
    )
    .unwrap()
}
fn reply(e: &mut TestEngine, r: &safe_cex_engine::HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn ready() -> TestEngine {
    let (mut e, _, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    e
}
/// Failures as the plain text the host renders.
fn drained(e: &mut TestEngine) -> Vec<Action> {
    e.drain_actions()
        .map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) => {
                Action::Notice(Notice::CommandFailed {
                    command,
                    error: error.clone(),
                })
            }
            a => a,
        })
        .collect()
}
fn failures(e: &mut TestEngine) -> Vec<safe_cex_engine::VenueError> {
    e.drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect()
}
fn results(e: &mut TestEngine) -> Vec<Result<String, String>> {
    e.drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(Ok(result)),
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(Err(error.legacy_text())),
            _ => None,
        })
        .collect()
}
#[test]
fn only_final_refreshed_batch_and_fallback_external_ids_are_receipted() {
    fn write(e: &mut TestEngine) -> safe_cex_engine::HttpRequest {
        let mut receipts = Vec::new();
        for action in e.drain_actions() {
            match action {
                Action::Notice(Notice::CommandProgress {
                    command: 91,
                    receipts: actual,
                }) => receipts.extend(actual),
                Action::Http(request) => {
                    let payload: serde_json::Value =
                        serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                    let rows = payload
                        .as_array()
                        .map_or_else(|| vec![&payload], |rows| rows.iter().collect());
                    let expected: Vec<_> = rows
                        .into_iter()
                        .map(|row| safe_cex_engine::OrderIdentityReceipt {
                            symbol: "BTCUSDT".into(),
                            kind: safe_cex_engine::OrderIdentityKind::ClientOrderId,
                            identity: row["externalOid"].as_str().unwrap().into(),
                        })
                        .collect();
                    assert_eq!(
                        receipts, expected,
                        "discarded preflight identities cannot stand in for the actual outgoing payload"
                    );
                    return request;
                }
                _ => {}
            }
        }
        panic!("placement write")
    }
    let mut e = ready();
    e.handle(
        200.,
        Event::Command(Command::PlaceOrders {
            command: 91,
            orders: vec![order(), order()],
        }),
    );
    let batch = write(&mut e);
    assert!(batch.url.ends_with("submit_batch"));
    reply(
        &mut e,
        &batch,
        250.,
        r#"{"success":false,"code":511,"message":"You do not have access to the interface"}"#,
    );
    for id in ["first", "second"] {
        let (timer, now) = e
            .drain_actions()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .unwrap();
        e.handle(now, Event::TimerFired { timer });
        let request = write(&mut e);
        assert!(request.url.ends_with("/submit"));
        reply(
            &mut e,
            &request,
            now + 1.,
            &format!(r#"{{"success":true,"code":0,"data":"{id}"}}"#),
        );
    }
}

#[test]
fn plan_payload_without_external_id_does_not_receive_a_fabricated_identity() {
    let mut e = ready();
    let mut stop = order();
    stop.kind = safe_cex_engine::model::OrderType::StopLoss;
    e.handle(
        200.,
        Event::Command(Command::PlaceOrders {
            command: 91,
            orders: vec![stop],
        }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(
        |action| matches!(action, Action::Http(request) if request.url.contains("planorder/place"))
    ));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Notice(Notice::CommandProgress { .. })))
    );
}

#[test]
fn bybit_execution_controls_reject_entire_batch_before_writes() {
    for bbo in [true, false] {
        let mut e = ready();
        let mut second = order();
        if bbo {
            second.bybit_bbo = Some(BybitBbo {
                side_type: "Queue".into(),
                level: 1.0,
            });
        } else {
            second.bybit_slippage_tolerance_ticks = Some(2.0);
        }
        e.handle(
            200.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![order(), second],
            }),
        );
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { error, .. }) if error.legacy_text().contains("Bybit")
        )));
    }
}

#[test]
fn every_caller_id_rejects_the_whole_batch_before_a_write() {
    for id in ["mine", ""] {
        let mut e = ready();
        let mut invalid = order();
        invalid.client_order_id = Some(id.into());
        e.handle(
            200.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![order(), invalid],
            }),
        );
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed { error, .. }) if error.legacy_text()=="mexc cannot honor a caller clientOrderId")));
    }
    assert!(serde_json::from_str::<PlaceOrderOpts>(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":60,"price":59000,"clientOrderId":null}"#).is_err());
}

#[test]
fn unsupported_opening_conditional_and_trailing_reject_before_any_batch_write() {
    use safe_cex_engine::model::OrderType;
    for kind in [OrderType::Conditional, OrderType::TrailingStopLoss] {
        for prefix in [false, true] {
            let mut e = ready();
            let mut invalid = order();
            invalid.kind = kind;
            invalid.trigger_price = Some(61000.0);
            let orders = if prefix {
                vec![order(), invalid]
            } else {
                vec![invalid]
            };
            e.handle(
                200.0,
                Event::Command(Command::PlaceOrders { command: 1, orders }),
            );
            let actions = drained(&mut e);
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
            assert!(e.store().orders().is_empty());
            assert!(actions.iter().any(|a| matches!(a,
                Action::Notice(Notice::CommandFailed { error, .. })
                if error.legacy_text() == "MEXC opening conditional and trailing-stop placement is not implemented"
            )));
        }
    }
}

#[test]
fn denied_batch_falls_back_once_and_paces_subsequent_writes() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(), order()],
        }),
    );
    let batch = requests(&mut e).remove(0);
    assert!(batch.url.ends_with("/submit_batch"));
    assert!(batch.body.as_ref().unwrap().contains("\"stpMode\":0"));
    assert!(batch.body.as_ref().unwrap().contains("\"vol\":60.0"));
    reply(
        &mut e,
        &batch,
        250.0,
        r#"{"success":false,"code":511,"message":"You do not have access to the interface"}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1700.0, Event::TimerFired { timer });
    let first = requests(&mut e).remove(0);
    assert!(first.url.ends_with("/submit"));
    reply(
        &mut e,
        &first,
        1800.0,
        r#"{"success":true,"code":0,"data":"one"}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(3200.0, Event::TimerFired { timer });
    let second = requests(&mut e).remove(0);
    reply(
        &mut e,
        &second,
        3300.0,
        r#"{"success":true,"code":0,"data":"two"}"#,
    );
    assert_eq!(results(&mut e), [Ok("[\"one\",\"two\"]".into())]);
    e.handle(
        5000.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![order()],
        }),
    );
    assert!(requests(&mut e)[0].url.ends_with("/submit"));
}

#[test]
fn lost_write_response_never_retries_and_finishes_the_command() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        }),
    );
    let r = requests(&mut e).remove(0);
    e.handle(
        300.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost after acceptance",
        },
    );
    let actions = drained(&mut e);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
    );
}

fn ambiguous_batch_never_falls_back(status: u16) {
    for body in [
        r#"{"success":false,"code":511,"message":"Batch API unavailable"}"#,
        r#"{"success":false,"code":406,"message":"You do not have access to the interface"}"#,
    ] {
        let mut e = ready();
        e.handle(
            200.0,
            Event::Command(Command::PlaceOrders {
                command: 91,
                orders: vec![order(), order()],
            }),
        );
        let batch = requests(&mut e).remove(0);
        assert!(batch.url.ends_with("/submit_batch"));
        e.handle(
            300.0,
            Event::HttpResponse {
                request: batch.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let mut actions = prepared(&mut e, 300.0);
        // Fire a fallback's pacing timer so the assertion sees physical sends.
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms <= 2000.0 => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        for (timer, at_ms) in timers {
            e.handle(at_ms, Event::TimerFired { timer });
            actions.extend(prepared(&mut e, at_ms));
        }
        assert!(
            !actions.iter().any(|action| matches!(action,
                Action::Http(request) if request.method == safe_cex_engine::Method::Post
                    && (request.url.ends_with("/submit") || request.url.ends_with("/submit_batch"))
            )),
            "HTTP {status} {body}: ambiguous batch resubmitted orders"
        );
        let errors: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1, "HTTP {status} {body}: {actions:?}");
        assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { command: 91, .. })
        )));

        e.handle(
            3000.0,
            Event::Command(Command::PlaceOrders {
                command: 92,
                orders: vec![order(), order()],
            }),
        );
        let next = requests(&mut e);
        assert_eq!(next.len(), 1);
        assert!(
            next[0].url.ends_with("/submit_batch"),
            "an ambiguous reply must not disable batching for later commands"
        );
    }
}

#[test]
fn ambiguous_batch_429_never_falls_back() {
    ambiguous_batch_never_falls_back(429);
}

#[test]
fn ambiguous_batch_408_never_falls_back() {
    ambiguous_batch_never_falls_back(408);
}

#[test]
fn ambiguous_batch_5xx_never_falls_back() {
    for status in [500, 503] {
        ambiguous_batch_never_falls_back(status);
    }
}

#[test]
fn late_placement_response_preserves_the_newer_stream_order() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        }),
    );
    let r = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":10,"price":59000,"state":2}}"#,
    );
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    assert_eq!(e.store().order("one").unwrap().filled, 0.001);
}

#[test]
fn partial_batch_success_and_cancel_failures_preserve_known_venue_state() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(), order()],
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0},{"errorCode":600,"errorMsg":"bad volume"}]}"#,
    );
    let failure = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("partial batch failure");
    let partial = failure.partial.as_deref().expect("accepted ids");
    assert_eq!(partial.order_ids, ["one"]);
    assert_eq!((partial.total, partial.failed_index), (Some(2), Some(1)));
    assert!(failure.message.contains("bad volume"), "{failure:?}");
    assert!(
        failure
            .legacy_text()
            .contains("\"partialOrderIds\":[\"one\"]")
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
    e.handle(
        400.0,
        Event::Command(Command::CancelOrders {
            command: 2,
            ids: vec!["one".into()],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert_eq!(r.body.as_deref(), Some("[\"one\"]"));
    reply(
        &mut e,
        &r,
        500.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":600,"errorMsg":"cannot cancel"}]}"#,
    );
    assert!(results(&mut e)[0].is_err());
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
}

#[test]
fn leverage_updates_use_native_position_id_and_ignore_failed_envelopes() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 10.0,
            side: None,
        }),
    );
    let first = requests(&mut e).remove(0);
    assert!(first.body.as_ref().unwrap().contains("\"positionType\":1"));
    reply(
        &mut e,
        &first,
        300.0,
        r#"{"success":false,"code":600,"message":"rejected"}"#,
    );
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    assert!(results(&mut e)[0].is_err());
}

#[test]
fn plan_orders_use_seven_day_market_triggers_and_distinct_cancel_routes() {
    let mut e = ready();
    let mut opts = order();
    opts.kind = safe_cex_engine::model::OrderType::StopLoss;
    opts.side = safe_cex_engine::model::OrderSide::Sell;
    opts.price = Some(55000.0);
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/planorder/place/v2"));
    let body = r.body.as_ref().unwrap();
    assert!(body.contains("\"executeCycle\":2"));
    assert!(body.contains("\"side\":4"));
    assert!(body.contains("\"triggerType\":2"));
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":"plan"}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        400.0,
        Event::Command(Command::CancelOrders {
            command: 2,
            ids: vec!["plan".into()],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/planorder/cancel"));
    assert_eq!(
        r.body.as_deref(),
        Some("[{\"symbol\":\"BTC_USDT\",\"orderId\":\"plan\"}]")
    );
}

#[test]
fn queued_margin_change_retains_the_leverage_accepted_before_it() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 10.0,
            side: None,
        }),
    );
    let first = requests(&mut e).remove(0);
    e.handle(
        210.0,
        Event::Command(Command::SetMarginType {
            command: 2,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    assert!(requests(&mut e).is_empty());
    reply(&mut e, &first, 300.0, r#"{"success":true,"code":0}"#);
    let second = requests(&mut e).remove(0);
    reply(&mut e, &second, 400.0, r#"{"success":true,"code":0}"#);
    let margin = requests(&mut e).remove(0);
    let body = margin.body.unwrap();
    assert!(body.contains("\"leverage\":10.0"));
    assert!(body.contains("\"openType\":1"));
}

#[test]
fn mutation_ack_does_not_overwrite_newer_mode_or_position_stream_state() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    e.handle(
        200.0,
        Event::Command(Command::SetPositionMode {
            command: 1,
            hedged: false,
            symbol: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.position.mode","data":{"positionMode":1}}"#,
    );
    reply(&mut e, &r, 300.0, r#"{"success":true,"code":0}"#);
    assert!(e.store().hedged());
    e.drain_actions().for_each(drop);
    e.handle(
        400.0,
        Event::Command(Command::SetLeverage {
            command: 2,
            symbol: "BTCUSDT".into(),
            leverage: 10.0,
            side: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.position","data":{"positionId":"native","symbol":"BTC_USDT","positionType":1,"positionMode":1,"holdVol":60,"holdAvgPrice":60000,"leverage":7,"openType":1}}"#,
    );
    reply(&mut e, &r, 500.0, r#"{"success":true,"code":0}"#);
    assert_eq!(e.store().positions()[0].leverage, 7.0);
}

#[test]
fn sized_stop_chooses_the_closing_position_rail_without_reduce_only_flag() {
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::FetchLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":[{"positionType":1,"leverage":10,"openType":2},{"positionType":2,"leverage":15,"openType":2}]}"#,
    );
    e.drain_actions().for_each(drop);
    let mut opts = order();
    opts.kind = safe_cex_engine::model::OrderType::StopLoss;
    opts.side = safe_cex_engine::model::OrderSide::Sell;
    opts.price = Some(55000.0);
    e.handle(
        400.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![opts],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.body.unwrap().contains("\"leverage\":10.0"));
}

#[test]
fn amend_recomputes_remaining_quantity_after_fills_during_cancel() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let original = r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":10,"price":59000,"state":2}}"#;
    ws(&mut e, socket, original);
    e.drain_actions().for_each(drop);
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        &original.replace("\"dealVol\":10", "\"dealVol\":30"),
    );
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    assert!(read.url.ends_with("/order/get/one"));
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":30,"price":59000,"state":4}}"#,
    );
    let replace = requests(&mut e).remove(0);
    assert!(replace.body.unwrap().contains("\"vol\":30.0"));
}

#[test]
fn stale_terminal_amend_read_cannot_erase_a_newer_partial_fill_or_replace_it() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let original = r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":10,"price":59000,"state":2}}"#;
    ws(&mut e, socket, original);
    e.drain_actions().for_each(drop);
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        &original.replace("\"dealVol\":10", "\"dealVol\":40"),
    );
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":30,"price":59000,"state":4}}"#,
    );
    assert!((e.store().order("one").unwrap().filled - 0.004).abs() < 1e-12);
    let actions = drained(&mut e);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed { error: e, .. }) if e.legacy_text().contains("Stale"))));
}

#[test]
fn filled_order_is_neither_replaced_nor_changed_to_canceled_by_cancel_ack() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let original = r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#;
    ws(&mut e, socket, original);
    e.drain_actions().for_each(drop);
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        &original
            .replace("\"dealVol\":0", "\"dealVol\":60")
            .replace("\"state\":2", "\"state\":3"),
    );
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    let actions = drained(&mut e);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
}

/// Below one contract is refused, naming the minimum, instead of silently becoming one.
#[test]
fn a_size_below_one_contract_is_refused_not_rounded_up() {
    for (command, amount) in [(1, 0.1), (2, 0.02)] {
        let mut e = ready();
        let mut opts = order();
        opts.amount = amount;
        e.handle(
            200.0,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![opts],
            }),
        );
        let actions = prepared(&mut e, 200.0);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "{amount}"
        );
        assert!(
            actions.iter().any(|a| format!("{a:?}")
                .contains("MEXC order size is below the 1 contract minimum (0.0001 BTC)")),
            "{actions:?}"
        );
    }
}

/// Whole contracts pass through, and half a contract or more rounds to the nearest one.
#[test]
fn contract_sizes_round_to_the_nearest_whole_contract() {
    for (amount, vol) in [(200.0, "\"vol\":200.0"), (0.6, "\"vol\":1.0")] {
        let mut e = ready();
        let mut opts = order();
        opts.amount = amount;
        e.handle(
            200.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![opts],
            }),
        );
        let r = requests(&mut e).remove(0);
        assert!(r.body.as_ref().unwrap().contains(vol), "{:?}", r.body);
        reply(
            &mut e,
            &r,
            300.0,
            r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
        );
    }
}

/// A reduce-only order below one contract gets the same minimum refusal.
#[test]
fn a_reduce_only_order_below_one_contract_is_refused() {
    let mut e = with_long_position();
    let mut opts: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"market","side":"sell","amount":0.02,"reduceOnly":true}"#,
    )
    .unwrap();
    opts.reduce_only = Some(true);
    e.handle(
        400.0,
        Event::Command(Command::PlaceOrders {
            command: 9,
            orders: vec![opts],
        }),
    );
    let actions = prepared(&mut e, 400.0);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| format!("{a:?}").contains("below the 1 contract minimum")),
        "{actions:?}"
    );
}

/// MEXC's stop placement answers per row; a row with `errorCode` placed nothing.
#[test]
fn a_stop_row_with_an_error_code_is_a_failure_not_order_zero() {
    let (mut e, _, reads, _) = private_started();
    for r in reads {
        let body = if r.url.contains("open_positions") {
            r#"{"success":true,"code":0,"data":[{"positionId":739113577038255616,"symbol":"BTC_USDT","positionType":1,"positionMode":2,"holdVol":60,"holdAvgPrice":60000,"leverage":10,"openType":1}]}"#
        } else {
            r#"{"success":true,"code":0,"data":[]}"#
        };
        response(&mut e, &r, body);
    }
    e.drain_actions().for_each(drop);
    let mut opts = order();
    opts.kind = safe_cex_engine::model::OrderType::StopLoss;
    opts.side = safe_cex_engine::model::OrderSide::Sell;
    opts.amount = 0.0;
    opts.price = Some(55000.0);
    e.handle(
        500.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![opts],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/stoporder/place"));
    reply(
        &mut e,
        &r,
        600.0,
        r#"{"success":true,"code":0,"message":"","data":[{"id":0,"symbol":"BTC_USDT","orderId":0,"errorCode":2009}]}"#,
    );
    let out: Vec<_> = e.drain_actions().collect();
    assert!(e.store().order("0").is_none());
    assert!(
        out.iter().any(|a| matches!(
            a,
            safe_cex_engine::Action::Notice(safe_cex_engine::Notice::CommandFailed {
                command: 2,
                ..
            })
        )),
        "{out:?}"
    );
}

#[test]
fn open_position_writes_keep_native_ids_and_full_stops_have_their_own_route() {
    let (mut e, _, reads, _) = private_started();
    for r in reads {
        let body = if r.url.contains("open_positions") {
            r#"{"success":true,"code":0,"data":[{"positionId":739113577038255616,"symbol":"BTC_USDT","positionType":1,"positionMode":2,"holdVol":60,"holdAvgPrice":60000,"leverage":10,"openType":1}]}"#
        } else {
            r#"{"success":true,"code":0,"data":[]}"#
        };
        response(&mut e, &r, body);
    }
    e.drain_actions().for_each(drop);
    e.handle(
        200.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    assert_eq!(
        r.body.as_deref(),
        Some("{\"positionId\":\"739113577038255616\",\"leverage\":5.0}")
    );
    reply(&mut e, &r, 300.0, r#"{"success":true,"code":0}"#);
    let r = requests(&mut e).remove(0);
    reply(&mut e, &r, 400.0, r#"{"success":true,"code":0}"#);
    e.drain_actions().for_each(drop);
    let mut opts = order();
    opts.kind = safe_cex_engine::model::OrderType::StopLoss;
    opts.side = safe_cex_engine::model::OrderSide::Sell;
    opts.amount = 0.0;
    opts.price = Some(55000.0);
    e.handle(
        500.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![opts],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/stoporder/place"));
    let body = r.body.as_ref().unwrap();
    assert!(body.contains("\"positionId\":\"739113577038255616\""));
    assert!(body.contains("\"vol\":60.0"));
    assert!(body.contains("\"volType\":2"));
    reply(
        &mut e,
        &r,
        600.0,
        r#"{"success":true,"code":0,"data":[{"id":"full"}]}"#,
    );
    e.drain_actions().for_each(drop);
    assert_eq!(e.store().order("full").unwrap().amount, 0.0);
    e.handle(
        700.0,
        Event::Command(Command::CancelOrders {
            command: 3,
            ids: vec!["full".into()],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("/stoporder/cancel"));
    assert_eq!(r.body.as_deref(), Some("[{\"stopPlanOrderId\":\"full\"}]"));
}

#[test]
fn amend_reads_final_quantity_when_fill_frames_arrive_after_cancel_response() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#,
    );
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    assert_eq!(read.method, safe_cex_engine::Method::Get);
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":30,"price":59000,"state":4}}"#,
    );
    let replacement = requests(&mut e).remove(0);
    assert_eq!(replacement.method, safe_cex_engine::Method::Post);
    assert!(replacement.body.unwrap().contains("\"vol\":30.0"));
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":30,"price":59000,"state":4}}"#,
    );
}

#[test]
fn short_stream_update_does_not_suppress_the_long_leverage_acknowledgement() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let long = r#"{"channel":"push.personal.position","data":{"positionId":"long","symbol":"BTC_USDT","positionType":1,"positionMode":1,"holdVol":60,"holdAvgPrice":60000,"leverage":10,"openType":1}}"#;
    ws(&mut e, socket, long);
    e.handle(
        300.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        &long
            .replace("\"long\"", "\"short\"")
            .replace("\"positionType\":1", "\"positionType\":2")
            .replace("\"leverage\":10", "\"leverage\":7"),
    );
    reply(&mut e, &r, 400.0, r#"{"success":true,"code":0}"#);
    assert_eq!(
        e.store()
            .positions()
            .iter()
            .find(|p| p.side == safe_cex_engine::model::PositionSide::Long)
            .unwrap()
            .leverage,
        5.0
    );
}

#[test]
fn definitive_filled_amend_read_updates_local_state_and_withholds_replacement() {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#,
    );
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":60,"price":59000,"state":3}}"#,
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    assert_eq!(e.store().order("one").unwrap().filled, 0.006);
    let actions = drained(&mut e);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn amend_retains_attached_bracket_prices_and_independent_sources_across_lifecycles() {
    use safe_cex_engine::model::StopTriggerType;
    for lifecycle in 0..3 {
        let (mut e, socket, reads, _) = private_started();
        for r in reads {
            response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
        }
        e.drain_actions().for_each(drop);
        let raw = r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#;
        if lifecycle < 2 {
            let mut opts = order();
            opts.stop_loss = Some(55000.0);
            opts.take_profit = Some(65000.0);
            opts.tp_trigger_type = Some(StopTriggerType::MarkPrice);
            opts.sl_trigger_type = Some(StopTriggerType::IndexPrice);
            e.handle(
                200.0,
                Event::Command(Command::PlaceOrders {
                    command: 1,
                    orders: vec![opts],
                }),
            );
            let place = requests(&mut e).remove(0);
            if lifecycle == 1 {
                ws(&mut e, socket, raw);
            }
            reply(
                &mut e,
                &place,
                300.0,
                r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
            );
            if lifecycle == 0 {
                ws(&mut e, socket, raw);
            }
        } else {
            ws(&mut e,socket,&raw.replace("\"state\":2","\"state\":2,\"stopLossPrice\":55000,\"takeProfitPrice\":65000,\"lossTrend\":3,\"profitTrend\":2"));
        }
        let stored = e.store().order("one").unwrap();
        assert_eq!(stored.stop_loss, Some(55000.0));
        assert_eq!(stored.take_profit, Some(65000.0));
        e.drain_actions().for_each(drop);
        e.handle(
            2000.0,
            Event::Command(Command::AmendOrder {
                command: 2,
                id: "one".into(),
                price: Some(58000.0),
                amount: None,
            }),
        );
        let inspect = requests(&mut e).remove(0);
        reply(
            &mut e,
            &inspect,
            2050.,
            r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2,"stopLossPrice":55000,"takeProfitPrice":65000,"lossTrend":3,"profitTrend":2}}"#,
        );
        let cancel = requests(&mut e).remove(0);
        reply(
            &mut e,
            &cancel,
            2100.0,
            r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
        );
        let read = requests(&mut e).remove(0);
        reply(
            &mut e,
            &read,
            2200.0,
            r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":4,"stopLossPrice":55000,"takeProfitPrice":65000}}"#,
        );
        let replacement = requests(&mut e).remove(0);
        let body = replacement.body.unwrap();
        for expected in [
            "\"stopLossPrice\":\"55000\"",
            "\"takeProfitPrice\":\"65000\"",
            "\"lossTrend\":3",
            "\"profitTrend\":2",
        ] {
            assert!(body.contains(expected), "{body}");
        }
    }
}

#[test]
fn plan_stop_amend_keeps_latest_native_source_after_ws() {
    use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts, StopTriggerType};
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        let body = if r.url.contains("open_positions") {
            r#"{"success":true,"code":0,"data":[{"positionId":"pos","symbol":"BTC_USDT","positionType":1,"holdVol":60,"holdAvgPrice":60000}]}"#
        } else {
            r#"{"success":true,"code":0,"data":[]}"#
        };
        response(&mut e, &r, body);
    }
    e.drain_actions().for_each(drop);
    let opts = PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::StopLoss,
        side: OrderSide::Sell,
        amount: 60.0,
        price: Some(55000.0),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: Some(true),
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: Some(StopTriggerType::MarkPrice),
        client_order_id: None,
        close: None,
        stp_mode: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    };
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        }),
    );
    let place = requests(&mut e).remove(0);
    assert!(place.body.as_ref().unwrap().contains("\"trend\":2"));
    response(&mut e, &place, r#"{"success":true,"code":0,"data":"plan"}"#);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.plan.order","data":{"id":"plan","symbol":"BTC_USDT","side":4,"orderType":5,"vol":60,"triggerPrice":55000,"triggerType":2,"trend":3,"state":1}}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "plan".into(),
            price: Some(54000.0),
            amount: None,
        }),
    );
    let inspect = requests(&mut e).remove(0);
    inspect_reply(
        &mut e,
        &inspect,
        2100.,
        r#"{"id":"plan","symbol":"BTC_USDT","side":4,"orderType":5,"vol":60,"triggerPrice":55000,"triggerType":2,"trend":3,"state":1}"#,
    );
    let replacement = requests(&mut e).remove(0);
    assert!(replacement.url.ends_with("/planorder/change_price"));
    assert!(
        replacement.body.as_ref().unwrap().contains("\"trend\":3"),
        "{}",
        replacement.body.unwrap()
    );
}

#[test]
fn native_bracket_clear_before_placement_ack_remains_authoritative() {
    use safe_cex_engine::model::StopTriggerType;
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    let mut opts = order();
    opts.stop_loss = Some(55000.0);
    opts.take_profit = Some(65000.0);
    opts.tp_trigger_type = Some(StopTriggerType::IndexPrice);
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        }),
    );
    let place = requests(&mut e).remove(0);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2,"stopLossPrice":0,"takeProfitPrice":66000,"profitTrend":2}}"#,
    );
    // A later sparse update must retain the explicit clear's authority.
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#,
    );
    reply(
        &mut e,
        &place,
        300.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let stored = e.store().order("one").unwrap();
    assert_eq!(stored.stop_loss, None);
    assert_eq!(stored.take_profit, Some(66000.0));
    assert_eq!(
        stored.extra.as_ref().unwrap().tp_trigger_type,
        Some(StopTriggerType::MarkPrice)
    );
}

#[test]
fn side_leverage_writes_only_the_selected_native_rail_and_keeps_other_setting() {
    use safe_cex_engine::model::PositionSide;
    for side in [PositionSide::Long, PositionSide::Short] {
        let mut e = ready();
        e.handle(
            200.,
            Event::Command(Command::SetLeverage {
                command: 401,
                symbol: "BTCUSDT".into(),
                leverage: 37.,
                side: Some(side),
            }),
        );
        let first = requests(&mut e).remove(0);
        let payload: serde_json::Value =
            serde_json::from_str(first.body.as_deref().unwrap()).unwrap();
        assert_eq!(
            payload["positionType"],
            if side == PositionSide::Long { 1 } else { 2 }
        );
        assert_eq!(payload["leverage"], 37.);
        reply(&mut e, &first, 300., r#"{"success":true,"code":0}"#);
        let actions = drained(&mut e);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
        );
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone {
                command: 401,
                result: _
            })
        )));
        let setting = e.store().setting("BTCUSDT").unwrap();
        assert_eq!(
            setting.buy_leverage,
            Some(if side == PositionSide::Long { 37. } else { 20. })
        );
        assert_eq!(
            setting.sell_leverage,
            Some(if side == PositionSide::Short {
                37.
            } else {
                20.
            })
        );
    }
}

#[test]
fn rejected_side_leverage_keeps_both_mexc_rails() {
    let mut e = ready();
    e.handle(
        200.,
        Event::Command(Command::SetLeverage {
            command: 402,
            symbol: "BTCUSDT".into(),
            leverage: 37.,
            side: Some(safe_cex_engine::model::PositionSide::Short),
        }),
    );
    let first = requests(&mut e).remove(0);
    reply(
        &mut e,
        &first,
        300.,
        r#"{"success":false,"code":600,"message":"rejected"}"#,
    );
    assert!(results(&mut e)[0].is_err());
    let setting = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(20.));
    assert_eq!(setting.sell_leverage, Some(20.));
}

#[test]
fn targeted_open_mexc_leg_uses_its_native_position_id_and_preserves_other_position() {
    use safe_cex_engine::model::PositionSide;
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    for (id, position_type, leverage) in [("native-long", 1, 10), ("native-short", 2, 20)] {
        let text = format!(
            r#"{{"channel":"push.personal.position","data":{{"positionId":"{id}","symbol":"BTC_USDT","positionType":{position_type},"positionMode":1,"holdVol":60,"holdAvgPrice":60000,"leverage":{leverage},"openType":1}}}}"#
        );
        ws(&mut e, socket, &text);
    }
    e.drain_actions().for_each(drop);
    e.handle(
        300.,
        Event::Command(Command::SetLeverage {
            command: 403,
            symbol: "BTCUSDT".into(),
            leverage: 37.,
            side: Some(PositionSide::Short),
        }),
    );
    let r = requests(&mut e).remove(0);
    let payload: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload["positionId"], "native-short");
    assert_eq!(payload["leverage"], 37.);
    assert!(payload.get("positionType").is_none());
    assert!(payload.get("symbol").is_none());
    reply(&mut e, &r, 400., r#"{"success":true,"code":0}"#);
    assert!(results(&mut e)[0].is_ok());
    for position in e.store().positions() {
        assert_eq!(
            position.leverage,
            if position.side == PositionSide::Long {
                10.
            } else {
                37.
            }
        );
    }
    let setting = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(
        (setting.buy_leverage, setting.sell_leverage),
        (Some(10.), Some(37.))
    );
}

fn singleton_orders_use_regular_submit_with_450ms_pacing_and_throttle_cooldown_with(refusal: &str) {
    let mut e = ready();
    for command in [1, 2] {
        e.handle(
            200.0,
            Event::Command(Command::PlaceOrders {
                command,
                orders: vec![order()],
            }),
        );
    }
    let first = requests(&mut e).remove(0);
    assert!(first.url.ends_with("/order/submit"));
    let body: serde_json::Value = serde_json::from_str(first.body.as_ref().unwrap()).unwrap();
    assert!(body.is_object());
    assert!(body.get("stpMode").is_none());
    reply(
        &mut e,
        &first,
        250.0,
        r#"{"success":true,"code":0,"data":"first"}"#,
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                assert_eq!(at_ms, 650.0);
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(650.0, Event::TimerFired { timer });
    let second = requests(&mut e).remove(0);
    assert!(second.url.ends_with("/order/submit"));
    reply(&mut e, &second, 700.0, refusal);
    e.drain_actions().for_each(drop);
    e.handle(
        750.0,
        Event::Command(Command::PlaceOrders {
            command: 3,
            orders: vec![order()],
        }),
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                assert_eq!(at_ms, 2200.0);
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(2200.0, Event::TimerFired { timer });
    assert!(requests(&mut e).remove(0).url.ends_with("/order/submit"));
}

#[test]
fn singleton_orders_use_regular_submit_with_450ms_pacing_and_throttle_cooldown() {
    singleton_orders_use_regular_submit_with_450ms_pacing_and_throttle_cooldown_with(
        r#"{"success":false,"code":510,"message":"Requests are too frequent"}"#,
    );
}

/// Legacy cooled down on the venue's too-frequent text as well as code 510.
#[test]
fn a_too_frequent_refusal_without_code_510_also_cools_submits_down() {
    singleton_orders_use_regular_submit_with_450ms_pacing_and_throttle_cooldown_with(
        r#"{"success":false,"code":1002,"message":"Requests are too frequent"}"#,
    );
}

#[test]
fn full_position_stops_snap_native_volume_to_contract_step() {
    use safe_cex_engine::model::{OrderSide, OrderType};
    let markets = MARKETS.replace("\"volUnit\":1", "\"volUnit\":0.1");
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        let (mut e, _, reads, _) = private_started_with_markets(&markets);
        for request in reads {
            let body = if request.url.contains("open_positions") {
                r#"{"success":true,"code":0,"data":[{"positionId":"pos","symbol":"BTC_USDT","positionType":1,"holdVol":59.99999999999999,"holdAvgPrice":60000,"leverage":10,"openType":1,"positionMode":2}]}"#
            } else {
                r#"{"success":true,"code":0,"data":[]}"#
            };
            response(&mut e, &request, body);
        }
        e.drain_actions().for_each(drop);
        let mut opts = order();
        opts.kind = kind;
        opts.side = OrderSide::Sell;
        opts.amount = 0.0;
        e.handle(
            200.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![opts],
            }),
        );
        let request = requests(&mut e).remove(0);
        assert!(request.url.ends_with("/stoporder/place"));
        let body: serde_json::Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["vol"], 60.0);
        assert_eq!(body["volType"], 2);
    }
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let rejected = |kind| (Some(kind), Some(Outcome::Rejected));
    for (status, body, classified, text) in [
        (503, "", (None, Some(Outcome::Unknown)), "MEXC HTTP 503"),
        (408, "", (None, Some(Outcome::Unknown)), "MEXC HTTP 408"),
        (401, "", rejected(ErrorKind::AuthInvalid), "MEXC HTTP 401"),
        (
            403,
            "",
            rejected(ErrorKind::AuthPermission),
            "MEXC HTTP 403",
        ),
        (429, "", rejected(ErrorKind::RateLimited), "MEXC HTTP 429"),
        (400, "", rejected(ErrorKind::Rejected), "MEXC HTTP 400"),
        (
            200,
            r#"{"success":false,"code":2005,"message":"balance insufficient"}"#,
            rejected(ErrorKind::Rejected),
            "MEXC 2005: balance insufficient",
        ),
        (
            200,
            r#"{"success":false,"code":602,"message":"signature"}"#,
            rejected(ErrorKind::AuthInvalid),
            "MEXC 602: signature",
        ),
        (
            200,
            r#"{"success":false,"code":510,"message":"too frequent"}"#,
            rejected(ErrorKind::RateLimited),
            "MEXC 510: too frequent",
        ),
        (
            200,
            r#"{"success":false,"code":9999,"message":"busy"}"#,
            (None, Some(Outcome::Unknown)),
            "MEXC 9999: busy",
        ),
        (
            200,
            r#"{"success":false,"code":8888,"message":"undocumented"}"#,
            (None, None),
            "MEXC 8888: undocumented",
        ),
        (
            200,
            r#"{"success":true,"code":0}"#,
            (None, Some(Outcome::Unknown)),
            "MEXC response has no data",
        ),
    ] {
        let mut e = ready();
        e.handle(
            200.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![order()],
            }),
        );
        let r = requests(&mut e).remove(0);
        e.handle(
            300.0,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let [error] = failures(&mut e)
            .try_into()
            .unwrap_or_else(|f| panic!("{f:?}"));
        assert_eq!(
            ((error.kind, error.outcome), error.legacy_text().as_str()),
            (classified, text),
            "{status} {body}"
        );
    }
    // Transport loss on a write is Unknown.
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        }),
    );
    let r = requests(&mut e).remove(0);
    e.handle(
        300.0,
        Event::HttpFailed {
            request: r.id,
            error: "reset",
        },
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
    // Preflight refusals were never sent.
    let mut e = ready();
    let mut invalid = order();
    invalid.client_order_id = Some("mine".into());
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![invalid],
        }),
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

#[test]
fn failures_after_an_effective_step_are_never_rejected_or_not_sent() {
    use safe_cex_engine::Outcome;
    // An explicit per-order refusal in a batch with an accepted leg.
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(), order()],
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0},{"errorCode":600,"errorMsg":"bad volume"}]}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, None);
    // A batch row without an order id does not prove what happened.
    let mut e = ready();
    e.handle(
        200.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(), order()],
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(
        &mut e,
        &r,
        300.0,
        r#"{"success":true,"code":0,"data":[{"errorCode":0},{"errorCode":600,"errorMsg":"bad volume"}]}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    // The replacement is refused after the amend's cancel took effect.
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2}}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":4}}"#,
    );
    let place = requests(&mut e).remove(0);
    e.handle(
        500.0,
        Event::HttpResponse {
            request: place.id,
            response: Response {
                status: 400,
                headers: "",
                body: "",
            },
        },
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, None, "{error:?}");
}

fn amend_fixture() -> (TestEngine, safe_cex_engine::SocketId) {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":10,"price":59000,"state":2}}"#,
    );
    e.drain_actions().for_each(drop);
    (e, socket)
}

fn amend_small_contract(amount: Option<f64>, expected_volume: f64) {
    let markets = MARKETS
        .replace("BTC_USDT", "XMR_USDT")
        .replace("\"baseCoin\":\"BTC\"", "\"baseCoin\":\"XMR\"")
        .replace("\"contractSize\":0.0001", "\"contractSize\":0.01");
    let (mut e, socket, reads, _) = private_started_with_markets(&markets);
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.order","data":{"orderId":"small","symbol":"XMR_USDT","side":1,"orderType":1,"vol":20,"dealVol":5,"price":300,"state":2}}"#,
    );
    e.drain_actions().for_each(drop);
    let existing = e.store().order("small").unwrap();
    assert_eq!(existing.amount, 0.2);
    assert_eq!(
        existing.extra.as_ref().unwrap().remaining_native,
        Some(15.0)
    );
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "small".into(),
            price: Some(310.0),
            amount,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    assert!(cancel.url.ends_with("/order/cancel"));
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"small","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    assert!(read.url.ends_with("/order/get/small"));
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"small","symbol":"XMR_USDT","side":1,"orderType":1,"vol":20,"dealVol":5,"price":300,"state":4}}"#,
    );
    let replacement = requests(&mut e).remove(0);
    assert!(replacement.url.ends_with("/order/submit"));
    let body: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["symbol"], "XMR_USDT");
    assert_eq!(body["vol"], expected_volume);
    assert_eq!(body["price"], "310");
}

#[test]
fn price_only_amend_keeps_native_quantity_for_a_subunit_contract() {
    amend_small_contract(None, 15.0);
}

#[test]
fn explicit_amend_amount_is_a_native_total_for_a_subunit_contract() {
    amend_small_contract(Some(20.0), 15.0);
    amend_small_contract(Some(30.0), 25.0);
}

fn amend_full_position_stop(amount: Option<f64>, updated_position: Option<f64>) {
    for take_profit in [false, true] {
        let (mut e, socket, reads, _) = private_started();
        for r in reads {
            let body = if r.url.contains("open_positions") {
                r#"{"success":true,"code":0,"data":[{"positionId":"native","symbol":"BTC_USDT","positionType":1,"positionMode":1,"holdVol":60,"holdAvgPrice":60000,"leverage":7,"openType":1}]}"#
            } else {
                r#"{"success":true,"code":0,"data":[]}"#
            };
            response(&mut e, &r, body);
        }
        e.drain_actions().for_each(drop);
        let row = if take_profit {
            r#"{"id":"full","symbol":"BTC_USDT","positionType":1,"vol":60,"volType":2,"triggerSide":1,"takeProfitPrice":65000,"state":1}"#
        } else {
            r#"{"id":"full","symbol":"BTC_USDT","positionType":1,"vol":60,"volType":2,"triggerSide":2,"stopLossPrice":55000,"state":1}"#
        };
        ws(
            &mut e,
            socket,
            &format!(r#"{{"channel":"push.personal.stop.order","data":{row}}}"#),
        );
        let original = e.store().order("full").expect("native full-position stop");
        assert_eq!(original.amount, 0.0);
        assert_eq!(original.extra.as_ref().unwrap().remaining_native, Some(0.0));
        e.drain_actions().for_each(drop);
        e.handle(
            400.0,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "full".into(),
                price: Some(if take_profit { 64000.0 } else { 54000.0 }),
                amount,
            }),
        );
        let inspect = requests(&mut e).remove(0);
        let row = exact_stop_row()
            .replace("guarded", "full")
            .replace("\"pos\"", "\"native\"");
        let row = if take_profit {
            row.replace("\"stopLossPrice\":55000", "\"stopLossPrice\":0")
                .replace("\"takeProfitPrice\":0", "\"takeProfitPrice\":65000")
                .replace("\"triggerSide\":2", "\"triggerSide\":1")
        } else {
            row
        };
        inspect_reply(&mut e, &inspect, 410., &row);
        let first = requests(&mut e).remove(0);
        if let Some(volume) = updated_position {
            e.handle(425.,Event::WsMessage{socket,text:&format!(r#"{{"channel":"push.personal.position","data":{{"positionId":"native","symbol":"BTC_USDT","positionType":1,"holdVol":{volume},"holdAvgPrice":60000}}}}"#)});
            e.drain_actions().for_each(drop);
        }
        assert!(first.url.ends_with("/stoporder/change_plan_price"));
        let replacement = first;
        let body: serde_json::Value =
            serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
        let price = if take_profit {
            "takeProfitPrice"
        } else {
            "stopLossPrice"
        };
        assert_eq!(body[price], if take_profit { "64000" } else { "54000" });
        assert!(body.get("vol").is_none());
    }
}

#[test]
fn price_only_full_position_stop_amend_uses_native_edit_without_resizing() {
    amend_full_position_stop(None, None);
    amend_full_position_stop(None, Some(25.0));
}

#[test]
fn explicit_full_position_stop_amend_keeps_zero_sentinel() {
    amend_full_position_stop(Some(0.0), None);
}

#[test]
fn reloaded_full_position_stop_amend_refuses_until_position_data_arrives() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let (mut e, socket, reads, _) = private_started();
    let mut position_read = None;
    for r in reads {
        if r.url.contains("open_positions") {
            position_read = Some(r);
        } else {
            response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
        }
    }
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.stop.order","data":{"id":"full","symbol":"BTC_USDT","positionType":1,"vol":60,"volType":2,"triggerSide":2,"stopLossPrice":55000,"state":1}}"#,
    );
    e.drain_actions().for_each(drop);
    let original = e.store().order("full").unwrap().clone();
    assert!(!e.store().loaded().positions);
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "full".into(),
            price: Some(54000.0),
            amount: None,
        }),
    );
    let actions = prepared(&mut e, 300.0);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 1, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, Some(ErrorKind::VenueUnavailable));
    assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
    assert!(errors[0].message.contains("position not loaded yet"));
    assert!(!errors[0].message.contains("minimum"));
    assert_eq!(e.store().order("full"), Some(&original));
    reply(
        &mut e,
        &position_read.expect("genuine outstanding position read"),
        350.0,
        r#"{"success":true,"code":0,"data":[{"positionId":"native","symbol":"BTC_USDT","positionType":1,"positionMode":1,"holdVol":60,"holdAvgPrice":60000,"leverage":7,"openType":1}]}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        400.0,
        Event::Command(Command::AmendOrder {
            command: 2,
            id: "full".into(),
            price: Some(54000.0),
            amount: None,
        }),
    );
    let inspect = requests(&mut e).remove(0);
    let row = exact_stop_row()
        .replace("guarded", "full")
        .replace("\"pos\"", "\"native\"");
    inspect_reply(&mut e, &inspect, 450., &row);
    let replacement = requests(&mut e).remove(0);
    assert!(replacement.url.ends_with("/stoporder/change_plan_price"));
    let body: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert!(body.get("vol").is_none());
    assert_eq!(body["stopLossPrice"], "54000");
}

#[test]
fn an_explicit_zero_regular_amend_does_not_become_a_full_position_order() {
    let (mut e, _) = amend_fixture();
    let original = e.store().order("one").unwrap().clone();
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: Some(0.0),
        }),
    );
    let actions = prepared(&mut e, 300.0);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 1, error })
            if error.outcome == Some(safe_cex_engine::Outcome::NotSent)
    )));
    assert_eq!(e.store().order("one"), Some(&original));
}

#[test]
fn an_amend_below_the_contract_minimum_is_refused_before_the_cancel() {
    let (mut e, _) = amend_fixture();
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: None,
            amount: Some(10.5),
        }),
    );
    let actions = drained(&mut e);
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "{actions:?}"
    );
    assert!(
        format!("{actions:?}").contains("the order is unchanged"),
        "{actions:?}"
    );
}

#[test]
fn a_replacement_refused_after_the_cancel_says_the_order_is_gone() {
    let (mut e, _) = amend_fixture();
    e.handle(
        300.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "one".into(),
            price: Some(58000.0),
            amount: None,
        }),
    );
    let cancel = requests(&mut e).remove(0);
    reply(
        &mut e,
        &cancel,
        400.0,
        r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#,
    );
    let read = requests(&mut e).remove(0);
    reply(
        &mut e,
        &read,
        450.0,
        r#"{"success":true,"code":0,"data":{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":59.5,"price":59000,"state":4}}"#,
    );
    let refused = failures(&mut e);
    assert!(
        refused
            .iter()
            .any(|f| f.message.contains("Your BTC_USDT order is gone")
                || f.message.contains("Your BTCUSDT order is gone")),
        "{refused:?}"
    );
}

fn with_long_position() -> TestEngine {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
    }
    ws(
        &mut e,
        socket,
        r#"{"channel":"push.personal.position","data":{"positionId":"native","symbol":"BTC_USDT","positionType":1,"positionMode":1,"holdVol":60,"holdAvgPrice":60000,"leverage":7,"openType":1}}"#,
    );
    e.drain_actions().for_each(drop);
    e
}

fn market_close(e: &mut TestEngine, side: &str, close: Option<bool>, reduce: Option<bool>) {
    let mut opts: PlaceOrderOpts = serde_json::from_str(&format!(
        r#"{{"symbol":"BTCUSDT","type":"market","side":"{side}","amount":0}}"#
    ))
    .unwrap();
    opts.close = close;
    opts.reduce_only = reduce;
    e.handle(
        400.0,
        Event::Command(Command::PlaceOrders {
            command: 9,
            orders: vec![opts],
        }),
    );
}

/// A position-panel market close sends no amount: it closes the whole position it reduces.
#[test]
fn a_market_close_without_an_amount_closes_the_whole_position() {
    for (close, reduce) in [(Some(true), None), (Some(true), Some(true))] {
        let mut e = with_long_position();
        market_close(&mut e, "sell", close, reduce);
        let sent = requests(&mut e);
        assert_eq!(sent.len(), 1, "{close:?} {reduce:?}");
        let body = sent[0].body.clone().unwrap();
        assert!(body.contains("\"vol\":60"), "{body}");
        assert!(body.contains("\"side\":4"), "{body}");
        assert!(body.contains("\"reduceOnly\":true"), "{body}");
    }
    // Reduce-only alone is never widened to the whole position: amount 0 stays refused.
    let mut e = with_long_position();
    market_close(&mut e, "sell", None, Some(true));
    let actions = prepared(&mut e, 400.0);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| format!("{a:?}").contains("MEXC order requires a positive native amount")),
        "{actions:?}"
    );
    // Nothing to close on the short side: refused before any send.
    let mut e = with_long_position();
    market_close(&mut e, "buy", Some(true), None);
    let actions = prepared(&mut e, 400.0);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| format!("{a:?}").contains("MEXC close requires an open position")),
        "{actions:?}"
    );
}

/// A reduce-only order closes the leg on its far side; with no such leg it is refused, not sent.
#[test]
fn a_reduce_only_order_with_nothing_to_reduce_is_refused_before_sending() {
    for (side, sent) in [("sell", true), ("buy", false)] {
        let mut e = with_long_position();
        let opts: PlaceOrderOpts = serde_json::from_str(&format!(
            r#"{{"symbol":"BTCUSDT","type":"limit","side":"{side}","amount":60,"price":60000,"reduceOnly":true}}"#
        ))
        .unwrap();
        e.handle(
            400.0,
            Event::Command(Command::PlaceOrders {
                command: 9,
                orders: vec![opts],
            }),
        );
        let actions = prepared(&mut e, 400.0);
        let http: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => r.body.clone(),
                _ => None,
            })
            .collect();
        if sent {
            assert_eq!(http.len(), 1);
            assert!(
                http[0].contains("\"side\":4") && http[0].contains("\"reduceOnly\":true"),
                "{http:?}"
            );
        } else {
            assert!(http.is_empty(), "{http:?}");
            assert!(
                actions.iter().any(|a| format!("{a:?}").contains(
                    "MEXC reduce-only buy needs an open short position on BTCUSDT to reduce"
                )),
                "{actions:?}"
            );
        }
    }
}

#[test]
fn clock_and_signature_refusals_never_probe_time_or_resend_a_placement() {
    for code in [513, 602] {
        let mut e = ready();
        e.handle(
            200.,
            Event::Command(Command::PlaceOrders {
                command: 991,
                orders: vec![order()],
            }),
        );
        let request = support::public_requests(&mut e, 200.)
            .into_iter()
            .find(|r| r.method == safe_cex_engine::Method::Post)
            .expect("real placement");
        reply(
            &mut e,
            &request,
            201.,
            &format!(r#"{{"success":false,"code":{code},"message":"refused"}}"#),
        );
        let actions = support::prepared(&mut e, 201.);
        assert!(!actions.iter().any(|a| matches!(a,Action::Http(r)
            if r.method!=safe_cex_engine::Method::Get || r.url.contains("/contract/ping"))));
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(
                    a,
                    Action::Notice(Notice::CommandFailed { command: 991, .. })
                ))
                .count(),
            1
        );
    }
}

fn protective_plan_fixture() -> (TestEngine, safe_cex_engine::SocketId) {
    let (mut e, socket, reads, _) = private_started();
    for r in reads {
        let body = if r.url.contains("open_positions") {
            r#"{"success":true,"code":0,"data":[{"positionId":"pos","symbol":"BTC_USDT","positionType":1,"holdVol":60,"holdAvgPrice":60000,"leverage":7,"openType":1}]}"#
        } else {
            r#"{"success":true,"code":0,"data":[]}"#
        };
        response(&mut e, &r, body);
    }
    ws(
        &mut e,
        socket,
        &format!(
            r#"{{"channel":"push.personal.plan.order","data":{}}}"#,
            protective_plan_row(1)
        ),
    );
    e.drain_actions().for_each(drop);
    (e, socket)
}
fn protective_plan_row(state: u8) -> String {
    format!(
        r#"{{"id":"guarded","symbol":"BTC_USDT","side":4,"orderType":5,"vol":60,"dealVol":0,"price":0,"triggerPrice":55000,"triggerType":2,"trend":2,"state":{state},"leverage":7,"openType":1,"executeCycle":2,"positionMode":1,"reduceOnly":true,"priceProtect":0,"stopLossPrice":0,"takeProfitPrice":0,"lossTrend":1,"profitTrend":1}}"#
    )
}
fn protective_amend(
    e: &mut TestEngine,
    amount: Option<f64>,
    now: f64,
) -> safe_cex_engine::HttpRequest {
    e.handle(
        now,
        Event::Command(Command::AmendOrder {
            command: 91,
            id: "guarded".into(),
            price: Some(54000.),
            amount,
        }),
    );
    requests(e).remove(0)
}
#[test]
fn protective_price_edit_uses_native_modify_without_cancel() {
    let (mut e, _) = protective_plan_fixture();
    let inspect = protective_amend(&mut e, None, 300.);
    assert_eq!(inspect.method, safe_cex_engine::Method::Get);
    reply(
        &mut e,
        &inspect,
        350.,
        &format!(
            r#"{{"success":true,"code":0,"data":[{}]}}"#,
            protective_plan_row(1)
        ),
    );
    let edit = requests(&mut e).remove(0);
    assert!(
        edit.url.ends_with("/planorder/change_price"),
        "{}",
        edit.url
    );
    let body: serde_json::Value = serde_json::from_str(edit.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["orderId"], "guarded");
    assert_eq!(body["triggerPrice"], "54000");
    assert_eq!(body["trend"], 2);
    reply(&mut e, &edit, 400., r#"{"success":true,"code":0}"#);
    assert!(!drained(&mut e).iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
    )));
}
#[test]
fn protective_quantity_edit_never_cancels_with_stale_reference() {
    let (mut e, _) = protective_plan_fixture();
    let inspect = protective_amend(&mut e, Some(70.), 6000.);
    assert_eq!(inspect.method, safe_cex_engine::Method::Get);
    reply(
        &mut e,
        &inspect,
        6100.,
        &format!(
            r#"{{"success":true,"code":0,"data":[{}]}}"#,
            protective_plan_row(1)
        ),
    );
    let actions = drained(&mut e);
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "{actions:?}"
    );
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.message.contains("reference stale"))),"{actions:?}");
    assert_eq!(
        e.store().order("guarded").unwrap().status,
        OrderStatus::Open
    );
}

fn guarded_fixture() -> (
    TestEngine,
    safe_cex_engine::SocketId,
    safe_cex_engine::SocketId,
) {
    let (mut e, private, reads, _, _, public) = private_started_with_public(MARKETS);
    for r in reads {
        response(
            &mut e,
            &r,
            if r.url.contains("open_positions") {
                r#"{"success":true,"code":0,"data":[{"positionId":"pos","symbol":"BTC_USDT","positionType":1,"holdVol":60,"holdAvgPrice":60000,"leverage":7,"openType":1}]}"#
            } else {
                r#"{"success":true,"code":0,"data":[]}"#
            },
        );
    }
    let public = public.unwrap();
    e.handle(150., Event::WsOpened { socket: public });
    ws(
        &mut e,
        private,
        &format!(
            r#"{{"channel":"push.personal.plan.order","data":{}}}"#,
            protective_plan_row(1)
        ),
    );
    e.drain_actions().for_each(drop);
    (e, private, public)
}
fn quote(e: &mut TestEngine, public: safe_cex_engine::SocketId, now: f64, fields: &str) {
    e.handle(
        now,
        Event::WsMessage {
            socket: public,
            text: &format!(
                r#"{{"channel":"push.tickers","data":[{{"symbol":"BTC_USDT",{fields}}}]}}"#
            ),
        },
    );
    e.drain_actions().for_each(drop);
}
fn inspect_reply(e: &mut TestEngine, r: &safe_cex_engine::HttpRequest, now: f64, row: &str) {
    reply(
        e,
        r,
        now,
        &format!(r#"{{"success":true,"code":0,"data":[{row}]}}"#),
    );
}
fn guarded_replacement(e: &mut TestEngine, now: f64) -> safe_cex_engine::HttpRequest {
    let inspect = protective_amend(e, Some(70.), now);
    inspect_reply(e, &inspect, now + 10., &protective_plan_row(1));
    let cancel = requests(e).remove(0);
    assert!(cancel.url.ends_with("/planorder/cancel"));
    reply(e, &cancel, now + 20., r#"{"success":true,"code":0}"#);
    let terminal = requests(e).remove(0);
    assert_eq!(terminal.method, safe_cex_engine::Method::Get);
    assert!(terminal.url.contains("states=2"));
    inspect_reply(e, &terminal, now + 30., &protective_plan_row(2));
    let replacement = requests(e).remove(0);
    assert!(replacement.url.ends_with("/planorder/place/v2"));
    replacement
}
#[test]
fn protective_cancel_requires_own_genuine_fresh_source() {
    for (fields, now, allowed) in [
        ("\"lastPrice\":50000,\"fairPrice\":60000", 1000., true),
        ("\"lastPrice\":60000,\"fairPrice\":53000", 1000., false),
        ("\"lastPrice\":60000", 6000., false),
        ("\"fairPrice\":0,\"lastPrice\":60000", 1000., false),
        ("\"fairPrice\":60000", 5989., true),
        ("\"fairPrice\":60000", 5990., true),
        ("\"fairPrice\":60000", 5991., false),
    ] {
        let (mut e, _, public) = guarded_fixture();
        let r = protective_amend(&mut e, Some(70.), 1000.);
        quote(&mut e, public, 1000., fields);
        inspect_reply(
            &mut e,
            &r,
            f64::max(now, 1000.) + 10.,
            &protective_plan_row(1),
        );
        let actions = prepared(&mut e, f64::max(now, 1000.) + 10.);
        assert_eq!(
            actions
                .iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/planorder/cancel"))),
            allowed,
            "{fields} {now}: {actions:?}"
        );
        if !allowed {
            assert_eq!(
                e.store().order("guarded").unwrap().status,
                OrderStatus::Open
            );
        }
    }
}
#[test]
fn protective_cancel_uses_actual_trigger_source_not_another_field() {
    for (trend, fields, allow) in [
        (
            1,
            "\"lastPrice\":60000,\"fairPrice\":53000,\"indexPrice\":53000",
            true,
        ),
        (
            2,
            "\"lastPrice\":53000,\"fairPrice\":60000,\"indexPrice\":53000",
            true,
        ),
        (
            3,
            "\"lastPrice\":53000,\"fairPrice\":53000,\"indexPrice\":60000",
            true,
        ),
        (
            3,
            "\"lastPrice\":60000,\"fairPrice\":60000,\"indexPrice\":54000",
            false,
        ),
    ] {
        let (mut e, private, public) = guarded_fixture();
        let row = protective_plan_row(1).replace("\"trend\":2", &format!("\"trend\":{trend}"));
        ws(
            &mut e,
            private,
            &format!(r#"{{"channel":"push.personal.plan.order","data":{row}}}"#),
        );
        let r = protective_amend(&mut e, Some(70.), 300.);
        quote(&mut e, public, 305., fields);
        inspect_reply(&mut e, &r, 310., &row);
        assert_eq!(!requests(&mut e).is_empty(), allow);
    }
}
#[test]
fn protective_cancel_refuses_changed_filled_missing_or_unprotected_original() {
    for mutation in ["filled", "changed", "gone", "position"] {
        let (mut e, private, _) = guarded_fixture();
        let r = protective_amend(&mut e, Some(70.), 300.);
        let text=match mutation {
            "filled"=>r#"{"channel":"push.personal.order","data":{"orderId":"guarded","symbol":"BTC_USDT","side":4,"orderType":5,"vol":60,"dealVol":1,"price":55000,"state":2}}"#.to_owned(),
            "changed"=>format!(r#"{{"channel":"push.personal.plan.order","data":{}}}"#,protective_plan_row(1).replace("55000","54500")),
            "gone"=>format!(r#"{{"channel":"push.personal.plan.order","data":{}}}"#,protective_plan_row(2)),
            _=>r#"{"channel":"push.personal.position","data":{"positionId":"pos","symbol":"BTC_USDT","positionType":1,"holdVol":0,"holdAvgPrice":60000}}"#.into(),
        };
        e.handle(
            305.,
            Event::WsMessage {
                socket: private,
                text: &text,
            },
        );
        e.drain_actions().for_each(drop);
        inspect_reply(&mut e, &r, 310., &protective_plan_row(1));
        assert!(requests(&mut e).is_empty(), "{mutation}");
    }
}
#[test]
fn protective_refusal_restores_exact_original_once() {
    for restore_refused in [false, true] {
        let (mut e, _, _) = guarded_fixture();
        let replacement = guarded_replacement(&mut e, 300.);
        let new: serde_json::Value =
            serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
        assert_eq!(new["vol"], 70.);
        assert_eq!(new["triggerPrice"], 54000.);
        reply(
            &mut e,
            &replacement,
            340.,
            r#"{"success":false,"code":2006,"message":"replacement refused"}"#,
        );
        let restore = requests(&mut e).remove(0);
        let old: serde_json::Value =
            serde_json::from_str(restore.body.as_deref().unwrap()).unwrap();
        assert_eq!(old["vol"], 60.);
        assert_eq!(old["triggerPrice"], 55000.);
        assert_eq!(old["side"], 4.);
        assert_eq!(old["orderType"], 5.);
        assert_eq!(old["trend"], 2);
        assert_eq!(old["priceProtect"], 0.);
        assert_eq!(old["reduceOnly"], true);
        assert_eq!(old["executeCycle"], 2.);
        reply(
            &mut e,
            &restore,
            350.,
            if restore_refused {
                r#"{"success":false,"code":2006,"message":"old trigger crossed"}"#
            } else {
                r#"{"success":true,"code":0,"data":"restored"}"#
            },
        );
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.protection==Some(if restore_refused{safe_cex_engine::error::Protection::Unprotected}else{safe_cex_engine::error::Protection::Restored}))),"{actions:?}");
    }
}
#[test]
fn protective_unknown_replacement_never_puts_back_or_retries() {
    for status in [200, 408, 500] {
        let (mut e, _, _) = guarded_fixture();
        let replacement = guarded_replacement(&mut e, 300.);
        e.handle(
            340.,
            Event::HttpResponse {
                request: replacement.id,
                response: Response {
                    status,
                    headers: "",
                    body: "{}",
                },
            },
        );
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.outcome==Some(safe_cex_engine::Outcome::Unknown))),"{actions:?}");
    }
}
#[test]
fn protective_native_refusal_and_404_have_no_cancel_fallback() {
    for (status, body) in [
        (200, r#"{"success":false,"code":3001,"message":"refused"}"#),
        (404, r#"{"success":false,"code":3001,"message":"refused"}"#),
        (
            200,
            r#"{"success":true,"code":0,"data":{"errorCode":2006,"errorMsg":"refused"}}"#,
        ),
    ] {
        let (mut e, _) = protective_plan_fixture();
        let r = protective_amend(&mut e, None, 300.);
        inspect_reply(&mut e, &r, 310., &protective_plan_row(1));
        let native = requests(&mut e).remove(0);
        e.handle(
            320.,
            Event::HttpResponse {
                request: native.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.outcome==Some(safe_cex_engine::Outcome::Rejected))),"{actions:?}");
        assert_eq!(
            e.store().order("guarded").unwrap().status,
            OrderStatus::Open
        );
    }
}
#[test]
fn protective_terminal_read_never_treats_absence_or_triggered_as_cancel_proof() {
    for row in [
        String::new(),
        protective_plan_row(3),
        protective_plan_row(2).replace("\"vol\":60", "\"vol\":61"),
    ] {
        let (mut e, _, _) = guarded_fixture();
        let r = protective_amend(&mut e, Some(70.), 300.);
        inspect_reply(&mut e, &r, 310., &protective_plan_row(1));
        let cancel = requests(&mut e).remove(0);
        reply(&mut e, &cancel, 320., r#"{"success":true,"code":0}"#);
        let terminal = requests(&mut e).remove(0);
        inspect_reply(&mut e, &terminal, 330., &row);
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.protection.is_none() && error.outcome.is_none() && error.partial.as_ref().is_some_and(|partial|partial.order_ids==["guarded"]) && error.message.contains("status is unknown"))),"{actions:?}");
    }
}

fn exact_stop_row() -> String {
    r#"{"id":"guarded","orderId":"0","symbol":"BTC_USDT","positionId":"pos","positionType":1,"vol":60,"volType":2,"triggerSide":2,"stopLossPrice":55000,"takeProfitPrice":0,"lossTrend":2,"profitTrend":1,"state":1,"priceProtect":0,"profitLossVolType":"SAME","takeProfitReverse":2,"stopLossReverse":2,"takeProfitType":0,"stopLossType":0,"takeProfitOrderPrice":0,"stopLossOrderPrice":0}"#.into()
}
#[test]
fn protective_native_endpoints_preserve_peer_prices_and_sources() {
    for class in ["stop", "limit-parent", "plan-parent"] {
        let (mut e, private, _) = guarded_fixture();
        let (channel,row,id,path)=match class {
            "stop"=>("push.personal.stop.order",exact_stop_row().replace("\"takeProfitPrice\":0","\"takeProfitPrice\":65000"),"guarded__stop_loss","/stoporder/change_plan_price"),
            "limit-parent"=>("push.personal.order",r#"{"orderId":"parent","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2,"stopLossPrice":55000,"takeProfitPrice":65000,"lossTrend":2,"profitTrend":3}"#.into(),"parent__stop_loss","/stoporder/change_price"),
            _=>("push.personal.plan.order",protective_plan_row(1).replace("\"side\":4","\"side\":1").replace("\"reduceOnly\":true","\"reduceOnly\":false").replace("\"stopLossPrice\":0","\"stopLossPrice\":52000").replace("\"takeProfitPrice\":0","\"takeProfitPrice\":65000").replace("\"lossTrend\":1","\"lossTrend\":2").replace("\"profitTrend\":1","\"profitTrend\":3"),"guarded__stop_loss","/planorder/change_stop_order"),
        };
        ws(
            &mut e,
            private,
            &format!(r#"{{"channel":"{channel}","data":{row}}}"#),
        );
        e.drain_actions().for_each(drop);
        e.handle(
            300.,
            Event::Command(Command::AmendOrder {
                command: 91,
                id: id.into(),
                price: Some(53000.),
                amount: None,
            }),
        );
        let inspect = requests(&mut e).remove(0);
        if class == "limit-parent" {
            reply(
                &mut e,
                &inspect,
                310.,
                &format!(r#"{{"success":true,"code":0,"data":{row}}}"#),
            );
        } else {
            inspect_reply(&mut e, &inspect, 310., &row);
        }
        let edit = requests(&mut e).remove(0);
        assert!(edit.url.ends_with(path), "{}", edit.url);
        let body: serde_json::Value = serde_json::from_str(edit.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["stopLossPrice"], "53000");
        assert_eq!(body["takeProfitPrice"], "65000");
        assert_eq!(body["lossTrend"], 2);
        assert_eq!(body["profitTrend"], if class == "stop" { 1 } else { 3 });
        assert!(!body.to_string().contains("cancel"));
    }
}
#[test]
fn protective_native_edit_without_peer_provenance_is_not_sent() {
    let (mut e, private, _) = guarded_fixture();
    let row = exact_stop_row().replace(",\"takeProfitPrice\":0", "");
    ws(
        &mut e,
        private,
        &format!(r#"{{"channel":"push.personal.stop.order","data":{row}}}"#),
    );
    e.drain_actions().for_each(drop);
    let r = protective_amend(&mut e, None, 300.);
    inspect_reply(&mut e, &r, 310., &row);
    assert!(requests(&mut e).is_empty());
}
#[test]
fn protective_native_ack_does_not_overwrite_newer_stream() {
    let (mut e, private) = protective_plan_fixture();
    let inspect = protective_amend(&mut e, None, 300.);
    inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
    let edit = requests(&mut e).remove(0);
    let row = protective_plan_row(1).replace("55000", "54500");
    e.handle(
        315.,
        Event::WsMessage {
            socket: private,
            text: &format!(r#"{{"channel":"push.personal.plan.order","data":{row}}}"#),
        },
    );
    e.drain_actions().for_each(drop);
    reply(&mut e, &edit, 320., r#"{"success":true,"code":0}"#);
    assert_eq!(e.store().order("guarded").unwrap().price, 54500.);
}
#[test]
fn protective_sparse_traffic_cannot_renew_mark_or_index() {
    for trend in [2, 3] {
        let (mut e, private, public) = guarded_fixture();
        let row = protective_plan_row(1).replace("\"trend\":2", &format!("\"trend\":{trend}"));
        ws(
            &mut e,
            private,
            &format!(r#"{{"channel":"push.personal.plan.order","data":{row}}}"#),
        );
        quote(
            &mut e,
            public,
            1000.,
            "\"fairPrice\":60000,\"indexPrice\":60000",
        );
        quote(&mut e, public, 6000., "\"lastPrice\":60000");
        let inspect = protective_amend(&mut e, Some(70.), 6001.);
        inspect_reply(&mut e, &inspect, 6010., &row);
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.message.contains("reference stale"))));
    }
}

#[test]
fn protective_cancel_veto_releases_job_for_a_later_valid_edit() {
    let (mut e, _, public) = guarded_fixture();
    let inspect = protective_amend(&mut e, Some(70.), 6000.);
    inspect_reply(&mut e, &inspect, 6010., &protective_plan_row(1));
    assert!(!failures(&mut e).is_empty());
    quote(&mut e, public, 6020., "\"fairPrice\":60000");
    let next = protective_amend(&mut e, Some(70.), 6030.);
    inspect_reply(&mut e, &next, 6040., &protective_plan_row(1));
    assert!(
        requests(&mut e)
            .remove(0)
            .url
            .ends_with("/planorder/cancel")
    );
}
#[test]
fn protective_restore_is_withheld_if_old_trigger_crossed_or_reference_expired() {
    for expired in [false, true] {
        let (mut e, _, public) = guarded_fixture();
        let replacement = guarded_replacement(&mut e, 300.);
        if !expired {
            quote(&mut e, public, 335., "\"fairPrice\":54000");
        }
        reply(
            &mut e,
            &replacement,
            if expired { 6100. } else { 340. },
            r#"{"success":false,"code":2006,"message":"replacement refused"}"#,
        );
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.protection==Some(safe_cex_engine::error::Protection::Unprotected))));
    }
}
#[test]
fn protective_partial_or_paired_refusal_is_unprotected_without_guessing_restore() {
    for grouped in [false, true] {
        let (mut e, private, _) = guarded_fixture();
        let row = if grouped {
            // A sized pair still proves Unprotected after refusal; full-position resizing is vetoed earlier.
            exact_stop_row()
                .replace("\"volType\":2", "\"volType\":1")
                .replace("\"takeProfitPrice\":0", "\"takeProfitPrice\":65000")
        } else {
            exact_stop_row().replace("\"volType\":2", "\"volType\":1")
        };
        ws(
            &mut e,
            private,
            &format!(r#"{{"channel":"push.personal.stop.order","data":{row}}}"#),
        );
        e.drain_actions().for_each(drop);
        let inspect = protective_amend(&mut e, Some(70.), 300.);
        inspect_reply(&mut e, &inspect, 310., &row);
        let cancel = requests(&mut e).remove(0);
        assert!(cancel.url.ends_with("/stoporder/cancel"));
        reply(&mut e, &cancel, 320., r#"{"success":true,"code":0}"#);
        let terminal = requests(&mut e).remove(0);
        assert!(terminal.url.contains("state=2"));
        inspect_reply(
            &mut e,
            &terminal,
            330.,
            &row.replace("\"state\":1", "\"state\":2"),
        );
        let place = requests(&mut e).remove(0);
        reply(
            &mut e,
            &place,
            340.,
            r#"{"success":false,"code":2006,"message":"refused"}"#,
        );
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.protection==Some(safe_cex_engine::error::Protection::Unprotected) && error.message.contains("re-placed"))));
    }
}
#[test]
fn protective_terminal_inspection_is_bounded_without_resending_cancel() {
    let (mut e, _, _) = guarded_fixture();
    let inspect = protective_amend(&mut e, Some(70.), 300.);
    inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
    let cancel = requests(&mut e).remove(0);
    reply(&mut e, &cancel, 320., r#"{"success":true,"code":0}"#);
    for attempt in 0..6 {
        let read = requests(&mut e).remove(0);
        inspect_reply(
            &mut e,
            &read,
            330. + f64::from(attempt) * 300.,
            &protective_plan_row(1),
        );
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        if attempt < 5 {
            let (timer, at) = actions
                .into_iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                    _ => None,
                })
                .unwrap();
            e.handle(at, Event::TimerFired { timer });
        } else {
            assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.protection.is_none() && error.outcome.is_none() && error.partial.as_ref().is_some_and(|partial|partial.order_ids==["guarded"]) && error.message.contains("status is unknown"))));
        }
    }
}
#[test]
fn protective_native_terminal_metadata_change_withholds_replacement() {
    let (mut e, _, _) = guarded_fixture();
    let inspect = protective_amend(&mut e, Some(70.), 300.);
    inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
    let cancel = requests(&mut e).remove(0);
    reply(&mut e, &cancel, 320., r#"{"success":true,"code":0}"#);
    let read = requests(&mut e).remove(0);
    inspect_reply(
        &mut e,
        &read,
        330.,
        &protective_plan_row(2).replace("\"priceProtect\":0", "\"priceProtect\":1"),
    );
    assert!(requests(&mut e).is_empty());
}

#[test]
fn protective_final_cancel_checks_stop_and_take_profit_on_both_sides() {
    for (side, trigger_type, old, next, crossed) in [
        (4, 2, 55000., 54000., 53000.),
        (4, 1, 65000., 64000., 65000.),
        (2, 1, 65000., 64000., 65000.),
        (2, 2, 55000., 54000., 53000.),
    ] {
        for should_cross in [false, true] {
            let (mut e, private, public) = guarded_fixture();
            let row = protective_plan_row(1)
                .replace("\"side\":4", &format!("\"side\":{side}"))
                .replace(
                    "\"triggerType\":2",
                    &format!("\"triggerType\":{trigger_type}"),
                )
                .replace("\"triggerPrice\":55000", &format!("\"triggerPrice\":{old}"));
            ws(
                &mut e,
                private,
                &format!(r#"{{"channel":"push.personal.plan.order","data":{row}}}"#),
            );
            if side == 2 {
                ws(
                    &mut e,
                    private,
                    r#"{"channel":"push.personal.position","data":{"positionId":"short","symbol":"BTC_USDT","positionType":2,"holdVol":60,"holdAvgPrice":60000}}"#,
                );
            }
            e.drain_actions().for_each(drop);
            e.handle(
                300.,
                Event::Command(Command::AmendOrder {
                    command: 91,
                    id: "guarded".into(),
                    price: Some(next),
                    amount: Some(70.),
                }),
            );
            let inspect = requests(&mut e).remove(0);
            quote(
                &mut e,
                public,
                305.,
                &format!(
                    "\"fairPrice\":{}",
                    if should_cross { crossed } else { 60000. }
                ),
            );
            inspect_reply(&mut e, &inspect, 310., &row);
            let actions = prepared(&mut e, 310.);
            assert_eq!(
                actions
                    .iter()
                    .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/planorder/cancel"))),
                !should_cross,
                "side={side} type={trigger_type} {actions:?}"
            );
        }
    }
}
#[test]
fn protective_public_disconnect_invalidates_reference_receipts() {
    let (mut e, _, public) = guarded_fixture();
    let inspect = protective_amend(&mut e, Some(70.), 300.);
    e.handle(
        305.,
        Event::WsClosed {
            socket: public,
            code: 1006,
        },
    );
    e.drain_actions().for_each(drop);
    inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
    let actions = drained(&mut e);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.message.contains("reference stale"))));
}

#[test]
fn protective_native_edits_are_paced_and_recheck_original_after_waiting() {
    let (mut e, private) = protective_plan_fixture();
    let inspect = protective_amend(&mut e, None, 300.);
    inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
    let native = requests(&mut e).remove(0);
    reply(&mut e, &native, 320., r#"{"success":true,"code":0}"#);
    e.drain_actions().for_each(drop);
    e.handle(
        400.,
        Event::Command(Command::AmendOrder {
            command: 92,
            id: "guarded".into(),
            price: Some(53000.),
            amount: None,
        }),
    );
    let inspect = requests(&mut e).remove(0);
    inspect_reply(
        &mut e,
        &inspect,
        410.,
        &protective_plan_row(1).replace("55000", "54000"),
    );
    let actions = drained(&mut e);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let (timer, at) = actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .unwrap();
    assert_eq!(at, 810.);
    e.handle(
        800.,
        Event::WsMessage {
            socket: private,
            text: &format!(
                r#"{{"channel":"push.personal.plan.order","data":{}}}"#,
                protective_plan_row(1).replace("55000", "53500")
            ),
        },
    );
    e.drain_actions().for_each(drop);
    e.handle(at, Event::TimerFired { timer });
    let actions = drained(&mut e);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:92,error}) if error.outcome==Some(safe_cex_engine::Outcome::NotSent))));
}

#[test]
fn protective_final_guard_uses_the_rounded_trigger_that_will_be_sent() {
    let (mut e, _, public) = guarded_fixture();
    e.handle(
        300.,
        Event::Command(Command::AmendOrder {
            command: 91,
            id: "guarded".into(),
            price: Some(60000.05),
            amount: Some(70.),
        }),
    );
    let inspect = requests(&mut e).remove(0);
    quote(&mut e, public, 305., "\"fairPrice\":60000.1");
    inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
    let actions = drained(&mut e);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.message.contains("crossed"))));
    assert_eq!(
        e.store().order("guarded").unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn protective_rate_limit_on_replacement_or_put_back_is_unknown_without_protection_claim() {
    for put_back in [false, true] {
        for (status, body) in [
            (429, ""),
            (
                429,
                r#"{"success":false,"code":510,"message":"rate limited"}"#,
            ),
            (429, r#"{"success":false,"code":2006,"message":"refused"}"#),
            (
                200,
                r#"{"success":false,"code":510,"message":"rate limited"}"#,
            ),
        ] {
            let (mut e, _, _) = guarded_fixture();
            let replacement = guarded_replacement(&mut e, 300.);
            let pending = if put_back {
                reply(
                    &mut e,
                    &replacement,
                    340.,
                    r#"{"success":false,"code":2006,"message":"replacement refused"}"#,
                );
                requests(&mut e).remove(0)
            } else {
                replacement
            };
            e.handle(
                350.,
                Event::HttpResponse {
                    request: pending.id,
                    response: Response {
                        status,
                        headers: "",
                        body,
                    },
                },
            );
            let actions = drained(&mut e);
            assert!(
                !actions.iter().any(|a| matches!(a, Action::Http(_))),
                "{actions:?}"
            );
            let error = actions
                .iter()
                .find_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                    _ => None,
                })
                .unwrap();
            assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
            assert!(error.protection.is_none());
            assert!(error.message.contains("is unknown"), "{error:?}");
            assert!(error.message.contains("Open Orders") && error.message.contains("position"));
            assert!(!error.message.contains("both refused"));
        }
    }
}

#[test]
fn protective_native_rate_limit_is_unknown_without_not_changed_or_cancel() {
    for (status, body) in [
        (429, ""),
        (429, r#"{"success":false,"code":2006,"message":"refused"}"#),
        (
            200,
            r#"{"success":false,"code":510,"message":"rate limited"}"#,
        ),
    ] {
        let (mut e, _) = protective_plan_fixture();
        let inspect = protective_amend(&mut e, None, 300.);
        inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
        let edit = requests(&mut e).remove(0);
        e.handle(
            320.,
            Event::HttpResponse {
                request: edit.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let actions = drained(&mut e);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "{actions:?}"
        );
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert!(error.protection.is_none());
        assert!(error.message.contains("is unknown"), "{error:?}");
        assert!(
            error.message.contains("Open Orders") && error.message.contains("position"),
            "{error:?}"
        );
        assert!(!error.message.contains("not changed"));
    }
}

#[test]
fn protective_native_modify_requires_fresh_own_reference_and_uncrossed_trigger() {
    for class in ["plan", "stop", "limit-parent", "plan-parent"] {
        for refusal in ["stale", "crossed", "allowed"] {
            let (mut e, private, public) = guarded_fixture();
            let (channel, row, id) = match class {
                "plan" => ("push.personal.plan.order", protective_plan_row(1), "guarded"),
                "stop" => ("push.personal.stop.order", exact_stop_row(), "guarded__stop_loss"),
                "limit-parent" => ("push.personal.order", r#"{"orderId":"parent","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2,"stopLossPrice":55000,"takeProfitPrice":65000,"lossTrend":2,"profitTrend":3}"#.into(), "parent__stop_loss"),
                _ => ("push.personal.plan.order", protective_plan_row(1).replace("\"side\":4", "\"side\":1").replace("\"reduceOnly\":true", "\"reduceOnly\":false").replace("\"stopLossPrice\":0", "\"stopLossPrice\":52000").replace("\"takeProfitPrice\":0", "\"takeProfitPrice\":65000").replace("\"lossTrend\":1", "\"lossTrend\":2").replace("\"profitTrend\":1", "\"profitTrend\":3"), "guarded__stop_loss"),
            };
            ws(
                &mut e,
                private,
                &format!(r#"{{"channel":"{channel}","data":{row}}}"#),
            );
            e.drain_actions().for_each(drop);
            let now = if refusal == "stale" { 6000. } else { 300. };
            e.handle(
                now,
                Event::Command(Command::AmendOrder {
                    command: 91,
                    id: id.into(),
                    price: Some(53000.),
                    amount: None,
                }),
            );
            let inspect = requests(&mut e).remove(0);
            if refusal == "crossed" {
                quote(
                    &mut e,
                    public,
                    now + 5.,
                    "\"lastPrice\":70000,\"fairPrice\":53000,\"indexPrice\":70000",
                );
            }
            if class == "limit-parent" {
                reply(
                    &mut e,
                    &inspect,
                    now + 10.,
                    &format!(r#"{{"success":true,"code":0,"data":{row}}}"#),
                );
            } else {
                inspect_reply(&mut e, &inspect, now + 10., &row);
            }
            let actions = support::prepared(&mut e, now + 10.);
            assert_eq!(
                actions.iter().any(|a| matches!(a, Action::Http(_))),
                refusal == "allowed",
                "{class} {refusal}: {actions:?}"
            );
            if refusal != "allowed" {
                assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {error,..}) if error.outcome==Some(safe_cex_engine::Outcome::NotSent) && error.message.contains(if refusal=="stale" {"reference stale"} else {"crossed"}))), "{class} {refusal}: {actions:?}");
            }
        }
    }
}

#[test]
fn protective_native_paced_send_rechecks_reference_after_admission() {
    for refusal in ["stale", "crossed", "allowed"] {
        let (mut e, _, public) = guarded_fixture();
        let inspect = protective_amend(&mut e, None, 4900.);
        inspect_reply(&mut e, &inspect, 4910., &protective_plan_row(1));
        let native = requests(&mut e).remove(0);
        reply(&mut e, &native, 4920., r#"{"success":true,"code":0}"#);
        e.drain_actions().for_each(drop);
        e.handle(
            4930.,
            Event::Command(Command::AmendOrder {
                command: 92,
                id: "guarded".into(),
                price: Some(53000.),
                amount: None,
            }),
        );
        let inspect = requests(&mut e).remove(0);
        inspect_reply(
            &mut e,
            &inspect,
            4940.,
            &protective_plan_row(1).replace("55000", "54000"),
        );
        let actions = drained(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        let (timer, at) = actions
            .into_iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .unwrap();
        assert_eq!(at, 5410.);
        if refusal != "stale" {
            quote(
                &mut e,
                public,
                5400.,
                if refusal == "crossed" {
                    "\"fairPrice\":53000"
                } else {
                    "\"fairPrice\":60000"
                },
            );
        }
        e.handle(at, Event::TimerFired { timer });
        let actions = support::prepared(&mut e, at);
        assert_eq!(
            actions.iter().any(|a| matches!(a, Action::Http(_))),
            refusal == "allowed",
            "{refusal}: {actions:?}"
        );
        if refusal != "allowed" {
            assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {error,..}) if error.outcome==Some(safe_cex_engine::Outcome::NotSent))), "{actions:?}");
        }
    }
}

#[test]
fn protective_ambiguous_envelope_codes_never_restore_or_report_not_sent() {
    for mode in ["replacement", "restore", "native"] {
        for status in [200, 400, 409, 499] {
            for code in [500., 501., 9999., 8888., 513., 2006.5] {
                let (mut e, _, _) = guarded_fixture();
                let request = if mode == "native" {
                    let inspect = protective_amend(&mut e, None, 300.);
                    inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
                    let request = requests(&mut e).remove(0);
                    assert!(request.url.ends_with("/planorder/change_price"));
                    request
                } else {
                    let replacement = guarded_replacement(&mut e, 300.);
                    if mode == "restore" {
                        reply(
                            &mut e,
                            &replacement,
                            335.,
                            r#"{"success":false,"code":2006,"message":"definitive replacement refusal"}"#,
                        );
                        requests(&mut e).remove(0)
                    } else {
                        replacement
                    }
                };
                e.handle(340., Event::HttpResponse {
                    request: request.id,
                    response: Response {status, headers: "", body: &format!(
                        r#"{{"success":false,"code":{code},"message":"ambiguous write failure"}}"#,
                    )},
                });
                let actions = drained(&mut e);
                assert!(
                    !actions.iter().any(|a| matches!(a, Action::Http(_))),
                    "mode={mode}, status={status}, code={code}: {actions:?}"
                );
                let errors: Vec<_> = actions
                    .iter()
                    .filter_map(|a| match a {
                        Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
                        _ => None,
                    })
                    .collect();
                assert_eq!(errors.len(), 1, "{actions:?}");
                assert_eq!(
                    errors[0].outcome,
                    Some(safe_cex_engine::Outcome::Unknown),
                    "mode={mode}, status={status}, code={code}"
                );
                assert!(errors[0].protection.is_none());
                assert!(
                    errors[0].message.contains("status is unknown"),
                    "{:?}",
                    errors[0]
                );
                assert!(
                    errors[0].message.contains("Open Orders")
                        && errors[0].message.contains("position")
                );
                assert!(!errors[0].message.contains("restored"));
            }
        }
    }
}

#[test]
fn protective_ambiguous_row_codes_never_restore_or_report_not_sent() {
    for native in [false, true] {
        for code in [500, 501, 9999, 8888, 510] {
            let (mut e, _, _) = guarded_fixture();
            let request = if native {
                let inspect = protective_amend(&mut e, None, 300.);
                inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
                requests(&mut e).remove(0)
            } else {
                guarded_replacement(&mut e, 300.)
            };
            reply(
                &mut e,
                &request,
                340.,
                &format!(
                    r#"{{"success":true,"code":0,"data":{{"id":"0","errorCode":{code},"errorMsg":"ambiguous row failure"}}}}"#,
                ),
            );
            let actions = drained(&mut e);
            assert!(
                !actions.iter().any(|a| matches!(a, Action::Http(_))),
                "native={native}, code={code}: {actions:?}"
            );
            let error = actions
                .iter()
                .find_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
                    _ => None,
                })
                .expect("one terminal failure");
            assert_eq!(
                error.outcome,
                Some(safe_cex_engine::Outcome::Unknown),
                "native={native}, code={code}"
            );
            assert!(error.protection.is_none());
        }
    }
}

#[test]
fn protective_stop_limit_trigger_edit_refuses_before_native_modify_or_cancel() {
    for amount in [None, Some(70.)] {
        let (mut e, private, _) = guarded_fixture();
        let row = protective_plan_row(1)
            .replace("\"orderType\":5", "\"orderType\":1")
            .replace("\"price\":0", "\"price\":54500");
        ws(
            &mut e,
            private,
            &format!(r#"{{"channel":"push.personal.plan.order","data":{row}}}"#),
        );
        e.drain_actions().for_each(drop);
        let original = e.store().order("guarded").unwrap().clone();
        let inspect = protective_amend(&mut e, amount, 300.);
        inspect_reply(&mut e, &inspect, 310., &row);
        let actions = drained(&mut e);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "{actions:?}"
        );
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
                _ => None,
            })
            .expect("one terminal refusal");
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
        assert!(error.message.contains("stop-limit"));
        assert_eq!(e.store().order("guarded"), Some(&original));
    }
}

#[test]
fn protective_full_position_resize_refuses_before_cancel() {
    for take in [false, true] {
        let (mut e, private, _) = guarded_fixture();
        let row = if take {
            exact_stop_row()
                .replace("\"triggerSide\":2", "\"triggerSide\":1")
                .replace("\"stopLossPrice\":55000", "\"stopLossPrice\":0")
                .replace("\"takeProfitPrice\":0", "\"takeProfitPrice\":65000")
        } else {
            exact_stop_row()
        };
        ws(
            &mut e,
            private,
            &format!(r#"{{"channel":"push.personal.stop.order","data":{row}}}"#),
        );
        e.drain_actions().for_each(drop);
        let original = e.store().order("guarded").unwrap().clone();
        assert_eq!(original.amount, 0.);
        e.handle(
            300.,
            Event::Command(Command::AmendOrder {
                command: 91,
                id: "guarded".into(),
                price: Some(if take { 64000. } else { 54000. }),
                amount: Some(20.),
            }),
        );
        let inspect = requests(&mut e).remove(0);
        inspect_reply(&mut e, &inspect, 310., &row);
        let actions = drained(&mut e);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "{actions:?}"
        );
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
                _ => None,
            })
            .expect("one terminal refusal");
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
        assert!(error.message.contains("full-position"));
        assert_eq!(e.store().order("guarded"), Some(&original));
    }
}

#[test]
fn protective_malformed_or_conflicting_refusals_never_authorize_put_back() {
    for status in [200, 400] {
        for body in [
            "not JSON",
            r#"{"success":true,"code":2006}"#,
            r#"{"code":2006}"#,
            r#"{"success":false,"code":0}"#,
            r#"{"success":true,"code":0,"data":[{"id":"live","errorCode":0},{"id":"0","errorCode":2006}]}"#,
            r#"{"success":true,"code":0,"data":{"id":"live","errorCode":2006}}"#,
        ] {
            let (mut e, _, _) = guarded_fixture();
            let request = guarded_replacement(&mut e, 300.);
            e.handle(
                340.,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status,
                        headers: "",
                        body,
                    },
                },
            );
            let actions = drained(&mut e);
            assert!(
                !actions.iter().any(|a| matches!(a, Action::Http(_))),
                "{status}, {body}: {actions:?}"
            );
            let error = actions
                .iter()
                .find_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                error.outcome,
                Some(safe_cex_engine::Outcome::Unknown),
                "{status}, {body}"
            );
            assert!(error.protection.is_none());
        }
    }
}

#[test]
fn protective_documented_row_refusal_still_permits_one_exact_restore() {
    for code in [2006, 3001] {
        for array in [false, true] {
            let (mut e, _, _) = guarded_fixture();
            let request = guarded_replacement(&mut e, 300.);
            let row = format!(r#"{{"id":"0","errorCode":{code},"errorMsg":"definitive refusal"}}"#);
            let rows = if array { format!("[{row}]") } else { row };
            reply(
                &mut e,
                &request,
                340.,
                &format!(r#"{{"success":true,"code":0,"data":{rows}}}"#),
            );
            let restore = requests(&mut e);
            assert_eq!(restore.len(), 1);
            let body: serde_json::Value =
                serde_json::from_str(restore[0].body.as_deref().unwrap()).unwrap();
            assert_eq!(body["vol"], 60.);
            assert_eq!(body["triggerPrice"], 55000.);
            assert_eq!(body["orderType"], 5.);
            reply(
                &mut e,
                &restore[0],
                350.,
                r#"{"success":true,"code":0,"data":"restored"}"#,
            );
            let actions = drained(&mut e);
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
            assert_eq!(actions.iter().filter(|a| matches!(a,Action::Notice(Notice::CommandFailed {error,..}) if error.protection==Some(safe_cex_engine::error::Protection::Restored))).count(),1);
        }
    }
}

#[test]
fn protective_stop_limit_quantity_only_recreate_keeps_both_native_prices() {
    let (mut e, private, _) = guarded_fixture();
    let row = protective_plan_row(1)
        .replace("\"orderType\":5", "\"orderType\":1")
        .replace("\"price\":0", "\"price\":54500");
    ws(
        &mut e,
        private,
        &format!(r#"{{"channel":"push.personal.plan.order","data":{row}}}"#),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        300.,
        Event::Command(Command::AmendOrder {
            command: 91,
            id: "guarded".into(),
            price: None,
            amount: Some(70.),
        }),
    );
    let inspect = requests(&mut e).remove(0);
    inspect_reply(&mut e, &inspect, 310., &row);
    let cancel = requests(&mut e).remove(0);
    assert!(cancel.url.ends_with("/planorder/cancel"));
    reply(&mut e, &cancel, 320., r#"{"success":true,"code":0}"#);
    let terminal = requests(&mut e).remove(0);
    inspect_reply(
        &mut e,
        &terminal,
        330.,
        &row.replace("\"state\":1", "\"state\":2"),
    );
    let place = requests(&mut e).remove(0);
    let body: serde_json::Value = serde_json::from_str(place.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["price"], 54500.);
    assert_eq!(body["triggerPrice"], 55000.);
    assert_eq!(body["vol"], 70.);
    assert_eq!(body["orderType"], 1.);
}

#[test]
fn ordinary_single_row_refusals_keep_the_unclassified_contract() {
    for code in [2006, 500, 8888] {
        let mut e = ready();
        e.handle(
            200.,
            Event::Command(Command::PlaceOrders {
                command: 91,
                orders: vec![order()],
            }),
        );
        let request = requests(&mut e).remove(0);
        reply(
            &mut e,
            &request,
            300.,
            &format!(
                r#"{{"success":true,"code":0,"data":{{"orderId":"0","errorCode":{code},"errorMsg":"ordinary row refusal"}}}}"#,
            ),
        );
        let actions = drained(&mut e);
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "{actions:?}"
        );
        let errors: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1, "code={code}: {actions:?}");
        let error = errors[0];
        assert_eq!(error.outcome, None, "code={code}: {error:?}");
        assert_eq!(error.kind, None, "code={code}: {error:?}");
        assert_eq!(error.venue_code, None, "code={code}: {error:?}");
        assert_eq!(error.venue_message, None, "code={code}: {error:?}");
        assert_eq!(
            error.message,
            format!("MEXC refused the order [{code}]: ordinary row refusal")
        );
    }
}

#[test]
fn ordinary_failure_envelopes_keep_the_base_classification() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (body, code, outcome) in [
        (r#"{"code":2006}"#, "2006", Some(Outcome::Rejected)),
        (
            r#"{"success":true,"code":2006}"#,
            "2006",
            Some(Outcome::Rejected),
        ),
        (
            r#"{"success":false,"code":2006.5}"#,
            "2006.5",
            Some(Outcome::Rejected),
        ),
        (r#"{"success":false,"code":3001}"#, "3001", None),
        (r#"{"success":false,"code":3001.5}"#, "3001.5", None),
    ] {
        let mut e = ready();
        e.handle(
            200.,
            Event::Command(Command::PlaceOrders {
                command: 91,
                orders: vec![order()],
            }),
        );
        let request = requests(&mut e).remove(0);
        reply(&mut e, &request, 300., body);
        let [error] = failures(&mut e).try_into().unwrap();
        assert_eq!(error.outcome, outcome, "{body}: {error:?}");
        assert_eq!(
            error.kind,
            outcome.map(|_| ErrorKind::Rejected),
            "{body}: {error:?}"
        );
        assert_eq!(error.venue_code.as_deref(), Some(code), "{body}: {error:?}");
    }
}

#[test]
fn protective_final_claim_rechecks_a_held_signed_cancel() {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    for (claim_at, crossed) in [(6000., false), (330., true)] {
        let (mut e, _, public) = guarded_fixture();
        let inspect = protective_amend(&mut e, Some(70.), 300.);
        inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "no raw protective write: {actions:?}"
        );
        let (preparation, request) = actions
            .into_iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => Some((preparation, request)),
                    _ => None,
                },
                _ => None,
            })
            .expect("genuine cancel preparation");
        assert!(request.url.ends_with("/planorder/cancel"));
        e.http_prepared(
            320.,
            preparation,
            Prepared {
                id: request.id.0,
                expires_at: None,
            },
        );
        let permit = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("genuine signed cancel binding");
        if crossed {
            quote(&mut e, public, 325., "\"fairPrice\":54000");
        }
        assert!(!matches!(
            e.claim_http(claim_at, permit),
            Ok(Claim::Released(_))
        ));
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Http(_))),
            "{actions:?}"
        );
        assert_eq!(
            e.store().order("guarded").unwrap().status,
            OrderStatus::Open
        );
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
                _ => None,
            })
            .expect("one final claim refusal");
        assert!(
            error.message.contains(if crossed {
                "crossed"
            } else {
                "reference stale"
            }),
            "{error:?}"
        );
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 91, .. })))
                .count(),
            1,
            "{actions:?}"
        );
    }
}

fn held_protective_packet(
    e: &mut TestEngine,
) -> (
    safe_cex_engine::request_context::Preparation,
    safe_cex_engine::HttpRequest,
) {
    use safe_cex_engine::request_context::HttpAction;
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })),
        "no legacy protective sender: {actions:?}"
    );
    let packets: Vec<_> = actions
        .into_iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let [packet] = packets.try_into().expect("one genuine preparation");
    packet
}
fn bind_protective_packet(
    e: &mut TestEngine,
    packet: (
        safe_cex_engine::request_context::Preparation,
        safe_cex_engine::HttpRequest,
    ),
    at: f64,
) -> (
    safe_cex_engine::request_context::Permit,
    safe_cex_engine::HttpRequest,
) {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    e.http_prepared(
        at,
        packet.0,
        Prepared {
            id: packet.1.id.0,
            expires_at: None,
        },
    );
    let packets: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let [packet] = packets.try_into().expect("one genuine binding");
    packet
}
#[test]
fn protective_held_claim_refuses_changed_filled_and_missing_position() {
    use safe_cex_engine::request_context::Claim;
    for changed in ["order", "fill", "position"] {
        let (mut e, private, _) = guarded_fixture();
        let inspect = protective_amend(&mut e, Some(70.), 300.);
        inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
        let packet = held_protective_packet(&mut e);
        let (permit, _) = bind_protective_packet(&mut e, packet, 320.);
        let text=match changed {
            "order"=>format!(r#"{{"channel":"push.personal.plan.order","data":{}}}"#,protective_plan_row(1).replace("55000","56000")),
            "fill"=>format!(r#"{{"channel":"push.personal.plan.order","data":{}}}"#,protective_plan_row(1).replace("\"dealVol\":0","\"dealVol\":1")),
            _=>r#"{"channel":"push.personal.position","data":{"positionId":"pos","symbol":"BTC_USDT","positionType":1,"holdVol":0,"holdAvgPrice":60000}}"#.into(),
        };
        e.handle(
            325.,
            Event::WsMessage {
                socket: private,
                text: &text,
            },
        );
        e.drain_actions().for_each(drop);
        assert!(
            !matches!(e.claim_http(330., permit), Ok(Claim::Released(_))),
            "{changed}"
        );
        let errors = failures(&mut e);
        assert_eq!(errors.len(), 1, "{changed}: {errors:?}");
        assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
    }
}
#[test]
fn protective_native_and_restore_recheck_after_the_signed_bind() {
    use safe_cex_engine::request_context::Claim;
    for restore in [false, true] {
        let (mut e, _, public) = guarded_fixture();
        if restore {
            let replacement = guarded_replacement(&mut e, 300.);
            reply(
                &mut e,
                &replacement,
                340.,
                r#"{"success":false,"code":2006,"message":"replacement refused"}"#,
            );
        } else {
            let inspect = protective_amend(&mut e, None, 300.);
            inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
        }
        let packet = held_protective_packet(&mut e);
        let (permit, request) = bind_protective_packet(&mut e, packet, 350.);
        assert!(request.url.ends_with(if restore {
            "/planorder/place/v2"
        } else {
            "/planorder/change_price"
        }));
        quote(&mut e, public, 355., "\"fairPrice\":53000");
        assert!(!matches!(
            e.claim_http(360., permit),
            Ok(Claim::Released(_))
        ));
        let errors = failures(&mut e);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(
            e.store().order("guarded").unwrap().status,
            if restore {
                OrderStatus::Canceled
            } else {
                OrderStatus::Open
            }
        );
        if restore {
            assert_eq!(
                errors[0].protection,
                Some(safe_cex_engine::error::Protection::Unprotected)
            );
        }
    }
}
#[test]
fn protective_preparation_expiry_advances_the_parked_fifo_once() {
    let (mut e, private, public) = guarded_fixture();
    let inspect = protective_amend(&mut e, Some(70.), 300.);
    inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
    let packet = held_protective_packet(&mut e);
    e.handle(
        315.,
        Event::WsMessage {
            socket: private,
            text: &format!(
                r#"{{"channel":"push.personal.plan.order","data":{}}}"#,
                protective_plan_row(1).replace("guarded", "peer")
            ),
        },
    );
    e.drain_actions().for_each(drop);
    e.handle(
        320.,
        Event::Command(Command::AmendOrder {
            command: 92,
            id: "peer".into(),
            price: Some(53000.),
            amount: None,
        }),
    );
    let parked: Vec<_> = e.drain_actions().collect();
    assert!(
        !parked.iter().any(|a| matches!(
            a,
            Action::Http(_)
                | Action::ManagedHttp(_)
                | Action::Notice(Notice::CommandFailed { command: 92, .. })
        )),
        "{parked:?}"
    );
    quote(&mut e, public, 20300., "\"fairPrice\":60000");
    e.http_prepared(
        20310.,
        packet.0,
        safe_cex_engine::request_context::Prepared {
            id: packet.1.id.0,
            expires_at: None,
        },
    );
    let actions = support::prepared(&mut e, 20310.);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 91, .. })))
            .count(),
        1,
        "{actions:?}"
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a,Action::Http(r) if r.method==safe_cex_engine::Method::Get))
            .count(),
        1,
        "{actions:?}"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.method!=safe_cex_engine::Method::Get)),
        "{actions:?}"
    );
    e.http_prepared(
        20311.,
        packet.0,
        safe_cex_engine::request_context::Prepared {
            id: packet.1.id.0,
            expires_at: None,
        },
    );
    assert!(!e.drain_actions().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { command: 91, .. })
    )));
}
#[test]
fn opening_parent_with_brackets_amend_keeps_the_legacy_wire() {
    let (mut e, private, reads, _) = private_started();
    for read in reads {
        response(&mut e, &read, r#"{"success":true,"code":0,"data":[]}"#);
    }
    let raw = r#"{"orderId":"one","symbol":"BTC_USDT","side":1,"orderType":1,"vol":60,"dealVol":0,"price":59000,"state":2,"stopLossPrice":55000,"takeProfitPrice":65000,"lossTrend":3,"profitTrend":2}"#;
    ws(
        &mut e,
        private,
        &format!(r#"{{"channel":"push.personal.order","data":{raw}}}"#),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 92,
            id: "one".into(),
            price: Some(58000.),
            amount: None,
        }),
    );
    let mut wire = Vec::new();
    for (at, body, path) in [
        (
            2050.,
            format!(r#"{{"success":true,"code":0,"data":{raw}}}"#),
            "/order/get/one",
        ),
        (
            2100.,
            r#"{"success":true,"code":0,"data":[{"orderId":"one","errorCode":0}]}"#.into(),
            "/order/cancel",
        ),
        (
            2200.,
            format!(
                r#"{{"success":true,"code":0,"data":{}}}"#,
                raw.replace("\"state\":2", "\"state\":4")
            ),
            "/order/get/one",
        ),
    ] {
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            !actions.iter().any(|a| matches!(a, Action::ManagedHttp(_))),
            "opening parent is unchanged legacy custody"
        );
        let request = actions
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .expect("legacy opening-parent wire");
        assert!(request.url.ends_with(path));
        wire.push(serde_json::json!({"method":request.method.as_str(),"url":request.url,"headers":request.headers,"body":request.body}));
        reply(&mut e, &request, at, &body);
    }
    let request = requests(&mut e).remove(0);
    assert!(request.url.ends_with("/order/submit"));
    let body: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["symbol"], "BTC_USDT");
    assert_eq!(body["side"], 1);
    assert_eq!(body["type"], 1);
    assert_eq!(body["vol"], 60.);
    assert_eq!(body["price"], "58000");
    assert_eq!(body["stopLossPrice"], "55000");
    assert_eq!(body["takeProfitPrice"], "65000");
    assert_eq!(body["lossTrend"], 3);
    assert_eq!(body["profitTrend"], 2);
    wire.push(serde_json::json!({"method":request.method.as_str(),"url":request.url,"headers":request.headers,"body":request.body}));
    let expected:serde_json::Value=serde_json::from_str(r#"[{"body":null,"headers":[["ApiKey","replay-key"],["Request-Time","2000"],["Signature","479a287be3d159fd6fb4b0d1785deadcb75e8b5421ca75c89d9d1c0a27219df3"],["Recv-Window","20000"],["Content-Type","application/json"],["source","TEAL"]],"method":"GET","url":"https://api.mexc.com/api/v1/private/order/get/one"},{"body":"[\"one\"]","headers":[["ApiKey","replay-key"],["Request-Time","2050"],["Signature","bf068177982dc8f27511a549137d8342d32a49c53b306bfb977fdfce93156095"],["Recv-Window","20000"],["Content-Type","application/json"],["source","TEAL"]],"method":"POST","url":"https://api.mexc.com/api/v1/private/order/cancel"},{"body":null,"headers":[["ApiKey","replay-key"],["Request-Time","2100"],["Signature","2525c94bd9a3d2e4e98a98bb3f64298cf30dfd1822da85040d6e67299a1a25fb"],["Recv-Window","20000"],["Content-Type","application/json"],["source","TEAL"]],"method":"GET","url":"https://api.mexc.com/api/v1/private/order/get/one"},{"body":"{\"symbol\":\"BTC_USDT\",\"price\":\"58000\",\"vol\":60.0,\"side\":1,\"type\":1,\"openType\":2,\"leverage\":20.0,\"externalOid\":\"ts_j7sHRjFY3nmpX4uRVHBu\",\"stopLossPrice\":\"55000\",\"takeProfitPrice\":\"65000\",\"lossTrend\":3,\"profitTrend\":2}","headers":[["ApiKey","replay-key"],["Request-Time","2200"],["Signature","4926bb61f567d08fa817c0c0d52350b9b5646a3add4af021d95122d408b35f26"],["Recv-Window","20000"],["Content-Type","application/json"],["source","TEAL"]],"method":"POST","url":"https://api.mexc.com/api/v1/private/order/submit"}]"#).unwrap();
    assert_eq!(serde_json::Value::Array(wire), expected);
}

#[test]
fn protective_unknown_seal_does_not_strand_a_queued_ordinary_job() {
    let (mut e, _, _) = guarded_fixture();
    let inspect = protective_amend(&mut e, None, 300.);
    inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
    let native = requests(&mut e).remove(0);
    e.handle(
        315.,
        Event::Command(Command::SetLeverage {
            command: 92,
            symbol: "BTCUSDT".into(),
            leverage: 5.,
            side: Some(safe_cex_engine::model::PositionSide::Long),
        }),
    );
    assert!(!e.drain_actions().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { command: 92, .. })
    )));
    e.handle(
        320.,
        Event::HttpResponse {
            request: native.id,
            response: Response {
                status: 503,
                headers: "",
                body: r#"{"success":false,"code":500}"#,
            },
        },
    );
    let actions = support::prepared(&mut e, 320.);
    assert_eq!(actions.iter().filter(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:91,error}) if error.outcome==Some(safe_cex_engine::Outcome::Unknown))).count(),1,"{actions:?}");
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a,Action::Http(r) if r.url.ends_with("/position/change_leverage")))
            .count(),
        1,
        "{actions:?}"
    );
    assert!(!actions.iter().any(|a|matches!(a,Action::Http(r) if r.url.ends_with("/planorder/change_price") || r.url.ends_with("/planorder/place/v2"))),"no replay: {actions:?}");
}

#[test]
fn protective_ambiguous_success_data_is_unknown_without_store_effects() {
    for native in [true, false] {
        for data in [
            r#""invalid acknowledgement""#,
            r#"{"errorCode":"invalid"}"#,
            r#"[{"id":"first","errorCode":0},{"id":"second","errorCode":0}]"#,
            r#"{"id":"first","orderId":"second","errorCode":0}"#,
        ] {
            let (mut e, _, _) = guarded_fixture();
            let request = if native {
                let inspect = protective_amend(&mut e, None, 300.);
                inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
                requests(&mut e).remove(0)
            } else {
                guarded_replacement(&mut e, 300.)
            };
            let before = e.store().orders().to_vec();
            reply(
                &mut e,
                &request,
                340.,
                &format!(r#"{{"success":true,"code":0,"data":{data}}}"#),
            );
            let actions = support::prepared(&mut e, 340.);
            assert_eq!(e.store().orders(), before, "native={native}, data={data}");
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a,Action::Http(r) if r.method!=safe_cex_engine::Method::Get)),
                "{actions:?}"
            );
            assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:91,error}) if error.outcome==Some(safe_cex_engine::Outcome::Unknown))),"native={native}, data={data}: {actions:?}");
        }
    }
}

#[test]
fn protective_engine_sealed_unknown_reports_uncertainty_on_every_write_leg() {
    use safe_cex_engine::request_context::HttpAction;
    for leg in ["cancel", "replacement", "put-back"] {
        for status in [408, 500, 503] {
            let (mut e, _, _) = guarded_fixture();
            let request = if leg == "cancel" {
                let inspect = protective_amend(&mut e, Some(70.), 300.);
                inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
                let request = requests(&mut e).remove(0);
                assert!(request.url.ends_with("/planorder/cancel"));
                request
            } else {
                let replacement = guarded_replacement(&mut e, 300.);
                if leg == "put-back" {
                    reply(
                        &mut e,
                        &replacement,
                        335.,
                        r#"{"success":false,"code":2006,"message":"replacement refused"}"#,
                    );
                    let restore = requests(&mut e).remove(0);
                    let body: serde_json::Value =
                        serde_json::from_str(restore.body.as_deref().unwrap()).unwrap();
                    assert_eq!(body["vol"], 60.);
                    assert_eq!(body["triggerPrice"], 55000.);
                    restore
                } else {
                    replacement
                }
            };
            e.handle(
                340.,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status,
                        headers: "",
                        body: "{}",
                    },
                },
            );
            let actions = drained(&mut e);
            let errors: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(errors.len(), 1, "{leg}/{status}: {actions:?}");
            let error = errors[0];
            assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
            assert!(error.protection.is_none(), "{leg}/{status}: {error:?}");
            assert_eq!(
                error.message,
                "The outcome of your BTCUSDT protection update is unknown. Check Open Orders and your position before replacing protection.",
                "{leg}/{status}"
            );
            assert!(!actions.iter().any(|a| match a {
                Action::Http(r) => r.method != safe_cex_engine::Method::Get,
                Action::ManagedHttp(a) => matches!(&**a, HttpAction::Prepare { request, .. } | HttpAction::Bind { request, .. }
                    if request.method != safe_cex_engine::Method::Get),
                Action::Notice(Notice::CommandDone { command: 91, .. }) => true,
                _ => false,
            }), "{leg}/{status}: no restore or resend: {actions:?}");
            e.handle(
                341.,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status,
                        headers: "",
                        body: "{}",
                    },
                },
            );
            let late = drained(&mut e);
            assert!(
                !late.iter().any(|a| matches!(
                    a,
                    Action::Http(_)
                        | Action::ManagedHttp(_)
                        | Action::Notice(
                            Notice::CommandDone { command: 91, .. }
                                | Notice::CommandFailed { command: 91, .. }
                        )
                )),
                "{leg}/{status}: duplicate failure is inert: {late:?}"
            );
        }
    }
}

#[test]
fn protective_replacement_rechecks_after_the_signed_bind() {
    use safe_cex_engine::request_context::{Claim, HttpAction};
    for changed in [
        "allowed",
        "stale",
        "crossed",
        "position",
        "not-canceled",
        "order",
    ] {
        let (mut e, private, public) = guarded_fixture();
        let inspect = protective_amend(&mut e, Some(70.), 300.);
        inspect_reply(&mut e, &inspect, 310., &protective_plan_row(1));
        let cancel = requests(&mut e).remove(0);
        assert!(cancel.url.ends_with("/planorder/cancel"));
        reply(&mut e, &cancel, 320., r#"{"success":true,"code":0}"#);
        let terminal = requests(&mut e).remove(0);
        assert!(terminal.url.contains("states=2"));
        inspect_reply(&mut e, &terminal, 330., &protective_plan_row(2));
        let packet = held_protective_packet(&mut e);
        assert!(packet.1.url.ends_with("/planorder/place/v2"));
        let (permit, request) = bind_protective_packet(&mut e, packet, 340.);
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["vol"], 70.);
        assert_eq!(body["triggerPrice"], 54000.);
        match changed {
            "crossed" => quote(&mut e, public, 345., "\"fairPrice\":54000"),
            "position" | "not-canceled" | "order" => {
                let text = match changed {
                    "position" => r#"{"channel":"push.personal.position","data":{"positionId":"pos","symbol":"BTC_USDT","positionType":1,"holdVol":0,"holdAvgPrice":60000}}"#.to_owned(),
                    "not-canceled" => format!(r#"{{"channel":"push.personal.plan.order","data":{}}}"#, protective_plan_row(3)),
                    _ => format!(r#"{{"channel":"push.personal.plan.order","data":{}}}"#, protective_plan_row(2).replace("55000", "56000")),
                };
                e.handle(
                    345.,
                    Event::WsMessage {
                        socket: private,
                        text: &text,
                    },
                );
                assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
            }
            _ => {}
        }
        if changed == "not-canceled" {
            assert_ne!(
                e.store().order("guarded").unwrap().status,
                OrderStatus::Canceled
            );
        }
        let claim = e.claim_http(if changed == "stale" { 6000. } else { 350. }, permit);
        if changed == "allowed" {
            assert!(
                matches!(claim, Ok(Claim::Released(_))),
                "healthy replacement claim: {claim:?}"
            );
            continue;
        }
        assert!(!matches!(claim, Ok(Claim::Released(_))), "{changed}");
        let actions = drained(&mut e);
        assert!(
            !actions.iter().any(|a| match a {
                Action::Http(_) | Action::Sign { .. } => true,
                Action::ManagedHttp(a) => !matches!(&**a, HttpAction::Cancel { .. }),
                _ => false,
            }),
            "{changed}: no replacement or restore: {actions:?}"
        );
        let errors: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1, "{changed}: {actions:?}");
        assert_eq!(
            errors[0].protection,
            Some(safe_cex_engine::error::Protection::Unprotected)
        );
    }
}
