//! Original caller-root controls; venue history is still native on this stack.
mod support;
use safe_cex_engine::request_context::{HttpAction, RoutingIntent};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Query, Response,
    VenueError,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use serde_json::Value;

const NOW: f64 = 5000.;
const OWNER: u32 = 778;
const CLOID: &str = "0x00000000000000000000000000000001";

fn ready() -> Engine<Hyperliquid> {
    let (mut e, _) = support::ready("disabled");
    e.drain_actions().for_each(drop);
    e
}
fn fills() -> Query {
    Query::Fills {
        symbol: Some("BTC-PERP".into()),
        order_id: None,
        from_ms: Some(1.),
        to_ms: Some(NOW),
        limit: None,
    }
}
fn income() -> Query {
    Query::Income {
        symbol: None,
        kind: None,
        from_ms: Some(1.),
        to_ms: Some(NOW),
        limit: None,
    }
}
fn trades() -> Query {
    Query::PublicTrades {
        symbol: "BTC-PERP".into(),
        limit: Some(2),
    }
}
fn lookup() -> Query {
    Query::OrderByClientId {
        symbol: "BTC-PERP".into(),
        client_order_id: CLOID.into(),
    }
}
fn ask(e: &mut Engine<Hyperliquid>, at: f64, command: u32, query: Query, wait: Option<u32>) {
    e.handle_command(at, Command::Query { command, query }, wait)
        .expect("genuine caller receipt");
}
fn actions(e: &mut Engine<Hyperliquid>, at: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, at)
}
fn read(output: &[Action], kind: &str) -> HttpRequest {
    let mut reads = output.iter().filter_map(|a| match a {
        Action::Http(r) if support::kind(r) == kind => Some(r.clone()),
        _ => None,
    });
    let request = reads.next().expect("native request reached real dispatch");
    assert!(reads.next().is_none(), "one selected physical request");
    request
}
fn reply(e: &mut Engine<Hyperliquid>, at: f64, request: &HttpRequest, body: &str) {
    e.handle(
        at,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn fill(tid: u64) -> String {
    format!(
        r#"{{"coin":"BTC","side":"B","px":"100","sz":"0.5","time":1000,"tid":{tid},"oid":99,"fee":"-0.01","closedPnl":"2","crossed":false,"feeToken":"USDC"}}"#
    )
}
fn done(output: &[Action], owner: u32) -> Option<&str> {
    output.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command, result }) if *command == owner => {
            Some(result.as_str())
        }
        _ => None,
    })
}
fn failure(output: &[Action], owner: u32) -> &VenueError {
    let errors: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "exactly one correlated typed terminal");
    assert!(
        done(output, owner).is_none(),
        "failure cannot also publish rows"
    );
    errors[0]
}
fn markers(output: &[Action], owner: u32) -> usize {
    output
        .iter()
        .filter(|a| {
            matches!(a, Action::Notice(Notice::Event(event))
        if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
        && event.fields.contains(&("command", safe_cex_engine::Value::Num(f64::from(owner)))))
        })
        .count()
}
fn timeout(output: &[Action], owner: u32) {
    let error = failure(output, owner);
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert!(error.outcome.is_none());
    assert_eq!(
        markers(output, owner),
        0,
        "root expiry cannot publish an incomplete prefix"
    );
}
fn first_page(e: &mut Engine<Hyperliquid>, wait: u32, query: Query) -> HttpRequest {
    ask(e, NOW, OWNER, query, Some(wait));
    let first = read(&actions(e, NOW), "userFillsByTime");
    reply(e, NOW + 1., &first, &format!("[{}]", fill(1)));
    read(&actions(e, NOW + 1.), "userFillsByTime")
}

#[test]
fn original_history_families_require_managed_public_preparation() {
    for (query, kind) in [
        (fills(), "userFillsByTime"),
        (trades(), "recentTrades"),
        (lookup(), "orderStatus"),
    ] {
        let mut e = ready();
        ask(&mut e, NOW, OWNER, query, Some(300_000));
        let output: Vec<_> = e.drain_actions().collect();
        assert!(
            !output.iter().any(|a| matches!(a, Action::Http(_))),
            "no legacy dispatch before preparation"
        );
        assert_eq!(output.iter().filter(|a| matches!(a, Action::ManagedHttp(action)
            if matches!(action.as_ref(), HttpAction::Prepare { intent: RoutingIntent::Public, request, .. }
            if support::kind(request) == kind))).count(), 1);
    }
}

#[test]
fn original_root_timer_uses_configured_default_and_fractional_receipt_deadlines() {
    for wait in [None, Some(300_000)] {
        let mut e = ready();
        let at = NOW + 0.25;
        ask(&mut e, at, OWNER, fills(), wait);
        let output: Vec<_> = e.drain_actions().collect();
        let deadline = at + f64::from(wait.unwrap_or(120_000));
        assert_eq!(
            output
                .iter()
                .filter(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == deadline))
                .count(),
            1,
            "one immutable engine root timer at the exact caller deadline"
        );
    }
}

#[test]
fn original_parked_history_expires_without_waiting_for_the_read_budget_wake() {
    let mut e = ready();
    for command in 1..=20 {
        ask(&mut e, NOW, command, fills(), Some(5000));
    }
    let dispatched = actions(&mut e, NOW);
    let sent = dispatched
        .iter()
        .filter(|a| matches!(a, Action::Http(r) if support::kind(r) == "userFillsByTime"))
        .count();
    assert!(
        (1..=8).contains(&sent),
        "native weighted reservation working control"
    );
    e.prepare_dispatch(NOW + 5000.);
    let expired: Vec<_> = e.drain_actions().collect();
    timeout(&expired, 20);
    assert!(!expired.iter().any(|a| matches!(a, Action::Http(_)) || matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. }))));
}

#[test]
fn original_inflight_root_expiry_discards_an_accepted_prefix_and_late_page() {
    let mut e = ready();
    let second = first_page(&mut e, 5000, fills());
    e.prepare_dispatch(NOW + 5000.);
    let expired: Vec<_> = e.drain_actions().collect();
    timeout(&expired, OWNER);
    reply(&mut e, NOW + 5001., &second, "[]");
    let late: Vec<_> = e.drain_actions().collect();
    assert!(done(&late, OWNER).is_none());
    assert_eq!(markers(&late, OWNER), 0);
    assert!(!late.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: OWNER, .. })
    )));
}

#[test]
fn original_last_page_at_root_deadline_cannot_publish_rows() {
    let mut e = ready();
    let second = first_page(&mut e, 5000, fills());
    reply(&mut e, NOW + 5000., &second, "[]");
    timeout(&e.drain_actions().collect::<Vec<_>>(), OWNER);
}

#[test]
fn original_income_stage_switch_cannot_renew_the_caller_root() {
    let mut e = ready();
    let second = first_page(&mut e, 5000, income());
    reply(&mut e, NOW + 2., &second, "[]");
    let funding = read(&actions(&mut e, NOW + 2.), "userFunding");
    reply(&mut e, NOW + 5000., &funding, "[]");
    timeout(&e.drain_actions().collect::<Vec<_>>(), OWNER);
}

#[test]
fn original_lookup_expiry_never_publishes_unknown_or_absent() {
    let mut e = ready();
    ask(&mut e, NOW, OWNER, lookup(), Some(5000));
    let request = read(&actions(&mut e, NOW), "orderStatus");
    reply(&mut e, NOW + 5000., &request, r#"{"status":"unknownOid"}"#);
    timeout(&e.drain_actions().collect::<Vec<_>>(), OWNER);
}

#[test]
fn original_missing_receipt_refuses_without_fallback_root_or_effects() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options::default()));
    e.handle(
        f64::MAX,
        Event::Command(Command::Query {
            command: OWNER,
            query: fills(),
        }),
    );
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(output.len(), 1, "no root, HTTP, timer or Store effect");
    let error = failure(&output, OWNER);
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert!(error.outcome.is_none());
}

#[test]
fn original_late_generation_cannot_finish_a_reused_command() {
    let mut e = ready();
    ask(&mut e, NOW, OWNER, trades(), Some(5000));
    let old = read(&actions(&mut e, NOW), "recentTrades");
    e.prepare_dispatch(NOW + 5000.);
    timeout(&e.drain_actions().collect::<Vec<_>>(), OWNER);
    ask(&mut e, NOW + 5001., OWNER, trades(), Some(5000));
    let new = read(&actions(&mut e, NOW + 5001.), "recentTrades");
    assert_ne!(old.id, new.id);
    reply(&mut e, NOW + 5002., &old, "[]");
    assert!(done(&e.drain_actions().collect::<Vec<_>>(), OWNER).is_none());
    reply(&mut e, NOW + 5003., &new, "[]");
    assert_eq!(
        done(&e.drain_actions().collect::<Vec<_>>(), OWNER),
        Some("[]")
    );
}

#[test]
fn native_unsigned_json_wire_preserves_vault_subject_and_range() {
    let vault = "0x0000000000000000000000000000000000000001";
    let (mut e, _) = support::ready_identity("disabled", false, None, "30", Some(vault.into()));
    e.drain_actions().for_each(drop);
    ask(&mut e, NOW, OWNER, fills(), Some(300_000));
    let request = read(&actions(&mut e, NOW), "userFillsByTime");
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.url, "https://api.hyperliquid.xyz/info");
    assert_eq!(
        request.headers,
        vec![("Content-Type", "application/json".into())]
    );
    assert!(request.signer_headers.is_empty());
    assert!(request.route.is_none());
    let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"type":"userFillsByTime","user":vault,"startTime":1,"endTime":5000,"aggregateByTime":false})
    );
}

#[test]
fn native_live_root_later_failure_retains_only_accepted_rows_and_owned_marker() {
    let mut e = ready();
    let second = first_page(&mut e, 300_000, fills());
    e.handle(
        NOW + 2.,
        Event::HttpFailed {
            request: second.id,
            error: "native lost page",
        },
    );
    let output: Vec<_> = e.drain_actions().collect();
    let rows: Value =
        serde_json::from_str(done(&output, OWNER).expect("accepted-only native recovery")).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "1");
    assert_eq!(rows[0]["commission"], -0.01);
    assert_eq!(rows[0]["realizedPnl"], 2.);
    assert_eq!(markers(&output, OWNER), 1);
    assert!(!output.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: OWNER, .. })
    )));
}

#[test]
fn native_first_page_failure_has_no_empty_success_or_incomplete_marker() {
    let mut e = ready();
    ask(&mut e, NOW, OWNER, fills(), Some(300_000));
    let first = read(&actions(&mut e, NOW), "userFillsByTime");
    e.handle(
        NOW + 1.,
        Event::HttpFailed {
            request: first.id,
            error: "native first page",
        },
    );
    let output: Vec<_> = e.drain_actions().collect();
    assert_eq!(failure(&output, OWNER).message, "native first page");
    assert_eq!(markers(&output, OWNER), 0);
}

#[test]
fn native_income_stage_recovery_keeps_gross_pnl_and_rebate_split() {
    let mut e = ready();
    let second = first_page(&mut e, 300_000, income());
    reply(&mut e, NOW + 2., &second, "[]");
    let funding = read(&actions(&mut e, NOW + 2.), "userFunding");
    e.handle(
        NOW + 3.,
        Event::HttpFailed {
            request: funding.id,
            error: "native funding failure",
        },
    );
    let output: Vec<_> = e.drain_actions().collect();
    let rows: Value =
        serde_json::from_str(done(&output, OWNER).expect("native fills survive funding failure"))
            .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["category"] == "rebate" && r["income"] == 0.01)
    );
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .any(|r| r["incomeType"] == "REALIZED_PNL" && r["income"] == 2.)
    );
    assert_eq!(markers(&output, OWNER), 1);
}

#[test]
fn host_invalid_clock_is_an_error_without_a_venue_dispatch() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options::default()));
    let error = e
        .handle_command(
            f64::NAN,
            Command::Query {
                command: OWNER,
                query: fills(),
            },
            None,
        )
        .expect_err("invalid host receipt cannot dispatch");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert!(e.drain_actions().next().is_none());
}

#[test]
fn original_queued_page_child_is_minted_at_dequeue_and_capped_by_its_root() {
    let mut e = ready();
    for command in 1..=20 {
        ask(&mut e, NOW, command, fills(), Some(65_000));
    }
    let initial = actions(&mut e, NOW);
    let (timer, at) = initial
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if *at_ms > NOW + 20_000. && *at_ms < NOW + 65_000. =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("native shared-budget wake before caller expiry");
    e.handle(at, Event::TimerFired { timer });
    let output = actions(&mut e, at);
    assert!(
        output
            .iter()
            .any(|a| matches!(a, Action::Http(r) if support::kind(r) == "userFillsByTime")),
        "native eligible budget wake releases a parked page"
    );
    assert!(
        output
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == NOW + 65_000.)),
        "dequeued physical child is capped by original root, not a fresh20s beyond it"
    );
}

#[test]
fn original_same_boundary_transport_failure_and_root_sweep_have_one_strict_terminal() {
    for leg_first in [false, true] {
        let mut e = ready();
        ask(&mut e, NOW, OWNER, fills(), Some(20_000));
        let first = read(&actions(&mut e, NOW), "userFillsByTime");
        reply(&mut e, NOW + 1., &first, &format!("[{}]", fill(1)));
        let output = actions(&mut e, NOW + 1.);
        let second = read(&output, "userFillsByTime");
        let deadline = NOW + 20_000.;
        let mut result = vec![];
        if leg_first {
            e.handle(
                deadline,
                Event::HttpFailed {
                    request: second.id,
                    error: "native boundary failure",
                },
            );
            result.extend(e.drain_actions());
            e.prepare_dispatch(deadline);
        } else {
            e.prepare_dispatch(deadline);
            result.extend(e.drain_actions());
            e.handle(
                deadline,
                Event::HttpFailed {
                    request: second.id,
                    error: "native boundary failure",
                },
            );
        }
        result.extend(e.drain_actions());
        timeout(&result, OWNER);
        reply(&mut e, deadline + 1., &second, "[]");
        let late: Vec<_> = e.drain_actions().collect();
        assert!(done(&late, OWNER).is_none());
        assert_eq!(markers(&late, OWNER), 0);
        assert!(!late.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: OWNER, .. })
        )));
    }
}
