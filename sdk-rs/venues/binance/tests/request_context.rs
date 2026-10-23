//! Native facts enter the shared journal; late replies have no reducer authority.
mod support;
use safe_cex_engine::{Command, Event, Outcome};
use support::{Harness, Options};

fn ready() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.load_public();
    h
}

fn read(command: u32, identity: bool) -> Command {
    if identity {
        Command::FetchAccount { command }
    } else {
        Command::FetchCandles {
            command,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(1),
            from_ms: Some(0.),
            to_ms: Some(1_000.),
        }
    }
}

#[test]
fn command_reads_park_unsigned_until_the_shared_route_is_prepared() {
    use safe_cex_engine::Action;
    use safe_cex_engine::request_context::HttpAction;
    for identity in [false, true] {
        let mut h = ready();
        h.engine.handle(h.now, Event::Command(read(710, identity)));
        let actions: Vec<_> = h.engine.drain_actions().collect();
        let prepares: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::ManagedHttp(action) => match action.as_ref() {
                    HttpAction::Prepare { request, .. } => Some(request),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(prepares.len(), 1, "one explicit read awaits preparation");
        assert!(prepares[0].headers.is_empty());
        assert!(!prepares[0].url.contains("signature="));
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_) | Action::Sign { .. }))
        );
    }
}

#[test]
fn command_read_deadline_rejects_late_results_and_old_ids_cannot_complete_reused_command() {
    for identity in [false, true] {
        let mut h = ready();
        let path = if identity {
            "/fapi/v3/balance"
        } else {
            "/klines"
        };
        let body = if identity {
            r#"[{"accountAlias":"account"}]"#
        } else {
            "[]"
        };
        h.event(Event::Command(read(711, identity)));
        let first = h.take(path);
        h.now += 20_000.;
        h.reply(first.id, 200, "", body);
        assert!(
            h.result(711).unwrap().is_err(),
            "deadline cannot be renewed by a reply"
        );
        assert_eq!(
            h.done.iter().filter(|(command, _)| *command == 711).count(),
            1
        );
        h.done.clear();
        h.event(Event::Command(read(711, identity)));
        let second = h.take(path);
        h.reply(first.id, 200, "", body);
        assert!(h.result(711).is_none());
        h.reply(second.id, 200, "", body);
        assert!(h.result(711).unwrap().is_ok());
        assert_eq!(
            h.done.iter().filter(|(command, _)| *command == 711).count(),
            1
        );
    }
}

#[test]
fn streamed_candles_discard_partial_rows_when_the_original_deadline_expires() {
    let mut h = ready();
    h.event(Event::Command(read(712, false)));
    let request = h.take("/klines");
    h.event(Event::HttpStart {
        request: request.id,
        status: 200,
        headers: "",
    });
    h.event(Event::HttpChunk {
        request: request.id,
        data: br#"[[1000,"1","3","1","2","8",2000,"16",1,"0","0","0"],"#,
    });
    assert!(h.result(712).is_none());
    h.now += 20_000.;
    h.event(Event::HttpChunk {
        request: request.id,
        data: b"]",
    });
    h.event(Event::HttpEnd {
        request: request.id,
    });
    assert!(h.result(712).unwrap().is_err());
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 712).count(), 1);
}

#[test]
fn command_read_preparation_failures_keep_typed_errors_and_emit_no_http() {
    use safe_cex_engine::request_context::{Error, HttpAction};
    use safe_cex_engine::{Action, ErrorKind, Notice, VenueError};
    for identity in [false, true] {
        let mut h = ready();
        h.engine.handle(h.now, Event::Command(read(713, identity)));
        let token = h
            .engine
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        h.engine.http_preparation_failed(
            h.now + 1.,
            token,
            Error::State,
            VenueError::new("route denied").kind(ErrorKind::AuthPermission),
        );
        let actions: Vec<_> = h.engine.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
        let failures: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed {
                    command: 713,
                    error,
                }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].kind, Some(ErrorKind::AuthPermission));
        assert_eq!(failures[0].message, "route denied");
    }
}

#[test]
fn identity_waits_for_provider_and_refusal_remains_typed_without_a_physical_send() {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    use safe_cex_engine::signing::{SignError, SignFuture, SignRequest, Signer};
    use safe_cex_engine::{Action, ErrorKind, Notice};
    struct Parked;
    impl Signer for Parked {
        fn sign(&self, _: SignRequest) -> SignFuture {
            Box::pin(std::future::pending())
        }
    }
    let mut engine = support::engine(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        ..Options::default()
    });
    engine
        .set_signer(Some(std::sync::Arc::new(Parked)))
        .unwrap();
    engine.handle(1_000., Event::Command(read(714, true)));
    let token = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.http_prepared(
        1_100.,
        token,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let attempt = actions
        .iter()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } => {
                assert_eq!(work.receipt.stamp_ms, Some(1_100.));
                Some(*attempt)
            }
            _ => None,
        })
        .unwrap();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    engine.handle(
        1_200.,
        Event::Signed {
            attempt,
            result: Err(SignError::Refused),
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandFailed {command:714,error}) if error.kind == Some(ErrorKind::InvalidInput))).count(), 1);
    engine.handle(
        1_201.,
        Event::Signed {
            attempt,
            result: Err(SignError::Refused),
        },
    );
    assert!(engine.drain_actions().next().is_none());
}

#[test]
fn terminal_native_reports_follow_logical_plan_not_callback_order() {
    let mut h = ready();
    h.event(Event::Command(Command::CancelSymbolOrders {
        command: 701,
        symbol: "BTCUSDT".into(),
    }));
    let first = h.take("/fapi/v1/allOpenOrders");
    let second = h.take("/fapi/v1/algoOpenOrders");
    h.reply(
        second.id,
        400,
        "",
        r#"{"code":-1100,"msg":"second native refusal"}"#,
    );
    assert!(h.result(701).is_none());
    h.reply(
        first.id,
        400,
        "",
        r#"{"code":-1102,"msg":"first native refusal"}"#,
    );
    let failure = &h.failures.iter().find(|(id, _)| *id == 701).unwrap().1;
    assert_eq!(failure.outcome, Some(Outcome::Rejected));
    assert!(failure.message.contains("first native refusal"));
    let reports: Vec<_> = h
        .command_errors
        .iter()
        .filter(|(id, _)| *id == 701)
        .map(|(_, text)| text.as_str())
        .collect();
    assert_eq!(
        reports,
        vec!["first native refusal", "second native refusal"]
    );
}

#[test]
fn empty_job_completes_without_declaring_a_write_or_emitting_http() {
    let mut h = ready();
    let before = h.requests.len();
    h.event(Event::Command(Command::PlaceOrders {
        command: 702,
        orders: vec![],
    }));
    assert_eq!(h.result(702), Some(&Ok("[]".into())));
    assert_eq!(h.requests.len(), before);
    assert!(!h.failures.iter().any(|(id, _)| *id == 702));
}

fn put_order(h: &mut Harness) {
    use safe_cex_engine::model::*;
    h.engine.store_mut().upsert_order(Order {
        id: "12".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 100.,
        amount: 1.,
        filled: 0.,
        remaining: 1.,
        reduce_only: false,
        close: None,
        client_order_id: Some("client12".into()),
        time_in_force: None,
        last_update: 1000.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    });
}

#[test]
fn gateway_codes_never_gain_cancel_or_position_mode_success_authority() {
    for cancel in [true, false] {
        let mut h = ready();
        put_order(&mut h);
        let (command, path, body) = if cancel {
            (
                Command::CancelOrders {
                    command: 703,
                    ids: vec!["12".into()],
                },
                "/fapi/v1/order",
                r#"{"code":-2011,"msg":"unknown order"}"#,
            )
        } else {
            (
                Command::SetPositionMode {
                    command: 703,
                    hedged: true,
                    symbol: None,
                },
                "/fapi/v1/positionSide/dual",
                r#"{"code":-4059,"msg":"already set"}"#,
            )
        };
        h.requests.clear(); // Exclude the unrelated startup GET for position mode.
        h.event(Event::Command(command));
        let write = h.take(path);
        h.reply(write.id, 500, "", body);
        assert_eq!(
            h.failures
                .iter()
                .find(|(id, _)| *id == 703)
                .unwrap()
                .1
                .outcome,
            Some(Outcome::Unknown)
        );
        assert_eq!(
            h.engine.store().order("12").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
        assert!(!h.engine.store().hedged());
    }
}

#[test]
fn duplicate_command_event_cannot_replace_a_live_managed_owner() {
    for nonempty in [false, true] {
        let mut h = ready();
        h.event(Event::Command(Command::CancelSymbolOrders {
            command: 704,
            symbol: "BTCUSDT".into(),
        }));
        let first = h.take("/fapi/v1/allOpenOrders");
        let second = h.take("/fapi/v1/algoOpenOrders");
        let order = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
        )
        .unwrap();
        h.event(Event::Command(Command::PlaceOrders {
            command: 704,
            orders: if nonempty { vec![order] } else { vec![] },
        }));
        assert!(
            h.result(704).is_none(),
            "duplicate must not seal the original"
        );
        assert!(!h.has_request("/fapi/v1/order"));
        h.reply(second.id, 200, "", r#"{"code":200}"#);
        h.reply(first.id, 200, "", r#"{"code":200}"#);
        assert_eq!(h.result(704), Some(&Ok("[]".into())));
        assert!(!h.failures.iter().any(|(id, _)| *id == 704));
        h.event(Event::Command(Command::PlaceOrders {
            command: 704,
            orders: vec![],
        }));
        let support::Harness { done, .. } = &h;
        assert_eq!(
            done.iter().filter(|(id, _)| *id == 704).count(),
            2,
            "completed numeric IDs may get a fresh authority"
        );
        assert!(!h.failures.iter().any(|(id, _)| *id == 704));
    }
}

#[test]
fn unproven_first_batch_stops_later_lots_but_item_refusals_do_not() {
    for transport in [true, false] {
        let mut h = ready();
        let order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
        )
        .unwrap();
        h.event(Event::Command(Command::PlaceOrders {
            command: 705,
            orders: vec![order; 6],
        }));
        let batch = h.take("/fapi/v1/batchOrders");
        if transport {
            h.event(Event::HttpFailed {
                request: batch.id,
                error: "connection lost",
            });
        } else {
            h.reply(
                batch.id,
                500,
                "",
                r#"{"code":-2015,"msg":"authorization failed"}"#,
            );
        }
        assert!(!h.has_request("/fapi/v1/order"));
        assert_eq!(
            h.failures
                .iter()
                .find(|(id, _)| *id == 705)
                .unwrap()
                .1
                .outcome,
            Some(Outcome::Unknown)
        );
    }
}

#[test]
fn expired_native_write_retires_unknown_without_reducing_a_late_ack() {
    let mut h = ready();
    put_order(&mut h);
    let before = h.engine.store().order("12").unwrap().clone();
    h.event(Event::Command(Command::CancelOrders {
        command: 706,
        ids: vec!["12".into()],
    }));
    let write = h.take("/fapi/v1/order");
    h.advance(20_001.);
    let failures: Vec<_> = h.failures.iter().filter(|(id, _)| *id == 706).collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].1.outcome, Some(Outcome::Unknown));
    h.reply(
        write.id,
        200,
        "",
        r#"{"symbol":"BTCUSDT","orderId":12,"status":"CANCELED","origQty":"1","executedQty":"0"}"#,
    );
    assert_eq!(h.engine.store().order("12"), Some(&before));
    assert_eq!(h.failures.iter().filter(|(id, _)| *id == 706).count(), 1);
}

#[test]
fn mismatched_batch_rows_keep_only_reconciliation_ids_without_claiming_acceptance() {
    for (body, expected) in [
        (r#"[{"orderId":1},{"orderId":2}]"#, vec!["1", "2"]),
        (
            r#"[{"orderId":1},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5},{"orderId":6}]"#,
            vec!["1", "2", "3", "4", "5", "6"],
        ),
        (
            r#"[{"orderId":11},{"orderId":"bad"},{"orderId":12},{"orderId":13,"code":-2010}]"#,
            vec!["11", "12"],
        ),
        (
            r#"[{"orderId":0},{"orderId":-1},{"orderId":14,"code":0},{"orderId":18446744073709551616}]"#,
            vec![],
        ),
        ("[]", vec![]),
    ] {
        let mut h = ready();
        put_order(&mut h);
        let before = h.engine.store().orders().to_vec();
        let order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
        )
        .unwrap();
        h.event(Event::Command(Command::PlaceOrders {
            command: 707,
            orders: vec![order; 6],
        }));
        let batch = h.take("/fapi/v1/batchOrders");
        h.reply(batch.id, 200, "", body);
        let error = &h.failures.iter().find(|(id, _)| *id == 707).unwrap().1;
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        let ids = error
            .partial
            .as_ref()
            .map_or(&[][..], |partial| partial.order_ids.as_slice());
        assert_eq!(
            ids, expected,
            "native IDs are evidence, not caller-leg attribution: {body}"
        );
        if let Some(partial) = &error.partial {
            assert_eq!((partial.total, partial.failed_index), (None, None));
        }
        assert_eq!(h.engine.store().orders(), before);
        assert!(h.result(707).unwrap().is_err());
        assert!(!h.has_request("/fapi/v1/order"));
    }
}

#[test]
fn background_catalogue_waits_for_shared_route_preparation() {
    use safe_cex_engine::Action;
    use safe_cex_engine::request_context::HttpAction;
    let mut engine = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    engine.start(1_000.);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|a| matches!(a, Action::ManagedHttp(action)
        if matches!(action.as_ref(), HttpAction::Prepare { request, .. } if request.url.contains("exchangeInfo")))));
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn background_catalogue_reply_at_its_deadline_cannot_publish_markets() {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Options::default()
    });
    let request = h.take("exchangeInfo");
    h.now += 20_000.;
    h.reply(request.id, 200, "", support::EXCHANGE_INFO);
    assert!(!h.engine.store().loaded().markets);
    assert!(h.engine.store().markets().is_empty());
    assert!(!h.has_request("24hr"));
}

#[test]
fn background_account_reply_at_its_deadline_cannot_publish_balance() {
    let mut h = ready();
    let request = h.take("/fapi/v3/account");
    let before = h.engine.store().balance().clone();
    h.now += 20_000.;
    h.reply(request.id, 200, "", r#"{"totalInitialMargin":"48","totalWalletBalance":"1000","availableBalance":"952","assets":[{"asset":"USDT","walletBalance":"1000","unrealizedProfit":"10","marginBalance":"1010","maintMargin":"3.8","initialMargin":"48","availableBalance":"952"}],"positions":[]}"#);
    assert_eq!(h.engine.store().balance(), &before);
    assert!(!h.engine.store().loaded().balance);
}

#[test]
fn background_preparation_refusal_releases_account_joins_for_fresh_polling() {
    use safe_cex_engine::request_context::{Error, HttpAction};
    use safe_cex_engine::{Action, ErrorKind, Response, VenueError};
    let mut h = Harness::started(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        ..Options::default()
    });
    let catalogue = h.take("exchangeInfo");
    h.engine.handle(
        h.now,
        Event::HttpResponse {
            request: catalogue.id,
            response: Response {
                status: 200,
                headers: "",
                body: support::EXCHANGE_INFO,
            },
        },
    );
    let actions: Vec<_> = h.engine.drain_actions().collect();
    let mut account = 0;
    // Preserve the native account-poll timer while this control intercepts
    // prepared routes rather than using Harness.collect for the first batch.
    h.timers
        .extend(actions.iter().filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        }));
    for action in &actions {
        if let Action::ManagedHttp(action) = action
            && let HttpAction::Prepare {
                preparation,
                request,
                ..
            } = action.as_ref()
        {
            if request.url.contains("/fapi/v3/account") {
                account += 1;
            }
            assert!(request.headers.is_empty());
            assert!(!request.url.contains("signature="));
            h.engine.http_preparation_failed(
                h.now,
                *preparation,
                Error::State,
                VenueError::new("route refused").kind(ErrorKind::AuthPermission),
            );
        }
    }
    assert_eq!(account, 1);
    assert!(
        !actions.iter().any(
            |a| matches!(a, Action::Http(request) if request.url.contains("/fapi/v3/account"))
        )
    );
    h.collect();
    assert!(h.done.is_empty());
    h.advance(76_000.);
    assert!(
        h.has_request("/fapi/v3/account"),
        "preparation refusal must clear account_in_flight"
    );
    assert!(
        h.has_request("/fapi/v3/positionRisk"),
        "preparation refusal must clear the positions join"
    );
    assert!(
        h.has_request("/fapi/v1/openOrders"),
        "preparation refusal must clear the orders join"
    );
}

#[test]
fn mixed_native_cancel_batch_with_proven_effects_is_known_partial_not_rejected() {
    use safe_cex_engine::model::OrderStatus;
    let mut h = ready();
    put_order(&mut h);
    let mut second = h.engine.store().order("12").unwrap().clone();
    second.id = "13".into();
    second.client_order_id = Some("client13".into());
    h.engine.store_mut().upsert_order(second);
    h.event(Event::Command(Command::CancelOrders {
        command: 715,
        ids: vec!["12".into(), "13".into()],
    }));
    let request = h.take("/fapi/v1/batchOrders");
    h.reply(request.id, 200, "", r#"[{"symbol":"BTCUSDT","orderId":12,"status":"CANCELED","origQty":"1","executedQty":"0"},{"code":-2011,"msg":"unknown order"}]"#);
    assert_eq!(
        h.engine.store().order("12").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(
        h.engine.store().order("13").unwrap().status,
        OrderStatus::Open
    );
    let error = &h.failures.iter().find(|(id, _)| *id == 715).unwrap().1;
    assert_eq!(
        error.outcome, None,
        "proven cancellation effects forbid whole-command rejection"
    );
}

#[test]
fn a_native_cancel_refusal_cannot_hide_uncertainty_in_a_later_batch_row() {
    use safe_cex_engine::model::OrderStatus;
    let accepted =
        r#"{"symbol":"BTCUSDT","orderId":12,"status":"CANCELED","origQty":"1","executedQty":"0"}"#;
    let refused = r#"{"code":-2011,"msg":"unknown order"}"#;
    for uncertain in [
        r#"{"code":-1007,"msg":"native timeout"}"#,
        r#"{"symbol":"BTCUSDT","orderId":999,"status":"CANCELED","origQty":"1","executedQty":"0"}"#,
        accepted,
    ] {
        for reverse in [false, true] {
            let mut h = ready();
            put_order(&mut h);
            for id in ["13", "14"] {
                let mut order = h.engine.store().order("12").unwrap().clone();
                order.id = id.into();
                order.client_order_id = Some(format!("client{id}"));
                h.engine.store_mut().upsert_order(order);
            }
            h.event(Event::Command(Command::CancelOrders {
                command: 716,
                ids: vec!["12".into(), "13".into(), "14".into()],
            }));
            let request = h.take("/fapi/v1/batchOrders");
            let (second, third) = if reverse {
                (uncertain, refused)
            } else {
                (refused, uncertain)
            };
            h.reply(
                request.id,
                200,
                "",
                &format!("[{accepted},{second},{third}]"),
            );
            assert_eq!(
                h.engine.store().order("12").unwrap().status,
                OrderStatus::Canceled
            );
            for id in ["13", "14"] {
                assert_eq!(
                    h.engine.store().order(id).unwrap().status,
                    OrderStatus::Open
                );
            }
            let error = &h.failures.iter().find(|(id, _)| *id == 716).unwrap().1;
            assert_eq!(
                error.outcome,
                Some(Outcome::Unknown),
                "a definitive refusal cannot erase an unresolved sibling"
            );
        }
    }
}

#[test]
fn unproven_placement_ids_are_evidence_only_and_match_the_native_endpoint() {
    let mut observations = Vec::new();
    for (algo, body, expected) in [
        (false, r#"{"orderId":12}"#, vec!["12"]),
        (false, "[null,null,12,null,null,null,null]", vec![]),
        (false, r#"{"algoId":13}"#, vec![]),
        (true, r#"{"algoId":13}"#, vec!["13"]),
        (true, r#"{"orderId":12}"#, vec![]),
        (false, r#"{"orderId":0}"#, vec![]),
        (false, r#"{"orderId":-1}"#, vec![]),
        (false, r#"{"orderId":1.5}"#, vec![]),
        (false, r#"{"orderId":18446744073709551616}"#, vec![]),
        (false, r#"{"orderId":"12"}"#, vec![]),
        (false, r#"{"orderId":12,"orderId":13}"#, vec![]),
        (
            false,
            r#"{"orderId":12,"code":-2015,"msg":"auth refused"}"#,
            vec![],
        ),
    ] {
        let mut h = ready();
        put_order(&mut h);
        let before = h.engine.store().orders().to_vec();
        let kind = if algo { "stop_market" } else { "limit" };
        let order = serde_json::from_str(&format!(
            r#"{{"symbol":"BTCUSDT","type":"{kind}","side":"buy","amount":1,"price":100}}"#,
        ))
        .unwrap();
        h.event(Event::Command(Command::PlaceOrders {
            command: 720,
            orders: vec![order],
        }));
        let request = h.take(if algo {
            "/fapi/v1/algoOrder"
        } else {
            "/fapi/v1/order"
        });
        h.reply(request.id, 503, "", body);
        let failure = &h.failures.iter().find(|(id, _)| *id == 720).unwrap().1;
        assert_eq!(failure.outcome, Some(Outcome::Unknown));
        assert_eq!(
            failure.kind,
            Some(safe_cex_engine::ErrorKind::VenueUnavailable)
        );
        assert_eq!(h.engine.store().orders(), before);
        let ids = failure
            .partial
            .as_ref()
            .map(|p| p.order_ids.clone())
            .unwrap_or_default();
        observations.push((algo, body, ids, expected));
    }
    for (algo, body, ids, expected) in observations {
        assert_eq!(ids, expected, "algo={algo} body={body}");
    }
}

#[test]
fn unproven_batch_ids_validate_the_whole_envelope_without_caller_attribution() {
    let mut observations = Vec::new();
    for (body, expected) in [
        (r#"[{"orderId":12},{"orderId":13}]"#, vec!["12", "13"]),
        ("[[null,null,12,null,null,null,null]]", vec![]),
        (
            r#"[{"orderId":12},{"code":-2015,"msg":"auth refused"}]"#,
            vec!["12"],
        ),
        (r#"[{"orderId":12},{"orderId":13}] trailing"#, vec![]),
        (r#"[{"orderId":12},{"orderId":}]"#, vec![]),
        (
            r#"[{"orderId":0},{"orderId":18446744073709551615}]"#,
            vec!["18446744073709551615"],
        ),
        (
            r#"[{"orderId":12,"code":-2015},{"orderId":13}]"#,
            vec!["13"],
        ),
    ] {
        let mut h = ready();
        let order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
        )
        .unwrap();
        h.event(Event::Command(Command::PlaceOrders {
            command: 721,
            orders: vec![order; 6],
        }));
        let request = h.take("/fapi/v1/batchOrders");
        h.reply(request.id, 503, "", body);
        let failure = &h.failures.iter().find(|(id, _)| *id == 721).unwrap().1;
        assert_eq!(failure.outcome, Some(Outcome::Unknown));
        assert!(!h.has_request("/fapi/v1/order"));
        assert!(h.engine.store().orders().is_empty());
        if let Some(partial) = failure.partial.as_ref() {
            assert_eq!(partial.total, None);
            assert_eq!(partial.failed_index, None);
        }
        let ids = failure
            .partial
            .as_ref()
            .map(|p| p.order_ids.clone())
            .unwrap_or_default();
        observations.push((body, ids, expected));
    }
    for (body, ids, expected) in observations {
        assert_eq!(ids, expected, "body={body}");
    }
}

#[test]
fn unproven_cancellation_and_settings_do_not_invent_placement_ids() {
    for cancel in [false, true] {
        let mut h = ready();
        put_order(&mut h);
        let before = h.engine.store().orders().to_vec();
        h.requests.clear();
        h.event(Event::Command(if cancel {
            Command::CancelOrders {
                command: 723,
                ids: vec!["12".into()],
            }
        } else {
            Command::SetPositionMode {
                command: 723,
                hedged: true,
                symbol: None,
            }
        }));
        let request = h.take(if cancel {
            "/fapi/v1/order"
        } else {
            "/fapi/v1/positionSide/dual"
        });
        h.reply(
            request.id,
            503,
            "",
            r#"{"orderId":12,"algoId":13,"clientOrderId":"client12"}"#,
        );
        let failure = &h.failures.iter().find(|(id, _)| *id == 723).unwrap().1;
        assert_eq!(failure.outcome, Some(Outcome::Unknown));
        assert!(
            failure
                .partial
                .as_ref()
                .is_none_or(|partial| partial.order_ids.is_empty())
        );
        assert_eq!(h.engine.store().orders(), before);
        assert!(!h.engine.store().hedged());
    }
}

#[test]
fn unproven_batch_error_object_keeps_bounded_native_diagnostics_without_ids() {
    let mut h = ready();
    let order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
    )
    .unwrap();
    h.event(Event::Command(Command::PlaceOrders {
        command: 724,
        orders: vec![order; 6],
    }));
    let request = h.take("/fapi/v1/batchOrders");
    h.reply(
        request.id,
        503,
        "",
        r#"{"code":-1007,"msg":"Timeout waiting for response","orderId":12}"#,
    );
    let failure = &h.failures.iter().find(|(id, _)| *id == 724).unwrap().1;
    assert_eq!(failure.outcome, Some(Outcome::Unknown));
    assert_eq!(
        failure.kind,
        Some(safe_cex_engine::ErrorKind::VenueUnavailable)
    );
    assert_eq!(failure.venue_code.as_deref(), Some("-1007"));
    assert_eq!(
        failure.venue_message.as_deref(),
        Some("Timeout waiting for response")
    );
    assert!(
        failure
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
    assert!(h.engine.store().orders().is_empty());
    assert!(!h.has_request("/fapi/v1/order"));
}

#[test]
fn documented_rate_limit_statuses_without_a_code_are_typed_refusals() {
    use safe_cex_engine::ErrorKind;
    for (status, headers, outcome, kind, retry) in [
        (
            429,
            "Retry-After: 3\r\n",
            Some(Outcome::Rejected),
            Some(ErrorKind::RateLimited),
            Some(3_000.),
        ),
        (
            418,
            "",
            Some(Outcome::Rejected),
            Some(ErrorKind::RateLimited),
            None,
        ),
        (
            403,
            "",
            Some(Outcome::Rejected),
            Some(ErrorKind::Rejected),
            None,
        ),
        // 409 can mean a partially successful cancel-replace: never a refusal.
        (409, "", Some(Outcome::Unknown), None, None),
    ] {
        let mut h = ready();
        let order = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
        )
        .unwrap();
        h.event(Event::Command(Command::PlaceOrders {
            command: 730,
            orders: vec![order],
        }));
        let request = h.take("/fapi/v1/order");
        h.reply(
            request.id,
            status,
            headers,
            "<html>Too Many Requests</html>",
        );
        let failure = &h.failures.iter().find(|(id, _)| *id == 730).unwrap().1;
        assert_eq!(failure.outcome, outcome, "{status}");
        assert_eq!(failure.kind, kind, "{status}");
        assert_eq!(failure.retry_after_ms, retry, "{status}");
        assert!(failure.message.contains("HTTP"), "{status}");
    }
}
