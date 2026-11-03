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
        response(&mut e, &r, r#"{"success":true,"code":0,"data":[]}"#);
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
    let cancel = requests(&mut e).remove(0);
    e.handle(
        2100.0,
        Event::HttpResponse {
            request: cancel.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"success":true,"code":0}"#,
            },
        },
    );
    let replacement = requests(&mut e).remove(0);
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
        let actions = prepared(&mut e, 400.0);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. }))),
            "a full-position stop must resolve its native quantity before checking the minimum: {actions:?}"
        );
        let cancel = actions
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .expect("native stop cancellation");
        assert!(cancel.url.ends_with("/stoporder/cancel"));
        if let Some(volume) = updated_position {
            e.handle(
                425.0,
                Event::WsMessage {
                    socket,
                    text: &format!(r#"{{"channel":"push.personal.position","data":{{"positionId":"native","symbol":"BTC_USDT","positionType":1,"positionMode":1,"holdVol":{volume},"holdAvgPrice":60000,"leverage":7,"openType":1}}}}"#),
                },
            );
            e.drain_actions().for_each(drop);
        }
        reply(&mut e, &cancel, 450.0, r#"{"success":true,"code":0}"#);
        let replacement = requests(&mut e).remove(0);
        let body: serde_json::Value =
            serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
        if amount.unwrap_or(0.0) == 0.0 {
            assert!(replacement.url.ends_with("/stoporder/place"));
            assert_eq!(body["positionId"], "native");
            assert_eq!(body["vol"], updated_position.unwrap_or(60.0));
            assert_eq!(body["volType"], 2);
            assert_eq!(
                body[if take_profit {
                    "takeProfitPrice"
                } else {
                    "stopLossPrice"
                }],
                if take_profit { "64000" } else { "54000" }
            );
        } else {
            assert!(replacement.url.ends_with("/planorder/place/v2"));
            assert_eq!(body["vol"], amount.unwrap());
        }
    }
}

#[test]
fn price_only_full_position_stop_amend_resolves_current_contract_quantity() {
    amend_full_position_stop(None, None);
    amend_full_position_stop(None, Some(25.0));
}

#[test]
fn explicit_full_position_stop_amend_keeps_zero_sentinel_and_positive_contract_amounts() {
    amend_full_position_stop(Some(0.0), None);
    amend_full_position_stop(Some(20.0), None);
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
    let cancel = requests(&mut e).remove(0);
    assert!(cancel.url.ends_with("/stoporder/cancel"));
    reply(&mut e, &cancel, 450.0, r#"{"success":true,"code":0}"#);
    let replacement = requests(&mut e).remove(0);
    assert!(replacement.url.ends_with("/stoporder/place"));
    let body: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["vol"], 60.0);
    assert_eq!(body["volType"], 2);
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
