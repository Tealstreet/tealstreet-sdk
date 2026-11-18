//! Busy bounded history must drain the source, including equal-timestamp boundaries.
mod support;
use safe_cex_engine::{Command, Event, Level, Method, Query, Value};
use std::collections::HashSet;
use support::{Harness, Options};

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
fn query(orders: bool, from: f64, to: f64) -> Query {
    if orders {
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(from),
            to_ms: Some(to),
            limit: Some(1000),
        }
    } else {
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(from),
            to_ms: Some(to),
            limit: Some(1000),
        }
    }
}
fn row(orders: bool, id: usize, time: f64) -> String {
    if orders {
        format!(
            r#"{{"orderId":{id},"symbol":"BTCUSDT","status":"FILLED","type":"LIMIT","side":"BUY","price":"100","origQty":"2","executedQty":"2","time":{time}}}"#
        )
    } else {
        format!(
            r#"{{"id":{id},"symbol":"BTCUSDT","orderId":9,"price":"100","qty":"2","side":"BUY","time":{time}}}"#
        )
    }
}
fn param(url: &str, key: &str) -> f64 {
    url.split_once('?')
        .unwrap()
        .1
        .split('&')
        .find_map(|part| {
            let (name, value) = part.split_once('=')?;
            (name == key).then(|| value.parse().unwrap())
        })
        .unwrap()
}
fn marked(h: &Harness, command: u32) -> bool {
    h.events.iter().any(|event| {
        event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
            && event
                .fields
                .contains(&("command", Value::Num(f64::from(command))))
    })
}
fn drive(h: &mut Harness, orders: bool, source: &[(usize, f64)]) -> usize {
    let path = if orders { "/allOrders" } else { "/userTrades" };
    let mut requests = 0;
    while let Some(index) = h
        .requests
        .iter()
        .position(|request| request.url.contains(path))
    {
        requests += 1;
        assert!(requests < 100, "bounded fixture must terminate");
        let request = h.requests.remove(index);
        assert_eq!(request.method, Method::Get);
        let from = param(&request.url, "startTime");
        let to = param(&request.url, "endTime");
        let body = source
            .iter()
            .rev()
            .filter(|(_, time)| *time >= from && *time <= to)
            .take(1000)
            .map(|(id, time)| row(orders, *id, *time))
            .collect::<Vec<_>>()
            .join(",");
        h.reply(request.id, 200, "", &format!("[{body}]"));
    }
    requests
}
fn result(h: &Harness, command: u32) -> Vec<serde_json::Value> {
    serde_json::from_str(h.result(command).expect("terminal").as_ref().expect("rows")).unwrap()
}

#[test]
fn busy_fills_and_orders_drain_every_page_without_duplicate_parent_rows() {
    for orders in [false, true] {
        let mut h = harness();
        let source = (1..=2001)
            .map(|id| (id, 100.0 + id as f64))
            .collect::<Vec<_>>();
        h.event(Event::Command(Command::Query {
            command: 1,
            query: query(orders, 100.0, 2200.0),
        }));
        assert!(drive(&mut h, orders, &source) > 1);
        let rows = result(&h, 1);
        assert_eq!(rows.len(), source.len());
        assert_eq!(
            rows.iter()
                .map(|row| row["id"].as_str().unwrap())
                .collect::<HashSet<_>>()
                .len(),
            source.len()
        );
        assert!(!marked(&h, 1), "every leaf was proven complete");
    }
}

#[test]
fn split_boundaries_preserve_every_row_with_equal_native_millisecond_timestamps() {
    for orders in [false, true] {
        let mut h = harness();
        let source = (1..=1800)
            .map(|id| (id, 100.0 + ((id - 1) / 600) as f64))
            .collect::<Vec<_>>();
        h.event(Event::Command(Command::Query {
            command: 2,
            query: query(orders, 100.0, 102.0),
        }));
        drive(&mut h, orders, &source);
        assert_eq!(result(&h, 2).len(), 1800);
        assert!(!marked(&h, 2));
    }
}

#[test]
fn an_indivisible_saturated_millisecond_stays_incomplete() {
    for orders in [false, true] {
        let mut h = harness();
        let source = (1..=1001).map(|id| (id, 100.0)).collect::<Vec<_>>();
        h.event(Event::Command(Command::Query {
            command: 3,
            query: query(orders, 100.0, 100.0),
        }));
        assert_eq!(drive(&mut h, orders, &source), 1);
        assert_eq!(result(&h, 3).len(), 1000);
        assert!(
            marked(&h, 3),
            "a cap at one millisecond cannot prove completeness"
        );
    }
}

#[test]
fn a_failed_child_page_preserves_the_parent_prefix_as_incomplete() {
    let mut h = harness();
    h.event(Event::Command(Command::Query {
        command: 4,
        query: query(false, 100.0, 2200.0),
    }));
    let parent = h.take("/userTrades");
    let body = (1..=1000)
        .map(|id| row(false, id, 100.0 + id as f64))
        .collect::<Vec<_>>()
        .join(",");
    h.reply(parent.id, 200, "", &format!("[{body}]"));
    assert!(h.result(4).is_none());
    let child = h.take("/userTrades");
    h.reply(child.id, 500, "", r#"{"code":-1000,"msg":"unavailable"}"#);
    assert_eq!(result(&h, 4).len(), 1000);
    assert!(marked(&h, 4));
}
