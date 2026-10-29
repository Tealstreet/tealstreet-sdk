#![cfg(feature = "lighter")]

use safe_cex_engine::signing::{Identities, Provider, SignError};
use safe_cex_venues::hosted_wallet::{self, Account};

// The Lighter crate's fixture-only 40-byte scalar, little-endian hex.
const SECRET: &str =
    "0x0b2d3a1c9e8f7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a3928170605040302010000000000";
// The device clock, 60 seconds before the auth statement's deadline.
const NOW: f64 = 1_700_000_000_000.;
const AUTH: &str =
    r#"{"kind":"auth","chainId":304,"accountIndex":42,"apiKeyIndex":3,"deadline":1700000060}"#;

fn device(venue: &str) -> (Provider, Identities) {
    let mut options: safe_cex_venues::EngineOptions = serde_json::from_value(serde_json::json!({
        "secret": SECRET, "apiKeyIndex": "3", "applicationId": "42"
    }))
    .unwrap();
    safe_cex_venues::prepare_local_signer_for(venue, &mut options).unwrap()
}

fn account(application_id: Option<&str>, testnet: bool) -> Account {
    Account {
        testnet,
        application_id: application_id.map(Into::into),
        api_key_index: Some("3".into()),
        ..Account::default()
    }
}

fn sign(
    venue: &str,
    account: &Account,
    to_sign: &str,
    output_format: &str,
) -> Result<String, SignError> {
    let (provider, identities) = device(venue);
    hosted_wallet::sign(
        venue,
        &identities,
        account,
        &provider,
        to_sign,
        output_format,
        NOW,
    )
}

#[test]
fn a_keyed_lighter_device_signs_the_words_it_re_derives_from_the_statement() {
    let signature = sign("lighter", &account(Some("42"), false), AUTH, "lighter-tx").unwrap();
    let bytes = (2..signature.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&signature[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    assert!(signature.starts_with("0x") && bytes.len() == 80);
    // The auth words: "{deadline}:{account}:{api key}" zero-padded to 8-byte words.
    let mut words = b"1700000060:42:3".to_vec();
    words.resize(16, 0);
    let scalar = (2..SECRET.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&SECRET[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let public = safe_cex_crypto::lighter::public_key(&scalar).unwrap();
    assert!(safe_cex_crypto::lighter::schnorr_verify(&words, &public, &bytes).unwrap());
    // An Application ID that is not an index leaves the account to the key itself.
    assert!(sign("lighter", &account(None, false), AUTH, "lighter-tx").is_ok());
}

#[test]
fn a_lighter_device_refuses_other_scopes_formats_and_bare_bytes() {
    let refused = |venue: &str, account: &Account, to_sign: &str, format: &str| {
        matches!(
            sign(venue, account, to_sign, format),
            Err(SignError::InvalidRequest)
        )
    };
    assert!(
        refused("lighter", &account(Some("43"), false), AUTH, "lighter-tx"),
        "another account"
    );
    assert!(
        refused("lighter", &account(Some("42"), true), AUTH, "lighter-tx"),
        "the testnet chain"
    );
    assert!(
        refused(
            "lighter-robinhood",
            &account(Some("42"), false),
            AUTH,
            "lighter-tx"
        ),
        "another deployment"
    );
    assert!(
        refused(
            "lighter",
            &account(Some("42"), false),
            "1700000060:42:3",
            "lighter-tx"
        ),
        "bare bytes"
    );
    // A Lighter key never answers HMAC work, and the format belongs to Lighter only.
    assert!(sign("lighter", &account(Some("42"), false), AUTH, "hex").is_err());
    // The device clock bounds the statement's expiry: never past, never beyond a lifetime.
    let (provider, identities) = device("lighter");
    for (now, why) in [
        (1_700_000_060_000., "an expired deadline"),
        (1_699_999_000_000., "a deadline beyond a lifetime"),
    ] {
        assert!(
            matches!(
                hosted_wallet::sign(
                    "lighter",
                    &identities,
                    &account(Some("42"), false),
                    &provider,
                    AUTH,
                    "lighter-tx",
                    now
                ),
                Err(SignError::InvalidRequest)
            ),
            "{why}"
        );
    }
    let mut no_key = account(Some("42"), false);
    no_key.api_key_index = None;
    assert!(matches!(
        sign("lighter", &no_key, AUTH, "lighter-tx"),
        Err(SignError::Unavailable)
    ));
}
