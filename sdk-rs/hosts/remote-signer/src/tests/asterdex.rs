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

async fn check_descriptor_bound(payload: String, admitted: bool) {
    let ids = Identities {
        primary: Some(Identity::new(
            KeyRef("bounded-public".into()),
            Some(KeyRef("bounded-signing".into())),
        )),
        secondary: None,
    };
    let mut signer = LocalSigner::new();
    signer.insert(KeyRef("bounded-public".into()), b"fixture-key".to_vec());
    signer.insert(
        KeyRef("bounded-signing".into()),
        b"fixture-material".to_vec(),
    );
    let signer: Provider = Arc::new(signer);
    let (link, seen) = device(Box::new(move |request| {
        let descriptor = request["wsAuth"]["toSign"].as_str().unwrap();
        let format = request["wsAuth"]["outputFormat"].as_str().unwrap();
        assert_eq!(format, "asterdex-hmac");
        match hosted_wallet::sign(
            "asterdex",
            &ids,
            &Account::default(),
            &signer,
            descriptor,
            format,
            1_700_000_000_123.,
        ) {
            Ok(signed) => json!({"wsAuthSignature":signed}),
            Err(_) => json!({"error":"device descriptor refused"}),
        }
    }));
    let (account, options) = account(link, "asterdex", json!({"key":"fixture-key"}));
    let mut work = graph(&options);
    let Operation::Sign { payload: parts, .. } = &mut work.operations[1] else {
        unreachable!()
    };
    *parts = vec![bytes(&payload)];
    let result = account.provider.sign(work.clone()).await;
    if admitted {
        let result = result.unwrap();
        work.validate_result(&result).unwrap();
        assert_eq!(result.receipt, work.receipt);
        assert_eq!(result.values[0], b"fixture-key");
        assert_eq!(
            safe_cex_crypto::hex(&result.values[1]),
            safe_cex_crypto::hmac_sha256(
                b"fixture-material",
                payload.as_bytes(),
                safe_cex_crypto::Encoding::Hex,
            )
        );
        let sent = seen.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(
            sent[0]["wsAuth"]["toSign"].as_str().unwrap().len(),
            json!({"slot":"primary","payload":payload})
                .to_string()
                .len()
        );
    } else {
        assert!(result.is_err());
        assert!(
            seen.lock().unwrap().is_empty(),
            "unsupported descriptor reached the device"
        );
    }
}

#[tokio::test]
async fn asterdex_v1_serialized_descriptor_limit_is_inclusive() {
    let overhead = json!({"slot":"primary","payload":""}).to_string().len();
    let payload = "a".repeat(1_048_576 - overhead);
    assert_eq!(
        json!({"slot":"primary","payload":payload})
            .to_string()
            .len(),
        1_048_576
    );
    check_descriptor_bound(payload, true).await;
}

#[tokio::test]
async fn asterdex_v1_serialized_descriptor_plus_one_refuses_before_device() {
    let overhead = json!({"slot":"primary","payload":""}).to_string().len();
    check_descriptor_bound("a".repeat(1_048_576 - overhead + 1), false).await;
}

#[tokio::test]
async fn asterdex_v1_raw_limit_cannot_overflow_the_serialized_descriptor() {
    check_descriptor_bound("a".repeat(1_048_576), false).await;
}

#[tokio::test]
async fn asterdex_v1_escaping_expansion_refuses_before_device() {
    check_descriptor_bound("\"".repeat(524_288), false).await;
}

#[tokio::test]
async fn asterdex_v1_raw_payload_over_limit_refuses_before_device() {
    check_descriptor_bound("a".repeat(1_048_577), false).await;
}
