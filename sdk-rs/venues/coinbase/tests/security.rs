use base64::{Engine as _, engine::general_purpose::STANDARD};
use p256::pkcs8::EncodePrivateKey;
use safe_cex_crypto::secrets::EraseSecrets;
use safe_cex_engine::{Action, Event};
use safe_cex_venue_coinbase::{Coinbase, Options};

const KEY: &str = "organizations/fixture/apiKeys/normalization";

fn startup_authorization(key: &str, secret: &str, spot_only: bool) -> String {
    let venue = Coinbase::new(Options {
        key: key.into(),
        secret: secret.into(),
        spot_only,
        ..Default::default()
    })
    .unwrap();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.start(1_700_000_000_000.0);
    let mut timers = vec![];
    for _ in 0..20 {
        for action in engine.drain_actions() {
            match action {
                Action::Http(http) if http.url.ends_with("/portfolios") => {
                    return http
                        .headers
                        .into_iter()
                        .find(|(name, _)| *name == "Authorization")
                        .unwrap()
                        .1;
                }
                Action::SetTimer { timer, at_ms } => timers.push((at_ms, timer)),
                Action::ClearTimer { timer } => timers.retain(|(_, id)| *id != timer),
                _ => {}
            }
        }
        timers.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (at, timer) = timers.remove(0);
        engine.handle(at, Event::TimerFired { timer });
    }
    panic!("private startup never issued its portfolio request");
}

#[test]
fn pasted_bom_in_key_and_ed25519_secret_preserves_the_real_startup_jwt() {
    let secret = STANDARD.encode([7; 32]);
    for spot in [false, true] {
        let expected = startup_authorization(KEY, &secret, spot);
        for (key, secret) in [
            (format!("\u{feff} \t{KEY}\n\u{feff}"), secret.clone()),
            (KEY.into(), format!("\u{feff} {secret}\u{feff}\n")),
            (
                KEY.into(),
                format!("ed25519:{}\u{feff}{}", &secret[..16], &secret[16..]),
            ),
        ] {
            assert_eq!(startup_authorization(&key, &secret, spot), expected);
        }
    }
}

#[test]
fn pasted_bom_in_downloaded_json_preserves_the_real_startup_jwt() {
    let secret = STANDARD.encode([7; 32]);
    let json = serde_json::json!({
        "name": format!("\u{feff}{KEY}\u{feff}"),
        "privateKey": format!("\u{feff}{secret}\u{feff}"),
    })
    .to_string();
    for spot in [false, true] {
        let expected = startup_authorization(KEY, &secret, spot);
        assert_eq!(
            startup_authorization(&format!("\u{feff} {json}\n\u{feff}"), "", spot),
            expected
        );
        assert_eq!(
            startup_authorization(KEY, &format!("\u{feff}{json}\u{feff}"), spot),
            expected
        );
    }
}

#[test]
fn pasted_bom_around_pem_preserves_the_real_es256_startup_jwt() {
    let key = p256::SecretKey::from_slice(&[7; 32]).unwrap();
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        STANDARD.encode(key.to_pkcs8_der().unwrap().as_bytes())
    );
    for spot in [false, true] {
        let expected = startup_authorization(KEY, &pem, spot);
        assert_eq!(
            startup_authorization(KEY, &format!("\u{feff}{pem}\u{feff}"), spot),
            expected
        );
    }
}

#[test]
fn credential_options_debug_redacts_and_erasure_scrubs_owned_fields() {
    let mut options = Options {
        key: "audit-secret-sentinel".into(),
        secret: "audit-secret-sentinel".into(),
        ..Options::default()
    };
    assert!(!format!("{options:?}").contains("audit-secret-sentinel"));
    options.erase_secrets();
    assert!(options.key.is_empty());
    assert!(options.secret.is_empty());
}
