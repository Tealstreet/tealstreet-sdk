//! OKX caller adoption slice 3 (`docs/plans/RUST_OKX_ACCOUNT_CYCLE.md`):
//! original REDs for Background Context custody over the account-cycle reads,
//! plus controls pinning the native wire the conversion must preserve.
#![allow(dead_code)]
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::signing::{Provider, SignError, SignFuture, SignRequest, SignResult, Signer};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice, RequestId, TimerId};
use safe_cex_venue_okx::Okx;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const DEADLINE_MS: f64 = 20_000.0;
const BALANCE: &str = "/api/v5/account/account-position-risk?instType=SWAP";
const POSITIONS: &str = "/api/v5/account/positions?instType=SWAP";
const ORDERS: &str = "/api/v5/trade/orders-pending?instType=SWAP";

struct Cycle {
    engine: Engine<Okx>,
    now: f64,
    requests: Vec<HttpRequest>,
    managed: HashSet<RequestId>,
    timers: Vec<(TimerId, f64)>,
    auth_notices: usize,
}

impl Cycle {
    /// Startup through login; the account-cycle reads it issues stay pending.
    fn started(read_key: bool) -> Self {
        Self::started_with(read_key, |_| {})
    }

    fn started_with(read_key: bool, configure: impl FnOnce(&mut Engine<Okx>)) -> Self {
        let opts = Options {
            key: Some("test-key".into()),
            secret: Some("test-secret".into()),
            passphrase: Some("test-passphrase".into()),
            read_credentials: read_key.then(|| safe_cex_engine::ReadCredentials {
                key: "read-key".into(),
                secret: "read-secret".into(),
                passphrase: Some("read-passphrase".into()),
            }),
            ..Options::default()
        };
        let mut engine = crate::support::engine(opts);
        configure(&mut engine);
        let mut cycle = Self {
            engine,
            now: 0.0,
            requests: Vec::new(),
            managed: HashSet::new(),
            timers: Vec::new(),
            auth_notices: 0,
        };
        cycle.engine.start(0.0);
        cycle.collect();
        cycle.answer("instruments", &envelope(MARKET), 1.0);
        cycle.answer("tickers", &envelope(TICKER), 2.0);
        cycle.answer(
            "/api/v5/account/config",
            &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
            3.0,
        );
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
                Action::Notice(Notice::AccountAuth { .. } | Notice::Deactivate) => {
                    self.auth_notices += 1
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

// ---- Original REDs ----

#[test]
fn account_cycle_reads_release_through_background_context() {
    let cycle = Cycle::started(false);
    for suffix in [BALANCE, POSITIONS, ORDERS] {
        let request = cycle
            .requests
            .iter()
            .find(|r| r.url.ends_with(suffix))
            .unwrap_or_else(|| panic!("no {suffix}: {:?}", cycle.urls()));
        assert!(
            cycle.managed.contains(&request.id),
            "{suffix} released outside managed Context custody"
        );
    }
}

#[test]
fn an_unanswered_balance_read_ends_at_the_shared_deadline_and_reads_again() {
    let mut cycle = Cycle::started(false);
    let first = cycle.take(BALANCE).expect("first balance read");
    let issued = cycle.now;
    cycle.advance(issued + DEADLINE_MS + 15_000.0);
    assert!(
        cycle
            .requests
            .iter()
            .any(|r| r.url.ends_with(BALANCE) && r.id != first.id),
        "the account join stayed latched on an unanswered read: {:?}",
        cycle.urls()
    );
}

#[test]
fn an_unanswered_orders_page_ends_at_the_shared_deadline_and_reads_again() {
    let mut cycle = Cycle::started(false);
    let first = cycle.take(ORDERS).expect("first orders page");
    let issued = cycle.now;
    cycle.advance(issued + DEADLINE_MS + 15_000.0);
    assert!(
        cycle
            .requests
            .iter()
            .any(|r| r.url.ends_with(ORDERS) && r.id != first.id),
        "the orders walk stayed latched in flight: {:?}",
        cycle.urls()
    );
}

// ---- Compatibility controls ----

#[test]
fn account_cycle_reads_keep_exact_urls_and_the_read_key() {
    let cycle = Cycle::started(true);
    for suffix in [BALANCE, POSITIONS, ORDERS] {
        assert_eq!(cycle.count(suffix), 1, "{suffix}: {:?}", cycle.urls());
        let request = cycle
            .requests
            .iter()
            .find(|r| r.url.ends_with(suffix))
            .unwrap();
        assert_eq!(request.method, safe_cex_engine::Method::Get);
        assert_eq!(
            header(request, "OK-ACCESS-KEY"),
            Some("read-key"),
            "{suffix}"
        );
    }
}

#[test]
fn balance_and_positions_commit_together() {
    let mut cycle = Cycle::started(false);
    cycle.answer(
        "account-position-risk",
        &envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#),
        5.0,
    );
    assert!(!cycle.engine.store().loaded().balance);
    cycle.answer(
        "/positions?",
        &envelope(r#"{"instId":"BTC-USDT-SWAP","pos":"5","posSide":"net","avgPx":"39000","lever":"10","upl":"50","liqPx":"30000","mgnMode":"cross","imr":"200","mmr":"20"}"#),
        5.0,
    );
    // Every remaining product answers empty until the join commits.
    for _ in 0..4 {
        if cycle.engine.store().loaded().balance {
            break;
        }
        if cycle.requests.iter().any(|r| r.url.contains("/positions?")) {
            cycle.answer("/positions?", &envelope(""), 6.0);
        }
    }
    assert!(cycle.engine.store().loaded().balance);
    assert!(cycle.engine.store().loaded().positions);
}

const ORDER_ROW: &str = r#"{"instId":"BTC-USDT-SWAP","ordId":"ID","ordType":"limit","state":"live","side":"buy","sz":"4","accFillSz":"0","px":"38000","lever":"10","reduceOnly":"false","clOrdId":""}"#;

fn full_orders_page(start: usize) -> String {
    let rows: Vec<String> = (start..start + 100)
        .map(|n| ORDER_ROW.replace("\"ID\"", &format!("\"{n}\"")))
        .collect();
    format!(r#"{{"code":"0","msg":"","data":[{}]}}"#, rows.join(","))
}

#[test]
fn an_orders_cursor_continuation_is_its_own_managed_page() {
    let mut cycle = Cycle::started(false);
    cycle.answer("orders-pending", &full_orders_page(1), 5.0);
    let next = cycle
        .requests
        .iter()
        .find(|r| r.url.contains("orders-pending") && r.url.contains("after=100"))
        .unwrap_or_else(|| panic!("no cursor page: {:?}", cycle.urls()));
    assert!(
        cycle.managed.contains(&next.id),
        "cursor page released outside custody"
    );
}

#[test]
fn a_walk_longer_than_one_deadline_completes_page_by_page() {
    let mut cycle = Cycle::started(false);
    // Each page answers 15 s after it is issued, so the walk outlasts one 20 s bound.
    let mut at = cycle.now;
    for page in 0..2 {
        at += 15_000.0;
        cycle.advance(at);
        let body = if page == 0 {
            full_orders_page(1)
        } else {
            envelope("")
        };
        cycle.answer("orders-pending", &body, at);
    }
    // The walk advanced to the next order kind, so page two was accepted.
    assert!(
        cycle
            .requests
            .iter()
            .any(|r| r.url.contains("orders-algo-pending")),
        "the second page was not accepted inside its own deadline: {:?}",
        cycle.urls()
    );
}

/// Refuses synchronously once armed, so managed preparation sees it.
struct Refusing {
    provider: Provider,
    refuse: Arc<AtomicBool>,
}
impl Signer for Refusing {
    fn sign(&self, request: SignRequest) -> SignFuture {
        self.provider.sign(request)
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if self.refuse.load(Ordering::SeqCst) && request.context.method == "GET" {
            Some(Err(SignError::Refused))
        } else {
            self.provider.sign_ready(request)
        }
    }
}

#[test]
fn a_provider_refusal_sends_no_account_read_and_reports_no_auth() {
    let refuse = Arc::new(AtomicBool::new(false));
    let armed = refuse.clone();
    let mut cycle = Cycle::started_with(false, move |engine| {
        let provider = engine.signer().unwrap();
        engine
            .set_signer(Some(Arc::new(Refusing {
                provider,
                refuse: armed,
            })))
            .unwrap();
    });
    // Startup already released the first cycle; refuse every later account read.
    for suffix in [BALANCE, POSITIONS, ORDERS] {
        cycle.take(suffix);
    }
    refuse.store(true, Ordering::SeqCst);
    let before = cycle.requests.len();
    cycle.advance(cycle.now + 2.0 * DEADLINE_MS);
    let sent: Vec<_> = cycle.requests[before..]
        .iter()
        .filter(|r| r.url.contains("/api/v5/account/") || r.url.contains("/api/v5/trade/orders"))
        .map(|r| r.url.clone())
        .collect();
    assert!(
        sent.is_empty(),
        "a refused account read was released: {sent:?}"
    );
    assert_eq!(
        cycle.auth_notices, 0,
        "a provider refusal is not auth evidence"
    );
}
