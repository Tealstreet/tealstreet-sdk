//! Fee tiers belong to the trading account, independently of its API signer.
mod support;

use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, TimerId};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

const AGENT: &str = "0x0000000000000000000000000000000000000001";
const ACCOUNT: &str = "0x0000000000000000000000000000000000000002";
const VAULT: &str = "0x0000000000000000000000000000000000000003";

struct Fees {
    engine: Engine<Hyperliquid>,
    reads: Vec<(String, String)>,
    role: Option<HttpRequest>,
    role_timer: Option<TimerId>,
    now: f64,
    fee_status: u16,
}

impl Fees {
    fn start(options: Options, fee_status: u16) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(options));
        engine.start(1000.0);
        let mut harness = Self {
            engine,
            reads: Vec::new(),
            role: None,
            role_timer: None,
            now: 1000.0,
            fee_status,
        };
        harness.pump();
        harness
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

    fn pump(&mut self) {
        for _ in 0..30 {
            let actions = self.engine.drain_actions().collect::<Vec<_>>();
            if actions.is_empty() {
                return;
            }
            for action in actions {
                match action {
                    Action::SetTimer {
                        timer,
                        at_ms: 7000.0,
                    } => {
                        self.role_timer = Some(timer);
                    }
                    Action::Http(request) => {
                        let value: serde_json::Value =
                            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                        let kind = value["type"].as_str().unwrap();
                        self.reads
                            .push((kind.into(), value["user"].as_str().unwrap_or("").into()));
                        if kind == "userRole" {
                            self.role = Some(request);
                            continue;
                        }
                        let body = match kind {
                            "metaAndAssetCtxs" => support::META,
                            "perpDexs" => "[null]",
                            "userFees" => r#"{"userCrossRate":"0.0002","userAddRate":"0.00005"}"#,
                            "userAbstraction" => r#""disabled""#,
                            "maxBuilderFee" => "30",
                            "clearinghouseState" => support::STATE,
                            "frontendOpenOrders" => "[]",
                            other => panic!("unexpected read {other}"),
                        };
                        self.reply(
                            &request,
                            if kind == "userFees" {
                                self.fee_status
                            } else {
                                200
                            },
                            body,
                        );
                    }
                    _ => {}
                }
            }
        }
        panic!("account reads did not quiesce");
    }

    fn subjects(&self, kind: &str) -> Vec<&str> {
        self.reads
            .iter()
            .filter(|(k, _)| k == kind)
            .map(|(_, user)| user.as_str())
            .collect()
    }

    fn assert_tier(&self, user: &str) {
        assert_eq!(self.subjects("userFees"), [user]);
        assert!(!self.engine.store().markets().is_empty());
        for market in self.engine.store().markets() {
            assert_eq!(market.taker_fee, Some(0.0002));
            assert_eq!(market.maker_fee, Some(0.00005));
        }
        let fee = self
            .reads
            .iter()
            .position(|(kind, _)| kind == "userFees")
            .unwrap();
        let mode = self
            .reads
            .iter()
            .position(|(kind, _)| kind == "userAbstraction")
            .unwrap();
        assert!(
            fee < mode,
            "fee read must retain its place before private state reads"
        );
    }
}

fn agent() -> Options {
    Options {
        key: Some(AGENT.into()),
        ..Options::default()
    }
}

#[test]
fn configured_trading_account_fee_tier_does_not_use_its_agent() {
    let h = Fees::start(
        Options {
            application_id: Some(ACCOUNT.into()),
            ..agent()
        },
        200,
    );
    h.assert_tier(ACCOUNT);
    assert!(h.subjects("userRole").is_empty());
}

#[test]
fn vault_fee_tier_takes_precedence_over_master_and_agent() {
    let h = Fees::start(
        Options {
            application_id: Some(ACCOUNT.into()),
            vault: Some(VAULT.into()),
            ..agent()
        },
        200,
    );
    h.assert_tier(VAULT);
}

#[test]
fn fees_wait_for_role_discovery_then_use_the_resolved_trading_account() {
    for (role, user) in [
        (
            format!(r#"{{"role":"agent","data":{{"user":"{ACCOUNT}"}}}}"#),
            ACCOUNT,
        ),
        (r#"{"role":"vault"}"#.into(), AGENT),
        (r#"{"role":"user"}"#.into(), AGENT),
    ] {
        let mut h = Fees::start(agent(), 200);
        assert!(
            h.subjects("userFees").is_empty(),
            "unresolved signer must not supply fees"
        );
        assert_eq!(h.subjects("userRole"), [AGENT]);
        let request = h.role.take().unwrap();
        h.reply(&request, 200, &role);
        h.pump();
        h.assert_tier(user);
        h.reply(&request, 200, &role);
        h.pump();
        assert_eq!(
            h.subjects("userFees"),
            [user],
            "late duplicate must not bootstrap again"
        );
    }
}

#[test]
fn failed_or_malformed_role_never_queries_unresolved_fees() {
    for (status, body) in [
        (503, "unavailable"),
        (200, "{}"),
        (200, r#"{"role":"agent","data":{}}"#),
    ] {
        let mut h = Fees::start(agent(), 200);
        assert!(h.subjects("userFees").is_empty());
        let request = h.role.take().unwrap();
        h.reply(&request, status, body);
        h.pump();
        assert!(h.subjects("userFees").is_empty());
        h.reply(
            &request,
            200,
            &format!(r#"{{"role":"agent","data":{{"user":"{ACCOUNT}"}}}}"#),
        );
        h.pump();
        assert!(h.subjects("userFees").is_empty());
    }
}

#[test]
fn timed_out_role_ignores_a_late_success_without_fee_bootstrap() {
    let mut h = Fees::start(agent(), 200);
    assert!(h.subjects("userFees").is_empty());
    let request = h.role.take().unwrap();
    let timer = h.role_timer.unwrap();
    h.now = 7000.0;
    h.engine.handle(h.now, Event::TimerFired { timer });
    h.pump();
    h.now = 8000.0;
    h.reply(
        &request,
        200,
        &format!(r#"{{"role":"agent","data":{{"user":"{ACCOUNT}"}}}}"#),
    );
    h.pump();
    assert!(h.subjects("userFees").is_empty());
    assert!(h.subjects("userAbstraction").is_empty());
}

#[test]
fn anonymous_public_fee_fallback_still_loads_markets_without_private_reads() {
    for options in [
        Options::default(),
        Options {
            public_data_only: true,
            ..agent()
        },
    ] {
        let h = Fees::start(options, 422);
        assert!(h.subjects("userFees").is_empty());
        assert!(h.subjects("userRole").is_empty());
        assert!(h.subjects("userAbstraction").is_empty());
        assert!(h.engine.store().loaded().markets);
    }
}
