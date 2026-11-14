use safe_cex_engine::signing::{SignFuture, SignRequest, Signer};
use safe_cex_engine::{Engine, ErrorKind, Outcome, Venue};
use safe_cex_venues::{
    ConstructionContext, EngineOptions, Visit, build_in_context, build_with_signer_in_context,
    supported,
};
use std::sync::Arc;

struct PublicMode;
impl Visit for PublicMode {
    type Output = bool;
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> bool {
        engine.account_startup()
    }
}

struct NoIo;
impl Signer for NoIo {
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("admission must not invoke a signer");
    }
    fn sign_ready(
        &self,
        _: &SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        panic!("admission must not invoke an inline signer");
    }
}

fn sdk_roster() -> Vec<&'static str> {
    include_str!("../../../packages/safe-cex/src/exchanges/rust/betaVenues.ts")
        .split_once("Object.freeze([")
        .expect("SDK beta roster")
        .1
        .split_once("] as const")
        .expect("literal SDK roster")
        .0
        .lines()
        .filter_map(|line| line.trim().strip_prefix('\'')?.strip_suffix("',"))
        .collect()
}

#[test]
fn every_compiled_name_uses_the_full_sdk_hosted_roster() {
    let beta = sdk_roster();
    assert!(!beta.is_empty());
    let mut checked = 0;
    let mut mismatches = Vec::new();
    for line in include_str!("../../hosts/wasm/js/venues.mjs").lines() {
        let Some((name, family)) = line.trim().split_once(": ") else {
            continue;
        };
        if !family.starts_with('\'') {
            continue;
        }
        let name = name.trim_matches('\'');
        if !supported(name) {
            continue;
        }
        checked += 1;
        assert!(ConstructionContext::Client.validate_venue(name).is_ok());
        if ConstructionContext::Hosted.validate_venue(name).is_ok() != beta.contains(&name) {
            mismatches.push(name);
        }
    }
    assert!(checked > 0, "actual compiled names were checked");
    assert!(
        mismatches.is_empty(),
        "Hosted/SDK roster mismatches: {mismatches:?}"
    );
}

#[test]
fn out_names_refuse_both_hosted_constructors_but_keep_client_codecs() {
    let mut mismatches = Vec::new();
    // restored by #3502; #3317 dropped a live venue without a product decision.
    for venue in ["orderly", "polymarket", "txflow", "weex"] {
        if !supported(venue) {
            continue;
        }
        for injected in [false, true] {
            for public in [true, false] {
                let options: EngineOptions = serde_json::from_value(serde_json::json!({
                    "publicDataOnly": public,
                    "key": "unused-key",
                    "secret": "unused-fixture"
                }))
                .expect("synthetic options");
                let result = if injected {
                    build_with_signer_in_context(
                        ConstructionContext::Hosted,
                        venue,
                        options,
                        None,
                        Some(Arc::new(NoIo)),
                        PublicMode,
                    )
                } else {
                    build_in_context(
                        ConstructionContext::Hosted,
                        venue,
                        options,
                        None,
                        PublicMode,
                    )
                };
                let error = result.expect_err("OUT name must refuse before setup or entropy");
                if error.kind != Some(ErrorKind::Unsupported)
                    || error.outcome != Some(Outcome::NotSent)
                {
                    mismatches.push(format!(
                        "{venue}: public={public}, injected={injected}: {error:?}"
                    ));
                }
            }
        }
        let options = serde_json::from_value(serde_json::json!({
            "publicDataOnly": true,
            "accountStartup": true,
            "publicMode": {"requested": true},
            "testnet": venue == "txflow"
        }))
        .expect("options");
        let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
        assert!(
            !build_in_context(
                ConstructionContext::Client,
                venue,
                options,
                entropy,
                PublicMode
            )
            .expect("Client codec remains constructible"),
            "{venue}: public codec cannot start private reads"
        );
    }
    // restored by #3502; #3317 dropped a live venue without a product decision.
    for venue in ["bitrue", "bybit", "kodiakfi", "mode", "what", "woofi"] {
        if !supported(venue) {
            continue;
        }
        let options = serde_json::from_str(
            r#"{"publicDataOnly":true,"accountStartup":true,"publicMode":{"requested":true}}"#,
        )
        .expect("options");
        let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
        assert!(
            !build_in_context(
                ConstructionContext::Hosted,
                venue,
                options,
                entropy,
                PublicMode
            )
            .expect("SDK beta positive control remains admitted")
        );
    }
    assert!(
        mismatches.is_empty(),
        "Hosted constructors must refuse before setup: {mismatches:?}"
    );
}
