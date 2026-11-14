use super::*;

fn fresh(e: &mut Engine<Coinbase>, command_id: u32) -> Vec<Action> {
    let mut o = opts();
    o.client_order_id = Some(format!("fresh-{command_id}"));
    command(
        e,
        Command::PlaceOrders {
            command: command_id,
            orders: vec![o],
        },
    )
}
fn refuse_if_dispatched(e: &mut Engine<Coinbase>, actions: &[Action]) {
    // Exercise the old overwrite-and-refuse path as well as the fixed early refusal.
    if let Some(r) = requests(actions).into_iter().find(|r| {
        matches!(
            packet(r)["method"].as_str(),
            Some("private/cancel" | "private/set_leverage")
        )
    }) {
        let body = json!({"jsonrpc":"2.0","id":packet(&r)["id"],"error":fixtures()["insufficient_margin"]}).to_string();
        e.handle(
            safe_cex_engine::test_support::dispatch_now(e),
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
            },
        );
        prepared_now(e);
    }
}
#[test]
fn duplicate_cancel_cannot_clear_the_original_unknown_fence() {
    let mut e = engine();
    let r = place(&mut e, opts());
    reply(&mut e, &r, fixtures()["partial"].clone());
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 2,
            ids: vec!["USDC-94166".into()],
        },
    );
    let cancel = requests(&a).pop().unwrap();
    e.handle(
        safe_cex_engine::test_support::dispatch_now(&e).max(1002.),
        Event::HttpFailed {
            request: cancel.id,
            error: "synthetic loss",
        },
    );
    let a = prepared_now(&mut e);
    let read = requests(&a)
        .into_iter()
        .find(|r| packet(r)["method"] == "private/get_order_state")
        .unwrap();
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 3,
            ids: vec!["USDC-94166".into()],
        },
    );
    refuse_if_dispatched(&mut e, &a);
    assert!(
        requests(&fresh(&mut e, 4)).is_empty(),
        "a later refusal must not erase older uncertainty"
    );
    assert!(
        requests(&a)
            .iter()
            .all(|r| packet(r)["method"] != "private/cancel")
    );
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:3,error}) if error.outcome==Some(Outcome::NotSent))));
    let a = reply(&mut e, &read, fixtures()["partial"]["order"].clone());
    assert!(
        requests(&fresh(&mut e, 5)).is_empty(),
        "unchanged read keeps original uncertainty"
    );
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms < 10000. => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(timer.1, Event::TimerFired { timer: timer.0 });
    let a = prepared_now(&mut e);
    let read = requests(&a)
        .into_iter()
        .find(|r| packet(r)["method"] == "private/get_order_state")
        .unwrap();
    let mut canceled = fixtures()["partial"]["order"].clone();
    canceled["order_state"] = json!("cancelled");
    reply(&mut e, &read, canceled);
    assert!(
        requests(&fresh(&mut e, 6))
            .iter()
            .any(|r| packet(r)["method"] == "private/buy"),
        "terminal original read releases its fence"
    );
}
#[test]
fn overlapping_leverage_cannot_replace_the_first_attempts_recovery() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 1,
            symbol: SYMBOL.into(),
            leverage: 5.,
            side: None,
        },
    );
    let first = requests(&a).pop().unwrap();
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 2,
            symbol: SYMBOL.into(),
            leverage: 7.,
            side: None,
        },
    );
    refuse_if_dispatched(&mut e, &a);
    e.handle(
        safe_cex_engine::test_support::dispatch_now(&e).max(1002.),
        Event::HttpFailed {
            request: first.id,
            error: "synthetic loss",
        },
    );
    let a = prepared_now(&mut e);
    assert!(
        requests(&a)
            .iter()
            .any(|r| packet(r)["method"] == "private/get_leverage"),
        "first attempt retains its native read-back"
    );
    assert!(requests(&fresh(&mut e, 3)).is_empty());
}
fn protection() -> PlaceOrderOpts {
    let mut o = opts();
    o.kind = OrderType::StopLoss;
    o.side = OrderSide::Sell;
    o.reduce_only = Some(true);
    o.price = Some(60000.);
    o.trigger_type = Some(StopTriggerType::MarkPrice);
    o
}
fn protective_row() -> Value {
    let mut row = fixtures()["partial"]["order"].clone();
    row["direction"] = json!("sell");
    row["order_type"] = json!("stop_market");
    row["reduce_only"] = json!(true);
    row["price"] = json!("market_price");
    row["trigger_price"] = json!(60000.);
    row["trigger"] = json!("mark_price");
    row["filled_amount"] = json!(0.);
    row["order_state"] = json!("untriggered");
    row
}
#[test]
fn protective_ack_and_readback_must_match_trigger_price_and_basis() {
    for (field, value) in [
        ("trigger_price", json!(70000.)),
        ("trigger_price", Value::Null),
        ("trigger", json!("last_price")),
        ("trigger", Value::Null),
    ] {
        let mut e = engine();
        let r = place(&mut e, protection());
        let mut row = protective_row();
        row[field] = value;
        let a = reply(&mut e, &r, json!({"order":row,"trades":[]}));
        assert!(!done(&a), "unproven {field} cannot acknowledge protection");
        assert!(e.store().orders().is_empty());
        let read = requests(&a)
            .into_iter()
            .find(|r| packet(r)["method"] == "private/get_order_state_by_label")
            .unwrap();
        reply(&mut e, &read, json!([row]));
        assert!(e.store().orders().is_empty());
        assert!(
            requests(&fresh(&mut e, 2)).is_empty(),
            "mismatched read retains the fence"
        );
    }
    let mut e = engine();
    let r = place(&mut e, protection());
    assert!(done(&reply(
        &mut e,
        &r,
        json!({"order":protective_row(),"trades":[]})
    )));
    assert_eq!(
        e.store().order("USDC-94166").unwrap().trigger_type,
        Some(StopTriggerType::MarkPrice)
    );
}

#[test]
fn default_leverage_query_and_recovery_validate_the_native_optional_value() {
    for (custom, max, expected) in [
        (Some(Value::Null), Some(json!(50.)), Some(50.)),
        (None, Some(json!(50.)), Some(50.)),
        (Some(json!(5.)), Some(json!(50.)), Some(5.)),
        (Some(Value::Null), None, None),
        (Some(json!(5.)), None, None),
        (Some(json!(75.)), Some(json!(50.)), None),
        (Some(json!("5")), Some(json!(50.)), None),
        (Some(json!(false)), Some(json!(50.)), None),
        (Some(Value::Null), Some(json!(-1.)), None),
    ] {
        for recovery in [false, true] {
            let mut e = engine();
            let cmd = if recovery {
                Command::SetLeverage {
                    command: 1,
                    symbol: SYMBOL.into(),
                    leverage: 5.,
                    side: None,
                }
            } else {
                Command::FetchLeverage {
                    command: 1,
                    symbol: SYMBOL.into(),
                }
            };
            let a = command(&mut e, cmd);
            let mut r = requests(&a).pop().unwrap();
            if recovery {
                e.handle(
                    safe_cex_engine::test_support::dispatch_now(&e).max(1001.),
                    Event::HttpFailed {
                        request: r.id,
                        error: "synthetic loss",
                    },
                );
                r = requests(&prepared_now(&mut e))
                    .into_iter()
                    .find(|r| packet(r)["method"] == "private/get_leverage")
                    .unwrap();
            }
            let mut result = json!({"instrument_name":"BTC_USDC-PERPETUAL"});
            if let Some(value) = &custom {
                result["leverage"] = value.clone();
            }
            if let Some(value) = &max {
                result["max_leverage"] = value.clone();
            }
            let a = reply(&mut e, &r, result);
            if let Some(expected) = expected {
                assert_eq!(
                    e.store().setting(SYMBOL).map(|s| s.leverage),
                    Some(expected),
                    "valid native default/custom leverage must publish"
                );
                if !recovery {
                    assert!(done(&a));
                } else {
                    assert!(
                        requests(&fresh(&mut e, 2))
                            .iter()
                            .any(|r| packet(r)["method"] == "private/buy")
                    );
                }
            } else {
                assert!(!done(&a));
                assert!(e.store().setting(SYMBOL).is_none());
                if recovery {
                    assert!(
                        requests(&fresh(&mut e, 2)).is_empty(),
                        "malformed read must retain uncertainty"
                    );
                }
            }
        }
    }
}

#[test]
fn internal_market_limit_ack_and_readback_keep_the_original_market_intent() {
    for original in [None, Some("market"), Some("market_limit")] {
        for recovery in [false, true] {
            let mut e = engine();
            let mut o = opts();
            o.kind = OrderType::Market;
            let r = place(&mut e, o);
            let mut row = fixtures()["partial"]["order"].clone();
            row["time_in_force"] = json!("immediate_or_cancel");
            if let Some(original) = original {
                row["original_order_type"] = json!(original);
            }
            let a = if recovery {
                e.handle(
                    safe_cex_engine::test_support::dispatch_now(&e).max(1001.),
                    Event::HttpFailed {
                        request: r.id,
                        error: "synthetic loss",
                    },
                );
                let read = requests(&prepared_now(&mut e))
                    .into_iter()
                    .find(|r| packet(r)["method"] == "private/get_order_state_by_label")
                    .unwrap();
                reply(&mut e, &read, json!([row]))
            } else {
                reply(&mut e, &r, json!({"order":row,"trades":[]}))
            };
            if original.is_some() {
                assert_eq!(
                    e.store().order("USDC-94166").map(|o| o.kind),
                    Some(OrderType::Market),
                    "documented original market metadata must survive internal limit execution"
                );
                if !recovery {
                    assert!(done(&a));
                }
                assert!(
                    requests(&fresh(&mut e, 2))
                        .iter()
                        .any(|r| packet(r)["method"] == "private/buy")
                );
            } else {
                assert!(!done(&a));
                assert!(e.store().orders().is_empty());
                assert!(
                    requests(&fresh(&mut e, 2)).is_empty(),
                    "a genuine limit cannot prove a market placement"
                );
            }
        }
    }
    let mut e = engine();
    let r = place(&mut e, opts());
    let mut result = fixtures()["partial"].clone();
    result["order"]["original_order_type"] = json!("market");
    assert!(
        !done(&reply(&mut e, &r, result)),
        "original market metadata cannot prove a requested limit"
    );
    assert!(e.store().orders().is_empty());
}
