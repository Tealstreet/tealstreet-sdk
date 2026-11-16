use safe_cex_exchange_server::{accounts::engine_options, envelope::InitPayload};
use serde_json::json;

fn preserves_algorithm(venue: &str) {
    let init: InitPayload = serde_json::from_value(json!({
        "accountName":"fixture", "exchangeName":venue, "signingMode":"cosigned",
        "key":"organizations/fixture/apiKeys/join", "coinbaseJwtAlgorithm":"edDsa"
    }))
    .unwrap();
    let options = engine_options(&init, false);
    assert_eq!(
        options["coinbaseJwtAlgorithm"], "edDsa",
        "{venue}: joining key's public algorithm was discarded"
    );
}

#[test]
fn coinbase_join_preserves_the_explicit_public_jwt_algorithm() {
    preserves_algorithm("coinbase");
}

#[test]
fn coinbasespot_join_preserves_the_explicit_public_jwt_algorithm() {
    preserves_algorithm("coinbasespot");
}

#[test]
fn coinbase_effective_algorithm_is_fingerprinted_and_unknown_algorithms_refuse() {
    for venue in ["coinbase", "coinbasespot"] {
        let raw = json!({"accountName":"fixture","exchangeName":venue,"key":"organizations/fixture/apiKeys/join"});
        let implicit: InitPayload = serde_json::from_value(raw.clone()).unwrap();
        let mut explicit = raw.clone();
        explicit["coinbaseJwtAlgorithm"] = "es256".into();
        let explicit: InitPayload = serde_json::from_value(explicit).unwrap();
        assert_eq!(implicit.fingerprint(), explicit.fingerprint());
        let mut eddsa = raw.clone();
        eddsa["coinbaseJwtAlgorithm"] = "edDsa".into();
        let eddsa: InitPayload = serde_json::from_value(eddsa).unwrap();
        assert_ne!(implicit.fingerprint(), eddsa.fingerprint());
        let mut unknown = raw;
        unknown["coinbaseJwtAlgorithm"] = "hs256".into();
        assert!(serde_json::from_value::<InitPayload>(unknown).is_err());
    }
}
