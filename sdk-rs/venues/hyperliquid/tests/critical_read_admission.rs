//! Bulk chart/history work must leave room for a genuine amend prerequisite.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Command, Event, Query};
use support::*;

#[test]
fn native_bulk_candles_are_bounded_before_the_shared_host_budget() {
    let (mut e, _) = ready("disabled");
    for command in 100..110 {
        e.handle(
            2000.,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTC-PERP".into(),
                interval: Interval::M1,
                limit: Some(5000),
                from_ms: None,
                to_ms: None,
            }),
        );
    }
    let actions = prepared_now(&mut e);
    let count = actions
        .iter()
        .filter(|a| matches!(a, Action::Http(r) if kind(r) == "candleSnapshot"))
        .count();
    assert!(
        count > 0 && count <= 7,
        "native bulk must leave a critical reserve, got {count}"
    );
}

#[test]
fn native_caller_history_cannot_take_the_critical_reserve() {
    let (mut e, _) = ready("disabled");
    for command in 100..110 {
        e.handle(
            2000.,
            Event::Command(Command::Query {
                command,
                query: Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: Some(1.),
                    to_ms: Some(2000.),
                    limit: None,
                },
            }),
        );
    }
    let actions = prepared_now(&mut e);
    let count = actions
        .iter()
        .filter(|a| matches!(a, Action::Http(r) if kind(r) == "userFillsByTime"))
        .count();
    assert!(
        count > 0 && count <= 6,
        "caller history is bulk, not critical: {count}"
    );
}

#[test]
fn a_genuine_amend_sizing_read_uses_the_reserve_after_bulk_saturates() {
    use safe_cex_engine::HttpReadClass;
    let (mut e, socket) = ready("disabled");
    message(
        &mut e,
        socket,
        1500.,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"A","limitPx":"80","triggerPx":"80","origSz":"0","sz":"0","reduceOnly":true,"orderType":"Stop Market"},"status":"open"}]}"#,
    );
    e.drain_actions().for_each(drop);
    for command in 100..110 {
        e.handle(
            2000.,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTC-PERP".into(),
                interval: Interval::M1,
                limit: Some(5000),
                from_ms: None,
                to_ms: None,
            }),
        );
    }
    let actions = prepared_now(&mut e);
    let bulk: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if kind(r) == "candleSnapshot" => Some(r),
            _ => None,
        })
        .collect();
    assert!(!bulk.is_empty());
    for read in bulk {
        assert_eq!(e.http_read_class(read.id), HttpReadClass::Bulk);
    }
    e.handle(
        2001.,
        Event::Command(Command::AmendOrder {
            command: 90,
            id: "9".into(),
            price: Some(75.),
            amount: None,
        }),
    );
    let actions = prepared_now(&mut e);
    let sizing = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "orderStatus" => Some(r),
            _ => None,
        })
        .expect("bulk must leave genuine amend sizing immediately admissible");
    assert_eq!(e.http_read_class(sizing.id), HttpReadClass::Critical);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange"))),
        "sizing still precedes cancel"
    );
    answer_order_status(&mut e, sizing);
    let actions = prepared_now(&mut e);
    let cancel = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .expect("only its own fresh sizing releases cancel");
    assert_eq!(
        e.http_read_class(cancel.id),
        HttpReadClass::Normal,
        "exchange writes bypass info admission"
    );
}

#[test]
fn caller_order_status_lookup_is_bulk_even_though_amends_use_the_same_endpoint() {
    let (mut e, _) = ready("disabled");
    e.handle(
        2000.,
        Event::Command(Command::Query {
            command: 100,
            query: Query::OrderByClientId {
                symbol: "BTC-PERP".into(),
                client_order_id: "0x00000000000000000000000000000001".into(),
            },
        }),
    );
    let actions = prepared_now(&mut e);
    let read = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "orderStatus" => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        e.http_read_class(read.id),
        safe_cex_engine::HttpReadClass::Bulk
    );
    assert_eq!(
        e.http_read_class(safe_cex_engine::RequestId(u64::MAX)),
        safe_cex_engine::HttpReadClass::Normal
    );
}

#[test]
fn queued_candles_expire_at_their_original_caller_deadline_without_a_late_send() {
    let (mut e, _) = ready("disabled");
    for command in 100..110 {
        e.handle(
            2000.,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTC-PERP".into(),
                interval: Interval::M1,
                limit: Some(5000),
                from_ms: None,
                to_ms: None,
            }),
        );
    }
    let actions = prepared_now(&mut e);
    let sent: std::collections::HashSet<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if kind(r) == "candleSnapshot" => Some(r.id),
            _ => None,
        })
        .collect();
    assert!(sent.len() < 10);
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms <= 22000. => Some(*timer),
            _ => None,
        })
        .collect();
    assert!(!timers.is_empty());
    let mut actions = Vec::new();
    for timer in timers {
        e.handle(22000., Event::TimerFired { timer });
        actions.extend(prepared_now(&mut e));
    }
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Http(r) if kind(r) == "candleSnapshot" && !sent.contains(&r.id))
    ));
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command, error })
                if *command >= 100 + sent.len() as u32 && *command < 110 =>
            {
                Some((*command, error))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        failures.len(),
        10 - sent.len(),
        "every queued caller receives a visible timeout"
    );
    assert!(
        failures
            .iter()
            .all(|(_, error)| error.kind == Some(safe_cex_engine::ErrorKind::Timeout))
    );
}

#[test]
fn bulk_refills_continuously_instead_of_waiting_for_a_minute_reset() {
    let (mut e, _) = ready("disabled");
    for command in 100..108 {
        e.handle(
            2000.,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTC-PERP".into(),
                interval: Interval::M1,
                limit: Some(5000),
                from_ms: None,
                to_ms: None,
            }),
        );
    }
    let mut now = 2000.;
    let mut completed = 0;
    for _ in 0..20 {
        let actions = prepared_now(&mut e);
        for action in &actions {
            if let Action::Http(read) = action
                && kind(read) == "candleSnapshot"
            {
                reply(&mut e, read, 200, "[]");
                completed += 1;
            }
            assert!(
                !matches!(action, Action::Notice(safe_cex_engine::Notice::CommandFailed {command, ..}) if (100..108).contains(command))
            );
        }
        if completed == 8 {
            break;
        }
        let (timer, at) = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms > now && *at_ms < 22000. => {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("queued bulk owns a refill wake inside its original bound");
        now = at;
        e.handle(now, Event::TimerFired { timer });
    }
    assert_eq!(completed, 8);
    assert!(now < 22000.);
}
