//! OKX family uses the real shared Session/Registry/HPKE join suite.
#[path = "join_negative/mod.rs"]
mod join_negative;
#[path = "join_negative/okx.rs"]
mod okx_fixture;
use join_negative::*;
use okx_fixture::Okx;
macro_rules! suite {
    ($module:ident, $venue:ty) => {
        mod $module {
            use super::*;
            #[tokio::test]
            async fn second_key_is_a_valid_independent_pair() {
                independent_pair::<$venue>().await;
            }
            #[tokio::test]
            async fn delegated_consumed_handover_preserves_registration() {
                delegated_consumed::<$venue>().await;
            }
            #[tokio::test]
            async fn cosigned_other_valid_key_preserves_resident() {
                cosigned_wrong_key::<$venue>().await;
            }
            #[tokio::test]
            async fn cosigned_prior_attempt_signature_preserves_resident() {
                cosigned_replay::<$venue>().await;
            }
            #[tokio::test]
            async fn cosigned_consumed_response_cannot_confirm_twice() {
                cosigned_consumed::<$venue>().await;
            }
            #[tokio::test]
            async fn delegated_wrong_credentials_preserve_resident() {
                delegated_wrong_pair::<$venue>().await;
            }
            #[tokio::test]
            async fn delegated_old_seal_cannot_join_a_new_attempt() {
                delegated_replay::<$venue>().await;
            }
        }
    };
}
suite!(mainnet, Okx<0>);
suite!(demo, Okx<1>);
suite!(eea, Okx<2>);
suite!(eea_demo, Okx<3>);
#[tokio::test]
async fn fixture_routes_only_to_loopback() {
    let f = Fixture::new::<Okx<0>>().await;
    assert!(f.websocket_url.starts_with("ws://127.0.0.1:"));
}

// F1: Native WsLogin emits an integer timestamp; the broad join proof alone
// previously let this fixture's rejected WS prerequisite go unobserved.
#[tokio::test]
async fn fixture_accepts_native_numeric_login_and_refuses_corrupt_proofs() {
    use futures_util::{SinkExt, StreamExt};
    use safe_cex_crypto::{Encoding, hmac_sha256};
    use safe_cex_exchange_server::accounts::now_ms;
    use serde_json::{Value, json};
    use std::time::Duration;
    use tokio_tungstenite::tungstenite::Message;
    async fn check<V: Venue>() {
        let fixture = Fixture::new::<V>().await;
        let (mut socket, _) = tokio_tungstenite::connect_async(&fixture.websocket_url)
            .await
            .unwrap();
        for other in [false, true] {
            let credentials = V::credentials(other);
            let stamp = now_ms() / 1000;
            let signature = hmac_sha256(
                credentials["secret"].as_str().unwrap().as_bytes(),
                format!("{stamp}GET/users/self/verify").as_bytes(),
                Encoding::Base64,
            );
            let login = json!({"op":"login","args":[{"apiKey":credentials["key"],"passphrase":credentials["passphrase"],"timestamp":stamp,"sign":signature}]});
            assert!(login["args"][0]["timestamp"].is_u64());
            socket.send(Message::text(login.to_string())).await.unwrap();
            let response = tokio::time::timeout(Duration::from_secs(2), socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let value: Value = serde_json::from_str(response.to_text().unwrap()).unwrap();
            assert_eq!(value["event"], "login");
            assert_eq!(
                value["code"], "0",
                "native numeric login must pass for each legitimate pair"
            );
            for (field, invalid) in [
                ("sign", json!("corrupt")),
                ("passphrase", json!("wrong")),
                ("apiKey", json!("unknown")),
                ("timestamp", Value::Null),
                ("timestamp", json!(stamp as f64 + 0.5)),
            ] {
                let mut bad = login.clone();
                bad["args"][0][field] = invalid;
                socket.send(Message::text(bad.to_string())).await.unwrap();
                let response = tokio::time::timeout(Duration::from_secs(2), socket.next())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
                let value: Value = serde_json::from_str(response.to_text().unwrap()).unwrap();
                assert_eq!(
                    value["code"], "60009",
                    "corrupt {field} must remain refused"
                );
            }
        }
        socket.close(None).await.unwrap();
        assert_eq!(fixture.registry.account_count(), 0);
        assert_eq!(fixture.seen.lock().unwrap().writes, 0);
    }
    check::<Okx<0>>().await;
    check::<Okx<1>>().await;
    check::<Okx<2>>().await;
    check::<Okx<3>>().await;
}
