use super::*;
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, Provider};
use safe_cex_venues::hosted_wallet::{self, Account};

const PAYLOAD: &str = "recvWindow=20000&timestamp=1700000000123";

fn graph(options: &Value) -> SignRequest {
    let (credential, signing, _) = refs(options);
    request(
        vec![
            Operation::Present { key: credential },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: signing,
                payload: vec![bytes(PAYLOAD)],
            },
        ],
        SignReceipt {
            stamp_ms: Some(1_700_000_000_123.),
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn asterdex_v1_uses_the_actual_device_codec_and_retains_receipt() {
    let ids = Identities {
        primary: Some(Identity::new(
            KeyRef("device-public".into()),
            Some(KeyRef("device-mac".into())),
        )),
        secondary: None,
    };
    let mut signer = LocalSigner::new();
    signer.insert(KeyRef("device-public".into()), b"fixture-key".to_vec());
    signer.insert(KeyRef("device-mac".into()), b"fixture-material".to_vec());
    let signer: Provider = Arc::new(signer);
    let (link, seen) = device(Box::new(move |request| {
        let format = request["wsAuth"]["outputFormat"].as_str().unwrap();
        assert_eq!(format, "asterdex-hmac");
        let descriptor = request["wsAuth"]["toSign"].as_str().unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(descriptor).unwrap(),
            json!({"slot":"primary","payload":PAYLOAD})
        );
        let signed = hosted_wallet::sign(
            "asterdex",
            &ids,
            &Account::default(),
            &signer,
            descriptor,
            format,
            1_700_000_000_123.,
        )
        .unwrap();
        json!({"wsAuthSignature":signed})
    }));
    let (account, options) = account(link, "asterdex", json!({"key":"fixture-key"}));
    let work = graph(&options);
    let result = account.provider.sign(work.clone()).await.unwrap();
    work.validate_result(&result).unwrap();
    assert_eq!(result.receipt, work.receipt);
    assert_eq!(result.values.len(), 2);
    assert_eq!(result.values[0], b"fixture-key");
    assert_eq!(
        safe_cex_crypto::hex(&result.values[1]),
        safe_cex_crypto::hmac_sha256(
            b"fixture-material",
            PAYLOAD.as_bytes(),
            safe_cex_crypto::Encoding::Hex
        )
    );
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn asterdex_v1_refuses_wrong_credential_partial_or_noncanonical_batch() {
    for reply in [
        json!({"credential":"foreign-key","signature":"ab".repeat(32)}),
        json!({"signature":"ab".repeat(32)}),
        json!({"credential":"fixture-key","signature":"ab".repeat(31)}),
        json!({"credential":"fixture-key","signature":"AB".repeat(32)}),
        json!({"credential":"fixture-key","signature":"ab".repeat(32),"extra":true}),
    ] {
        let (link, _) = device(Box::new(
            move |_| json!({"wsAuthSignature":reply.to_string()}),
        ));
        let (account, options) = account(link, "asterdex", json!({"key":"fixture-key"}));
        assert!(account.provider.sign(graph(&options)).await.is_err());
    }
}

#[tokio::test]
async fn asterdex_v1_partial_foreign_and_wallet_graphs_never_reach_device() {
    let (link, seen) = device(Box::new(|_| panic!("unsupported graph reached device")));
    let (account, options) = account(link, "asterdex", json!({"key":"fixture-key"}));
    let valid = graph(&options);
    let mut partial = valid.clone();
    partial.operations.remove(0);
    let mut foreign = valid.clone();
    let Operation::Sign { key, .. } = &mut foreign.operations[1] else {
        unreachable!()
    };
    *key = KeyRef("foreign".into());
    let mut wallet = valid.clone();
    wallet.receipt.nonce = Some(1);
    for work in [partial, foreign, wallet] {
        assert!(account.provider.sign(work).await.is_err());
    }
    assert!(seen.lock().unwrap().is_empty());
}
