//! Native rare identity reads: route/signer, exact IDs, failure ownership and no state commits.
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_backpack::{Backpack, Options};
const NOW: f64 = 1000.0;
fn engine(read: bool, public: bool) -> Engine<Backpack> {
    engine_window(read, public, None)
}
fn engine_window(read: bool, public: bool, recv_window: Option<f64>) -> Engine<Backpack> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    let (provider, identities) = if public {
        (None, safe_cex_engine::signing::Identities::default())
    } else {
        let local = |key: &str, seed| LocalIdentity {
            key: Secret::new(key.into()),
            secret: Secret::new(STANDARD.encode([seed; 32])),
            secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
            ..Default::default()
        };
        let (provider, identities) = LocalSigner::from_setup_with_policy(
            LocalSetup {
                primary: Some(local("primary-key", 7u8)),
                secondary: read.then(|| local("read-key", 8u8)),
            },
            ed25519_seeds(),
        )
        .unwrap();
        (
            Some(std::sync::Arc::new(provider) as safe_cex_engine::signing::Provider),
            identities,
        )
    };
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        identity: identities.primary.clone(),
        read_identity: identities.secondary.clone(),
        key: Some("primary-key".into()),
        secret: Some(STANDARD.encode([7u8; 32])),
        read_credentials: read.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: STANDARD.encode([8u8; 32]),
            passphrase: None,
        }),
        public_data_only: public,
        recv_window,
        ..Default::default()
    }));
    e.set_signer(provider).unwrap();
    e.set_identities(identities).unwrap();
    e.start(NOW);
    for _ in 0..12 {
        let reads: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.ends_with("/markets") {
                r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#
            } else if r.url.contains("tickers") {
                r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.01","volume":"10","quoteVolume":"1410"}]"#
            } else if r.url.ends_with("/account") {
                r#"{"leverageLimit":"10"}"#
            } else if r.url.ends_with("/collateral") {
                r#"{"netEquity":"100","netEquityAvailable":"90","netEquityLocked":"10","pnlUnrealized":"0"}"#
            } else {
                "[]"
            };
            e.handle(
                NOW,
                Event::HttpResponse {
                    request: r.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
        }
    }

    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 123.0,
        free: 100.0,
        used: 23.0,
        upnl: 7.0,
    });
    e.drain_actions().for_each(drop);
    e
}
fn ask(e: &mut Engine<Backpack>) -> Vec<Action> {
    e.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
    safe_cex_engine::test_support::prepared_actions(e, NOW)
}
fn request(e: &mut Engine<Backpack>) -> HttpRequest {
    let actions = ask(e);
    let mut reads = actions.into_iter().filter_map(|a| match a {
        Action::Http(r) => Some(r),
        _ => None,
    });
    let r = reads.next().expect("identity HTTP request");
    assert!(reads.next().is_none(), "identity requires one rare read");
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.body, None);
    assert_eq!(r.route, None, "use the selected ordinary read egress");
    assert_eq!(r.url, "https://api.backpack.exchange/api/v1/account");
    r
}
fn result(actions: Vec<Action>, rejected: bool) -> Result<String, String> {
    let mut done = Vec::new();
    let mut errors = Vec::new();
    for action in actions {
        match action {
            Action::Notice(Notice::CommandDone { command, result }) => {
                let result: Result<String, String> = Ok(result);
                assert_eq!(command, 42);
                done.push(result);
            }
            Action::Notice(Notice::CommandFailed { command, error }) => {
                let result: Result<String, String> = Err(error.legacy_text());
                assert_eq!(command, 42);
                done.push(result);
            }
            Action::Notice(Notice::CommandError { command, message }) => {
                assert_eq!(command, 42);
                assert!(done.is_empty(), "diagnostic before Done");
                errors.push(message);
            }
            Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
                level: safe_cex_engine::Level::Error,
                ..
            })) => panic!("identity error must be correlated"),
            Action::Http(_) => panic!("identity must not trigger account reconciliation"),
            _ => {}
        }
    }
    assert_eq!(done.len(), 1);
    assert_eq!(errors.len(), usize::from(rejected));
    let result = done.remove(0);
    assert_eq!(result.is_err(), rejected);
    result
}
fn respond(e: &mut Engine<Backpack>, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    e.handle(
        NOW,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
#[test]
fn identity_returns_exact_response_ids_without_mutating_live_account() {
    let mut e = engine(false, false);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&mut e);
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "X-API-Key" && value == "primary-key")
    );
    let out = respond(
        &mut e,
        &r,
        200,
        r#"{"accountId":9007199254740993,"userId":"other"}"#,
    );
    assert_eq!(
        result(out, false).unwrap(),
        r#"{"userId":"9007199254740993","affiliateId":null}"#
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert!(
        respond(
            &mut e,
            &r,
            200,
            r#"{"accountId":9007199254740993,"userId":"other"}"#
        )
        .is_empty(),
        "duplicate response is consumed once"
    );
}
#[test]
fn identity_uses_secondary_read_signer_and_contract_fallback() {
    let mut e = engine(true, false);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&mut e);
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "X-API-Key" && value == "read-key")
    );
    let out = respond(
        &mut e,
        &r,
        200,
        r#"{"accountId":"","userId":"read-account"}"#,
    );
    assert_eq!(
        result(out, false).unwrap(),
        r#"{"userId":"read-account","affiliateId":null}"#
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
}
#[test]
fn identity_missing_false_empty_or_failed_read_rejects_once_and_preserves_state() {
    for body in [
        r#"{}"#,
        r#"{"accountId":false}"#,
        r#"{"accountId":"","userId":""}"#,
        r#"{"accountId":1.5}"#,
    ] {
        let mut e = engine(false, false);
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = request(&mut e);
        result(respond(&mut e, &r, 200, body), true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
    for network in [false, true] {
        let mut e = engine(false, false);
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = request(&mut e);
        let out = if network {
            e.handle(
                NOW,
                Event::HttpFailed {
                    request: r.id,
                    error: "connection lost",
                },
            );
            e.drain_actions().collect()
        } else {
            respond(&mut e, &r, 503, "unavailable")
        };
        result(out, true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_public_only_fails_before_network_even_with_saved_credentials() {
    let mut e = engine(true, true);
    let before = e.account_snapshot_frame(NOW).to_vec();
    result(ask(&mut e), true).unwrap_err();
    assert_eq!(e.account_snapshot_frame(NOW), before);
}

#[test]
fn identity_without_credentials_fails_before_network() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options::default()));
    result(ask(&mut e), true).unwrap_err();
}

#[test]
fn identity_rejects_incomplete_saved_read_credentials_without_primary_fallback() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: String::new(),
            passphrase: None,
        }),
        ..Default::default()
    }));
    let error = result(ask(&mut e), true).unwrap_err();
    assert!(error.contains("Secondary read credentials"));
}

#[test]
fn identity_secondary_read_inherits_the_primary_native_window() {
    let mut e = engine_window(true, false, Some(12_345.));
    let r = request(&mut e);
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "X-Window" && value == "12345")
    );
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "X-API-Key" && value == "read-key")
    );
}

/// Backpack seeds are Ed25519 keys, declared as the provider builder does.
fn ed25519_seeds() -> safe_cex_engine::signing::CredentialSetupPolicy {
    let ed25519 = Some(safe_cex_engine::signing::IdentitySetupPolicy {
        signing_algorithm: Some(safe_cex_engine::signing::Algorithm::Ed25519),
        ..Default::default()
    });
    safe_cex_engine::signing::CredentialSetupPolicy {
        primary: ed25519,
        secondary: ed25519,
    }
}
