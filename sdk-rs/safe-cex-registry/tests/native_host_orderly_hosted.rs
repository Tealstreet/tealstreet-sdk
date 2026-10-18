//! Hosted HTTP preserves Orderly's local signing requirement and local-only WS.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use base64::Engine;
use safe_cex_exchange::{
    runtime::{Cleanup, RemoteSigner},
    socket_lifecycle::SocketSlot,
};
use safe_cex_transport::{
    TransportFuture,
    signing::{HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc, sync::OnceLock};

fn source() -> &'static Value {
    static CAPTURE: OnceLock<Value> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/orderly_hosted_source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual Orderly source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            capture["cases"].as_array().unwrap().len(),
            12,
            "Orderly source six-name/two-secret census"
        );
        for (index, name) in [
            "kodiakfi",
            "mode",
            "salsadex",
            "tealstreet-dex",
            "what",
            "woofi",
        ]
        .into_iter()
        .enumerate()
        {
            for (offset, local) in [true, false].into_iter().enumerate() {
                let case = &capture["cases"][index * 2 + offset];
                assert_eq!(case["name"], name);
                assert_eq!(case["local"], local);
                assert_eq!(
                    case["calls"].as_array().unwrap().len(),
                    if local { 2 } else { 1 }
                );
                assert_eq!(
                    case["wire"].as_array().unwrap().len(),
                    if local { 2 } else { 1 }
                );
                assert_eq!(
                    case["frames"].as_array().unwrap().len(),
                    if local { 5 } else { 0 }
                );
                assert_eq!(case["dials"].as_array().unwrap().len(), 1);
                assert_eq!(case["wsRemoteReads"], 0);
            }
        }
        eprintln!(
            "Orderly source Node {} / {} / V8 {}",
            capture["node"], capture["arch"], capture["v8"]
        );
        capture
    })
}
struct HostedOrderly {
    name: String,
    calls: RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for HostedOrderly {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let RemoteSignerRequest::Http(request) = request else {
            panic!("Orderly WS must retain its source local signer")
        };
        let mut headers = request.config.headers;
        headers.insert("X-Hosted-Orderly".into(), self.name.clone());
        let response = RemoteSignerResponse::Http(HttpSigningResponse {
            id: request.id,
            config: Some(SignedConfig {
                url: None,
                params: request.config.params,
                data: request.config.data,
                headers,
            }),
            error: None,
        });
        Box::pin(async move { Ok(response) })
    }
}
impl RemoteSigner for HostedOrderly {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}
fn verify(signature: &Value, preimage: &str) {
    let bytes = base64::engine::general_purpose::URL_SAFE
        .decode(signature.as_str().unwrap())
        .unwrap();
    let signature = ed25519_dalek::Signature::from_slice(&bytes).unwrap();
    let key = ed25519_dalek::SigningKey::from_bytes(&[17; 32]).verifying_key();
    key.verify_strict(preimage.as_bytes(), &signature)
        .expect("ORDERLY HOSTED source local Ed25519 signature");
    assert!(key.verify_strict(b"different-route", &signature).is_err());
}
async fn exercise(local: bool) {
    for (index, name) in [
        ExchangeName::Kodiakfi,
        ExchangeName::Mode,
        ExchangeName::Salsadex,
        ExchangeName::TealstreetDex,
        ExchangeName::What,
        ExchangeName::Woofi,
    ]
    .into_iter()
    .enumerate()
    {
        let expected = &source()["cases"][index * 2 + usize::from(!local)];
        let label = expected["name"].as_str().unwrap().to_owned();
        let signer = Rc::new(HostedOrderly {
            name: label.clone(),
            calls: RefCell::default(),
        });
        let h=support::venue_harness(name,Rc::new(|request|Some(if request.path=="/v1/client/info" {json!({"success":true,"data":{"account_id":"orderly-hosted"}})}else{json!({"success":true,"data":{"rows":[]}})})),
            json!({"key":"hosted-public-key","secret":if local {source()["encodedSeed"].as_str().unwrap()}else{""},"applicationId":"hosted-application"}),Some((signer.clone(),Some(format!("{label}-hosted-account")))),safe_cex_exchange::native::native_runtime()).await;
        assert!(
            h.exchange.can_sign(),
            "ORDERLY HOSTED typed capability {label}"
        );
        assert_eq!(h.exchange.base().options().key, "ed25519:hosted-public-key");
        if local {
            assert_eq!(
                h.exchange.get_account().await.unwrap().user_id,
                expected["account"]["userId"].as_str().unwrap()
            );
            assert_eq!(
                signer.calls.borrow().len(),
                1,
                "ORDERLY HOSTED private HTTP reaches shared signer {label}"
            );
        } else {
            assert!(h.exchange.base().options().secret.is_empty());
            let error = h
                .exchange
                .get_account()
                .await
                .expect_err("ORDERLY HOSTED private HTTP still requires source local secret");
            assert_eq!(error.name, expected["error"]["name"].as_str().unwrap());
            assert_eq!(
                error.message,
                expected["error"]["message"].as_str().unwrap()
            );
            assert!(
                signer.calls.borrow().is_empty(),
                "ORDERLY HOSTED secretless private failure precedes RPC {label}"
            );
            assert!(
                h.seen.borrow().is_empty(),
                "ORDERLY HOSTED secretless private failure precedes wire {label}"
            );
        }
        assert!(h.exchange.fetch_markets().await.unwrap().is_empty());
        {
            let calls = signer.calls.borrow();
            assert_eq!(
                calls.len(),
                if local { 2 } else { 1 },
                "ORDERLY HOSTED source HTTP census {label}"
            );
            for i in 0..if local { 2 } else { 1 } {
                for field in ["exchangeName", "accountName"] {
                    assert_eq!(
                        calls[i][field], expected["calls"][i][field],
                        "ORDERLY HOSTED metadata {field} {label}"
                    );
                }
                for field in ["method", "url", "baseURL"] {
                    assert_eq!(
                        calls[i]["config"][field], expected["calls"][i]["config"][field],
                        "ORDERLY HOSTED config {field} {label}"
                    );
                }
            }
            if local {
                let headers = &calls[0]["config"]["headers"];
                let time = headers["orderly-timestamp"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| headers["orderly-timestamp"].to_string());
                let preimage = format!("{time}GET/v1/client/info");
                assert_eq!(
                    headers["orderly-textsign"], preimage,
                    "ORDERLY HOSTED local signing precedes RPC {label}"
                );
                verify(&headers["orderly-signature"], &preimage);
            }
            let seen = h.seen.borrow();
            assert_eq!(seen.len(), if local { 2 } else { 1 });
            if local {
                for field in ["orderly-textsign", "orderly-signature"] {
                    assert_eq!(
                        seen[0].headers.get(field).map(String::as_str),
                        calls[0]["config"]["headers"][field].as_str(),
                        "ORDERLY HOSTED local signing survives callback to wire {field}"
                    );
                }
            }
            for request in seen.iter() {
                assert_eq!(
                    request.headers["x-hosted-orderly"], label,
                    "ORDERLY HOSTED callback header reaches wire"
                );
            }
        }
        let logs = Rc::new(RefCell::new(vec![]));
        let capture = logs.clone();
        h.exchange.base().on_args(
            "log",
            Rc::new(move |_, args| {
                capture.borrow_mut().push(
                    safe_cex_exchange::js_runtime::value::to_json(
                        &safe_cex_exchange::js_runtime::value::Js::array(args.to_vec()),
                    )
                    .unwrap(),
                );
                Ok(())
            }),
        );
        let socket = h.exchange.base().get_socket(SocketSlot::Private).unwrap();
        socket.connect_and_subscribe(false).unwrap();
        if local {
            support::eventually("Orderly local auth with hosted HTTP", || {
                h.received
                    .borrow()
                    .iter()
                    .any(|s| serde_json::from_str::<Value>(s).unwrap()["event"] == "auth")
            })
            .await;
            let frames: Vec<Value> = h
                .received
                .borrow()
                .iter()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect();
            assert_eq!(
                frames.len(),
                1,
                "ORDERLY HOSTED private subscriptions wait for auth reply {label}"
            );
            let auth = &frames[0];
            assert_eq!(
                auth["params"]["orderly_key"],
                expected["frames"][0]["params"]["orderly_key"]
            );
            verify(
                &auth["params"]["sign"],
                &format!("{:.0}", auth["params"]["timestamp"].as_f64().unwrap()),
            );
            for peer in h.peers.borrow().iter() {
                peer.push
                    .send(json!({"event":"auth","success":true}).to_string())
                    .unwrap();
            }
            support::eventually("Orderly three source private topics", || {
                h.received.borrow().len() == 5
            })
            .await;
            let frames: Vec<Value> = h
                .received
                .borrow()
                .iter()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect();
            assert_eq!(
                &frames[1..],
                &expected["frames"].as_array().unwrap()[1..],
                "ORDERLY HOSTED source private subscriptions {label}"
            );
        } else {
            assert_eq!(expected["wsError"]["name"], "TypeError");
            assert_eq!(expected["wsError"]["message"], "Expected String");
            support::eventually("Orderly approved shared signing recovery", || {
                logs.borrow().iter().any(|args| {
                    args[0].as_str().is_some_and(|s| {
                        s.contains("Private WebSocket signing could not complete; reconnecting")
                    })
                })
            })
            .await;
            assert!(
                socket.diagnostics().reconnect_count > 0,
                "ORDERLY HOSTED approved shared recovery {label}"
            );
            assert!(
                h.received.borrow().is_empty(),
                "ORDERLY HOSTED empty local secret sends no WS frames {label}"
            );
        }
        assert_eq!(
            signer.calls.borrow().len(),
            if local { 2 } else { 1 },
            "ORDERLY HOSTED WS never uses remote signer {label}"
        );
        let path = h.peers.borrow()[0].path.clone();
        let source_dial = expected["dials"][0].as_str().unwrap();
        let source_path = format!(
            "/{}",
            source_dial
                .split_once("://")
                .unwrap()
                .1
                .split_once('/')
                .unwrap()
                .1
        );
        assert_eq!(
            path, source_path,
            "ORDERLY HOSTED source application WS route {label}"
        );
        h.exchange.dispose().unwrap();
    }
}
#[test]
fn orderly_hosted_http_with_local_secret_and_local_only_ws_match_source() {
    support::run_local(exercise(true));
}
#[test]
fn orderly_hosted_empty_secret_preserves_source_http_and_ws_failures() {
    support::run_local(exercise(false));
}
