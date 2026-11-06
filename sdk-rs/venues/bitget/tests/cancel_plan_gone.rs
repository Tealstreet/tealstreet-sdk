//! A Classic cancel-plan-order answered with empty success and failure lists
//! (Bitget's answer for a position TP/SL it already removed with the position)
//! reads the plan back once: a clean, complete page without it proves it gone.
mod support;
use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{Command, Event, Outcome};
use support::*;

const EMPTY_ACK: &str = r#"{"code":"00000","data":{"successList":[],"failureList":[]}}"#;
const LISTED: &str = r#"{"code":"00000","data":{"entrustedList":[{"orderId":"target","symbol":"BTCUSDT","size":"0","triggerPrice":"120","side":"buy","holdSide":"short","planType":"pos_loss","planStatus":"live"}],"endId":"target"}}"#;
const UNACKNOWLEDGED: &str = "Bitget batch cancel did not acknowledge every order; these may still be open: BTCUSDT buy stoploss target (acknowledged [], planType pos_loss); outcome unknown, check Open Orders";

fn ready() -> Harness {
    let mut h = Harness::classic();
    while h.requests.iter().any(|r| r.url.contains("/orders-pending")) {
        h.orders(OPEN_ORDER);
    }
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"target","symbol":"BTCUSDT","size":"0","triggerPrice":"120","side":"buy","holdSide":"short","planType":"pos_loss","status":"live"},{"orderId":"sibling","symbol":"BTCUSDT","size":"0","triggerPrice":"80","side":"buy","holdSide":"short","planType":"pos_profit","status":"live"}]}"#);
    h
}

fn cancel(h: &mut Harness, reply: &str) {
    h.command(Command::CancelOrders {
        command: 7,
        ids: vec!["target".into()],
    });
    let request = h.take("/cancel-plan-order");
    h.reply(request.id, 200, reply);
}

/// The confirming read, with the documented pending-plan selector.
fn confirming_read(h: &mut Harness) -> safe_cex_engine::RequestId {
    let read = h.take("/orders-plan-pending?");
    assert_eq!(
        read.url,
        "https://api.bitget.com/api/v2/mix/order/orders-plan-pending?symbol=BTCUSDT&productType=usdt-futures&planType=profit_loss&orderId=target"
    );
    read.id
}

/// Fires the earliest pending timer.
fn next_timer(h: &mut Harness) {
    let (timer, at) = h
        .timers
        .iter()
        .copied()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a pending timer");
    h.timers.retain(|(t, _)| *t != timer);
    h.now = h.now.max(at);
    h.event(Event::TimerFired { timer });
}

fn result(h: &Harness) -> &Result<String, String> {
    &h.done
        .iter()
        .find(|(id, _)| *id == 7)
        .expect("command 7 answered")
        .1
}

fn absent_events(h: &Harness) -> usize {
    h.events
        .iter()
        .filter(|e| {
            e.code == "bitget.cancel.confirmed"
                && e.fields.iter().any(|(key, value)| {
                    *key == "result"
                        && matches!(value, safe_cex_engine::Value::Str(s) if s == "absent")
                })
        })
        .count()
}

#[test]
fn a_clean_page_without_the_plan_proves_it_gone() {
    for page in [
        r#"{"code":"00000","data":{"entrustedList":[],"endId":null}}"#,
        // A real Classic capture of an empty pending list.
        r#"{"code":"00000","msg":"success","requestTime":1793543514194,"data":{"entrustedList":null,"endId":null}}"#,
        r#"{"code":"00000","data":{"entrustedList":[{"orderId":"sibling","symbol":"BTCUSDT","planType":"pos_profit","planStatus":"live"}],"endId":"sibling"}}"#,
    ] {
        let mut h = ready();
        cancel(&mut h, EMPTY_ACK);
        assert!(h.done.is_empty(), "the cancel waits for its read");
        let read = confirming_read(&mut h);
        h.reply(read, 200, page);
        assert!(result(&h).is_ok(), "{page}: {:?}", h.done);
        assert!(h.failures.is_empty(), "{page}: {:?}", h.failures);
        assert!(
            h.engine.store().order("target").is_none(),
            "removed, not marked Canceled: {page}"
        );
        assert_eq!(
            h.engine.store().order("sibling").unwrap().status,
            OrderStatus::Open
        );
        assert_eq!(absent_events(&h), 1, "{page}");
        assert!(
            !h.requests
                .iter()
                .any(|r| r.url.contains("/cancel-plan-order")),
            "never resent"
        );
    }
}

#[test]
fn a_page_still_listing_the_plan_keeps_the_cancel_unknown() {
    let mut h = ready();
    cancel(&mut h, EMPTY_ACK);
    let read = confirming_read(&mut h);
    h.reply(read, 200, LISTED);
    let error = h.failures.last().unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.message, UNACKNOWLEDGED);
    assert!(result(&h).is_err());
    assert_eq!(
        h.engine.store().order("target").unwrap().status,
        OrderStatus::Open
    );
    assert_eq!(absent_events(&h), 0);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/cancel-plan-order"))
    );
}

#[test]
fn failed_busy_malformed_or_truncated_reads_keep_the_cancel_unknown() {
    let truncated = format!(
        r#"{{"code":"00000","data":{{"entrustedList":[{}],"endId":"p99"}}}}"#,
        (0..100)
            .map(|i| format!(r#"{{"orderId":"p{i}","symbol":"BTCUSDT","planStatus":"live"}}"#))
            .collect::<Vec<_>>()
            .join(",")
    );
    for (status, page) in [
        (429, r#"{"code":"429","msg":"Too Many Requests"}"#),
        (503, "unavailable"),
        (
            200,
            r#"{"code":"40010","msg":"Request timed out","data":null}"#,
        ),
        (200, r#"{"code":"00000","data":{}}"#),
        (200, r#"{"code":"00000","data":null}"#),
        (
            200,
            r#"{"code":"00000","data":{"entrustedList":{"orderId":"target"}}}"#,
        ),
        (
            200,
            r#"{"code":"00000","data":{"entrustedList":[{"symbol":"BTCUSDT"}]}}"#,
        ),
        (200, "not json"),
        (200, truncated.as_str()),
    ] {
        let mut h = ready();
        cancel(&mut h, EMPTY_ACK);
        let read = confirming_read(&mut h);
        h.reply(read, status, page);
        let error = h.failures.last().unwrap();
        assert_eq!(error.outcome, Some(Outcome::Unknown), "{status} {page}");
        assert_eq!(error.message, UNACKNOWLEDGED, "{status} {page}");
        assert_eq!(
            h.engine.store().order("target").unwrap().status,
            OrderStatus::Open,
            "{status} {page}"
        );
        assert_eq!(absent_events(&h), 0, "{status} {page}");
        assert!(
            !h.requests
                .iter()
                .any(|r| r.url.contains("/cancel-plan-order"))
        );
    }
}

#[test]
fn a_lost_read_keeps_the_cancel_unknown() {
    let mut h = ready();
    cancel(&mut h, EMPTY_ACK);
    let mut read = confirming_read(&mut h);
    // Reads retry a lost transport; each retry is the same selector.
    while h.done.is_empty() {
        h.event(Event::HttpFailed {
            request: read,
            error: "connection reset",
        });
        while h.done.is_empty() && !h.requests.iter().any(|r| r.url.contains("orderId=target")) {
            next_timer(&mut h);
        }
        if h.done.is_empty() {
            read = confirming_read(&mut h);
        }
    }
    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::Unknown));
    assert_eq!(
        h.engine.store().order("target").unwrap().status,
        OrderStatus::Open
    );
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/cancel-plan-order"))
    );
}

// Controls: every other answer takes the path it took before.
#[test]
fn failure_lists_coded_and_lost_cancels_never_read_the_plan_back() {
    for (status, reply, outcome) in [
        (
            200,
            r#"{"code":"00000","data":{"successList":[],"failureList":[{"orderId":"target","errorMsg":"notExistend"}]}}"#,
            Some(Outcome::Rejected),
        ),
        (
            200,
            r#"{"code":"40034","msg":"Parameter verification exception","data":null}"#,
            Some(Outcome::Rejected),
        ),
        (
            200,
            r#"{"code":"00000","data":{"orderId":"unrelated"}}"#,
            Some(Outcome::Unknown),
        ),
        (503, "unavailable", Some(Outcome::Unknown)),
    ] {
        let mut h = ready();
        h.command(Command::CancelOrders {
            command: 7,
            ids: vec!["target".into()],
        });
        let request = h.take("/cancel-plan-order");
        h.reply(request.id, status, reply);
        assert!(result(&h).is_err(), "{reply}");
        assert_eq!(h.failures.last().unwrap().outcome, outcome, "{reply}");
        assert!(
            !h.requests.iter().any(|r| r.url.contains("orderId=target")),
            "{reply}: {:?}",
            h.requests
        );
    }
    let mut h = ready();
    h.command(Command::CancelOrders {
        command: 7,
        ids: vec!["target".into()],
    });
    let request = h.take("/cancel-plan-order");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "connection reset",
    });
    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::Unknown));
    assert!(!h.requests.iter().any(|r| r.url.contains("orderId=target")));
}

#[test]
fn an_acknowledged_plan_cancel_is_unchanged() {
    let mut h = ready();
    cancel(
        &mut h,
        r#"{"code":"00000","data":{"successList":[{"orderId":"target"}],"failureList":[]}}"#,
    );
    assert!(result(&h).is_ok());
    assert_eq!(
        h.engine.store().order("target").map(|o| o.status),
        Some(OrderStatus::Canceled)
    );
    assert!(!h.requests.iter().any(|r| r.url.contains("orderId=target")));
}
