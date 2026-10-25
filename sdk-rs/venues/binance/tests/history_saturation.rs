//! A saturated Binance history page is not proof of absence (#2128 finding):
//! fills read one page per 7-day window and orders one page, so a page that
//! fills its limit keeps its rows and marks the result incomplete.
mod support;
use safe_cex_engine::{Command, Event, Level, Query, Value};
use support::{Harness, Options};

const WEEK: f64 = 604_800_000.0;

fn harness() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.engine.set_log_level(Level::Error);
    h.requests.clear();
    h
}

fn fill(id: u32, time: f64) -> String {
    format!(
        r#"{{"id":{id},"symbol":"BTCUSDT","orderId":9,"price":"100","qty":"2","side":"BUY","time":{time}}}"#
    )
}

fn order(id: u32) -> String {
    format!(
        r#"{{"orderId":{id},"symbol":"BTCUSDT","status":"FILLED","type":"LIMIT","side":"BUY","price":"100","origQty":"2","executedQty":"2","time":{}}}"#,
        100 + id
    )
}

fn rows(h: &Harness, command: u32) -> usize {
    let result = h.result(command).expect("terminal").as_ref().expect("rows");
    serde_json::from_str::<Vec<serde_json::Value>>(result)
        .unwrap()
        .len()
}

fn marked(h: &Harness, command: u32) -> bool {
    h.events.iter().any(|event| {
        event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
            && event
                .fields
                .contains(&("command", Value::Num(f64::from(command))))
    })
}

fn fills(limit: u32) -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from_ms: Some(0.0),
        to_ms: Some(WEEK + 10.0),
        limit: Some(limit),
    }
}

#[test]
fn a_saturated_fills_window_keeps_its_rows_and_marks_the_result_incomplete() {
    let mut h = harness();
    h.event(Event::Command(Command::Query {
        command: 1,
        query: fills(2),
    }));
    // The first 7-day window fills its page of two; the second does not.
    let first = h.take("/userTrades");
    h.reply(
        first.id,
        200,
        "",
        &format!("[{},{}]", fill(1, 10.0), fill(2, 20.0)),
    );
    let second = h.take("/userTrades");
    h.reply(second.id, 200, "", &format!("[{}]", fill(3, WEEK + 1.0)));
    assert_eq!(rows(&h, 1), 3, "every accepted fill is kept");
    assert!(marked(&h, 1), "a saturated window is not proof of absence");
}

#[test]
fn unsaturated_fills_windows_complete_without_a_marker() {
    let mut h = harness();
    h.event(Event::Command(Command::Query {
        command: 2,
        query: fills(2),
    }));
    let first = h.take("/userTrades");
    h.reply(first.id, 200, "", &format!("[{}]", fill(1, 10.0)));
    let second = h.take("/userTrades");
    h.reply(second.id, 200, "", "[]");
    assert_eq!(rows(&h, 2), 1);
    assert!(!marked(&h, 2));
}

#[test]
fn a_saturated_order_page_keeps_its_rows_and_marks_the_result_incomplete() {
    for (limit, saturated) in [(2, true), (3, false)] {
        let mut h = harness();
        h.event(Event::Command(Command::Query {
            command: 3,
            query: Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: Some(0.0),
                to_ms: Some(1_000.0),
                limit: Some(limit),
            },
        }));
        let page = h.take("/allOrders");
        h.reply(page.id, 200, "", &format!("[{},{}]", order(1), order(2)));
        assert_eq!(rows(&h, 3), 2);
        assert_eq!(marked(&h, 3), saturated, "limit {limit}");
    }
}
