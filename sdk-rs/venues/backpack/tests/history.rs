mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Query};
use support::*;
fn requests(e: &mut Driver) -> Vec<safe_cex_engine::HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(e, 2000.)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}
fn reply(e: &mut Driver, request: &safe_cex_engine::HttpRequest, status: u16, body: &str) {
    e.handle(
        2000.,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn ask(e: &mut Driver, query: Query) {
    e.handle(2000.0, Event::Command(Command::Query { command: 1, query }));
}
fn result(e: &mut Driver) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(Ok(result))
            } else if let Action::Notice(Notice::CommandFailed { error, .. }) = a {
                Some(Err(error.legacy_text()))
            } else {
                None
            }
        })
        .unwrap()
}
#[test]
fn fills_stream_into_execution_shape_with_millisecond_filters_and_seconds_output() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: Some("123".into()),
            from_ms: Some(1765805280000.0),
            to_ms: Some(1765805340000.0),
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.contains("from=1765805280000"));
    assert!(r.url.contains("sortDirection=Desc"));
    reply(
        &mut e,
        &r,
        200,
        r#"[{"orderId":"123","symbol":"SOL_USDC_PERP","side":"Bid","price":"141","quantity":"2","fee":"-0.1","feeSymbol":"USDC","tradeId":999,"timestamp":"2025-12-15T13:28:00.123Z","isMaker":false}]"#,
    );
    let json = result(&mut e).unwrap();
    assert!(json.contains("\"timestamp\":1765805280.0"));
    assert!(json.contains("\"commission\":0.1"));
    assert!(json.contains("\"qty\":2.0"));
    assert!(e.store().orders().is_empty());
}
#[test]
fn malformed_fill_rows_are_skipped_and_logged_while_valid_fills_return() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    let fill = r#"{"orderId":"123","symbol":"SOL_USDC_PERP","side":"Bid","price":"141","quantity":"2","fee":"-0.1","feeSymbol":"USDC","tradeId":999,"timestamp":"2025-12-15T13:28:00.123Z","isMaker":false}"#;
    let null_fee = fill.replace(r#""-0.1""#, "null").replace("999", "998");
    let bad_side = fill.replace("Bid", "Sideways").replace("999", "997");
    reply(&mut e, &r, 200, &format!("[{fill},{null_fee},{bad_side}]"));
    let actions: Vec<_> = e.drain_actions().collect();
    let json = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(result.clone()),
            _ => None,
        })
        .unwrap();
    assert!(json.contains("\"id\":\"999\""));
    assert!(!json.contains("998"));
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. }))
            if message == "Skipped 2 malformed Backpack fill rows: 998, 997")));
}
#[test]
fn order_history_accepts_iso_created_at_and_computes_average_execution_price() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: Some(SYMBOL.into()),
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    let order = ORDER
        .replace(
            "\"createdAt\":1000",
            "\"createdAt\":\"2025-12-15T13:28:00Z\"",
        )
        .replace(
            "\"executedQuantity\":\"0\"",
            "\"executedQuantity\":\"2\",\"executedQuoteQuantity\":\"280\"",
        );
    reply(&mut e, &r, 200, &format!("[{order}]"));
    let json = result(&mut e).unwrap();
    assert!(json.contains("\"createdTime\":1765805280000.0"));
    assert!(json.contains("\"averagePrice\":140.0"));
}
#[test]
fn funding_income_preserves_payment_sign_and_unsupported_ledgers_are_explicit() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::Income {
            symbol: Some(SYMBOL.into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"[{"symbol":"SOL_USDC_PERP","quantity":"-1.25","intervalEndTimestamp":"2025-12-15T13:00:00Z","fundingRate":"0.0001"}]"#,
    );
    let json = result(&mut e).unwrap();
    assert!(json.contains("\"income\":-1.25"));
    assert!(json.contains("\"category\":\"funding\""));
    ask(
        &mut e,
        Query::Income {
            symbol: None,
            kind: Some("REALIZED_PNL".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    assert!(
        result(&mut e)
            .unwrap_err()
            .contains("funding payments only")
    );
}
#[test]
fn public_trades_are_unsigned_and_buyer_maker_means_aggressive_sell() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::PublicTrades {
            symbol: SYMBOL.into(),
            limit: Some(1),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(!r.headers.iter().any(|(k, _)| *k == "X-Signature"));
    reply(
        &mut e,
        &r,
        200,
        r#"[{"id":87992941,"isBuyerMaker":true,"price":"104.12","quantity":"0.1","quoteQuantity":"10.412","timestamp":1792425943313}]"#,
    );
    assert!(result(&mut e).unwrap().contains("\"side\":\"sell\""));
}
#[test]
fn malformed_lists_and_venue_failures_never_claim_empty_success() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::PublicTrades {
            symbol: SYMBOL.into(),
            limit: Some(1),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, r#"{"error":{"message":"not an array"}}"#);
    assert!(result(&mut e).is_err());
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: Some(SYMBOL.into()),
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 503, "unavailable");
    assert!(result(&mut e).is_err());
}

#[test]
fn account_wide_fills_page_past_spot_rows_instead_of_claiming_no_perp_fills() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    let fill = r#"{"orderId":"123","symbol":"SOL_USDC","side":"Bid","price":"141","quantity":"2","fee":"0.1","feeSymbol":"USDC","tradeId":999,"timestamp":"2025-12-15T13:28:00.123+00:00","isMaker":false}"#;
    reply(&mut e, &r, 200, &format!("[{fill}]"));
    let next = requests(&mut e).pop().unwrap();
    assert!(next.url.contains("offset=1"));
    reply(
        &mut e,
        &next,
        200,
        &format!("[{}]", fill.replace("SOL_USDC", "SOL_USDC_PERP")),
    );
    assert!(result(&mut e).unwrap().contains("SOL_USDC_PERP"));
}
#[test]
fn fractional_fill_boundaries_are_broadened_on_wire_and_kept_exact_locally() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: None,
            from_ms: Some(1765805280000.1),
            to_ms: Some(1765805340000.1),
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.contains("from=1765805280000"));
    assert!(r.url.contains("to=1765805340001"));
}

#[test]
fn invalid_order_side_fails_and_a_repeated_full_page_is_not_silently_truncated() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::OrderHistory {
            symbol: Some(SYMBOL.into()),
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        &format!("[{}]", ORDER.replace("Bid", "Unknown")),
    );
    assert!(result(&mut e).is_err());
    ask(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    let r = requests(&mut e).pop().unwrap();
    let spot = r#"[{"orderId":"123","symbol":"SOL_USDC","side":"Bid","price":"141","quantity":"2","fee":"0.1","feeSymbol":"USDC","tradeId":999,"timestamp":"2025-12-15T13:28:00Z"}]"#;
    reply(&mut e, &r, 200, spot);
    let next = requests(&mut e).pop().unwrap();
    reply(&mut e, &next, 200, spot);
    // Shared ownership supersedes the old strict repeat rejection: an empty
    // eligible result still carries a command-owned missing-coverage marker.
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
            && matches!(event.fields.as_slice(), [("command", safe_cex_engine::Value::Num(1.0))]))));
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandDone { command: 1, result }) if result == "[]")));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
}

/// Pins the exact request sequence of an offset-paged read. Paging
/// refactors keep it.
#[test]
fn offset_paged_request_sequence_is_pinned() {
    let (mut e, _) = ready();
    ask(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    let fill = r#"{"orderId":"123","symbol":"SOL_USDC","side":"Bid","price":"141","quantity":"2","fee":"0.1","feeSymbol":"USDC","tradeId":999,"timestamp":"2025-12-15T13:28:00.123+00:00","isMaker":false}"#;
    let mut sent = Vec::new();
    let r = requests(&mut e).pop().unwrap();
    sent.push(
        r.url
            .split_once("/wapi/")
            .map_or(r.url.as_str(), |(_, p)| p)
            .to_owned(),
    );
    reply(&mut e, &r, 200, &format!("[{fill}]"));
    let r = requests(&mut e).pop().unwrap();
    sent.push(
        r.url
            .split_once("/wapi/")
            .map_or(r.url.as_str(), |(_, p)| p)
            .to_owned(),
    );
    reply(
        &mut e,
        &r,
        200,
        &format!("[{}]", fill.replace("SOL_USDC", "SOL_USDC_PERP")),
    );
    assert!(result(&mut e).is_ok());
    assert_eq!(
        sent,
        [
            "v1/history/fills?limit=1&sortDirection=Desc",
            "v1/history/fills?limit=1&offset=1&sortDirection=Desc",
        ]
    );
}

#[test]
fn null_or_missing_order_quantity_uses_proven_trigger_amount_in_history() {
    for missing in [false, true] {
        let (mut e, _) = ready();
        ask(
            &mut e,
            Query::OrderHistory {
                symbol: Some(SYMBOL.into()),
                from_ms: None,
                to_ms: None,
                limit: Some(100),
            },
        );
        let request = requests(&mut e).pop().unwrap();
        let mut row: serde_json::Value = serde_json::from_str(ORDER).unwrap();
        row["quantity"] = serde_json::Value::Null;
        if missing {
            row.as_object_mut().unwrap().remove("quantity");
        }
        row["triggerPrice"] = "130".into();
        row["triggerQuantity"] = "3".into();
        reply(&mut e, &request, 200, &format!("[{row}]"));
        let rows: serde_json::Value = serde_json::from_str(&result(&mut e).unwrap()).unwrap();
        assert_eq!(rows[0]["amount"], 3.0);
    }
}

#[test]
fn null_ordinary_or_unproven_trigger_history_fails_instead_of_returning_zero_amount() {
    for trigger in [false, true] {
        let (mut e, _) = ready();
        ask(
            &mut e,
            Query::OrderHistory {
                symbol: Some(SYMBOL.into()),
                from_ms: None,
                to_ms: None,
                limit: Some(100),
            },
        );
        let request = requests(&mut e).pop().unwrap();
        let mut row: serde_json::Value = serde_json::from_str(ORDER).unwrap();
        row["quantity"] = serde_json::Value::Null;
        if trigger {
            row["triggerPrice"] = "130".into();
            row["triggerQuantity"] = "0".into();
        }
        reply(&mut e, &request, 200, &format!("[{row}]"));
        assert!(result(&mut e).is_err());
    }
}

mod owned_history {
    use super::*;
    use safe_cex_engine::{ErrorKind, HttpRequest, Level, LogEvent, Value, log::code};

    fn fill(id: u64, symbol: &str) -> String {
        format!(
            r#"{{"orderId":"123","symbol":"{symbol}","side":"Bid","price":"141","quantity":"2","fee":"0.1","feeSymbol":"USDC","tradeId":{id},"timestamp":"2025-12-15T13:28:00Z"}}"#
        )
    }
    fn query(e: &mut Driver, command: u32, limit: u32) -> HttpRequest {
        e.handle(
            2000.,
            Event::Command(Command::Query {
                command,
                query: Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: Some(limit),
                },
            }),
        );
        requests(e).pop().expect("history request")
    }
    fn respond(e: &mut Driver, r: &HttpRequest, body: &str, streamed: bool) {
        if streamed {
            e.handle(
                2000.,
                Event::HttpStart {
                    request: r.id,
                    status: 200,
                    headers: "",
                },
            );
            e.handle(
                2000.,
                Event::HttpChunk {
                    request: r.id,
                    data: body.as_bytes(),
                },
            );
            e.handle(2000., Event::HttpEnd { request: r.id });
        } else {
            reply(e, r, 200, body);
        }
    }
    fn fail_page(e: &mut Driver, r: &HttpRequest, mode: u8) {
        let prefix = fill(999, SYMBOL);
        match mode {
            0 => reply(e, r, 503, &format!("[{prefix}]")),
            1 => {
                e.handle(
                    2000.,
                    Event::HttpStart {
                        request: r.id,
                        status: 200,
                        headers: "",
                    },
                );
                let chunk = format!("[{prefix},");
                e.handle(
                    2000.,
                    Event::HttpChunk {
                        request: r.id,
                        data: chunk.as_bytes(),
                    },
                );
                e.handle(
                    2000.,
                    Event::HttpFailed {
                        request: r.id,
                        error: "fixture history transport loss",
                    },
                );
            }
            2 => respond(e, r, &format!("[{prefix},"), true),
            _ => unreachable!(),
        }
    }
    fn markers(actions: &[Action]) -> Vec<&LogEvent> {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event)) if event.code == code::HISTORY_INCOMPLETE => {
                    Some(event)
                }
                _ => None,
            })
            .collect()
    }
    fn rows(actions: &[Action], command: u32) -> Vec<serde_json::Value> {
        let result = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone {
                    command: id,
                    result,
                }) if *id == command => Some(result),
                _ => None,
            })
            .expect("accepted history rows resolve");
        serde_json::from_str(result).unwrap()
    }
    fn partial(actions: &[Action], command: u32, count: usize) {
        let notices = markers(actions);
        assert_eq!(notices.len(), 1, "exactly one owned marker: {actions:?}");
        assert_eq!(notices[0].level, Level::Warn);
        assert!(
            matches!(notices[0].fields.as_slice(), [("command", Value::Num(id))] if *id == f64::from(command))
        );
        let marker = actions.iter().position(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code::HISTORY_INCOMPLETE)).unwrap();
        let done = actions.iter().position(|a| matches!(a, Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command)).unwrap();
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command)).count(), 1);
        assert!(marker < done, "coverage precedes success");
        let output = rows(actions, command);
        assert_eq!(output.len(), count);
        assert!(
            output.iter().all(|row| row["id"] != "999"),
            "failed prefix excluded"
        );
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
        )));
    }
    fn typed_failure(actions: &[Action], command: u32) {
        let failures: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                    Some(error)
                }
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].kind, Some(ErrorKind::Internal));
        assert!(markers(actions).is_empty());
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Notice(Notice::CommandDone { .. })
        )));
    }

    #[test]
    fn first_http_stream_and_decode_failures_reject_typed_without_success() {
        for mode in 0..3 {
            let (mut e, _) = ready();
            let r = query(&mut e, 71, 2);
            fail_page(&mut e, &r, mode);
            typed_failure(&e.drain_actions().collect::<Vec<_>>(), 71);
        }
    }

    #[test]
    fn later_failures_retain_only_validated_pages_at_error_level() {
        for mode in 0..3 {
            for streamed in [false, true] {
                let (mut e, _) = ready();
                e.set_log_level(Level::Error);
                let r = query(&mut e, 72, 2);
                respond(
                    &mut e,
                    &r,
                    &format!("[{},{}]", fill(1, SYMBOL), fill(2, "SOL_USDC")),
                    streamed,
                );
                let next = requests(&mut e).pop().unwrap();
                assert!(next.url.contains("offset=2"));
                fail_page(&mut e, &next, mode);
                partial(&e.drain_actions().collect::<Vec<_>>(), 72, 1);
                assert!(e.store().orders().is_empty());
            }
        }
    }

    #[test]
    fn filtered_pages_cannot_authorize_an_empty_partial_after_actual_failure() {
        let (mut e, _) = ready();
        let r = query(&mut e, 73, 1);
        respond(&mut e, &r, &format!("[{}]", fill(1, "SOL_USDC")), true);
        let next = requests(&mut e).pop().unwrap();
        fail_page(&mut e, &next, 1);
        typed_failure(&e.drain_actions().collect::<Vec<_>>(), 73);
    }

    #[test]
    fn repeated_valid_raw_page_is_owned_incomplete_even_with_no_eligible_rows() {
        let (mut e, _) = ready();
        e.set_log_level(Level::Error);
        let r = query(&mut e, 74, 1);
        let first = format!("[{}]", fill(1, "SOL_USDC"));
        respond(&mut e, &r, &first, false);
        let second = requests(&mut e).pop().unwrap();
        respond(&mut e, &second, &format!("[{}]", fill(2, "SOL_USDC")), true);
        let third = requests(&mut e).pop().unwrap();
        assert!(third.url.contains("offset=2"));
        respond(&mut e, &third, &first, false);
        partial(&e.drain_actions().collect::<Vec<_>>(), 74, 0);
    }

    #[test]
    fn skipped_fills_keep_valid_peers_and_report_an_owned_coverage_gap() {
        for all_skipped in [false, true] {
            let (mut e, _) = ready();
            let r = query(&mut e, 75, 10);
            let bad = fill(2, SYMBOL).replace("\"0.1\"", "null");
            let body = if all_skipped {
                format!("[{bad}]")
            } else {
                format!("[{},{}]", fill(1, SYMBOL), bad)
            };
            respond(&mut e, &r, &body, true);
            let actions: Vec<_> = e.drain_actions().collect();
            partial(&actions, 75, usize::from(!all_skipped));
            assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code::ROW_SKIPPED && event.message == "Skipped 1 malformed Backpack fill rows: 2")).count(), 1);
        }
    }

    #[test]
    fn fifty_page_cap_is_owned_and_a_fiftieth_empty_probe_is_complete() {
        for full_last in [false, true] {
            let (mut e, _) = ready();
            let mut r = query(&mut e, 76, 1);
            for i in 0..50 {
                if i > 0 {
                    assert!(r.url.contains(&format!("offset={i}&")));
                }
                let body = if i == 49 && !full_last {
                    "[]".into()
                } else {
                    format!("[{}]", fill(i + 1, "SOL_USDC"))
                };
                respond(&mut e, &r, &body, i % 2 == 0);
                if i < 49 {
                    r = requests(&mut e).pop().expect("next native offset");
                }
            }
            let actions: Vec<_> = e.drain_actions().collect();
            if full_last {
                partial(&actions, 76, 0);
            } else {
                assert!(rows(&actions, 76).is_empty());
                assert!(markers(&actions).is_empty());
                assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
            }
        }
    }

    #[test]
    fn concurrent_query_markers_never_attach_to_the_other_owner() {
        let (mut e, _) = ready();
        e.set_log_level(Level::Error);
        let first = query(&mut e, 77, 2);
        let other = query(&mut e, 78, 1);
        respond(
            &mut e,
            &first,
            &format!("[{},{}]", fill(1, SYMBOL), fill(2, "SOL_USDC")),
            true,
        );
        let next = requests(&mut e).pop().unwrap();
        fail_page(&mut e, &next, 1);
        partial(&e.drain_actions().collect::<Vec<_>>(), 77, 1);
        fail_page(&mut e, &other, 0);
        typed_failure(&e.drain_actions().collect::<Vec<_>>(), 78);
    }

    #[test]
    fn failed_skipped_prefix_cannot_replace_an_admitted_gap_or_diagnostic() {
        for admitted_skip in [false, true] {
            let (mut e, _) = ready();
            let first = query(&mut e, 79, 3);
            let middle = if admitted_skip {
                fill(2, SYMBOL).replace("\"0.1\"", "null")
            } else {
                fill(2, "SOL_USDC")
            };
            respond(
                &mut e,
                &first,
                &format!("[{},{middle},{}]", fill(1, SYMBOL), fill(3, "SOL_USDC")),
                false,
            );
            let next = requests(&mut e).pop().unwrap();
            let skipped = fill(90, SYMBOL).replace("\"0.1\"", "null");
            e.handle(
                2000.,
                Event::HttpStart {
                    request: next.id,
                    status: 200,
                    headers: "",
                },
            );
            let prefix = format!("[{skipped},{},", fill(999, SYMBOL));
            e.handle(
                2000.,
                Event::HttpChunk {
                    request: next.id,
                    data: prefix.as_bytes(),
                },
            );
            e.handle(
                2000.,
                Event::HttpFailed {
                    request: next.id,
                    error: "later transport loss",
                },
            );
            let actions: Vec<_> = e.drain_actions().collect();
            partial(&actions, 79, 1);
            let marker = markers(&actions)[0];
            if admitted_skip {
                assert_eq!(
                    marker.message,
                    "Backpack history is incomplete: malformed fill rows were skipped"
                );
            } else {
                assert!(marker.message.contains("later transport loss"));
            }
            let skipped: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::Event(event)) if event.code == code::ROW_SKIPPED => {
                        Some(event.message.as_str())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                skipped,
                if admitted_skip {
                    vec!["Skipped 1 malformed Backpack fill rows: 2"]
                } else {
                    vec![]
                }
            );
        }
    }

    #[test]
    fn malformed_nonfill_page_retains_prior_output_and_its_eager_diagnostic() {
        for streamed in [false, true] {
            let (mut e, _) = ready();
            ask(
                &mut e,
                Query::OrderHistory {
                    symbol: Some(SYMBOL.into()),
                    from_ms: Some(0.),
                    to_ms: None,
                    limit: Some(2),
                },
            );
            let first = requests(&mut e).pop().unwrap();
            assert!(first.url.contains("limit=1000"));
            let outside = ORDER.replace(SYMBOL, "UNLISTED_PERP");
            let mut page = vec![ORDER.to_owned()];
            page.extend(std::iter::repeat_n(outside, 999));
            respond(&mut e, &first, &format!("[{}]", page.join(",")), streamed);
            let next = requests(&mut e).pop().unwrap();
            assert!(next.url.contains("offset=1000"));
            let prefix = ORDER.replace("\"123\"", "\"999\"");
            let bad = ORDER.replace("Bid", "Unknown");
            respond(&mut e, &next, &format!("[{prefix},{bad}]"), streamed);
            let actions: Vec<_> = e.drain_actions().collect();
            partial(&actions, 1, 1);
            assert_eq!(rows(&actions, 1)[0]["id"], "123");
            assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandError { command: 1, message }) if message == "Backpack invalid order side")));
        }
    }

    #[test]
    fn offset_counts_filtered_and_skipped_neighbors_without_losing_valid_rows() {
        let (mut e, _) = ready();
        let first = query(&mut e, 80, 3);
        let bad = fill(2, SYMBOL).replace("\"0.1\"", "null");
        respond(
            &mut e,
            &first,
            &format!("[{},{bad},{}]", fill(1, "SOL_USDC"), fill(3, SYMBOL)),
            true,
        );
        let next = requests(&mut e).pop().unwrap();
        assert!(next.url.contains("offset=3"));
        for header in ["X-API-Key", "X-Broker-Id", "X-Window"] {
            assert_eq!(
                first.headers.iter().find(|(name, _)| *name == header),
                next.headers.iter().find(|(name, _)| *name == header)
            );
        }
        respond(&mut e, &next, &format!("[{}]", fill(4, SYMBOL)), false);
        let actions: Vec<_> = e.drain_actions().collect();
        partial(&actions, 80, 2);
        let output = rows(&actions, 80);
        assert_eq!(
            output
                .iter()
                .map(|row| row["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["3", "4"]
        );
    }

    #[test]
    fn public_trade_dedupe_limit_and_single_unsigned_request_are_preserved() {
        let (mut e, _) = ready();
        ask(
            &mut e,
            Query::PublicTrades {
                symbol: SYMBOL.into(),
                limit: Some(1),
            },
        );
        let request = requests(&mut e).pop().unwrap();
        assert!(
            !request
                .headers
                .iter()
                .any(|(name, _)| *name == "X-Signature")
        );
        let trade = r#"{"id":7,"isBuyerMaker":true,"price":"104.12","quantity":"0.1","quoteQuantity":"10.412","timestamp":1792425943313}"#;
        respond(
            &mut e,
            &request,
            &format!(
                "[{trade},{trade},{}]",
                trade.replace("\"id\":7", "\"id\":8")
            ),
            true,
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(rows(&actions, 1)[0]["id"], "7");
        assert_eq!(rows(&actions, 1).len(), 1);
        assert!(markers(&actions).is_empty());
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    }

    #[test]
    fn synchronous_setup_refusal_stays_strict_without_http_or_incomplete_success() {
        let mut e = safe_cex_engine::test_support::seeded_engine(
            safe_cex_venue_backpack::Backpack::new(safe_cex_venue_backpack::Options {
                public_data_only: true,
                ..Default::default()
            }),
        );
        e.start(1000.);
        e.drain_actions().for_each(drop);
        e.handle(
            2000.,
            Event::Command(Command::Query {
                command: 81,
                query: Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: Some(2),
                },
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 81, error }) if error.message == "Backpack credentials unavailable")).count(), 1);
        assert!(markers(&actions).is_empty());
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Notice(Notice::CommandDone { .. })
        )));
    }
}
