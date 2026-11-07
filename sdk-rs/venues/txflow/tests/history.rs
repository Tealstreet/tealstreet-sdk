//! Current TxFlow indexed history payloads and continuation semantics.
#[path = "support/account.rs"]
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query};
use safe_cex_venue_txflow::TxFlow;
use serde_json::{Value, json};
use support::*;
fn query(e: &mut Engine<TxFlow>, q: Query) -> safe_cex_engine::HttpRequest {
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 91,
            query: q,
        }),
    );
    requests(e)
        .into_iter()
        .find(|r| {
            r.body
                .as_ref()
                .is_some_and(|b| b.contains("\"limit\":2000"))
        })
        .unwrap()
}
fn ready() -> Engine<TxFlow> {
    let (mut e, _, reads, _) = bootstrap();
    answer(&mut e, reads);
    e.drain_actions().for_each(drop);
    e
}
fn fills(from: Option<f64>, limit: Option<u32>) -> Query {
    Query::Fills {
        symbol: Some("BTC-USDC".into()),
        order_id: None,
        from_ms: from,
        to_ms: Some(NOW + 10000.0),
        limit,
    }
}
fn execution(tid: u64, time: f64) -> Value {
    json!({"instrumentId":1,"tid":tid,"oid":42,"time":time as u64,"px":"101.5","sz":"0.5","side":"B","fee":"0.02","feeToken":"USDC","closedPnl":"3","crossed":true})
}
fn done(e: &mut Engine<TxFlow>) -> Result<Value, String> {
    e.drain_actions()
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
        .unwrap()
}
#[test]
fn exact_millisecond_filtering_precedes_seconds_output_and_native_tid_dedup() {
    let mut e = ready();
    let r = query(&mut e, fills(Some(NOW + 1.0), None));
    let body=json!({"success":true,"code":0,"data":{"data":[execution(7,NOW),execution(8,NOW+1.0),execution(8,NOW+1.0)],"hasMore":false}}).to_string();
    reply(&mut e, &r, &body);
    let rows = done(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "8");
    assert_eq!(rows[0]["timestamp"], (NOW + 1.0) / 1000.0);
    assert_eq!(rows[0]["commission"], 0.02);
    assert_eq!(rows[0]["realizedPnl"], 3.0);
    assert_eq!(rows[0]["maker"], false);
}
#[test]
fn offset_continuation_uses_native_raw_pages_not_requested_output_limit() {
    let mut e = ready();
    let r = query(&mut e, fills(None, Some(1)));
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[execution(1,NOW)],"hasMore":true}})
            .to_string(),
    );
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(r.body.as_ref().unwrap()).unwrap()["offset"],
        2000
    );
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[execution(2,NOW+1.0)],"hasMore":false}})
            .to_string(),
    );
    assert_eq!(done(&mut e).unwrap()[0]["id"], "2");
}
#[test]
fn failed_or_malformed_page_never_returns_successful_partial_history() {
    for body in [
        r#"{"success":false,"code":9,"message":"denied","data":{"data":[],"hasMore":false}}"#,
        r#"{"success":true,"code":0,"data":{"data":[{"px":"NaN"}],"hasMore":false}}"#,
        r#"{"success":true,"code":0,"data":{"data":[],"hasMore":true}}"#,
    ] {
        let mut e = ready();
        let r = query(&mut e, fills(None, None));
        reply(&mut e, &r, body);
        assert!(done(&mut e).is_err());
    }
}
#[test]
fn repeated_full_continuation_is_an_explicit_incomplete_read() {
    let mut e = ready();
    let r = query(&mut e, fills(None, None));
    let body = json!({"success":true,"code":0,"data":{"data":[execution(1,NOW)],"hasMore":true}})
        .to_string();
    reply(&mut e, &r, &body);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, &body);
    assert!(done(&mut e).unwrap_err().contains("Incomplete"));
}
#[test]
fn income_filter_preserves_signed_native_funding() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::Income {
            symbol: Some("BTC-USDC".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    assert!(r.body.as_ref().unwrap().contains("userFundings"));
    reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[{"time":NOW as u64,"coin":"BTC","amount":"-0.12","szi":"2","fundingRate":"0.01","usdc":"-0.12"}],"hasMore":false}}).to_string());
    let rows = done(&mut e).unwrap();
    assert_eq!(rows[0]["income"], -0.12);
    assert_eq!(rows[0]["category"], "funding");
}
#[test]
fn order_history_keeps_native_total_and_remaining_distinct() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[{"order":{"instrumentId":1,"oid":42,"timestamp":NOW as u64,"side":"A","orderType":"Limit","limitPx":"101","origSz":"2","filledAmount":"0.5","reduceOnly":false},"status":"Canceled","statusTimestamp":(NOW+1000.0) as u64}],"hasMore":false}}).to_string());
    let rows = done(&mut e).unwrap();
    assert_eq!(rows[0]["amount"], 2.0);
    assert_eq!(rows[0]["filled"], 0.5);
    assert_eq!(rows[0]["status"], "canceled");
}

fn ready_with_retired(relisted: bool) -> Engine<TxFlow> {
    let mut e =
        safe_cex_engine::test_support::seeded_engine(TxFlow::new(safe_cex_venue_txflow::Options {
            testnet: true,
            application_id: Some(WALLET.into()),
            secret: Some(format!("0x{:064x}", 1)),
            ..Default::default()
        }));
    e.start(NOW);
    let r = requests(&mut e).pop().unwrap();
    let mut meta: Value = serde_json::from_str(META).unwrap();
    let mut old = meta["data"]["universe"][0].clone();
    old["index"] = 9.into();
    old["delisted"] = true.into();
    old["haltTrading"] = true.into();
    old["basePrecision"] = "0".into();
    old["priceTick"] = "0".into();
    old["maxLeverage"] = 0.into();
    if !relisted {
        old["name"] = "OLD-USDC".into();
        old["baseCurrency"] = "OLD".into();
    }
    meta["data"]["universe"].as_array_mut().unwrap().push(old);
    reply(&mut e, &r, &meta.to_string());
    let r = requests(&mut e).pop().unwrap();
    assert!(r.body.as_ref().unwrap().contains("\"coin\":\"1\""));
    reply(
        &mut e,
        &r,
        r#"{"success":true,"code":0,"data":{"instrumentId":1,"lastPrice":"100","markPx":"100","oraclePx":"100"}}"#,
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
    e.handle(NOW + 25.0, Event::TimerFired { timer });
    let reads = requests(&mut e);
    answer(&mut e, reads);
    e.drain_actions().for_each(drop);
    assert_eq!(e.store().markets().len(), 1);
    e
}
#[test]
fn accountwide_and_filtered_history_resolve_delisted_metadata_without_tradable_market() {
    let mut e = ready_with_retired(false);
    let r = query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let mut old = execution(99, NOW);
    old["instrumentId"] = 9.into();
    reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[execution(1,NOW),old.clone()],"hasMore":false}}).to_string());
    let rows = done(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["symbol"] == "OLD-USDC")
    );
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: Some("OLD-USDC".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    assert!(r.body.as_ref().unwrap().contains("\"coin\":\"9\""));
    reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[{"order":{"instrumentId":9,"oid":99,"timestamp":NOW as u64,"side":"B","orderType":"Limit","limitPx":"100","origSz":"1","filledAmount":"1","reduceOnly":false},"status":"Filled","statusTimestamp":NOW as u64}],"hasMore":false}}).to_string());
    assert_eq!(done(&mut e).unwrap()[0]["symbol"], "OLD-USDC");
    let r = query(
        &mut e,
        Query::Income {
            symbol: Some("OLD-USDC".into()),
            kind: Some("REALIZED_PNL".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[old],"hasMore":false}}).to_string(),
    );
    assert_eq!(done(&mut e).unwrap()[0]["symbol"], "OLD-USDC");
    let r = query(
        &mut e,
        Query::Income {
            symbol: Some("OLD-USDC".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[{"coin":"OLD","time":NOW as u64,"amount":"-0.12","szi":"2","fundingRate":"0.01","usdc":"-0.12"}],"hasMore":false}}).to_string());
    assert_eq!(done(&mut e).unwrap()[0]["symbol"], "OLD-USDC");
}
#[test]
fn relisted_name_keeps_accountwide_identities_and_rejects_ambiguous_symbol_filter() {
    let mut e = ready_with_retired(true);
    let r = query(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let mut old = execution(99, NOW);
    old["instrumentId"] = 9.into();
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[old],"hasMore":false}}).to_string(),
    );
    assert_eq!(done(&mut e).unwrap()[0]["symbol"], "BTC-USDC");
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 91,
            query: fills(None, None),
        }),
    );
    assert!(done(&mut e).unwrap_err().contains("ambiguous"));
}
#[test]
fn genuinely_unknown_historical_instrument_still_fails_instead_of_fabricating_symbol() {
    let mut e = ready_with_retired(false);
    let r = query(&mut e, fills(None, None));
    let mut unknown = execution(99, NOW);
    unknown["instrumentId"] = 999.into();
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[unknown],"hasMore":false}}).to_string(),
    );
    assert!(done(&mut e).is_err());
}

/// Pins the exact request bodies of an offset-paged read. Paging refactors
/// keep them.
#[test]
fn offset_paged_request_sequence_is_pinned() {
    let mut e = ready();
    let mut sent = Vec::new();
    let r = query(&mut e, fills(None, None));
    sent.push(r.body.clone().unwrap());
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[execution(1,NOW)],"hasMore":true}})
            .to_string(),
    );
    let r = requests(&mut e).pop().unwrap();
    sent.push(r.body.clone().unwrap());
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[execution(2,NOW)],"hasMore":false}})
            .to_string(),
    );
    assert!(done(&mut e).is_ok());
    assert_eq!(
        sent,
        [
            "{\"type\":\"userFills\",\"user\":\"0x0000000000000000000000000000000000000001\",\"accountType\":[\"perp\"],\"coin\":\"1\",\"endTimeMs\":1700000010000,\"offset\":0,\"limit\":2000}",
            "{\"type\":\"userFills\",\"user\":\"0x0000000000000000000000000000000000000001\",\"accountType\":[\"perp\"],\"coin\":\"1\",\"endTimeMs\":1700000010000,\"offset\":2000,\"limit\":2000}",
        ]
    );
}

fn owned_terminal(
    e: &mut Engine<TxFlow>,
    command: u32,
    partial: bool,
) -> Result<Value, safe_cex_engine::VenueError> {
    let out: Vec<_> = e.drain_actions().collect();
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    let markers: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some(event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        markers.len(),
        usize::from(partial),
        "one owned incomplete warning only for recovery"
    );
    for marker in markers {
        assert_eq!(marker.level, safe_cex_engine::Level::Warn);
        assert!(marker.fields.iter().any(|(k, v)| *k == "command"
            && matches!(v,safe_cex_engine::Value::Num(n) if *n == f64::from(command))));
    }
    let mut terminals = out.into_iter().filter_map(|a| match a {
        Action::Notice(Notice::CommandDone {
            command: id,
            result,
        }) if id == command => Some(Ok(serde_json::from_str(&result).unwrap())),
        Action::Notice(Notice::CommandFailed { command: id, error }) if id == command => {
            Some(Err(error))
        }
        _ => None,
    });
    let result = terminals.next().expect("one terminal");
    assert!(terminals.next().is_none());
    result
}
fn fail_owned(e: &mut Engine<TxFlow>, r: &safe_cex_engine::HttpRequest, failure: u8) {
    match failure {
        0 => reply(e,r,r#"{"success":false,"code":9,"message":"denied","data":{"data":[],"hasMore":false}}"#),
        1 => e.handle(NOW,Event::HttpFailed {request:r.id,error:"offline"}),
        _ => reply(e,r,&json!({"success":true,"code":0,"data":{"data":[execution(99,NOW+99.0),{"bad":true}],"hasMore":false}}).to_string()),
    }
}
#[test]
fn owned_zero_and_filtered_only_failures_are_typed() {
    for filtered in [false, true] {
        for failure in 0..3 {
            let mut e = ready();
            let mut r = query(
                &mut e,
                fills(if filtered { Some(NOW + 1.0) } else { None }, None),
            );
            if filtered {
                reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[execution(1,NOW)],"hasMore":true}}).to_string());
                r = requests(&mut e).pop().unwrap();
            }
            fail_owned(&mut e, &r, failure);
            assert_eq!(
                owned_terminal(&mut e, 91, false).unwrap_err().kind,
                Some(safe_cex_engine::ErrorKind::Internal)
            );
        }
    }
}
#[test]
fn owned_later_failures_drop_current_prefix_and_keep_sorted_accepted_output() {
    for failure in 0..3 {
        let mut e = ready();
        let r = query(&mut e, fills(None, Some(1)));
        reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[execution(1,NOW),execution(2,NOW+1.0)],"hasMore":true}}).to_string());
        let r = requests(&mut e).pop().unwrap();
        fail_owned(&mut e, &r, failure);
        let rows = owned_terminal(&mut e, 91, true).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["id"], "2");
    }
}
#[test]
fn owned_mixed_income_keeps_prior_stages_on_ledger_failure() {
    let mut e = ready();
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
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[execution(1,NOW)],"hasMore":false}})
            .to_string(),
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.body.as_ref().unwrap().contains("userFundings"));
    reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[{"time":NOW as u64,"coin":"BTC","amount":"-0.12","szi":"2","fundingRate":"0.01","usdc":"-0.12"}],"hasMore":false}}).to_string());
    let r = requests(&mut e).pop().unwrap();
    assert!(
        r.body
            .as_ref()
            .unwrap()
            .contains("userNonFundingLedgerUpdates")
    );
    fail_owned(&mut e, &r, 0);
    assert_eq!(
        owned_terminal(&mut e, 91, true)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn owned_terminal_offset_cap_completes_but_required_next_offset_is_strict() {
    for needs_more in [false, true] {
        let mut e = ready();
        let mut r = query(&mut e, fills(None, Some(1)));
        for n in 0..100 {
            assert_eq!(
                serde_json::from_str::<Value>(r.body.as_ref().unwrap()).unwrap()["offset"],
                n * 2000
            );
            reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[execution(n+1,NOW+n as f64)],"hasMore":true}}).to_string());
            r = requests(&mut e).pop().unwrap();
        }
        assert_eq!(
            serde_json::from_str::<Value>(r.body.as_ref().unwrap()).unwrap()["offset"],
            200000
        );
        let rows = if needs_more {
            vec![execution(101, NOW + 100.0)]
        } else {
            vec![]
        };
        reply(
            &mut e,
            &r,
            &json!({"success":true,"code":0,"data":{"data":rows,"hasMore":needs_more}}).to_string(),
        );
        let result = owned_terminal(&mut e, 91, false);
        if needs_more {
            assert!(result.unwrap_err().message.contains("offset cap"));
        } else {
            assert_eq!(result.unwrap()[0]["id"], "100");
        }
    }
}

#[test]
fn owned_same_time_fills_keep_numeric_id_tie_sort() {
    let mut e = ready();
    let r = query(&mut e, fills(None, Some(1)));
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[execution(9,NOW)],"hasMore":true}})
            .to_string(),
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[execution(10,NOW)],"hasMore":false}})
            .to_string(),
    );
    assert_eq!(owned_terminal(&mut e, 91, false).unwrap()[0]["id"], "10");
}
#[test]
fn owned_empty_or_repeated_continuation_remains_strict_after_accepted_page() {
    for repeated in [false, true] {
        let mut e = ready();
        let r = query(&mut e, fills(None, None));
        let row = execution(1, NOW);
        reply(
            &mut e,
            &r,
            &json!({"success":true,"code":0,"data":{"data":[row.clone()],"hasMore":true}})
                .to_string(),
        );
        let r = requests(&mut e).pop().unwrap();
        let rows = if repeated { vec![row] } else { vec![] };
        reply(
            &mut e,
            &r,
            &json!({"success":true,"code":0,"data":{"data":rows,"hasMore":true}}).to_string(),
        );
        assert!(
            owned_terminal(&mut e, 91, false)
                .unwrap_err()
                .message
                .contains("Incomplete")
        );
    }
}
#[test]
fn owned_native_stage_offset_budget_restarts_after_terminal_cap() {
    let mut e = ready();
    let mut r = query(
        &mut e,
        Query::Income {
            symbol: Some("BTC-USDC".into()),
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    for n in 0..100 {
        reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[execution(n+1,NOW+n as f64)],"hasMore":true}}).to_string());
        r = requests(&mut e).pop().unwrap();
    }
    assert_eq!(
        serde_json::from_str::<Value>(r.body.as_ref().unwrap()).unwrap()["offset"],
        200000
    );
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[],"hasMore":false}}).to_string(),
    );
    let r = requests(&mut e).pop().unwrap();
    let body = serde_json::from_str::<Value>(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(body["type"], "userFundings");
    assert_eq!(body["offset"], 0);
    let funding = json!({"time":(NOW+1000.0) as u64,"coin":"BTC","amount":"-0.12","szi":"2","fundingRate":"0.01","usdc":"-0.12"});
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[funding],"hasMore":true}}).to_string(),
    );
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(r.body.as_ref().unwrap()).unwrap()["offset"],
        2000
    );
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[],"hasMore":false}}).to_string(),
    );
    let rows = owned_terminal(&mut e, 91, false).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["income"], -0.12);
}
