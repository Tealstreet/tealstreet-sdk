//! Official futures userTrades, order/history, allAlgoOrders, income and trades payloads.
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, LogLevel, Method, Notice, Query, RequestId,
    Response,
};
use safe_cex_venue_weex::{Options, WeexFutures};
use serde::Deserialize;

const NOW: f64 = 1_764_505_700_000.0;
const DAY: f64 = 86_400_000.0;
fn engine() -> Engine<WeexFutures> {
    Engine::new(WeexFutures::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        ..Options::default()
    }))
}
fn command(e: &mut Engine<WeexFutures>, query: Query) -> Vec<Action> {
    e.handle(NOW, Event::Command(Command::Query { command: 7, query }));
    e.drain_actions().collect()
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
    e.drain_actions().collect()
}
fn result(actions: &[Action]) -> Result<String, String> {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 7, result }) => Some(result.clone()),
            _ => None,
        })
        .expect("query completion")
        .map_err(|error| error.message)
}
fn no_completion(actions: &[Action]) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
}
fn advance(e: &mut Engine<WeexFutures>, now: &mut f64, actions: Vec<Action>) -> Vec<Action> {
    let (timer, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("paced history timer");
    assert!(at >= *now + 100.0);
    *now = at;
    e.handle(*now, Event::TimerFired { timer });
    e.drain_actions().collect()
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
    let mut e = Engine::new(WeexFutures::new(Options::default()));
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
fn failed_half_of_order_history_never_returns_partial_success() {
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
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(result(&actions).unwrap_err(), "connection reset");
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
fn failed_income_chunk_continues_next_hundred_day_chunk_with_incomplete_warning() {
    let mut e = engine();
    let mut now = NOW;
    let actions = command(&mut e, income(Some(NOW), Some(NOW + 200.0 * DAY)));
    let req = request(&actions, "/account/income");
    let body: IncomeBody = serde_json::from_str(req.body.as_ref().unwrap()).unwrap();
    assert_eq!(body.end_time, Some((NOW + 100.0 * DAY) as u64));
    e.handle(
        now,
        Event::HttpFailed {
            request: req.id,
            error: "page reset",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::Log{level:LogLevel::Warning,message}) if message.contains("INCOMPLETE"))));
    let actions = advance(&mut e, &mut now, actions);
    let actions = advance(&mut e, &mut now, actions);
    let req = request(&actions, "/account/income");
    let body: IncomeBody = serde_json::from_str(req.body.as_ref().unwrap()).unwrap();
    assert_eq!(body.start_time, Some((NOW + 100.0 * DAY) as u64));
    let actions = response(&mut e, now, req.id, r#"{"items":[]}"#);
    let actions = advance(&mut e, &mut now, actions);
    assert_eq!(result(&actions).unwrap(), "[]");
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::Log{level:LogLevel::Warning,message}) if message.contains("INCOMPLETE")&&message.contains("1 chunk"))));
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
    let mut e = Engine::new(WeexFutures::new(Options::default()));
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
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::Log{level:LogLevel::Warning,message}) if message.contains("INCOMPLETE"))));
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
    let actions: Vec<_> = e.drain_actions().collect();
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
    request(&actions, "/allAlgoOrders");
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
        warned|=actions.iter().any(|action|matches!(action,Action::Notice(Notice::Log {message,..}) if message.contains("INCOMPLETE")));
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
    assert!(actions.iter().any(|action|matches!(action,Action::Notice(Notice::Log {message,..}) if message.contains("INCOMPLETE"))));
}
