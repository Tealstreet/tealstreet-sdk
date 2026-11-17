//! Backpack negative admission through the shared real Session/native/HPKE harness.
#[path = "join_negative/backpack.rs"]
mod backpack_fixture;
#[path = "join_negative/mod.rs"]
mod join_negative;
use backpack_fixture::Backpack;
use join_negative::*;

#[tokio::test]
async fn second_key_is_a_valid_independent_pair() {
    independent_pair::<Backpack>().await;
}
#[tokio::test]
async fn cosigned_other_valid_key_preserves_resident() {
    cosigned_wrong_key::<Backpack>().await;
}
#[tokio::test]
async fn cosigned_prior_attempt_signature_preserves_resident() {
    cosigned_replay::<Backpack>().await;
}
#[tokio::test]
async fn cosigned_consumed_response_cannot_confirm_twice() {
    cosigned_consumed::<Backpack>().await;
}
#[tokio::test]
async fn delegated_wrong_credentials_preserve_resident() {
    delegated_wrong_pair::<Backpack>().await;
}
#[tokio::test]
async fn delegated_old_seal_cannot_join_a_new_attempt() {
    delegated_replay::<Backpack>().await;
}
#[tokio::test]
async fn delegated_consumed_handover_preserves_registration() {
    delegated_consumed::<Backpack>().await;
}

#[tokio::test]
async fn fixture_validates_both_pairs_and_rejects_wrong_material_and_stale_bytes() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde_json::Value;
    use std::time::Duration;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let f = Fixture::new::<Backpack>().await;
    let base = f
        .websocket_url
        .strip_suffix("/ws")
        .unwrap()
        .replacen("ws:", "http:", 1);
    assert!(base.starts_with("http://127.0.0.1:"));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    for (public_seed, signing_seed, signed_stamp, wire_stamp, status) in [
        (7, 7, "10000", "10000", 200),
        (8, 8, "10000", "10000", 200),
        (7, 8, "10000", "10000", 401),
        (7, 7, "10000", "10001", 401),
    ] {
        let key =
            STANDARD.encode(safe_cex_crypto::ed25519::public_key(&[public_seed; 32]).unwrap());
        let text = format!("instruction=accountQuery&timestamp={signed_stamp}&window=5000");
        let signature =
            safe_cex_crypto::ed25519::sign(text.as_bytes(), &[signing_seed; 32]).unwrap();
        let response = client
            .get(format!("{base}/api/v1/account"))
            .header("x-api-key", key)
            .header("x-signature", STANDARD.encode(signature))
            .header("x-timestamp", wire_stamp)
            .header("x-window", "5000")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status);
        let body: Value = response.json().await.unwrap();
        if status == 401 {
            assert_eq!(body["code"], "UNAUTHORIZED");
        } else {
            assert_eq!(body["leverageLimit"], "10");
        }
    }
    let seen = f.seen.lock().unwrap();
    assert_eq!(seen.signed_reads, 4);
    assert_eq!(seen.rejections, 2);
    assert_eq!(seen.writes, 0);
}
