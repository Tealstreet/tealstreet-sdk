//! Bound Bybit family through production registry, signed API factory and local HTTP.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use safe_cex_exchange::{
    js_runtime::value::Js,
    runtime::{Cleanup, RemoteSigner},
};
use safe_cex_transport::{
    TransportFuture,
    signing::{HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig},
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn signature(payload: &str) -> String {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
    mac.update(payload.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn bybit_ret_msg_keeps_utf16_through_native_http_and_exchange_error() {
    use safe_cex_exchange::js_runtime::value::Thrown;
    use safe_cex_types::retained_json::RetainedJsonInput as Input;

    support::run_local(async {
        for name in [ExchangeName::Bybit, ExchangeName::Bybitdemo] {
            for (encoded, expected) in [
                (r#"\ud800"#, vec![0xd800]),
                (r#"\udc00"#, vec![0xdc00]),
                (r#"\ud800x\udc00"#, vec![0xd800, 120, 0xdc00]),
                (
                    "ordinary rejection",
                    "ordinary rejection".encode_utf16().collect(),
                ),
            ] {
                let body = format!(
                    "{{\"retCode\":10001,\"retMsg\":\"{encoded}\",\"result\":{{\"list\":[]}}}}"
                );
                let h = support::venue_harness(
                    name.clone(),
                    Rc::new(move |request| match request.path.as_str() {
                        "/v5/account/fee-rate" => Some(json!({
                            "retCode":0,"retMsg":"OK","result":{"list":[]}
                        })),
                        "/v5/market/instruments-info" => Some(json!({"__rawBody":body})),
                        _ => None,
                    }),
                    json!({"key":"native-bybit-key","secret":support::SECRET}),
                    None,
                    safe_cex_exchange::native::native_runtime(),
                )
                .await;
                let error = h.exchange.fetch_markets().await.unwrap_err();
                let value = Thrown::from(error).value;
                assert_eq!(
                    value
                        .get("message")
                        .unwrap()
                        .to_js_string()
                        .unwrap()
                        .as_ref(),
                    expected.as_slice(),
                    "Bybit ret_msg UTF-16 survives the actual HTTP client and typed error: {name:?}/{encoded}"
                );
                assert_eq!(value.get("name").unwrap().to_display(), "Error");
                assert!(
                    h.reports.borrow().iter().any(|context| {
                        context
                            .get("extra")
                            .and_then(|extra| extra.get("originalMessage"))
                            .and_then(|message| message.as_text())
                            .is_some_and(|units| units.ends_with(&expected))
                    }),
                    "the real native reporter receives original UTF-16 context"
                );
                let seen = h.seen.borrow();
                let request = seen
                    .iter()
                    .find(|request| request.path == "/v5/market/instruments-info")
                    .expect("actual market HTTP request");
                assert!(request.query.contains("requestId="));
                let payload = format!(
                    "{}{}{}{}",
                    request.headers["x-bapi-timestamp"],
                    request.headers["x-bapi-api-key"],
                    request.headers["x-bapi-recv-window"],
                    request.query
                );
                assert_eq!(request.headers["x-bapi-sign"], signature(&payload));
                drop(seen);
                let extra = Input::Object(vec![
                    (
                        "retMsg".encode_utf16().collect(),
                        Input::Text(expected.clone()),
                    ),
                    ("missing".encode_utf16().collect(), Input::Undefined),
                    ("nan".encode_utf16().collect(), Input::Float(f64::NAN)),
                    ("zero".encode_utf16().collect(), Input::Float(-0.)),
                ]);
                h.exchange
                    .base()
                    .emit_and_log_error(
                        safe_cex_exchange::error_reporting::ExchangeErrorInput::Raw(Js::str(
                            "retained context",
                        )),
                        None,
                        Some(&extra),
                    )
                    .unwrap();
                let reports = h.reports.borrow();
                let report = reports.last().unwrap().get("extra").unwrap();
                assert_eq!(
                    report.get("retMsg").unwrap().as_text(),
                    Some(expected.as_slice())
                );
                assert!(matches!(report.get("missing"), Some(Input::Undefined)));
                assert!(report.get("nan").unwrap().as_f64().unwrap().is_nan());
                assert_eq!(
                    report.get("zero").unwrap().as_f64().unwrap().to_bits(),
                    (-0f64).to_bits()
                );
                drop(reports);
                h.exchange.dispose().unwrap();
            }
        }
    });
}

#[test]
fn bybit_ticker_socket_preserves_coercion_type_errors_through_native_registry() {
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    use safe_cex_transport::socket::SocketFrame;

    support::run_local(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitspot,
            ExchangeName::Bybitinverse,
        ] {
            let h = support::venue_harness(
                name.clone(),
                Rc::new(account),
                json!({"key":"native-bybit-key","secret":support::SECRET}),
                None,
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            let slot = if name == ExchangeName::Bybitinverse {
                SocketSlot::PublicSecondary
            } else {
                SocketSlot::Public
            };
            let socket = h.exchange.base().get_socket(slot).unwrap();
            let error = socket.on_message(&SocketFrame::Text(
                r#"{"topic":"tickers.BTCUSDT","data":{"symbol":{"toString":null},"lastPrice":"42"}}"#.into(),
            )).unwrap_err();
            assert_eq!(
                error.name, "TypeError",
                "native socket retains coercion error class: {name:?}"
            );
            assert_eq!(error.message, "Cannot convert object to primitive value");
            assert!(
                h.store.tickers().unwrap().is_empty(),
                "coercion fails before ticker mutation"
            );
            h.exchange.dispose().unwrap();
        }
    });
}

#[test]
fn bybit_ticker_socket_preserves_subscriber_error_through_native_registry() {
    use safe_cex_exchange::{runtime::ExchangeError, socket_lifecycle::SocketSlot};
    use safe_cex_store::StoreUpdate;
    use safe_cex_transport::socket::SocketFrame;
    use std::cell::Cell;

    support::run_local(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitspot,
            ExchangeName::Bybitinverse,
        ] {
            let h = support::venue_harness(
                name.clone(),
                Rc::new(account),
                json!({"key":"native-bybit-key","secret":support::SECRET}),
                None,
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            h.store
                .update(
                    StoreUpdate {
                        tickers: Some(vec![
                            serde_json::from_value(json!({
                                "id":"BTCUSDT","symbol":"BTCUSDT","bid":1,"ask":1,"last":1,
                                "mark":1,"index":1,"percentage":0,"openInterest":0,
                                "fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0
                            }))
                            .unwrap(),
                        ]),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            let mut original = ExchangeError::new("ticker subscriber rejected");
            original.name = "TickerSubscriberError".into();
            original.response = Some(json!({"status":418,"data":{"reason":"listener"}}));
            original.store_subscriber_ids = vec![71];
            let cause: safe_cex_types::StoreSubscriberError = original.clone().into();
            let thrown = cause.clone();
            let first = h.store.subscribe(Box::new(move |data, _| {
                if data.tickers[0].last == 42. {
                    Err(thrown.clone())
                } else {
                    Ok(())
                }
            }));
            let later = Rc::new(Cell::new(0));
            let observed = later.clone();
            let second = h.store.subscribe(Box::new(move |data, _| {
                if data.tickers[0].last == 42. {
                    observed.set(observed.get() + 1);
                }
                Ok(())
            }));
            let slot = if name == ExchangeName::Bybitinverse {
                SocketSlot::PublicSecondary
            } else {
                SocketSlot::Public
            };
            let socket = h.exchange.base().get_socket(slot).unwrap();
            let error = socket
                .on_message(&SocketFrame::Text(
                    r#"{"topic":"tickers.BTCUSDT","data":{"symbol":"BTCUSDT","lastPrice":"42"}}"#
                        .into(),
                ))
                .unwrap_err();
            assert_eq!(
                h.store.tickers().unwrap()[0].last,
                42.,
                "mutation commits before rejection: {name:?}"
            );
            assert_eq!(later.get(), 0, "the throwing listener stops fanout");
            assert_eq!(
                error.name, original.name,
                "native socket retains error name: {name:?}"
            );
            assert_eq!(error.message, original.message);
            assert_eq!(error.response, original.response);
            assert_eq!(error.store_subscriber_cause, Some(cause));
            assert_eq!(error.store_subscriber_ids.len(), 2);
            assert_eq!(error.store_subscriber_ids[0], 71);
            first();
            second();
            h.exchange.dispose().unwrap();
        }
    });
}

#[test]
fn bybit_candle_string_tuples_reach_native_registry_readers() {
    support::run_local(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitspot,
            ExchangeName::Bybitinverse,
        ] {
            let h = support::venue_harness(
                name.clone(),
                Rc::new(|request| {
                    (request.path == "/v5/market/kline").then(|| {
                        json!({
                        "retCode":0,"retMsg":"OK","result":{"list":["234567","123456suffix"]}
                        })
                    })
                }),
                json!({"key":"native-bybit-key","secret":support::SECRET}),
                None,
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            let candles = h
                .exchange
                .fetch_ohlcv(
                    serde_json::from_value(json!({
                        "symbol":"BTCUSDT","interval":"1m","from":1000,"to":2000,"limit":2
                    }))
                    .unwrap(),
                )
                .await
                .unwrap_or_else(|e| panic!("{name:?}: iterable KLINE rows must resolve: {e:?}"));
            assert_eq!(candles.len(), 2);
            assert_eq!(
                (
                    candles[0].timestamp,
                    candles[0].open,
                    candles[0].high,
                    candles[0].low,
                    candles[0].close,
                    candles[0].volume
                ),
                (0.001, 2., 3., 4., 5., 6.)
            );
            assert_eq!(
                (
                    candles[1].timestamp,
                    candles[1].open,
                    candles[1].high,
                    candles[1].low,
                    candles[1].close,
                    candles[1].volume
                ),
                (0.002, 3., 4., 5., 6., 7.)
            );
            let seen = h.seen.borrow();
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0].path, "/v5/market/kline");
            assert!(
                !seen[0].headers.contains_key("x-bapi-sign"),
                "KLINE remains unsigned"
            );
            h.exchange.dispose().unwrap();
        }
    });
}

fn account(request: &support::Seen) -> Option<Value> {
    (request.path == "/v5/user/query-api").then(|| {
        json!({
            "retCode":0,"retMsg":"OK","result":{"userID":12345,"inviterID":678,"vipLevel":"No VIP"}
        })
    })
}

fn assert_signed(request: &support::Seen) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.headers["x-bapi-api-key"], "native-bybit-key");
    let payload = format!(
        "{}{}{}{}",
        request.headers["x-bapi-timestamp"],
        request.headers["x-bapi-api-key"],
        request.headers["x-bapi-recv-window"],
        request.query
    );
    assert_eq!(
        request.headers["x-bapi-sign"],
        signature(&payload),
        "native server verifies exact Bybit signed bytes"
    );
}

#[test]
fn bybit_and_demo_construct_and_sign_through_native_registry() {
    support::run_local(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitspot,
            ExchangeName::Bybitinverse,
        ] {
            let h = support::venue_harness(
                name.clone(),
                Rc::new(account),
                json!({"key":"native-bybit-key","secret":support::SECRET}),
                None,
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            assert_eq!(h.exchange.base().name(), &name);
            if name == ExchangeName::Bybitinverse {
                assert_eq!(h.exchange.class_name(), "BybitInverseExchange");
                assert!(!h.exchange.base().is_spot && !h.exchange.base().is_demo);
            }
            let account =
                tokio::time::timeout(std::time::Duration::from_secs(10), h.exchange.get_account())
                    .await
                    .expect("Bybit native account read completes")
                    .unwrap();
            assert_eq!(account.user_id, "12345");
            assert_eq!(account.affiliate_id, Some(Some("678".into())));
            let seen = h.seen.borrow();
            assert_eq!(seen.len(), 1);
            assert_signed(&seen[0]);
            h.exchange.dispose().unwrap();
        }
    });
}

#[test]
fn bybit_response_error_listener_rejection_reaches_native_registry() {
    support::run_local(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitspot,
            ExchangeName::Bybitinverse,
        ] {
            let h = support::venue_harness(
                name.clone(),
                Rc::new(|request| {
                    (request.path == "/v5/user/query-api").then(|| json!({
                        "retCode":10016,"retMsg":"service unavailable",
                        "raw_response":"There might be too much traffic or a configuration error",
                        "result":{"userID":12345}
                    }))
                }),
                json!({"key":"native-bybit-key","secret":support::SECRET}),
                None,
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            let calls = Rc::new(std::cell::Cell::new(0));
            let observed = calls.clone();
            let cleanup = h.exchange.base().on(
                "error",
                Rc::new(move |_, _| {
                    observed.set(observed.get() + 1);
                    let mut error = safe_cex_exchange::runtime::ExchangeError::new(
                        "subscriber refused diagnostic",
                    );
                    error.name = "DiagnosticSubscriberError".into();
                    error.code = Some(Js::object(vec![
                        ("retry", Js::Bool(false)),
                        ("sentinel", Js::Number(f64::NAN)),
                        ("ownUndefined", Js::Undefined),
                    ]));
                    Err(error)
                }),
            );
            let error =
                tokio::time::timeout(std::time::Duration::from_secs(10), h.exchange.get_account())
                    .await
                    .expect("native account rejection completes")
                    .unwrap_err();
            assert_eq!(
                calls.get(),
                1,
                "{name:?}: real error subscriber was reached"
            );
            assert_eq!(h.seen.borrow().len(), 1, "callback refusal stops retry");
            assert_signed(&h.seen.borrow()[0]);
            assert_eq!(
                error.name, "DiagnosticSubscriberError",
                "{name:?}: preserve callback error name"
            );
            assert_eq!(error.message, "subscriber refused diagnostic");
            let code = error.code.as_ref().expect("retained callback code");
            assert!(code.get("sentinel").unwrap().to_number().unwrap().is_nan());
            assert!(code.has_own("ownUndefined"));
            assert!(code.get("ownUndefined").unwrap().is_undefined());
            assert!(matches!(code.get("retry").unwrap(), Js::Bool(false)));
            h.exchange.base().off("error", Some(&cleanup));
            h.exchange.dispose().unwrap();
        }
    });
}

#[derive(Default)]
struct HostedBybit {
    calls: RefCell<Vec<Value>>,
}
impl safe_cex_transport::signing::RemoteSigner for HostedBybit {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        self.calls
            .borrow_mut()
            .push(serde_json::to_value(&request).unwrap());
        let RemoteSignerRequest::Http(request) = request else {
            panic!("HTTP-only witness")
        };
        let mut headers = request.config.headers;
        headers.insert("X-BAPI-API-KEY".into(), "native-bybit-key".into());
        headers.insert("X-BAPI-TIMESTAMP".into(), "1700000000000".into());
        headers.insert("X-BAPI-RECV-WINDOW".into(), "5000".into());
        headers.insert(
            "X-BAPI-SIGN".into(),
            signature(&format!(
                "1700000000000native-bybit-key5000requestId={}",
                request.config.params.as_ref().unwrap()["requestId"]
                    .as_str()
                    .unwrap()
            )),
        );
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
impl RemoteSigner for HostedBybit {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> Cleanup {
        Box::new(|| {})
    }
}

#[test]
fn bybit_and_demo_use_hosted_signer_without_local_credentials() {
    support::run_local(async {
        for name in [
            ExchangeName::Bybit,
            ExchangeName::Bybitdemo,
            ExchangeName::Bybitspot,
            ExchangeName::Bybitinverse,
        ] {
            let signer = Rc::new(HostedBybit::default());
            let h = support::venue_harness(
                name.clone(),
                Rc::new(account),
                json!({"key":"","secret":""}),
                Some((signer.clone(), Some("bybit-hosted-account".into()))),
                safe_cex_exchange::native::native_runtime(),
            )
            .await;
            assert!(h.exchange.base().options().key.is_empty());
            assert!(h.exchange.base().options().secret.is_empty());
            let result =
                tokio::time::timeout(std::time::Duration::from_secs(10), h.exchange.get_account())
                    .await
                    .expect("secretless Bybit account read completes")
                    .unwrap();
            assert_eq!(result.user_id, "12345");
            assert_eq!(
                signer.calls.borrow().len(),
                1,
                "source options.remoteSigner reaches shared signing adapter"
            );
            let call = &signer.calls.borrow()[0];
            assert_eq!(call["exchangeName"], serde_json::to_value(name).unwrap());
            assert_eq!(call["accountName"], "bybit-hosted-account");
            assert_signed(&h.seen.borrow()[0]);
            h.exchange.dispose().unwrap();
        }
    });
}

/// Records the production terminal request without resolving or rewriting its origin.
#[derive(Default)]
struct OriginWire {
    seen: RefCell<Vec<safe_cex_transport::RestRequest>>,
}
impl safe_cex_transport::RestAdapter for OriginWire {
    fn execute(
        &self,
        request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        self.seen.borrow_mut().push(request);
        Box::pin(async {
            Ok(safe_cex_transport::RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    json!({"retCode":0,"retMsg":"OK","result":{"userID":12345}})
                        .to_string()
                        .into_bytes(),
                ),
            })
        })
    }
}

#[test]
#[allow(
    clippy::arc_with_non_send_sync,
    reason = "native adapters run on the LocalSet"
)]
fn registry_selects_demo_origin_before_any_transport_rewrite() {
    support::run_local(async {
        for (name, testnet, expected) in [
            (ExchangeName::Bybit, false, "https://api.bybit.com"),
            (ExchangeName::Bybitdemo, false, "https://api-demo.bybit.com"),
            (ExchangeName::Bybit, true, "https://api-testnet.bybit.com"),
            (
                ExchangeName::Bybitdemo,
                true,
                "https://api-testnet.bybit.com",
            ),
        ] {
            let wire = std::sync::Arc::new(OriginWire::default());
            let terminal = wire.clone();
            let host = safe_cex_registry::native_host::native_venue_host_with(
                safe_cex_exchange::native::native_runtime(),
                Rc::new(move || Ok(terminal.clone())),
                safe_cex_registry::native_host::hyperliquid_rate_limiter(),
            );
            let constructor =
                safe_cex_registry::rust_venues::rust_constructor(&name, host).unwrap();
            let exchange = constructor
                .construct(
                    name.clone(),
                    serde_json::from_value(json!({
                        "key":"origin-key", "secret":"origin-secret", "testnet":testnet
                    }))
                    .unwrap(),
                    Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
                )
                .unwrap();
            assert_eq!(exchange.get_account().await.unwrap().user_id, "12345");
            let seen = wire.seen.borrow();
            assert_eq!(seen.len(), 1);
            assert_eq!(
                seen[0].base_url.as_deref(),
                Some(expected),
                "untouched {name:?} testnet={testnet} REST origin"
            );
            assert!(seen[0].url.starts_with("/v5/user/query-api?requestId="));
            exchange.dispose().unwrap();
        }
    });
}
