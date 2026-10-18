//! BingX registry construction and signed lifecycle over real native local HTTP/WS.
#![cfg(not(target_arch = "wasm32"))]
mod support;
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

#[test]
fn bingx_native_registry_signs_nan_leverage_before_json_projection() {
    run_local(async {
        const ENDPOINT: &str = "/openApi/swap/v2/trade/leverage";
        let h = venue_harness(
            ExchangeName::Bingx,
            Rc::new(|request| {
                if request.method == "POST" && request.path == ENDPOINT {
                    Some(json!({"code":0,"data":{}}))
                } else {
                    routes(request)
                }
            }),
            json!({"key":"native-key","secret":SECRET,"tealstreetTunnel":false}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        h.exchange.start().await.expect("BingX native start");
        let result = h
            .exchange
            .set_leverage("BTCUSDT".into(), f64::NAN, None)
            .await;
        let calls = h
            .seen
            .borrow()
            .iter()
            .filter(|request| request.method == "POST" && request.path == ENDPOINT)
            .map(|request| request.body.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            calls.len(),
            1,
            "registry NaN leverage reaches HTTP: {result:?}"
        );
        result.expect("scripted venue accepts source NaN leverage request");
        let body: Value = serde_json::from_str(&calls[0]).unwrap();
        assert_eq!(body.as_object().unwrap().len(), 5);
        assert_eq!(body["leverage"], Value::Null);
        assert_eq!(body["side"], "BOTH");
        assert_eq!(body["symbol"], "BTC-USDT");
        let timestamp = body["timestamp"].as_f64().unwrap();
        let preimage = format!("leverage=NaN&side=BOTH&symbol=BTC-USDT&timestamp={timestamp}");
        assert_eq!(body["signature"], hosted_bingx_mac(&preimage));
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
