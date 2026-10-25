//! Original caller-root controls credited to claude-4yjlw0 (#1977).
//! Requalified against 8fcedfbbdfc with current host handshake and additive
//! source gates; see RUST_BITGET_HISTORY_READ_FUNNEL.md.
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
            HttpAction::BindGroup { .. } => panic!("history never declares Mutation barriers"),
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
    // walk advances before replying: slack is one page latency plus a timer tick.
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
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput), "{error:?}");
    assert_eq!(error.message, "Request context was revoked");
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
fn entry_refusals_keep_their_native_text_and_credentials_precedence() {
    let mut public = Harness::public();
    query_with_wait(&mut public, fills(), 60_000);
    let terminals_public = terminals(&public);
    assert_eq!(
        terminals_public[0].as_ref().unwrap_err().message,
        "Bitget credentials are required"
    );

    for query in [
        Query::OrderByClientId {
            symbol: "BTCUSDT".into(),
            client_order_id: "x".into(),
        },
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: Some(10.),
            to_ms: Some(5.),
            limit: None,
        },
    ] {
        let mut keyless = Harness::public();
        query_with_wait(&mut keyless, query, 60_000);
        assert_eq!(
            terminals(&keyless)[0].as_ref().unwrap_err().message,
            "Bitget credentials are required"
        );
    }

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
    // Native public trades emits no headers, even when private keys are configured.
    assert!(request.headers.is_empty(), "public trades acquired headers");
}

fn query_raw(h: &mut Harness, query: Query) -> Vec<Action> {
    h.engine
        .handle_command(
            h.now,
            Command::Query {
                command: COMMAND,
                query,
            },
            Some(60_000),
        )
        .unwrap();
    h.engine.prepare_dispatch(h.now);
    h.engine.drain_actions().collect()
}

fn assert_prepared(actions: &[Action], path: &str) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains(path))),
        "physical history HTTP must wait for route preparation: {path}"
    );
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::ManagedHttp(http)
        if matches!(http.as_ref(), HttpAction::Prepare { request, .. } if request.url.contains(path)))).count(),1,
        "exactly one managed Prepare for {path}");
}

#[test]
fn fills_waits_for_route_preparation() {
    let mut h = Harness::classic();
    assert_prepared(&query_raw(&mut h, fills()), "/order/fills");
}

#[test]
fn orders_waits_for_route_preparation() {
    let mut h = Harness::classic();
    assert_prepared(
        &query_raw(
            &mut h,
            Query::OrderHistory {
                symbol: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        ),
        "/orders-history",
    );
}

#[test]
fn income_waits_for_route_preparation() {
    let mut h = Harness::classic();
    assert_prepared(
        &query_raw(
            &mut h,
            Query::Income {
                symbol: None,
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        ),
        "/account/bill",
    );
}

#[test]
fn unsigned_public_trades_waits_for_route_preparation() {
    let mut h = Harness::public();
    assert_prepared(
        &query_raw(
            &mut h,
            Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: None,
            },
        ),
        "/market/fills",
    );
}

#[test]
fn unclaimed_response_cannot_complete_history() {
    let mut h = Harness::classic();
    let actions = query_raw(&mut h, fills());
    let fills_before = fills_requests(&h);
    let request = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/order/fills") => Some(r.id),
            Action::ManagedHttp(http) => match http.as_ref() {
                HttpAction::Prepare { request, .. } if request.url.contains("/order/fills") => {
                    Some(request.id)
                }
                _ => None,
            },
            _ => None,
        })
        .expect("history queued");
    h.engine.handle(
        h.now,
        safe_cex_engine::Event::HttpResponse {
            request,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: &page("", "unclaimed"),
            },
        },
    );
    let actions: Vec<_> = h.engine.drain_actions().collect();
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Notice(
                safe_cex_engine::Notice::CommandDone {
                    command: COMMAND,
                    ..
                } | safe_cex_engine::Notice::CommandFailed {
                    command: COMMAND,
                    ..
                }
            )
        )),
        "unclaimed bytes must not emit any history terminal"
    );
    assert!(
        !actions.iter().any(|a| match a {
            Action::Http(r) => r.url.contains("/order/fills"),
            Action::ManagedHttp(http) =>
                matches!(http.as_ref(), HttpAction::Prepare { request, .. }
            if request.url.contains("/order/fills")),
            _ => false,
        }),
        "unclaimed bytes must not issue a replacement page"
    );
    assert_eq!(fills_requests(&h), fills_before);
}

#[test]
fn retry_wait_keeps_original_deadline_and_seals_once_at_equality() {
    let mut h = Harness::classic();
    let before = armed(&h);
    let start = h.now;
    query_with_wait(&mut h, fills(), 200);
    h.respond("/order/fills", 500, "{}");
    fire_new(&mut h, &before, start + 200.);
    let result = terminals(&h);
    assert_eq!(
        result.len(),
        1,
        "root must seal at caller equality while retry is parked"
    );
    assert!(result[0].as_ref().is_err_and(timeout));
    fire_new(&mut h, &before, start + 1_000.);
    assert_eq!(terminals(&h).len(), 1);
    assert_eq!(fills_requests(&h), 0, "expired retry cannot send");
}

#[test]
fn native_secondary_history_identity_and_public_unsigned_control() {
    let mut h = Harness::started(FixtureOptions {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "history-read-key".into(),
            secret: "history-read-material".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Default::default()
    });
    h.load_public();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    query_with_wait(&mut h, fills(), 60_000);
    let r = h.take("/order/fills");
    assert_eq!(
        r.headers
            .iter()
            .find(|(name, _)| *name == "ACCESS-KEY")
            .map(|(_, v)| v.as_str()),
        Some("history-read-key")
    );
    h.reply(r.id, 200, &page("", "secondary"));
    assert!(terminals(&h)[0].is_ok());
    h.engine
        .handle_command(
            h.now,
            Command::Query {
                command: 43,
                query: Query::PublicTrades {
                    symbol: "BTCUSDT".into(),
                    limit: None,
                },
            },
            Some(60_000),
        )
        .unwrap();
    h.collect();
    assert!(
        h.take("/market/fills").headers.is_empty(),
        "public endpoint stays unsigned with two configured keys"
    );
}

#[test]
fn native_promotion_uses_body_code_on_non_success_http_status() {
    for status in [408, 500] {
        let mut h = Harness::classic();
        query_with_wait(&mut h, fills(), 60_000);
        h.respond(
            "/order/fills",
            status,
            r#"{"code":"40085","msg":"Classic unsupported","data":null}"#,
        );
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
        );
        h.timer_after(100.);
        h.respond(
            "/v3/trade/fills",
            200,
            r#"{"code":"00000","data":{"list":[],"cursor":""}}"#,
        );
        assert_eq!(
            terminals(&h),
            vec![Ok("[]".into())],
            "HTTP {status} promotion is native"
        );
    }
}

#[test]
fn native_later_page_exhaustion_remains_failure_without_prefix_success() {
    let mut h = Harness::classic();
    query_with_wait(&mut h, fills(), 60_000);
    h.respond("/order/fills", 200, &page("next", "accepted"));
    h.timer_after(100.);
    for delay in [250., 500., 750.] {
        h.respond("/order/fills", 500, "{}");
        h.timer_after(delay);
    }
    h.respond("/order/fills", 500, "{}");
    let result = terminals(&h);
    assert_eq!(result.len(), 1);
    assert!(
        result[0]
            .as_ref()
            .unwrap_err()
            .message
            .starts_with("Bitget history incomplete: ")
    );
    assert!(
        !h.done.iter().any(|(id, r)| *id == COMMAND && r.is_ok()),
        "native Bitget strict exhaustion has no prefix-success terminal"
    );
}

#[test]
fn native_bad_page_rolls_back_without_poisoning_retry_identities() {
    let mut h = Harness::classic();
    query_with_wait(&mut h, fills(), 60_000);
    h.respond("/order/fills", 200, &page("next", "accepted"));
    h.timer_after(100.);
    h.respond(
        "/order/fills",
        200,
        &json!({"code":"00000","data":{"fillList":[fill("retry-id"),{}],"endId":""}}).to_string(),
    );
    h.timer_after(250.);
    let retry = h.take("/order/fills");
    assert!(retry.url.contains("idLessThan=next"));
    h.reply(retry.id, 200, &page("", "retry-id"));
    let result = terminals(&h);
    let rows: Value = serde_json::from_str(result[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["id"], "accepted");
    assert_eq!(rows[1]["id"], "retry-id");
}

// ---- Source-stage controls (the original bodies above remain frozen) ----

#[test]
fn response_408_without_promotion_spends_only_the_native_retry_budget() {
    let mut h = Harness::classic();
    let before = armed(&h);
    query_with_wait(&mut h, fills(), 60_000);
    for delay in [250., 500., 750.] {
        let start = h.now;
        h.respond("/order/fills", 408, r#"{"code":"40010","msg":"not ready"}"#);
        fire_new(&mut h, &before, start + delay - 1.);
        assert_eq!(fills_requests(&h), 0, "no early retry");
        fire_new(&mut h, &before, start + delay);
        assert_eq!(fills_requests(&h), 1);
        assert!(!h.requests.iter().any(|r| r.url.contains("/v3/trade/fills")));
    }
    h.respond("/order/fills", 200, &page("", "recovered"));
    assert!(terminals(&h)[0].is_ok());
}

#[test]
fn typed_integrity_failure_never_retries_or_returns_an_accepted_prefix() {
    let mut h = Harness::classic();
    let before = armed(&h);
    query_with_wait(&mut h, fills(), 60_000);
    h.respond("/order/fills", 200, &page("next", "accepted"));
    h.timer_after(100.);
    let bad = h.take("/order/fills");
    h.event(safe_cex_engine::Event::HttpStart {
        request: bad.id,
        status: 500,
        headers: "",
    });
    // Non-success bodies bypass the list splitter and report a typed engine failure.
    h.event(safe_cex_engine::Event::HttpChunk {
        request: bad.id,
        data: &[0xff],
    });
    h.event(safe_cex_engine::Event::HttpEnd { request: bad.id });
    let result = terminals(&h);
    assert_eq!(result.len(), 1);
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.message, "response body is not UTF-8");
    let until = h.now + 2_000.;
    fire_new(&mut h, &before, until);
    assert_eq!(fills_requests(&h), 0);
    assert_eq!(terminals(&h).len(), 1);
}

#[test]
fn root_equality_wins_in_both_response_and_timer_event_orders() {
    for response_first in [true, false] {
        let mut h = Harness::classic();
        let before = armed(&h);
        let start = h.now;
        query_with_wait(&mut h, fills(), 500);
        let r = h.take("/order/fills");
        h.now = start + 500.;
        if response_first {
            h.reply(r.id, 200, &page("next", "too-late"));
        }
        fire_new(&mut h, &before, start + 500.);
        h.reply(r.id, 200, &page("", "stale"));
        fire_new(&mut h, &before, start + 1_000.);
        let result = terminals(&h);
        assert_eq!(result.len(), 1);
        assert!(result[0].as_ref().is_err_and(timeout));
        assert_eq!(fills_requests(&h), 0);
    }
}

#[test]
fn promotion_does_not_renew_the_original_receipt() {
    let mut h = Harness::classic();
    let before = armed(&h);
    let start = h.now;
    query_with_wait(&mut h, fills(), 300);
    h.now += 200.;
    h.respond("/order/fills", 500, r#"{"code":"40085","msg":"upgrade"}"#);
    fire_new(&mut h, &before, start + 300.);
    let result = terminals(&h);
    assert_eq!(result.len(), 1);
    assert!(result[0].as_ref().is_err_and(timeout));
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    fire_new(&mut h, &before, start + 1_000.);
    assert!(!h.requests.iter().any(|r| r.url.contains("/v3/trade/fills")));
    assert_eq!(terminals(&h).len(), 1);
}

#[test]
fn each_exact_endpoint_prepares_with_its_selected_routing_intent() {
    use safe_cex_engine::request_context::RoutingIntent;
    for secondary in [false, true] {
        for (query, path) in [
            (fills(), "/order/fills"),
            (
                Query::OrderHistory {
                    symbol: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
                "/orders-history",
            ),
            (
                Query::Income {
                    symbol: None,
                    kind: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
                "/account/bill",
            ),
            (
                Query::PublicTrades {
                    symbol: "BTCUSDT".into(),
                    limit: None,
                },
                "/market/fills",
            ),
        ] {
            let mut h = Harness::started(FixtureOptions {
                key: Some("key".into()),
                secret: Some("material".into()),
                passphrase: Some("pass".into()),
                read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
                    key: "read-key".into(),
                    secret: "read-material".into(),
                    passphrase: Some("read-pass".into()),
                }),
                ..Default::default()
            });
            h.load_public();
            h.respond(
                "/account/settings",
                200,
                r#"{"code":"40084","msg":"Classic","data":null}"#,
            );
            let actions = query_raw(&mut h, query);
            let intents: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::ManagedHttp(http) => match http.as_ref() {
                        HttpAction::Prepare {
                            request, intent, ..
                        } if request.url.contains(path) => Some(*intent),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            let expected = if path == "/market/fills" {
                RoutingIntent::Public
            } else if secondary {
                RoutingIntent::Secondary
            } else {
                RoutingIntent::Primary
            };
            assert_eq!(intents, vec![expected], "{path}, secondary={secondary}");
        }
    }
}

#[test]
fn preparation_refusal_is_strict_and_does_not_consume_a_transport_retry() {
    let mut h = Harness::classic();
    let before = armed(&h);
    let actions = query_raw(&mut h, fills());
    let token = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(http) => match http.as_ref() {
                HttpAction::Prepare {
                    request,
                    preparation,
                    ..
                } if request.url.contains("/order/fills") => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    h.engine.http_preparation_failed(
        h.now,
        token,
        safe_cex_engine::request_context::Error::Capacity,
        VenueError::new("history route queue refused").kind(ErrorKind::Internal),
    );
    h.collect();
    let result = terminals(&h);
    assert_eq!(result.len(), 1);
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.message, "history route queue refused");
    let until = h.now + 2_000.;
    fire_new(&mut h, &before, until);
    assert_eq!(fills_requests(&h), 0);
    assert_eq!(terminals(&h).len(), 1);
}

#[test]
fn holding_preparation_until_root_expiry_cannot_release_or_complete() {
    let mut h = Harness::classic();
    let before = armed(&h);
    let start = h.now;
    h.engine
        .handle_command(
            h.now,
            Command::Query {
                command: COMMAND,
                query: fills(),
            },
            Some(250),
        )
        .unwrap();
    h.engine.prepare_dispatch(h.now);
    let actions: Vec<_> = h.engine.drain_actions().collect();
    let (token, id) = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(http) => match http.as_ref() {
                HttpAction::Prepare {
                    request,
                    preparation,
                    ..
                } if request.url.contains("/order/fills") => Some((*preparation, request.id)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    h.absorb(
        actions
            .into_iter()
            .filter(|a| !matches!(a, Action::ManagedHttp(_)))
            .collect(),
    );
    fire_new(&mut h, &before, start + 250.);
    h.engine.http_prepared(
        h.now,
        token,
        Prepared {
            id: id.0,
            expires_at: None,
        },
    );
    h.collect();
    h.reply(id, 200, &page("", "late"));
    let result = terminals(&h);
    assert_eq!(result.len(), 1);
    assert!(result[0].as_ref().is_err_and(timeout));
    assert_eq!(fills_requests(&h), 0);
}

#[test]
fn stale_old_page_cannot_mutate_the_current_page_or_retry_state() {
    let mut h = Harness::classic();
    query_with_wait(&mut h, fills(), 60_000);
    let first = h.take("/order/fills");
    h.reply(first.id, 200, &page("next", "accepted"));
    h.timer_after(100.);
    let second = h.take("/order/fills");
    for _ in 0..3 {
        h.reply(first.id, 500, r#"{"code":"40085"}"#);
        h.event(safe_cex_engine::Event::HttpStart {
            request: first.id,
            status: 200,
            headers: "",
        });
        h.event(safe_cex_engine::Event::HttpChunk {
            request: first.id,
            data: &[0xff],
        });
        h.event(safe_cex_engine::Event::HttpEnd { request: first.id });
    }
    assert!(terminals(&h).is_empty());
    h.reply(second.id, 200, &page("", "fresh"));
    let result = terminals(&h);
    let rows: Value = serde_json::from_str(result[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["id"], "accepted");
    assert_eq!(rows[1]["id"], "fresh");
}

struct HistoryProvider {
    local: safe_cex_engine::signing::Provider,
    refuse: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl safe_cex_engine::signing::Signer for HistoryProvider {
    fn sign(
        &self,
        work: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        self.local.sign(work)
    }
    fn sign_ready(
        &self,
        work: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        if self.refuse.load(std::sync::atomic::Ordering::SeqCst) {
            Some(Err(safe_cex_engine::signing::SignError::Refused))
        } else {
            self.local.sign_ready(work)
        }
    }
}

#[test]
fn genuine_provider_refusal_stops_first_and_later_pages_without_retry() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    for prefix in [false, true] {
        let refuse = Arc::new(AtomicBool::new(false));
        let mut h = Harness::started_configure(
            FixtureOptions {
                key: Some("key".into()),
                secret: Some("material".into()),
                passphrase: Some("pass".into()),
                ..Default::default()
            },
            false,
            |engine| {
                let local = engine.signer().unwrap();
                engine
                    .set_signer(Some(Arc::new(HistoryProvider {
                        local,
                        refuse: refuse.clone(),
                    })))
                    .unwrap();
            },
        );
        h.load_public();
        h.respond("/account/settings", 200, r#"{"code":"40084","data":null}"#);
        let before = armed(&h);
        if !prefix {
            refuse.store(true, Ordering::SeqCst);
        }
        query_with_wait(&mut h, fills(), 60_000);
        if prefix {
            h.respond("/order/fills", 200, &page("next", "accepted"));
            refuse.store(true, Ordering::SeqCst);
            h.timer_after(100.);
        }
        let result = terminals(&h);
        assert_eq!(result.len(), 1);
        let error = result[0].as_ref().unwrap_err();
        assert_eq!(error.message, "Signing provider refused the request");
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
        let until = h.now + 2_000.;
        fire_new(&mut h, &before, until);
        assert_eq!(fills_requests(&h), 0);
        assert_eq!(terminals(&h).len(), 1);
    }
}

#[test]
fn network_retry_uses_a_fresh_stamp_and_keeps_the_page_cursor() {
    let mut h = Harness::classic();
    query_with_wait(&mut h, fills(), 60_000);
    let first = h.take("/order/fills");
    let stamp = first
        .headers
        .iter()
        .find(|(name, _)| *name == "ACCESS-TIMESTAMP")
        .unwrap()
        .1
        .clone();
    h.event(safe_cex_engine::Event::HttpFailed {
        request: first.id,
        error: "stream envelope is not UTF-8",
    });
    // The wording resembles integrity, but its engine-recorded kind is Network.
    h.timer_after(250.);
    let retry = h.take("/order/fills");
    let next = &retry
        .headers
        .iter()
        .find(|(name, _)| *name == "ACCESS-TIMESTAMP")
        .unwrap()
        .1;
    assert_ne!(&stamp, next);
    assert_eq!(first.url, retry.url);
    h.reply(retry.id, 200, &page("", "retried"));
    assert!(terminals(&h)[0].is_ok());
}

#[test]
fn repeated_cursor_and_global_page_cap_are_strict() {
    let mut h = Harness::classic();
    let before = armed(&h);
    query_with_wait(&mut h, fills(), 300_000);
    for n in 0..1_000 {
        assert!(
            fills_requests(&h) > 0,
            "page {n}, clock {}, terminal {:?}",
            h.now,
            terminals(&h)
        );
        h.respond(
            "/order/fills",
            200,
            &page(&format!("cursor-{n}"), &n.to_string()),
        );
        if n < 999 {
            let until = h.now + 100.;
            fire_new(&mut h, &before, until);
        }
    }
    let result = terminals(&h);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].as_ref().unwrap_err().message,
        "Bitget history page cap reached; result incomplete"
    );
    assert_eq!(fills_requests(&h), 0);

    let mut h = Harness::classic();
    query_with_wait(&mut h, fills(), 60_000);
    h.respond("/order/fills", 200, &page("same", "one"));
    h.timer_after(100.);
    h.respond("/order/fills", 200, &page("same", "two"));
    let result = terminals(&h);
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].as_ref().unwrap_err().message,
        "Bitget history cursor did not advance; result incomplete"
    );
}

#[test]
fn expiry_under_registry_pressure_retires_only_the_matching_parked_query() {
    let mut h = Harness::private_started();
    let before = armed(&h);
    let start = h.now;
    query_with_wait(&mut h, fills(), 200);
    for command in 100..356 {
        h.engine
            .handle_command(
                h.now,
                Command::Query {
                    command,
                    query: fills(),
                },
                Some(60_000),
            )
            .unwrap();
        h.collect();
    }
    let refused: HashSet<_> = h
        .done
        .iter()
        .filter_map(|(id, result)| result.is_err().then_some(*id))
        .collect();
    assert!(!refused.is_empty(), "real registry capacity is exhausted");
    assert!(!refused.contains(&COMMAND));
    fire_new(&mut h, &before, start + 400.);
    assert_eq!(terminals(&h).len(), 1);
    assert!(terminals(&h)[0].as_ref().is_err_and(timeout));
    assert_eq!(
        h.done.iter().filter(|(id, _)| *id != COMMAND).count(),
        refused.len(),
        "live peers did not retire"
    );
    h.engine
        .handle_command(
            h.now,
            Command::Query {
                command: 900,
                query: fills(),
            },
            Some(60_000),
        )
        .unwrap();
    h.collect();
    assert!(
        !h.done.iter().any(|(id, _)| *id == 900),
        "the retired owner's capacity was returned"
    );
    assert_eq!(fills_requests(&h), 0);
}

#[test]
fn native_malformed_streams_retry_the_same_page_and_preserve_accepted_rows() {
    for body in [
        &[0xff][..],
        &b"{\"code\":\"00000\",\"data\":{\"fillList\":["[..],
        &b"{\"code\":\"00000\",\"data\":{\"fillList\":[],\"endId\":\"\"},\"bad\":\"\xff\"}"[..],
    ] {
        let mut h = Harness::classic();
        let before = armed(&h);
        query_with_wait(&mut h, fills(), 60_000);
        h.respond("/order/fills", 200, &page("next", "accepted"));
        h.timer_after(100.);
        for delay in [250., 500., 750.] {
            let bad = h.take("/order/fills");
            assert!(bad.url.contains("idLessThan=next"));
            let failed_at = h.now;
            h.event(safe_cex_engine::Event::HttpStart {
                request: bad.id,
                status: 200,
                headers: "",
            });
            h.event(safe_cex_engine::Event::HttpChunk {
                request: bad.id,
                data: body,
            });
            h.event(safe_cex_engine::Event::HttpEnd { request: bad.id });
            assert!(
                terminals(&h).is_empty(),
                "native malformed response remains retryable"
            );
            fire_new(&mut h, &before, failed_at + delay - 1.);
            assert_eq!(fills_requests(&h), 0);
            fire_new(&mut h, &before, failed_at + delay);
            assert_eq!(fills_requests(&h), 1);
        }
        h.respond("/order/fills", 200, &page("", "recovered"));
        let result = terminals(&h);
        assert_eq!(result.len(), 1);
        let rows: Value = serde_json::from_str(result[0].as_ref().unwrap()).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 2);
        assert_eq!(rows[0]["id"], "accepted");
        assert_eq!(rows[1]["id"], "recovered");
    }
}

#[test]
fn promotion_resets_only_the_native_detection_wait() {
    let mut h = Harness::classic();
    let before = armed(&h);
    let start = h.now;
    query_with_wait(&mut h, fills(), 30_000);
    h.now = start + 9_000.;
    h.respond("/order/fills", 408, r#"{"code":"40085","msg":"upgrade"}"#);
    fire_new(&mut h, &before, start + 17_000.);
    assert!(
        terminals(&h).is_empty(),
        "promotion resets the native 10 s detection budget"
    );
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    fire_new(&mut h, &before, start + 17_100.);
    h.respond(
        "/v3/trade/fills",
        200,
        r#"{"code":"00000","data":{"list":[],"cursor":""}}"#,
    );
    assert_eq!(terminals(&h), vec![Ok("[]".into())]);
}
