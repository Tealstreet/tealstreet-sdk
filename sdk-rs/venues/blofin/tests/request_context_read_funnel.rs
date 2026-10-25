//! Original controls: BloFin's private reads go out as managed legs, never legacy HTTP.
#![allow(dead_code)]
mod support;
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Query, VenueAction};
use support::*;

const NOW: f64 = 1.0;

fn legacy(actions: &[Action], path: &str) -> usize {
    actions
        .iter()
        .filter(|a| matches!(a, Action::Http(r) if r.url.contains(path)))
        .count()
}

fn prepared(actions: &[Action], path: &str) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare { request, .. } if request.url.contains(path) => {
                    Some(request.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// One managed Prepare for `path` and no legacy request for it.
fn managed(actions: &[Action], path: &str) -> HttpRequest {
    assert_eq!(
        legacy(actions, path),
        0,
        "{path} must not go out as legacy HTTP"
    );
    let mut requests = prepared(actions, path);
    assert_eq!(requests.len(), 1, "one managed preparation for {path}");
    requests.remove(0)
}

fn query(h: &mut Harness, query: Query) -> Vec<Action> {
    h.handle(NOW, Event::Command(Command::Query { command: 41, query }));
    h.e.drain_actions().collect()
}

#[test]
fn a_fills_history_page_is_a_managed_read() {
    let mut h = Harness::new();
    h.seed();
    let actions = query(
        &mut h,
        Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(1_000.0),
            to_ms: Some(2_000.0),
            limit: None,
        },
    );
    let request = managed(&actions, "/api/v1/trade/fills-history?");
    assert!(
        request
            .url
            .contains("/api/v1/trade/fills-history?instId=BTC-USDT&begin=1000&end=2000&limit="),
        "{}",
        request.url
    );
}

#[test]
fn an_order_history_page_is_a_managed_read() {
    let mut h = Harness::new();
    h.seed();
    let actions = query(
        &mut h,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    managed(&actions, "/api/v1/trade/orders-history?");
}

#[test]
fn an_income_page_is_a_managed_read() {
    let mut h = Harness::new();
    h.seed();
    let actions = query(
        &mut h,
        Query::Income {
            symbol: Some("BTCUSDT".into()),
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    managed(&actions, "/api/v1/trade/fills-history?");
}

#[test]
fn a_closed_positions_page_is_a_managed_read() {
    let mut h = Harness::new();
    h.seed();
    h.handle(
        NOW,
        Event::Command(Command::Venue {
            command: 9,
            action: VenueAction::FetchClosedPositions {
                symbol: Some("BTCUSDT".into()),
                from_ms: Some(1_000.0),
                to_ms: Some(2_000.0),
            },
        }),
    );
    let actions: Vec<_> = h.e.drain_actions().collect();
    let request = managed(&actions, "/api/v1/account/positions-history?");
    assert!(request.url.ends_with(
        "/api/v1/account/positions-history?instId=BTC-USDT&begin=1000&end=2000&limit=100"
    ));
}

#[test]
fn the_account_refresh_tick_reads_positions_and_orders_as_managed_reads() {
    let mut h = Harness::new();
    h.seed();
    let tick = h.tick;
    h.handle(10_000.0, Event::TimerFired { timer: tick });
    let actions: Vec<_> = h.e.drain_actions().collect();
    for path in ["account/positions", "orders-pending"] {
        managed(&actions, path);
    }
}

#[test]
fn the_balance_read_that_follows_positions_is_a_managed_read() {
    let mut h = Harness::new();
    h.seed();
    let tick = h.tick;
    h.handle(10_000.0, Event::TimerFired { timer: tick });
    h.collect();
    // Balance follows the positions answer in the native join.
    let positions = h.take("account/positions");
    let body = format!("{{\"code\":\"0\",\"data\":[{POSITION}]}}");
    h.handle(
        10_001.0,
        Event::HttpResponse {
            request: positions.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    let actions: Vec<_> = h.e.drain_actions().collect();
    managed(&actions, "balances");
}

#[test]
fn a_retried_startup_setting_read_is_a_managed_read() {
    let mut h = Harness::new();
    // A failed margin-mode read schedules the native startup retry.
    h.reply(
        "margin-mode",
        r#"{"code":"50001","msg":"busy","data":null}"#,
    );
    let retry = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > 0.0 && *timer != h.tick => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("startup setting retry timer");
    h.handle(retry.1, Event::TimerFired { timer: retry.0 });
    let actions: Vec<_> = h.e.drain_actions().collect();
    managed(&actions, "margin-mode");
}

/// Candle reads refused for owner capacity after parking 300 more (#2115's probe).
fn capacity_refusals(h: &mut Harness, now: f64) -> usize {
    let mut refused = 0;
    for command in 3_000..3_300 {
        h.handle(
            now,
            Event::Command(Command::FetchCandles {
                command,
                symbol: "BTCUSDT".into(),
                interval: safe_cex_engine::model::Interval::M1,
                limit: Some(2),
                from_ms: None,
                to_ms: None,
            }),
        );
        refused += h
            .e
            .drain_actions()
            .filter(|a| {
                matches!(a, Action::Notice(safe_cex_engine::Notice::CommandFailed { command: c, error })
                    if *c == command && error.legacy_text().contains("capacity"))
            })
            .count();
    }
    refused
}

/// dazzy3 on #2143: a leg whose late answer the native revision check drops
/// must still finish its Context, or it holds owner capacity until expiry.
#[test]
fn a_superseded_account_leg_publishes_nothing_and_holds_no_capacity() {
    let run = |supersede: bool| {
        let mut h = Harness::new();
        h.seed();
        let tick = h.tick;
        h.handle(10_000.0, Event::TimerFired { timer: tick });
        h.collect_at(10_000.0);
        let positions = h.take("account/positions");
        if supersede {
            h.ws(r#"{"arg":{"channel":"positions"},"data":[]}"#);
        }
        h.respond_at(
            positions,
            &format!("{{\"code\":\"0\",\"data\":[{POSITION}]}}"),
            10_001.0,
        );
        if supersede {
            assert!(
                h.e.store().positions().is_empty(),
                "the superseded REST answer publishes nothing"
            );
        }
        capacity_refusals(&mut h, 10_002.0)
    };
    let completed = run(false);
    assert!(completed > 0, "the probe reaches owner capacity");
    assert_eq!(
        run(true),
        completed,
        "the dropped leg must not hold capacity"
    );
}

/// dazzy3 on #2143: a delivered error (here a body without its list) on a
/// superseded account leg validates custody, leaves the newer positions
/// untouched and finishes its Context, like a delivered success.
#[test]
fn a_superseded_account_leg_answered_with_a_stream_error_changes_nothing() {
    let run = |error: bool| {
        let mut h = Harness::new();
        h.seed();
        let tick = h.tick;
        h.handle(10_000.0, Event::TimerFired { timer: tick });
        h.collect_at(10_000.0);
        let positions = h.take("account/positions");
        h.ws(&format!(
            r#"{{"arg":{{"channel":"positions"}},"data":[{POSITION}]}}"#
        ));
        let pushed = h.e.store().positions().len();
        let body = if error {
            r#"{"code":"50001","msg":"busy"}"#.to_owned()
        } else {
            r#"{"code":"0","data":[]}"#.to_owned()
        };
        h.respond_at(positions, &body, 10_001.0);
        assert_eq!(
            h.e.store().positions().len(),
            pushed,
            "the superseded answer changes no account state"
        );
        // A completed positions read joins its balance read; answer it too so
        // both runs end with no account leg in flight.
        if let Some(index) = h.pending.iter().position(|r| r.url.contains("balances")) {
            let balance = h.pending.remove(index);
            h.respond_at(balance, r#"{"code":"0","data":[]}"#, 10_001.0);
        }
        capacity_refusals(&mut h, 10_002.0)
    };
    let completed = run(false);
    assert!(completed > 0, "the probe reaches owner capacity");
    assert_eq!(
        run(true),
        completed,
        "the errored leg must not hold capacity"
    );
}
