//! Binance history under the caller Read root (`docs/plans/RUST_BINANCE_HISTORY_CONTEXT.md`):
//! original REDs for managed page and lookup custody, the per-page bound and the
//! root wait, plus the chunk-gap expiry control.
mod support;
use safe_cex_engine::request_context::{HttpAction, RoutingIntent};
use safe_cex_engine::{Action, Command, ErrorKind, Event, Level, Query};
use support::{Harness, Options};

const WEEK: f64 = 604_800_000.0;
const INCOME: &str = r#"{"tranId":1,"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","income":"2","asset":"USDT","time":20}"#;

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

fn fills() -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from_ms: Some(0.0),
        to_ms: Some(10.0),
        limit: Some(1),
    }
}

/// Raw actions for one command: managed preparations with their routing intent,
/// and any legacy request or signature the venue emitted instead.
fn prepared(h: &mut Harness, query: Query) -> (Vec<(String, RoutingIntent)>, usize) {
    h.engine.handle(
        h.now,
        Event::Command(Command::Query {
            command: 900,
            query,
        }),
    );
    let mut managed = Vec::new();
    let mut legacy = 0;
    for action in h.engine.drain_actions() {
        match action {
            Action::ManagedHttp(action) => {
                if let HttpAction::Prepare {
                    intent, request, ..
                } = *action
                {
                    managed.push((request.url.clone(), intent));
                }
            }
            Action::Http(request) if request.url.contains("/fapi/") => legacy += 1,
            Action::Sign { .. } => legacy += 1,
            _ => {}
        }
    }
    (managed, legacy)
}

// ---- Original REDs ----

#[test]
fn a_fills_page_is_a_managed_read_preparation() {
    let mut h = harness();
    let (managed, legacy) = prepared(&mut h, fills());
    assert!(
        managed
            .iter()
            .any(|(url, _)| url.contains("/fapi/v1/userTrades")),
        "the fills page must be a managed preparation, not a legacy signed request: {managed:?}"
    );
    assert_eq!(legacy, 0, "no legacy signed request");
}

#[test]
fn a_client_order_lookup_is_a_managed_primary_preparation() {
    let mut h = harness();
    let (managed, legacy) = prepared(
        &mut h,
        Query::OrderByClientId {
            symbol: "BTCUSDT".into(),
            client_order_id: "caller".into(),
        },
    );
    let lookup = managed
        .iter()
        .find(|(url, _)| url.contains("/fapi/v1/order"))
        .unwrap_or_else(|| panic!("no managed lookup preparation: {managed:?}"));
    assert_eq!(lookup.1, RoutingIntent::Primary);
    assert_eq!(legacy, 0);
}

#[test]
fn an_unanswered_history_page_ends_at_its_bound_as_a_typed_timeout() {
    let mut h = harness();
    h.event(Event::Command(Command::Query {
        command: 901,
        query: fills(),
    }));
    let page = h.take("/userTrades");
    h.now += 20_000.0;
    for (timer, at) in h.timers.clone() {
        if at <= h.now {
            h.event(Event::TimerFired { timer });
        }
    }
    let failure = h
        .failures
        .iter()
        .find(|(command, _)| *command == 901)
        .map(|(_, error)| error.clone())
        .expect("the page bound ends the query");
    assert_eq!(failure.kind, Some(ErrorKind::Timeout));
    h.reply(page.id, 200, "", "[]");
    assert_eq!(
        h.done.iter().filter(|(command, _)| *command == 901).count(),
        1,
        "a late reply adds no terminal"
    );
}

/// Ranged income crossing two 7-day chunks under a 1.5 s caller wait: the root
/// expires inside the 2 s chunk gap.
fn income_in_the_gap(h: &mut Harness) {
    h.engine
        .handle_command(
            h.now,
            Command::Query {
                command: 902,
                query: Query::Income {
                    symbol: Some("BTCUSDT".into()),
                    kind: None,
                    from_ms: Some(0.0),
                    to_ms: Some(WEEK + 100.0),
                    limit: Some(1000),
                },
            },
            Some(1_500),
        )
        .unwrap();
    h.collect();
    let first = h.take("/income");
    h.reply(first.id, 200, "", &format!("[{INCOME}]"));
}

#[test]
fn root_expiry_mid_walk_is_a_strict_timeout_without_rows() {
    let mut h = harness();
    income_in_the_gap(&mut h);
    let start = h.now;
    h.now = start + 2_000.0;
    for (timer, at) in h.timers.clone() {
        if at <= h.now {
            h.event(Event::TimerFired { timer });
        }
    }
    let terminals: Vec<_> = h
        .done
        .iter()
        .filter(|(command, _)| *command == 902)
        .collect();
    assert_eq!(terminals.len(), 1, "exactly one terminal: {terminals:?}");
    assert!(
        terminals[0].1.is_err(),
        "root expiry is strict, never a partial"
    );
    let failure = h
        .failures
        .iter()
        .find(|(command, _)| *command == 902)
        .map(|(_, error)| error.clone())
        .unwrap();
    assert_eq!(failure.kind, Some(ErrorKind::Timeout));
}

#[test]
fn a_chunk_timer_firing_after_expiry_sends_nothing() {
    let mut h = harness();
    income_in_the_gap(&mut h);
    let start = h.now;
    h.now = start + 2_000.0;
    for (timer, at) in h.timers.clone() {
        if at <= h.now {
            h.event(Event::TimerFired { timer });
        }
    }
    assert!(
        !h.has_request("/income"),
        "no next chunk after the root expired: {:?}",
        h.paths()
    );
    assert_eq!(
        h.done.iter().filter(|(command, _)| *command == 902).count(),
        1
    );
}
