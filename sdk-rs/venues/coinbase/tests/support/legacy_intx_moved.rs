//! Legacy INTX migration contract, selected only in this internal fixture.
//! Production gateway startup is covered by private_drb::tests.

use crate::{Coinbase, Options};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::{Action, ErrorKind, Event, HttpRequest, Level, Notice, Response, TimerId};
use std::collections::{HashMap, VecDeque};

// Approved S4a user-facing contract; all responses and credentials are local fixtures.
const MOVED: &str =
    "Coinbase International moved to Coinbase's new derivatives API; an update is coming.";

struct Rig {
    engine: safe_cex_engine::Engine<Coinbase>,
    now: f64,
    timers: HashMap<TimerId, f64>,
    requests: VecDeque<HttpRequest>,
}
impl Rig {
    fn actions(&mut self) -> Vec<Action> {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now);
        for action in &actions {
            match action {
                Action::SetTimer { timer, at_ms } => {
                    self.timers.insert(*timer, *at_ms);
                }
                Action::ClearTimer { timer } => {
                    self.timers.remove(timer);
                }
                _ => {}
            }
        }
        actions
    }
    fn request(&mut self) -> HttpRequest {
        for _ in 0..30 {
            for action in self.actions() {
                if let Action::Http(request) = action {
                    self.requests.push_back(request);
                }
            }
            if let Some(request) = self.requests.pop_front() {
                return request;
            }
            let (timer, at) = self
                .timers
                .iter()
                .min_by(|a, b| a.1.total_cmp(b.1))
                .map(|(timer, at)| (*timer, *at))
                .expect("scheduled request");
            self.timers.remove(&timer);
            self.now = self.now.max(at);
            self.engine.handle(self.now, Event::TimerFired { timer });
        }
        panic!("request did not arrive");
    }
    fn reply(&mut self, request: &HttpRequest, status: u16, body: &str) {
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
    }
    fn fail(&mut self, request: &HttpRequest) {
        self.engine.handle(
            self.now,
            Event::HttpFailed {
                request: request.id,
                error: "connection refused",
            },
        );
    }
}
fn start(private: bool, spot: bool) -> (Rig, HttpRequest) {
    let mut venue = Coinbase::new(Options {
        public_data_only: !private,
        spot_only: spot,
        key: if private {
            "organizations/test/apiKeys/test".into()
        } else {
            String::new()
        },
        secret: if private {
            STANDARD.encode([7; 32])
        } else {
            String::new()
        },
        ..Default::default()
    })
    .unwrap();
    venue.account.legacy_protocol_fixture = true;
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.start(1000.);
    let mut rig = Rig {
        engine,
        now: 1000.,
        timers: HashMap::new(),
        requests: VecDeque::new(),
    };
    let request = rig.request();
    (rig, request)
}

fn assert_moved(actions: &[Action]) {
    let failures = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::StartupFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        failures.len(),
        1,
        "one actionable startup failure: {actions:?}"
    );
    assert_eq!(failures[0].message, MOVED);
    assert_eq!(failures[0].kind, Some(ErrorKind::Unsupported));
    assert!(actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.level == Level::Error && event.message == MOVED
    )), "the visible error must carry the same message: {actions:?}");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
    );
}

fn assert_not_moved(actions: &[Action]) {
    assert!(
        !actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::StartupFailed { error, .. }) if error.message == MOVED
        ) || matches!(a,
            Action::Notice(Notice::Event(event)) if event.message == MOVED
        )),
        "unrelated failure must retain its original handling: {actions:?}"
    );
}

#[test]
fn unavailable_drb_discovery_retries_without_the_legacy_migration_message() {
    for status in [404, 410, 500, 502, 503, 504] {
        for body in [
            "<html>Bad Gateway</html>",
            "[]",
            r#"{"error":"Bad Gateway"}"#,
        ] {
            let (mut engine, request) = start(false, false);
            assert!(request.url.starts_with("https://drb.coinbase.com/api/v2/"));
            engine.reply(&request, status, body);
            let actions = engine.actions();
            assert_not_moved(&actions);
            assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
        }
    }
}

#[test]
fn unreachable_drb_discovery_retries_without_the_legacy_migration_message() {
    let (mut engine, request) = start(false, false);
    engine.fail(&request);
    let actions = engine.actions();
    assert_not_moved(&actions);
    assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
}

#[test]
fn malformed_successful_drb_response_keeps_retrying() {
    let (mut engine, request) = start(false, false);
    engine.reply(&request, 200, "<html>invalid</html>");
    let actions = engine.actions();
    assert_not_moved(&actions);
    assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
}

#[test]
fn drb_auth_and_rate_limit_responses_keep_their_existing_handling() {
    for status in [401, 403, 429] {
        let (mut engine, request) = start(false, false);
        engine.reply(&request, status, "{}");
        let actions = engine.actions();
        assert_not_moved(&actions);
        assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    }
}

#[test]
fn spot_discovery_and_unidentified_private_accounts_keep_their_existing_handling() {
    for (private, spot) in [(false, true), (true, false), (true, true)] {
        let (mut engine, request) = start(private, spot);
        engine.reply(&request, 502, "Bad Gateway");
        assert_not_moved(&engine.actions());
    }
}

fn probe(kind: &str) -> (Rig, HttpRequest) {
    let (mut engine, request) = start(true, false);
    engine.reply(
        &request,
        200,
        &format!(r#"{{"portfolios":[{{"uuid":"portfolio-one","type":"{kind}"}}]}}"#),
    );
    let request = engine.request();
    assert!(request.url.ends_with("/portfolios/portfolio-one"));
    (engine, request)
}

#[test]
fn only_known_intx_portfolio_probe_failure_reports_migration() {
    for kind in ["INTX", "DEFAULT"] {
        let (mut engine, request) = probe(kind);
        engine.reply(&request, 502, "Bad Gateway");
        let actions = engine.actions();
        if kind == "INTX" {
            assert_moved(&actions);
        } else {
            assert_not_moved(&actions);
        }
    }
}

#[test]
fn private_intx_catalogue_failure_reports_migration() {
    let (mut engine, request) = probe("INTX");
    engine.reply(&request, 200, r#"{"breakdown":{}}"#);
    let request = engine.request();
    assert!(request.url.contains("/products"));
    engine.reply(&request, 502, "Bad Gateway");
    assert_moved(&engine.actions());
}

#[test]
fn private_intx_reconciliation_failure_reports_migration_once() {
    for network in [false, true] {
        let (mut engine, request) = probe("INTX");
        engine.reply(&request, 200, r#"{"breakdown":{}}"#);
        let request = engine.request();
        engine.reply( &request, 200,
            r#"{"products":[{"product_id":"BTC-PERP-INTX","product_type":"FUTURE","base_currency_id":"BTC","quote_currency_id":"USDC","status":"TRADING","base_increment":"0.001","price_increment":"0.01","base_min_size":"0.001","base_max_size":"100","price":"100","volume_24h":"2","price_percentage_change_24h":"1","future_product_details":{"contract_expiry_type":"PERPETUAL","contract_root_unit":"BTC","contract_size":"1"}}]}"#);
        let mut actions = vec![];
        for path in ["/intx/portfolio/", "/intx/positions/"] {
            let request = engine.request();
            assert!(request.url.contains(path), "expected {path}: {request:?}");
            if network {
                engine.fail(&request);
            } else {
                engine.reply(&request, 502, "Bad Gateway");
            }
            actions.extend(engine.actions());
        }
        assert_moved(&actions);
    }
}

#[test]
fn cfm_catalogue_failure_keeps_its_existing_handling() {
    let (mut engine, request) = probe("DEFAULT");
    engine.reply(&request, 200, r#"{"breakdown":{}}"#);
    let request = engine.request();
    assert!(request.url.contains("/cfm/balance_summary"));
    engine.reply( &request, 200,
        r#"{"balance_summary":{"total_usd_balance":{"value":"10"},"available_margin":{"value":"8"},"unrealized_pnl":{"value":"1.5"}}}"#);
    for _ in 0..2 {
        let request = engine.request();
        engine.reply(&request, 502, "Bad Gateway");
        assert_not_moved(&engine.actions());
    }
}

#[test]
fn running_drb_public_feed_failure_keeps_retrying() {
    let (mut engine, request) = start(false, false);
    let body = drb_response("public/get_instruments", &request);
    engine.reply(&request, 200, &body);
    let request = engine.request();
    let body = drb_response("public/get_book_summary_by_currency", &request);
    engine.reply(&request, 200, &body);
    let actions = engine.actions();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 6000. => Some(*timer),
            _ => None,
        })
        .expect("public drb summary poll");
    engine.now = 6000.;
    engine
        .engine
        .handle(engine.now, Event::TimerFired { timer });
    let request = engine.request();
    engine.reply(&request, 502, "Bad Gateway");
    let actions = engine.actions();
    assert_not_moved(&actions);
    assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
}

fn drb_response(method: &str, request: &HttpRequest) -> String {
    let record = include_str!("../fixtures/drb-public.jsonl")
        .lines()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
        .find(|r| r["method"] == method)
        .unwrap();
    let sent: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    let mut body: serde_json::Value =
        serde_json::from_str(record["body"].as_str().unwrap()).unwrap();
    body["id"] = sent["id"].clone();
    body.to_string()
}
