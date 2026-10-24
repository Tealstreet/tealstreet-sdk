//! Official futures userTrades, order/history, allAlgoOrders, income and trades payloads.
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Query, RequestId, Response,
};
mod support;
use safe_cex_venue_weex::WeexFutures;
use serde::Deserialize;
use support::setup::Options;

const NOW: f64 = 1_764_505_700_000.0;
const DAY: f64 = 86_400_000.0;
fn engine() -> Engine<WeexFutures> {
    support::setup::engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        ..Options::default()
    })
}
fn command(e: &mut Engine<WeexFutures>, query: Query) -> Vec<Action> {
    e.handle(NOW, Event::Command(Command::Query { command: 7, query }));
    support::managed::drain(e, NOW)
}
fn request(actions: &[Action], path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .expect("history request")
}
fn response(e: &mut Engine<WeexFutures>, now: f64, id: RequestId, body: &str) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    support::managed::drain(e, now)
}
fn result(actions: &[Action]) -> Result<String, String> {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 7, result }) => Some(Ok(result.clone())),
            Action::Notice(Notice::CommandFailed { command: 7, error }) => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .expect("query completion")
}
fn owned_incomplete(actions: &[Action], command: u32) {
    let mut count = 0;
    let completion = actions.iter().position(|a| matches!(a, Action::Notice(Notice::CommandDone { command: id, .. } | Notice::CommandFailed { command: id, .. }) if *id == command));
    for (index, action) in actions.iter().enumerate() {
        if let Action::Notice(Notice::Event(event)) = action
            && event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
        {
            assert_eq!(event.level, safe_cex_engine::Level::Warn);
            assert_eq!(
                event.fields,
                vec![("command", safe_cex_engine::Value::Num(f64::from(command)))]
            );
            if let Some(completion) = completion {
                assert!(index < completion, "coverage must precede its completion");
            }
            count += 1;
        }
    }
    assert!(count > 0, "owned incomplete marker missing");
}
fn no_completion(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}
fn advance(e: &mut Engine<WeexFutures>, now: &mut f64, actions: Vec<Action>) -> Vec<Action> {
    let (timer, at) = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("paced history timer");
    assert!(at >= *now + 100.0);
    *now = at;
    e.handle(*now, Event::TimerFired { timer });
    support::managed::drain(e, *now)
}
fn income(from: Option<f64>, to: Option<f64>) -> Query {
    Query::Income {
        symbol: None,
        kind: None,
        from_ms: from,
        to_ms: to,
        limit: Some(1),
    }
}

fn owned_income_failure(actions: &[Action], command: u32) {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "first failure must terminate exactly once");
    assert_eq!(errors[0].kind, Some(safe_cex_engine::ErrorKind::Internal));
    assert!(
        !actions.iter().any(
            |a| matches!(a,Action::Notice(Notice::CommandDone{command:id,..}) if *id==command)
        )
    );
    assert!(!actions.iter().any(|a| matches!(a,Action::Notice(Notice::Event(e)) if e.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
    );
}
fn owned_income_partial(actions: &[Action], count: usize) {
    let markers: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::Notice(Notice::Event(e))
                if e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some((i, e))
            }
            _ => None,
        })
        .collect();
    assert_eq!(markers.len(), 1, "one final command-owned coverage marker");
    assert_eq!(
        markers[0].1.fields,
        vec![("command", safe_cex_engine::Value::Num(7.0))]
    );
    assert_eq!(markers[0].1.level, safe_cex_engine::Level::Warn);
    let done = actions
        .iter()
        .position(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
        .expect("partial success");
    assert!(markers[0].0 < done);
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(&result(actions).unwrap())
            .unwrap()
            .len(),
        count
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 7, .. })
            | Action::Http(_)
            | Action::SetTimer { .. }
    )));
}
fn failed_income_page(e: &mut Engine<WeexFutures>, req: HttpRequest, mode: u8) -> Vec<Action> {
    match mode {
        0 => e.handle(
            NOW,
            Event::HttpFailed {
                request: req.id,
                error: "page reset",
            },
        ),
        1 => e.handle(
            NOW,
            Event::HttpResponse {
                request: req.id,
                response: Response {
                    status: 503,
                    headers: "",
                    body: "unavailable",
                },
            },
        ),
        2 => {
            return response(
                e,
                NOW,
                req.id,
                &format!(
                    r#"{{"items":[{},{{"income":"bad"}}]}}"#,
                    bill(999, NOW + 999.0)
                ),
            );
        }
        3 => return response(e, NOW, req.id, r#"{"items":null}"#),
        4 => return response(e, NOW, req.id, r#"{"code":40014,"msg":"native refusal"}"#),
        5 => return response(e, NOW, req.id, r#"{"items":{}}"#),
        6 => {
            return response(
                e,
                NOW,
                req.id,
                &format!(r#"{{"items":[{},"#, bill(999, NOW + 999.0)),
            );
        }
        _ => unreachable!(),
    }
    support::managed::drain(e, NOW)
}
fn full_income_page() -> String {
    format!(
        r#"{{"items":[{}]}}"#,
        (0..100)
            .map(|i| bill(i + 1, NOW + i as f64))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[test]
fn owned_income_first_failure_rejects_without_marker_or_continuation() {
    for ranged in [false, true] {
        for mode in 0..7 {
            let mut e = engine();
            e.set_log_level(safe_cex_engine::Level::Error);
            let actions = command(
                &mut e,
                if ranged {
                    income(Some(NOW), Some(NOW + 1000.0))
                } else {
                    income(None, None)
                },
            );
            let req = request(&actions, "/account/income");
            owned_income_failure(&failed_income_page(&mut e, req, mode), 7);
        }
    }
}
#[test]
fn owned_income_later_failure_returns_accepted_page_without_failed_prefix() {
    for mode in 0..7 {
        let mut e = engine();
        let mut now = NOW;
        e.set_log_level(safe_cex_engine::Level::Error);
        let actions = command(&mut e, income(Some(NOW), Some(NOW + 1000.0)));
        let req = request(&actions, "/account/income");
        let actions = response(&mut e, now, req.id, &full_income_page());
        let actions = advance(&mut e, &mut now, actions);
        let req = request(&actions, "/account/income");
        let actions = failed_income_page(&mut e, req, mode);
        owned_income_partial(&actions, 100);
        assert!(
            !serde_json::from_str::<Vec<serde_json::Value>>(&result(&actions).unwrap())
                .unwrap()
                .iter()
                .any(|row| row["tranId"] == "999")
        );
    }
}
#[test]
fn owned_income_empty_prior_chunk_does_not_authorize_partial_success() {
    let mut e = engine();
    let mut now = NOW;
    let actions = command(&mut e, income(Some(NOW), Some(NOW + 200.0 * DAY)));
    let req = request(&actions, "/account/income");
    let actions = response(&mut e, now, req.id, r#"{"items":[]}"#);
    let actions = advance(&mut e, &mut now, actions);
    let actions = advance(&mut e, &mut now, actions);
    let req = request(&actions, "/account/income");
    owned_income_failure(&failed_income_page(&mut e, req, 0), 7);
}
#[test]
fn owned_income_cursor_gap_is_emitted_once_at_success_only() {
    for empty in [false, true] {
        let mut e = engine();
        let mut now = NOW;
        e.set_log_level(safe_cex_engine::Level::Error);
        let actions = command(&mut e, income(Some(NOW), Some(NOW + 1000.0)));
        let req = request(&actions, "/account/income");
        let body = format!(
            r#"{{"items":[{}],"hasNextPage":true}}"#,
            if empty {
                String::new()
            } else {
                bill(1, NOW + 1.0)
            }
        );
        let mut actions = response(&mut e, now, req.id, &body);
        assert!(!actions.iter().any(|a|matches!(a,Action::Notice(Notice::Event(v)) if v.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)),"coverage publishes with completion only");
        for _ in 0..8 {
            if actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
            {
                break;
            }
            actions = if let Some(req) = actions.iter().find_map(|a| match a {
                Action::Http(r) => Some(r.clone()),
                _ => None,
            }) {
                response(&mut e, now, req.id, r#"{"items":[]}"#)
            } else {
                advance(&mut e, &mut now, actions)
            };
        }
        owned_income_partial(&actions, usize::from(!empty));
    }
}

#[test]
fn owned_income_partial_cannot_mark_or_complete_a_pending_neighbour() {
    let mut e = engine();
    let mut now = NOW;
    e.set_log_level(safe_cex_engine::Level::Error);
    let actions = command(&mut e, income(Some(NOW), Some(NOW + 1000.0)));
    let req = request(&actions, "/account/income");
    let actions = response(&mut e, now, req.id, &full_income_page());
    let actions = advance(&mut e, &mut now, actions);
    let req = request(&actions, "/account/income");
    e.handle(
        now,
        Event::Command(Command::Query {
            command: 8,
            query: income(None, None),
        }),
    );
    let neighbour = request(&support::managed::drain(&mut e, now), "/account/income");
    let actions = failed_income_page(&mut e, req, 0);
    owned_income_partial(&actions, 100);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { command: 8, .. } | Notice::CommandFailed { command: 8, .. }
        )
    )));
    let actions = response(&mut e, now, neighbour.id, r#"{"items":[]}"#);
    assert!(actions.iter().any(
        |a| matches!(a,Action::Notice(Notice::CommandDone{command:8,result}) if result=="[]")
    ));
    assert!(!actions.iter().any(|a|matches!(a,Action::Notice(Notice::Event(v)) if v.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
}

#[test]
fn owned_single_page_empty_cursor_gap_and_equal_bounds_keep_native_rows() {
    for equal in [false, true] {
        let mut e = engine();
        let actions = command(
            &mut e,
            if equal {
                income(Some(NOW), Some(NOW))
            } else {
                income(None, None)
            },
        );
        let req = request(&actions, "/account/income");
        let actions = response(
            &mut e,
            NOW,
            req.id,
            r#"{"items":[],"nextKey":"unconsumed"}"#,
        );
        owned_income_partial(&actions, 0);
    }
    let mut e = engine();
    let actions = command(&mut e, income(Some(NOW), Some(NOW)));
    let req = request(&actions, "/account/income");
    let row = bill(1, NOW + 1000.0);
    let actions = response(
        &mut e,
        NOW,
        req.id,
        &format!(r#"{{"items":[{row},{row}]}}"#),
    );
    assert_eq!(
        serde_json::from_str::<Vec<IncomeResult>>(&result(&actions).unwrap())
            .unwrap()
            .len(),
        2,
        "single-page mode adds neither range filtering nor dedupe"
    );
}

#[test]
fn owned_income_range_resume_keeps_the_selected_bound_after_a_shared_pause() {
    let mut e = engine();
    let mut now = NOW;
    let actions = command(&mut e, income(Some(NOW), Some(NOW + 1000.0)));
    let req = request(&actions, "/account/income");
    ban(&mut e);
    let actions = response(&mut e, now, req.id, &full_income_page());
    let actions = advance(&mut e, &mut now, actions);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let actions = advance(&mut e, &mut now, actions);
    assert_eq!(now, NOW + 10_000.0);
    let req = request(&actions, "/account/income");
    let body: IncomeBody = serde_json::from_str(req.body.as_ref().unwrap()).unwrap();
    assert_eq!(body.start_time, Some((NOW + 99.0) as u64));
    let actions = response(&mut e, now, req.id, r#"{"items":[]}"#);
    let actions = advance(&mut e, &mut now, actions);
    assert_eq!(
        serde_json::from_str::<Vec<IncomeResult>>(&result(&actions).unwrap())
            .unwrap()
            .len(),
        100
    );
    assert!(!actions.iter().any(|a|matches!(a,Action::Notice(Notice::Event(v)) if v.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    timestamp: f64,
    order_id: String,
    side: String,
    qty: f64,
    commission: f64,
    position_side: String,
    realized_pnl: f64,
    maker: bool,
}
#[test]
fn documented_fill_preserves_large_identity_and_seconds() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: Some("702345678901234567".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(500),
        },
    );
    let req = request(&actions, "/userTrades");
    assert_eq!(req.method, Method::Get);
    assert!(
        req.url
            .ends_with("limit=100&orderId=702345678901234567&symbol=BTCUSDT")
    );
    assert!(req.headers.iter().any(|(name, _)| *name == "ACCESS-SIGN"));
    let actions = response(
        &mut e,
        NOW,
        req.id,
        r#"[{"id":801234567890123456,"orderId":702345678901234567,"symbol":"BTCUSDT","commission":"-0.138","commissionAsset":"USDT","maker":false,"price":"69000","qty":"0.01","quoteQty":"690","realizedPnl":"2","side":"BUY","positionSide":"LONG","time":1764505701456}]"#,
    );
    let rows: Vec<Execution> = serde_json::from_str(&result(&actions).unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.id, "801234567890123456");
    assert_eq!(r.order_id, "702345678901234567");
    assert_eq!(r.timestamp, 1_764_505_701.0);
    assert_eq!(r.side, "buy");
    assert_eq!(r.qty, 0.01);
    assert_eq!(r.commission, 0.138);
    assert_eq!(r.position_side, "long");
    assert_eq!(r.realized_pnl, 2.0);
    assert!(!r.maker);
}
#[test]
fn fills_split_documented_seven_day_windows_and_filter_order() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: Some("wanted".into()),
            from_ms: Some(NOW),
            to_ms: Some(NOW + 8.0 * DAY),
            limit: None,
        },
    );
    let first = request(&actions, "/userTrades");
    assert!(
        first
            .url
            .contains(&format!("endTime={:.0}", NOW + 7.0 * DAY))
    );
    let actions = response(
        &mut e,
        NOW,
        first.id,
        &format!(
            r#"[{{"id":"wrong","orderId":"other","symbol":"BTCUSDT","side":"SELL","price":"1","qty":"1","time":{NOW}}}]"#
        ),
    );
    no_completion(&actions);
    let second = request(&actions, "/userTrades");
    assert!(
        second
            .url
            .contains(&format!("startTime={:.0}", NOW + 7.0 * DAY + 1.0))
    );
    let actions = response(&mut e, NOW, second.id, "[]");
    assert_eq!(result(&actions).unwrap(), "[]");
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicTrade {
    id: String,
    timestamp: f64,
    side: String,
    quote_qty: f64,
}
#[test]
fn public_trades_need_no_credentials_and_keep_milliseconds() {
    let mut e = support::setup::engine(Options::default());
    let actions = command(
        &mut e,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(2000),
        },
    );
    let req = request(&actions, "/market/trades");
    assert!(req.url.ends_with("limit=1000&symbol=BTCUSDT"));
    assert!(req.headers.is_empty());
    let actions = response(
        &mut e,
        NOW,
        req.id,
        r#"[{"id":"123456789012345678","price":"69355.2","qty":"0.01","quoteQty":"693.552","time":1764505701456,"isBuyerMaker":true,"isBestMatch":true}]"#,
    );
    let rows: Vec<PublicTrade> = serde_json::from_str(&result(&actions).unwrap()).unwrap();
    assert_eq!(rows[0].id, "123456789012345678");
    assert_eq!(rows[0].timestamp, 1_764_505_701_456.0);
    assert_eq!(rows[0].side, "sell");
    assert_eq!(rows[0].quote_qty, 693.552);
}
const NORMAL: &str = r#"[{"orderId":702345678901234567,"symbol":"BTCUSDT","side":"BUY","positionSide":"LONG","status":"FILLED","type":"LIMIT","time":1764400000000,"price":"65000","origQty":"0.01","executedQty":"0.01","avgPrice":"64999.5","updateTime":1764400001234}]"#;
const ALGO: &str = r#"{"orders":[{"algoId":812345678901234500,"algoType":"PLAN","orderType":"STOP","symbol":"BTCUSDT","side":"BUY","positionSide":"LONG","quantity":"0.01","algoStatus":"TRIGGERED","actualOrderId":702345678901234700,"actualPrice":"68850","triggerPrice":"68900","price":"68800","workingType":"MARK_PRICE","createTime":1764505800123,"updateTime":1764506000456}],"hasMore":false}"#;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    status: String,
    average_price: Option<f64>,
    created_time: f64,
}
#[test]
fn normal_and_algo_history_commit_together_in_binance_result_shape() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let req = request(&actions, "/order/history");
    assert!(req.url.contains("page=0"));
    let actions = response(&mut e, NOW, req.id, NORMAL);
    no_completion(&actions);
    let req = request(&actions, "/allAlgoOrders");
    assert!(!req.url.contains("page="));
    let actions = response(&mut e, NOW, req.id, ALGO);
    let rows: Vec<HistoricalOrder> = serde_json::from_str(&result(&actions).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "812345678901234500");
    assert_eq!(rows[0].kind, "conditional");
    assert_eq!(rows[0].status, "closed");
    assert_eq!(rows[0].created_time, 1_764_505_800_123.0);
    assert_eq!(rows[1].kind, "limit");
    assert_eq!(rows[1].average_price, Some(64999.5));
}
#[test]
fn failed_algo_history_retains_valid_normal_rows_with_owned_incompleteness() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let actions = response(&mut e, NOW, request(&actions, "/order/history").id, NORMAL);
    let id = request(&actions, "/allAlgoOrders").id;
    e.handle(
        NOW,
        Event::HttpFailed {
            request: id,
            error: "connection reset",
        },
    );
    let actions: Vec<_> = support::managed::drain(&mut e, NOW);
    owned_income_partial(&actions, 1);
}
#[test]
fn income_post_preserves_ts_omission_and_explicit_null_contract() {
    let mut e = engine();
    let actions = command(&mut e, income(None, None));
    let req = request(&actions, "/account/income");
    assert_eq!(req.method, Method::Post);
    assert_eq!(
        req.body.as_deref(),
        Some(
            r#"{"asset":"","symbol":"","incomeType":"","startTime":null,"endTime":null,"limit":1}"#
        )
    );
    let actions = response(&mut e, NOW, req.id, r#"{"items":[]}"#);
    assert_eq!(result(&actions).unwrap(), "[]");
    let actions = command(
        &mut e,
        Query::Income {
            symbol: Some("BTCUSDT".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let req = request(&actions, "/account/income");
    assert_eq!(
        req.body.as_deref(),
        Some(
            r#"{"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","startTime":null,"endTime":null,"limit":100}"#
        )
    );
}
fn bill(id: u64, t: f64) -> String {
    format!(
        r#"{{"billId":{id},"asset":"USDT","symbol":"BTCUSDT","income":"0.08266646","incomeType":"position_funding","time":{t:.0}}}"#
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomeBody {
    start_time: Option<u64>,
    end_time: Option<u64>,
    limit: u32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomeResult {
    tran_id: String,
    timestamp: f64,
    category: String,
}
#[test]
fn income_walks_beyond_short_pages_paces_and_deduplicates_large_ids() {
    let mut e = engine();
    let mut now = NOW;
    let actions = command(&mut e, income(Some(NOW), Some(NOW + 1000.0)));
    let req = request(&actions, "/account/income");
    let body: IncomeBody = serde_json::from_str(req.body.as_ref().unwrap()).unwrap();
    assert_eq!(body.limit, 100);
    assert_eq!(body.start_time, Some(NOW as u64));
    let a = bill(686960019383517338, NOW + 800.0);
    let b = bill(686960019383517339, NOW + 700.0);
    let c = bill(686960019383517340, NOW + 500.0);
    let actions = response(&mut e, now, req.id, &format!(r#"{{"items":[{a},{b}]}}"#));
    no_completion(&actions);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let actions = advance(&mut e, &mut now, actions);
    let req = request(&actions, "/account/income");
    let body: IncomeBody = serde_json::from_str(req.body.as_ref().unwrap()).unwrap();
    assert_eq!(body.end_time, Some((NOW + 700.0) as u64));
    let actions = response(&mut e, now, req.id, &format!(r#"{{"items":[{b},{c}]}}"#));
    let actions = advance(&mut e, &mut now, actions);
    let req = request(&actions, "/account/income");
    let actions = response(&mut e, now, req.id, &format!(r#"{{"items":[{c}]}}"#));
    let actions = advance(&mut e, &mut now, actions);
    let rows: Vec<IncomeResult> = serde_json::from_str(&result(&actions).unwrap()).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].tran_id, "686960019383517338");
    assert_eq!(rows[2].timestamp, NOW + 500.0);
    assert_eq!(rows[0].category, "other");
}
#[test]
fn failed_income_first_chunk_rejects_and_stops_the_range() {
    let mut e = engine();
    let actions = command(&mut e, income(Some(NOW), Some(NOW + 200.0 * DAY)));
    let req = request(&actions, "/account/income");
    let body: IncomeBody = serde_json::from_str(req.body.as_ref().unwrap()).unwrap();
    assert_eq!(body.end_time, Some((NOW + 100.0 * DAY) as u64));
    owned_income_failure(&failed_income_page(&mut e, req, 0), 7);
}

#[test]
fn malformed_streamed_fill_rows_fail_the_whole_query() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let req = request(&actions, "/userTrades");
    let actions = response(
        &mut e,
        NOW,
        req.id,
        r#"[{"id":"1","orderId":"2","symbol":"BTCUSDT","side":"BUY","price":"oops","qty":"1","time":1764505701456}]"#,
    );
    assert!(result(&actions).is_err());
}
#[test]
fn private_queries_without_credentials_complete_without_http() {
    let mut e = support::setup::engine(Options::default());
    let actions = command(&mut e, income(Some(NOW), Some(NOW + 100.0)));
    assert_eq!(result(&actions).unwrap(), "[]");
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn income_page_budget_reports_incomplete_instead_of_silent_success() {
    let mut e = engine();
    let mut now = NOW;
    let mut actions = command(&mut e, income(Some(NOW), Some(NOW + 1000.0)));
    for page in 0..50 {
        let req = request(&actions, "/account/income");
        let a = bill(page * 2 + 1, NOW + (page * 2 + 1) as f64);
        let b = bill(page * 2 + 2, NOW + (page * 2 + 2) as f64);
        actions = response(&mut e, now, req.id, &format!(r#"{{"items":[{a},{b}]}}"#));
        actions = advance(&mut e, &mut now, actions);
    }
    let rows: Vec<IncomeResult> = serde_json::from_str(&result(&actions).unwrap()).unwrap();
    assert_eq!(rows.len(), 100);
    owned_incomplete(&actions, 7);
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Warn, message, .. })) if message.contains("INCOMPLETE"))));
}

fn ban(e: &mut Engine<WeexFutures>) {
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 99,
            query: Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: None,
            },
        }),
    );
    let actions: Vec<_> = support::managed::drain(e, NOW);
    e.handle(
        NOW,
        Event::HttpResponse {
            request: request(&actions, "/market/trades").id,
            response: Response {
                status: 429,
                headers: "retry-after: 10",
                body: r#"{"msg":"rate limit"}"#,
            },
        },
    );
    e.drain_actions().for_each(drop);
}
#[test]
fn initial_history_reads_wait_until_the_shared_ip_ban_expires() {
    for query in [
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: None,
        },
        income(Some(NOW), None),
    ] {
        let mut e = engine();
        ban(&mut e);
        let actions = command(&mut e, query);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        no_completion(&actions);
        let mut now = NOW;
        let actions = advance(&mut e, &mut now, actions);
        assert_eq!(now, NOW + 10_000.0);
        assert!(actions.iter().any(|a| matches!(a, Action::Http(_))));
        if let Some(Action::Http(r)) = actions
            .iter()
            .find(|a| matches!(a,Action::Http(r) if r.url.ends_with("/account/income")))
        {
            let body: IncomeBody = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
            assert_eq!(body.start_time, Some(NOW as u64));
            assert_eq!(body.end_time, None);
        }
    }
}
#[test]
fn order_history_continuation_waits_for_a_ban_from_another_query() {
    let mut e = engine();
    let actions = command(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let normal = request(&actions, "/order/history");
    ban(&mut e);
    let actions = response(&mut e, NOW, normal.id, NORMAL);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    no_completion(&actions);
    let mut now = NOW;
    let actions = advance(&mut e, &mut now, actions);
    assert_eq!(now, NOW + 10_000.0);
    let req = request(&actions, "/allAlgoOrders");
    let actions = response(&mut e, now, req.id, ALGO);
    assert_eq!(
        serde_json::from_str::<Vec<serde_json::Value>>(&result(&actions).unwrap())
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn income_without_bill_ids_uses_time_in_the_deduplication_identity() {
    let mut e = engine();
    let mut now = NOW;
    let actions = command(&mut e, income(Some(NOW), Some(NOW + 1000.0)));
    let req = request(&actions, "/account/income");
    let a = bill(0, NOW + 800.0).replace("\"billId\":0,", "");
    let b = bill(0, NOW + 700.0).replace("\"billId\":0", "\"billId\":\"\"");
    let actions = response(&mut e, now, req.id, &format!(r#"{{"items":[{a},{b}]}}"#));
    let actions = advance(&mut e, &mut now, actions);
    let req = request(&actions, "/account/income");
    let actions = response(&mut e, now, req.id, &format!(r#"{{"items":[{b}]}}"#));
    let actions = advance(&mut e, &mut now, actions);
    let rows: Vec<IncomeResult> = serde_json::from_str(&result(&actions).unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].tran_id, "");
    assert_ne!(rows[0].timestamp, rows[1].timestamp);
}
#[test]
fn pathological_income_ranges_are_rejected_before_network_io() {
    let mut e = engine();
    let actions = command(&mut e, income(Some(0.0), Some(9_007_199_254_740_991.0)));
    assert!(result(&actions).unwrap_err().contains("budget"));
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn positive_income_cursor_never_reports_a_short_repeated_span_as_complete() {
    let mut e = engine();
    let mut now = NOW;
    let mut actions = command(&mut e, income(Some(NOW), Some(NOW + 1000.0)));
    let mut warned = false;
    let mut completed = false;
    for _ in 0..10 {
        warned|=actions.iter().any(|action|matches!(action,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("INCOMPLETE")));
        if actions
            .iter()
            .any(|action| matches!(action, Action::Notice(Notice::CommandDone { .. })))
        {
            let rows: Vec<IncomeResult> = serde_json::from_str(&result(&actions).unwrap()).unwrap();
            assert_eq!(rows.len(), 1);
            completed = true;
            break;
        }
        if actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
        {
            let req = request(&actions, "/account/income");
            let row = bill(42, NOW + 500.0);
            actions = response(
                &mut e,
                now,
                req.id,
                &format!(
                    r#"{{"hasNextPage":true,"nextKey":{{"nextKeyId":42,"nextKeyTime":{}}},"items":[{row}]}}"#,
                    NOW + 500.0
                ),
            );
        } else {
            actions = advance(&mut e, &mut now, actions);
        }
    }
    assert!(completed && warned);
}
#[test]
fn non_null_income_cursor_warns_even_when_has_next_page_is_false() {
    let mut e = engine();
    let actions = command(&mut e, income(None, None));
    let req = request(&actions, "/account/income");
    let actions = response(
        &mut e,
        NOW,
        req.id,
        r#"{"hasNextPage":false,"nextKey":{"nextKeyId":42},"items":[]}"#,
    );
    assert_eq!(result(&actions).unwrap(), "[]");
    assert!(actions.iter().any(|action|matches!(action,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("INCOMPLETE"))));
    owned_incomplete(&actions, 7);
}

#[test]
fn later_income_failure_emits_native_owned_fixture_before_partial_success_at_error_only_level() {
    let mut e = engine();
    e.set_log_level(safe_cex_engine::Level::Error);
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 1,
            query: income(Some(NOW), Some(NOW + 1000.0)),
        }),
    );
    let actions: Vec<_> = support::managed::drain(&mut e, NOW);
    let req = request(&actions, "/account/income");
    let actions = response(&mut e, NOW, req.id, &full_income_page());
    let mut now = NOW;
    let actions = advance(&mut e, &mut now, actions);
    let req = request(&actions, "/account/income");
    e.handle(
        NOW,
        Event::HttpFailed {
            request: req.id,
            error: "page reset",
        },
    );
    let actions: Vec<_> = support::managed::drain(&mut e, NOW);
    owned_incomplete(&actions, 1);
    let event = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::Event(e))
                if e.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some(e)
            }
            _ => None,
        })
        .unwrap();
    let [("command", safe_cex_engine::Value::Num(command))] = event.fields.as_slice() else {
        panic!("actual native ownership must be exact")
    };
    let actual = serde_json::json!({"op":"event","level":event.level.as_str(),"code":event.code,"message":event.message,"fields":{"command":command}});
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/incomplete-query-event.json")).unwrap();
    assert_eq!(
        actual, fixture,
        "host/worker fixture must match the real venue event"
    );
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone {command:1,result}) if serde_json::from_str::<Vec<serde_json::Value>>(result).unwrap().len()==100)));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
    );
}
#[test]
fn lifetime_income_budget_marker_is_owned_before_retained_rows_finish() {
    let mut e = engine();
    let mut now = NOW;
    // Exercise the native 10,000-request budget with an explicit caller wait
    // large enough for the unchanged 100 ms page/chunk pacing.
    e.handle_command(
        NOW,
        Command::Query {
            command: 7,
            query: income(Some(NOW), Some(NOW + 5001.0 * 100.0 * DAY)),
        },
        Some(2_000_000),
    )
    .unwrap();
    let mut actions = support::managed::drain(&mut e, NOW);
    let mut requests = 0;
    loop {
        if actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
        {
            assert_eq!(requests, 10_000);
            owned_incomplete(&actions, 7);
            assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::Event(e)) if e.message.contains("lifetime request budget"))));
            let rows: Vec<IncomeResult> = serde_json::from_str(&result(&actions).unwrap()).unwrap();
            assert_eq!(rows.len(), 5000);
            break;
        }
        if actions.iter().any(|a| matches!(a, Action::Http(_))) {
            let req = request(&actions, "/account/income");
            requests += 1;
            assert!(requests <= 10_000);
            let body: IncomeBody = serde_json::from_str(req.body.as_ref().unwrap()).unwrap();
            let response_body = if requests % 2 == 1 {
                format!(
                    r#"{{"items":[{}]}}"#,
                    bill(requests, body.end_time.unwrap() as f64 - 1.0)
                )
            } else {
                r#"{"items":[]}"#.to_owned()
            };
            actions = response(&mut e, now, req.id, &response_body);
        } else {
            actions = advance(&mut e, &mut now, actions);
        }
    }
}

fn fill_identity_row(field: &str, id: &str, symbol: &str, time: f64) -> String {
    format!(
        r#"{{"{field}":"{id}","orderId":"order","symbol":"{symbol}","side":"BUY","price":"69000","qty":"0.01","commission":"-0.138","commissionAsset":"USDT","positionSide":"LONG","realizedPnl":"2","maker":false,"time":{time}}}"#
    )
}
fn identity_fills(from: Option<f64>, to: Option<f64>) -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: Some("order".into()),
        from_ms: from,
        to_ms: to,
        limit: None,
    }
}
fn identity_marker_count(actions: &[Action], code: &str) -> usize {
    actions
        .iter()
        .filter(|a| matches!(a,Action::Notice(Notice::Event(event))if event.code==code))
        .count()
}

fn list_query(kind: u8) -> Query {
    match kind {
        0 => identity_fills(Some(NOW), Some(NOW + 8.0 * DAY)),
        1 => Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        _ => Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: None,
        },
    }
}
fn list_failure(e: &mut Engine<WeexFutures>, r: HttpRequest, mode: u8) -> Vec<Action> {
    if mode == 0 {
        e.handle(
            NOW,
            Event::HttpFailed {
                request: r.id,
                error: "list reset",
            },
        );
        return support::managed::drain(e, NOW);
    }
    if mode == 1 {
        e.handle(
            NOW,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 503,
                    headers: "",
                    body: "unavailable",
                },
            },
        );
        return support::managed::drain(e, NOW);
    }
    let (prefix, algo) = if r.url.contains("allAlgoOrders") {
        (
            serde_json::from_str::<serde_json::Value>(ALGO).unwrap()["orders"][0].to_string(),
            true,
        )
    } else if r.url.contains("market/trades") {
        (
            format!(
                r#"{{"id":"failed-prefix","price":"69000","qty":"1","quoteQty":"69000","time":{NOW},"isBuyerMaker":true}}"#
            ),
            false,
        )
    } else if r.url.contains("order/history") {
        (
            serde_json::from_str::<serde_json::Value>(NORMAL).unwrap()[0].to_string(),
            false,
        )
    } else {
        (
            fill_identity_row("id", "failed-prefix", "BTCUSDT", NOW + 8.0 * DAY),
            false,
        )
    };
    let body = match mode {
        2 if algo => format!(r#"{{"orders":[{prefix},null]}}"#),
        2 => format!("[{prefix},null]"),
        3 => r#"{"code":40014,"msg":"native refusal"}"#.to_owned(),
        4 if algo => r#"{"orders":null}"#.to_owned(),
        4 => "{}".to_owned(),
        5 if algo => format!(r#"{{"orders":[{prefix},"#),
        5 => format!("[{prefix},"),
        _ => unreachable!(),
    };
    response(e, NOW, r.id, &body)
}
#[test]
fn owned_lists_first_failure_is_typed_without_false_complete_history() {
    for kind in 0..3 {
        for mode in 0..6 {
            let mut e = engine();
            e.set_log_level(safe_cex_engine::Level::Error);
            let actions = command(&mut e, list_query(kind));
            let r = request(
                &actions,
                if kind == 0 {
                    "userTrades"
                } else if kind == 1 {
                    "order/history"
                } else {
                    "market/trades"
                },
            );
            owned_income_failure(&list_failure(&mut e, r, mode), 7);
        }
    }
}
#[test]
fn owned_lists_later_window_or_algo_failure_returns_only_accepted_rows() {
    for kind in 0..2 {
        for mode in 0..6 {
            let mut e = engine();
            e.set_log_level(safe_cex_engine::Level::Error);
            let actions = command(&mut e, list_query(kind));
            let r = request(
                &actions,
                if kind == 0 {
                    "userTrades"
                } else {
                    "order/history"
                },
            );
            let body = if kind == 0 {
                format!("[{}]", fill_identity_row("id", "kept", "BTCUSDT", NOW))
            } else {
                NORMAL.into()
            };
            let actions = response(&mut e, NOW, r.id, &body);
            no_completion(&actions);
            let r = request(
                &actions,
                if kind == 0 {
                    "userTrades"
                } else {
                    "allAlgoOrders"
                },
            );
            let actions = list_failure(&mut e, r, mode);
            owned_income_partial(&actions, 1);
            assert!(!result(&actions).unwrap().contains("failed-prefix"));
        }
    }
}
#[test]
fn owned_lists_empty_or_filtered_prior_window_cannot_authorize_partial_success() {
    for filtered in [false, true] {
        let mut e = engine();
        let actions = command(&mut e, list_query(0));
        let r = request(&actions, "userTrades");
        let body = if filtered {
            format!("[{}]", fill_identity_row("id", "other", "ETHUSDT", NOW))
        } else {
            "[]".into()
        };
        let actions = response(&mut e, NOW, r.id, &body);
        let r = request(&actions, "userTrades");
        owned_income_failure(&list_failure(&mut e, r, 0), 7);
    }
}
#[test]
fn owned_missing_fill_fact_waits_for_valid_page_and_final_success() {
    let mut e = engine();
    let actions = command(&mut e, list_query(0));
    let r = request(&actions, "userTrades");
    let body = format!(
        "[{},{}]",
        fill_identity_row("id", "kept", "BTCUSDT", NOW),
        fill_identity_row("id", "", "BTCUSDT", NOW)
    );
    let actions = response(&mut e, NOW, r.id, &body);
    assert_eq!(
        identity_marker_count(&actions, safe_cex_engine::log::code::HISTORY_INCOMPLETE),
        0,
        "no coverage publication before final success"
    );
    assert_eq!(
        identity_marker_count(&actions, safe_cex_engine::log::code::ROW_SKIPPED),
        1
    );
    let r = request(&actions, "userTrades");
    let actions = response(&mut e, NOW, r.id, "[]");
    owned_income_partial(&actions, 1);
}
#[test]
fn owned_failed_fill_page_cannot_publish_or_consume_a_missing_id_fact() {
    let mut e = engine();
    let actions = command(&mut e, identity_fills(None, None));
    let r = request(&actions, "userTrades");
    let body = format!("[{},null]", fill_identity_row("id", "", "BTCUSDT", NOW));
    let actions = response(&mut e, NOW, r.id, &body);
    assert_eq!(
        identity_marker_count(&actions, safe_cex_engine::log::code::ROW_SKIPPED),
        0
    );
    owned_income_failure(&actions, 7);
    let actions = command(&mut e, identity_fills(None, None));
    let r = request(&actions, "userTrades");
    let actions = response(
        &mut e,
        NOW,
        r.id,
        &format!("[{}]", fill_identity_row("id", "", "BTCUSDT", NOW)),
    );
    owned_income_partial(&actions, 0);
    assert_eq!(
        identity_marker_count(&actions, safe_cex_engine::log::code::ROW_SKIPPED),
        1
    );
}
#[test]
fn owned_algo_advertised_more_is_a_validated_gap_even_without_rows() {
    for normal in [false, true] {
        let mut e = engine();
        e.set_log_level(safe_cex_engine::Level::Error);
        let actions = command(&mut e, list_query(1));
        let r = request(&actions, "order/history");
        let actions = response(&mut e, NOW, r.id, if normal { NORMAL } else { "[]" });
        let r = request(&actions, "allAlgoOrders");
        let actions = response(&mut e, NOW, r.id, r#"{"orders":[],"hasMore":true}"#);
        owned_income_partial(&actions, usize::from(normal));
    }
}
#[test]
fn owned_algo_malformed_continuation_metadata_excludes_the_whole_page() {
    for metadata in ["null", "\"yes\"", "[]", "1"] {
        let mut e = engine();
        let actions = command(&mut e, list_query(1));
        let r = request(&actions, "order/history");
        let actions = response(&mut e, NOW, r.id, NORMAL);
        let r = request(&actions, "allAlgoOrders");
        let body = ALGO.replace("\"hasMore\":false", &format!("\"hasMore\":{metadata}"));
        let actions = response(&mut e, NOW, r.id, &body);
        owned_income_partial(&actions, 1);
    }
}
#[test]
fn public_list_keeps_duplicate_ids_and_native_order() {
    let mut e = engine();
    let actions = command(&mut e, list_query(2));
    let r = request(&actions, "market/trades");
    let row = |id: &str, time: f64| {
        format!(
            r#"{{"id":"{id}","price":"10","qty":"1","quoteQty":"10","time":{time},"isBuyerMaker":false}}"#
        )
    };
    let actions = response(
        &mut e,
        NOW,
        r.id,
        &format!(
            "[{},{},{}]",
            row("same", NOW),
            row("other", NOW + 1.0),
            row("same", NOW)
        ),
    );
    let rows: serde_json::Value = serde_json::from_str(&result(&actions).unwrap()).unwrap();
    assert_eq!(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["same", "other", "same"]
    );
}

#[test]
fn native_list_windows_preserve_the_legacy_ratio_admission_boundary() {
    let span = 7.0 * DAY;
    for over in [false, true] {
        let mut e = engine();
        let actions = command(
            &mut e,
            identity_fills(
                Some(NOW),
                Some(NOW + 1000.0 * span + if over { 0.5 } else { 0.0 }),
            ),
        );
        if over {
            assert!(result(&actions).unwrap_err().contains("budget"));
            assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        } else {
            let req = request(&actions, "userTrades");
            assert!(req.url.contains(&format!("startTime={NOW:.0}")));
            assert!(req.url.contains(&format!("endTime={:.0}", NOW + span)));
            no_completion(&actions);
        }
    }
}
#[test]
fn empty_fill_identity_aliases_keep_valid_financial_peers_and_mark_owned_incomplete_once() {
    for field in ["id", "tradeId", "fillId"] {
        let mut e = engine();
        let actions = command(&mut e, identity_fills(None, None));
        let r = request(&actions, "/userTrades");
        let good = fill_identity_row("id", "good", "BTCUSDT", NOW);
        let empty = fill_identity_row(field, "", "BTCUSDT", NOW);
        let actions = response(&mut e, NOW, r.id, &format!("[{good},{empty},{empty}]"));
        let rows: serde_json::Value = serde_json::from_str(&result(&actions).unwrap()).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["id"], "good");
        assert_eq!(rows[0]["price"], 69000.0);
        assert_eq!(rows[0]["qty"], 0.01);
        assert_eq!(rows[0]["commission"], 0.138);
        assert_eq!(rows[0]["realizedPnl"], 2.0);
        owned_incomplete(&actions, 7);
        assert_eq!(
            identity_marker_count(&actions, safe_cex_engine::log::code::HISTORY_INCOMPLETE),
            1
        );
        assert_eq!(
            identity_marker_count(&actions, safe_cex_engine::log::code::ROW_SKIPPED),
            1
        );
        let skipped = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::ROW_SKIPPED =>
                {
                    Some(event)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(skipped.level, safe_cex_engine::Level::Warn);
        assert!(
            skipped
                .fields
                .contains(&("field", safe_cex_engine::Value::Str("id".into())))
        );
        assert!(skipped.message.contains("id"));
    }
}
#[test]
fn missing_fill_identity_warning_is_per_query_across_windows_and_other_queries_stay_clean() {
    let mut e = engine();
    let actions = command(&mut e, identity_fills(Some(NOW), Some(NOW + 8.0 * DAY)));
    let first = request(&actions, "/userTrades");
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 8,
            query: identity_fills(None, None),
        }),
    );
    let other: Vec<_> = support::managed::drain(&mut e, NOW);
    let other = request(&other, "/userTrades");
    let body = format!(
        "[{},{}]",
        fill_identity_row("id", "first", "BTCUSDT", NOW),
        fill_identity_row("id", "", "BTCUSDT", NOW)
    );
    let mut all = response(&mut e, NOW, first.id, &body);
    no_completion(&all);
    let second = request(&all, "/userTrades");
    let clean = response(
        &mut e,
        NOW,
        other.id,
        &format!("[{}]", fill_identity_row("id", "other", "BTCUSDT", NOW)),
    );
    assert_eq!(
        identity_marker_count(&clean, safe_cex_engine::log::code::HISTORY_INCOMPLETE),
        0
    );
    assert!(
        clean
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 8, .. })))
    );
    let body = format!(
        "[{},{}]",
        fill_identity_row("id", "second", "BTCUSDT", NOW + 8.0 * DAY),
        fill_identity_row("fillId", "", "BTCUSDT", NOW + 8.0 * DAY)
    );
    let final_actions = response(&mut e, NOW, second.id, &body);
    let rows: serde_json::Value = serde_json::from_str(&result(&final_actions).unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    all.extend(final_actions);
    owned_incomplete(&all, 7);
    assert_eq!(
        identity_marker_count(&all, safe_cex_engine::log::code::HISTORY_INCOMPLETE),
        1
    );
    assert_eq!(
        identity_marker_count(&all, safe_cex_engine::log::code::ROW_SKIPPED),
        1
    );
    // A new query owns a fresh warning budget, even when every row is skipped.
    let actions = command(&mut e, identity_fills(None, None));
    let r = request(&actions, "/userTrades");
    let actions = response(
        &mut e,
        NOW,
        r.id,
        &format!("[{}]", fill_identity_row("id", "", "BTCUSDT", NOW)),
    );
    assert_eq!(result(&actions).unwrap(), "[]");
    owned_incomplete(&actions, 7);
    assert_eq!(
        identity_marker_count(&actions, safe_cex_engine::log::code::ROW_SKIPPED),
        1
    );
}
#[test]
fn genuine_empty_or_filtered_identity_rows_do_not_mark_incomplete_and_malformed_fills_still_fail() {
    for body in [
        "[]".to_owned(),
        format!("[{}]", fill_identity_row("id", "", "ETHUSDT", NOW)),
        format!(
            "[{}]",
            fill_identity_row("id", "", "BTCUSDT", NOW)
                .replace("\"orderId\":\"order\"", "\"orderId\":\"other\"")
        ),
        format!("[{}]", fill_identity_row("id", "", "BTCUSDT", NOW - 1.0)),
        format!("[{}]", fill_identity_row("id", "", "BTCUSDT", NOW + 1.0)),
    ] {
        let mut e = engine();
        let actions = command(&mut e, identity_fills(Some(NOW), Some(NOW)));
        let r = request(&actions, "/userTrades");
        let actions = response(&mut e, NOW, r.id, &body);
        assert_eq!(result(&actions).unwrap(), "[]");
        assert_eq!(
            identity_marker_count(&actions, safe_cex_engine::log::code::HISTORY_INCOMPLETE),
            0
        );
        assert_eq!(
            identity_marker_count(&actions, safe_cex_engine::log::code::ROW_SKIPPED),
            0
        );
    }
    let mut e = engine();
    let actions = command(&mut e, identity_fills(None, None));
    let r = request(&actions, "/userTrades");
    let malformed =
        fill_identity_row("id", "", "BTCUSDT", NOW).replace("\"69000\"", "\"malformed\"");
    let actions = response(&mut e, NOW, r.id, &format!("[{malformed}]"));
    assert!(
        result(&actions).is_err(),
        "typed malformed financial row retains strict failure"
    );
}
#[test]
fn empty_fill_identity_semantic_marker_survives_error_only_logging_before_success() {
    let mut e = engine();
    e.set_log_level(safe_cex_engine::Level::Error);
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 1,
            query: identity_fills(None, None),
        }),
    );
    let actions: Vec<_> = support::managed::drain(&mut e, NOW);
    let r = request(&actions, "/userTrades");
    let body = format!(
        "[{},{}]",
        fill_identity_row("id", "good", "BTCUSDT", NOW),
        fill_identity_row("id", "", "BTCUSDT", NOW)
    );
    let actions = response(&mut e, NOW, r.id, &body);
    owned_incomplete(&actions, 1);
    assert_eq!(
        identity_marker_count(&actions, safe_cex_engine::log::code::ROW_SKIPPED),
        0
    );
    let event = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some(event)
            }
            _ => None,
        })
        .unwrap();
    let [("command", safe_cex_engine::Value::Num(command))] = event.fields.as_slice() else {
        panic!("exact native query owner")
    };
    let actual = serde_json::json!({"op":"event","level":event.level.as_str(),"code":event.code,"message":event.message,"fields":{"command":command}});
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/empty-fill-incomplete-query-event.json"
    ))
    .unwrap();
    assert_eq!(
        actual, fixture,
        "host/worker fixture must equal actual native marker"
    );
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone{command:1,result})if result.contains("good"))));
}

#[test]
fn provider_refusal_after_an_accepted_page_rejects_strictly_without_partial_rows() {
    let mut e = support::setup::engine_with_provider(
        Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("passphrase".into()),
            ..Options::default()
        },
        |inner| std::sync::Arc::new(support::setup::Gated::new(inner, &[true, false])),
    );
    let actions = command(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    let actions = response(&mut e, NOW, request(&actions, "/order/history").id, NORMAL);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/allAlgoOrders"))),
        "the refused page is never released"
    );
    let error = result(&actions).unwrap_err();
    assert!(!error.is_empty());
}
