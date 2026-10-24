//! History Context adoption (`docs/plans/RUST_BITGET_HISTORY_CONTEXT.md`):
//! original REDs for the caller-wait Read root and per-page legs, plus controls
//! pinning the native behaviour the conversion must preserve.
mod support;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{Action, Command, ErrorKind, Query, RequestId, TimerId, VenueError};
use serde_json::{Value, json};
use std::collections::HashSet;
use support::*;

const COMMAND: u32 = 42;

fn fills() -> Query {
    Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    }
}

/// Dispatches through the host entry that carries the caller's wait.
fn query_with_wait(h: &mut Harness, query: Query, wait_ms: u32) {
    h.engine
        .handle_command(
            h.now,
            Command::Query {
                command: COMMAND,
                query,
            },
            Some(wait_ms),
        )
        .expect("valid caller wait");
    h.collect();
}

/// Like `Harness::collect`, recording which released requests were managed.
fn collect_managed(h: &mut Harness) -> HashSet<RequestId> {
    h.engine.prepare_dispatch(h.now);
    let mut queue: std::collections::VecDeque<_> = h.engine.drain_actions().collect();
    let mut managed = HashSet::new();
    let mut actions = Vec::new();
    while let Some(action) = queue.pop_front() {
        let Action::ManagedHttp(http) = action else {
            actions.push(action);
            continue;
        };
        match *http {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } => {
                if h.engine.http_preparation_pending(preparation) {
                    h.engine.http_prepared(
                        h.now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
            }
            HttpAction::Bind {
                permit, request, ..
            } => {
                if let Ok(Claim::Released(_)) = h.engine.claim_http(h.now, permit) {
                    managed.insert(request.id);
                    actions.push(Action::Http(request));
                }
            }
            HttpAction::Cancel { .. } => {}
        }
        for action in h
            .engine
            .drain_actions()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            queue.push_front(action);
        }
    }
    h.absorb(actions);
    managed
}

fn terminals(h: &Harness) -> Vec<Result<String, VenueError>> {
    let mut failures = h.failures.iter();
    h.done
        .iter()
        .filter_map(|(c, result)| {
            let terminal = match result {
                Ok(data) => Ok(data.clone()),
                Err(_) => Err(failures.next().expect("failure per Err").clone()),
            };
            (*c == COMMAND).then_some(terminal)
        })
        .collect()
}

fn fills_requests(h: &Harness) -> usize {
    h.requests
        .iter()
        .filter(|r| r.url.contains("/order/fills"))
        .count()
}

fn armed(h: &Harness) -> HashSet<TimerId> {
    h.timers.iter().map(|(timer, _)| *timer).collect()
}

/// Fires, in time order, only timers armed after `before`, up to `until`.
fn fire_new(h: &mut Harness, before: &HashSet<TimerId>, until: f64) {
    for _ in 0..100_000 {
        let next = h
            .timers
            .iter()
            .copied()
            .filter(|(timer, at)| !before.contains(timer) && *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((timer, at)) = next else {
            h.now = h.now.max(until);
            return;
        };
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(safe_cex_engine::Event::TimerFired { timer });
    }
    panic!("timer chain did not settle by {until}");
}

fn fill(id: &str) -> Value {
    json!({"tradeId":id,"symbol":"BTCUSDT","orderId":"1","price":"1900","baseVolume":"1","feeDetail":[{"feeCoin":"USDT","totalFee":"-0.1"}],"side":"buy","profit":"0","tradeSide":"open","tradeScope":"taker","cTime":"1627293509612"})
}

fn page(cursor: &str, id: &str) -> String {
    json!({"code":"00000","data":{"fillList":[fill(id)],"endId":cursor}}).to_string()
}

/// Answers each fills page `latency` ms after it is issued with a fresh cursor
/// until `pages` pages are answered (the last ends the walk) or the command
/// settles. Returns the number of pages answered.
fn walk(h: &mut Harness, before: &HashSet<TimerId>, latency: f64, pages: usize) -> usize {
    let mut answered = 0;
    while answered < pages && terminals(h).is_empty() {
        let Some(i) = h
            .requests
            .iter()
            .position(|r| r.url.contains("/order/fills"))
        else {
            let until = h
                .timers
                .iter()
                .filter(|(t, _)| !before.contains(t))
                .map(|(_, at)| *at)
                .fold(f64::INFINITY, f64::min);
            assert!(until.is_finite(), "walk stalled after {answered} pages");
            fire_new(h, before, until);
            continue;
        };
        let request = h.requests.remove(i);
        fire_new(h, before, h.now + latency);
        answered += 1;
        let cursor = if answered == pages {
            String::new()
        } else {
            format!("c{answered}")
        };
        h.reply(request.id, 200, &page(&cursor, &answered.to_string()));
    }
    answered
}

fn timeout(error: &VenueError) -> bool {
    error.kind == Some(ErrorKind::Timeout)
}

// ---- Original REDs ----

#[test]
fn private_history_page_releases_through_a_managed_read_context() {
    let mut h = Harness::classic();
    h.engine
        .handle_command(
            h.now,
            Command::Query {
                command: COMMAND,
                query: fills(),
            },
            Some(60_000),
        )
        .unwrap();
    let managed = collect_managed(&mut h);
    let page = h
        .requests
        .iter()
        .find(|r| r.url.contains("/order/fills"))
        .expect("first fills page");
    assert!(
        managed.contains(&page.id),
        "fills page released outside managed Context custody"
    );
}

#[test]
fn short_caller_wait_ends_the_detection_wait_with_a_typed_timeout() {
    let mut h = Harness::private_started();
    let before = armed(&h);
    let start = h.now;
    query_with_wait(&mut h, fills(), 1_500);
    fire_new(&mut h, &before, start + 1_700.0);
    assert_eq!(fills_requests(&h), 0);
    let terminals = terminals(&h);
    assert_eq!(terminals.len(), 1, "no terminal by the caller's 1.5 s wait");
    assert!(terminals[0].as_ref().is_err_and(timeout), "{terminals:?}");
}

#[test]
fn caller_wait_bounds_the_whole_walk_across_valid_pages() {
    let mut h = Harness::classic();
    let before = armed(&h);
    let start = h.now;
    query_with_wait(&mut h, fills(), 30_000);
    walk(&mut h, &before, 5_000.0, 20);
    assert!(
        h.now <= start + 30_000.0 + 5_200.0,
        "walk kept paging to {} after the 30 s caller wait",
        h.now - start
    );
    let terminals = terminals(&h);
    assert_eq!(terminals.len(), 1);
    assert!(terminals[0].as_ref().is_err_and(timeout), "{terminals:?}");
}

#[test]
fn an_unanswered_page_times_out_twenty_seconds_after_its_dequeue_and_retries() {
    let mut h = Harness::classic();
    let before = armed(&h);
    query_with_wait(&mut h, fills(), 120_000);
    let first = h.take("/order/fills");
    let dequeued = h.now;
    fire_new(&mut h, &before, dequeued + 20_000.0 + 250.0 + 50.0);
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.contains("/order/fills") && r.id != first.id),
        "no page timeout and native retry by dequeue + 20.3 s"
    );
}

#[test]
fn unstamped_receipt_fails_closed_without_a_default_root() {
    let mut h = Harness::classic();
    // At this clock `now + 120000` does not advance, so the receipt is never stamped.
    h.now = 1e300;
    h.command(Command::Query {
        command: COMMAND,
        query: fills(),
    });
    assert_eq!(fills_requests(&h), 0, "an unstamped receipt started a walk");
    let terminals = terminals(&h);
    assert_eq!(terminals.len(), 1);
    let error = terminals[0].as_ref().unwrap_err();
    // The initial Read entry refusal has no outcome; today the walk starts and
    // its signing times out NotSent at this clock instead.
    assert_eq!(error.outcome, None, "{error:?}");
}

#[test]
fn an_expired_root_leaves_an_unrelated_live_query_running() {
    let mut h = Harness::classic();
    let before = armed(&h);
    let start = h.now;
    let query = |command| Command::Query {
        command,
        query: fills(),
    };
    h.engine
        .handle_command(h.now, query(42), Some(2_000))
        .unwrap();
    h.collect();
    h.engine
        .handle_command(h.now, query(43), Some(60_000))
        .unwrap();
    h.collect();
    let short = h.take("/order/fills");
    let long = h.take("/order/fills");
    h.reply(long.id, 200, &page("c1", "1"));
    fire_new(&mut h, &before, start + 1_900.0);
    let long_next = h.take("/order/fills");
    // The short query's page fails and parks its first native retry past its wait.
    h.reply(short.id, 500, "{}");
    fire_new(&mut h, &before, start + 2_100.0);
    h.reply(long_next.id, 200, &page("", "2"));
    fire_new(&mut h, &before, start + 2_400.0);
    let of = |command| {
        let mut failures = h.failures.iter();
        h.done
            .iter()
            .filter_map(|(c, result)| {
                let terminal = match result {
                    Ok(data) => Ok(data.clone()),
                    Err(_) => Err(failures.next().unwrap().clone()),
                };
                (*c == command).then_some(terminal)
            })
            .collect::<Vec<_>>()
    };
    let short = of(42);
    assert_eq!(short.len(), 1, "{short:?}");
    assert!(short[0].as_ref().is_err_and(timeout), "{short:?}");
    let long = of(43);
    assert_eq!(long.len(), 1, "{long:?}");
    let rows: Value = serde_json::from_str(long[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(
        fills_requests(&h),
        0,
        "the expired query's parked retry still sent"
    );
}

// ---- Compatibility controls ----

#[test]
fn entry_refusals_keep_their_text_and_order() {
    let mut public = Harness::public();
    query_with_wait(&mut public, fills(), 60_000);
    let terminals_public = terminals(&public);
    assert_eq!(
        terminals_public[0].as_ref().unwrap_err().message,
        "Bitget credentials are required"
    );

    for (query, text) in [
        (
            Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "x".into(),
            },
            "Bitget client-order-id query is not supported",
        ),
        (
            Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: Some(10.0),
                to_ms: Some(5.0),
                limit: None,
            },
            "invalid Bitget history range",
        ),
        (
            Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: Some(0.0),
                to_ms: Some(1001.0 * 30.0 * 86_400_000.0),
                limit: None,
            },
            "Bitget history range exceeds 1000 windows",
        ),
    ] {
        let mut h = Harness::classic();
        query_with_wait(&mut h, query, 60_000);
        let terminals = terminals(&h);
        assert_eq!(terminals.len(), 1, "{text}");
        assert_eq!(terminals[0].as_ref().unwrap_err().message, text);
        assert_eq!(fills_requests(&h), 0, "{text}");
    }
}

#[test]
fn a_longer_caller_wait_completes_a_long_walk() {
    let mut h = Harness::classic();
    let before = armed(&h);
    query_with_wait(&mut h, fills(), 300_000);
    // 26 pages at 5 s each run well past 120 s; no fixed 120 s root may cut them.
    assert_eq!(walk(&mut h, &before, 5_000.0, 26), 26);
    let terminals = terminals(&h);
    assert_eq!(terminals.len(), 1);
    let rows: Value = serde_json::from_str(terminals[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 26);
}

#[test]
fn three_retries_then_a_strict_incomplete_failure() {
    let mut h = Harness::classic();
    query_with_wait(&mut h, fills(), 60_000);
    for delay in [250.0, 500.0, 750.0] {
        h.respond("/order/fills", 500, "{}");
        h.timer_after(delay);
    }
    h.respond("/order/fills", 500, "{}");
    let terminals = terminals(&h);
    assert_eq!(terminals.len(), 1);
    assert!(
        terminals[0]
            .as_ref()
            .unwrap_err()
            .message
            .starts_with("Bitget history incomplete: "),
        "{terminals:?}"
    );
}

#[test]
fn public_trades_stay_unsigned_without_credentials() {
    let mut h = Harness::public();
    query_with_wait(
        &mut h,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(2),
        },
        60_000,
    );
    let request = h.take("/market/fills");
    assert!(
        !request
            .headers
            .iter()
            .any(|(name, _)| *name == "ACCESS-KEY"),
        "public trades were signed"
    );
}
