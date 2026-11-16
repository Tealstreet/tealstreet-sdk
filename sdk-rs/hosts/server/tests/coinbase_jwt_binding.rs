use safe_cex_exchange_server::{accounts::engine_options, envelope::InitPayload};
use serde_json::json;

fn key_name() -> String {
    ["organizations", "fixture", "apiKeys", "join"].join("/")
}

fn preserves_algorithm(venue: &str) {
    let init: InitPayload = serde_json::from_value(json!({
        "accountName":"fixture", "exchangeName":venue, "signingMode":"cosigned",
        "key":key_name(), "coinbaseJwtAlgorithm":"edDsa"
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
        let raw = json!({"accountName":"fixture","exchangeName":venue,"key":key_name()});
        let implicit: InitPayload = serde_json::from_value(raw.clone()).unwrap();
        let mut null_algorithm = raw.clone();
        null_algorithm["coinbaseJwtAlgorithm"] = serde_json::Value::Null;
        let null_algorithm: InitPayload = serde_json::from_value(null_algorithm).unwrap();
        assert_eq!(implicit.fingerprint(), null_algorithm.fingerprint());
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

#[test]
fn coinbase_join_algorithm_requires_a_string_or_null() {
    for venue in ["coinbase", "coinbasespot"] {
        for algorithm in [
            json!({"es256":null}),
            json!({"edDsa":null}),
            json!(true),
            json!(1),
            json!([]),
        ] {
            assert!(
                serde_json::from_value::<InitPayload>(json!({"accountName":"fixture",
                "exchangeName":venue,"key":key_name(),"coinbaseJwtAlgorithm":algorithm}))
                .is_err()
            );
        }
    }
}
