mod support;
use safe_cex_engine::model::{BybitBbo, OrderSide, OrderType, PlaceOrderOpts, StopTriggerType};
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";
fn assert_unknown(h: &Harness, command: u32, placed: &[&str]) {
    let errors: Vec<_> = h
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "one journal terminal");
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert!(
        errors[0].message.contains(ENGINE_UNKNOWN_TEXT),
        "{}",
        errors[0].message
    );
    let ids = errors[0]
        .partial
        .as_ref()
        .map(|p| p.order_ids.iter().map(String::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(ids, placed);
}

fn assert_unknown_inventory(actions: &[Action], command: u32, expected: &[(&str, &str, &str)]) {
    let inventory: Vec<_> = actions
        .iter()
        .filter_map(|action| {
            let Action::Notice(Notice::Event(event)) = action else {
                return None;
            };
            if event.code != "command.unknown_inventory" {
                return None;
            }
            assert_eq!(event.level, safe_cex_engine::Level::Warn);
            assert!(event.message.contains(ENGINE_UNKNOWN_TEXT));
            assert!(event.fields.iter().any(|(key, value)| *key == "commandId"
                && *value == safe_cex_engine::Value::from(command)));
            let field = |name| {
                event
                    .fields
                    .iter()
                    .find(|(key, _)| *key == name)
                    .and_then(|(_, value)| value.as_str())
                    .unwrap_or("")
            };
            if field("scope") != "accepted" {
                assert!(!field("requestPath").is_empty());
            }
            let (kind, identity) = ["orderId", "clientOrderId", "targetOrderId"]
                .into_iter()
                .find_map(|key| (!field(key).is_empty()).then_some((key, field(key))))
                .unwrap_or(("", ""));
            Some((field("scope"), kind, identity))
        })
        .collect();
    assert_eq!(
        inventory, expected,
        "passive identity inventory, separate from the journal terminal"
    );
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts:630-636
fn after_failure_middle_row_refusal_continues_but_unknown_stops_last_trigger() {
    for lost in [false, true] {
        let mut h = Harness::new();
        h.load();
        let first = order();
        let mut second = order();
        second.client_order_id = Some("middle".into());
        let mut last = order();
        last.client_order_id = None;
        last.kind = OrderType::StopLoss;
        last.price = Some(80.0);
        command(
            &mut h,
            Command::PlaceOrders {
                command: 995,
                orders: vec![first, second, last],
            },
        );
        let first = h.http("private/create-order");
        h.respond(&first, r#"{"code":0,"result":{"order_id":"101"}}"#);
        pace(&mut h);
        let middle = h.http("private/create-order");
        if lost {
            h.event(Event::HttpFailed {
                request: middle.id,
                error: "middle row response lost",
            });
        } else {
            h.respond(
                &middle,
                r#"{"code":213,"message":"INSUFFICIENT_AVAILABLE_BALANCE"}"#,
            );
        }
        pace(&mut h);
        if lost {
            assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(request)
                if request.url.ends_with("private/create-order") || request.url.ends_with("private/advanced/create-order"))));
            assert_unknown(&h, 995, &["101"]);
            continue;
        }
        assert!(h.actions.iter().any(|a| matches!(a, Action::Http(request) if request.url.ends_with("private/advanced/create-order"))), "definite refusal allows the last trigger");
        let last = h.http("private/advanced/create-order");
        assert_ne!(last.id, middle.id);
        assert_ne!(last.id, first.id);
        h.respond(&last, r#"{"code":0,"result":{"order_id":"103"}}"#);
        pace(&mut h);
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(request)
            if request.url.ends_with("private/create-order") || request.url.ends_with("private/advanced/create-order"))));
        let failures: Vec<_> = h
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed {
                    command: 995,
                    error,
                }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1);
        assert_eq!(
            failures[0].outcome,
            lost.then_some(safe_cex_engine::Outcome::Unknown)
        );
        assert_eq!(
            failures[0]
                .partial
                .as_ref()
                .expect("accepted prefix and last trigger remain reported")
                .order_ids,
            ["101", "103"]
        );
    }
}
fn order() -> PlaceOrderOpts {
    serde_json::from_str(r#"{"symbol":"BTCUSD-PERP","type":"limit","side":"buy","amount":0.0021,"price":90.04,"clientOrderId":"caller-123"}"#).unwrap()
}
fn command(h: &mut Harness, c: Command) {
    h.event(Event::Command(c));
}
fn pace(h: &mut Harness) {
    if h.actions
        .iter()
        .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        && !h
            .actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 20.0))
    {
        return;
    }
    let i = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 20.0))
        .unwrap();
    let Action::SetTimer { timer, at_ms } = h.actions.remove(i) else {
        unreachable!()
    };
    h.now = at_ms;
    h.event(Event::TimerFired { timer });
}

#[test]
fn caller_receipt_precedes_write_and_missing_id_stays_missing() {
    for supplied in [false, true] {
        let mut h = Harness::new();
        h.load();
        h.actions.clear();
        let mut o = order();
        if !supplied {
            o.client_order_id = None;
        }
        command(
            &mut h,
            Command::PlaceOrders {
                command: 71,
                orders: vec![o],
            },
        );
        let http = h
            .actions
            .iter()
            .position(|a| matches!(a, Action::Http(_)))
            .unwrap();
        let receipts: Vec<_> = h
            .actions
            .iter()
            .enumerate()
            .filter_map(|(i, action)| {
                if let Action::Notice(Notice::CommandProgress {
                    command: 71,
                    receipts,
                }) = action
                {
                    assert!(i < http);
                    Some(receipts)
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        let Action::Http(request) = &h.actions[http] else {
            unreachable!()
        };
        let payload: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        if supplied {
            assert_eq!(receipts.len(), 1);
            assert_eq!(receipts[0].symbol, "BTCUSD-PERP");
            assert_eq!(
                receipts[0].kind,
                safe_cex_engine::OrderIdentityKind::ClientOrderId
            );
            assert_eq!(payload["params"]["client_oid"], receipts[0].identity);
        } else {
            assert!(receipts.is_empty());
            assert!(payload["params"].get("client_oid").is_none());
        }
    }
}
#[test]
fn caller_id_and_precision_are_sent_verbatim() {
    let mut h = Harness::new();
    h.load();
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        },
    );
    let r = h.http("private/create-order");
    let body = r.body.as_deref().unwrap();
    assert!(body.contains(r#""client_oid":"caller-123""#));
    assert!(body.contains(r#""quantity":"0.0021""#));
    assert!(body.contains(r#""price":"90.04""#));
    h.respond(&r, r#"{"code":0,"result":{"order_id":9007199254740993}}"#);
    pace(&mut h);
    assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1, result: s }) if s == r#"["9007199254740993"]"#)));
}
#[test]
fn invalid_or_duplicate_caller_ids_reject_entire_batch_before_writes() {
    for invalid in ["", "caller-123", "abcdefghijklmnopqrstuvwxyz12345678901"] {
        let mut h = Harness::new();
        h.load();
        let mut second = order();
        second.client_order_id = Some(invalid.into());
        command(
            &mut h,
            Command::PlaceOrders {
                command: 1,
                orders: vec![order(), second],
            },
        );
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
        );
    }
}

#[test]
fn bybit_execution_controls_reject_entire_batch_before_writes() {
    for bbo in [true, false] {
        let mut h = Harness::new();
        h.load();
        let mut second = order();
        second.client_order_id = Some("second".into());
        if bbo {
            second.bybit_bbo = Some(BybitBbo {
                side_type: "Queue".into(),
                level: 1.0,
            });
        } else {
            second.bybit_slippage_tolerance_ticks = Some(2.0);
        }
        command(
            &mut h,
            Command::PlaceOrders {
                command: 1,
                orders: vec![order(), second],
            },
        );
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(h.actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 1, error: message }) if message.legacy_text().contains("Bybit")
        )));
    }
}
#[test]
fn reduce_only_market_sell_uses_quantity_and_advanced_stop_uses_reference_price() {
    let mut h = Harness::new();
    h.load();
    let mut opts = order();
    opts.kind = OrderType::Market;
    opts.side = OrderSide::Sell;
    opts.reduce_only = Some(true);
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        },
    );
    let r = h.http("private/create-order");
    let body = r.body.as_deref().unwrap();
    assert!(body.contains("REDUCE_ONLY"));
    assert!(!body.contains("notional"));
    assert!(!body.contains("time_in_force"));
    h.respond(&r, r#"{"code":0,"result":{"order_id":"1"}}"#);
    pace(&mut h);
    let mut opts = order();
    opts.client_order_id = None;
    opts.kind = OrderType::StopLoss;
    opts.trigger_price = Some(80.0);
    opts.sl_trigger_type = Some(StopTriggerType::IndexPrice);
    command(
        &mut h,
        Command::PlaceOrders {
            command: 2,
            orders: vec![opts],
        },
    );
    let r = h.http("advanced/create-order");
    let body = r.body.unwrap();
    assert!(body.contains(r#""ref_price":"80""#));
    assert!(body.contains("INDEX_PRICE"));
    assert!(!body.contains(r#""price":"#));
}
#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts:630-636
fn unknown_first_row_is_not_retried_and_batch_stops() {
    let mut h = Harness::new();
    h.load();
    let mut second = order();
    second.client_order_id = Some("second".into());
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order(), second],
        },
    );
    let r = h.http("private/create-order");
    h.event(Event::HttpFailed {
        request: r.id,
        error: "response lost after acceptance",
    });
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("private/create-order") || r.url.ends_with("private/advanced/create-order")))
    );
    pace(&mut h);
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(request)
        if request.url.ends_with("private/create-order"))));
    assert_unknown(&h, 1, &[]);
}

#[test]
fn native_amend_backfills_quantity_and_never_rewrites_client_id() {
    let mut h = Harness::new();
    h.load();
    h.ws("user.order", ORDER);
    command(
        &mut h,
        Command::AmendOrder {
            command: 1,
            id: "9007199254740993".into(),
            price: Some(95.0),
            amount: None,
        },
    );
    let r = h.http("private/amend-order");
    let body = r.body.unwrap();
    assert!(body.contains(r#""new_quantity":"1""#));
    assert!(body.contains(r#""new_price":"95""#));
    assert!(!body.contains("client_oid"));
}
#[test]
fn leverage_uses_account_identity_and_global_numeric_leverage() {
    let mut h = Harness::new();
    h.load();
    command(
        &mut h,
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSD-PERP".into(),
            leverage: 20.0,
            side: None,
        },
    );
    h.reply(
        "private/user-balance",
        r#"{"code":0,"result":{"data":[{"account_id":"account-123"}]}}"#,
    );
    pace(&mut h);
    let r = h.http("private/change-account-leverage");
    let body = r.body.as_deref().unwrap();
    assert!(body.contains(r#""account_id":"account-123""#));
    assert!(body.contains(r#""leverage":20"#));
    assert!(!body.contains("instrument_name"));
    h.respond(&r, r#"{"code":0}"#);
    pace(&mut h);
    assert_eq!(
        h.engine.store().setting("BTCUSD-PERP").unwrap().leverage,
        20.0
    );
}
#[test]
fn attached_brackets_and_margin_switching_fail_before_mutations() {
    let mut h = Harness::new();
    h.load();
    let mut opts = order();
    opts.client_order_id = None;
    opts.stop_loss = Some(80.0);
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        },
    );
    command(
        &mut h,
        Command::SetMarginType {
            command: 2,
            symbol: "BTCUSD-PERP".into(),
            isolated: true,
        },
    );
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn post_only_uses_execution_instructions_and_can_combine_reduce_only() {
    let mut h = Harness::new();
    h.load();
    let mut opts = order();
    opts.time_in_force = Some(safe_cex_engine::model::TimeInForce::PostOnly);
    opts.reduce_only = Some(true);
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        },
    );
    let body = h.http("private/create-order").body.unwrap();
    assert!(body.contains(r#""exec_inst":["REDUCE_ONLY","POST_ONLY"]"#));
    assert!(body.contains(r#""time_in_force":"GOOD_TILL_CANCEL""#));
}
#[test]
fn unsuccessful_http_or_missing_success_code_never_reports_write_success() {
    for status in [200, 503] {
        let mut h = Harness::new();
        h.load();
        command(
            &mut h,
            Command::CancelSymbolOrders {
                command: 1,
                symbol: "BTCUSD-PERP".into(),
            },
        );
        let r = h.http("cancel-all-orders");
        h.event(Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status,
                headers: "",
                body: "{}",
            },
        });
        pace(&mut h);
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
        );
    }
}

#[test]
fn unsuccessful_http_write_preserves_native_rejection_details() {
    let mut h = Harness::new();
    h.load();
    command(
        &mut h,
        Command::CancelSymbolOrders {
            command: 1,
            symbol: "BTCUSD-PERP".into(),
        },
    );
    let request = h.http("cancel-all-orders");
    h.event(Event::HttpResponse {
        request: request.id,
        response: safe_cex_engine::Response {
            status: 403,
            headers: "",
            body: r#"{"code":30003,"message":"INVALID_INSTRUMENT_NAME"}"#,
        },
    });
    pace(&mut h);
    assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, error }) if error.legacy_text().contains("30003") && error.legacy_text().contains("INVALID_INSTRUMENT_NAME"))));
}

#[test]
fn cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch() {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    use safe_cex_engine::{Action, Notice};
    for mixed in [false, true] {
        let mut h = Harness::new();
        h.load();
        h.engine.store_mut().set_orders(vec![Order {
            id: "123".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSD-PERP".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 90.0,
            amount: 1.0,
            filled: 0.0,
            remaining: 1.0,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }]);
        h.engine.drain_actions().for_each(drop);
        h.engine.handle(
            2000.0,
            Event::Command(Command::CancelOrders {
                command: 991,
                ids: if mixed {
                    vec!["123".into(), "456".into()]
                } else {
                    vec!["456".into()]
                },
            }),
        );
        h.collect();
        let actions = std::mem::take(&mut h.actions);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 991, error })
                    if error.legacy_text().contains("456") && error.legacy_text().contains("not found")
            )),
            "missing cancellation error: {actions:?}"
        );
        assert_eq!(
            h.engine.store().order("123").unwrap().status,
            OrderStatus::Open
        );
    }
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let unknown = (None, Some(Outcome::Unknown));
    for (status, body, classified) in [
        (
            200,
            "{}",
            (Some(ErrorKind::Internal), Some(Outcome::Unknown)),
        ),
        (
            503,
            "busy",
            (Some(ErrorKind::VenueUnavailable), Some(Outcome::Unknown)),
        ),
        (
            400,
            "bad",
            (Some(ErrorKind::Rejected), Some(Outcome::Rejected)),
        ),
        (
            200,
            r#"{"code":306,"message":"INSUFFICIENT_AVAILABLE_BALANCE"}"#,
            (Some(ErrorKind::Rejected), Some(Outcome::Rejected)),
        ),
        (
            200,
            r#"{"code":42901,"message":"TOO_MANY_REQUESTS"}"#,
            (Some(ErrorKind::RateLimited), Some(Outcome::Unknown)),
        ),
        (200, r#"{"code":50001,"message":"internal"}"#, unknown),
        (200, r#"{"code":43012,"message":"self trade"}"#, unknown),
    ] {
        let mut h = Harness::new();
        h.load();
        command(
            &mut h,
            Command::CancelSymbolOrders {
                command: 1,
                symbol: "BTCUSD-PERP".into(),
            },
        );
        let r = h.http("cancel-all-orders");
        h.event(Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status,
                headers: "",
                body,
            },
        });
        pace(&mut h);
        let [error] = std::mem::take(&mut h.failures).try_into().unwrap();
        assert_eq!(
            (error.kind, error.outcome),
            classified,
            "{status} {body}: {}",
            error.message
        );
        assert!(h.actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 1, error: text })
                if text.legacy_text() == error.legacy_text()
        )));
    }
    // Transport loss on a write is Unknown; preflight refusals were never sent.
    let mut h = Harness::new();
    h.load();
    command(
        &mut h,
        Command::CancelSymbolOrders {
            command: 1,
            symbol: "BTCUSD-PERP".into(),
        },
    );
    let r = h.http("cancel-all-orders");
    h.event(Event::HttpFailed {
        request: r.id,
        error: "reset",
    });
    pace(&mut h);
    assert_eq!(
        (h.failures[0].kind, h.failures[0].outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
    command(
        &mut h,
        Command::SetMarginType {
            command: 2,
            symbol: "BTCUSD-PERP".into(),
            isolated: true,
        },
    );
    assert_eq!(
        (h.failures[1].kind, h.failures[1].outcome),
        (Some(ErrorKind::Unsupported), Some(Outcome::NotSent))
    );
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: cryptocom | serial-placement-batch
// TS packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts:501-511,630-636.
fn parity_batch_sends_off_step_below_minimum_tail_verbatim() {
    let mut h = Harness::new();
    h.load();
    let mut first = order();
    first.amount = 0.002;
    first.price = Some(90.0);
    let mut tail = order();
    tail.client_order_id = Some("tail".into());
    tail.amount = 0.0001;
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1104,
            orders: vec![first, tail],
        },
    );
    for (qty, price, id) in [("0.002", "90", "1"), ("0.0001", "90.04", "2")] {
        let write = h.http("private/create-order");
        let body: serde_json::Value = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["params"]["quantity"], qty);
        assert_eq!(body["params"]["price"], price);
        h.respond(
            &write,
            &format!(r#"{{"code":0,"result":{{"order_id":"{id}"}}}}"#),
        );
        pace(&mut h);
    }
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1104, .. })))
    );
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/cryptocom/cryptocom.exchange.ts:630-636
fn unknown_middle_native_error_or_malformed_ack_stops_after_accepted_prefix() {
    for body in [
        r#"{"code":50001,"message":"internal"}"#,
        r#"{"code":0,"result":{}}"#,
    ] {
        let mut h = Harness::new();
        h.load();
        let mut middle = order();
        middle.client_order_id = Some("middle".into());
        let mut last = order();
        last.client_order_id = Some("last".into());
        command(
            &mut h,
            Command::PlaceOrders {
                command: 1106,
                orders: vec![order(), middle, last],
            },
        );
        let first = h.http("private/create-order");
        h.respond(&first, r#"{"code":0,"result":{"order_id":"101"}}"#);
        pace(&mut h);
        let middle = h.http("private/create-order");
        h.respond(&middle, body);
        pace(&mut h);
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("private/create-order")))
        );
        assert_unknown(&h, 1106, &["101"]);
        assert_unknown_inventory(
            &h.actions,
            1106,
            &[
                ("accepted", "orderId", "101"),
                ("currentRequest", "clientOrderId", "middle"),
                ("withheld", "clientOrderId", "last"),
            ],
        );
    }
}
