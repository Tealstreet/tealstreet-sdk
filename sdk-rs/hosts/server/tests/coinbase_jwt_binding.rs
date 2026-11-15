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
