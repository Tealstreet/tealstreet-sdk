//! Native caller evidence before managed recipe and FetchLeverage adoption.
use safe_cex_engine::request_context::{HttpAction, RoutingIntent};
use safe_cex_engine::signing::{SignFuture, SignRequest, Signer};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, ReadCredentials, Response,
};
use safe_cex_venue_blofin::{Blofin, Options};
use std::sync::{Arc, Mutex};

const NOW: f64 = 1_700_000_000_123.;
const PATH: &str = "/api/v1/account/batch-leverage-info?instId=BTC-USDT&marginMode=cross";
const MARKET: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;
const RESULT: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"12"}]}"#;
const SECONDARY_SIGNATURE: &str =
    "Y2IxNzMwYjY0NzY0YzU3MmM0YzNmODNmOGRlY2Q1MDNjMDcxNmQ5MGZiMGRiMzU4ZWNhYjg3Yjg1MDkxZDFkYg==";
const PRIMARY_SIGNATURE: &str =
    "MDIzM2U3NzAzYjk1ZWFmYjhlMGExY2IyOWYwODcyZDEwNWQ4YjQyM2M1NGQwOThjYzU2NWQzNTM3MWE1NTczZg==";
type Draws = Arc<Mutex<Vec<usize>>>;

fn options(secondary: bool) -> Options {
    Options {
        key: Some("fixture-primary-key".into()),
        secret: Some("fixture-primary-secret".into()),
        passphrase: Some("fixture-primary-pass".into()),
        read_credentials: secondary.then(|| ReadCredentials {
            key: "fixture-secondary-key".into(),
            secret: "fixture-secondary-secret".into(),
            passphrase: Some("fixture-secondary-pass".into()),
        }),
        ..Default::default()
    }
}

struct Rig {
    engine: Engine<Blofin>,
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
    draws: Draws,
}

impl Rig {
    fn new(options: Options, parked: bool, filtered: bool) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Blofin::new(options));
        if parked {
            engine.set_signer(Some(Arc::new(Parked))).unwrap();
        }
        let draws = Draws::default();
        let recorded = draws.clone();
        engine
            .set_signing_entropy(Box::new(move |bytes| {
                recorded.lock().unwrap().push(bytes.len());
                bytes.fill(7);
                if filtered {
                    for (index, byte) in bytes.iter_mut().enumerate() {
                        *byte = if index == 0 {
                            4
                        } else if index % 2 == 1 {
                            8
                        } else {
                            44
                        };
                    }
                }
                Ok(())
            }))
            .unwrap();
        engine.start(NOW - 10.);
        let mut rig = Self {
            engine,
            requests: vec![],
            actions: vec![],
            draws,
        };
        rig.collect(NOW - 10.);
        rig
    }

    fn ready(options: Options, parked: bool, filtered: bool) -> Self {
        let mut rig = Self::new(options, parked, filtered);
        rig.reply("/market/instruments", MARKET);
        rig.reply(
            "/account/margin-mode",
            r#"{"code":"0","data":{"marginMode":"cross"}}"#,
        );
        rig.reply(
            "/account/position-mode",
            r#"{"code":"0","data":{"positionMode":"net_mode"}}"#,
        );
        rig.reply("/market/tickers", r#"{"code":"0","data":[]}"#);
        assert!(rig.engine.store().market("BTCUSDT").is_some());
        rig.actions.clear();
        rig.draws.lock().unwrap().clear();
        rig
    }

    fn collect(&mut self, now: f64) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, now) {
            if let Action::Http(request) = action {
                self.requests.push(request);
            } else {
                self.actions.push(action);
            }
        }
    }

    fn reply(&mut self, suffix: &str, body: &str) {
        let index = self
            .requests
            .iter()
            .position(|request| request.url.ends_with(suffix))
            .unwrap_or_else(|| panic!("native setup missing {suffix}"));
        let request = self.requests.remove(index);
        self.engine.handle(
            NOW,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect(NOW);
    }

    fn lookup(&mut self) -> Vec<Action> {
        self.engine
            .handle_command(
                NOW,
                Command::FetchLeverage {
                    command: 71,
                    symbol: "BTCUSDT".into(),
                },
                Some(20_000),
            )
            .unwrap();
        self.engine.drain_actions().collect()
    }
}

struct Parked;
impl Signer for Parked {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
}

fn request(actions: &[Action]) -> &HttpRequest {
    let mut found = actions.iter().filter_map(|action| match action {
        Action::Http(request) if request.url.ends_with(PATH) => Some(request),
        _ => None,
    });
    let request = found.next().expect("one genuine leverage HTTP");
    assert!(found.next().is_none());
    request
}

fn headers(key: &str, passphrase: &str, signature: &str) -> Vec<(&'static str, String)> {
    vec![
        ("ACCESS-KEY", key.into()),
        ("ACCESS-SIGN", signature.into()),
        ("ACCESS-NONCE", "m".repeat(21)),
        ("ACCESS-TIMESTAMP", "1700000000123".into()),
        ("ACCESS-PASSPHRASE", passphrase.into()),
        ("Content-Type", "application/json".into()),
    ]
}

fn prepared_intent(actions: &[Action], intent: RoutingIntent) {
    let packets = actions
        .iter()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    request, intent, ..
                } if request.url.ends_with(PATH) => Some((request, intent)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(packets.len(), 1, "leverage needs a genuine managed Prepare");
    assert_eq!(*packets[0].1, intent);
    assert_eq!(
        packets[0].0.headers,
        [("Content-Type", "application/json".into())]
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

#[test]
fn secondary_get_is_an_unsigned_managed_preparation() {
    let mut rig = Rig::ready(options(true), false, false);
    prepared_intent(&rig.lookup(), RoutingIntent::Secondary);
}

#[test]
fn primary_fallback_get_is_an_unsigned_managed_preparation() {
    let mut rig = Rig::ready(options(false), false, false);
    prepared_intent(&rig.lookup(), RoutingIntent::Primary);
}

#[test]
fn nonce_entropy_waits_for_route_and_signing_preparation() {
    let mut rig = Rig::ready(options(true), false, false);
    let actions = rig.lookup();
    assert!(
        rig.draws.lock().unwrap().is_empty(),
        "issuing a Prepare must not spend signing entropy"
    );
    prepared_intent(&actions, RoutingIntent::Secondary);
}

#[test]
fn parked_provider_cannot_be_bypassed_by_native_signed_http() {
    let mut rig = Rig::ready(options(true), true, false);
    let actions = rig.lookup();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "no private HTTP while the genuine provider is parked"
    );
    prepared_intent(&actions, RoutingIntent::Secondary);
}

#[test]
fn delayed_action_handoff_does_not_keep_the_staging_stamp() {
    let mut rig = Rig::ready(options(true), false, false);
    let mut actions = rig.lookup();
    rig.engine.prepare_dispatch(NOW + 800.);
    actions.extend(rig.engine.drain_actions());
    let request = request(&actions);
    assert_eq!(
        request
            .headers
            .iter()
            .find(|(name, _)| *name == "ACCESS-TIMESTAMP")
            .unwrap()
            .1,
        "1700000000923",
        "delayed handoff is evidence of stale staging, not a physical Claim proof"
    );
}

#[test]
fn native_secondary_get_preserves_the_literal_hmac_and_header_order() {
    let mut rig = Rig::ready(options(true), false, false);
    let actions = rig.lookup();
    let request = request(&actions);
    assert_eq!(request.method, Method::Get);
    assert_eq!(request.url, format!("https://openapi.blofin.com{PATH}"));
    assert_eq!(request.body, None);
    assert_eq!(
        request.headers,
        headers(
            "fixture-secondary-key",
            "fixture-secondary-pass",
            SECONDARY_SIGNATURE,
        )
    );
}

#[test]
fn native_get_without_secondary_uses_primary_key_secret_and_passphrase() {
    let mut rig = Rig::ready(options(false), false, false);
    assert_eq!(
        request(&rig.lookup()).headers,
        headers(
            "fixture-primary-key",
            "fixture-primary-pass",
            PRIMARY_SIGNATURE,
        )
    );
}

#[test]
fn absent_secondary_passphrase_inherits_only_the_primary_passphrase() {
    let mut opts = options(true);
    opts.read_credentials.as_mut().unwrap().passphrase = None;
    let mut rig = Rig::ready(opts, false, false);
    assert_eq!(
        request(&rig.lookup()).headers,
        headers(
            "fixture-secondary-key",
            "fixture-primary-pass",
            SECONDARY_SIGNATURE,
        )
    );
}

#[test]
fn explicit_empty_secondary_passphrase_refuses_before_discovery_or_entropy() {
    let mut opts = options(true);
    opts.read_credentials.as_mut().unwrap().passphrase = Some(String::new());
    let rig = Rig::new(opts, false, false);
    assert!(rig.requests.is_empty());
    assert!(rig.draws.lock().unwrap().is_empty());
    assert!(rig.actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::Event(event)) if event.message.contains("passphrase"))));
    assert!(!rig.actions.iter().any(|action| matches!(
        action,
        Action::ManagedHttp(_) | Action::Sign { .. } | Action::WsConnect { .. }
    )));
}

#[test]
fn secondary_does_not_supply_a_missing_primary_or_bypass_public_only() {
    for public_only in [false, true] {
        let mut opts = options(true);
        opts.public_data_only = public_only;
        if !public_only {
            opts.key = None;
            opts.secret = None;
        }
        let mut rig = Rig::new(opts, false, false);
        rig.reply("/market/instruments", MARKET);
        assert!(rig.draws.lock().unwrap().is_empty());
        assert!(
            rig.requests
                .iter()
                .all(|request| !request.url.contains("/account/"))
        );
        assert!(!rig.actions.iter().any(|action| matches!(action,
            Action::WsConnect { url, .. } if url.ends_with("/private"))));
    }
}

#[test]
fn native_nonce_uses_one_21_byte_draw_and_filters_both_alphabet_symbols() {
    let mut rig = Rig::ready(options(true), false, true);
    let actions = rig.lookup();
    assert_eq!(*rig.draws.lock().unwrap(), [21]);
    let request = request(&actions);
    assert_eq!(request.headers[2], ("ACCESS-NONCE", "n".into()));
    assert_eq!(request.headers[1], ("ACCESS-SIGN", "ZTYxMjQ1MTI0MGU5YzgzYjNhOGY2OTZmYzAyM2YwY2Q5YTljNDRjNjEyMDE1NjY2MWEwM2M1ZTMxNWJkMmFhZg==".into()));
}

#[test]
fn native_entropy_failure_is_terminal_without_provider_or_transport() {
    let mut rig = Rig::ready(options(true), false, false);
    // Construction-time entropy setup stays immutable; a shared flag changes only the source result.
    let mut engine = safe_cex_engine::test_support::seeded_engine(Blofin::new(options(true)));
    let fail = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let source = fail.clone();
    engine
        .set_signing_entropy(Box::new(move |bytes| {
            if source.load(std::sync::atomic::Ordering::SeqCst) {
                return Err("fixture signing entropy unavailable".into());
            }
            bytes.fill(7);
            Ok(())
        }))
        .unwrap();
    rig.engine = engine;
    rig.requests.clear();
    rig.actions.clear();
    rig.engine.start(NOW - 10.);
    rig.collect(NOW - 10.);
    rig.reply("/market/instruments", MARKET);
    rig.reply(
        "/account/margin-mode",
        r#"{"code":"0","data":{"marginMode":"cross"}}"#,
    );
    rig.reply(
        "/account/position-mode",
        r#"{"code":"0","data":{"positionMode":"net_mode"}}"#,
    );
    fail.store(true, std::sync::atomic::Ordering::SeqCst);
    let actions = rig.lookup();
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
    )));
    assert_eq!(actions.iter().filter(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command:71, error }) if error.message == "fixture signing entropy unavailable")).count(),1);
}

#[test]
fn native_leverage_response_keeps_financial_result_and_no_positions() {
    let mut rig = Rig::ready(options(true), false, false);
    let request = request(&rig.lookup()).clone();
    rig.engine.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: RESULT,
            },
        },
    );
    let actions = rig.engine.drain_actions().collect::<Vec<_>>();
    let results = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 71,
                result,
            }) => Some(serde_json::from_str::<serde_json::Value>(result).unwrap()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        results,
        [
            serde_json::json!({"buyLeverage":12.,"sellLeverage":12.,"marginType":"cross","positionMode":"oneway"})
        ]
    );
    assert_eq!(rig.engine.store().setting("BTCUSDT").unwrap().leverage, 12.);
    assert!(rig.engine.store().positions().is_empty());
}
