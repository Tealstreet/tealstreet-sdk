#![cfg(feature = "bybit")]

use safe_cex_engine::{Action, Engine, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer, local_signer_from_json};

struct Start;
impl Visit for Start {
    type Output = Vec<Action>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        engine.start(1_700_000_000_000.0);
        engine.drain_actions().collect()
    }
}

fn options(venue: &str) -> EngineOptions {
    let mut value = serde_json::json!({});
    if venue == "hyrotrader" {
        value["endpoints"] = serde_json::json!({
            "rest": "https://hyro.invalid",
            "publicWs": "wss://hyro.invalid/public",
            "privateWs": "wss://hyro.invalid/private"
        });
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn all_bybit_aliases_accept_setup_and_ordinary_startup_gets_use_secondary() {
    for venue in [
        "bybit",
        "bybitinverse",
        "bybitspot",
        "bybitdemo",
        "hyrotrader",
    ] {
        let mut input = options(venue);
        input.key = Some("primary-key".into());
        input.secret = Some("primary-secret".into());
        input.read_credentials = Some(safe_cex_engine::ReadCredentials {
            key: "secondary-key".into(),
            secret: "secondary-secret".into(),
            passphrase: None,
        });
        let actions = build(
            venue,
            input,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Start,
        )
        .unwrap();
        let requests: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        let signed: Vec<_> = requests
            .iter()
            .filter(|r| r.headers.iter().any(|(k, _)| *k == "X-BAPI-SIGN"))
            .collect();
        if venue == "hyrotrader" {
            // Hyro waits for its catalogue before private startup; provider setup
            // must construct without pretending it has issued ordinary V5 reads.
            assert!(signed.is_empty());
            assert!(!requests.is_empty());
            continue;
        }
        assert!(
            !signed.is_empty(),
            "{venue}: private startup releases signed GETs"
        );
        for request in signed {
            let header = |name: &str| {
                request
                    .headers
                    .iter()
                    .find(|(k, _)| *k == name)
                    .unwrap()
                    .1
                    .as_str()
            };
            assert_eq!(header("X-BAPI-API-KEY"), "secondary-key", "{venue}");
            assert_eq!(header("X-BAPI-TIMESTAMP"), "1700000000000", "{venue}");
            let query = request.url.split_once('?').map_or("", |(_, q)| q);
            let expected = safe_cex_crypto::hmac_sha256(
                b"secondary-secret",
                format!("1700000000000secondary-key20000{query}").as_bytes(),
                safe_cex_crypto::Encoding::Hex,
            );
            assert_eq!(header("X-BAPI-SIGN"), expected, "{venue}");
        }
    }
}

#[test]
fn injected_refs_work_without_flat_credentials_and_missing_provider_is_refused() {
    let (provider, identities) = local_signer_from_json(
        r#"{"primary":{"key":"injected-key","secret":"injected-secret"}}"#.into(),
    )
    .unwrap();
    let mut input = options("bybit");
    input.signing = Some(identities.clone());
    let failure = build_with_signer(
        "bybit",
        input,
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        None,
        Start,
    )
    .unwrap_err();
    assert!(failure.message.contains("provider"));
    let mut input = options("bybit");
    input.signing = Some(identities);
    let actions = build_with_signer(
        "bybit",
        input,
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        Some(provider),
        Start,
    )
    .unwrap();
    assert!(actions.iter().any(|a| matches!(a, Action::Http(r) if r.headers.iter().any(|(k,v)| *k == "X-BAPI-API-KEY" && v == "injected-key"))));
}

struct Registered;
impl Visit for Registered {
    type Output = safe_cex_engine::signing::Identities;
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> Self::Output {
        engine.identities().clone()
    }
}

#[test]
fn trusted_construction_registers_the_venue_identities_for_capability_admission() {
    let mut input = options("bybit");
    input.key = Some("primary-key".into());
    input.secret = Some("primary-secret".into());
    input.read_credentials = Some(safe_cex_engine::ReadCredentials {
        key: "secondary-key".into(),
        secret: "secondary-secret".into(),
        passphrase: None,
    });
    let registered = build(
        "bybit",
        input,
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        Registered,
    )
    .unwrap();
    let primary = registered.primary.expect("primary refs registered");
    let secondary = registered.secondary.expect("secondary refs registered");
    assert_ne!(primary.signing, secondary.signing);
    // Refs only: no key material is registered with the engine.
    let serialized = serde_json::to_string(&(primary, secondary)).unwrap();
    assert!(!serialized.contains("primary-secret") && !serialized.contains("secondary-secret"));
}
