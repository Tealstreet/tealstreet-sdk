mod support;
mod trusted;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;
fn place(h: &mut Harness, raw: &str) {
    let orders: Vec<PlaceOrderOpts> = serde_json::from_str(raw).unwrap();
    h.handle(
        1.0,
        Event::Command(Command::PlaceOrders { command: 7, orders }),
    );
    h.collect();
}
fn done(h: &Harness) -> bool {
    h.actions
        .iter()
        .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
}
/// No write follows; the only request is the uncertain write's reconcile snapshot.
fn only_reconcile_pending(h: &Harness) {
    let urls: Vec<_> = h.pending.iter().map(|r| r.url.as_str()).collect();
    assert_eq!(
        urls,
        ["https://openapi.blofin.com/api/v1/trade/orders-pending?limit=100"]
    );
}
fn seed_order(h: &mut Harness, id: &str, extra: &str) {
    h.ws(&format!(r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"{id}","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"60000","state":"live"{extra}}}]}}"#));
}

fn seed_attached_entry(h: &mut Harness, filled: u32) {
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"balance":"1000","available":"1000","frozen":"0"}]}"#,
    );
    let state = if filled == 0 {
        "live"
    } else {
        "partially_filled"
    };
    h.reply("orders-pending", &format!(r#"{{"code":"0","data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","filledSize":"{filled}","price":"60000","state":"{state}","tpTriggerPrice":"65000","slTriggerPrice":"55000"}}]}}"#));
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
}

#[test]
fn unknown_placement_rows_present_uncertainty_instead_of_rejection() {
    for (code, message) in [
        ("103013", "internal failure"),
        ("150003", "clientId already exist"),
        ("987654", "unlisted failure"),
    ] {
        let mut h = Harness::new();
        h.seed();
        place(
            &mut h,
            r#"[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}]"#,
        );
        let request = h.take("batch-orders");
        let response = serde_json::json!({
            "code": "0", "data": [{"orderId": "0", "code": code, "msg": message}],
        });
        h.respond(request, &response.to_string());
        let [error] = h.failures.as_slice() else {
            panic!("{code}: expected one terminal failure");
        };
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        let text = error.legacy_text();
        assert!(text.contains("unknown"), "{code}: {text}");
        assert!(text.contains("Check Open Orders"), "{code}: {text}");
        assert!(!text.contains("rejected"), "{code}: {text}");
        assert_eq!(error.venue_code.as_deref(), Some(code));
        assert_eq!(error.venue_message.as_deref(), Some(message));
        assert!(
            h.pending
                .iter()
                .all(|r| r.method == safe_cex_engine::Method::Get)
        );
    }
}

#[test]
fn unknown_cancel_rows_present_uncertainty_instead_of_rejection() {
    for (code, top_level) in ["103013", "150003", "987654"]
        .into_iter()
        .flat_map(|code| [(code, false), (code, true)])
    {
        let mut h = Harness::new();
        h.open_private();
        h.seed();
        seed_order(&mut h, "12", "");
        h.handle(
            1.0,
            Event::Command(Command::CancelOrders {
                command: 7,
                ids: vec!["12".into()],
            }),
        );
        h.collect();
        let request = h.take("cancel-batch-orders");
        let response = if top_level {
            serde_json::json!({"code": code, "msg": "internal failure", "data": []})
        } else {
            serde_json::json!({
                "code": "0", "data": [{"orderId": "12", "code": code, "msg": "internal failure"}],
            })
        };
        h.respond(request, &response.to_string());
        let [error] = h.failures.as_slice() else {
            panic!("{code}: expected one terminal failure");
        };
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        let text = error.legacy_text();
        assert!(text.contains("unknown"), "{code}: {text}");
        assert!(text.contains("Check Open Orders"), "{code}: {text}");
        assert!(!text.contains("rejected"), "{code}: {text}");
        assert_eq!(error.venue_code.as_deref(), Some(code));
        assert_eq!(error.venue_message.as_deref(), Some("internal failure"));
        assert!(
            h.pending
                .iter()
                .all(|r| r.method == safe_cex_engine::Method::Get)
        );
    }
}

#[test]
fn unknown_top_level_placement_errors_present_uncertainty() {
    for code in ["103013", "150003", "987654"] {
        let mut h = Harness::new();
        h.seed();
        place(
            &mut h,
            r#"[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}]"#,
        );
        let request = h.take("batch-orders");
        let response = serde_json::json!({"code": code, "msg": "internal failure", "data": []});
        h.respond(request, &response.to_string());
        let [error] = h.failures.as_slice() else {
            panic!("{code}: expected one terminal failure");
        };
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        let text = error.legacy_text();
        assert!(text.contains("unknown"), "{code}: {text}");
        assert!(text.contains("Check Open Orders"), "{code}: {text}");
        assert_eq!(error.venue_code.as_deref(), Some(code));
        assert_eq!(error.venue_message.as_deref(), Some("internal failure"));
        assert!(
            h.pending
                .iter()
                .all(|r| r.method == safe_cex_engine::Method::Get)
        );
    }
}

fn assert_codeless_write_requires_uncertainty(response: &str) {
    for cancel in [true, false] {
        let mut h = Harness::new();
        h.open_private();
        h.seed();
        let path = if cancel {
            seed_order(&mut h, "12", "");
            h.handle(
                1.0,
                Event::Command(Command::CancelOrders {
                    command: 7,
                    ids: vec!["12".into()],
                }),
            );
            h.collect();
            "cancel-batch-orders"
        } else {
            place(
                &mut h,
                r#"[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}]"#,
            );
            "batch-orders"
        };
        let request = h.take(path);
        h.respond(request, response);
        let [error] = h.failures.as_slice() else {
            panic!("{path}: expected one terminal failure");
        };
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        let text = error.legacy_text();
        assert!(text.contains("unknown"), "{path}: {text}");
        assert!(text.contains("Check Open Orders"), "{path}: {text}");
        assert!(!text.contains("rejected"), "{path}: {text}");
        assert!(
            h.pending
                .iter()
                .all(|r| r.method == safe_cex_engine::Method::Get)
        );
    }
}

#[test]
fn blank_write_error_code_requires_uncertainty_guidance() {
    assert_codeless_write_requires_uncertainty(r#"{"code":"","msg":"internal failure","data":[]}"#);
}

#[test]
fn missing_write_error_code_requires_uncertainty_guidance() {
    assert_codeless_write_requires_uncertainty(r#"{"msg":"internal failure","data":[]}"#);
}

fn assert_combined_leg_cancel_unknown(code: &str) {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","slTriggerPrice":"55000","slOrderPrice":"-1","state":"live"}]}"#);
    h.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["91__take_profit".into()],
        }),
    );
    h.collect();
    let request = h.take("amend-tpsl");
    h.respond(
        request,
        &serde_json::json!({"code": code, "msg": "internal failure", "data": []}).to_string(),
    );
    let [error] = h.failures.as_slice() else {
        panic!("{:?}", h.failures)
    };
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    let text = error.legacy_text();
    assert!(
        text.contains("unknown") && text.contains("Check Open Orders"),
        "{text}"
    );
    assert!(
        !h.pending
            .iter()
            .any(|r| r.method == safe_cex_engine::Method::Post)
    );
}

#[test]
fn combined_leg_cancel_internal_code_retains_uncertainty_guidance() {
    assert_combined_leg_cancel_unknown("103013");
}

#[test]
fn combined_leg_cancel_blank_code_retains_uncertainty_guidance() {
    assert_combined_leg_cancel_unknown("");
}

#[test]
fn combined_leg_cancel_unlisted_code_retains_uncertainty_guidance() {
    assert_combined_leg_cancel_unknown("999999");
}

#[derive(Clone, Copy, Debug)]
enum WriteFailure<'a> {
    Response(u16, &'a str),
    HttpFailed,
    Timeout,
}

#[test]
fn dispatched_ambiguity_covers_every_write_path() {
    let responses = [
        (
            200,
            r#"{"code":"103013","msg":"internal failure","data":[]}"#,
        ),
        (200, r#"{"code":"","msg":"internal failure","data":[]}"#),
        (
            200,
            r#"{"code":"999999","msg":"internal failure","data":[]}"#,
        ),
        (
            400,
            r#"{"code":"103013","msg":"internal failure","data":[]}"#,
        ),
        (400, r#"{"code":"","msg":"internal failure","data":[]}"#),
        (
            400,
            r#"{"code":"999999","msg":"internal failure","data":[]}"#,
        ),
        (400, r#"{"msg":"internal failure","data":[]}"#),
        (400, "Bad Request"),
        (
            200,
            r#"{"code":"0","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"103013","msg":"internal failure"}]}"#,
        ),
        (
            200,
            r#"{"code":"0","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"999999","msg":"internal failure"}]}"#,
        ),
        (
            200,
            r#"{"code":"0","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"","msg":"internal failure"}]}"#,
        ),
        (
            400,
            r#"{"code":"0","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"103013","msg":"internal failure"}]}"#,
        ),
        (
            400,
            r#"{"code":"0","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"999999","msg":"internal failure"}]}"#,
        ),
        (
            400,
            r#"{"code":"0","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"","msg":"internal failure"}]}"#,
        ),
        (
            200,
            r#"{"code":"0","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"102014","msg":"refused"},{"orderId":"12","tpslId":"91","algoId":"78","code":"103013","msg":"internal failure"}]}"#,
        ),
        (
            200,
            r#"{"code":"0","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"102014","msg":"refused"},{"orderId":"12","tpslId":"91","algoId":"78","code":"999999","msg":"internal failure"}]}"#,
        ),
        (
            200,
            r#"{"code":"0","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"102014","msg":"refused"},{"orderId":"12","tpslId":"91","algoId":"78","code":"","msg":"internal failure"}]}"#,
        ),
        (
            200,
            r#"{"code":"102014","msg":"envelope refusal","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"103013","msg":"internal failure"}]}"#,
        ),
        (
            200,
            r#"{"code":"102014","msg":"envelope refusal","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"999999","msg":"internal failure"}]}"#,
        ),
        (
            200,
            r#"{"code":"102014","msg":"envelope refusal","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"","msg":"internal failure"}]}"#,
        ),
        (
            400,
            r#"{"code":"102014","msg":"envelope refusal","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"103013","msg":"internal failure"}]}"#,
        ),
        (
            400,
            r#"{"code":"102014","msg":"envelope refusal","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"999999","msg":"internal failure"}]}"#,
        ),
        (
            400,
            r#"{"code":"102014","msg":"envelope refusal","data":[{"orderId":"12","tpslId":"91","algoId":"78","code":"","msg":"internal failure"}]}"#,
        ),
        (429, "{}"),
        (429, r#"{"code":"152401","msg":"Invalid signature"}"#),
        (200, r#"{"code":"429","msg":"Too Many Requests"}"#),
        (
            200,
            r#"{"code":"0","data":[{"orderId":"12","tpslId":"91","code":"429","msg":"Too Many Requests"}]}"#,
        ),
        (408, r#"{"code":"152401","msg":"Invalid signature"}"#),
        (500, r#"{"code":"152401","msg":"Invalid signature"}"#),
        (503, r#"{"code":"152401","msg":"Invalid signature"}"#),
    ];
    assert_dispatched_ambiguity(
        responses
            .into_iter()
            .map(|(status, body)| WriteFailure::Response(status, body)),
    );
}

#[test]
fn dispatched_http_failed_covers_every_write_path() {
    assert_dispatched_ambiguity([WriteFailure::HttpFailed]);
}

#[test]
fn dispatched_timeout_covers_every_write_path() {
    assert_dispatched_ambiguity([WriteFailure::Timeout]);
}

#[test]
fn uncertain_batch_envelopes_override_placement_rows_before_put_back() {
    assert_uncertain_batch_envelopes(&[
        "create",
        "protective-create",
        "algo-create",
        "amend-replacement",
        "protective-replacement",
        "protective-put-back",
    ]);
}

#[test]
fn uncertain_batch_envelopes_override_cancellation_rows_before_replacement() {
    assert_uncertain_batch_envelopes(&[
        "cancel",
        "protective-cancel",
        "algo-cancel",
        "combined-cancel",
        "amend",
    ]);
}

fn assert_uncertain_batch_envelopes(operations: &[&str]) {
    for envelope in ["103013", "999999", "", "429"] {
        for row_code in ["102014", "0", "1000", "102068", "103013", "999999", ""] {
            for status in [200, 400] {
                for single in [false, true] {
                    let row = serde_json::json!({
                        "orderId": "12", "tpslId": "91", "algoId": "78", "code": row_code,
                        "msg": if row_code == "0" { "success" } else { "row reply" },
                    });
                    let data = if single {
                        row
                    } else {
                        serde_json::json!([row])
                    };
                    let response = serde_json::json!({
                        "code": envelope, "msg": "uncertain envelope", "data": data,
                    })
                    .to_string();
                    assert_dispatched_ambiguity_for(
                        [WriteFailure::Response(status, &response)],
                        operations,
                    );
                }
            }
        }
    }
}

fn assert_dispatched_ambiguity<'a>(failures: impl IntoIterator<Item = WriteFailure<'a>>) {
    assert_dispatched_ambiguity_for(
        failures,
        &[
            "create",
            "protective-create",
            "algo-create",
            "amend",
            "amend-replacement",
            "protective-replacement",
            "protective-put-back",
            "cancel",
            "protective-amend",
            "protective-cancel",
            "algo-amend",
            "algo-cancel",
            "attached-amend",
            "combined-tp-cancel",
            "combined-sl-cancel",
            "leverage",
            "margin-mode",
            "position-mode",
            "combined-cancel",
        ],
    );
}

fn assert_dispatched_ambiguity_for<'a>(
    failures: impl IntoIterator<Item = WriteFailure<'a>>,
    operations: &[&str],
) {
    for failure in failures {
        for &operation in operations {
            let mut h = Harness::new();
            h.open_private();
            if matches!(
                operation,
                "protective-replacement" | "protective-put-back" | "attached-amend"
            ) {
                seed_attached_entry(&mut h, if operation != "attached-amend" { 4 } else { 0 });
            } else {
                h.seed();
                seed_order(&mut h, "12", "");
            }
            let sibling = if operation.starts_with("combined-") {
                r#","slTriggerPrice":"55000","slOrderPrice":"-1""#
            } else {
                ""
            };
            h.ws(&format!(r#"{{"arg":{{"channel":"orders-algo"}},"data":[{{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"index","state":"live"{sibling}}}]}}"#));
            h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","algoId":"78","orderType":"trigger","side":"buy","size":"3","triggerPrice":"61000","orderPrice":"60900","triggerPriceType":"mark","marginMode":"isolated","positionSide":"long","state":"live"}]}"#);
            fresh_prices(&mut h);
            let protective =
                h.e.store()
                    .orders()
                    .iter()
                    .find(|o| o.kind == safe_cex_engine::model::OrderType::TakeProfit)
                    .unwrap()
                    .id
                    .clone();
            let (command, path) = match operation {
                "create" | "protective-create" | "algo-create" => {
                    let kind = match operation {
                        "create" => "market",
                        "protective-create" => "stop_market",
                        _ => "conditional",
                    };
                    let orders = serde_json::from_value(serde_json::json!([{"symbol":"BTCUSDT","type":kind,"side":"sell","amount":1,"price":55000,"triggerPrice":61000,"reduceOnly":operation != "algo-create"}])).unwrap();
                    (
                        Command::PlaceOrders { command: 7, orders },
                        match operation {
                            "create" => "batch-orders",
                            "protective-create" => "order-tpsl",
                            _ => "order-algo",
                        },
                    )
                }
                "amend"
                | "amend-replacement"
                | "protective-replacement"
                | "protective-put-back"
                | "protective-amend"
                | "attached-amend"
                | "algo-amend" => (
                    Command::AmendOrder {
                        command: 7,
                        id: match operation {
                            "amend" | "amend-replacement" => "12".into(),
                            "protective-replacement" | "protective-put-back" | "attached-amend" => {
                                "12__stop_loss".into()
                            }
                            "algo-amend" => "78".into(),
                            _ => protective.clone(),
                        },
                        price: Some(
                            if operation == "protective-replacement"
                                || operation == "protective-put-back"
                                || operation == "attached-amend"
                            {
                                56000.0
                            } else {
                                66000.0
                            },
                        ),
                        amount: None,
                    },
                    match operation {
                        "amend"
                        | "amend-replacement"
                        | "protective-replacement"
                        | "protective-put-back" => "cancel-batch-orders",
                        "attached-amend" => "amend-order",
                        "algo-amend" => "amend-algo",
                        _ => "amend-tpsl",
                    },
                ),
                "cancel" | "protective-cancel" | "algo-cancel" | "combined-tp-cancel"
                | "combined-sl-cancel" | "combined-cancel" => (
                    Command::CancelOrders {
                        command: 7,
                        ids: if operation == "combined-cancel" {
                            vec!["91__take_profit".into(), "91__stop_loss".into()]
                        } else {
                            vec![match operation {
                                "cancel" => "12".into(),
                                "algo-cancel" => "78".into(),
                                "combined-tp-cancel" => "91__take_profit".into(),
                                "combined-sl-cancel" => "91__stop_loss".into(),
                                _ => protective.clone(),
                            }]
                        },
                    },
                    match operation {
                        "cancel" => "cancel-batch-orders",
                        "algo-cancel" => "cancel-algo",
                        "combined-tp-cancel" | "combined-sl-cancel" => "amend-tpsl",
                        _ => "cancel-tpsl",
                    },
                ),
                "margin-mode" => (
                    Command::SetMarginType {
                        command: 7,
                        symbol: "BTCUSDT".into(),
                        isolated: true,
                    },
                    "set-margin-mode",
                ),
                "position-mode" => (
                    Command::SetPositionMode {
                        command: 7,
                        symbol: None,
                        hedged: true,
                    },
                    "set-position-mode",
                ),
                _ => (
                    Command::SetLeverage {
                        command: 7,
                        symbol: "BTCUSDT".into(),
                        leverage: 30.0,
                        side: None,
                    },
                    "set-leverage",
                ),
            };
            h.handle(1.0, Event::Command(command));
            h.collect();
            assert!(
                h.pending.iter().any(|r| r.url.contains(path)),
                "{operation}: {:?}",
                h.failures
            );
            let mut request = h.take(path);
            if matches!(
                operation,
                "amend-replacement" | "protective-replacement" | "protective-put-back"
            ) {
                h.respond(
                    request,
                    r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
                );
                let (timer, at) = h
                    .actions
                    .iter()
                    .rev()
                    .find_map(|a| match a {
                        Action::SetTimer { timer, at_ms } if *at_ms == h.now + 350.0 => {
                            Some((*timer, *at_ms))
                        }
                        _ => None,
                    })
                    .unwrap();
                h.handle(at, Event::TimerFired { timer });
                h.collect();
                let filled = if operation != "amend-replacement" {
                    4
                } else {
                    0
                };
                h.reply("order-detail", &format!(r#"{{"code":"0","data":{{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"{filled}"}}}}"#));
                request = h.take("batch-orders");
                if operation == "protective-put-back" {
                    h.respond(
                        request,
                        r#"{"code":"102014","msg":"trigger refused","data":[]}"#,
                    );
                    request = h.take("batch-orders");
                }
            }
            assert_eq!(request.method, safe_cex_engine::Method::Post, "{operation}");
            let settings = h.e.store().setting("BTCUSDT").unwrap();
            let live_orders = |h: &Harness| {
                serde_json::to_value(
                    h.e.store()
                        .orders()
                        .iter()
                        .filter(|o| o.status != safe_cex_engine::model::OrderStatus::Creating)
                        .cloned()
                        .map(|mut order| {
                            // Pending UI statuses clear when the command ends.
                            if matches!(
                                order.status,
                                safe_cex_engine::model::OrderStatus::Updating
                                    | safe_cex_engine::model::OrderStatus::Canceling
                            ) {
                                order.status = safe_cex_engine::model::OrderStatus::Open;
                            }
                            order
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap()
            };
            let orders = live_orders(&h);
            match failure {
                WriteFailure::Response(status, response) => {
                    h.respond_status(request, status, response)
                }
                WriteFailure::HttpFailed => {
                    h.handle(
                        h.now + 1.,
                        Event::HttpFailed {
                            request: request.id,
                            error: "fixture connection reset",
                        },
                    );
                    h.collect();
                }
                WriteFailure::Timeout => {
                    let deadline = h.now + f64::from(request.timeout_ms.unwrap());
                    let timer = h
                        .actions
                        .iter()
                        .rev()
                        .find_map(|action| match action {
                            Action::SetTimer { timer, at_ms } if *at_ms == deadline => Some(*timer),
                            _ => None,
                        })
                        .expect("released write has a deadline timer");
                    h.handle(deadline, Event::TimerFired { timer });
                    h.collect();
                }
            }
            assert_eq!(h.failures.len(), 1, "{operation} {failure:?}");
            let error = &h.failures[0];
            assert_eq!(
                error.outcome,
                Some(safe_cex_engine::Outcome::Unknown),
                "{operation} {failure:?}: {error:?}"
            );
            if matches!(failure, WriteFailure::HttpFailed) {
                assert!(
                    error.legacy_text().contains("fixture connection reset")
                        || error
                            .venue_message
                            .as_deref()
                            .is_some_and(|message| message.contains("fixture connection reset")),
                    "transport diagnostic retained: {operation}: {error:?}"
                );
                if operation != "protective-put-back" {
                    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Network));
                }
            }
            if matches!(failure, WriteFailure::Timeout) && operation != "protective-put-back" {
                assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Timeout));
            }
            assert_eq!(error.protection, None, "{operation} {failure:?}: {error:?}");
            assert!(
                error.message.to_lowercase().contains("unknown"),
                "{operation} {failure:?}: {error:?}"
            );
            assert!(
                error.message.contains("Check Open Orders"),
                "{operation} {failure:?}: {error:?}"
            );
            assert!(
                !error.message.contains("refused")
                    || (operation == "protective-put-back"
                        && error.message.contains("put-back's outcome is unknown")),
                "{operation} {failure:?}: {error:?}"
            );
            assert!(!error.message.contains("has no stop"), "{error:?}");
            for action in &h.actions {
                if let Action::Notice(Notice::CommandError {
                    command: 7,
                    message,
                }) = action
                {
                    assert!(
                        message.contains("unknown") && message.contains("Check Open Orders"),
                        "warning uses final outcome: {operation} {failure:?}: {message}"
                    );
                }
            }
            let after = h.e.store().setting("BTCUSDT").unwrap();
            assert_eq!(after.leverage.to_bits(), settings.leverage.to_bits());
            assert_eq!(
                after.buy_leverage.map(f64::to_bits),
                settings.buy_leverage.map(f64::to_bits)
            );
            assert_eq!(
                after.sell_leverage.map(f64::to_bits),
                settings.sell_leverage.map(f64::to_bits)
            );
            assert_eq!(after.margin_type, settings.margin_type);
            assert!(!h.e.store().hedged());
            assert_eq!(live_orders(&h), orders, "{operation} {failure:?}");
            assert!(
                h.pending
                    .iter()
                    .all(|r| r.method == safe_cex_engine::Method::Get),
                "no resend or put-back: {operation}"
            );
        }
    }
}
#[test]
fn bybit_execution_controls_reject_entire_batch_before_writes() {
    for (field, value) in [
        (
            "bybitBbo",
            serde_json::json!({"sideType":"Queue","level":1}),
        ),
        ("bybitSlippageToleranceTicks", serde_json::json!(2)),
    ] {
        let mut h = Harness::new();
        h.seed();
        let mut orders = serde_json::json!([
            {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"first"},
            {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"second"}
        ]);
        orders[1][field] = value;
        place(&mut h, &orders.to_string());
        assert!(h.pending.is_empty());
        assert!(h.e.store().orders().is_empty());
        assert!(h.actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 7, error }) if error.legacy_text().contains("Bybit")
        )));
    }
}

#[test]
fn invalid_final_caller_id_rejects_whole_batch_before_any_write() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"valid"},{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1,"clientOrderId":"bad-id"}]"#,
    );
    assert!(h.pending.is_empty());
    assert!(done(&h));
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn caller_id_is_verbatim_and_brackets_stay_on_normal_batch() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1.23,"price":60000.04,"clientOrderId":"Bare123"},{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":2,"price":60000,"takeProfit":65000,"stopLoss":55000}]"#,
    );
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p[0]["clientOrderId"], "Bare123");
    assert_eq!(p[0]["size"], "1.2");
    assert_eq!(p[0]["price"], "60000");
    assert_eq!(p[1]["attachAlgoOrders"][0]["slTriggerPrice"], "55000");
    h.respond(r,r#"{"code":"0","data":[{"orderId":"1","code":"0","msg":"Order placed"},{"orderId":"2","code":"0","msg":"success"}]}"#);
    assert!(!done(&h));
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn lost_placement_response_is_never_retried() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}]"#,
    );
    let r = h.take("batch-orders");
    h.handle(
        2.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost after acceptance",
        },
    );
    h.collect();
    assert!(done(&h));
    only_reconcile_pending(&h);
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn successful_envelope_does_not_hide_mismatched_or_failed_cancel_rows() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    seed_order(&mut h, "123", "");
    h.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["12".into()],
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"123","code":"1000"}]}"#,
    );
    assert!(done(&h));
    assert!(h.e.store().order("12").is_some());
    assert!(h.e.store().order("123").is_some());
    h.actions.clear();
    h.handle(
        2.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["12".into()],
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"500","msg":"permission denied"}]}"#,
    );
    assert!(done(&h));
    assert!(h.e.store().order("12").is_some());
}
#[test]
fn already_gone_cancel_only_removes_correlated_identity() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    seed_order(&mut h, "123", "");
    h.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["12".into()],
        }),
    );
    h.collect();
    h.reply("cancel-batch-orders",r#"{"code":"1","data":[{"orderId":"12","code":"1000","msg":"Cancel failed as the order has been filled, triggered, canceled or does not exist"}]}"#);
    assert!(!done(&h));
    assert!(h.e.store().order("12").is_none());
    assert!(h.e.store().order("123").is_some());
}
#[test]
fn rejected_cancel_prevents_amend_replacement() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"500","msg":"denied"}]}"#,
    );
    assert!(done(&h));
    only_reconcile_pending(&h);
}
#[test]
fn a_refused_take_profit_edit_leaves_it_in_place() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"index","state":"live"}]}"#);
    let id =
        h.e.store()
            .orders()
            .iter()
            .find(|o| o.kind == safe_cex_engine::model::OrderType::TakeProfit)
            .expect("the resting take profit")
            .id
            .clone();
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id,
            price: Some(66000.0),
            amount: None,
        }),
    );
    h.collect();
    let r = h.take("amend-tpsl");
    let body: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"instId":"BTC-USDT","tpslId":"91","newTpTriggerPrice":"66000"})
    );
    h.respond(
        r,
        r#"{"code":"102014","msg":"trigger refused","data":null}"#,
    );
    let error = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("a failure");
    assert!(
        error.message.contains("BTCUSDT take profit is unchanged"),
        "{}",
        error.message
    );
    assert!(!h.pending.iter().any(|r| r.url.contains("cancel")));
}

/// Fresh mark, last and index prices: a replaced stop's cancel needs them.
fn fresh_prices(h: &mut Harness) {
    h.e.store_mut().update_ticker("BTCUSDT", |t| {
        t.mark = 60010.0;
        t.last = 60005.0;
        t.index = 60000.0;
    });
}

#[test]
fn conditional_amend_preserves_nested_protection_and_native_execution_provenance() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","algoId":"77","orderType":"trigger","side":"buy","size":"3","triggerPrice":"61000","orderPrice":"60900","triggerPriceType":"mark","marginMode":"isolated","positionSide":"long","state":"live","attachAlgoOrders":[{"tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"index","slTriggerPrice":"55000","slOrderPrice":"-1","slTriggerPriceType":"mark"}]}]}"#);
    fresh_prices(&mut h);
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "77".into(),
            price: Some(62000.0),
            amount: None,
        }),
    );
    h.collect();
    h.reply(
        "cancel-algo",
        r#"{"code":"0","data":{"algoId":"77","code":"0"}}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && *at_ms == h.now + 350.0
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    let at = h.now + 350.0;
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    let r = h.take("order-algo");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p["triggerPrice"], "62000");
    assert_eq!(p["orderPrice"], "60900");
    assert_eq!(p["marginMode"], "isolated");
    assert_eq!(p["positionSide"], "long");
    assert_eq!(p["triggerPriceType"], "mark");
    assert_eq!(p["attachAlgoOrders"][0]["slTriggerPriceType"], "mark");
    h.respond(r, r#"{"code":"0","data":{"algoId":"88","code":"0"}}"#);
    assert!(!done(&h));
}
#[test]
fn a_limit_whose_replacement_is_refused_says_the_order_is_gone() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0","msg":null}]}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 350.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    let at = h.now + 350.0;
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#);
    let r = h.take("batch-orders");
    h.respond(
        r,
        r#"{"code":"102014","msg":"replacement refused","data":null}"#,
    );
    let error = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("a failure");
    assert!(
        error.message.contains("BTC") && error.message.contains("order is gone"),
        "{}",
        error.message
    );
}

#[test]
fn settings_rejections_preserve_known_leverage() {
    let mut h = Harness::new();
    h.seed();
    h.handle(
        1.0,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        }),
    );
    h.collect();
    h.reply("set-leverage", r#"{"code":"500","msg":"denied"}"#);
    assert!(done(&h));
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
}
#[test]
fn legacy_split_sizing_uses_rounded_equal_lots_and_native_remainder() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 1000.9 of requested 1001); kept for exact TS parity per Sam's decision
    // TS:2199-2220 yields 333.3 * 3 + 1.
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1001}]"#,
    );
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    let sizes = p
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["size"].as_str().unwrap().parse::<f64>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(sizes, [333.3, 333.3, 333.3, 1.0]);
    assert!(sizes.iter().all(|s| *s <= 500.0));
    let allocated_tenths = sizes
        .iter()
        .map(|size| (size * 10.0).round() as u64)
        .sum::<u64>();
    assert_eq!(allocated_tenths, 10_009);
}
#[test]
fn grouped_normal_batches_keep_their_own_native_receipt_slices() {
    let mut h = Harness::new();
    h.seed();
    let orders: Vec<_> = (0..21).map(|index| serde_json::json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":format!("Caller{index}")})).collect();
    place(&mut h, &serde_json::to_string(&orders).unwrap());
    let first = h.take("batch-orders");
    let payload: serde_json::Value = serde_json::from_str(first.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.as_array().unwrap().len(), 20);
    let first_receipts = h
        .actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandProgress {
                command: 7,
                receipts,
            }) => Some(receipts),
            _ => None,
        })
        .unwrap();
    assert_eq!(first_receipts.len(), 20);
    assert_eq!(first_receipts[0].identity, "Caller0");
    assert_eq!(first_receipts[19].identity, "Caller19");
    let ack: Vec<_> = (0..20)
        .map(|index| serde_json::json!({"orderId":format!("Accepted{index}"),"code":"0"}))
        .collect();
    h.respond(
        first,
        &serde_json::json!({"code":"0","data":ack}).to_string(),
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 350. => Some(*timer),
            _ => None,
        })
        .unwrap();
    let at = h.now + 350.;
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    let second = h.take("batch-orders");
    let payload: serde_json::Value = serde_json::from_str(second.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.as_array().unwrap().len(), 1);
    assert_eq!(payload[0]["clientOrderId"], "Caller20");
    let second_receipts = h
        .actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandProgress {
                command: 7,
                receipts,
            }) => Some(receipts),
            _ => None,
        })
        .unwrap();
    assert_eq!(second_receipts.len(), 1);
    assert_eq!(second_receipts[0].symbol, "BTCUSDT");
    assert_eq!(second_receipts[0].identity, "Caller20");
}

#[test]
fn tiny_partial_protection_and_negative_amend_do_not_become_full_position() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0.01,"price":55000}]"#,
    );
    assert!(done(&h));
    assert!(h.pending.is_empty());
    seed_order(&mut h, "12", "");
    h.actions.clear();
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: None,
            amount: Some(-1.0),
        }),
    );
    h.collect();
    assert!(done(&h));
    assert!(h.pending.is_empty());
}
#[test]
fn full_position_combination_preserves_sibling_trigger_reference() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"index","state":"live"}]}"#);
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":55000}]"#,
    );
    h.reply(
        "cancel-tpsl",
        r#"{"code":"0","data":[{"tpslId":"91","code":"0","msg":null}]}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && *at_ms == h.now + 1000.0
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    let at = h.now + 1000.0;
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    let r = h.take("order-tpsl");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p["size"], "-1");
    assert_eq!(p["tpTriggerPriceType"], "index");
    assert_eq!(p["slTriggerPrice"], "55000");
    h.respond(
        r,
        r#"{"code":"0","data":{"tpslId":"92","code":"0","msg":null}}"#,
    );
    assert!(!done(&h));
}
#[test]
fn cancel_one_combined_leg_recreates_only_its_sibling() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"index","slTriggerPrice":"55000","slOrderPrice":"54900","slTriggerPriceType":"mark","state":"live"}]}"#);
    h.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["91__take_profit".into()],
        }),
    );
    h.collect();
    // The amend is tried first (ruled); a refused amend falls back to this path.
    h.reply(
        "amend-tpsl",
        r#"{"code":"102014","msg":"amend refused","data":null}"#,
    );
    h.reply(
        "cancel-tpsl",
        r#"{"code":"0","data":[{"tpslId":"91","code":"0","msg":null}]}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && *at_ms == h.now + 1000.0
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    let at = h.now + 1000.0;
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    let r = h.take("order-tpsl");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert!(p.get("tpTriggerPrice").is_none());
    assert_eq!(p["slTriggerPrice"], "55000");
    assert_eq!(p["slTriggerPriceType"], "mark");
    assert_eq!(p["slOrderPrice"], "54900");
}

#[test]
fn normal_amend_preserves_brackets_before_fallback_children_exist() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(
        &mut h,
        "12",
        r#", "tpTriggerPrice":"65000", "tpOrderPrice":"64900", "tpTriggerPriceType":"index", "slTriggerPrice":"55000""#,
    );
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0","msg":null}]}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && *at_ms == h.now + 350.0
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    let at = h.now + 350.0;
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#);
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p[0]["tpTriggerPrice"], "65000");
    assert_eq!(p[0]["tpOrderPrice"], "64900");
    assert_eq!(p[0]["attachAlgoOrders"][0]["tpTriggerPriceType"], "index");
    assert_eq!(p[0]["attachAlgoOrders"][0]["slTriggerPrice"], "55000");
}
#[test]
fn margin_default_change_preserves_live_position_margin() {
    use safe_cex_engine::model::MarginType;
    let mut h = Harness::new();
    h.seed();
    h.handle(
        1.0,
        Event::Command(Command::SetMarginType {
            command: 7,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    h.collect();
    h.reply(
        "set-margin-mode",
        r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
    );
    assert_eq!(
        h.e.store().positions()[0].margin_type,
        Some(MarginType::Cross)
    );
    assert_eq!(
        h.e.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Cross)
    );
}
#[test]
/// W1 interim (RUST_BLOFIN_WRITE_FUNNEL.md): short follows long's Accepted;
/// W2's barrier restores native parallel release.
fn isolated_hedge_leverage_sends_short_after_long_is_accepted() {
    let mut h = Harness::new();
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"balance":"1000","available":"1000","frozen":"0"}]}"#,
    );
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    h.handle(
        1.0,
        Event::Command(Command::SetMarginType {
            command: 8,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    h.collect();
    h.reply(
        "set-margin-mode",
        r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
    );
    h.handle(
        1.0,
        Event::Command(Command::SetPositionMode {
            command: 9,
            hedged: true,
            symbol: None,
        }),
    );
    h.collect();
    h.reply(
        "set-position-mode",
        r#"{"code":"0","data":{"positionMode":"long_short_mode"}}"#,
    );
    h.handle(
        2.0,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        }),
    );
    h.collect();
    let leg = |h: &Harness| {
        let requests = h
            .pending
            .iter()
            .filter(|r| r.url.ends_with("set-leverage"))
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        requests[0].body.clone().unwrap()
    };
    assert!(leg(&h).contains("\"long\""));
    h.reply("set-leverage", r#"{"code":"0","data":{"leverage":"30"}}"#);
    assert!(leg(&h).contains("\"short\""));
    assert_ne!(h.e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
    h.reply("set-leverage", r#"{"code":"0","data":{"leverage":"30"}}"#);
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
}

#[test]
/// A size on this edit is refused (guarded_replacement.rs); a price edit keeps the remainder.
fn editing_inline_virtual_protection_preserves_parent_remainder() {
    let mut h = Harness::new();
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"balance":"1000","available":"1000","frozen":"0"}]}"#,
    );
    h.reply("orders-pending",r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","filledSize":"4","price":"60000","state":"partially_filled","tpTriggerPrice":"65000","slTriggerPrice":"55000"}]}"#);
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    fresh_prices(&mut h);
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12__take_profit".into(),
            price: Some(66000.0),
            amount: None,
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0","msg":null}]}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && *at_ms == h.now + 350.0
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    let at = h.now + 350.0;
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"4"}}"#);
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p[0]["size"], "6");
    assert_eq!(p[0]["side"], "buy");
    assert_eq!(p[0]["orderType"], "limit");
    assert_eq!(p[0]["price"], "60000");
    assert_eq!(p[0]["tpTriggerPrice"], "66000");
    assert_eq!(p[0]["attachAlgoOrders"][0]["slTriggerPrice"], "55000");
}

#[test]
fn terminal_rows_cannot_be_amended_or_resurrect_protection() {
    let mut h = Harness::new();
    h.seed();
    seed_order(&mut h, "12", "");
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderId":"12","orderCategory":"normal","orderType":"limit","side":"buy","size":"10","price":"60000","state":"canceled"}]}"#);
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect();
    assert!(done(&h));
    assert!(h.pending.is_empty());
    let pair = |state| {
        format!(
            r#"{{"arg":{{"channel":"orders-algo"}},"data":[{{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","slTriggerPrice":"55000","slOrderPrice":"-1","state":"{state}"}}]}}"#
        )
    };
    h.ws(&pair("live"));
    h.ws(&pair("canceled"));
    h.handle(
        2.0,
        Event::Command(Command::CancelOrders {
            command: 8,
            ids: vec!["91__take_profit".into()],
        }),
    );
    h.collect();
    assert!(h.pending.is_empty());
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":54000}]"#,
    );
    let request = h.take("order-tpsl");
    let payload: serde_json::Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
    assert!(payload.get("tpTriggerPrice").is_none());
    assert_eq!(payload["slTriggerPrice"], "54000");
    assert!(h.pending.is_empty());
}

fn amend_normal(h: &mut Harness) {
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect();
}
fn resume_cancel(h: &mut Harness) {
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 350.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    let at = h.now + 350.0;
    h.handle(at, Event::TimerFired { timer });
    h.collect();
}
#[test]
fn already_gone_amend_aborts_without_a_replacement() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    amend_normal(&mut h);
    h.reply("cancel-batch-orders", r#"{"code":"0","data":[{"orderId":"12","code":"1000","msg":"filled, triggered, canceled or does not exist"}]}"#);
    assert!(done(&h));
    only_reconcile_pending(&h);
}
#[test]
fn replacement_requires_exact_canceled_detail_with_unchanged_fill_quantity() {
    for detail in [
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"1"}}"#,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"filled","filledSize":"10"}}"#,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"other","state":"canceled","filledSize":"0"}}"#,
        r#"{"code":"0","data":{"instId":"ETH-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"live","filledSize":"0"}}"#,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"bad"}}"#,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled"}}"#,
        r#"{"code":"0","data":[]}"#,
        r#"{"code":"500","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#,
    ] {
        let mut h = Harness::new();
        h.open_private();
        h.seed();
        seed_order(&mut h, "12", "");
        amend_normal(&mut h);
        h.reply(
            "cancel-batch-orders",
            r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
        );
        resume_cancel(&mut h);
        let request = h.take("order-detail");
        assert_eq!(request.method, safe_cex_engine::Method::Get);
        assert!(
            request
                .url
                .ends_with("order-detail?instId=BTC-USDT&orderId=12")
        );
        assert!(request.body.is_none());
        assert!(
            request
                .headers
                .iter()
                .any(|(name, _)| *name == "ACCESS-SIGN")
        );
        h.respond(request, detail);
        assert!(done(&h), "accepted {detail}");
        assert!(h.pending.is_empty());
    }
}
#[test]
fn stable_final_cancel_detail_allows_only_the_remaining_replacement() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", r#", "filledSize":"4""#);
    amend_normal(&mut h);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    resume_cancel(&mut h);
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"4"}}"#);
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p[0]["size"], "6");
    h.respond(r, r#"{"code":"0","data":[{"orderId":"13","code":"0"}]}"#);
    assert!(!done(&h));
}
#[test]
fn filled_stream_during_cancel_pause_aborts_before_the_detail_or_replacement() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    amend_normal(&mut h);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","size":"10","filledSize":"10","averagePrice":"60000","state":"filled"}]}"#);
    resume_cancel(&mut h);
    assert!(done(&h));
    assert!(h.pending.is_empty());
}
#[test]
fn failed_final_detail_read_is_not_retried_and_never_places() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    amend_normal(&mut h);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    resume_cancel(&mut h);
    let request = h.take("order-detail");
    h.handle(
        350.0,
        Event::HttpFailed {
            request: request.id,
            error: "read unavailable",
        },
    );
    h.collect();
    assert!(done(&h));
    assert!(h.pending.is_empty());
}

#[test]
fn normal_batches_partition_instrument_and_margin_without_reordering_ids() {
    use safe_cex_engine::{SocketId, TimerId};
    use safe_cex_venue_blofin::Options;
    let mut e = trusted::trusted_engine_with(
        Options::default(),
        Some(trusted::Keys::new("key", "secret", "pass")),
    );
    e.start(0.0);
    let mut h = Harness {
        e,
        pending: vec![],
        requests: vec![],
        actions: vec![],
        failures: vec![],
        private: SocketId(0),
        tick: TimerId(0),
        now: 0.0,
        start: 0.0,
    };
    h.collect_at(0.);
    let mut instruments: serde_json::Value = serde_json::from_str(INSTRUMENT).unwrap();
    let mut eth = instruments["data"][0].clone();
    eth["instId"] = "ETH-USDT".into();
    eth["baseCurrency"] = "ETH".into();
    instruments["data"].as_array_mut().unwrap().push(eth);
    h.reply("instruments", &instruments.to_string());
    h.reply("tickers", r#"{"code":"0","data":[]}"#);
    h.modes();
    h.reply(
        "account/positions",
        &format!(
            r#"{{"code":"0","data":[{POSITION},{}]}}"#,
            POSITION
                .replace("BTC-USDT", "ETH-USDT")
                .replace("\"cross\"", "\"isolated\"")
        ),
    );
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"balance":"1000","available":"1000","frozen":"0"}]}"#,
    );
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    place(
        &mut h,
        r#"[
        {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"first"},
        {"symbol":"ETHUSDT","type":"limit","side":"buy","amount":1,"price":3000,"clientOrderId":"second"},
        {"symbol":"ETHUSDT","type":"limit","side":"buy","amount":1,"price":2900,"clientOrderId":"third"},
        {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":59000,"clientOrderId":"fourth"}
    ]"#,
    );
    for (index, (inst, margin, clients, ids)) in [
        ("BTC-USDT", "cross", vec!["first"], vec!["1"]),
        (
            "ETH-USDT",
            "isolated",
            vec!["second", "third"],
            vec!["2", "3"],
        ),
        ("BTC-USDT", "cross", vec!["fourth"], vec!["4"]),
    ]
    .into_iter()
    .enumerate()
    {
        if index > 0 {
            resume_cancel(&mut h);
        }
        let request = h.take("batch-orders");
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(rows.len(), clients.len());
        for (row, client) in rows.iter().zip(clients) {
            assert_eq!(row["instId"], inst);
            assert_eq!(row["marginMode"], margin);
            assert_eq!(row["clientOrderId"], client);
        }
        let data: Vec<_> = ids
            .iter()
            .map(|id| serde_json::json!({"orderId":id,"code":"0"}))
            .collect();
        h.respond(
            request,
            &serde_json::json!({"code":"0","data":data}).to_string(),
        );
    }
    assert!(h.actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandDone { command:7, result })
            if result.as_str() == r#"["1","2","3","4"]"#)));
}

#[test]
fn fill_arriving_during_final_detail_read_blocks_a_stale_detail_replacement() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    amend_normal(&mut h);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    resume_cancel(&mut h);
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","size":"10","filledSize":"2","averagePrice":"60000","state":"partially_filled"}]}"#);
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#);
    assert!(done(&h));
    assert!(h.pending.is_empty());
}

#[test]
fn rest_cancel_tombstone_survives_snapshot_then_delayed_live_replay() {
    for reply in [
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
        r#"{"code":"1","data":[{"orderId":"12","code":"1000"}]}"#,
    ] {
        let mut h = Harness::new();
        h.open_private();
        h.seed();
        seed_order(&mut h, "12", "");
        h.handle(
            1.0,
            Event::Command(Command::CancelOrders {
                command: 7,
                ids: vec!["12".into()],
            }),
        );
        h.collect();
        h.reply("cancel-batch-orders", reply);
        assert!(!done(&h));
        h.handle(10_000.0, Event::TimerFired { timer: h.tick });
        h.collect();
        h.reply("account/positions", r#"{"code":"0","data":[]}"#);
        h.reply("balances", r#"{"code":"0","data":[]}"#);
        for path in [
            "orders-pending",
            "orders-tpsl-pending",
            "orders-algo-pending",
        ] {
            h.reply(path, r#"{"code":"0","data":[]}"#);
        }
        assert!(h.e.store().order("12").is_none());
        seed_order(&mut h, "12", "");
        assert!(h.e.store().order("12").is_none());
    }
}

#[test]
fn successful_cancel_ack_cannot_downgrade_an_intervening_filled_status() {
    use safe_cex_engine::model::OrderStatus;
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    h.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["12".into()],
        }),
    );
    h.collect();
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","size":"10","filledSize":"10","averagePrice":"60000","state":"filled"}]}"#);
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    assert_eq!(h.e.store().order("12").unwrap().status, OrderStatus::Closed);
}

#[test]
fn cancel_symbol_includes_ack_before_private_or_rest_order_arrives() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"Pending1"}]"#,
    );
    h.reply(
        "batch-orders",
        r#"{"code":"0","data":[{"orderId":"ack-only","code":"0","msg":"success"}]}"#,
    );
    assert!(h.e.store().orders().is_empty());
    h.handle(
        2.,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "BTCUSDT".into(),
        }),
    );
    h.collect();
    let cancel = h.take("cancel-batch-orders");
    let payload: serde_json::Value = serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload[0]["orderId"], "ack-only");
    assert_eq!(payload[0]["instId"], "BTC-USDT");
    h.respond(
        cancel,
        r#"{"code":"0","data":[{"orderId":"ack-only","code":"0"}]}"#,
    );
    assert!(h.actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 8,
            result: _
        })
    )));
}

#[test]
fn retained_protective_and_trigger_orders_keep_their_cancel_family() {
    for (kind, trigger, place_path, cancel_path, id_field) in [
        ("stop_market", "", "order-tpsl", "cancel-tpsl", "tpslId"),
        (
            "conditional",
            ",\"triggerPrice\":61000",
            "order-algo",
            "cancel-algo",
            "algoId",
        ),
    ] {
        for bad_ack in [
            r#"{"code":"0","data":[]}"#,
            r#"{"code":"0","data":[{"orderId":"retained","code":"0"}]}"#,
        ] {
            let mut h = Harness::new();
            h.seed();
            place(
                &mut h,
                &format!(
                    r#"[{{"symbol":"BTCUSDT","type":"{kind}","side":"sell","amount":1,"price":60000{trigger}}}]"#
                ),
            );
            let placed = h.take(place_path);
            h.respond(
                placed,
                &format!(r#"{{"code":"0","data":{{"{id_field}":"retained","code":"0"}}}}"#),
            );
            assert!(h.e.store().orders().is_empty());
            h.handle(
                2.,
                Event::Command(Command::CancelSymbolOrders {
                    command: 8,
                    symbol: "BTCUSDT".into(),
                }),
            );
            h.collect();
            let cancel = h.take(cancel_path);
            let payload: serde_json::Value =
                serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
            let row = if kind == "conditional" {
                &payload
            } else {
                &payload[0]
            };
            assert_eq!(row[id_field], "retained");
            assert_eq!(row["instId"], "BTC-USDT");
            h.respond(cancel, bad_ack);
            assert!(
                h.actions
                    .iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, .. })))
            );
            h.handle(
                3.,
                Event::Command(Command::CancelSymbolOrders {
                    command: 9,
                    symbol: "BTCUSDT".into(),
                }),
            );
            h.collect();
            let cancel = h.take(cancel_path);
            h.respond(
                cancel,
                &format!(r#"{{"code":"0","data":[{{"{id_field}":"retained","code":"0"}}]}}"#),
            );
            assert!(h.actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandDone {
                    command: 9,
                    result: _
                })
            )));
        }
    }
}

#[test]
fn ambiguous_placement_ack_cannot_authorize_empty_cancel_all_success() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000}]"#,
    );
    h.reply("batch-orders", r#"{"code":"0","data":[]}"#);
    assert!(done(&h));
    h.handle(
        2.,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "BTCUSDT".into(),
        }),
    );
    h.collect();
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, .. }))),
        "successful empty ACK does not prove placement absence"
    );
}

#[test]
fn cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch() {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    use safe_cex_engine::{Action, Notice};
    for mixed in [false, true] {
        let mut h = Harness::new();
        h.seed();
        h.e.store_mut().set_orders(vec![Order {
            id: "123".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSDT".into(),
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
        h.e.drain_actions().for_each(drop);
        h.handle(
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
        let actions = h.e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 991, error })
                    if error.message.contains("456") && error.message.contains("not found")
                        && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
            )),
            "missing cancellation error: {actions:?}"
        );
        assert_eq!(h.e.store().order("123").unwrap().status, OrderStatus::Open);
    }
}

#[test]
fn broker_payload_gate_covers_normal_trigger_and_protective_orders_without_rewriting_ids() {
    for attributed in [false, true] {
        for (raw, endpoint) in [
            (
                r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":50000,"clientOrderId":"Caller123"}]"#,
                "batch-orders",
            ),
            (
                r#"[{"symbol":"BTCUSDT","type":"conditional","side":"buy","amount":1,"price":50000,"triggerPrice":51000}]"#,
                "order-algo",
            ),
            (
                r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":1,"price":40000}]"#,
                "order-tpsl",
            ),
        ] {
            let mut h = Harness::with_broker_attribution(attributed);
            h.seed();
            place(&mut h, raw);
            let request = h.take(endpoint);
            let body: serde_json::Value =
                serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
            let row = body.as_array().map_or(&body, |rows| &rows[0]);
            if attributed {
                assert_eq!(row["brokerId"], "b3cdedc0a20880ba");
            } else {
                assert!(row.get("brokerId").is_none());
            }
            let client = row["clientOrderId"].as_str().unwrap();
            if raw.contains("Caller123") {
                assert_eq!(client, "Caller123");
            } else {
                assert!(client.starts_with("b3cdedc0a20880ba"));
                assert_eq!(client.len(), 32);
            }
        }
    }
}

#[test]
fn mixed_unknown_cancel_keeps_ack_route_and_sends_no_partial_write() {
    for cached_first in [false, true] {
        let mut h = Harness::new();
        h.seed();
        place(
            &mut h,
            r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":"Pending1"}]"#,
        );
        h.reply(
            "batch-orders",
            r#"{"code":"0","data":[{"orderId":"ack-only","code":"0","msg":"success"}]}"#,
        );
        assert!(h.e.store().orders().is_empty());
        h.handle(
            2.,
            Event::Command(Command::CancelOrders {
                command: 991,
                ids: if cached_first {
                    vec!["ack-only".into(), "missing".into()]
                } else {
                    vec!["missing".into(), "ack-only".into()]
                },
            }),
        );
        h.collect();
        assert!(
            h.pending
                .iter()
                .all(|request| request.method == safe_cex_engine::Method::Get)
        );
        assert!(h.actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandFailed { command: 991, error })
                if error.legacy_text().contains("missing") && error.legacy_text().contains("not found")
        )));
        h.handle(
            3.,
            Event::Command(Command::CancelOrders {
                command: 992,
                ids: vec!["ack-only".into()],
            }),
        );
        h.collect();
        let cancel = h.take("cancel-batch-orders");
        let payload: serde_json::Value =
            serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload[0]["orderId"], "ack-only");
        assert_eq!(payload[0]["instId"], "BTC-USDT");
        h.respond(
            cancel,
            r#"{"code":"0","data":[{"orderId":"ack-only","code":"0"}]}"#,
        );
        assert!(h.actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone {
                command: 992,
                result: _
            })
        )));
    }
}

#[test]
fn invalid_dynamic_leverage_limits_reject_without_native_write() {
    for (min, max) in [
        (30.0, 20.0),
        (f64::NAN, 20.0),
        (1.0, 0.0),
        (1.0, f64::INFINITY),
    ] {
        let mut h = Harness::new();
        h.seed();
        h.pending.clear();
        h.actions.clear();
        h.e.store_mut().update_markets(|market| {
            market.limits.leverage.min = min;
            market.limits.leverage.max = max;
        });
        h.handle(
            1.0,
            Event::Command(Command::SetLeverage {
                command: 99,
                symbol: "BTCUSDT".into(),
                leverage: 5.0,
                side: None,
            }),
        );
        h.collect();
        assert!(h.pending.is_empty());
        assert!(h.actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 99, error }) if error.legacy_text().contains("leverage limits"))));
    }
}

#[test]
fn closed_position_restores_account_margin_default_before_next_order() {
    use safe_cex_engine::model::MarginType;
    // Empty snapshots and explicit zero rows are both valid closure signals.
    for rest in [false, true] {
        for explicit_zero in [false, true] {
            let mut h = Harness::new();
            h.open_private();
            h.seed();
            h.handle(
                1.0,
                Event::Command(Command::SetMarginType {
                    command: 8,
                    symbol: "BTCUSDT".into(),
                    isolated: true,
                }),
            );
            h.collect();
            h.reply("set-margin-mode", r#"{"code":"0","data":{}}"#);
            assert_eq!(
                h.e.store().positions()[0].margin_type,
                Some(MarginType::Cross)
            );
            let data = if explicit_zero {
                format!(
                    "[{}]",
                    POSITION.replace("\"positions\":\"4\"", "\"positions\":\"0\"")
                )
            } else {
                "[]".into()
            };
            if rest {
                h.reply(
                    "account/positions",
                    &format!(r#"{{"code":"0","data":{data}}}"#),
                );
                h.reply(
                    "balances",
                    r#"{"code":"0","data":[{"balance":"1000","available":"1000","frozen":"0"}]}"#,
                );
            } else {
                h.ws(&format!(
                    r#"{{"arg":{{"channel":"positions"}},"data":{data}}}"#
                ));
            }
            assert!(h.e.store().positions().is_empty());
            let setting = h.e.store().setting("BTCUSDT").unwrap();
            assert_eq!(
                setting.margin_type,
                Some(MarginType::Isolated),
                "rest={rest}, zero={explicit_zero}"
            );
            assert_eq!(setting.leverage, 20.0);
            place(
                &mut h,
                r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000}]"#,
            );
            let request = h.take("batch-orders");
            let payload: serde_json::Value =
                serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
            assert_eq!(payload[0]["marginMode"], "isolated");
        }
    }
}

#[test]
fn trading_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome, Response};
    let market = r#"[{"symbol":"BTCUSDT","type":"market","side":"buy","amount":1}]"#;
    let mut h = Harness::new();
    h.seed();
    place(&mut h, market);
    let r = h.take("batch-orders");
    h.handle(
        2.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    h.collect();
    assert_eq!(h.failures[0].outcome, Some(Outcome::Unknown));
    assert_eq!(h.failures[0].kind, Some(ErrorKind::Network));

    // D-a: a documented row code is a typed refusal; a 5xx or an unproven
    // acknowledgement carries the shared projection's kind. Text is unchanged.
    for (status, body, kind, outcome) in [
        (
            502,
            "Bad Gateway",
            Some(ErrorKind::VenueUnavailable),
            Some(Outcome::Unknown),
        ),
        (
            429,
            "{}",
            Some(ErrorKind::RateLimited),
            Some(Outcome::Unknown),
        ),
        (
            200,
            r#"{"code":"1","msg":"","data":[{"orderId":"","code":"103003","msg":"Insufficient balance"}]}"#,
            Some(ErrorKind::Rejected),
            Some(Outcome::Rejected),
        ),
        (
            200,
            r#"{"code":"1","msg":"","data":[{"orderId":"","code":"102002","msg":"Duplicate customized order ID"}]}"#,
            None,
            Some(Outcome::Unknown),
        ),
        (
            200,
            r#"{"code":"1","msg":"","data":[{"orderId":"","code":"150003","msg":"clientId already exist"}]}"#,
            None,
            Some(Outcome::Unknown),
        ),
        (
            200,
            r#"{"code":"1","msg":"","data":[{"orderId":"","code":"152030","msg":"Duplicated clientId."}]}"#,
            None,
            Some(Outcome::Unknown),
        ),
        (
            200,
            r#"{"code":"0","msg":"","data":[{"orderId":"9","msg":"Order placed successfully"}]}"#,
            Some(ErrorKind::Internal),
            Some(Outcome::Unknown),
        ),
        (
            200,
            r#"{"code":"0","msg":"","data":[]}"#,
            Some(ErrorKind::Internal),
            Some(Outcome::Unknown),
        ),
    ] {
        let mut h = Harness::new();
        h.seed();
        place(&mut h, market);
        let r = h.take("batch-orders");
        h.handle(
            2.0,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        h.collect();
        let [error] = h.failures.as_slice() else {
            panic!("{status} {body}: {:?}", h.failures);
        };
        assert_eq!(
            (error.kind, error.outcome),
            (kind, outcome),
            "{status} {body}: {}",
            error.message
        );
    }
}

#[test]
fn attached_take_profit_and_stop_loss_are_rounded_to_the_price_tick() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"takeProfit":65000.04,"stopLoss":55000.06}]"#,
    );
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    let attached = &p[0]["attachAlgoOrders"][0];
    assert_eq!(attached["tpTriggerPrice"], "65000");
    assert_eq!(attached["slTriggerPrice"], "55000.1");
}

#[test]
fn attached_trigger_that_rounds_to_zero_is_refused_before_any_write() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"stopLoss":0.04}]"#,
    );
    assert!(h.pending.is_empty());
    assert!(done(&h));
}

#[test]
fn multi_position_mode_refusal_explains_how_to_continue() {
    for reply in [
        r#"{"code":"102143","msg":"position selection required","data":[]}"#,
        r#"{"code":"0","data":[{"orderId":"","code":"102143","msg":"position selection required"}]}"#,
    ] {
        let mut h = Harness::new();
        h.seed();
        place(
            &mut h,
            r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000}]"#,
        );
        let r = h.take("batch-orders");
        h.respond(r, reply);
        let error = h.failures.last().expect("placement refused");
        assert!(
            error
                .message
                .contains("Turn off Multi-Position mode on BloFin")
        );
        assert_ne!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    }
}

#[test]
fn margin_type_change_warns_while_orders_are_open() {
    for open in [false, true] {
        let mut h = Harness::new();
        h.open_private();
        h.modes();
        h.reply("account/positions", r#"{"code":"0","data":[]}"#);
        h.reply(
            "balances",
            r#"{"code":"0","data":[{"currency":"USDT","balance":"1000","available":"1000","frozen":"0"}]}"#,
        );
        for path in [
            "orders-pending",
            "orders-tpsl-pending",
            "orders-algo-pending",
        ] {
            h.reply(path, r#"{"code":"0","data":[]}"#);
        }
        if open {
            seed_order(&mut h, "1", "");
        }
        h.handle(
            1.0,
            Event::Command(Command::SetMarginType {
                command: 7,
                symbol: "BTCUSDT".into(),
                isolated: true,
            }),
        );
        h.collect();
        assert_eq!(margin_warned(&h), open);
        assert!(h.pending.iter().any(|r| r.url.contains("set-margin-mode")));
    }
}

#[test]
fn margin_type_change_warns_while_a_position_is_open() {
    let mut h = Harness::new();
    h.seed();
    assert!(h.e.store().positions().iter().any(|p| p.contracts > 0.0));
    h.handle(
        1.0,
        Event::Command(Command::SetMarginType {
            command: 7,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    h.collect();
    assert!(margin_warned(&h));
}

fn margin_warned(h: &Harness) -> bool {
    h.actions.iter().any(|a| {
        matches!(
            a,
            Action::Notice(Notice::Event(event))
                if event.level == safe_cex_engine::Level::Error
                    && event.message.starts_with("Changing margin type with open positions / orders")
        )
    })
}

#[test]
fn multi_position_mode_refusal_of_a_protective_order_is_explained() {
    let mut h = Harness::new();
    h.seed();
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":55000}]"#,
    );
    let r = h.take("order-tpsl");
    h.respond(
        r,
        r#"{"code":"102143","msg":"position selection required","data":null}"#,
    );
    let error = h.failures.last().expect("placement refused");
    assert!(
        error
            .message
            .contains("Turn off Multi-Position mode on BloFin")
    );
}

#[test]
fn a_size_amend_of_a_partly_filled_order_treats_the_size_as_the_new_total() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", r#", "filledSize":"4""#);
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: None,
            amount: Some(9.0),
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#,
    );
    resume_cancel(&mut h);
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"4"}}"#);
    let r = h.take("batch-orders");
    let p: serde_json::Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(p[0]["size"], "5");
}

// deaws2's live body.
#[test]
fn a_partly_succeeded_cancel_envelope_stops_but_retains_applied_facts() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_order(&mut h, "1000171264712", "");
    seed_order(&mut h, "1000171264711", "");
    h.handle(
        1.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 7,
            symbol: "BTCUSDT".into(),
        }),
    );
    h.collect();
    h.reply("cancel-batch-orders", r#"{"code":"2","msg":"Batch operation partially succeeded.","data":[{"orderId":"1000171264712","clientOrderId":"","msg":"Cancel failed as the order has been filled, triggered, canceled or does not exist.","code":"1000"},{"orderId":"1000171264711","clientOrderId":"x","msg":"Order canceled","code":"0"}]}"#);
    // TS:2445-2463 treats every non-gone row under envelope 2 as a real failure.
    assert!(done(&h));
    let error = h.failures.last().unwrap();
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
    assert!(error.elements.as_ref().unwrap().iter().all(|row| matches!(
        row.result,
        safe_cex_engine::write_plan::ElementResult::Applied { .. }
    )));
    assert!(h.e.store().order("1000171264712").is_none());
}
