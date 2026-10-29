#![cfg(feature = "hyperliquid")]

use safe_cex_engine::signing::{Identities, Provider, SignError};
use safe_cex_venue_hyperliquid::sign;
use safe_cex_venues::hosted_signing::{self, Account, Input, OutputFormat};

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
    output_format: OutputFormat,
) -> Result<String, SignError> {
    let (provider, identities) = device(venue);
    let (work, kind) = hosted_signing::prepare_for(
        venue,
        &identities,
        account,
        Input::WsAuth {
            to_sign: to_sign.into(),
            output_format,
        },
    )?;
    work.validate()?;
    let result = provider.sign_ready(&work).unwrap()?;
    work.validate_result(&result)?;
    Ok(hosted_signing::finish(kind, &result)?.signature.unwrap())
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
                OutputFormat::HyperliquidL1,
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
            OutputFormat::HyperliquidAddress
        )
        .unwrap(),
        address.to_ascii_lowercase()
    );
    assert_eq!(
        sign_ws(
            "hyperliquid",
            &own,
            "0x2222222222222222222222222222222222222222",
            OutputFormat::HyperliquidAddress
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
            OutputFormat::HyperliquidL1
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
            sign_ws("hyperliquid", &account, &text, OutputFormat::HyperliquidL1).is_err(),
            "{text}"
        );
    }
    // HMAC formats never reach a wallet key, and wallet formats no other venue.
    for format in [OutputFormat::Hex, OutputFormat::Base64] {
        assert_eq!(
            sign_ws("hyperliquid", &own, &statement(cancel, true), format),
            Err(SignError::Unsupported)
        );
    }
    let (_, identities) = device("hyperliquid");
    let wrong_venue = hosted_signing::prepare_for(
        "bitget",
        &identities,
        &own,
        Input::WsAuth {
            to_sign: statement(cancel, true),
            output_format: OutputFormat::HyperliquidL1,
        },
    );
    assert!(matches!(wrong_venue, Err(SignError::InvalidRequest)));
    let credential =
        hosted_signing::prepare_for("hyperliquid", &identities, &own, Input::Credential {});
    assert!(matches!(credential, Err(SignError::Unsupported)));
}
