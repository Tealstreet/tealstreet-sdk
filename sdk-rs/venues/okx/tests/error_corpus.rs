//! Recorded OKX error replies (tests/fixtures/errors), each driven through the
//! production path that consumes it: the account-cycle reads, public startup
//! discovery, an order placement, or the candles command.
#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::log::code;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::test_support::error_corpus::{Fixture, Pinned, load};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, Response,
};
use safe_cex_venue_okx::Okx;
use std::collections::BTreeMap;

const BALANCE: &str = "/api/v5/account/account-position-risk?instType=SWAP";
const POSITIONS: &str = "/api/v5/account/positions?instType=SWAP";

fn corpus() -> BTreeMap<String, Fixture> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/errors");
    load(&dir).into_iter().collect()
}

/// A keyed account through startup whose account-cycle reads stay pending,
/// recording every notice the engine emits.
struct Cycle {
    engine: Engine<Okx>,
    now: f64,
    requests: Vec<HttpRequest>,
    notices: Vec<Notice>,
}

impl Cycle {
    fn started() -> Self {
        let mut engine = crate::support::engine(Options {
            key: Some("test-key".into()),
            secret: Some("test-secret".into()),
            passphrase: Some("test-passphrase".into()),
            ..Options::default()
        });
        engine.start(0.0);
        let mut cycle = Self {
            engine,
            now: 0.0,
            requests: Vec::new(),
            notices: Vec::new(),
        };
        cycle.collect();
        cycle.answer("instruments", 200, &envelope(MARKET));
        cycle.answer("tickers", 200, &envelope(TICKER));
        cycle.answer(
            "/api/v5/account/config",
            200,
            &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        );
        cycle
    }

    fn collect(&mut self) {
        self.engine.prepare_dispatch(self.now);
        let mut queue: std::collections::VecDeque<_> = self.engine.drain_actions().collect();
        while let Some(action) = queue.pop_front() {
            match action {
                Action::ManagedHttp(http) => match *http {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        if self.engine.http_preparation_pending(preparation) {
                            self.engine.http_prepared(
                                self.now,
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
                        if let Ok(Claim::Released(_)) = self.engine.claim_http(self.now, permit) {
                            self.requests.push(request);
                        }
                    }
                    HttpAction::BindGroup { .. } | HttpAction::Cancel { .. } => {}
                },
                Action::Http(request) => self.requests.push(request),
                Action::Notice(notice) => self.notices.push(notice),
                _ => {}
            }
            for action in self
                .engine
                .drain_actions()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                queue.push_front(action);
            }
        }
    }

    fn answer(&mut self, contains: &str, status: u16, body: &str) {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.contains(contains))
            .unwrap_or_else(|| panic!("no {contains}"));
        let request = self.requests.remove(i);
        self.now += 1.0;
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
}

#[test]
fn the_corpus_covers_every_recorded_case_with_its_production_path() {
    let names: Vec<_> = corpus().into_keys().collect();
    assert_eq!(
        names,
        [
            "balance-bogus-key",
            "balance-demo-header-bogus-key",
            "balance-expired-timestamp",
            "balance-malformed-timestamp",
            "eea-balance-bogus-key",
            "instruments-invalid-type",
            "order-bogus-key",
            "positions-bogus-key",
            "spot-ticker-unknown-instrument",
            "swap-books-unknown-instrument",
            "swap-candles-invalid-bar",
        ]
    );
    for (name, fixture) in corpus() {
        let expected = if name.starts_with("order") {
            Pinned::Write
        } else {
            Pinned::Read
        };
        assert_eq!(fixture.expect.path, expected, "{name}");
    }
}

/// OKX checks the key before the timestamp and the environment, so every keyed
/// read answered 401 50111. An account-cycle read reports it as a visible
/// `auth.rejected` Error with the table's `authInvalid` and the native code.
#[test]
fn a_rejected_key_on_the_account_cycle_reads_is_classified_auth_invalid() {
    for (name, path) in [
        ("balance-bogus-key", BALANCE),
        ("balance-expired-timestamp", BALANCE),
        ("balance-malformed-timestamp", BALANCE),
        ("balance-demo-header-bogus-key", BALANCE),
        ("eea-balance-bogus-key", BALANCE),
        ("positions-bogus-key", POSITIONS),
    ] {
        let fixture = &corpus()[name];
        assert!(fixture.body.contains(r#""code":"50111""#), "{name}");
        let mut cycle = Cycle::started();
        cycle.answer(path, fixture.http_status.unwrap(), &fixture.body);
        let event = cycle
            .notices
            .iter()
            .find_map(|notice| match notice {
                Notice::Event(event) if event.code == code::AUTH_REJECTED => Some(event),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}: {:?}", cycle.notices));
        assert_eq!(event.level, safe_cex_engine::Level::Error, "{name}");
        assert!(
            event.fields.contains(&("kind", "authInvalid".into())),
            "{name}: {:?}",
            event.fields
        );
        assert!(
            event.fields.contains(&("venueCode", "50111".into())),
            "{name}"
        );
        assert_eq!(fixture.expect.kind, Some(ErrorKind::AuthInvalid), "{name}");
    }
}

#[test]
fn a_rejected_key_on_an_order_is_a_rejected_auth_failure() {
    let fixture = &corpus()["order-bogus-key"];
    let mut e = trading_engine();
    e.handle(
        5.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![
                serde_json::from_str(
                    r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000}"#,
                )
                .unwrap(),
            ],
        }),
    );
    let out = prepared(&mut e, 5.0);
    let Some(Action::Http(request)) = out.iter().find(|a| matches!(a, Action::Http(_))) else {
        panic!("no order write: {out:?}");
    };
    e.handle(
        6.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: fixture.http_status.unwrap(),
                headers: "",
                body: &fixture.body,
            },
        },
    );
    let failure = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("the write fails");
    assert_eq!(
        (failure.kind, failure.outcome, failure.venue_code.as_deref()),
        (
            Some(ErrorKind::AuthInvalid),
            Some(Outcome::Rejected),
            Some("50111")
        )
    );
}

/// Public discovery keeps the engine's shared startup classification (not the
/// OKX table): six failures degrade startup once, carrying the native code.
#[test]
fn discovery_refusals_degrade_public_startup_with_their_native_code() {
    for (name, code) in [
        ("instruments-invalid-type", "51000"),
        ("swap-books-unknown-instrument", "51001"),
        ("spot-ticker-unknown-instrument", "51001"),
    ] {
        let fixture = &corpus()[name];
        let mut e = crate::support::engine(Options {
            public_data_only: true,
            ..Default::default()
        });
        e.start(0.0);
        let mut out = prepared(&mut e, 0.0);
        let mut now = 0.0;
        let mut degraded = None;
        for _ in 1..=6 {
            let request = out
                .iter()
                .find_map(|action| match action {
                    Action::Http(request) if request.url.contains("public/instruments") => {
                        Some(request.id)
                    }
                    _ => None,
                })
                .expect("a discovery read");
            e.handle(
                now,
                Event::HttpResponse {
                    request,
                    response: Response {
                        status: fixture.http_status.unwrap(),
                        headers: "",
                        body: &fixture.body,
                    },
                },
            );
            let answered = prepared(&mut e, now);
            degraded = degraded.or_else(|| {
                answered.iter().find_map(|a| match a {
                    Action::Notice(Notice::Event(event))
                        if event.code == code::STARTUP_DEGRADED =>
                    {
                        Some(event.clone())
                    }
                    _ => None,
                })
            });
            let (timer, at) = answered
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                    _ => None,
                })
                .expect("discovery retries");
            now = at;
            e.handle(now, Event::TimerFired { timer });
            out = prepared(&mut e, now);
        }
        let event = degraded.unwrap_or_else(|| panic!("{name}: startup never degraded"));
        assert!(
            event.fields.contains(&("venueCode", code.into())),
            "{name}: {:?}",
            event.fields
        );
    }
}

/// A failed candle read keeps OKX's native soft shape: a command error and an
/// empty result, never a typed failure.
#[test]
fn an_invalid_candle_bar_keeps_the_soft_empty_result() {
    let fixture = &corpus()["swap-candles-invalid-bar"];
    assert_eq!(fixture.expect.kind, None);
    let mut e = trading_engine();
    e.handle(
        5.0,
        Event::Command(Command::FetchCandles {
            command: 4,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::parse("1m").unwrap(),
            limit: Some(10),
            from_ms: None,
            to_ms: None,
        }),
    );
    let out = prepared(&mut e, 5.0);
    let Some(Action::Http(request)) = out
        .iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.contains("candles")))
    else {
        panic!("no candle read: {out:?}");
    };
    e.handle(
        6.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: fixture.http_status.unwrap(),
                headers: "",
                body: &fixture.body,
            },
        },
    );
    let out = prepared(&mut e, 6.0);
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 4, result }) if result == "[]")), "{out:?}");
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 4, .. })))
    );
}

/// Inline, not corpus: a write refused with a code the table does not list is
/// unproven on a 4xx, as `CODES.unlisted` (Unknown) says.
#[test]
fn an_unlisted_code_on_a_4xx_write_is_unknown() {
    let mut e = trading_engine();
    e.handle(
        5.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![
                serde_json::from_str(
                    r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000}"#,
                )
                .unwrap(),
            ],
        }),
    );
    let out = prepared(&mut e, 5.0);
    let Some(Action::Http(request)) = out.iter().find(|a| matches!(a, Action::Http(_))) else {
        panic!("no order write");
    };
    e.handle(
        6.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 400,
                headers: "",
                body: r#"{"code":"59999","msg":"unlisted","data":[]}"#,
            },
        },
    );
    let failure = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("the write fails");
    assert_eq!(
        failure.outcome,
        Some(Outcome::Unknown),
        "{}",
        failure.message
    );
}

/// Inline, not corpus: a non-key refusal on the account config read is
/// `request.failed` carrying the table kind (a key refusal ends startup instead).
#[test]
fn a_rate_limited_config_read_carries_its_kind() {
    let mut engine = crate::support::engine(Options {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Options::default()
    });
    engine.start(0.0);
    let mut cycle = Cycle {
        engine,
        now: 0.0,
        requests: Vec::new(),
        notices: Vec::new(),
    };
    cycle.collect();
    cycle.answer("instruments", 200, &envelope(MARKET));
    cycle.answer("tickers", 200, &envelope(TICKER));
    cycle.answer(
        "/api/v5/account/config",
        429,
        r#"{"code":"50011","msg":"Too Many Requests","data":[]}"#,
    );
    let event = cycle
        .notices
        .iter()
        .find_map(|notice| match notice {
            Notice::Event(event) if event.code == code::REQUEST_FAILED => Some(event),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{:?}", cycle.notices));
    assert!(
        event.fields.contains(&("kind", "rateLimited".into())),
        "{:?}",
        event.fields
    );
    assert!(event.fields.contains(&("venueCode", "50011".into())));
}
