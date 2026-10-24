//! Bitget caller adoption slice 2 (`docs/plans/RUST_BITGET_DISCOVERY_CONTEXT.md`):
//! original REDs for Background Context custody over contracts and tickers
//! discovery, plus controls pinning the native wire the conversion preserves.
#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::{CONTRACTS, TICKERS};
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice, RequestId, TimerId};
use safe_cex_venue_bitget::{BitgetFutures, Product};
use std::collections::HashSet;

const DEADLINE_MS: f64 = 20_000.0;
const MARKETS: &str = "https://api.bitget.com/api/v2/mix/market/contracts?productType=usdt-futures";
const TICKER_URL: &str =
    "https://api.bitget.com/api/v2/mix/market/tickers?productType=usdt-futures";

struct Cycle {
    engine: Engine<BitgetFutures>,
    now: f64,
    requests: Vec<HttpRequest>,
    managed: HashSet<RequestId>,
    timers: Vec<(TimerId, f64)>,
    markets_published: usize,
}

impl Cycle {
    /// Public engine started; discovery reads it issues stay pending.
    fn started(options: Options) -> Self {
        let mut cycle = Self {
            engine: crate::support::engine(Options {
                public_data_only: true,
                ..options
            }),
            now: 0.0,
            requests: Vec::new(),
            managed: HashSet::new(),
            timers: Vec::new(),
            markets_published: 0,
        };
        cycle.engine.start(0.0);
        cycle.collect();
        cycle
    }

    /// Releases pending actions, recording which HTTP requests were managed.
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
                            self.managed.insert(request.id);
                            self.requests.push(request);
                        }
                    }
                    HttpAction::Cancel { .. } => {}
                },
                Action::Http(request) => self.requests.push(request),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::WsConnect { .. } => self.markets_published += 1,
                Action::Notice(Notice::StartupFailed { error }) => {
                    panic!("unexpected Bitget startup failure: {error}")
                }
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

    fn take(&mut self, url: &str) -> Option<HttpRequest> {
        let i = self.requests.iter().position(|r| r.url == url)?;
        Some(self.requests.remove(i))
    }

    fn reply(&mut self, request: RequestId, status: u16, body: &str, at: f64) {
        self.now = self.now.max(at);
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request,
                response: safe_cex_engine::Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }

    fn answer(&mut self, url: &str, body: &str, at: f64) {
        let request = self
            .take(url)
            .unwrap_or_else(|| panic!("no {url}: {:?}", self.urls()));
        self.reply(request.id, 200, body, at);
    }

    /// Fires every armed timer up to `until`, in time order.
    fn advance(&mut self, until: f64) {
        for _ in 0..10_000 {
            let next = self
                .timers
                .iter()
                .copied()
                .filter(|(_, at)| *at <= until)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((timer, at)) = next else {
                self.now = self.now.max(until);
                return;
            };
            self.timers.retain(|(t, _)| *t != timer);
            self.now = self.now.max(at);
            self.engine.handle(self.now, Event::TimerFired { timer });
            self.collect();
        }
        panic!("timer chain did not settle by {until}");
    }

    fn urls(&self) -> Vec<String> {
        self.requests.iter().map(|r| r.url.clone()).collect()
    }

    fn count(&self, url: &str) -> usize {
        self.requests.iter().filter(|r| r.url == url).count()
    }
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.as_str())
}

// ---- Original REDs ----

#[test]
fn contracts_read_releases_through_background_context() {
    let cycle = Cycle::started(Options::default());
    let markets = cycle
        .requests
        .iter()
        .find(|r| r.url == MARKETS)
        .unwrap_or_else(|| panic!("no contracts read: {:?}", cycle.urls()));
    assert!(
        cycle.managed.contains(&markets.id),
        "contracts released outside managed Context custody"
    );
}

#[test]
fn tickers_read_releases_through_background_context() {
    let mut cycle = Cycle::started(Options::default());
    cycle.answer(MARKETS, CONTRACTS, 1.0);
    let tickers = cycle
        .requests
        .iter()
        .find(|r| r.url == TICKER_URL)
        .unwrap_or_else(|| panic!("no tickers read: {:?}", cycle.urls()));
    assert!(
        cycle.managed.contains(&tickers.id),
        "tickers released outside managed Context custody"
    );
}

#[test]
fn an_unanswered_contracts_read_ends_at_the_shared_deadline_and_retries() {
    let mut cycle = Cycle::started(Options::default());
    let first = cycle.take(MARKETS).expect("contracts read");
    cycle.advance(DEADLINE_MS + 30_000.0);
    assert!(
        cycle
            .requests
            .iter()
            .any(|r| r.url == MARKETS && r.id != first.id),
        "discovery stayed latched on an unanswered read: {:?}",
        cycle.urls()
    );
}

#[test]
fn a_contracts_reply_after_its_deadline_publishes_nothing() {
    let mut cycle = Cycle::started(Options::default());
    let first = cycle.take(MARKETS).expect("contracts read");
    cycle.advance(DEADLINE_MS + 1.0);
    cycle.reply(first.id, 200, CONTRACTS, DEADLINE_MS + 2.0);
    assert_eq!(
        cycle.count(TICKER_URL),
        0,
        "an expired contracts reply published and chained tickers: {:?}",
        cycle.urls()
    );
    assert_eq!(
        cycle.markets_published, 0,
        "an expired reply opened sockets"
    );
}

// ---- Native controls (GREEN on the native path, preserved by the cutover) ----

#[test]
fn discovery_keeps_exact_urls_header_and_no_signature() {
    let mut cycle = Cycle::started(Options::default());
    let markets = cycle.take(MARKETS).expect("contracts read");
    assert_eq!(header(&markets, "X-CHANNEL-API-CODE"), Some("Tealstreet"));
    assert!(
        markets.signer_headers.is_empty(),
        "public discovery is unsigned"
    );
    assert!(markets.body.is_none());
    cycle.reply(markets.id, 200, CONTRACTS, 1.0);
    let tickers = cycle.take(TICKER_URL).expect("tickers read");
    assert_eq!(header(&tickers, "X-CHANNEL-API-CODE"), Some("Tealstreet"));
    assert!(tickers.signer_headers.is_empty());
}

#[test]
fn product_and_testnet_select_the_native_product_type() {
    for (product, testnet, wanted) in [
        (Product::Usdt, true, "susdt-futures"),
        (Product::Usdc, false, "usdc-futures"),
        (Product::Inverse, false, "coin-futures"),
    ] {
        let cycle = Cycle::started(Options {
            product,
            testnet,
            ..Options::default()
        });
        assert!(
            cycle
                .urls()
                .iter()
                .any(|u| u.ends_with(&format!("/contracts?productType={wanted}"))),
            "{wanted}: {:?}",
            cycle.urls()
        );
    }
}

#[test]
fn contracts_then_tickers_then_public_sockets_in_native_order() {
    let mut cycle = Cycle::started(Options::default());
    assert_eq!(
        cycle.count(TICKER_URL),
        0,
        "tickers waits for the catalogue"
    );
    cycle.answer(MARKETS, CONTRACTS, 1.0);
    assert_eq!(cycle.count(TICKER_URL), 1);
    assert_eq!(
        cycle.markets_published, 1,
        "catalogue opens the public socket"
    );
    cycle.answer(TICKER_URL, TICKERS, 2.0);
    assert_eq!(
        cycle.markets_published, 3,
        "tickers open the two remaining sockets"
    );
    assert!(cycle.engine.store().market("BTCUSDT").is_some());
}

#[test]
fn an_empty_catalogue_backs_off_without_publishing() {
    let mut cycle = Cycle::started(Options::default());
    cycle.answer(MARKETS, r#"{"code":"00000","data":[]}"#, 1.0);
    assert_eq!(cycle.count(TICKER_URL), 0);
    assert_eq!(cycle.markets_published, 0);
    assert!(
        cycle.timers.iter().any(|(_, at)| *at > 1.0),
        "native discovery backoff armed"
    );
}

#[test]
fn an_http_error_backs_off_without_publishing() {
    let mut cycle = Cycle::started(Options::default());
    let markets = cycle.take(MARKETS).expect("contracts read");
    cycle.reply(markets.id, 500, r#"{"code":"40000","msg":"busy"}"#, 1.0);
    assert_eq!(cycle.count(TICKER_URL), 0);
    assert_eq!(cycle.markets_published, 0);
    assert!(cycle.timers.iter().any(|(_, at)| *at > 1.0));
}

#[test]
fn a_refused_contracts_preparation_backs_off_and_retries_through_managed_custody() {
    let mut engine = crate::support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    engine.start(0.0);
    engine.prepare_dispatch(0.0);
    let preparation = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(http) => match *http {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url == MARKETS => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("managed contracts preparation");
    engine.http_preparation_failed(
        1.0,
        preparation,
        safe_cex_engine::request_context::Error::Stale,
        safe_cex_engine::VenueError::new("route refused"),
    );
    let mut cycle = Cycle {
        engine,
        now: 1.0,
        requests: Vec::new(),
        managed: HashSet::new(),
        timers: Vec::new(),
        markets_published: 0,
    };
    cycle.collect();
    assert!(
        cycle.requests.is_empty(),
        "a refused preparation sends nothing"
    );
    assert_eq!(cycle.markets_published, 0);
    assert!(
        cycle.timers.iter().any(|(_, at)| *at > 1.0),
        "native discovery backoff armed"
    );
    cycle.advance(30_000.0);
    let retry = cycle
        .requests
        .iter()
        .find(|r| r.url == MARKETS)
        .unwrap_or_else(|| panic!("no contracts retry: {:?}", cycle.urls()));
    assert!(
        cycle.managed.contains(&retry.id),
        "the retry is managed too"
    );
}

/// Drives `attempts` real contracts preparations, each refused with full venue evidence.
fn refused_preparations(attempts: usize) -> Vec<Action> {
    use safe_cex_engine::{ErrorKind, VenueError};
    let mut engine = crate::support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    let mut now = 0.0;
    engine.start(now);
    let mut actions: Vec<_> = engine.drain_actions().collect();
    for attempt in 0..attempts {
        let preparation = actions
            .iter()
            .find_map(|action| match action {
                Action::ManagedHttp(http) => match &**http {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } if request.url == MARKETS => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("real contracts Prepare");
        assert!(engine.http_preparation_pending(preparation));
        engine.http_preparation_failed(
            now,
            preparation,
            safe_cex_engine::request_context::Error::Invalid,
            VenueError::new("route refused")
                .kind(ErrorKind::InvalidInput)
                .venue_code("ROUTE_REFUSED")
                .venue_message("policy declined"),
        );
        assert!(!engine.http_preparation_pending(preparation));
        actions = engine.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::ManagedHttp(http) if matches!(&**http, HttpAction::Bind { .. })
        )));
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1, "one native retry, with the read retired");
        if attempt + 1 < attempts {
            now = timers[0].1;
            engine.handle(now, Event::TimerFired { timer: timers[0].0 });
            actions = engine.drain_actions().collect();
        }
    }
    actions
}

fn degraded(actions: &[Action]) -> &safe_cex_engine::LogEvent {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == "startup.degraded" => Some(event),
            _ => None,
        })
        .expect("six real preparation failures reach native degradation")
}

#[test]
fn an_invalid_preparation_retires_and_backs_off_without_release() {
    let actions = refused_preparations(1);
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::Event(event)) if event.code == "request.failed"
            ))
            .count(),
        1
    );
}

#[test]
fn an_invalid_preparation_keeps_its_typed_cause_into_degradation() {
    let actions = refused_preparations(6);
    let event = degraded(&actions);
    assert!(
        event.fields.contains(&(
            "kind",
            safe_cex_engine::ErrorKind::InvalidInput.as_str().into()
        )),
        "{:?}",
        event.fields
    );
}

#[test]
fn an_invalid_preparation_keeps_its_venue_evidence_into_degradation() {
    let actions = refused_preparations(6);
    let event = degraded(&actions);
    assert!(
        event
            .fields
            .contains(&("venueCode", "ROUTE_REFUSED".into())),
        "{:?}",
        event.fields
    );
    assert!(
        event
            .fields
            .contains(&("venueMessage", "policy declined".into()))
    );
}
