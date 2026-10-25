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
    let mut h = started();
    h.requests.clear();
    h
}
fn started() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.engine.set_log_level(Level::Error);
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
    // An unknown market answers UNKNOWN locally, before any lookup I/O.
    let mut h = started();
    h.load_public();
    h.requests.clear();
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

/// Fires, in time order, every armed timer due by `until`.
fn fire_until(h: &mut Harness, until: f64) {
    loop {
        let Some((timer, at)) = h
            .timers
            .iter()
            .copied()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        else {
            h.now = h.now.max(until);
            return;
        };
        h.timers.retain(|(t, _)| *t != timer);
        h.now = h.now.max(at);
        h.event(Event::TimerFired { timer });
    }
}

/// Ranged income crossing two 7-day chunks: the first chunk answers, and the
/// next waits on the 2 s chunk timer. Returns the time the first chunk answered.
fn income_two_chunks(h: &mut Harness, wait_ms: u32) -> f64 {
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
            Some(wait_ms),
        )
        .unwrap();
    h.collect();
    let first = h.take("/income");
    h.reply(first.id, 200, "", &format!("[{INCOME}]"));
    // The same chunk pages on from the last row until a page comes back empty.
    while h.has_request("/income") {
        let page = h.take("/income");
        h.reply(page.id, 200, "", "[]");
    }
    assert!(
        !h.has_request("/income"),
        "the second chunk waits for the chunk timer"
    );
    h.now
}

#[test]
fn a_generous_wait_sends_the_second_chunk_after_the_chunk_delay() {
    let mut h = harness();
    let start = income_two_chunks(&mut h, 120_000);
    fire_until(&mut h, start + 1_999.0);
    assert!(!h.has_request("/income"), "not before the 2 s chunk delay");
    fire_until(&mut h, start + 2_000.0);
    assert!(h.has_request("/income"), "chunk 2 at +2 s: {:?}", h.paths());
}

#[test]
fn root_expiry_mid_walk_is_a_strict_timeout_without_rows() {
    let mut h = harness();
    let start = income_two_chunks(&mut h, 1_500);
    // Step 1: the root's wait expires inside the chunk gap.
    fire_until(&mut h, start + 1_500.0);
    // Step 2: the chunk timer fires after the expiry.
    fire_until(&mut h, start + 2_000.0);
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
    let start = income_two_chunks(&mut h, 1_500);
    fire_until(&mut h, start + 1_500.0);
    fire_until(&mut h, start + 2_000.0);
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

// ---- Source pins ----

#[test]
fn a_fills_page_uses_the_read_key_on_its_own_route() {
    let keyed = || {
        let mut h = Harness::started(Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            read_credentials: Some(safe_cex_engine::ReadCredentials {
                key: "read-key".into(),
                secret: "read-secret".into(),
                passphrase: None,
            }),
            ..Options::default()
        });
        h.requests.clear();
        h
    };
    let (managed, _) = prepared(&mut keyed(), fills());
    assert_eq!(
        managed
            .iter()
            .map(|(_, intent)| *intent)
            .collect::<Vec<_>>(),
        [RoutingIntent::Secondary]
    );
    let mut h = keyed();
    h.event(Event::Command(Command::Query {
        command: 903,
        query: fills(),
    }));
    let page = h.take("/fapi/v1/userTrades");
    assert!(
        page.url.starts_with(
            "https://fapi.binance.com/fapi/v1/userTrades?symbol=BTCUSDT&limit=1&startTime=0&endTime=10&timestamp="
        ),
        "{}",
        page.url
    );
    assert!(
        page.headers
            .iter()
            .any(|(name, value)| *name == "X-MBX-APIKEY" && value == "read-key"),
        "{:?}",
        page.headers
    );
    assert_eq!(page.timeout_ms, Some(20_000));
}
