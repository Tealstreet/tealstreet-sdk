//! OKX caller adoption slice 4 (`docs/plans/RUST_OKX_DISCOVERY.md`): original
//! REDs for Background Context custody over market and ticker discovery, plus
//! controls pinning the native wire the conversion must preserve.
#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, RequestId, TimerId};
use safe_cex_venue_okx::Okx;
use std::collections::HashSet;

const DEADLINE_MS: f64 = 20_000.0;
const PUBLIC_MARKETS: &str = "/api/v5/public/instruments?instType=SWAP";
const PRIVATE_MARKETS: &str = "/api/v5/account/instruments?instType=SWAP";

struct Cycle {
    engine: Engine<Okx>,
    now: f64,
    requests: Vec<HttpRequest>,
    managed: HashSet<RequestId>,
    timers: Vec<(TimerId, f64)>,
}

impl Cycle {
    /// Engine started; discovery reads it issues stay pending.
    fn started(opts: Options) -> Self {
        let mut cycle = Self {
            engine: crate::support::engine(opts),
            now: 0.0,
            requests: Vec::new(),
            managed: HashSet::new(),
            timers: Vec::new(),
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

    fn take(&mut self, suffix: &str) -> Option<HttpRequest> {
        let i = self.requests.iter().position(|r| r.url.ends_with(suffix))?;
        Some(self.requests.remove(i))
    }

    fn answer(&mut self, contains: &str, body: &str, at: f64) {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.contains(contains))
            .unwrap_or_else(|| panic!("no {contains}: {:?}", self.urls()));
        let request = self.requests.remove(i);
        self.now = self.now.max(at);
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
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

    fn count(&self, suffix: &str) -> usize {
        self.requests
            .iter()
            .filter(|r| r.url.ends_with(suffix))
            .count()
    }
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    let native = request.headers.iter().find(|(key, _)| *key == name);
    let signer = request.signer_headers.iter().find(|(key, _)| key == name);
    native
        .map(|(_, value)| value.as_str())
        .or(signer.map(|(_, value)| value.as_str()))
}

fn credentialed(eea: bool) -> Options {
    Options {
        eea,
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..Options::default()
    }
}

// ---- Original REDs ----

#[test]
fn public_discovery_reads_release_through_background_context() {
    let mut cycle = Cycle::started(Options::default());
    let markets = cycle.take(PUBLIC_MARKETS).expect("public instruments read");
    assert!(
        cycle.managed.contains(&markets.id),
        "instruments released outside managed Context custody"
    );
    cycle.engine.handle(
        1.0,
        Event::HttpResponse {
            request: markets.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: &envelope(MARKET),
            },
        },
    );
    cycle.now = 1.0;
    cycle.collect();
    let tickers = cycle
        .requests
        .iter()
        .find(|r| r.url.contains("/api/v5/market/tickers?instType=SWAP"))
        .unwrap_or_else(|| panic!("no tickers read: {:?}", cycle.urls()));
    assert!(
        cycle.managed.contains(&tickers.id),
        "tickers released outside managed Context custody"
    );
}

#[test]
fn an_unanswered_instruments_read_ends_at_the_shared_deadline_and_retries() {
    let mut cycle = Cycle::started(Options::default());
    let first = cycle.take(PUBLIC_MARKETS).expect("public instruments read");
    cycle.advance(DEADLINE_MS + 30_000.0);
    assert!(
        cycle
            .requests
            .iter()
            .any(|r| r.url.ends_with(PUBLIC_MARKETS) && r.id != first.id),
        "discovery stayed latched on an unanswered read: {:?}",
        cycle.urls()
    );
}

#[test]
fn eea_private_instruments_read_is_managed_and_signed() {
    let cycle = Cycle::started(credentialed(true));
    let read = cycle
        .requests
        .iter()
        .find(|r| r.url.ends_with(PRIVATE_MARKETS))
        .unwrap_or_else(|| panic!("no account instruments read: {:?}", cycle.urls()));
    assert_eq!(header(read, "OK-ACCESS-KEY"), Some("test-key"));
    assert!(
        cycle.managed.contains(&read.id),
        "EEA instruments released outside managed Context custody"
    );
}

// ---- Compatibility controls ----

#[test]
fn public_discovery_is_unsigned_with_exact_urls() {
    let cycle = Cycle::started(credentialed(false));
    let read = cycle
        .requests
        .iter()
        .find(|r| r.url.ends_with(PUBLIC_MARKETS))
        .unwrap_or_else(|| panic!("no public instruments read: {:?}", cycle.urls()));
    assert_eq!(read.method, safe_cex_engine::Method::Get);
    assert_eq!(header(read, "OK-ACCESS-KEY"), None);
    assert!(
        !cycle
            .requests
            .iter()
            .any(|r| r.url.ends_with(PRIVATE_MARKETS))
    );
}

#[test]
fn demo_discovery_keeps_the_simulated_trading_header() {
    let cycle = Cycle::started(Options {
        testnet: true,
        ..Options::default()
    });
    let read = cycle
        .requests
        .iter()
        .find(|r| r.url.contains("/instruments?instType=SWAP"))
        .unwrap_or_else(|| panic!("no instruments read: {:?}", cycle.urls()));
    assert_eq!(header(read, "x-simulated-trading"), Some("1"));
}
