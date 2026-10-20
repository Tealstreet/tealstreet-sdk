//! Oracles: apidocs.lighter.xyz/reference/{trades,accountinactiveorders,positionfunding}.md.
//! FeeTick: github.com/elliottech/lighter-go/blob/main/types/txtypes/constants.go.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Query};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::*;

fn query(e: &mut Engine<Lighter>, read: Query) -> HttpRequest {
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 91,
            query: read,
        }),
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .expect("history read")
}
fn result(e: &mut Engine<Lighter>) -> Result<Value, String> {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 91,
                result,
            }) => Some(result.map(|s| serde_json::from_str(&s).unwrap())),
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
fn repeated_cursor_and_later_page_failure_never_return_partial_history() {
    for transport in [false, true] {
        let (mut e, _) = ready();
        let r = query(&mut e, fills(Some(3)));
        answer(&mut e, &r, vec![trade("1", NOW as u64)], Some("same"));
        let next = requests(&mut e).into_iter().next().unwrap();
        if transport {
            e.handle(
                NOW,
                Event::HttpFailed {
                    request: next.id,
                    error: "lost read",
                },
            );
        } else {
            answer(&mut e, &next, vec![trade("2", NOW as u64)], Some("same"));
        }
        assert!(result(&mut e).is_err());
        assert_eq!(e.store().orders().len(), 1);
    }
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
    assert!(result(&mut e).unwrap_err().contains("saturated"));
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
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { result: Err(_), .. })
        )));
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
