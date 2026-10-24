//! Native caller evidence before managed recipe and FetchLeverage adoption.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared, RoutingIntent};
use safe_cex_engine::signing::{SignError, SignFuture, SignRequest, Signer};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, ReadCredentials, Response,
};
use safe_cex_venue_blofin::{Blofin, Options};
use std::collections::{HashMap, VecDeque};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

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
    fail_entropy: Arc<AtomicBool>,
}

impl Rig {
    fn new(mut options: Options, parked: bool, filtered: bool) -> Self {
        let setup = serde_json::json!({
            "primary": {
                "key":options.key,
                "secret":options.secret,
                "passphrase":options.passphrase,
            },
            "secondary":options.read_credentials.as_ref().map(|read|serde_json::json!({
                "key":read.key,
                "secret":read.secret,
                "passphrase":read.passphrase.as_ref().or(options.passphrase.as_ref()),
            })),
        });
        // Trusted setup hands the venue the same opaque refs it registers.
        let local = LocalSigner::from_json(setup.to_string()).ok();
        if let Some((_, identities)) = &local {
            options.identities = identities.clone();
        }
        let mut engine = safe_cex_engine::test_support::seeded_engine(Blofin::new(options));
        if let Some((provider, identities)) = local {
            engine.set_signer(Some(Arc::new(provider))).unwrap();
            engine.set_identities(identities).unwrap();
        }
        if parked {
            engine.set_signer(Some(Arc::new(Parked))).unwrap();
        }
        let draws = Draws::default();
        let recorded = draws.clone();
        let fail_entropy = Arc::new(AtomicBool::new(false));
        let fail = fail_entropy.clone();
        engine
            .set_signing_entropy(Box::new(move |bytes| {
                recorded.lock().unwrap().push(bytes.len());
                if fail.load(Ordering::SeqCst) {
                    return Err("fixture signing entropy unavailable".into());
                }
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
            fail_entropy,
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
        for action in release_actions(&mut self.engine, now) {
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

    fn issue_lookup(&mut self) {
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
    }

    fn lookup(&mut self) -> Vec<Action> {
        self.issue_lookup();
        self.engine.drain_actions().collect()
    }

    fn released_lookup(&mut self, now: f64) -> Vec<Action> {
        self.issue_lookup();
        release_actions(&mut self.engine, now)
    }
}

fn release_actions(engine: &mut Engine<Blofin>, now: f64) -> Vec<Action> {
    engine.prepare_dispatch(now);
    let mut queue = VecDeque::from(engine.drain_actions().collect::<Vec<_>>());
    let mut routes = HashMap::new();
    let mut actions = vec![];
    let mut visited = 0;
    while let Some(action) = queue.pop_front() {
        visited += 1;
        assert!(visited <= 4096, "bounded immediate fixture drain");
        let mut released = None;
        if let Action::ManagedHttp(http) = &action {
            match http.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(engine.http_preparation_pending(*preparation));
                    assert_eq!(request.id, preparation.attempt().request);
                    assert!(routes.insert(request.id, *preparation).is_none());
                    engine.http_prepared(
                        now,
                        *preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(routes.remove(&request.id), Some(*preparation));
                    assert!(matches!(
                        engine.claim_http(now, *permit),
                        Ok(Claim::Released(_))
                    ));
                    released = Some(request.clone());
                }
                HttpAction::Cancel { preparation, .. } => {
                    let request = preparation.attempt().request;
                    if routes.get(&request) == Some(preparation) {
                        routes.remove(&request);
                    }
                }
            }
        }
        actions.push(action);
        if let Some(request) = released {
            actions.push(Action::Http(request));
        }
        for next in engine.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(next);
        }
    }
    assert!(
        routes.is_empty(),
        "immediate route driver cannot hide parked work"
    );
    actions
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
    let actions = rig.released_lookup(NOW + 800.);
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
    let actions = rig.released_lookup(NOW);
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
        request(&rig.released_lookup(NOW)).headers,
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
        request(&rig.released_lookup(NOW)).headers,
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
    let actions = rig.released_lookup(NOW);
    assert_eq!(*rig.draws.lock().unwrap(), [21]);
    let request = request(&actions);
    assert_eq!(request.headers[2], ("ACCESS-NONCE", "n".into()));
    assert_eq!(request.headers[1], ("ACCESS-SIGN", "ZTYxMjQ1MTI0MGU5YzgzYjNhOGY2OTZmYzAyM2YwY2Q5YTljNDRjNjEyMDE1NjY2MWEwM2M1ZTMxNWJkMmFhZg==".into()));
}

#[test]
fn entropy_refusal_at_preparation_is_typed_notsent_without_wire() {
    let mut rig = Rig::ready(options(true), false, false);
    rig.fail_entropy.store(true, Ordering::SeqCst);
    let actions = rig.released_lookup(NOW);
    assert_eq!(
        *rig.draws.lock().unwrap(),
        [21],
        "the configured entropy refusal fired"
    );
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Http(_) | Action::Sign { .. } | Action::WsSend { .. }
    )));
    let errors = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 71, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(*errors[0], SignError::Unavailable.venue_error());
}

#[test]
fn native_leverage_response_keeps_financial_result_and_no_positions() {
    let mut rig = Rig::ready(options(true), false, false);
    let request = request(&rig.released_lookup(NOW)).clone();
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

#[test]
fn native_post_pins_primary_hmac_over_the_exact_caller_body() {
    let mut rig = Rig::ready(options(true), false, false);
    rig.engine
        .handle_command(
            NOW,
            Command::SetLeverage {
                command: 72,
                symbol: "BTCUSDT".into(),
                leverage: 12.,
                side: None,
            },
            Some(20_000),
        )
        .unwrap();
    let actions = release_actions(&mut rig.engine, NOW);
    let requests = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/api/v1/account/set-leverage") => {
                Some(request)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, Method::Post);
    assert_eq!(
        requests[0].body.as_deref(),
        Some(r#"{"instId":"BTC-USDT","leverage":"12","marginMode":"cross"}"#)
    );
    assert_eq!(
        requests[0].headers,
        headers(
            "fixture-primary-key",
            "fixture-primary-pass",
            "MzRjMzZmZWFmOWMyZjFhYTAzZmQ5NmY1Y2ZiZjFiOGNmMGUyZWRhODExMjc5ZDcwM2JmODliZTg4NjI1YmY4YQ=="
        )
    );
}

#[test]
fn private_start_still_requires_primary_passphrase_with_valid_secondary() {
    let mut opts = options(true);
    opts.passphrase = None;
    let mut rig = Rig::new(opts, false, false);
    rig.reply("/market/instruments", MARKET);
    assert!(
        rig.requests
            .iter()
            .all(|request| !request.url.contains("/account/"))
    );
    assert!(rig.draws.lock().unwrap().is_empty());
    assert!(rig.actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::Event(event)) if event.message=="BloFin requires a passphrase")));
}

#[test]
fn missing_primary_preserves_the_reachable_local_leverage_refusal() {
    let mut opts = options(true);
    opts.key = None;
    opts.secret = None;
    let mut rig = Rig::new(opts, false, false);
    rig.reply("/market/instruments", MARKET);
    let actions = rig.released_lookup(NOW);
    assert_eq!(actions.iter().filter(|action|matches!(action,
        Action::Notice(Notice::CommandFailed { command:71,error }) if error.message=="BloFin margin mode is not loaded")).count(),1);
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
    )));
    assert!(rig.draws.lock().unwrap().is_empty());
}
