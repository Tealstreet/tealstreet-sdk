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

#[tokio::test]
async fn hosted_testnet_refuses_both_modes_when_hosted_origin_is_unavailable() {
    use backpack_fixture::BackpackTestnet;
    assert!(
        matches!(safe_cex_native::resolve::rest_base_url("backpack", true),
        Err(safe_cex_native::resolve::RoutingError::NoBaseUrl(venue)) if venue == "backpack")
    );
    let f = Fixture::new::<BackpackTestnet>().await;
    for mode in ["cosigned", "delegated"] {
        let mut client = Client::new(&f);
        let mut init = BackpackTestnet::init();
        init.signing_mode = Some(mode.into());
        if mode == "delegated" {
            let offer = client.offer::<BackpackTestnet>().await;
            init.sealed = Some(
                serde_json::from_value(seal(&offer, &init, BackpackTestnet::credentials(false)))
                    .unwrap(),
            );
        }
        client.init(init).await;
        client
            .finish::<BackpackTestnet>(&mut Device::default(), Answer::Correct)
            .await;
        assert!(f.registry.held(USER, ACCOUNT, client.socket()).is_none());
        assert_eq!(f.registry.account_count(), 0);
        let refusal = client
            .frames
            .iter()
            .find(|frame| frame["d"]["op"] == "error")
            .expect("typed unsupported-network refusal");
        assert_eq!(refusal["d"]["accountName"], ACCOUNT);
        assert_eq!(refusal["d"]["instanceId"], "negative-join-registration");
        assert_eq!(refusal["d"]["data"][1]["kind"], "unverified");
        // COSIGNED final construction and DELEGATED signer setup classify route refusal separately.
        assert_eq!(refusal["d"]["data"][1]["transient"], mode == "cosigned");
        assert!(
            !client
                .frames
                .iter()
                .any(|frame| frame["d"]["op"] == "account-signing-mode")
        );
        let requests: Vec<_> = client
            .frames
            .iter()
            .filter(|frame| frame["t"] == "sign-req")
            .collect();
        assert_eq!(requests.len(), usize::from(mode == "cosigned"));
        assert!(
            requests
                .iter()
                .all(|frame| frame["d"]["type"] == "join-proof")
        );
    }
    let seen = f.seen.lock().unwrap();
    assert_eq!(seen.signed_reads, 0);
    assert_eq!(seen.writes, 0);
}

#[test]
fn private_socket_fixture_requires_authentication_and_keeps_public_subscriptions_unsigned() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde_json::json;
    let mut seen = Observed::default();
    for seed in [7u8, 8] {
        let key = STANDARD.encode(safe_cex_crypto::ed25519::public_key(&[seed; 32]).unwrap());
        let signature = STANDARD.encode(
            safe_cex_crypto::ed25519::sign(
                b"instruction=subscribe&timestamp=10000&window=5000",
                &[seed; 32],
            )
            .unwrap(),
        );
        let reply = Backpack::websocket(
            &json!({"method":"SUBSCRIBE","params":["account.orderUpdate"],"signature":[key,signature,"10000","5000"]}),
            &mut seen,
        );
        assert!(reply.get("error").is_none());
    }
    let no_auth = Backpack::websocket(
        &json!({"method":"SUBSCRIBE","params":["account.orderUpdate"]}),
        &mut seen,
    );
    assert_eq!(no_auth["error"]["code"], "UNAUTHORIZED");
    let key = STANDARD.encode(safe_cex_crypto::ed25519::public_key(&[7; 32]).unwrap());
    for (seed, wire_stamp) in [(8u8, "10000"), (7, "10001")] {
        let signature = STANDARD.encode(
            safe_cex_crypto::ed25519::sign(
                b"instruction=subscribe&timestamp=10000&window=5000",
                &[seed; 32],
            )
            .unwrap(),
        );
        let reply = Backpack::websocket(
            &json!({"method":"SUBSCRIBE","params":["account.positionUpdate"],"signature":[key,signature,wire_stamp,"5000"]}),
            &mut seen,
        );
        assert_eq!(reply["error"]["code"], "UNAUTHORIZED");
    }
    let public = Backpack::websocket(
        &json!({"method":"SUBSCRIBE","params":["ticker.SOL_USDC_PERP"]}),
        &mut seen,
    );
    assert!(public.get("error").is_none());
    assert_eq!(seen.rejections, 3);
    assert_eq!(seen.writes, 0);
}
