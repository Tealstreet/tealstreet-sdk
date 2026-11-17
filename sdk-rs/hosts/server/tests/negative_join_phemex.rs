//! Phemex family uses production Session, private-startup proof, signer and HPKE paths.
#[path = "join_negative/mod.rs"]
mod join_negative;
#[path = "join_negative/phemex.rs"]
mod phemex_fixture;
use join_negative::*;
use phemex_fixture::Phemex;

macro_rules! suite {
    ($module:ident, $venue:ty) => {
        mod $module {
            use super::*;
            #[tokio::test]
            async fn second_key_is_a_valid_independent_pair() {
                independent_pair::<$venue>().await;
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
            #[tokio::test]
            async fn delegated_consumed_handover_preserves_registration() {
                delegated_consumed::<$venue>().await;
            }
        }
    };
}
suite!(mainnet, Phemex<false>);
suite!(testnet, Phemex<true>);

#[tokio::test]
async fn native_socket_authentication_and_connection_state_are_verified() {
    use futures_util::{SinkExt, StreamExt};
    use safe_cex_crypto::{Encoding, hmac_sha256};
    use safe_cex_exchange_server::accounts::now_ms;
    use serde_json::{Value, json};
    use std::time::Duration;
    use tokio_tungstenite::{WebSocketStream, tungstenite::Message};

    async fn reply<S>(socket: &mut WebSocketStream<S>) -> Value
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let frame = tokio::time::timeout(Duration::from_secs(2), socket.next())
            .await
            .expect("loopback Phemex socket response")
            .unwrap()
            .unwrap();
        serde_json::from_str(frame.to_text().unwrap()).unwrap()
    }
    async fn check<const TESTNET: bool>() {
        let f = Fixture::new::<Phemex<TESTNET>>().await;
        for other in [false, true] {
            let credentials = Phemex::<TESTNET>::credentials(other);
            let expiry = now_ms() / 1000 + 60;
            let signature = hmac_sha256(
                credentials["secret"].as_str().unwrap().as_bytes(),
                format!("{}{expiry}", credentials["key"].as_str().unwrap()).as_bytes(),
                Encoding::Hex,
            );
            // This is the native WsAuth writer's exact u64 expiry representation.
            let auth = json!({"id":1,"method":"user.auth","params":["API",credentials["key"],signature,expiry]});
            let subscribe = json!({"id":2,"method":"aop_p.subscribe","params":[]});
            let (mut socket, _) = tokio_tungstenite::connect_async(&f.websocket_url)
                .await
                .unwrap();
            socket
                .send(Message::text(subscribe.to_string()))
                .await
                .unwrap();
            assert_eq!(reply(&mut socket).await["error"]["code"], 401);
            assert!(
                tokio::time::timeout(Duration::from_millis(25), socket.next())
                    .await
                    .is_err()
            );
            socket.send(Message::text(auth.to_string())).await.unwrap();
            assert_eq!(reply(&mut socket).await["result"]["status"], "success");
            socket
                .send(Message::text(subscribe.to_string()))
                .await
                .unwrap();
            assert_eq!(reply(&mut socket).await["result"]["status"], "success");
            assert_eq!(reply(&mut socket).await["type"], "snapshot");
            let mut invalid = Vec::new();
            for (index, value) in [
                (0, json!("not-API")),
                (1, json!("synthetic-unregistered-key")),
                (1, Value::Null),
                (2, json!("bad-mac")),
                (2, Value::Null),
                (3, json!(0)),
                (3, json!(expiry.to_string())),
                (3, json!(1.5)),
            ] {
                let mut request = auth.clone();
                request["params"][index] = value;
                invalid.push(request);
            }
            invalid.push(json!({"id":1,"method":"user.auth","params":[]}));
            invalid.push(json!({"id":1,"method":"user.auth","params":["API",credentials["key"],auth["params"][2],expiry,"extra"]}));
            for request in invalid {
                socket
                    .send(Message::text(request.to_string()))
                    .await
                    .unwrap();
                assert_eq!(reply(&mut socket).await["error"]["code"], 401);
                socket
                    .send(Message::text(subscribe.to_string()))
                    .await
                    .unwrap();
                assert_eq!(reply(&mut socket).await["error"]["code"], 401);
                assert!(
                    tokio::time::timeout(Duration::from_millis(25), socket.next())
                        .await
                        .is_err()
                );
            }
            socket.close(None).await.unwrap();
        }
        assert_eq!(f.registry.account_count(), 0);
        assert_eq!(f.seen.lock().unwrap().writes, 0);
    }
    check::<false>().await;
    check::<true>().await;
}
