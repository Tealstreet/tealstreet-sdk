//! BingX registry construction and signed lifecycle over real native local HTTP/WS.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_transport::RestAdapter;
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::{Value, json};
use std::rc::Rc;
use support::*;

fn routes(request: &Seen) -> Option<Value> {
    let data = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/openApi/swap/v2/quote/contracts") => {
            json!([{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":3,"pricePrecision":2,"tradeMinLimit":0.01,"maxLongLeverage":50}])
        }
        ("GET", "/openApi/swap/v2/quote/ticker") => {
            json!([{"symbol":"BTC-USDT","lastPrice":"100","priceChangePercent":"1","volume":"10","quoteVolume":"1000"}])
        }
        ("GET", "/openApi/swap/v2/quote/premiumIndex") => {
            json!([{"symbol":"BTC-USDT","markPrice":"101","indexPrice":"100","lastFundingRate":"0.01","nextFundingTime":1700000100000_f64}])
        }
        ("GET", "/openApi/swap/v3/user/balance") => {
            json!([{"asset":"USDT","userId":"native-bingx","balance":"10","availableMargin":"8","usedMargin":"2","unrealizedProfit":"1"}])
        }
        ("GET", "/openApi/swap/v1/positionSide/dual") => json!({"dualSidePosition":"false"}),
        ("GET", "/openApi/swap/v2/user/positions") => json!([]),
        ("GET", "/openApi/swap/v2/trade/openOrders") => json!([]),
        ("POST", "/openApi/user/auth/userDataStream") => {
            return Some(json!({"listenKey":"bingx-native-key"}));
        }
        ("POST", "/openApi/swap/v2/trade/order") => {
            json!({"order":{"orderId":9007199254740993_u64,"symbol":"BTC-USDT","side":"BUY","positionSide":"BOTH","type":"LIMIT","status":"NEW","origQty":"1","price":"100","stopPrice":"0","executedQty":"0","timeInForce":"GTC","reduceOnly":false}})
        }
        ("DELETE", "/openApi/swap/v2/trade/order") => json!({}),
        _ => return None,
    };
    Some(json!({"code":0,"data":data}))
}

#[derive(Default)]
struct ListenKeyGate {
    calls: std::sync::atomic::AtomicUsize,
    entered: std::sync::atomic::AtomicBool,
    release: tokio::sync::Notify,
}

struct HeldListenKeyAdapter {
    inner: safe_cex_transport::native::ReqwestAdapter,
    origin: String,
    gate: std::sync::Arc<ListenKeyGate>,
}
impl RestAdapter for HeldListenKeyAdapter {
    fn execute(
        &self,
        request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|receipt| receipt.response)
        })
    }
    fn execute_with_receipt(
        &self,
        mut request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestExecution> {
        use std::sync::atomic::Ordering;
        let held = request.method.eq_ignore_ascii_case("POST")
            && request.url.ends_with("/openApi/user/auth/userDataStream")
            && self.gate.calls.fetch_add(1, Ordering::SeqCst) == 1;
        request.base_url = request.base_url.map(|url| rewrite(&url, &self.origin));
        if request.url.starts_with("http") {
            request.url = rewrite(&request.url, &self.origin);
        }
        Box::pin(async move {
            let receipt = self.inner.execute_with_receipt(request).await?;
            if held {
                self.gate.entered.store(true, Ordering::SeqCst);
                self.gate.release.notified().await;
            }
            Ok(receipt)
        })
    }
}

#[test]
fn native_listen_key_rebuild_resumes_after_io_wake_and_ignores_old_wire_close() {
    use futures_util::{SinkExt, StreamExt};
    use safe_cex_registry::{
        native_host::{hyperliquid_rate_limiter, native_venue_host_with},
        rust_venues::rust_constructor,
    };
    use std::{
        cell::{Cell, RefCell},
        sync::{Arc, atomic::Ordering},
        time::Duration,
    };
    use tokio::{net::TcpListener, sync::mpsc};
    use tokio_tungstenite::tungstenite::Message;
    run_local(async {
        let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_origin = format!("http://{}", http.local_addr().unwrap());
        let ws_origin = format!("ws://{}", ws.local_addr().unwrap());
        let seen = Rc::new(RefCell::new(vec![]));
        let keys = Rc::new(Cell::new(0));
        tokio::task::spawn_local(serve_http(
            http,
            Rc::new(move |request| {
                if request.method == "POST" && request.path == "/openApi/user/auth/userDataStream" {
                    keys.set(keys.get() + 1);
                    return Some(
                        json!({"listenKey":if keys.get() == 1 {"bingx-native-key"} else {"fresh-key"}}),
                    );
                }
                routes(request)
            }),
            seen.clone(),
        ));
        let peers = Rc::new(RefCell::new(
            Vec::<(String, mpsc::UnboundedSender<Message>)>::new(),
        ));
        let captured = peers.clone();
        let close_written = Rc::new(Cell::new(false));
        let closed = close_written.clone();
        tokio::task::spawn_local(async move {
            loop {
                let (stream, _) = ws.accept().await.unwrap();
                let peers = captured.clone();
                let closed = closed.clone();
                tokio::task::spawn_local(async move {
                    let path = Rc::new(RefCell::new(String::new()));
                    let capture = path.clone();
                    let socket = tokio_tungstenite::accept_hdr_async(stream, move |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                        *capture.borrow_mut() = request.uri().to_string();
                        Ok(response)
                    }).await.unwrap();
                    let old_private = path.borrow().contains("listenKey=bingx-native-key");
                    let (mut sink, mut source) = socket.split();
                    let (push, mut outgoing) = mpsc::unbounded_channel::<Message>();
                    peers
                        .borrow_mut()
                        .push((path.borrow().clone(), push.clone()));
                    tokio::task::spawn_local(async move {
                        while let Some(message) = outgoing.recv().await {
                            if message.is_close() {
                                sink.flush()
                                    .await
                                    .expect("flush the real Close acknowledgement");
                                if old_private {
                                    closed.set(true);
                                }
                                break;
                            }
                            sink.send(message)
                                .await
                                .expect("send the real private frame");
                        }
                    });
                    while let Some(message) = source.next().await {
                        let message = message.expect("read the real native socket frame");
                        if message.is_close() {
                            push.send(Message::Close(None)).unwrap();
                            break;
                        }
                    }
                });
            }
        });
        let gate = Arc::new(ListenKeyGate::default());
        let control = gate.clone();
        let adapters = Rc::new(move || {
            let inner = safe_cex_transport::native::reqwest_adapter().map_err(|error| {
                safe_cex_exchange::runtime::ExchangeError::new(error.to_string())
            })?;
            Ok(Arc::new(HeldListenKeyAdapter {
                inner,
                origin: http_origin.clone(),
                gate: control.clone(),
            }) as Arc<dyn safe_cex_transport::RestAdapter>)
        });
        let runtime = Rc::new(LocalDial {
            inner: safe_cex_exchange::native::native_runtime(),
            origin: ws_origin,
            reports: Default::default(),
        });
        let host = native_venue_host_with(runtime, adapters, hyperliquid_rate_limiter());
        let store = Rc::new(safe_cex_store::store::DefaultStore::new());
        let exchange = rust_constructor(&ExchangeName::Bingx, host)
            .unwrap()
            .construct(
                ExchangeName::Bingx,
                serde_json::from_value(json!({"key":"native-key","secret":SECRET})).unwrap(),
                store,
            )
            .unwrap();
        exchange.start().await.unwrap();
        eventually("initial native private socket", || {
            peers
                .borrow()
                .iter()
                .any(|(path, _)| path.contains("listenKey=bingx-native-key"))
        })
        .await;
        let old = peers
            .borrow()
            .iter()
            .find(|(path, _)| path.contains("listenKey=bingx-native-key"))
            .unwrap()
            .1
            .clone();
        old.send(Message::text(json!({"e":"listenKeyExpired"}).to_string()))
            .unwrap();
        let held_and_closed = tokio::time::timeout(Duration::from_secs(5), async {
            while !gate.entered.load(Ordering::SeqCst) || !close_written.get() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await;
        assert!(
            held_and_closed.is_ok(),
            "held={}, close={}, calls={}, peers={:?}, seen={:?}",
            gate.entered.load(Ordering::SeqCst),
            close_written.get(),
            gate.calls.load(Ordering::SeqCst),
            peers
                .borrow()
                .iter()
                .map(|(path, _)| path)
                .collect::<Vec<_>>(),
            seen.borrow()
        );
        tokio::time::sleep(Duration::from_millis(3100)).await;
        assert_eq!(
            gate.calls.load(Ordering::SeqCst),
            2,
            "no duplicate listen-key request while the real response is held"
        );
        gate.release.notify_one();
        eventually(
            "replacement native private socket after its IO wake",
            || {
                peers
                    .borrow()
                    .iter()
                    .any(|(path, _)| path.contains("listenKey=fresh-key"))
            },
        )
        .await;
        assert_eq!(gate.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            seen.borrow()
                .iter()
                .filter(|request| request.method == "POST"
                    && request.path == "/openApi/user/auth/userDataStream")
                .count(),
            2
        );
        assert_eq!(
            peers
                .borrow()
                .iter()
                .filter(|(path, _)| path.contains("listenKey="))
                .count(),
            2,
            "one replacement physical private socket"
        );
        exchange.dispose().unwrap();
    });
}
#[test]
fn bingx_native_signed_get_query_keeps_the_source_qs_sort() {
    run_local(async {
        let h = venue_harness(
            ExchangeName::Bingx,
            Rc::new(routes),
            json!({"key":"native-key","secret":SECRET,"tealstreetTunnel":false}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        h.exchange.get_account().await.expect("signed account read");
        let seen = h.seen.borrow();
        let request = seen
            .iter()
            .find(|r| r.path == "/openApi/swap/v3/user/balance")
            .unwrap();
        let keys = request
            .query
            .split('&')
            .map(|pair| pair.split_once('=').unwrap().0)
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            ["signature", "timestamp"],
            "source qs locale sort includes signature at the final wire serializer"
        );
        use hmac::{Hmac, Mac};
        let timestamp = query_value(&request.query, "timestamp").unwrap();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(format!("timestamp={timestamp}").as_bytes());
        let signature = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(query_value(&request.query, "signature").unwrap(), signature);
        drop(seen);
        h.exchange.dispose().unwrap();
    });
}

#[test]
fn bingx_native_registry_bootstraps_signs_places_and_cancels() {
    run_local(async {
        let h = venue_harness(
            ExchangeName::Bingx,
            Rc::new(routes),
            json!({"key":"native-key","secret":SECRET,"tealstreetTunnel":false}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        h.exchange.start().await.expect("BingX native start");
        let loaded = h.store.loaded().clone();
        assert!(
            loaded.markets && loaded.tickers && loaded.balance && loaded.orders && loaded.positions
        );
        eventually("BingX listen-key socket", || {
            h.peers
                .borrow()
                .iter()
                .any(|p| p.path.contains("bingx-native-key"))
        })
        .await;
        assert_eq!(
            h.exchange.get_account().await.unwrap().user_id,
            "native-bingx"
        );
        let opts:PlaceOrderOpts=serde_json::from_value(json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100,"clientOrderId":"tsnativecaller"})).unwrap();
        let ids = h
            .exchange
            .base()
            .place_order(h.exchange.as_ref(), opts)
            .await
            .unwrap();
        assert_eq!(ids, ["9007199254740993"]);
        let orders = h.store.orders().to_vec();
        assert_eq!(orders.len(), 1);
        h.exchange.cancel_orders(orders).await.unwrap();
        let seen = h.seen.borrow();
        let placed = seen
            .iter()
            .find(|r| r.method == "POST" && r.path == "/openApi/swap/v2/trade/order")
            .unwrap();
        let mut body: Value = serde_json::from_str(&placed.body).unwrap();
        let signature = body
            .as_object_mut()
            .unwrap()
            .remove("signature")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        let query = body
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                format!(
                    "{k}={}",
                    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(query.as_bytes());
        let expected = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(signature, expected, "native HMAC matches the wire payload");
        assert_eq!(body["clientOrderId"], "tsnativecaller");
        assert_eq!(
            placed.headers.get("x-source-key").map(String::as_str),
            Some("Tealstreet")
        );
        assert!(seen.iter().any(|r| r.method == "DELETE"
            && r.path == "/openApi/swap/v2/trade/order"
            && r.query.contains("9007199254740993")));
        drop(seen);
        h.exchange.dispose().unwrap();
    });
}

#[derive(Default)]
struct HostedBingx {
    calls: std::cell::RefCell<Vec<Value>>,
    reject: std::cell::Cell<bool>,
}
fn hosted_bingx_mac(text: &str) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(text.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn hosted_bingx_query(data: &Value) -> String {
    // The fixture has only flat ASCII keys/unreserved values; source qs sorts
    // independently of JSON object order and JS numbers omit integral .0.
    data.as_object()
        .unwrap()
        .iter()
        .collect::<std::collections::BTreeMap<_, _>>()
        .into_iter()
        .map(|(k, v)| {
            format!(
                "{k}={}",
                v.as_str().map_or_else(
                    || v.as_f64()
                        .map_or_else(|| v.to_string(), |number| number.to_string()),
                    str::to_owned
                )
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}
impl safe_cex_transport::signing::RemoteSigner for HostedBingx {
    fn sign(
        &self,
        request: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> safe_cex_transport::TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse>
    {
        use safe_cex_transport::signing::{
            HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig,
        };
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let RemoteSignerRequest::Http(request) = request else {
            panic!("BingX authenticates its private socket through the HTTP listen-key request")
        };
        assert_eq!(request.exchange_name, ExchangeName::Bingx);
        assert_eq!(request.account_name.as_deref(), Some("hosted-bingx"));
        let response = if self.reject.get() {
            HttpSigningResponse {
                id: request.id,
                error: Some("fixture signer unavailable".into()),
                config: None,
            }
        } else {
            let config = request.config;
            let mut headers = config.headers;
            headers.insert("X-BX-APIKEY".into(), "native-key".into());
            let mut data = config
                .data
                .clone()
                .map(|value| value.into_value().unwrap())
                .or(config.params.clone())
                .unwrap_or_else(|| json!({}));
            if let Some(text) = data.as_str() {
                data = serde_json::from_str(text).unwrap();
            }
            if data.get("signature").is_some() {
                return Box::pin(async {
                    Err(safe_cex_transport::TransportError::Host(
                        "hosted capability bypasses local API signing".into(),
                    ))
                });
            }
            let mut sorted = std::collections::BTreeMap::<String, Value>::new();
            sorted.extend(
                data.as_object()
                    .unwrap()
                    .iter()
                    .filter(|(k, _)| k.as_str() != "signature")
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
            let listen_key = config.url.ends_with("/openApi/user/auth/userDataStream");
            if !listen_key {
                sorted.insert(
                    "timestamp".into(),
                    json!(1_700_000_000_000u64 + self.calls.borrow().len() as u64),
                );
            }
            let mut payload = serde_json::to_value(sorted).unwrap();
            if !listen_key {
                let signature = hosted_bingx_mac(&hosted_bingx_query(&payload));
                payload
                    .as_object_mut()
                    .unwrap()
                    .insert("signature".into(), json!(signature));
            }
            let mut signed = SignedConfig {
                headers,
                ..Default::default()
            };
            if listen_key {
                signed.params = config.params;
                signed.data = config.data;
            } else if config.method.eq_ignore_ascii_case("post") {
                for field in ["quantity", "price"] {
                    if let Some(text) = payload[field].as_str() {
                        payload[field] = json!(text.parse::<f64>().unwrap());
                    }
                }
                signed
                    .headers
                    .insert("Content-Type".into(), "application/json".into());
                signed.data = Some(safe_cex_types::retained_json::RetainedJsonInput::Text(
                    serde_json::to_string(&payload)
                        .unwrap()
                        .encode_utf16()
                        .collect(),
                ));
            } else {
                signed.params = Some(payload);
            }
            HttpSigningResponse {
                id: request.id,
                config: Some(signed),
                error: None,
            }
        };
        Box::pin(async move { Ok(RemoteSignerResponse::Http(response)) })
    }
}
impl safe_cex_exchange::runtime::RemoteSigner for HostedBingx {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> safe_cex_exchange::runtime::Cleanup {
        Box::new(|| {})
    }
}
#[test]
fn bingx_native_hosted_signer_keeps_secret_remote() {
    run_local(async {
        for server_secret in ["", "must-not-sign-locally"] {
            let signer = Rc::new(HostedBingx::default());
            let h = venue_harness(
                ExchangeName::Bingx,
                Rc::new(routes),
                json!({"key":"native-key","secret":server_secret,"tealstreetTunnel":false}),
                Some((signer.clone(), Some("hosted-bingx".into()))),
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            assert!(
                h.exchange.can_sign(),
                "hosted signer participates in BaseExchange canSign"
            );
            assert_eq!(h.exchange.base().options().secret, server_secret);
            h.exchange
                .start()
                .await
                .expect("BingX starts with hosted signing");
            assert!(h.store.loaded().orders && h.store.loaded().positions);
            eventually("hosted BingX listen-key socket", || {
                h.peers
                    .borrow()
                    .iter()
                    .any(|p| p.path.contains("bingx-native-key"))
            })
            .await;
            let opts: PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1.2345,"price":100.127,"clientOrderId":"tshostedcaller"})).unwrap();
            assert_eq!(
                h.exchange
                    .base()
                    .place_order(h.exchange.as_ref(), opts)
                    .await
                    .unwrap(),
                ["9007199254740993"]
            );
            let orders = h.store.orders().to_vec();
            h.exchange.cancel_orders(orders).await.unwrap();
            {
                let seen = h.seen.borrow();
                let placed = seen
                    .iter()
                    .find(|r| r.method == "POST" && r.path == "/openApi/swap/v2/trade/order")
                    .unwrap();
                let mut body: Value = serde_json::from_str(&placed.body).unwrap();
                let signature = body.as_object_mut().unwrap().remove("signature").unwrap();
                // This verifies bytes from the network, independently of remote-channel metadata.
                assert_eq!(
                    signature,
                    hosted_bingx_mac(&hosted_bingx_query(&body)),
                    "remote HMAC authenticates transmitted order fields: {}",
                    hosted_bingx_query(&body)
                );
                assert_eq!(body["clientOrderId"], "tshostedcaller");
                assert_eq!(body["quantity"].as_f64().unwrap(), 1.235);
                assert!(
                    seen.iter()
                        .any(|r| r.method == "DELETE" && r.query.contains("9007199254740993"))
                );
            }
            let calls = signer.calls.borrow();
            assert!(calls.iter().any(|r| {
                r["config"]["url"]
                    .as_str()
                    .is_some_and(|u| u.ends_with("/openApi/swap/v2/trade/order"))
            }));
            assert!(
                calls.iter().all(|r| !r.to_string().contains(SECRET)),
                "the exchange never receives the browser secret"
            );
            drop(calls);
            h.exchange.dispose().unwrap();
        }
        let signer = Rc::new(HostedBingx::default());
        signer.reject.set(true);
        let h = venue_harness(
            ExchangeName::Bingx,
            Rc::new(routes),
            json!({"key":"native-key","secret":"","tealstreetTunnel":false}),
            Some((signer, Some("hosted-bingx".into()))),
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        let error = h.exchange.get_account().await.unwrap_err();
        assert!(
            error.message.contains("fixture signer unavailable"),
            "{error:?}"
        );
        assert!(
            h.seen.borrow().is_empty(),
            "signer refusal sends no unsigned request"
        );
        h.exchange.dispose().unwrap();
    });
}
