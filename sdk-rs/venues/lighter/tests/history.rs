//! Oracles: apidocs.lighter.xyz/reference/{trades,accountinactiveorders,positionfunding}.md.
//! FeeTick: github.com/elliottech/lighter-go/blob/main/types/txtypes/constants.go.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Query};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::*;

fn query(e: &mut Engine<Lighter>, read: Query) -> HttpRequest {
    e.handle_command(NOW, Command::Query { command: 91, query: read }, Some(120_000)).unwrap();
    safe_cex_engine::test_support::prepared_actions(e, NOW).into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .expect("history read")
}
fn result(e: &mut Engine<Lighter>) -> Result<Value, String> {
    safe_cex_engine::test_support::prepared_actions(e, NOW).into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 91,
                result,
            }) => Some(Ok(serde_json::from_str(&result).unwrap())),
            Action::Notice(Notice::CommandFailed { command: 91, error }) => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .expect("history completion")
}
fn fills(limit: Option<u32>) -> Query {
    Query::Fills {
        symbol: Some("ETH".into()),
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit,
    }
}
fn trade(id: &str, timestamp: u64) -> Value {
    json!({"trade_id_str":id,"trade_id":99,"market_id":1,"price":"100","size":"0.5","usd_amount":"50","timestamp":timestamp,"ask_account_id":718895,"bid_account_id":2,"ask_id_str":"9007199254740993","ask_id":9,"bid_id":8,"is_maker_ask":true,"maker_fee":-10,"taker_fee":196,"integrator_maker_fee":5,"integrator_taker_fee":250,"ask_account_pnl":"2.5","bid_account_pnl":"-1"})
}
fn answer(e: &mut Engine<Lighter>, r: &HttpRequest, rows: Vec<Value>, cursor: Option<&str>) {
    let body = json!({"code":200,"trades":rows,"next_cursor":cursor});
    reply(e, r, 200, &body.to_string());
}

fn fail_page(e: &mut Engine<Lighter>, r: &HttpRequest, mode: u8) {
    match mode {
        0 => e.handle(
            NOW,
            Event::HttpFailed {
                request: r.id,
                error: "lost read",
            },
        ),
        1 => reply(
            e,
            r,
            503,
            r#"{"code":503,"message":"unavailable","trades":[]}"#,
        ),
        2 => reply(
            e,
            r,
            200,
            r#"{"code":21120,"message":"native failure","trades":[]}"#,
        ),
        3 => reply(
            e,
            r,
            200,
            &json!({"code":200,"trades":[trade("failed-prefix", NOW as u64),{"trade_id":"bad"}]})
                .to_string(),
        ),
        4 => reply(
            e,
            r,
            200,
            &format!(
                r#"{{"code":200,"trades":[{},"#,
                trade("failed-prefix", NOW as u64)
            ),
        ),
        5 => reply(e, r, 200, r#"{"code":200}"#),
        6 => {
            let mut foreign = trade("foreign", NOW as u64);
            foreign["ask_account_id"] = json!(3);
            reply(
                e,
                r,
                200,
                &json!({"code":200,"trades":[trade("failed-prefix",NOW as u64),foreign]})
                    .to_string(),
            );
        }
        7 => reply(e, r, 200, r#"{"code":200,"trades":null}"#),
        8 => reply(e, r, 200, r#"{"code":200,"trades":{}}"#),
        _ => unreachable!(),
    }
}
fn typed_failure(actions: &[Action], not_sent: bool) {
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1, "one typed terminal error");
    assert_eq!(failures[0].kind, Some(safe_cex_engine::ErrorKind::Internal));
    if not_sent {
        assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
    }
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 91, .. })))
    );
    assert!(!actions.iter().any(|a| matches!(a,Action::Notice(Notice::Event(v)) if v.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
}
fn owned_partial(actions: &[Action], count: usize) -> Value {
    let markers: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::Notice(Notice::Event(v))
                if v.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some((i, v))
            }
            _ => None,
        })
        .collect();
    assert_eq!(markers.len(), 1, "exactly one completeness marker");
    assert_eq!(markers[0].1.level, safe_cex_engine::Level::Warn);
    assert_eq!(
        markers[0].1.fields,
        vec![("command", safe_cex_engine::Value::Num(91.0))]
    );
    let completed: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::Notice(Notice::CommandDone {
                command: 91,
                result,
            }) => Some((i, result)),
            _ => None,
        })
        .collect();
    assert_eq!(completed.len(), 1);
    assert!(markers[0].0 < completed[0].0);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 91, .. })))
    );
    let rows: Value = serde_json::from_str(completed[0].1).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), count);
    assert!(!rows.to_string().contains("failed-prefix"));
    rows
}
fn all_income(limit: Option<u32>) -> Query {
    Query::Income {
        symbol: Some("ETH".into()),
        kind: None,
        from_ms: None,
        to_ms: None,
        limit,
    }
}

#[test]
fn owned_first_page_failure_is_typed_and_never_complete() {
    for mode in 0..9 {
        let (mut e, _) = ready();
        let r = query(&mut e, fills(None));
        fail_page(&mut e, &r, mode);
        typed_failure(&safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>(), false);
    }
}
#[test]
fn owned_later_failure_returns_only_accepted_rows_and_its_command_marker() {
    for mode in 0..9 {
        let (mut e, _) = ready();
        e.set_log_level(safe_cex_engine::Level::Error);
        let r = query(&mut e, fills(None));
        answer(&mut e, &r, vec![trade("accepted", NOW as u64)], Some("p2"));
        let next = requests(&mut e).remove(0);
        fail_page(&mut e, &next, mode);
        let rows = owned_partial(&safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>(), 1);
        assert_eq!(rows[0]["id"], "accepted");
        assert_eq!(e.store().orders().len(), 1);
    }
}
#[test]
fn owned_empty_or_filtered_pages_do_not_authorize_partial_success() {
    for filtered in [false, true] {
        let (mut e, _) = ready();
        let r = query(&mut e, fills(None));
        let mut other = trade("filtered", NOW as u64);
        other["market_id"] = json!(2);
        answer(
            &mut e,
            &r,
            if filtered { vec![other] } else { vec![] },
            Some("p2"),
        );
        let next = requests(&mut e).remove(0);
        fail_page(&mut e, &next, 0);
        typed_failure(&safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>(), false);
    }
}
#[test]
fn owned_income_preserves_trade_and_prior_funding_stages_on_failure() {
    for funding_page in [false, true] {
        let (mut e, _) = ready();
        e.set_log_level(safe_cex_engine::Level::Error);
        let r = query(&mut e, all_income(None));
        answer(&mut e, &r, vec![trade("1", NOW as u64)], None);
        let mut r = requests(&mut e).remove(0);
        assert!(r.url.contains("positionFunding"));
        if funding_page {
            reply(&mut e,&r,200,&json!({"code":200,"position_fundings":[{"funding_id":"7","market_id":1,"timestamp":NOW/1000.0,"change":"-0.25"}],"next_cursor":"f2"}).to_string());
            r = requests(&mut e).remove(0);
        }
        fail_page(&mut e, &r, 0);
        let rows = owned_partial(
            &safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>(),
            if funding_page { 3 } else { 2 },
        );
        assert!(
            rows.as_array()
                .unwrap()
                .iter()
                .any(|v| v["tranId"] == "pnl:1")
        );
    }
}
#[test]
fn income_stage_limits_are_independent_and_funding_is_still_requested() {
    let (mut e, _) = ready();
    let r = query(&mut e, all_income(Some(2)));
    answer(
        &mut e,
        &r,
        vec![trade("1", NOW as u64)],
        Some("must-not-read"),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("positionFunding"));
    assert!(!r.url.contains("cursor="));
    reply(&mut e,&r,200,&json!({"code":200,"position_fundings":[{"funding_id":"7","market_id":1,"timestamp":NOW/1000.0+1.0,"change":"-0.25"}],"next_cursor":"f2"}).to_string());
    let r = requests(&mut e).remove(0);
    assert!(r.url.contains("cursor=f2"));
    reply(&mut e,&r,200,&json!({"code":200,"position_fundings":[{"funding_id":"8","market_id":1,"timestamp":NOW/1000.0+2.0,"change":"-0.5"}],"next_cursor":"must-not-read"}).to_string());
    let rows = result(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["tranId"], "8");
    assert_eq!(rows[1]["tranId"], "7");
}

#[test]
fn owned_malformed_funding_prefix_cannot_enter_an_accepted_prior_stage() {
    let (mut e, _) = ready();
    let r = query(&mut e, all_income(None));
    answer(&mut e, &r, vec![trade("1", NOW as u64)], None);
    let r = requests(&mut e).remove(0);
    reply(&mut e, &r, 200, &json!({"code":200,"position_fundings":[{"funding_id":"failed-prefix","market_id":1,"timestamp":NOW/1000.0,"change":"-0.25"},{"funding_id":"bad"}]}).to_string());
    owned_partial(&safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>(), 2);
}

#[test]
fn owned_order_and_public_trade_pages_share_the_completion_contract() {
    for public in [false, true] {
        for later in [false, true] {
            let (mut e, _) = ready();
            e.set_log_level(safe_cex_engine::Level::Error);
            let read = if public {
                Query::PublicTrades {
                    symbol: "ETH".into(),
                    limit: None,
                }
            } else {
                Query::OrderHistory {
                    symbol: Some("ETH".into()),
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                }
            };
            let mut r = query(&mut e, read);
            if later {
                if public {
                    answer(&mut e, &r, vec![trade("1", NOW as u64)], Some("p2"));
                } else {
                    reply(&mut e,&r,200,&json!({"code":200,"orders":[{"order_index":"45","market_id":1,"type":"limit","status":"filled","is_ask":false,"initial_base_amount":"2","remaining_base_amount":"0","filled_base_amount":"2","price":"100","created_at":NOW-500.0,"updated_at":NOW}],"next_cursor":"p2"}).to_string());
                }
                r = requests(&mut e).remove(0);
            }
            fail_page(&mut e, &r, 0);
            let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>();
            if later {
                owned_partial(&actions, 1);
            } else {
                typed_failure(&actions, false);
            }
        }
    }
}
#[test]
fn owned_partial_does_not_complete_or_mark_a_neighbour() {
    let (mut e, _) = ready();
    e.set_log_level(safe_cex_engine::Level::Error);
    let r = query(&mut e, fills(None));
    answer(&mut e, &r, vec![trade("1", NOW as u64)], Some("p2"));
    let r = requests(&mut e).remove(0);
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 92,
            query: fills(None),
        }),
    );
    let neighbour = requests(&mut e).remove(0);
    fail_page(&mut e, &r, 0);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>();
    owned_partial(&actions, 1);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { command: 92, .. } | Notice::CommandFailed { command: 92, .. }
        )
    )));
    answer(&mut e, &neighbour, vec![], None);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>();
    assert!(actions.iter().any(
        |a| matches!(a,Action::Notice(Notice::CommandDone{command:92,result}) if result=="[]")
    ));
    assert!(!actions.iter().any(|a| matches!(a,Action::Notice(Notice::Event(v)) if v.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
}
#[test]
fn owned_auth_preparation_refusal_is_strict_after_accepted_rows() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let refused = Arc::new(AtomicBool::new(false));
    let control = refused.clone();
    let mut e = support::engine(options());
    e.set_signing_entropy(Box::new(move |bytes| {
        if control.load(Ordering::SeqCst) {
            Err("signing source unavailable".into())
        } else {
            bytes.fill(7);
            Ok(())
        }
    }))
    .unwrap();
    let (mut e, _, _) = ready_engine_meta(e, META);
    let r = query(&mut e, fills(None));
    refused.store(true, Ordering::SeqCst);
    answer(&mut e, &r, vec![trade("accepted", NOW as u64)], Some("p2"));
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>();
    typed_failure(&actions, true);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn fills_are_executions_with_exact_string_ids_and_signed_fee_rate_conversion() {
    let (mut e, _) = ready();
    let r = query(&mut e, fills(None));
    assert!(
        r.url.contains("/trades?")
            && r.url.contains("market_type=perp")
            && r.url.contains("aggregate=false")
    );
    assert!(!r.url.contains("auth="));
    assert!(r.headers.iter().any(|(k, _)| *k == "authorization"));
    answer(
        &mut e,
        &r,
        vec![trade("18446744073709551610", NOW as u64)],
        None,
    );
    let rows = result(&mut e).unwrap();
    assert_eq!(rows[0]["id"], "18446744073709551610");
    assert_eq!(rows[0]["orderId"], "9007199254740993");
    assert_eq!(rows[0]["side"], "sell");
    assert_eq!(rows[0]["maker"], true);
    assert_eq!(rows[0]["qty"], 0.5);
    assert_eq!(rows[0]["realizedPnl"], 2.5);
    assert!((rows[0]["commission"].as_f64().unwrap() + 0.00025).abs() < 1e-12);
    assert_eq!(rows[0]["timestamp"], NOW / 1000.0);
}

#[test]
fn distinct_same_millisecond_executions_survive_dedup() {
    let (mut e, _) = ready();
    let r = query(&mut e, fills(None));
    let row = trade("1", NOW as u64);
    answer(
        &mut e,
        &r,
        vec![row.clone(), trade("2", NOW as u64), row],
        None,
    );
    assert_eq!(result(&mut e).unwrap().as_array().unwrap().len(), 2);
}

#[test]
fn native_order_filter_uses_lossless_venue_parameter_and_exact_match() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: Some("9007199254740993".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    assert!(r.url.contains("order_index_str=9007199254740993"));
    assert!(!r.url.contains("market_id="));
    answer(&mut e, &r, vec![trade("1", NOW as u64)], None);
    assert_eq!(result(&mut e).unwrap()[0]["orderId"], "9007199254740993");
}

#[test]
fn microsecond_execution_time_filters_before_serializing_seconds() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::Fills {
            symbol: Some("ETH".into()),
            order_id: None,
            from_ms: Some(NOW + 123.4),
            to_ms: Some(NOW + 123.5),
            limit: None,
        },
    );
    let mut first = trade("1", NOW as u64);
    first["transaction_time"] = json!(1_700_000_000_123_456_u64);
    let mut second = first.clone();
    second["trade_id_str"] = json!("2");
    second["transaction_time"] = json!(1_700_000_000_123_600_u64);
    answer(&mut e, &r, vec![first, second], None);
    let rows = result(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert!((rows[0]["timestamp"].as_f64().unwrap() - (NOW + 123.456) / 1000.0).abs() < 1e-6);
}

#[test]
fn public_trades_use_aggressor_side_and_do_not_require_auth() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::PublicTrades {
            symbol: "ETH".into(),
            limit: Some(10),
        },
    );
    assert!(!r.url.contains("account_index="));
    assert!(r.headers.is_empty());
    answer(&mut e, &r, vec![trade("1", NOW as u64)], None);
    let rows = result(&mut e).unwrap();
    assert_eq!(rows[0]["side"], "buy");
    assert_eq!(rows[0]["quoteQty"], 50.0);
    assert_eq!(rows[0]["timestamp"], NOW);
}

#[test]
fn cursor_pages_dedup_by_execution_id_and_escape_cursor() {
    let (mut e, _) = ready();
    let r = query(&mut e, fills(Some(3)));
    answer(&mut e, &r, vec![trade("1", NOW as u64)], Some("a+b/="));
    let next = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("/trades?"))
        .unwrap();
    assert!(next.url.contains("cursor=a%2Bb%2F%3D"));
    answer(
        &mut e,
        &next,
        vec![
            trade("1", NOW as u64),
            trade("2", NOW as u64 - 1),
            trade("3", NOW as u64 - 2),
        ],
        None,
    );
    assert_eq!(result(&mut e).unwrap().as_array().unwrap().len(), 3);
}

#[test]
fn repeated_cursor_stays_strict_after_accepted_rows() {
    let (mut e, _) = ready();
    let r = query(&mut e, fills(Some(3)));
    answer(&mut e, &r, vec![trade("1", NOW as u64)], Some("same"));
    let next = requests(&mut e).into_iter().next().unwrap();
    answer(&mut e, &next, vec![trade("2", NOW as u64)], Some("same"));
    typed_failure(&safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>(), false);
    assert_eq!(e.store().orders().len(), 1);
}

#[test]
fn saturated_page_without_cursor_fails_if_requested_history_is_incomplete() {
    let (mut e, _) = ready();
    let r = query(&mut e, fills(Some(200)));
    answer(
        &mut e,
        &r,
        (0..100)
            .map(|i| trade(&i.to_string(), NOW as u64))
            .collect(),
        None,
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>();
    typed_failure(&actions, false);
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.message.contains("saturated"))));
}

#[test]
fn malformed_or_wrong_principal_rows_fail_the_complete_read() {
    for malformed in [true, false] {
        let (mut e, _) = ready();
        let r = query(&mut e, fills(None));
        let mut row = trade("1", NOW as u64);
        if malformed {
            row["size"] = json!("not-a-number");
        } else {
            row["ask_account_id"] = json!(1);
            row["bid_account_id"] = json!(2);
        }
        answer(&mut e, &r, vec![row], None);
        assert!(result(&mut e).is_err());
    }
}

#[test]
fn historical_orders_use_native_id_status_average_and_millisecond_times() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("ETH".into()),
            from_ms: Some(NOW - 1000.0),
            to_ms: Some(NOW + 1000.0),
            limit: Some(100),
        },
    );
    assert!(r.url.contains("accountInactiveOrders"));
    reply(&mut e,&r,200,&json!({"code":200,"orders":[{"order_index":"9007199254740993","market_id":1,"type":"limit","status":"filled","is_ask":false,"initial_base_amount":"2","filled_base_amount":"2","remaining_base_amount":"0","filled_quote_amount":"210","price":"100","created_at":NOW-500.0,"updated_at":NOW}]}).to_string());
    let rows = result(&mut e).unwrap();
    assert_eq!(rows[0]["id"], "9007199254740993");
    assert_eq!(rows[0]["averagePrice"], 105.0);
    assert_eq!(rows[0]["status"], "closed");
    assert_eq!(rows[0]["createdTime"], NOW - 500.0);
    assert_eq!(rows[0]["updatedTime"], NOW);
}

#[test]
fn funding_ledger_preserves_native_signed_change_and_filters_times() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::Income {
            symbol: Some("ETH".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: Some(NOW - 1000.0),
            to_ms: Some(NOW),
            limit: None,
        },
    );
    assert!(
        r.url.contains("positionFunding")
            && r.url.contains("market_ids=1")
            && r.url.contains("start_timestamp=1699999999")
    );
    reply(&mut e,&r,200,&json!({"code":200,"position_fundings":[{"funding_id":"7","market_id":1,"timestamp":NOW/1000.0,"change":"-0.25"},{"funding_id":"8","market_id":1,"timestamp":NOW/1000.0-10.0,"change":"5"}]}).to_string());
    let rows = result(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["income"], -0.25);
    assert_eq!(rows[0]["category"], "funding");
    assert_eq!(rows[0]["tranId"], "7");
}

#[test]
fn all_income_merges_realized_pnl_signed_rebate_and_funding() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    answer(&mut e, &r, vec![trade("1", NOW as u64)], None);
    let funding = requests(&mut e)
        .into_iter()
        .find(|r| r.url.contains("positionFunding"))
        .unwrap();
    reply(&mut e,&funding,200,&json!({"code":200,"position_fundings":[{"funding_id":"7","market_id":1,"timestamp":NOW/1000.0,"change":"-0.25"}]}).to_string());
    let rows = result(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 3);
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["category"] == "realized_pnl" && r["income"] == 2.5)
    );
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["category"] == "rebate" && r["income"].as_f64().unwrap() > 0.0)
    );
}

#[test]
fn caller_id_lookup_and_invalid_ranges_fail_before_http() {
    let (mut e, _) = ready();
    for read in [
        Query::OrderByClientId {
            symbol: "ETH".into(),
            client_order_id: "123".into(),
        },
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(NOW + 1.0),
            to_ms: Some(NOW),
            limit: None,
        },
    ] {
        e.handle(
            NOW,
            Event::Command(Command::Query {
                command: 91,
                query: read,
            }),
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}

#[test]
fn a_successful_http_error_code_does_not_return_empty_history() {
    let (mut e, _) = ready();
    let r = query(&mut e, fills(None));
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":21120,"message":"invalid signature","trades":[]}"#,
    );
    assert!(result(&mut e).unwrap_err().contains("invalid signature"));
}

#[test]
fn commission_and_rebate_kind_filters_do_not_mix_ledger_categories() {
    for (kind, expected) in [
        ("COMMISSION", "commission"),
        ("COMMISSION_REBATE", "rebate"),
    ] {
        let (mut e, _) = ready();
        let r = query(
            &mut e,
            Query::Income {
                symbol: Some("ETH".into()),
                kind: Some(kind.into()),
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        );
        let mut paid = trade("paid", NOW as u64);
        paid["maker_fee"] = json!(10);
        answer(&mut e, &r, vec![paid, trade("rebate", NOW as u64)], None);
        let rows = result(&mut e).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["category"], expected);
    }
}

#[test]
fn historical_orders_share_native_hyphenated_conditional_and_terminal_mapping() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("ETH".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    reply(&mut e, &r, 200, &json!({"code":200,"orders":[{"order_index":"45","market_id":1,"type":"stop-loss-limit","time_in_force":"post-only","status":"canceled-post-only","is_ask":true,"initial_base_amount":"2","remaining_base_amount":"2","price":"89","trigger_price":"90","created_at":NOW-1000.0,"updated_at":NOW}]}).to_string());
    let rows = result(&mut e).unwrap();
    assert_eq!(rows[0]["type"], "stop_market");
    assert_eq!(rows[0]["status"], "canceled");
    assert_eq!(rows[0]["price"], 90.0);
}

#[test]
fn windowed_fills_stop_paging_once_rows_are_older_than_the_window() {
    let (mut e, _) = ready();
    let r = query(
        &mut e,
        Query::Fills {
            symbol: Some("ETH".into()),
            order_id: None,
            from_ms: Some(NOW - 1_000.0),
            to_ms: Some(NOW),
            limit: Some(200),
        },
    );
    let mut rows = vec![trade("in", NOW as u64 - 500)];
    rows.extend((0..99).map(|i| trade(&format!("old{i}"), NOW as u64 - 5_000)));
    answer(&mut e, &r, rows, Some("next-page"));
    let rows = result(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "in");
}

/// Pins the exact request sequence of a paged read: trades pages by cursor,
/// then the funding stage from a fresh cursor. Paging refactors keep it.
#[test]
fn paged_income_request_sequence_is_pinned() {
    let (mut e, _) = ready();
    let mut urls = Vec::new();
    let r = query(
        &mut e,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    urls.push(r.url.clone());
    answer(&mut e, &r, vec![trade("1", NOW as u64)], Some("t1"));
    let r = requests(&mut e).into_iter().next().unwrap();
    urls.push(r.url.clone());
    answer(&mut e, &r, vec![trade("2", NOW as u64 - 1)], None);
    let r = requests(&mut e).into_iter().next().unwrap();
    urls.push(r.url.clone());
    reply(&mut e, &r, 200, &json!({"code":200,"position_fundings":[{"funding_id":"7","market_id":1,"timestamp":NOW/1000.0,"change":"-0.25"}],"next_cursor":"f1"}).to_string());
    let r = requests(&mut e).into_iter().next().unwrap();
    urls.push(r.url.clone());
    reply(
        &mut e,
        &r,
        200,
        &json!({"code":200,"position_fundings":[]}).to_string(),
    );
    assert!(result(&mut e).is_ok());
    let host = urls[0].split("/api/").next().unwrap().to_owned();
    let paths: Vec<_> = urls
        .iter()
        .map(|u| u.trim_start_matches(&host).to_owned())
        .collect();
    assert_eq!(
        paths,
        [
            "/api/v1/trades?limit=100&account_index=718895&market_type=perp&sort_by=timestamp&sort_dir=desc&aggregate=false",
            "/api/v1/trades?limit=100&account_index=718895&market_type=perp&sort_by=timestamp&sort_dir=desc&aggregate=false&cursor=t1",
            "/api/v1/positionFunding?limit=100&account_index=718895",
            "/api/v1/positionFunding?limit=100&account_index=718895&cursor=f1",
        ]
    );
}
