#![cfg(feature = "paradex")]
//! The Paradex device arm: it binds its Stark public key for its own account
//! and signs only digests it rebuilds from a closed `paradex/1` descriptor.

use safe_cex_engine::signing::{Identities, Operation, Provider, SignError, SignRequest};
use safe_cex_venue_paradex::hosted;
use safe_cex_venues::hosted_signing::{self, Account, Input, OutputFormat};

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

fn account(testnet: bool, now_ms: Option<f64>) -> Account {
    Account {
        testnet,
        paradex_address: Some(ACCOUNT.into()),
        now_ms,
        ..Account::default()
    }
}

fn ws(to_sign: &str, output_format: OutputFormat) -> Input {
    Input::WsAuth {
        to_sign: to_sign.into(),
        output_format,
    }
}

fn capture(
    provider: &Provider,
    identities: &Identities,
    account: &Account,
    input: Input,
) -> Result<(SignRequest, String), SignError> {
    let (work, kind) = hosted_signing::prepare_for("paradex", identities, account, input)?;
    let result = provider
        .sign_ready(&work)
        .expect("local signing is ready")?;
    work.validate_result(&result)?;
    Ok((
        work,
        hosted_signing::finish(kind, &result)?.signature.unwrap(),
    ))
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
    let (provider, ids) = device(false);
    let (work, key) = capture(
        &provider,
        &ids,
        &account(false, None),
        ws(ACCOUNT, OutputFormat::ParadexPublic),
    )
    .unwrap();
    assert!(matches!(work.operations[..], [Operation::PublicKey { .. }]));
    let scalar = safe_cex_crypto::starknet::parse_stark_key(STARK).unwrap();
    let public = safe_cex_crypto::starknet::public_key_bytes(&scalar);
    assert_eq!(key, format!("0x{}", safe_cex_crypto::hex(&public)));
    // Address case is not identity; another account is refused.
    assert!(
        capture(
            &provider,
            &ids,
            &account(false, None),
            ws("0X0123", OutputFormat::ParadexPublic)
        )
        .is_ok()
    );
    assert!(matches!(
        capture(
            &provider,
            &ids,
            &account(false, None),
            ws("0x0124", OutputFormat::ParadexPublic)
        ),
        Err(SignError::InvalidRequest)
    ));
}

#[test]
fn every_rebuilt_digest_is_signed_in_item_order_for_the_device_network() {
    for testnet in [false, true] {
        let (provider, ids) = device(testnet);
        let binding = hosted::ParadexBinding::new(ACCOUNT, hosted::network_chain(testnet)).unwrap();
        for (statement, count) in [(auth(NOW as u64 / 1000), 1), (batch(1), 1), (batch(10), 10)] {
            let (work, signature) = capture(
                &provider,
                &ids,
                &account(testnet, Some(NOW)),
                ws(&statement, OutputFormat::ParadexStark),
            )
            .unwrap();
            let descriptor = hosted::ParadexDescriptor::from_json(&statement).unwrap();
            let digests = hosted::validated_digests(&descriptor, &binding, NOW, NOW + 1.).unwrap();
            assert_eq!(work.operations.len(), count);
            for (operation, digest) in work.operations.iter().zip(digests.as_slice()) {
                let Operation::Sign { payload, .. } = operation else {
                    panic!("sign")
                };
                assert_eq!(
                    payload,
                    &[safe_cex_engine::signing::PayloadPart::Bytes {
                        bytes: digest.to_vec()
                    }]
                );
            }
            assert_eq!(signature.len(), 2 + count * 128);
        }
    }
}

#[test]
fn malformed_stale_foreign_and_bare_digest_requests_refuse_before_signing() {
    let (provider, ids) = device(false);
    let now = account(false, Some(NOW));
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
        assert!(
            matches!(
                hosted_signing::prepare_for(
                    "paradex",
                    &ids,
                    &now,
                    ws(statement, OutputFormat::ParadexStark)
                ),
                Err(SignError::InvalidRequest)
            ),
            "{statement}"
        );
    }
    // No clock, no time policy, no signature.
    assert!(matches!(
        hosted_signing::prepare_for(
            "paradex",
            &ids,
            &account(false, None),
            ws(&auth(NOW as u64 / 1000), OutputFormat::ParadexStark)
        ),
        Err(SignError::InvalidRequest)
    ));
    // Formats stay with their families: no HMAC or wallet format on Paradex,
    // and no Stark format elsewhere.
    for format in [OutputFormat::Hex, OutputFormat::Base64] {
        assert!(matches!(
            hosted_signing::prepare_for("paradex", &ids, &now, ws("x", format)),
            Err(SignError::Unsupported)
        ));
    }
    assert!(matches!(
        hosted_signing::prepare_for("paradex", &ids, &now, ws("x", OutputFormat::HyperliquidL1)),
        Err(SignError::InvalidRequest)
    ));
    assert!(matches!(
        hosted_signing::prepare_for(
            "binance",
            &ids,
            &now,
            ws(&batch(1), OutputFormat::ParadexStark)
        ),
        Err(SignError::InvalidRequest)
    ));
    let _ = provider;
}
