//! Woo family uses production Session, private-startup proof, signer and HPKE paths.
#[path = "join_negative/mod.rs"]
mod join_negative;
#[path = "join_negative/woo.rs"]
mod woo_fixture;
use join_negative::*;
use woo_fixture::Woo;

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
suite!(mainnet, Woo<false>);
suite!(testnet, Woo<true>);

#[tokio::test]
async fn native_socket_authentication_is_bound_to_each_connection() {
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
            .expect("WOO loopback reply")
            .unwrap()
            .unwrap();
        serde_json::from_str(frame.to_text().unwrap()).unwrap()
    }
    async fn check<const TESTNET: bool>() {
        let fixture = Fixture::new::<Woo<TESTNET>>().await;
        for other in [false, true] {
            let credentials = Woo::<TESTNET>::credentials(other);
            let timestamp = now_ms();
            let mac = hmac_sha256(
                credentials["secret"].as_str().unwrap().as_bytes(),
                format!("|{timestamp}").as_bytes(),
                Encoding::Hex,
            );
            // Native WOO serializes protocol milliseconds as a JSON f64 number.
            let auth = json!({"event":"auth","params":{"apikey":credentials["key"],"sign":mac,"timestamp":timestamp as f64}});
            let private = json!({"event":"subscribe","topic":"executionreport"});
            let (mut socket, _) = tokio_tungstenite::connect_async(&fixture.websocket_url)
                .await
                .unwrap();
            socket
                .send(Message::text(private.to_string()))
                .await
                .unwrap();
            assert_eq!(reply(&mut socket).await["code"], -1002);
            socket.send(Message::text(auth.to_string())).await.unwrap();
            assert_eq!(reply(&mut socket).await["success"], true);
            for topic in ["executionreport", "algoexecutionreportv2"] {
                socket
                    .send(Message::text(
                        json!({"event":"subscribe","topic":topic}).to_string(),
                    ))
                    .await
                    .unwrap();
                assert_eq!(reply(&mut socket).await["success"], true);
            }
            let (mut foreign, _) = tokio_tungstenite::connect_async(&fixture.websocket_url)
                .await
                .unwrap();
            foreign
                .send(Message::text(private.to_string()))
                .await
                .unwrap();
            assert_eq!(reply(&mut foreign).await["code"], -1002);
            foreign.close(None).await.unwrap();
            let mut invalid = Vec::new();
            for (field, value) in [
                ("apikey", json!("synthetic-unknown-key")),
                ("sign", json!("bad-mac")),
                ("timestamp", json!(1.0)),
                ("timestamp", json!(timestamp + 60_000)),
                ("timestamp", json!(timestamp.to_string())),
                ("timestamp", json!(1.5)),
            ] {
                let mut bad = auth.clone();
                bad["params"][field] = value;
                invalid.push(bad);
            }
            for field in ["apikey", "sign", "timestamp"] {
                let mut bad = auth.clone();
                bad["params"].as_object_mut().unwrap().remove(field);
                invalid.push(bad);
            }
            for bad in invalid {
                socket.send(Message::text(bad.to_string())).await.unwrap();
                assert_eq!(reply(&mut socket).await["code"], -1002);
                socket
                    .send(Message::text(private.to_string()))
                    .await
                    .unwrap();
                assert_eq!(reply(&mut socket).await["code"], -1002);
            }
            socket.close(None).await.unwrap();
        }
        assert_eq!(fixture.registry.account_count(), 0);
        assert_eq!(fixture.seen.lock().unwrap().writes, 0);
    }
    check::<false>().await;
    check::<true>().await;
}
