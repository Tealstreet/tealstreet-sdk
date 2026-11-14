//! Signed request/acknowledgement behavior, independent of a live account.
use account_support::signer;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome, Response};
use safe_cex_venue_txflow::{Options, TxFlow};
use serde_json::{Value, json};
#[path = "support/account.rs"]
mod account_support;
use account_support::{Plain, failures};
const NOW: f64 = 1_800_000_000_000.0;
fn engine() -> Engine<TxFlow> {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        application_id: Some("0x0000000000000000000000000000000000000001".into()),
        credentials: signer::identity(),
        ..Default::default()
    }));
    signer::start(&mut e, NOW);
    let request = e
        .drain_plain()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    let body = json!({"success":true,"code":0,"data":{"universe":[{"index":1,"name":"BTC-USDC","baseCurrency":"BTC","quoteCurrency":"USDC","currencyType":"usdc","delisted":false,"haltTrading":false,"basePrecision":0.001,"priceTick":0.1,"maxLeverage":50,"maxLimitOrderSize":100,"maxMarketOrderSize":50}]}}).to_string();
    e.handle(
        NOW,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    let mut startup = signer::prepared_actions(&mut e, NOW);
    let ticker = startup
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if account_support::kind(request) == "activeAssetCtx" => {
                Some(request.clone())
            }
            _ => None,
        })
        .unwrap();
    ack(
        &mut e,
        &ticker,
        200,
        r#"{"success":true,"code":0,"data":{"instrumentId":1}}"#,
    );
    let timer = e
        .drain_plain()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == NOW + 25.0).then_some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(NOW + 25.0, Event::TimerFired { timer });
    startup.extend(signer::prepared_actions(&mut e, NOW + 25.0));
    let initial = startup;
    let node = initial
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/ws") => Some(*socket),
            _ => None,
        })
        .unwrap();
    let reads = initial
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    e.handle(NOW + 25.0, Event::WsOpened { socket: node });
    for r in reads {
        e.handle(
            NOW + 25.0,
            Event::HttpFailed {
                request: r.id,
                error: "fixture initial read failed",
            },
        );
    }
    e.drain_plain().for_each(drop);
    e
}
fn opts(value: Value) -> PlaceOrderOpts {
    serde_json::from_value(value).unwrap()
}
fn limit(id: &str) -> PlaceOrderOpts {
    opts(
        json!({"symbol":"BTC-USDC","type":"limit","side":"buy","amount":0.01,"price":50000,"clientOrderId":id}),
    )
}
fn place(e: &mut Engine<TxFlow>, command: u32, orders: Vec<PlaceOrderOpts>) {
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders { command, orders }),
    );
}
fn write(e: &mut Engine<TxFlow>) -> HttpRequest {
    e.drain_plain()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .expect("signed write")
}
fn ack(e: &mut Engine<TxFlow>, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        NOW,
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
#[test]
fn every_order_without_a_caller_id_gets_a_distinct_native_cloid() {
    let mut e = engine();
    place(
        &mut e,
        6,
        vec![
            opts(
                json!({"symbol":"BTC-USDC","type":"limit","side":"buy","amount":0.01,"price":50000}),
            ),
            opts(json!({"symbol":"BTC-USDC","type":"market","side":"sell","amount":0.02})),
        ],
    );
    let request = write(&mut e);
    let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    let orders = body["action"]["orders"].as_array().unwrap();
    let ids: Vec<_> = orders
        .iter()
        .map(|order| order["c"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    assert!(
        ids.iter()
            .all(|id| !id.is_empty() && id.len() <= 64 && !id.starts_with("0x"))
    );
}

#[test]
fn batch_receipts_match_all_caller_and_generated_ids_before_signed_http() {
    let mut e = engine();
    let mut generated = limit("unused");
    generated.client_order_id = None;
    place(&mut e, 91, vec![limit("caller-receipt"), generated]);
    let actions = e.drain_plain().collect::<Vec<_>>();
    let (write_at, request) = actions
        .iter()
        .enumerate()
        .find_map(|(i, action)| match action {
            Action::Http(request) if request.url.ends_with("/exchange") => Some((i, request)),
            _ => None,
        })
        .expect("signed placement");
    let (progress_at, receipts) = actions
        .iter()
        .enumerate()
        .find_map(|(i, action)| match action {
            Action::Notice(Notice::CommandProgress {
                command: 91,
                receipts,
            }) => Some((i, receipts)),
            _ => None,
        })
        .expect("batch identities");
    assert!(progress_at < write_at);
    #[derive(serde::Deserialize)]
    struct Envelope {
        action: OrderAction,
    }
    #[derive(serde::Deserialize)]
    struct OrderAction {
        orders: Vec<NativeOrder>,
    }
    #[derive(serde::Deserialize)]
    struct NativeOrder {
        c: String,
    }
    let body: Envelope = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body.action.orders[0].c, "caller-receipt");
    assert!(body.action.orders[1].c.starts_with("ts_"));
    assert_eq!(
        receipts,
        &body
            .action
            .orders
            .into_iter()
            .map(|order| OrderIdentityReceipt {
                symbol: "BTC-USDC".into(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: order.c,
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn complete_batch_preflight_rejects_invalid_last_shape_without_writes() {
    let mut e = engine();
    place(
        &mut e,
        7,
        vec![
            limit("good"),
            opts(
                json!({"symbol":"BTC-USDC","type":"stop_market","side":"sell","amount":0.01,"price":45000}),
            ),
        ],
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
    );
}
#[test]
fn request_preserves_caller_id_and_confirm_executed_but_ack_never_fabricates_oid() {
    let mut e = engine();
    place(&mut e, 7, vec![limit("caller-Ä")]);
    let r = write(&mut e);
    let body: Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(body["action"]["orders"][0]["c"], "caller-Ä");
    assert_eq!(body["action"]["orders"][0]["a"], 1);
    assert_eq!(body["confirm"], "executed");
    assert!(body["nonce"].as_u64().unwrap() >= NOW as u64);
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| *k == "x-api-version" && v == "2026-08-16")
    );
    ack(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","confirm":"executed","code":0,"data":{"type":"PlaceOrder","data":{"statuses":["success"]}}}"#,
    );
    assert!(!e.drain_plain().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { command: 7, .. } | Notice::CommandFailed { command: 7, .. }
        )
    )));
    assert!(e.store().orders().is_empty());
}
#[test]
fn authoritative_read_resolves_native_ids_in_requested_order() {
    let mut e = engine();
    place(&mut e, 7, vec![limit("first"), limit("second")]);
    let r = write(&mut e);
    ack(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","confirm":"executed","code":0,"data":{"type":"PlaceOrder","data":{"statuses":["success"]}}}"#,
    );
    let read = e
        .drain_plain()
        .find_map(|a| match a {
            Action::Http(r)
                if r.body
                    .as_ref()
                    .is_some_and(|b| b.contains("frontendOpenOrders")) =>
            {
                Some(r)
            }
            _ => None,
        })
        .expect("reconciliation read");
    let order = |oid: u64, cloid: &str| json!({"asset":1,"oid":oid,"side":"B","orderType":"Limit","limitPx":"50000","origSz":"0.01","sz":"0.01","timestamp":NOW,"cloid":cloid,"state":"Open","tif":"GTC","children":null});
    let body = json!({"success":true,"code":0,"data":[order(22,"second"),order(11,"first")],"hasMore":false}).to_string();
    ack(&mut e, &read, 200, &body);
    let observed = e.drain_plain().collect::<Vec<_>>();
    assert!(observed.iter().any(|a| matches!(a,Action::Notice(Notice::CommandDone {command:7,result:ids}) if ids == r#"["11","22"]"#)), "{observed:?}");
}
#[test]
fn pending_execution_never_retries_write_and_releases_command_as_uncertain() {
    let mut e = engine();
    place(&mut e, 7, vec![limit("queued")]);
    let r = write(&mut e);
    ack(
        &mut e,
        &r,
        202,
        r#"{"status":"pending","txHash":"fixture"}"#,
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error })
        if error.message.contains("uncertain") && error.outcome == Some(Outcome::Unknown))
    ));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/exchange")))
    );
}
#[test]
fn unconfirmed_consensus_success_is_not_executed_success() {
    let mut e = engine();
    place(&mut e, 7, vec![limit("consensus-only")]);
    let r = write(&mut e);
    ack(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","code":0,"data":{"type":"PlaceOrder","data":{"statuses":["success"]}}}"#,
    );
    assert!(e.drain_actions().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error })
        if error.message.contains("uncertain") && error.outcome == Some(Outcome::Unknown))
    ));
}
#[test]
fn explicit_rejections_are_rejected_and_lost_responses_are_unknown() {
    let mut e = engine();
    place(&mut e, 7, vec![limit("rejected")]);
    let r = write(&mut e);
    ack(
        &mut e,
        &r,
        200,
        r#"{"status":"err","message":"Insufficient margin"}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.legacy_text(), "Insufficient margin");
    // A top-level `status: "err"` is TxFlow's explicit refusal.
    assert_eq!(error.outcome, Some(Outcome::Rejected));

    let mut e = engine();
    place(&mut e, 8, vec![limit("lost")]);
    let r = write(&mut e);
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "connection reset",
        },
    );
    let [error] = failures(&mut e).try_into().unwrap();
    // The engine selects the physical transport error over the venue fallback.
    // Reconciliation remains required by the Unknown outcome below.
    assert_eq!(error.message, "connection reset");
    assert_eq!(
        (error.kind, error.outcome),
        (
            Some(safe_cex_engine::ErrorKind::Network),
            Some(Outcome::Unknown)
        )
    );
}
#[test]
fn duplicate_id_is_rejected_while_first_placement_is_inflight() {
    let mut e = engine();
    place(&mut e, 7, vec![limit("same-id")]);
    let _r = write(&mut e);
    place(&mut e, 8, vec![limit("same-id")]);
    assert!(e.drain_plain().any(|a| matches!(a,Action::Notice(Notice::CommandFailed { command: 8, error: e }) if e.legacy_text().contains("Duplicate"))));
}
#[test]
fn queued_amend_revalidates_children_before_signing_and_never_clears_new_protection() {
    let mut e = engine();
    e.store_mut().upsert_order(Order {
        id: "99".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTC-USDC".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 50000.0,
        amount: 0.01,
        filled: 0.0,
        remaining: 0.01,
        reduce_only: false,
        close: None,
        client_order_id: Some("original".into()),
        time_in_force: None,
        last_update: NOW,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: Some(OrderExtra {
            take_profit_known: true,
            stop_loss_known: true,
            ..Default::default()
        }),
    });
    place(&mut e, 7, vec![limit("blocking")]);
    let r = write(&mut e);
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "99".into(),
            price: Some(50001.0),
            amount: None,
        }),
    );
    e.store_mut().update_order("99", |o| {
        o.extra.as_mut().unwrap().take_profit_known = false
    });
    ack(
        &mut e,
        &r,
        202,
        r#"{"status":"pending","txHash":"fixture"}"#,
    );
    let mut actions = e.drain_plain().collect::<Vec<_>>();
    if let Some(timer) = actions.iter().find_map(|a| {
        if let Action::SetTimer { timer, at_ms } = a {
            (*at_ms == NOW + 110.0).then_some(*timer)
        } else {
            None
        }
    }) {
        e.handle(NOW + 110.0, Event::TimerFired { timer });
        actions.extend(e.drain_plain());
    }
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/exchange")))
    );
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed { command: 8, error: e }) if e.legacy_text().contains("TP/SL"))));
}
#[test]
fn reserved_prefix_and_utf8_limit_fail_before_write() {
    for id in ["0xreserved".to_owned(), "é".repeat(33)] {
        let mut e = engine();
        place(&mut e, 7, vec![limit(&id)]);
        assert!(
            !e.drain_plain()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/exchange")))
        );
    }
}
#[test]
fn bybit_controls_and_unknown_slippage_are_not_silently_dropped() {
    for opts in [
        json!({"symbol":"BTC-USDC","type":"limit","side":"buy","amount":0.01,"price":50000,"bybitSlippageToleranceTicks":1}),
        json!({"symbol":"BTC-USDC","type":"market","side":"buy","amount":0.01,"maxSlippagePct":4}),
    ] {
        let mut e = engine();
        place(&mut e, 7, vec![self::opts(opts)]);
        assert!(
            !e.drain_plain()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/exchange")))
        );
    }
}

#[test]
fn unimplemented_request_commands_complete_with_errors_instead_of_hanging() {
    let mut e = engine();
    for command in [
        Command::FetchAccount { command: 91 },
        Command::FetchOrderBook {
            command: 91,
            symbol: "BTC-USDC".into(),
            limit: None,
        },
        Command::FetchLeverage {
            command: 91,
            symbol: "BTC-USDC".into(),
        },
        Command::FetchStpMode { command: 91 },
        Command::SetStpMode {
            command: 91,
            mode: StpMode::ExpireMaker,
        },
    ] {
        e.handle(NOW, Event::Command(command));
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 91, error }) if !error.legacy_text().is_empty())).count(), 1);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn fetch_leverage_returns_confirmed_account_rails_in_wrapper_json_shape() {
    let (mut e, node, reads, _) = account_support::bootstrap();
    account_support::answer(&mut e, reads);
    account_support::message(
        &mut e,
        node,
        account_support::NOW + 10.0,
        r#"{"channel":"activeAssetData","data":{"coin":"1","leverage":{"mode":"hedge","type":"isolated","long_value":5,"short_value":8}}}"#,
    );
    e.drain_plain().for_each(drop);
    e.handle(
        account_support::NOW + 20.0,
        Event::Command(Command::FetchLeverage {
            command: 91,
            symbol: "BTC-USDC".into(),
        }),
    );
    let result = e
        .drain_plain()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 91,
                result,
            }) => Some(result),
            _ => None,
        })
        .unwrap();
    let rails: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(
        rails,
        json!({"buyLeverage":5.0,"sellLeverage":8.0,"marginType":"isolated"})
    );
    assert_eq!(e.store().positions().len(), 1);
}

#[test]
fn caller_id_preflight_preserves_shared_shape_duplicate_and_split_errors() {
    let cases = [
        (
            vec![opts(
                json!({"symbol":"BTC-USDC","type":"limit","side":"buy","amount":0.01,"price":50000,"stopLoss":45000,"takeProfit":55000,"clientOrderId":"bracket"}),
            )],
            "TxFlow clientOrderId cannot be honored for this order shape (type=limit +stopLoss +takeProfit); caller client order IDs are supported only on plain Limit/Market orders",
        ),
        (
            vec![limit("duplicate"), limit("duplicate")],
            "TxFlow clientOrderId duplicate is duplicated in one batch",
        ),
        (
            vec![opts(
                json!({"symbol":"BTC-USDC","type":"limit","side":"buy","amount":101,"price":50000,"clientOrderId":"oversized"}),
            )],
            "TxFlow order exceeds the market's max size for one order; a caller clientOrderId cannot span several orders",
        ),
    ];
    for (orders, expected) in cases {
        let mut e = engine();
        place(&mut e, 7, orders);
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(a,Action::Notice(Notice::CommandFailed { command: 7, error }) if error.legacy_text() == expected)), "{actions:?}");
    }
}

#[test]
fn preflight_refusals_are_never_sent() {
    let mut e = engine();
    place(&mut e, 7, vec![limit("same-id")]);
    let _r = write(&mut e);
    place(&mut e, 8, vec![limit("same-id")]);
    let [error] = failures(&mut e).try_into().unwrap();
    assert!(error.message.contains("Duplicate"));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

#[test]
fn a_single_action_error_status_is_a_rejection() {
    let mut e = engine();
    place(&mut e, 7, vec![limit("rejected")]);
    let r = write(&mut e);
    ack(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","confirm":"executed","code":0,"data":{"type":"PlaceOrder","data":{"statuses":[{"error":"Insufficient margin"}]}}}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.legacy_text(), "Insufficient margin");
    assert_eq!(error.outcome, Some(Outcome::Rejected));
}

#[test]
fn separate_engines_with_the_same_signer_cannot_reuse_action_nonces() {
    let mut a = engine();
    let mut b = engine();
    place(&mut a, 901, vec![limit("same-engine-id")]);
    place(&mut b, 901, vec![limit("same-engine-id")]);
    let first: Value = serde_json::from_str(write(&mut a).body.as_deref().unwrap()).unwrap();
    let first = first["nonce"].as_u64().unwrap();
    let second: Value = serde_json::from_str(write(&mut b).body.as_deref().unwrap()).unwrap();
    let second = second["nonce"].as_u64().unwrap();
    assert!(second > first);
}

// A nuke must not leave a stop, and one stop must not keep the rest open.
#[test]
fn cancel_all_sends_stops_with_the_rest() {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus};
    let mut e = engine();
    let symbol = limit("x").symbol;
    let row = |id: &str, kind| Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: symbol.clone(),
        kind,
        side: OrderSide::Sell,
        price: 100.0,
        amount: 1.0,
        filled: 0.0,
        remaining: 1.0,
        reduce_only: true,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    };
    e.store_mut().set_orders(vec![
        row("11", OrderType::Limit),
        row("22", OrderType::StopLoss),
    ]);
    e.handle(
        1000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 9,
            symbol: symbol.clone(),
        }),
    );
    let body = write(&mut e).body.unwrap();
    assert!(body.contains("11") && body.contains("22"), "{body}");
}

#[test]
fn unapproved_revoked_or_expired_agent_cannot_release_an_exchange_write() {
    let mut e = engine();
    place(&mut e, 771, vec![limit("not-approved")]);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 100.0);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
    let authorization = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r)
                if r.body
                    .as_deref()
                    .is_some_and(|body| body.contains("approvedAgents")) =>
            {
                Some(r)
            }
            _ => None,
        })
        .expect("read current approval before any signing");
    let owner =
        serde_json::from_str::<Value>(authorization.body.as_deref().unwrap()).unwrap()["user"]
            .clone();
    let absent = json!({"success":true,"code":0,"data":{"user":owner,"agents":[]}}).to_string();
    ack(&mut e, authorization, 200, &absent);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 100.0);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
    assert!(actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed { command: 771, error })
        if error.kind == Some(safe_cex_engine::ErrorKind::AuthInvalid))
    ));
}

#[test]
fn authorization_read_failures_are_recoverable_without_releasing_a_write() {
    use safe_cex_engine::ErrorKind;
    for (status, body, expected) in [
        (None, "connection reset", ErrorKind::Network),
        // Managed HTTP delivers a non-success read through HttpFailed.
        (Some(503), "unavailable", ErrorKind::Network),
        (Some(200), "malformed", ErrorKind::VenueUnavailable),
        (
            Some(200),
            r#"{"success":true,"code":0,"data":{"user":"0x1111111111111111111111111111111111111111","agents":[]}}"#,
            ErrorKind::VenueUnavailable,
        ),
    ] {
        let mut e = engine();
        place(&mut e, 881, vec![limit("authorization-read")]);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 100.0);
        let authorization = actions
            .iter()
            .find_map(|action| match action {
                Action::Http(r)
                    if r.body
                        .as_deref()
                        .is_some_and(|body| body.contains("approvedAgents")) =>
                {
                    Some(r)
                }
                _ => None,
            })
            .expect("authorization is checked before signing");
        if let Some(status) = status {
            ack(&mut e, authorization, status, body);
        } else {
            e.handle(
                NOW + 100.0,
                Event::HttpFailed {
                    request: authorization.id,
                    error: body,
                },
            );
        }
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 100.0);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
        );
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed {
                    command: 881,
                    error,
                }) => Some(error),
                _ => None,
            })
            .expect("read failure terminates this attempt without an order");
        assert_eq!(error.kind, Some(expected), "{status:?}");
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        place(&mut e, 882, vec![limit("fresh-after-read-failure")]);
        assert!(write(&mut e).url.ends_with("/exchange"));
    }
}
