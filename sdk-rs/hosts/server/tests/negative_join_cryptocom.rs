//! CryptoCom family uses production Session, private-startup proof, signer and HPKE paths.
#[path = "join_negative/cryptocom.rs"]
mod cryptocom_fixture;
#[path = "join_negative/mod.rs"]
mod join_negative;
use cryptocom_fixture::CryptoCom;
use join_negative::*;

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
suite!(mainnet, CryptoCom<false>);
suite!(testnet, CryptoCom<true>);

#[tokio::test]
async fn native_socket_authentication_and_private_channels_are_connection_bound() {
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
            .expect("Crypto.com loopback reply")
            .unwrap()
            .unwrap();
        serde_json::from_str(frame.to_text().unwrap()).unwrap()
    }
    async fn check<const TESTNET: bool>() {
        let fixture = Fixture::new::<CryptoCom<TESTNET>>().await;
        for other in [false, true] {
            let pair = CryptoCom::<TESTNET>::credentials(other);
            let key = pair["key"].as_str().unwrap();
            let nonce = now_ms();
            let auth = json!({"id":nonce,"method":"public/auth","api_key":key,"nonce":nonce,"sig":hmac_sha256(pair["secret"].as_str().unwrap().as_bytes(), format!("public/auth{nonce}{key}{nonce}").as_bytes(), Encoding::Hex)});
            let private = json!({"id":9,"method":"subscribe","params":{"channels":["user.order","user.advance.order","user.positions","user.balance","user.trade"]}});
            let (mut socket, _) = tokio_tungstenite::connect_async(&fixture.websocket_url)
                .await
                .unwrap();
            socket
                .send(Message::text(private.to_string()))
                .await
                .unwrap();
            assert_eq!(reply(&mut socket).await["code"], 40101);
            socket.send(Message::text(auth.to_string())).await.unwrap();
            let accepted = reply(&mut socket).await;
            assert_eq!(accepted["code"], 0);
            assert_eq!(accepted["id"], nonce);
            socket
                .send(Message::text(private.to_string()))
                .await
                .unwrap();
            assert_eq!(reply(&mut socket).await["code"], 0);
            let (mut foreign, _) = tokio_tungstenite::connect_async(&fixture.websocket_url)
                .await
                .unwrap();
            foreign
                .send(Message::text(private.to_string()))
                .await
                .unwrap();
            assert_eq!(reply(&mut foreign).await["code"], 40101);
            foreign.close(None).await.unwrap();
            let mut invalid = Vec::new();
            for (field, value) in [
                ("api_key", json!("synthetic-unregistered-key")),
                ("sig", json!("bad-mac")),
                ("nonce", json!(1)),
                ("nonce", json!(nonce.to_string())),
                ("id", json!(1)),
                ("params", json!({"hidden":"write"})),
            ] {
                let mut bad = auth.clone();
                bad[field] = value;
                invalid.push(bad);
            }
            for field in ["id", "api_key", "nonce", "sig"] {
                let mut bad = auth.clone();
                bad.as_object_mut().unwrap().remove(field);
                invalid.push(bad);
            }
            for bad in invalid {
                socket.send(Message::text(bad.to_string())).await.unwrap();
                assert_eq!(reply(&mut socket).await["code"], 40101);
                socket
                    .send(Message::text(private.to_string()))
                    .await
                    .unwrap();
                assert_eq!(reply(&mut socket).await["code"], 40101);
            }
            socket
                .send(Message::text(
                    json!({"id":10,"method":"public/respond-heartbeat"}).to_string(),
                ))
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(25), socket.next())
                    .await
                    .is_err()
            );
            socket.close(None).await.unwrap();
        }
        assert_eq!(fixture.registry.account_count(), 0);
        assert_eq!(fixture.seen.lock().unwrap().writes, 0);
    }
    check::<false>().await;
    check::<true>().await;
}
