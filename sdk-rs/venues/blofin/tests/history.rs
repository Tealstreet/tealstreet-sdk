mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Query};
use support::*;
fn query(h: &mut Harness, query: Query) {
    h.ws("pong");
    h.handle(1.0, Event::Command(Command::Query { command: 7, query }));
    h.collect_at(1.0);
}
fn result(h: &Harness) -> Result<serde_json::Value, String> {
    h.actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 7, result }) => {
                Some(Ok(serde_json::from_str(result).unwrap()))
            }
            Action::Notice(Notice::CommandFailed { command: 7, error }) => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .unwrap()
}
#[test]
fn fills_keep_independent_execution_ids_and_sub_lot_quantities() {
    let mut h = Harness::new();
    h.seed();
    query(
        &mut h,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: Some("12".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    );
    h.reply("fills-history",r#"{"code":"0","data":[{"instId":"BTC-USDT","tradeId":"1","orderId":"12","side":"buy","fillPrice":"60000","fillSize":"0.01","fee":"0.02","feeCurrency":"USDT","fillPnl":"1","ts":"1700000000123"},{"instId":"BTC-USDT","tradeId":"2","orderId":"12","side":"buy","fillPrice":"60000","fillSize":"0.01","fee":"0.02","ts":"1700000000123"}]}"#);
    let rows = result(&h).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["qty"], 0.00001);
    assert_eq!(rows[0]["timestamp"], 1700000000.0);
    assert_eq!(rows[1]["id"], "2");
    assert_eq!(h.e.store().positions()[0].contracts, 0.004);
}
#[test]
fn order_history_reports_base_units_and_millisecond_times() {
    let mut h = Harness::new();
    h.seed();
    query(
        &mut h,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    h.reply("orders-history",r#"{"code":"0","data":[{"instId":"BTC-USDT","orderId":"12","side":"sell","positionSide":"net","orderType":"post_only","state":"canceled","price":"60000","size":"10","filledSize":"4","averagePrice":"59999","createTime":"1700000000000","updateTime":"1700000001000","reduceOnly":"true"},{"instId":"DELISTED-USDT","orderId":"13","side":"buy","positionSide":"net","orderType":"limit","state":"filled","price":"1","size":"10","filledSize":"10","createTime":"1700000000000"}]}"#);
    let rows = result(&h).unwrap();
    // contractValue 0.001: 10 contracts are 0.01 BTC, as open orders and fills report.
    assert_eq!(
        rows.as_array().unwrap().len(),
        1,
        "an unknown market cannot be converted"
    );
    assert_eq!(rows[0]["amount"], 0.01);
    assert_eq!(rows[0]["filled"], 0.004);
    assert_eq!(rows[0]["type"], "limit");
    assert_eq!(rows[0]["status"], "canceled");
    assert_eq!(rows[0]["createdTime"], 1700000000000.0);
    assert_eq!(rows[0]["positionSide"], "both");
}
#[test]
fn income_filters_the_window_and_reverses_charged_fees() {
    let mut h = Harness::new();
    h.seed();
    query(
        &mut h,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: Some(1700000000000.0),
            to_ms: Some(1700000002000.0),
            limit: None,
        },
    );
    h.reply("fills-history",r#"{"code":"0","data":[{"instId":"BTC-USDT","tradeId":"1","ts":"1700000001000","fillPnl":"3","fee":"0.2"},{"instId":"DELISTED-USDT","tradeId":"2","ts":"1700000001000","fillPnl":"1","fee":"-0.1"},{"instId":"BTC-USDT","tradeId":"3","ts":"1699999999000","fillPnl":"8","fee":"0.2"}]}"#);
    let rows = result(&h).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 4);
    assert_eq!(rows[1]["income"], -0.2);
    assert_eq!(rows[1]["category"], "commission");
    assert_eq!(rows[3]["incomeType"], "REBATE");
    assert_eq!(rows[2]["symbol"], "DELISTED-USDT");
}
#[test]
fn failing_later_income_page_returns_only_prior_accepted_ledger() {
    let mut h = Harness::new();
    h.seed();
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
    let rows = (0..100)
        .map(|i| {
            format!(
                r#"{{"instId":"BTC-USDT","tradeId":"{}","ts":"1700000000000","fillPnl":"1"}}"#,
                1000 - i
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    h.reply(
        "fills-history",
        &format!("{{\"code\":\"0\",\"data\":[{rows}]}}"),
    );
    let r = h.take("fills-history");
    assert!(r.url.ends_with("&after=901"));
    h.respond(r, r#"{"code":"152401","msg":"denied","data":[]}"#);
    assert_eq!(result(&h).unwrap().as_array().unwrap().len(), 100);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::Notice(Notice::Event(event))
        if event.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE
        && event.fields.contains(&("command",safe_cex_engine::Value::Num(7.0)))))
    );
}
#[test]
fn public_trades_have_base_and_quote_quantities() {
    let mut h = Harness::new();
    h.seed();
    query(
        &mut h,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(5),
        },
    );
    let r = h.take("market/trades");
    assert!(r.headers.is_empty());
    h.respond(r,r#"{"code":"0","data":[{"instId":"BTC-USDT","tradeId":"1","side":"sell","price":"60000","size":"2","ts":"1700000000000"}]}"#);
    let rows = result(&h).unwrap();
    assert_eq!(rows[0]["qty"], 0.002);
    assert_eq!(rows[0]["quoteQty"], 120.0);
}

#[test]
fn execution_aliases_signed_size_seconds_and_instrument_filter_are_preserved() {
    let mut h = Harness::new();
    h.seed();
    query(
        &mut h,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    );
    h.reply("fills-history",r#"{"code":"0","data":[{"instId":"BTC-USDT","tradeId":"1","orderId":"12","actualSide":"sell","execPrice":"60000","execQty":"-0.01","execTime":"1700000000","execFee":"-0.02","feeCcy":"USDT"},{"instId":"BTC-USDT","instType":"SPOT","tradeId":"2","orderId":"12","side":"buy","fillPrice":"60000","fillSize":"1","ts":"1700000000000"}]}"#);
    let rows = result(&h).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["qty"], 0.00001);
    assert_eq!(rows[0]["commission"], 0.02);
    assert_eq!(rows[0]["commissionAsset"], "USDT");
    assert_eq!(rows[0]["timestamp"], 1700000000.0);
}
#[test]
fn income_uses_smallest_unbounded_numeric_id_even_for_ascending_pages() {
    let mut h = Harness::new();
    h.seed();
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
    let rows=(0..100).map(|i|format!(r#"{{"instId":"BTC-USDT","tradeId":"9007199254740993{:03}","ts":"1700000000000","fillPnl":"1"}}"#,i)).collect::<Vec<_>>().join(",");
    h.reply(
        "fills-history",
        &format!("{{\"code\":\"0\",\"data\":[{rows}]}}"),
    );
    let r = h.take("fills-history");
    assert!(r.url.ends_with("&after=9007199254740993000"));
    h.respond(r, r#"{"code":"0","data":[]}"#);
    assert_eq!(result(&h).unwrap().as_array().unwrap().len(), 100);
}
#[test]
fn older_fill_windows_walk_past_newer_pages() {
    let mut h = Harness::new();
    h.seed();
    query(
        &mut h,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(1700000000000.0),
            to_ms: Some(1700000001000.0),
            limit: Some(1),
        },
    );
    h.reply("fills-history",r#"{"code":"0","data":[{"instId":"BTC-USDT","tradeId":"2","orderId":"12","side":"buy","fillPrice":"60000","fillSize":"1","ts":"1700000002000"}]}"#);
    let r = h.take("fills-history");
    assert!(r.url.ends_with("&after=2"));
    h.respond(r,r#"{"code":"0","data":[{"instId":"BTC-USDT","tradeId":"1","orderId":"12","side":"buy","fillPrice":"60000","fillSize":"1","ts":"1700000000500"}]}"#);
    assert_eq!(result(&h).unwrap()[0]["id"], "1");
}

#[test]
fn primary_execution_fields_win_over_conflicting_aliases_including_zero_fee() {
    let mut h = Harness::new();
    h.seed();
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
    h.reply("fills-history",r#"{"code":"0","data":[{"instId":"BTC-USDT","tradeId":"1","orderId":"12","side":"buy","actualSide":"sell","fillPrice":"60000","fillSize":"1","fee":"0","execFee":"0.2","ts":"1700000000000"}]}"#);
    let rows = result(&h).unwrap();
    assert_eq!(rows[0]["side"], "buy");
    assert_eq!(rows[0]["commission"], 0.0);
}

/// Pins the exact request sequence of id-cursor paging. Paging refactors
/// keep it.
#[test]
fn id_cursor_request_sequence_is_pinned() {
    let mut h = Harness::new();
    h.seed();
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
    let page = |base: u32| {
        (0..100)
            .map(|i| {
                format!(
                    r#"{{"instId":"BTC-USDT","tradeId":"{}","ts":"1700000000000","fillPnl":"1"}}"#,
                    base - i
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut sent = Vec::new();
    for body in [page(1000), page(900), String::new()] {
        let r = h.take("fills-history");
        sent.push(r.url.split_once("/api/").unwrap().1.to_owned());
        h.respond(r, &format!("{{\"code\":\"0\",\"data\":[{body}]}}"));
    }
    assert!(result(&h).is_ok());
    assert_eq!(
        sent,
        [
            "v1/trade/fills-history?limit=100",
            "v1/trade/fills-history?limit=100&after=901",
            "v1/trade/fills-history?limit=100&after=801",
        ]
    );
}

/// Filtering does not change native page-size/cursor progress, and the fill
/// budget counts admitted executions across all accepted pages.
#[test]
fn filtered_fills_request_sequence_and_global_mapped_limit_are_pinned() {
    let mut h = Harness::new();
    h.seed();
    query(
        &mut h,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: Some("12".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(2),
        },
    );
    let row = |id: u32, order: u32| {
        format!(
            r#"{{"instId":"BTC-USDT","tradeId":"{id}","orderId":"{order}","side":"buy","fillPrice":"60000","fillSize":"1","ts":"1700000000000"}}"#
        )
    };
    let bodies = [
        format!("{},{}", row(200, 99), row(100, 99)),
        format!("{},{}", row(90, 12), row(80, 99)),
        row(70, 12),
    ];
    let mut sent = Vec::new();
    for body in bodies {
        let r = h.take("fills-history");
        sent.push(r.url.split_once("/api/").unwrap().1.to_owned());
        h.respond(r, &format!(r#"{{"code":"0","data":[{body}]}}"#));
    }
    assert_eq!(
        sent,
        [
            "v1/trade/fills-history?instId=BTC-USDT&orderId=12&limit=2",
            "v1/trade/fills-history?instId=BTC-USDT&orderId=12&limit=2&after=100",
            "v1/trade/fills-history?instId=BTC-USDT&orderId=12&limit=2&after=80",
        ]
    );
    let rows = result(&h).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["id"], "90");
    assert_eq!(rows[1]["id"], "70");
}

#[test]
fn income_final_limit_does_not_shorten_native_paging_or_split_native_id_dedupe() {
    let mut h = Harness::new();
    h.seed();
    query(
        &mut h,
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    let page=(0..100).map(|i|format!(r#"{{"instId":"BTC-USDT","tradeId":"{}","ts":"1700000000000","fillPnl":"1","fee":"0.2"}}"#,1000-i)).collect::<Vec<_>>().join(",");
    h.reply(
        "fills-history",
        &format!(r#"{{"code":"0","data":[{page}]}}"#),
    );
    let r = h.take("fills-history");
    assert!(r.url.ends_with("limit=100&after=901"));
    h.respond(r, r#"{"code":"0","data":[]}"#);
    let rows = result(&h).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["tranId"], "1000:pnl");
}
