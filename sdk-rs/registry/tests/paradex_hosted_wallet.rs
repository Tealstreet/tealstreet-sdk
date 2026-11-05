#![cfg(feature = "paradex")]
//! The Paradex device arm: it binds its Stark public key for its own account
//! and signs only digests it rebuilds from a closed `paradex/1` descriptor.

use safe_cex_engine::signing::{
    Algorithm, Identities, Operation, PayloadPart, Provider, SignError, SignRequest,
};
use safe_cex_venue_paradex::hosted;
use safe_cex_venues::hosted_wallet::{self, Account};

const ACCOUNT: &str = "0x0123";
const STARK: &str = "0x0000000000000000000000000000000000000000000000000000000000000007";
const NOW: f64 = 1_717_171_717_171.;

fn device(testnet: bool) -> (Provider, Identities) {
    let mut options: safe_cex_venues::EngineOptions = serde_json::from_value(serde_json::json!({
        "testnet": testnet,
        "paradexAddress": ACCOUNT,
        "subkeyAddress": "0x456",
    }))
    .unwrap();
    safe_cex_venues::local_signer_from_capsule(
        "paradex",
        serde_json::json!({"primary": {"secret": STARK}}).to_string(),
        &mut options,
    )
    .unwrap()
}

fn account(testnet: bool) -> Account {
    Account {
        testnet,
        paradex_address: Some(ACCOUNT.into()),
        ..Account::default()
    }
}

fn sign(testnet: bool, to_sign: &str, format: &str, now: f64) -> Result<String, SignError> {
    let (provider, identities) = device(testnet);
    hosted_wallet::sign(
        "paradex",
        &identities,
        &account(testnet),
        &provider,
        to_sign,
        format,
        now,
    )
}

fn auth(timestamp_s: u64) -> String {
    format!(
        r#"{{"family":"paradex","op":"auth","revision":"paradex/1","timestampS":{timestamp_s},"expirationS":{}}}"#,
        timestamp_s + 300
    )
}

fn batch(items: usize) -> String {
    let items: Vec<_> = (0..items)
        .map(|index| {
            format!(
                r#"{{"timestampMs":{},"market":"BTC-USD-PERP","side":"BUY","orderType":"LIMIT","size":"1.5","price":"{}","triggerPrice":null,"instruction":"GTC","flags":null,"stp":"EXPIRE_TAKER","clientId":"c{index}"}}"#,
                NOW as u64,
                100 + index
            )
        })
        .collect();
    format!(
        r#"{{"family":"paradex","op":"orderBatch","revision":"paradex/1","items":[{}]}}"#,
        items.join(",")
    )
}

#[test]
fn the_public_key_binds_only_for_the_own_account() {
    let scalar = safe_cex_crypto::starknet::parse_stark_key(STARK).unwrap();
    let public = safe_cex_crypto::starknet::public_key_bytes(&scalar);
    assert_eq!(
        sign(false, ACCOUNT, "paradex-public", NOW),
        Ok(format!("0x{}", safe_cex_crypto::hex(&public)))
    );
    // Address case is not identity; another account is refused.
    assert!(sign(false, "0X0123", "paradex-public", NOW).is_ok());
    assert_eq!(
        sign(false, "0x0124", "paradex-public", NOW),
        Err(SignError::InvalidRequest)
    );
}

#[test]
fn every_rebuilt_digest_is_signed_in_item_order_for_the_device_network() {
    for testnet in [false, true] {
        let binding = hosted::ParadexBinding::new(ACCOUNT, hosted::network_chain(testnet)).unwrap();
        for (statement, count) in [(auth(NOW as u64 / 1000), 1), (batch(1), 1), (batch(10), 10)] {
            let signature = sign(testnet, &statement, "paradex-stark", NOW).unwrap();
            assert_eq!(signature.len(), 2 + count * 128);
            let descriptor = hosted::ParadexDescriptor::from_json(&statement).unwrap();
            let digests = hosted::validated_digests(&descriptor, &binding, NOW, NOW + 1.).unwrap();
            // Deterministic Stark signing: each item equals the provider's own
            // signature over the digest the device rebuilt.
            let (provider, identities) = device(testnet);
            let key = identities.primary.unwrap().signing_ref().unwrap().clone();
            let expected: String = digests
                .as_slice()
                .iter()
                .map(|digest| {
                    let work = SignRequest {
                        receipt: Default::default(),
                        context: Default::default(),
                        operations: vec![Operation::Sign {
                            algorithm: Algorithm::StarkDigest,
                            key: key.clone(),
                            payload: vec![PayloadPart::Bytes {
                                bytes: digest.to_vec(),
                            }],
                        }],
                    };
                    let result = provider.sign_ready(&work).unwrap().unwrap();
                    safe_cex_crypto::hex(&result.values[0])
                })
                .collect();
            assert_eq!(signature, format!("0x{expected}"));
        }
    }
}

#[test]
fn malformed_stale_foreign_and_bare_digest_requests_refuse_before_signing() {
    let stale = auth(NOW as u64 / 1000 - 31);
    let digest = format!("0x{}", "11".repeat(32));
    let mut extra = batch(1);
    extra.insert_str(extra.len() - 1, r#","digest":"0x1""#);
    for statement in [
        stale.as_str(),
        digest.as_str(),
        extra.as_str(),
        &batch(11),
        "{}",
    ] {
        assert_eq!(
            sign(false, statement, "paradex-stark", NOW),
            Err(SignError::InvalidRequest),
            "{statement}"
        );
    }
    // A wallet format on Paradex, and a Paradex format elsewhere, refuse.
    assert_eq!(
        sign(false, &auth(NOW as u64 / 1000), "hyperliquid-l1", NOW),
        Err(SignError::InvalidRequest)
    );
    let (provider, identities) = device(false);
    assert!(
        hosted_wallet::sign(
            "binance",
            &identities,
            &account(false),
            &provider,
            &batch(1),
            "paradex-stark",
            NOW
        )
        .is_err()
    );
}
