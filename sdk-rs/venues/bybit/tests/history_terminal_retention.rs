//! V5 order-history terminal statuses are available only for the last 24 hours.
//! https://bybit-exchange.github.io/docs/v5/order/order-list
mod support;

use safe_cex_engine::{Command, Query};
use support::{Harness, Options};

const DAY: f64 = 86_400_000.0;
const EMPTY: &str = r#"{"retCode":0,"result":{"list":[],"nextPageCursor":""}}"#;
const REFUSED: &str =
    r#"{"retCode":10001,"retMsg":"Terminal order status outside the last 24 hours","result":{}}"#;

fn ready() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        ..Options::default()
    });
    for (path, body) in [
        ("/v5/account/fee-rate", support::FEE_RATES),
        ("/v5/account/info", support::ACCOUNT_INFO),
        ("status=PreLaunch", EMPTY),
    ] {
        if h.has_request(path) {
            h.respond(path, 200, body);
        }
    }
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    h.requests.clear();
    h
}

fn query_value<'a>(url: &'a str, name: &str) -> Option<&'a str> {
    url.split_once('?')?.1.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })
}

fn exercise(from_age: Option<f64>, to_age: Option<f64>, expected_statuses: usize) {
    let mut h = ready();
    let now = h.engine.signing_clock(h.now).0;
    let from = from_age.map(|age| now - age);
    let to = to_age.map(|age| now - age);
    h.command(Command::Query {
        command: 901,
        query: Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: from,
            to_ms: to,
            limit: None,
        },
    });
    let mut statuses = Vec::new();
    let mut default_windows = Vec::new();
    for _ in 0..50 {
        if h.result(901).is_some() {
            break;
        }
        let request = h.take("/v5/order/history");
        assert_eq!(query_value(&request.url, "category"), Some("linear"));
        assert_eq!(query_value(&request.url, "symbol"), Some("BTCUSDT"));
        assert_eq!(query_value(&request.url, "limit"), Some("50"));
        let start = query_value(&request.url, "startTime").map(|v| v.parse::<f64>().unwrap());
        let end = query_value(&request.url, "endTime").map(|v| v.parse::<f64>().unwrap());
        if let Some(status) = query_value(&request.url, "orderStatus") {
            statuses.push(status.to_owned());
            // Fake venue rejects the documented retention violation, not a local assertion.
            if start.is_some_and(|start| start < now - DAY)
                || end.is_some_and(|end| end < now - DAY)
            {
                h.reply(request.id, 200, REFUSED);
                continue;
            }
        } else {
            default_windows.push((start, end));
        }
        h.reply(request.id, 200, EMPTY);
    }
    assert_eq!(h.result(901), Some(&Ok("[]".into())), "{:?}", h.notices);
    assert_eq!(statuses.len(), expected_statuses);
    if expected_statuses > 0 {
        assert_eq!(statuses, ["Cancelled", "Rejected", "Deactivated"]);
    }
    assert!(!default_windows.is_empty());
    assert_eq!(default_windows.last().unwrap().0, from);
    assert_eq!(default_windows[0].1, to.or(from.map(|_| now)));
    for (start, end) in default_windows {
        if let (Some(start), Some(end)) = (start, end) {
            assert!(end - start < 7.0 * DAY);
        }
    }
}

#[test]
fn seven_day_backfill_clips_only_terminal_status_passes() {
    exercise(Some(7.0 * DAY), Some(0.0), 3);
}

#[test]
fn older_window_omits_terminal_passes_but_keeps_default_history() {
    exercise(Some(14.0 * DAY), Some(7.0 * DAY), 0);
}

#[test]
fn start_only_history_clips_terminal_passes() {
    exercise(Some(7.0 * DAY), None, 3);
}

#[test]
fn recent_window_keeps_all_three_terminal_statuses() {
    exercise(Some(DAY / 2.0), Some(0.0), 3);
}

#[test]
fn unbounded_history_keeps_default_query_and_terminal_passes() {
    exercise(None, None, 3);
}

#[test]
fn end_only_older_history_omits_terminal_passes() {
    exercise(None, Some(2.0 * DAY), 0);
}

#[test]
fn filled_orders_from_before_terminal_retention_are_still_returned() {
    let mut h = ready();
    let now = h.engine.signing_clock(h.now).0;
    h.command(Command::Query {
        command: 902,
        query: Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(now - 3.0 * DAY),
            to_ms: Some(now - 2.0 * DAY),
            limit: None,
        },
    });
    let request = h.take("/v5/order/history");
    assert!(!request.url.contains("orderStatus="));
    let body = serde_json::json!({
        "retCode": 0,
        "result": {"list": [{
            "orderId": "older-filled", "symbol": "BTCUSDT", "side": "Buy",
            "orderType": "Limit", "orderStatus": "Filled", "qty": "1",
            "cumExecQty": "1", "price": "100", "createdTime": (now - 2.5 * DAY).to_string()
        }], "nextPageCursor": ""}
    })
    .to_string();
    h.reply(request.id, 200, &body);
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(902).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "older-filled");
    assert_eq!(rows[0]["filled"], 1.0);
    assert!(!h.has_request("/v5/order/history"));
}
