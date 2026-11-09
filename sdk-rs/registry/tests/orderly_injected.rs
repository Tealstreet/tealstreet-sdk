#![cfg(feature = "orderly")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, Identity, KeyRef, Provider, SignError, SignFuture,
    SignRequest, SignResult, Signer,
};
use safe_cex_engine::{Action, Engine, ErrorKind, Event, Outcome, Response, Venue};
use safe_cex_venues::{EngineOptions, Visit, build_with_signer};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const NOW: f64 = 1_700_000_000_123.;
const ALIASES: [&str; 4] = ["kodiakfi", "mode", "what", "woofi"];
struct Probe {
    local: LocalSigner,
    seen: Arc<Mutex<Vec<SignRequest>>>,
}
impl Signer for Probe {
    fn immediate(&self) -> bool {
        true
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.local.can_sign(key, algorithm)
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        let result = (|| {
            self.seen
                .lock()
                .map_err(|_| SignError::Unavailable)?
                .push(request.clone());
            self.local.sign_now(request)
        })();
        Some(result)
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        let result = self
            .sign_ready(&request)
            .ok_or(SignError::Unavailable)
            .and_then(|result| result);
        Box::pin(async move { result })
    }
}
struct Setup {
    provider: Provider,
    ids: Identities,
    key: String,
    seen: Arc<Mutex<Vec<SignRequest>>>,
}
fn setup() -> TestResult<Setup> {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../packages/safe-cex/src/__tests__/fixtures/orderly-device-signing.json"
    ))?;
    let key = fixture["publicKey"]
        .as_str()
        .ok_or("missing public fixture")?
        .to_owned();
    let mut local = LocalSigner::new();
    local.insert(KeyRef("owned-public".into()), key.as_bytes().to_vec());
    local.insert_declared(
        KeyRef("owned-signing".into()),
        vec![7; 32],
        Algorithm::Ed25519,
    )?;
    let ids = Identities {
        primary: Some(Identity::new(
            KeyRef("owned-public".into()),
            Some(KeyRef("owned-signing".into())),
        )),
        secondary: None,
    };
    let seen = Arc::new(Mutex::new(Vec::new()));
    Ok(Setup {
        provider: Arc::new(Probe {
            local,
            seen: seen.clone(),
        }),
        ids,
        key,
        seen,
    })
}
fn options(setup: &Setup) -> TestResult<EngineOptions> {
    let mut options: EngineOptions = serde_json::from_value(
        json!({"key":setup.key,"applicationId":"account-1","publicDataOnly":false}),
    )?;
    options.signing = Some(setup.ids.clone());
    Ok(options)
}
fn body(url: &str) -> &'static str {
    if url.ends_with("/v1/client/info") {
        r#"{"success":true,"data":{"max_leverage":20}}"#
    } else if url.ends_with("/v1/public/info") {
        r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":20,"base_imr":0.01}]}}"#
    } else if url.ends_with("/v1/public/futures") {
        r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","24h_open":50000,"24h_close":51000,"mark_price":51000}]}}"#
    } else if url.ends_with("/v1/positions") {
        r#"{"success":true,"data":{"rows":[],"total_collateral_value":10000,"free_collateral":7000}}"#
    } else {
        r#"{"success":true,"data":{"rows":[],"meta":{"total":0}}}"#
    }
}
fn assert_closed<T>(result: Result<T, safe_cex_engine::VenueError>) -> TestResult {
    let error = match result {
        Err(error) => error,
        Ok(_) => return Err("closed private admission constructed an engine".into()),
    };
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(
        error.message,
        "Orderly injected private signing admission is closed"
    );
    Ok(())
}

#[test]
fn orderly_private_injection_refuses_before_engine_or_signing() -> TestResult {
    for venue in ALIASES {
        for (captured, legacy_public) in [(false, false), (true, false), (true, true)] {
            for (remote, payload) in [(false, false), (true, false), (false, true), (true, true)] {
                for (refs, provider) in [(true, false), (false, true), (true, true)] {
                    let setup = setup()?;
                    let mut options = if captured {
                        captured_private_options(&setup)?
                    } else {
                        options(&setup)?
                    };
                    options.public_data_only = legacy_public;
                    options.remote_signing = remote;
                    options.payload_signer = payload;
                    options.signing = refs.then(|| setup.ids.clone());
                    assert_closed(build_with_signer(
                        venue,
                        options,
                        Some(safe_cex_engine::test_support::seeded_entropy(17)),
                        provider.then(|| setup.provider.clone()),
                        Accept,
                    ))?;
                    assert!(setup.seen.lock().map_err(|_| "poisoned record")?.is_empty());
                }
            }
        }
    }
    Ok(())
}

struct Public;
impl Visit for Public {
    type Output = TestResult;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> TestResult {
        assert!(engine.identities() == &Identities::default());
        assert!(engine.signer().is_none());
        engine.start(NOW);
        assert!(
            safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
                .iter()
                .all(|action| !matches!(
                    action,
                    Action::WsConnect {
                        channel: Some(safe_cex_engine::SocketChannel::Private),
                        ..
                    }
                ))
        );
        Ok(())
    }
}
#[test]
fn orderly_public_installs_no_refs_and_has_no_device_link() -> TestResult {
    for venue in ALIASES {
        let setup = setup()?;
        let mut options = options(&setup)?;
        options.public_data_only = true;
        options.key = Some("unused malformed key".into());
        options.secret = Some("unused malformed secret".into());
        build_with_signer(
            venue,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Some(setup.provider),
            Public,
        )??;
        assert!(setup.seen.lock().map_err(|_| "poisoned record")?.is_empty());
        assert!(!safe_cex_venues::remote_signing_policy(venue).enabled);
    }
    Ok(())
}

struct Accept;
impl Visit for Accept {
    type Output = ();
    fn visit<V: Venue + 'static>(self, _: Engine<V>) {
        panic!("closed private admission visited an engine");
    }
}
#[test]
fn orderly_private_injection_stays_closed_even_for_invalid_metadata() -> TestResult {
    for venue in ALIASES {
        for failure in [
            "key",
            "account",
            "provider",
            "identity",
            "credential-ref",
            "signing-ref",
            "algorithm",
            "secondary",
            "secret",
            "remote",
            "payload",
        ] {
            let setup = setup()?;
            let mut options = options(&setup)?;
            let mut provider = Some(setup.provider);
            match failure {
                "key" => options.key = Some("invalid".into()),
                "account" => options.application_id = None,
                "provider" => provider = None,
                "identity" => options.signing = None,
                "credential-ref" | "signing-ref" => {
                    if let Some(ids) = options.signing.as_mut() {
                        ids.primary = Some(Identity::new(
                            KeyRef(
                                if failure == "credential-ref" {
                                    ""
                                } else {
                                    "owned-public"
                                }
                                .into(),
                            ),
                            Some(KeyRef(
                                if failure == "signing-ref" {
                                    ""
                                } else {
                                    "owned-signing"
                                }
                                .into(),
                            )),
                        ));
                    }
                }
                "algorithm" => provider = Some(Arc::new(LocalSigner::new())),
                "secondary" => {
                    if let Some(ids) = options.signing.as_mut() {
                        ids.secondary = ids.primary.clone();
                    }
                }
                "secret" => options.secret = Some("synthetic forbidden material".into()),
                "remote" => options.remote_signing = true,
                "payload" => options.payload_signer = true,
                _ => return Err("unknown refusal case".into()),
            }
            assert_closed(build_with_signer(venue, options, None, provider, Accept))?;
            assert!(setup.seen.lock().map_err(|_| "poisoned record")?.is_empty());
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct StartupState {
    account: bool,
    pending_before: bool,
    complete_before: bool,
    pending_after: bool,
    complete_after: bool,
    signed_reads: usize,
}

struct Startup;
impl Visit for Startup {
    type Output = StartupState;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> StartupState {
        let account = engine.account_startup();
        engine.start(NOW);
        let pending_before = engine.private_startup_pending();
        let complete_before = engine.private_startup_complete();
        let mut signed_reads = 0;
        for _ in 0..12 {
            let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
            if actions.is_empty() {
                break;
            }
            for action in actions {
                if let Action::Http(http) = action {
                    if http
                        .headers
                        .iter()
                        .any(|(name, _)| *name == "orderly-signature")
                    {
                        signed_reads += 1;
                    }
                    engine.handle(
                        NOW,
                        Event::HttpResponse {
                            request: http.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body: body(&http.url),
                            },
                        },
                    );
                }
            }
        }
        StartupState {
            account,
            pending_before,
            complete_before,
            pending_after: engine.private_startup_pending(),
            complete_after: engine.private_startup_complete(),
            signed_reads,
        }
    }
}

fn captured_private_options(setup: &Setup) -> TestResult<EngineOptions> {
    let mut options: EngineOptions = serde_json::from_value(json!({
        "key": setup.key,
        "applicationId": "account-1",
        "publicDataOnly": false,
        "publicMode": {"requested": false},
    }))?;
    options.signing = Some(setup.ids.clone());
    Ok(options)
}

fn refuses_captured_private_payload_signer(venue: &str) -> TestResult {
    for legacy_public in [false, true] {
        let setup = setup()?;
        let mut options = captured_private_options(&setup)?;
        options.public_data_only = legacy_public;
        options.payload_signer = true;
        let result = build_with_signer(
            venue,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Some(setup.provider),
            Startup,
        );
        let error = match result {
            Err(error) => error,
            Ok(_) => return Err("closed PayloadSigner admission constructed an engine".into()),
        };
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert!(setup.seen.lock().map_err(|_| "poisoned record")?.is_empty());
    }
    Ok(())
}

fn private_injection_refuses_and_local_startup_completes(venue: &str) -> TestResult {
    let setup = setup()?;
    let local_options: EngineOptions = serde_json::from_value(json!({
        "key": setup.key,
        "secret": bs58::encode([7u8; 32]).into_string(),
        "applicationId": "account-1",
        "publicMode": {"requested": false},
    }))?;
    let local = safe_cex_venues::build(
        venue,
        local_options,
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        Startup,
    )?;
    assert!(local.account && local.pending_before && !local.complete_before);
    assert!(!local.pending_after && local.complete_after && local.signed_reads >= 4);
    for legacy_public in [false, true] {
        let mut options = captured_private_options(&setup)?;
        options.public_data_only = legacy_public;
        assert_closed(build_with_signer(
            venue,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Some(setup.provider.clone()),
            Accept,
        ))?;
        assert!(setup.seen.lock().map_err(|_| "poisoned record")?.is_empty());
    }
    Ok(())
}

#[test]
fn orderly_captured_public_ignores_private_flags_and_unused_signer_material() -> TestResult {
    for venue in ALIASES {
        let setup = setup()?;
        let mut options: EngineOptions = serde_json::from_value(json!({
            "publicDataOnly": false,
            "publicMode": {"requested": true},
            "payloadSigner": true,
            "remoteSigning": true,
            "key": "unused malformed key",
            "secret": "unused malformed secret",
        }))?;
        options.signing = Some(setup.ids);
        build_with_signer(
            venue,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Some(setup.provider),
            Public,
        )??;
        assert!(setup.seen.lock().map_err(|_| "poisoned record")?.is_empty());
    }
    Ok(())
}

#[test]
fn kodiakfi_captured_private_payload_signer_refuses() -> TestResult {
    refuses_captured_private_payload_signer("kodiakfi")
}
#[test]
fn mode_captured_private_payload_signer_refuses() -> TestResult {
    refuses_captured_private_payload_signer("mode")
}
#[test]
fn what_captured_private_payload_signer_refuses() -> TestResult {
    refuses_captured_private_payload_signer("what")
}
#[test]
fn woofi_captured_private_payload_signer_refuses() -> TestResult {
    refuses_captured_private_payload_signer("woofi")
}
#[test]
fn kodiakfi_private_injection_refuses_and_local_startup_completes() -> TestResult {
    private_injection_refuses_and_local_startup_completes("kodiakfi")
}
#[test]
fn mode_private_injection_refuses_and_local_startup_completes() -> TestResult {
    private_injection_refuses_and_local_startup_completes("mode")
}
#[test]
fn what_private_injection_refuses_and_local_startup_completes() -> TestResult {
    private_injection_refuses_and_local_startup_completes("what")
}
#[test]
fn woofi_private_injection_refuses_and_local_startup_completes() -> TestResult {
    private_injection_refuses_and_local_startup_completes("woofi")
}
