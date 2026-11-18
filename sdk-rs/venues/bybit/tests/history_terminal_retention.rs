//! V5 order-history terminal statuses are available only for the last 24 hours.
//! https://bybit-exchange.github.io/docs/v5/order/order-list
mod support;

use safe_cex_engine::{Command, Method, Notice, Query, Value};
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

fn check_incomplete(h: &Harness, command: u64, incomplete: bool) {
    let gaps: Vec<_> = h
        .notices
        .iter()
        .filter_map(|notice| match notice {
            Notice::Event(event)
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                Some(event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(gaps.len(), usize::from(incomplete), "retention coverage");
    for gap in gaps {
        assert!(
            gap.fields
                .contains(&("command", Value::Num(command as f64)))
        );
        assert!(
            gap.fields
                .contains(&("reason", Value::Str("retentionLimit".into())))
        );
        assert!(gap.message.contains("24 hours"));
        assert!(gap.message.contains("Cancelled/Rejected/Deactivated"));
    }
}

fn exercise(
    from_age: Option<f64>,
    to_age: Option<f64>,
    expected_statuses: usize,
    incomplete: bool,
) {
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
            if let (Some(start), Some(end)) = (start, end) {
                assert!(end - start < 7.0 * DAY, "terminal page exceeds V5 range");
                assert!(start <= now, "wholly future status slice must be omitted");
            }
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
    check_incomplete(&h, 901, incomplete);
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
    exercise(Some(7.0 * DAY), Some(0.0), 3, true);
}

#[test]
fn older_window_omits_terminal_passes_but_keeps_default_history() {
    exercise(Some(14.0 * DAY), Some(7.0 * DAY), 0, true);
}

#[test]
fn start_only_history_clips_terminal_passes() {
    exercise(Some(7.0 * DAY), None, 3, true);
}

#[test]
fn recent_window_keeps_all_three_terminal_statuses() {
    exercise(Some(DAY / 2.0), Some(0.0), 3, false);
}

#[test]
fn unbounded_history_keeps_default_query_and_terminal_passes() {
    exercise(None, None, 3, true);
}

#[test]
fn end_only_older_history_omits_terminal_passes() {
    exercise(None, Some(2.0 * DAY), 0, true);
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

fn check_terminal_wire_clock(offset: f64, explicit: bool) {
    let mut h = ready();
    h.advance(offset);
    let clock = h.engine.signing_clock(h.now).0;
    assert_eq!(clock.fract(), offset);
    let from = explicit.then_some(clock - DAY / 2.0);
    let to = explicit.then_some(clock - 1000.0);
    h.command(Command::Query {
        command: 903,
        query: Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: from,
            to_ms: to,
            limit: None,
        },
    });
    let default = h.take("/v5/order/history");
    assert_eq!(
        query_value(&default.url, "startTime").map(str::to_owned),
        from.map(|value| value.to_string()),
        "default query keeps its original caller bounds"
    );
    assert_eq!(
        query_value(&default.url, "endTime").map(str::to_owned),
        to.map(|value| value.to_string())
    );
    h.reply(default.id, 200, EMPTY);
    for status in ["Cancelled", "Rejected", "Deactivated"] {
        let request = h.take(&format!("orderStatus={status}"));
        let start = query_value(&request.url, "startTime")
            .unwrap()
            .parse::<u64>()
            .expect("terminal startTime must be integer milliseconds");
        let end = query_value(&request.url, "endTime")
            .unwrap()
            .parse::<u64>()
            .expect("terminal endTime must be integer milliseconds");
        assert_eq!(start, from.unwrap_or(clock - DAY) as u64);
        assert_eq!(end, to.unwrap_or(clock) as u64);
        h.reply(request.id, 200, EMPTY);
    }
    assert_eq!(h.result(903), Some(&Ok("[]".into())));
}

#[test]
fn fractional_protocol_clock_uses_integer_terminal_bounds() {
    check_terminal_wire_clock(0.5, false);
}

#[test]
fn fractional_caller_bounds_are_normalized_only_for_terminal_queries() {
    check_terminal_wire_clock(0.5, true);
}

#[test]
fn integer_protocol_clock_keeps_the_existing_terminal_bounds() {
    check_terminal_wire_clock(0.0, false);
}

#[test]
fn replay_window_retains_explicit_end_and_all_terminal_cursor_rows() {
    let mut h = ready();
    let now = h.engine.signing_clock(h.now).0;
    let from = now - 2600.0;
    let to = from + 60_000.0;
    h.command(Command::Query {
        command: 904,
        query: Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(from),
            to_ms: Some(to),
            limit: None,
        },
    });
    for (index, status) in ["Filled", "Cancelled", "Rejected", "Deactivated"]
        .into_iter()
        .enumerate()
    {
        let request = h.take("/v5/order/history");
        assert_eq!(
            query_value(&request.url, "endTime"),
            Some(to.to_string().as_str())
        );
        assert_eq!(
            query_value(&request.url, "startTime"),
            Some(from.to_string().as_str())
        );
        assert_eq!(
            query_value(&request.url, "orderStatus"),
            (index > 0).then_some(status)
        );
        let row = serde_json::json!({
            "orderId": format!("history-{status}"), "symbol": "BTCUSDT", "side": "Buy",
            "orderType": "Limit", "orderStatus": status, "qty": "1",
            "cumExecQty": if index == 0 { "1" } else { "0" }, "price": "100",
            "createdTime": (from + index as f64).to_string()
        });
        let response = |cursor: &str| {
            serde_json::json!({
                "retCode": 0, "result": {"list": [row], "nextPageCursor": cursor}
            })
            .to_string()
        };
        h.reply(
            request.id,
            200,
            &response(if index == 0 { "" } else { "shared" }),
        );
        if index > 0 {
            let next = h.take("/v5/order/history");
            assert_eq!(query_value(&next.url, "cursor"), Some("shared"));
            assert_eq!(query_value(&next.url, "orderStatus"), Some(status));
            assert_eq!(
                query_value(&next.url, "endTime"),
                Some(to.to_string().as_str())
            );
            h.reply(next.id, 200, &response(""));
        }
    }
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(904).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 4);
    for status in ["Filled", "Cancelled", "Rejected", "Deactivated"] {
        assert!(
            rows.iter()
                .any(|row| row["id"] == format!("history-{status}"))
        );
    }
    assert!(!h.has_request("/v5/order/history"));
}

#[test]
fn wholly_future_window_keeps_default_read_without_terminal_passes() {
    let mut h = ready();
    let now = h.engine.signing_clock(h.now).0;
    h.command(Command::Query {
        command: 905,
        query: Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(now + 1000.0),
            to_ms: Some(now + 60_000.0),
            limit: None,
        },
    });
    h.respond("/v5/order/history", 200, EMPTY);
    assert_eq!(h.result(905), Some(&Ok("[]".into())));
    assert!(!h.has_request("/v5/order/history"));
}

#[test]
fn distant_future_end_retains_valid_past_status_pages_only() {
    exercise(Some(DAY / 2.0), Some(-14.0 * DAY), 3, false);
}

#[test]
fn expired_unfilled_orders_leave_explicit_incomplete_coverage() {
    let mut h = ready();
    let now = h.engine.signing_clock(h.now).0;
    let source = [
        ("old-filled", "Filled", now - 3.0 * DAY, "1"),
        ("expired-cancelled", "Cancelled", now - 2.0 * DAY, "0"),
        ("recent-cancelled", "Cancelled", now - DAY / 2.0, "0"),
    ];
    h.command(Command::Query {
        command: 906,
        query: Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(now - 6.0 * DAY),
            to_ms: Some(now),
            limit: None,
        },
    });
    let mut requests = 0;
    while h.result(906).is_none() {
        let request = h.take("/v5/order/history");
        assert_eq!(request.method, Method::Get);
        requests += 1;
        assert!(
            requests <= 4,
            "retention must not trigger extra recovery reads"
        );
        let status = query_value(&request.url, "orderStatus");
        let rows: Vec<_> = source
            .iter()
            .filter(|(_, native, at, _)| match status {
                None => *native == "Filled",
                Some(status) => status == *native && *at >= now - DAY,
            })
            .map(|(id, status, at, filled)| {
                serde_json::json!({
                    "orderId": id, "symbol": "BTCUSDT", "side": "Buy", "orderType": "Limit",
                    "orderStatus": status, "qty": "1", "cumExecQty": filled, "price": "100",
                    "createdTime": at.to_string()
                })
            })
            .collect();
        h.reply(
            request.id,
            200,
            &serde_json::json!({
                "retCode": 0, "result": {"list": rows, "nextPageCursor": ""}
            })
            .to_string(),
        );
    }
    assert_eq!(requests, 4);
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(906).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|row| row["id"] == "old-filled"));
    assert!(rows.iter().any(|row| row["id"] == "recent-cancelled"));
    check_incomplete(&h, 906, true);
}

#[test]
fn terminal_retention_boundary_is_explicit() {
    exercise(Some(DAY), Some(0.0), 3, false);
    exercise(Some(DAY + 1.0), Some(0.0), 3, true);
    exercise(None, Some(-7.0 * DAY), 3, false);
    exercise(Some(-1000.0), Some(-60_000.0), 0, false);
}
