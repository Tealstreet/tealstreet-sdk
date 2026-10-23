#![cfg(all(
    feature = "dispatch-signing",
    feature = "orderly",
    feature = "coinbase"
))]
//! P8 browser half: the hosted executor signs only its own family's recipes,
//! through the recipe's allowlist and the account's role-bound key.

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, KeyRef, SignError, Signer};
use safe_cex_venues::hosted_signing::{self, Input};

// RFC 8032 test 1: a published seed/public-key pair, not account material.
const SEED: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
const PUBLIC: [u8; 32] = [
    0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07, 0x3a,
    0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07, 0x51, 0x1a,
];
const KID: &str = "organizations/org-id/apiKeys/key-id";

fn account(credential: &str) -> (LocalSigner, Identities) {
    let mut signer = LocalSigner::new();
    signer.insert(
        KeyRef("primary.credential".into()),
        credential.as_bytes().to_vec(),
    );
    signer.insert(KeyRef("primary.signing".into()), SEED.to_vec());
    let identities = Identities {
        primary: Some(Identity {
            credential: KeyRef("primary.credential".into()),
            signing: KeyRef("primary.signing".into()),
            passphrase: None,
            stable_id: None,
            extra: Default::default(),
        }),
        secondary: None,
    };
    (signer, identities)
}

fn op(recipe: &str, inputs: serde_json::Value) -> Input {
    serde_json::from_value(serde_json::json!({ "kind": "op", "recipe": recipe, "inputs": inputs }))
        .unwrap()
}

/// Runs the executor end to end and returns the raw signature bytes.
fn execute(venue: &str, credential: &str, input: Input) -> Result<Vec<u8>, SignError> {
    let (signer, identities) = account(credential);
    let (work, kind) = hosted_signing::prepare(venue, &identities, input)?;
    let result = signer
        .sign_ready(&work)
        .expect("local signer is immediate")?;
    work.validate_result(&result)?;
    let signature = hosted_signing::finish(kind, &result)?.signature.unwrap();
    Ok((0..signature.len() / 2)
        .map(|i| u8::from_str_radix(&signature[i * 2..i * 2 + 2], 16).unwrap())
        .collect())
}

fn orderly_inputs() -> serde_json::Value {
    serde_json::json!({ "timestamp": "1700000000000", "method": "GET", "path": "/v1/positions" })
}

#[test]
fn orderly_recipe_signs_the_canonical_bytes_the_server_verifies() {
    let key = format!("ed25519:{}", bs58::encode(PUBLIC).into_string());
    let verify = safe_cex_venue_orderly::remote_verifier(&key).unwrap();
    for venue in ["orderly", "woofi", "tealstreet-dex"] {
        let signature = execute(venue, &key, op("orderly.rest.1", orderly_inputs())).unwrap();
        assert!(
            verify(b"1700000000000GET/v1/positions", &signature),
            "{venue}"
        );
    }
}

#[test]
fn executor_refuses_other_families_skewed_versions_and_disallowed_inputs() {
    let key = "ed25519:any";
    assert_eq!(
        execute("coinbase", key, op("orderly.rest.1", orderly_inputs())),
        Err(SignError::Unsupported)
    );
    assert_eq!(
        execute("binance", key, op("orderly.rest.1", orderly_inputs())),
        Err(SignError::Unsupported)
    );
    assert_eq!(
        execute("orderly", key, op("orderly.rest.2", orderly_inputs())),
        Err(SignError::Unsupported)
    );
    let mut withdraw = orderly_inputs();
    withdraw["path"] = "/v1/withdraw_request".into();
    assert_eq!(
        execute("orderly", key, op("orderly.rest.1", withdraw)),
        Err(SignError::Refused)
    );
}

#[test]
fn coinbase_jwt_recipe_binds_kid_to_the_accounts_own_key_name() {
    let inputs = |kid: &str| {
        serde_json::json!({
            "kid": kid, "nonce": "n-1", "nbf": 1_700_000_000u64,
            "uri": "GET api.coinbase.com/api/v3/brokerage/key_permissions"
        })
    };
    let signature = execute(
        "coinbase",
        KID,
        op("coinbase.restJwtEd25519.1", inputs(KID)),
    )
    .unwrap();
    let recipe = safe_cex_venues::remote_recipe("coinbase.restJwtEd25519.1").unwrap();
    let message = recipe.canonicalize(&inputs(KID)).unwrap();
    assert!(safe_cex_crypto::ed25519::verify(&message, &PUBLIC, &signature).unwrap());
    // Another account's key name is refused even though the recipe accepts it.
    let other = "organizations/org-id/apiKeys/someone-else";
    assert_eq!(
        execute(
            "coinbase",
            KID,
            op("coinbase.restJwtEd25519.1", inputs(other))
        ),
        Err(SignError::Refused)
    );
}
