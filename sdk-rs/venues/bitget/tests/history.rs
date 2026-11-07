use crate::support::FixtureOptions as Options;
mod support;
use safe_cex_engine::{Command, Event, Query};
use serde_json::{Value, json};
use support::*;
fn query(h: &mut Harness, q: Query) {
    h.command(Command::Query {
        command: 42,
        query: q,
    });
}
fn fills() -> Query {
    Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    }
}
fn result(h: &Harness) -> Value {
    serde_json::from_str(
        h.done
            .iter()
            .find(|(id, _)| *id == 42)
            .unwrap()
            .1
            .as_ref()
            .unwrap(),
    )
    .unwrap()
}
fn fill(id: &str, side: &str, trade: &str) -> Value {
    json!({"tradeId":id,"symbol":"BTCUSDT","orderId":"999999999999999999","price":"1900","baseVolume":"1","feeDetail":[{"feeCoin":"USDT","totalFee":"-0.1"}],"side":side,"profit":"102","tradeSide":trade,"tradeScope":"taker","cTime":"1627293509612"})
}
fn page(h: &mut Harness, rows: Value, cursor: &str) {
    h.respond(
        "/order/fills",
        200,
        &json!({"code":"00000","data":{"fillList":rows,"endId":cursor}}).to_string(),
    );
}
#[test]
fn classic_hedge_close_is_execution_buy_and_cursor_does_not_stop_short_page() {
    let mut h = Harness::classic();
    query(&mut h, fills());
    page(
        &mut h,
        json!([fill("9007199254740993", "sell", "close")]),
        "page1",
    );
    assert!(h.done.iter().all(|(id, _)| *id != 42));
    h.timer_after(100.0);
    let r = h.take("/order/fills");
    assert!(r.url.contains("idLessThan=page1"));
    h.reply(r.id,200,&json!({"code":"00000","data":{"fillList":[fill("9007199254740994","buy","buy_single")],"endId":""}}).to_string());
    let rows = result(&h);
    assert_eq!(rows.as_array().unwrap().len(), 2);
    let row = &rows[0];
    assert_eq!(row["id"], "9007199254740993");
    assert_eq!(row["side"], "buy");
    assert_eq!(row["positionSide"], "short");
    assert_eq!(row["commission"], 0.1);
    assert_eq!(row["timestamp"], 1627293509.0);
    assert_eq!(rows[1]["positionSide"], "both");
}
#[test]
fn empty_last_page_ends_even_if_last_cursor_is_repeated() {
    let mut h = Harness::classic();
    query(&mut h, fills());
    page(&mut h, json!([fill("1", "buy", "open")]), "1");
    h.timer_after(100.0);
    page(&mut h, json!([]), "1");
    assert_eq!(result(&h).as_array().unwrap().len(), 1);
}
#[test]
fn nonadvancing_nonempty_cursor_is_incomplete_not_silent_success() {
    let mut h = Harness::classic();
    query(&mut h, fills());
    page(&mut h, json!([fill("1", "buy", "open")]), "1");
    h.timer_after(100.0);
    page(&mut h, json!([fill("1", "buy", "open")]), "1");
    assert!(h.done[0].1.as_ref().unwrap_err().contains("incomplete"));
}
#[test]
fn bills_separate_gross_pnl_signed_fee_and_keep_unknown_types() {
    let mut h = Harness::classic();
    query(
        &mut h,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    h.respond("/account/bill",200,&json!({"code":"00000","data":{"bills":[{"billId":"1","symbol":"BTCUSDT","coin":"USDT","businessType":"close_short","amount":"10","fee":"-1","cTime":"1627293509612"},{"billId":"2","symbol":"BTCUSDT","coin":"USDT","businessType":"contract_settle_fee","amount":"-2","fee":"0","cTime":"1627293509612"},{"billId":"3","businessType":"new_native_event","amount":"3","cTime":"1627293509612"}]}}).to_string());
    let r = result(&h);
    assert_eq!(r.as_array().unwrap().len(), 4);
    assert_eq!(r[0]["income"], 10.0);
    assert_eq!(r[0]["tranId"], "1:pnl");
    assert_eq!(r[1]["income"], -1.0);
    assert_eq!(r[1]["tranId"], "1:fee");
    assert_eq!(r[2]["category"], "funding");
    assert_eq!(r[3]["category"], "other");
}
#[test]
fn documented_order_history_and_public_trades_keep_json_contract() {
    let mut h = Harness::classic();
    query(
        &mut h,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    h.respond("/orders-history",200,r#"{"code":"00000","data":{"entrustedList":[{"symbol":"BTCUSDT","size":"100","orderId":"123","baseVolume":"12.1","price":"1900","priceAvg":"1903","status":"filled","side":"buy","posSide":"long","reduceOnly":"NO","tradeSide":"open","posMode":"hedge_mode","orderType":"limit","cTime":"1627293504612","uTime":"1627293505612"}]}}"#);
    let r = result(&h);
    assert_eq!(r[0]["id"], "123");
    assert_eq!(r[0]["averagePrice"], 1903.0);
    assert_eq!(r[0]["status"], "closed");
    assert_eq!(r[0]["createdTime"], 1627293504612.0);
    let mut h = Harness::public();
    query(
        &mut h,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(999),
        },
    );
    let r = h.take("/market/fills");
    assert!(r.url.contains("limit=100"));
    assert!(r.headers.is_empty());
    h.reply(r.id,200,r#"{"code":"00000","data":[{"tradeId":"1","price":"29990.5","size":"0.0166","side":"sell","ts":"1627116776464","symbol":"BTCUSDT"}]}"#);
    let r = result(&h);
    assert_eq!(r[0]["side"], "sell");
    assert_eq!(r[0]["quoteQty"], 497.8423);
}
#[test]
fn range_windows_are_inclusive_and_nonoverlapping() {
    let mut h = Harness::classic();
    let end = 30.0 * 86_400_000.0;
    query(
        &mut h,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(0.0),
            to_ms: Some(end),
            limit: Some(100),
        },
    );
    let r = h.take("/order/fills");
    assert!(r.url.contains("endTime=2591999999"));
    h.reply(r.id, 200, r#"{"code":"00000","data":{"fillList":[]}}"#);
    h.timer_after(100.0);
    let r = h.take("/order/fills");
    assert!(r.url.contains("startTime=2592000000"));
    h.reply(r.id, 200, r#"{"code":"00000","data":{"fillList":[]}}"#);
    assert_eq!(result(&h), json!([]));
}
#[test]
fn malformed_or_failed_history_never_succeeds_empty_or_keeps_partial_page() {
    let mut h = Harness::classic();
    query(&mut h, fills());
    let first = h.take("/order/fills");
    h.event(Event::HttpFailed {
        request: first.id,
        error: "offline",
    });
    // Transport failures retain retries; a proven row refusal now stops immediately.
    h.timer_after(250.0);
    let retry = h.take("/order/fills");
    h.reply(
        retry.id,
        200,
        r#"{"code":"00000","data":{"fillList":[{"bad":"row"}]}}"#,
    );
    assert!(h.done[0].1.is_err());
    assert_eq!(
        serde_json::to_value(&h.failures[0]).unwrap()["causeCode"],
        "HISTORY_DECODE"
    );
}
#[test]
fn uta_fills_use_native_exec_values_filter_symbols_and_preserve_rebates() {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    query(
        &mut h,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let r = h.take("/v3/trade/fills");
    assert!(!r.url.contains("symbol="));
    assert!(r.url.contains("category=USDT-FUTURES"));
    h.reply(r.id,200,&json!({"code":"00000","data":{"list":[{"execId":"9007199254740995","orderId":"2","symbol":"BTCUSDT","execPrice":"100","execQty":"0.2","execPnl":"1","createdTime":"1627293509612","side":"buy","tradeSide":"close","posSide":"short","feeDetail":[{"feeCoin":"USDT","fee":"-0.1"}]},{"execId":"other","symbol":"ETHUSDT"}],"cursor":""}}).to_string());
    let r = result(&h);
    assert_eq!(r.as_array().unwrap().len(), 1);
    assert_eq!(r[0]["commission"], -0.1);
    assert_eq!(r[0]["side"], "buy");
    assert_eq!(r[0]["positionSide"], "short");
    assert_eq!(r[0]["id"], "9007199254740995");
}
#[test]
fn uta_bills_use_financial_records_and_future_range_does_not_panic() {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    query(
        &mut h,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    assert!(h.requests.iter().all(|r| !r.url.contains("/account/bill")));
    h.respond(
        "/financial-records",
        200,
        r#"{"code":"00000","data":{"list":[],"cursor":""}}"#,
    );
    assert_eq!(result(&h), json!([]));
    let mut h = Harness::classic();
    query(
        &mut h,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(1_000_001.0),
            to_ms: None,
            limit: None,
        },
    );
    assert!(h.done[0].1.is_err());
}

#[test]
fn filtered_missing_symbol_and_invalid_bill_values_fail_without_replaying_rows() {
    for q in [
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    ] {
        let income = matches!(q, Query::Income { .. });
        let mut h = Harness::classic();
        query(&mut h, q);
        // Row validation is deterministic; the old three immediate replays are removed.
        if income {
            h.respond("/account/bill",200,r#"{"code":"00000","data":{"bills":[{"billId":"1","businessType":"close_long","cTime":"1627293509612","amount":"garbage","fee":"-1"}]}}"#);
        } else {
            page(&mut h, json!([{}]), "");
        }
        assert!(h.done[0].1.is_err());
    }
}
#[test]
fn failed_account_detection_has_a_deadline() {
    let mut h = Harness::private_started();
    h.expect_startup_failure = true;
    query(&mut h, fills());
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40014","msg":"bad passphrase","data":null}"#,
    );
    for _ in 0..200 {
        if !h.done.is_empty() {
            break;
        }
        let (timer, at) = h
            .timers
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        h.timers.retain(|(t, _)| *t != timer);
        h.now = at;
        h.event(Event::TimerFired { timer });
    }
    assert!(
        h.done[0]
            .1
            .as_ref()
            .unwrap_err()
            .contains("account detection")
    );
    assert_eq!(
        h.startup_failures.len(),
        1,
        "a refused permission fails startup"
    );
}
#[test]
fn first_query_late_uta_signal_reprobes_and_uses_v3() {
    let mut h = Harness::classic();
    query(&mut h, fills());
    h.respond(
        "/order/fills",
        200,
        r#"{"code":"40085","msg":"Classic Account API is not supported","data":null}"#,
    );
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    h.timer_after(100.0);
    h.respond(
        "/v3/trade/fills",
        200,
        r#"{"code":"00000","data":{"list":[],"cursor":""}}"#,
    );
    assert_eq!(result(&h), json!([]));
}

#[test]
fn inverse_fill_without_fee_coin_uses_the_symbol_collateral() {
    use safe_cex_venue_bitget::Product;
    let mut h = Harness::started(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        product: Product::Inverse,
        ..Options::default()
    });
    h.respond(
        "/contracts",
        200,
        &CONTRACTS
            .replace("BTCUSDT", "ETHUSD")
            .replace("BTC", "ETH")
            .replace("USDT", "USD"),
    );
    h.respond("/tickers", 200, &TICKERS.replace("BTCUSDT", "ETHUSD"));
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    query(&mut h, fills());
    let mut row = fill("eth-fill", "buy", "buy_single");
    row["symbol"] = json!("ETHUSD");
    row["feeDetail"] = json!([{"totalFee":"-0.0001"}]);
    page(&mut h, json!([row]), "");
    assert_eq!(result(&h)[0]["commissionAsset"], "ETH");
    assert_eq!(result(&h)[0]["commission"], 0.0001);
}

/// Pins the exact request sequence of a windowed, cursor-paged read: 30-day
/// windows oldest first, the first walked over two cursor pages. Paging
/// refactors keep it.
#[test]
fn windowed_fill_request_sequence_is_pinned() {
    let mut h = Harness::classic();
    let day = 86_400_000.0;
    query(
        &mut h,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(0.0),
            to_ms: Some(45.0 * day),
            limit: Some(100),
        },
    );
    let mut sent = Vec::new();
    for (i, cursor) in [("1", "c1"), ("2", ""), ("3", "")] {
        if !sent.is_empty() {
            h.timer_after(100.0);
        }
        let r = h.take("/order/fills");
        sent.push(r.url.split_once("/api/").unwrap().1.to_owned());
        let rows = if cursor.is_empty() {
            json!([])
        } else {
            json!([fill(i, "buy", "buy_single")])
        };
        h.reply(
            r.id,
            200,
            &json!({"code":"00000","data":{"fillList":rows,"endId":cursor}}).to_string(),
        );
    }
    assert!(h.done.iter().any(|(id, r)| *id == 42 && r.is_ok()));
    assert_eq!(
        sent,
        [
            "v2/mix/order/fills?productType=usdt-futures&limit=100&startTime=0&endTime=2591999999",
            "v2/mix/order/fills?productType=usdt-futures&limit=100&startTime=0&endTime=2591999999&idLessThan=c1",
            "v2/mix/order/fills?productType=usdt-futures&limit=100&startTime=2592000000&endTime=3888000000",
        ]
    );
}

// Live: a range ending one past a window edge left a final window with start equal to end: 20001.
#[test]
fn a_one_millisecond_final_window_still_has_a_start_before_its_end() {
    let mut h = Harness::classic();
    let end = 30.0 * 86_400_000.0;
    query(
        &mut h,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(0.0),
            to_ms: Some(end),
            limit: Some(100),
        },
    );
    let r = h.take("/order/fills");
    h.reply(r.id, 200, r#"{"code":"00000","data":{"fillList":[]}}"#);
    h.timer_after(100.0);
    let r = h.take("/order/fills");
    assert!(r.url.contains("startTime=2592000000"));
    assert!(r.url.contains("endTime=2592000001"), "{}", r.url);
}

#[test]
fn classic_order_row_refusal_is_typed_immediate_and_keeps_the_first_closed_reason() {
    let mut h = Harness::classic();
    query(
        &mut h,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let request = h.take("/orders-history");
    assert!(request.url.contains("/api/v2/mix/order/orders-history"));
    h.reply(request.id, 200, &json!({"code":"00000","data":{"entrustedList":[
        {"symbol":"BTCUSDT","size":"0","orderId":"123","baseVolume":"0","status":"canceled","side":"buy","posSide":"long","tradeSide":"open","posMode":"hedge_mode","orderType":"limit","cTime":"1627293504612"},
        {"symbol":"","size":"1"}
    ]}}).to_string());
    assert_eq!(
        h.failures.len(),
        1,
        "a proven invalid row must not be replayed three times"
    );
    let error = serde_json::to_value(&h.failures[0]).unwrap();
    assert_eq!(error["kind"], "internal");
    assert_eq!(error["causeCode"], "HISTORY_DECODE");
    assert_eq!(
        error["historyDecode"],
        json!({"schema":"bitgetClassicOrders","reason":"quantity"})
    );
    assert!(h.done.iter().all(|(_, result)| result.is_err()));
    h.timer_after(2_000.0);
    assert!(!h.requests.iter().any(|r| r.url.contains("/orders-history")));
}

#[test]
fn uta_fill_decode_names_schema_and_first_failed_predicate() {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    query(&mut h, fills());
    let request = h.take("/v3/trade/fills");
    h.reply(request.id, 200, &json!({"code":"00000","data":{"list":[
        {"execId":"first","symbol":"BTCUSDT","execPrice":"100","execQty":"0","createdTime":"1627293509612","side":"buy","tradeSide":"close","posSide":"short"},
        {"execId":"second","symbol":""}
    ]}}).to_string());
    assert_eq!(h.failures.len(), 1);
    let error = serde_json::to_value(&h.failures[0]).unwrap();
    assert_eq!(error["causeCode"], "HISTORY_DECODE");
    assert_eq!(
        error["historyDecode"],
        json!({"schema":"bitgetUtaFills","reason":"quantity"})
    );
    assert!(h.done.iter().all(|(_, result)| result.is_err()));
}

#[test]
fn transient_http_history_failure_still_retries_and_accepts_a_valid_page() {
    let mut h = Harness::classic();
    query(&mut h, fills());
    let request = h.take("/order/fills");
    h.reply(
        request.id,
        503,
        r#"{"code":"50000","msg":"temporarily unavailable"}"#,
    );
    assert!(h.failures.is_empty());
    assert!(!h.done.iter().any(|(id, _)| *id == 42));
    h.timer_after(250.0);
    let retry = h.take("/order/fills");
    assert_ne!(retry.id, request.id);
    h.reply(
        retry.id,
        200,
        &json!({"code":"00000","data":{"fillList":[fill("valid", "buy", "buy_single")]}})
            .to_string(),
    );
    assert_eq!(result(&h)[0]["id"], "valid");
    assert!(h.failures.is_empty());
}

fn cancelled_order_history(uta: bool, limit: Option<u32>) -> Harness {
    let mut h = if uta {
        Harness::private_started()
    } else {
        Harness::classic()
    };
    if uta {
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
        );
    }
    query(
        &mut h,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit,
        },
    );
    h
}

fn cancelled_order(id: usize) -> Value {
    json!({
        "orderId": (1500078781190803457_u64 + id as u64).to_string(),
        "symbol": "USELESSUSDT", "orderType": "limit", "side": "sell",
        "size": "10", "qty": "10", "baseVolume": "0", "cumExecQty": "0",
        "status": "canceled", "orderStatus": "cancelled", "price": "0.1", "priceAvg": "0", "avgPrice": "0",
        "reduceOnly": "YES", "tradeSide": "close", "posMode": "one_way_mode", "holdMode": "one_way_mode",
        "cTime": "1793372400000", "createdTime": "1793372400000", "uTime": "1793373600000", "updatedTime": "1793373600000"
    })
}

fn orders_page(h: &mut Harness, uta: bool, rows: Vec<Value>, cursor: Option<Value>) {
    let mut data = json!({ if uta { "list" } else { "entrustedList" }: rows });
    if let Some(cursor) = cursor {
        data[if uta { "cursor" } else { "endId" }] = cursor;
    }
    h.respond(
        if uta {
            "/v3/trade/history-orders"
        } else {
            "/orders-history"
        },
        200,
        &json!({"code": "00000", "data": data}).to_string(),
    );
}

#[test]
fn a_full_orders_page_without_a_cursor_never_proves_complete_history() {
    for uta in [false, true] {
        for cursor in [None, Some(Value::Null), Some(json!(""))] {
            for limit in [100, 1] {
                let mut h = cancelled_order_history(uta, Some(limit));
                orders_page(
                    &mut h,
                    uta,
                    (0..limit as usize).map(cancelled_order).collect(),
                    cursor.clone(),
                );
                assert!(
                    h.events
                        .iter()
                        .any(|event| event.code == "history.incomplete"),
                    "a full page with no cursor was silently completed: uta={uta}, limit={limit}, cursor={cursor:?}"
                );
                let markers = h
                    .events
                    .iter()
                    .filter(|event| event.code == "history.incomplete")
                    .collect::<Vec<_>>();
                assert_eq!(markers.len(), 1);
                assert!(
                    markers[0]
                        .fields
                        .contains(&("command", safe_cex_engine::log::Value::Num(42.)))
                );
                assert_eq!(h.done.iter().filter(|(id, _)| *id == 42).count(), 1);
                assert_eq!(result(&h).as_array().unwrap().len(), limit as usize);
                assert!(
                    h.failures.is_empty(),
                    "validated rows remain a usable lower bound"
                );
            }
        }
    }
}

#[test]
fn cancelled_zero_fill_ladders_survive_paging_and_do_not_need_fills() {
    for uta in [false, true] {
        let mut h = cancelled_order_history(uta, None);
        let first = h.take(if uta {
            "/v3/trade/history-orders"
        } else {
            "/orders-history"
        });
        assert!(!first.url.contains("status="));
        assert!(!first.url.contains("orderSource="));
        h.requests.push(first);
        orders_page(
            &mut h,
            uta,
            (0..100).map(cancelled_order).collect(),
            Some(json!("next-page")),
        );
        assert!(h.done.iter().all(|(id, _)| *id != 42));
        h.timer_after(100.0);
        let next = h.take(if uta {
            "/v3/trade/history-orders"
        } else {
            "/orders-history"
        });
        assert!(next.url.contains(if uta {
            "cursor=next-page"
        } else {
            "idLessThan=next-page"
        }));
        h.requests.push(next);
        orders_page(&mut h, uta, (100..120).map(cancelled_order).collect(), None);
        assert!(
            !h.events
                .iter()
                .any(|event| event.code == "history.incomplete")
        );
        let rows = result(&h);
        assert_eq!(rows.as_array().unwrap().len(), 120);
        for (i, row) in rows.as_array().unwrap().iter().enumerate() {
            assert_eq!(row["id"], (1500078781190803457_u64 + i as u64).to_string());
            assert_eq!(row["status"], "canceled");
            assert_eq!(row["type"], "limit");
            assert_eq!(row["filled"], 0.0);
            assert_eq!(row["reduceOnly"], true);
        }
    }
}
