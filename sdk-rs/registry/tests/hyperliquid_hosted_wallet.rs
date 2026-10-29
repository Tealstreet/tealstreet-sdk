#![cfg(feature = "hyperliquid")]

use safe_cex_engine::signing::{Identities, Provider, SignError};
use safe_cex_venue_hyperliquid::sign;
use safe_cex_venues::hosted_wallet::{self, Account};

const ACCOUNT: &str = "0x1111111111111111111111111111111111111111";
const VAULT: &str = "0x3333333333333333333333333333333333333333";
const KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";

fn device(venue: &str) -> (Provider, Identities) {
    let mut options: safe_cex_venues::EngineOptions =
        serde_json::from_value(serde_json::json!({ "key": ACCOUNT, "secret": KEY })).unwrap();
    safe_cex_venues::prepare_local_signer_for(venue, &mut options).unwrap()
}

fn sign_ws(
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
        0.,
    )
}

fn account(vault: Option<&str>, testnet: bool) -> Account {
    Account {
        key: Some(ACCOUNT.into()),
        vault: vault.map(Into::into),
        testnet,
    }
}

#[derive(serde::Serialize)]
#[serde(tag = "type", rename = "cancel")]
struct Cancel {
    cancels: Vec<CancelWire>,
}
#[derive(serde::Serialize)]
struct CancelWire {
    a: u32,
    o: u64,
}

/// The device signature equals the engine's local signature for the same
/// action, so the remote-signed body is byte-identical.
#[test]
fn device_signs_the_statement_exactly_as_local_signing() {
    let action = Cancel {
        cancels: vec![CancelWire { a: 3, o: 77 }],
    };
    let json = serde_json::value::to_raw_value(&action).unwrap();
    for venue in ["hyperliquid", "hyperliquidspot"] {
        for (vault, testnet) in [
            (None, false),
            (None, true),
            (Some(VAULT), false),
            (Some(VAULT), true),
        ] {
            let nonce = 1_700_000_000_123;
            let statement = sign::statement::encode(&json, nonce, vault, !testnet).unwrap();
            let device = sign_ws(
                venue,
                &account(vault, testnet),
                &statement,
                "hyperliquid-l1",
            )
            .unwrap();
            let local = sign::sign(&action, vault, nonce, !testnet, KEY).unwrap();
            let pad = |word: &str| format!("{:0>64}", word.trim_start_matches("0x"));
            assert_eq!(
                device,
                format!("0x{}{}{:02x}", pad(&local.r), pad(&local.s), local.v),
                "{venue} {vault:?} {testnet}"
            );
        }
    }
}

#[test]
fn device_binds_its_agent_address_only_for_the_signed_in_account() {
    let address = safe_cex_crypto::ethereum::private_key_address(KEY).unwrap();
    let own = account(None, false);
    assert_eq!(
        sign_ws(
            "hyperliquid",
            &own,
            &ACCOUNT.to_uppercase().replace("0X", "0x"),
            "hyperliquid-address"
        )
        .unwrap(),
        address.to_ascii_lowercase()
    );
    assert_eq!(
        sign_ws(
            "hyperliquid",
            &own,
            "0x2222222222222222222222222222222222222222",
            "hyperliquid-address"
        ),
        Err(SignError::InvalidRequest)
    );
}

#[test]
fn device_refuses_other_networks_vaults_actions_and_formats() {
    let cancel = r#"{"type":"cancel","cancels":[{"a":3,"o":77}]}"#;
    let statement = |action: &str, mainnet: bool| {
        format!(r#"{{"kind":"l1","action":{action},"nonce":1,"isMainnet":{mainnet}}}"#)
    };
    let own = account(None, false);
    assert!(
        sign_ws(
            "hyperliquid",
            &own,
            &statement(cancel, true),
            "hyperliquid-l1"
        )
        .is_ok()
    );
    for (account, text) in [
        (account(None, true), statement(cancel, true)),
        (account(Some(VAULT), false), statement(cancel, true)),
        (
            own.clone(),
            statement(
                r#"{"type":"usdSend","hyperliquidChain":"Mainnet","signatureChainId":"0xa4b1","amount":"1","time":1,"destination":"0x0000000000000000000000000000000000000001"}"#,
                true,
            ),
        ),
        (own.clone(), "0x".to_owned() + &"ab".repeat(32)),
    ] {
        assert!(
            sign_ws("hyperliquid", &account, &text, "hyperliquid-l1").is_err(),
            "{text}"
        );
    }
    // Only the two wallet formats sign, and only for a wallet venue.
    for format in ["hex", "base64", "hyperliquid-user", ""] {
        assert_eq!(
            sign_ws("hyperliquid", &own, &statement(cancel, true), format),
            Err(SignError::InvalidRequest)
        );
    }
    let (provider, identities) = device("hyperliquid");
    assert_eq!(
        hosted_wallet::sign(
            "bitget",
            &identities,
            &own,
            &provider,
            &statement(cancel, true),
            "hyperliquid-l1",
            0.
        ),
        Err(SignError::Unsupported)
    );
}
