//! tealstreetTunnel on the production native host: venue REST uses HOW
//! tunnel (rpc.http frames) to a local tradetunnel, authenticated with the account's
//! proxy token, and never reaches the venue origin directly.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use safe_cex_exchange::native::native_tealstreet_runtime_with;
use safe_cex_transport::{
    TransportFuture,
    native::NativeTunnelHost,
    socket::{SocketCapabilities, TunnelDialOptions, TunnelHost, TunnelSocket, TunnelTask},
};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::HashMap, future::Future, pin::Pin, rc::Rc};
use support::*;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

/// An unexpired proxy JWT (exp 2100-01-01); the tunnel checks expiry, not the signature.
const TOKEN: &str =
    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhY2N0LTEiLCJleHAiOjQxMDI0NDQ4MDB9.c2ln";

/// Production tunnel dials, pointed at the local tradetunnel.
struct LocalTunnel {
    inner: NativeTunnelHost,
    origin: String,
}

async fn source_factory_tunnel(name: safe_cex_types::ExchangeName, mut options: Value) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("ws://{}", listener.local_addr().unwrap());
    let tunnel = Rc::new(Tunnel::default());
    tokio::task::spawn_local(serve_tunnel(listener, Rc::new(|_| None), tunnel.clone()));
    let runtime = native_tealstreet_runtime_with(
        Rc::new(|| Box::pin(async { Ok(TOKEN.to_owned()) })),
        Rc::new(LocalTunnel {
            inner: NativeTunnelHost,
            origin,
        }),
    );
    options["tealstreetTunnel"] = json!(true);
    let h = venue_harness(
        name.clone(),
        Rc::new(|request| {
            request
                .path
                .ends_with("/time")
                .then(|| json!({"serverTime":1700000000000u64}))
        }),
        options,
        None,
        runtime,
    )
    .await;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        h.exchange.validate_account(),
    )
    .await
    .expect("tunneled account validation completes");
    assert!(
        !tunnel.seen.borrow().is_empty(),
        "{name:?} reached rpc.http: {result:?}"
    );
    assert!(
        h.seen.borrow().iter().all(|request| {
            name == safe_cex_types::ExchangeName::Asterdex && request.path.ends_with("/time")
        }),
        "{name:?} account request bypassed its tunnel: {:?}",
        h.seen.borrow()
    );
    assert!(
        !tunnel.tokens.borrow().is_empty(),
        "{name:?} authenticated its tunnel"
    );
    assert!(tunnel.tokens.borrow().iter().all(|token| token == TOKEN));
    assert!(
        tunnel
            .frames
            .borrow()
            .iter()
            .all(|frame| frame.get("format").is_none()),
        "{name:?} uses its source adapter variant"
    );
    assert_eq!(
        h.store.request_stats().unwrap().other,
        tunnel.seen.borrow().len() as f64,
        "{name:?} records every actual tunnel 404 on its owning Store"
    );
    h.exchange.dispose().unwrap();
}

macro_rules! factory_tunnel_case {
    ($test:ident, $name:ident, $options:expr) => {
        #[test]
        fn $test() {
            run_local(source_factory_tunnel(
                safe_cex_types::ExchangeName::$name,
                $options,
            ));
        }
    };
}
factory_tunnel_case!(
    asterdex_factory_tunnel,
    Asterdex,
    json!({"key":"native-key","secret":SECRET})
);
factory_tunnel_case!(
    orderly_factory_tunnel,
    Kodiakfi,
    json!({"key":"native-key","secret":"11111111111111111111111111111111","applicationId":"account-native"})
);
factory_tunnel_case!(
    woo_factory_tunnel,
    Woo,
    json!({"key":"native-key","secret":SECRET})
);
factory_tunnel_case!(
    breakoutprop_factory_tunnel,
    Breakoutprop,
    json!({"key":"k","secret":"p","applicationId":"default"})
);
impl TunnelHost for LocalTunnel {
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
        self.inner.spawn_local(future)
    }
    fn capabilities(&self) -> SocketCapabilities {
        self.inner.capabilities()
    }
    fn now_ms(&self) -> u64 {
        self.inner.now_ms()
    }
    fn sleep_until(&self, deadline_ms: u64) -> TransportFuture<'static, ()> {
        self.inner.sleep_until(deadline_ms)
    }
    fn connect(
        &self,
        url: String,
        options: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        self.inner.connect(rewrite(&url, &self.origin), options)
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        self.inner.proxy_token()
    }
    fn is_token_expired(&self, token: &str) -> bool {
        self.inner.is_token_expired(token)
    }
    fn sync_server_time(&self, server_time: u64) {
        self.inner.sync_server_time(server_time)
    }
}

fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
fn query_of(params: &serde_json::Map<String, Value>) -> String {
    params
        .iter()
        .map(|(key, value)| {
            let text = match value {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            format!("{}={}", encode(key), encode(&text))
        })
        .collect::<Vec<_>>()
        .join("&")
}

#[derive(Default)]
struct Tunnel {
    tokens: RefCell<Vec<String>>,
    frames: RefCell<Vec<Value>>,
    seen: RefCell<Vec<Seen>>,
}
/// A local tradetunnel: serverTime on connect, rpc.auth accepted, rpc.http answered
/// from the shared route table with the venue response as the raw or JSON body.
async fn serve_tunnel(listener: TcpListener, handler: Handler, tunnel: Rc<Tunnel>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let (handler, tunnel) = (handler.clone(), tunnel.clone());
        tokio::task::spawn_local(async move {
            let Ok(socket) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            let (mut sink, mut source) = socket.split();
            let time = json!({"type":"serverTime","serverTime":1_700_000_000_000u64});
            if sink.send(Message::text(time.to_string())).await.is_err() {
                return;
            }
            while let Some(Ok(message)) = source.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                let frame: Value = serde_json::from_str(&text).unwrap_or_default();
                let reply = match frame["type"].as_str() {
                    Some("rpc.auth") => {
                        tunnel
                            .tokens
                            .borrow_mut()
                            .push(frame["token"].as_str().unwrap_or_default().into());
                        json!({"type":"rpc.auth","status":"success"})
                    }
                    Some("rpc.http") => {
                        tunnel.frames.borrow_mut().push(frame.clone());
                        let arg = &frame["arg"];
                        let url = arg["url"].as_str().unwrap_or_default();
                        let path = url
                            .split_once("://")
                            .and_then(|(_, rest)| rest.find('/').map(|i| rest[i..].to_owned()))
                            .unwrap_or_default();
                        let request = Seen {
                            body: String::new(),
                            method: arg["method"].as_str().unwrap_or_default().into(),
                            path,
                            query: query_of(arg["params"].as_object().unwrap()),
                            headers: arg["headers"]
                                .as_object()
                                .unwrap()
                                .iter()
                                .map(|(k, v)| {
                                    (k.to_ascii_lowercase(), v.as_str().unwrap_or("").into())
                                })
                                .collect::<HashMap<_, _>>(),
                        };
                        let answer = handler(&request);
                        tunnel.seen.borrow_mut().push(request);
                        let (status, body) = match answer {
                            Some(body) => (200, body),
                            None => (404, json!({"code":-1,"msg":"no route"})),
                        };
                        let mut data = json!({"config":{"url":url},"status":status,
                            "status_text":"OK","headers":{"content-type":"application/json"}});
                        if frame["format"] == "raw" {
                            data["data_raw"] = json!(body.to_string());
                        } else {
                            data["data"] = body;
                        }
                        json!({"type":"rpc.http","nonce":frame["nonce"],"data":data})
                    }
                    _ => continue,
                };
                if sink.send(Message::text(reply.to_string())).await.is_err() {
                    return;
                }
            }
        });
    }
}

#[test]
fn binance_rest_runs_over_the_tealstreet_tunnel_with_the_proxy_token() {
    run_local(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}", listener.local_addr().unwrap());
        let tunnel = Rc::new(Tunnel::default());
        tokio::task::spawn_local(serve_tunnel(
            listener,
            handler(Rc::new(|_| None)),
            tunnel.clone(),
        ));
        let runtime = native_tealstreet_runtime_with(
            Rc::new(|| Box::pin(async { Ok(TOKEN.to_owned()) })),
            Rc::new(LocalTunnel {
                inner: NativeTunnelHost,
                origin,
            }),
        );
        let Harness { exchange, seen, .. } = binance_harness_over(
            Rc::new(|_| None),
            json!({"key":"native-key","secret":SECRET,"tealstreetTunnel":true}),
            None,
            runtime,
        )
        .await;

        exchange.start().await.expect("start over the HOW tunnel");
        let order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":10}),
        )
        .unwrap();
        let ids = exchange
            .base()
            .place_order(exchange.as_ref(), order)
            .await
            .expect("placeOrder over the HOW tunnel");
        assert_eq!(ids, vec!["9001"]);

        assert!(
            seen.borrow().is_empty(),
            "no REST request reached the venue origin directly: {:?}",
            seen.borrow()
        );
        let tunneled = tunnel.seen.borrow();
        for route in [
            "GET /fapi/v1/exchangeInfo",
            "POST /fapi/v1/listenKey",
            "POST /fapi/v1/order",
        ] {
            assert!(
                tunneled
                    .iter()
                    .any(|s| format!("{} {}", s.method, s.path) == route),
                "{route} went over the tunnel"
            );
        }
        assert!(!tunnel.tokens.borrow().is_empty());
        assert!(
            tunnel.tokens.borrow().iter().all(|t| t == TOKEN),
            "the tunnel authenticated with the account's proxy token"
        );
        assert!(
            tunnel.frames.borrow().iter().all(|f| f["format"] == "raw"),
            "futures clients take the bigInt (raw) tunnel adapter"
        );

        let placed = tunneled
            .iter()
            .rev()
            .find(|s| s.path == "/fapi/v1/order")
            .cloned()
            .unwrap();
        assert_eq!(
            placed.headers.get("x-mbx-apikey").map(String::as_str),
            Some("native-key")
        );
        let signature = query_value(&placed.query, "signature").expect("signed params");
        let signed = placed.query.split("&signature=").next().unwrap();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(signed.as_bytes());
        let expected: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            signature, expected,
            "tunnel-verified HMAC over the params sent"
        );
        exchange.dispose().expect("dispose");
    });
}

#[test]
fn phemex_rest_runs_over_the_tealstreet_tunnel_with_signed_account_query() {
    run_local(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}", listener.local_addr().unwrap());
        let tunnel = Rc::new(Tunnel::default());
        tokio::task::spawn_local(serve_tunnel(
            listener,
            Rc::new(|request| match request.path.as_str() {
                "/public/products" => Some(json!({"data":{"perpProductsV2":[{
                    "symbol":"BTCUSDT","type":"PerpetualV2","baseCurrency":"BTC",
                    "quoteCurrency":"USDT","settleCurrency":"USDT","qtyStepSize":"0.003",
                    "tickSize":"0.25","maxOrderQtyRq":"100","maxLeverage":"50"
                }]}})),
                "/g-accounts/accountPositions" => Some(json!({"code":0,"data":{
                    "account":{"userID":123,"accountBalanceRv":"1000","totalUsedBalanceRv":"200"},
                    "positions":[]
                }})),
                _ => None,
            }),
            tunnel.clone(),
        ));
        let runtime = native_tealstreet_runtime_with(
            Rc::new(|| Box::pin(async { Ok(TOKEN.to_owned()) })),
            Rc::new(LocalTunnel {
                inner: NativeTunnelHost,
                origin,
            }),
        );
        let h = venue_harness(
            safe_cex_types::ExchangeName::Phemex,
            Rc::new(|_| None),
            json!({"key":"native-key","secret":SECRET,"tealstreetTunnel":true}),
            None,
            runtime,
        )
        .await;
        let markets = h
            .exchange
            .fetch_markets()
            .await
            .expect("Phemex tunneled markets");
        assert_eq!(markets.len(), 1);
        assert_eq!(
            (markets[0].precision.amount, markets[0].precision.price),
            (0.003, 0.25)
        );
        assert!(
            h.store.markets().is_empty(),
            "fetchMarkets is pure over tunnel"
        );
        let account = h
            .exchange
            .get_account()
            .await
            .expect("Phemex tunneled account");
        assert_eq!(serde_json::to_value(account).unwrap()["userId"], "123");
        assert!(
            h.seen.borrow().is_empty(),
            "no Phemex request reaches direct venue origin"
        );
        assert!(!tunnel.tokens.borrow().is_empty());
        assert!(tunnel.tokens.borrow().iter().all(|token| token == TOKEN));
        let requests = tunnel.seen.borrow();
        assert_eq!(
            requests.len(),
            2,
            "both Phemex reads use actual rpc.http frames"
        );
        assert!(
            requests
                .iter()
                .any(|request| request.path == "/public/products")
        );
        let request = requests
            .iter()
            .find(|r| r.path == "/g-accounts/accountPositions")
            .unwrap();
        assert_eq!(request.method, "GET");
        assert_eq!(request.query, "currency=USDT");
        assert_eq!(request.headers["x-phemex-access-token"], "native-key");
        let expiry = &request.headers["x-phemex-request-expiry"];
        let payload = format!("/g-accounts/accountPositionscurrency=USDT{expiry}");
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        let signature: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(request.headers["x-phemex-request-signature"], signature);
        assert!(
            tunnel
                .frames
                .borrow()
                .iter()
                .all(|frame| frame.get("format").is_none()),
            "Phemex uses the source default adapter variant"
        );
        h.exchange.dispose().expect("dispose Phemex tunnel account");
    });
}

#[test]
fn okx_family_signed_rest_uses_the_authenticated_bigint_tunnel() {
    use base64::Engine;
    use safe_cex_types::ExchangeName;
    run_local(async {
        for name in [
            ExchangeName::Okx,
            ExchangeName::Okxeu,
            ExchangeName::Okxeudemo,
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("ws://{}", listener.local_addr().unwrap());
            let tunnel = Rc::new(Tunnel::default());
            tokio::task::spawn_local(serve_tunnel(
                listener,
                Rc::new(|request| match request.path.as_str() {
                    "/api/v5/account/account-position-risk" => Some(json!({"code":"0","data":[{
                        "balData":[{"ccy":"USDT","eq":"1000"}],"posData":[]
                    }]})),
                    "/api/v5/account/config" => {
                        Some(json!({"code":"0","data":[{"posMode":"net_mode","acctLv":"2"}]}))
                    }
                    _ => None,
                }),
                tunnel.clone(),
            ));
            let runtime = native_tealstreet_runtime_with(
                Rc::new(|| Box::pin(async { Ok(TOKEN.to_owned()) })),
                Rc::new(LocalTunnel {
                    inner: NativeTunnelHost,
                    origin,
                }),
            );
            let h = venue_harness(
                name.clone(),
                Rc::new(|_| None),
                json!({"key":"native-key","secret":SECRET,"passphrase":"native-passphrase",
                    "tealstreetProxy":true,"tealstreetTunnel":true}),
                None,
                runtime,
            )
            .await;
            let result = h
                .exchange
                .validate_account()
                .await
                .expect("signed tunneled account validation");
            assert_eq!(
                serde_json::to_value(result.error).unwrap(),
                json!(""),
                "{name:?} paths: {:?}",
                tunnel
                    .seen
                    .borrow()
                    .iter()
                    .map(|r| &r.path)
                    .collect::<Vec<_>>()
            );
            assert!(
                h.seen.borrow().is_empty(),
                "{name:?} must send no direct HTTP request"
            );
            assert_eq!(
                *tunnel.tokens.borrow(),
                [TOKEN],
                "tunnel authenticates with proxy token"
            );
            let requests = tunnel.seen.borrow();
            assert_eq!(
                requests.len(),
                1,
                "one private validation read for {name:?}"
            );
            assert_eq!(h.store.request_stats().unwrap().ok, requests.len() as f64);
            let request = &requests[0];
            assert_eq!(
                request.path,
                if name == ExchangeName::Okx {
                    "/api/v5/account/account-position-risk"
                } else {
                    "/api/v5/account/config"
                }
            );
            assert_eq!(request.headers["ok-access-key"], "native-key");
            assert_eq!(request.headers["ok-access-passphrase"], "native-passphrase");
            let timestamp = &request.headers["ok-access-timestamp"];
            let preimage = format!("{timestamp}GET{}", request.path);
            let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
            mac.update(preimage.as_bytes());
            let signature =
                base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
            assert_eq!(
                request.headers["ok-access-sign"], signature,
                "tunnel verifies signed bytes for {name:?}"
            );
            assert!(
                tunnel
                    .frames
                    .borrow()
                    .iter()
                    .all(|frame| frame["format"] == "raw"),
                "source bigInt transport uses raw responses"
            );
            drop(requests);
            h.exchange.dispose().unwrap();
        }
    });
}

struct DiagnosticSigner;
impl safe_cex_exchange::runtime::RemoteSigner for DiagnosticSigner {
    fn is_available(&self) -> bool {
        true
    }
    fn on_availability_change(&self, _: Rc<dyn Fn(bool)>) -> safe_cex_exchange::runtime::Cleanup {
        Box::new(|| {})
    }
}
impl safe_cex_transport::signing::RemoteSigner for DiagnosticSigner {
    fn sign(
        &self,
        request: safe_cex_transport::signing::RemoteSignerRequest,
    ) -> TransportFuture<'_, safe_cex_transport::signing::RemoteSignerResponse> {
        use safe_cex_transport::signing::{
            HttpSigningResponse, RemoteSignerRequest, RemoteSignerResponse, SignedConfig,
        };
        let RemoteSignerRequest::Http(request) = request else {
            panic!("HTTP-only witness");
        };
        Box::pin(async move {
            tokio::time::sleep(std::time::Duration::from_millis(3)).await;
            let mut headers = request.config.headers;
            headers.insert(
                "X-BAPI-TIMESTAMP".into(),
                (safe_cex_transport::native::wall_clock_ms() - 2000).to_string(),
            );
            Ok(RemoteSignerResponse::Http(HttpSigningResponse {
                id: request.id,
                error: None,
                config: Some(SignedConfig {
                    url: None,
                    params: request.config.params,
                    data: request.config.data,
                    headers,
                }),
            }))
        })
    }
}

async fn native_rest_diagnostics(name: safe_cex_types::ExchangeName) {
    use safe_cex_exchange::runtime::ExchangeRuntime;
    use safe_cex_store::venue_rest_diagnostics::{
        drain_venue_rest_failure_samples, get_venue_rest_stats, reset_venue_rest_diagnostics,
    };
    for (tunneled, hosted) in [(false, false), (true, false), (false, true), (true, true)] {
        reset_venue_rest_diagnostics();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("ws://{}", listener.local_addr().unwrap());
        let tunnel = Rc::new(Tunnel::default());
        tokio::task::spawn_local(serve_tunnel(listener, Rc::new(|_| None), tunnel.clone()));
        let runtime = native_tealstreet_runtime_with(
            Rc::new(|| Box::pin(async { Ok(TOKEN.to_owned()) })),
            Rc::new(LocalTunnel {
                inner: NativeTunnelHost,
                origin,
            }),
        );
        let h = venue_harness(
            name.clone(),
            Rc::new(|_| None),
            json!({"key":"native-key","secret":SECRET,"tealstreetTunnel":tunneled}),
            hosted.then(|| {
                (
                    Rc::new(DiagnosticSigner) as Rc<dyn safe_cex_exchange::runtime::RemoteSigner>,
                    Some("diagnostic-account".into()),
                )
            }),
            runtime.clone(),
        )
        .await;
        h.exchange.validate_account().await.unwrap();
        let key = serde_json::to_value(&name).unwrap();
        let stats = get_venue_rest_stats(runtime.now_ms());
        let stats = stats
            .get(key.as_str().unwrap())
            .expect("real native HTTP records venue diagnostics");
        let sent = if tunneled {
            tunnel.seen.borrow().len()
        } else {
            h.seen.borrow().len()
        };
        assert!(sent >= 1, "the venue really sent an HTTP request");
        assert_eq!(
            stats.requests, sent as f64,
            "one diagnostic per physical request"
        );
        assert_eq!(
            stats.sign_p50_ms.is_some(),
            hosted,
            "hosted signing timing survives the adapter receipt"
        );
        assert_eq!(stats.ok, if tunneled { sent as f64 } else { 0.0 });
        assert_eq!(stats.failures, if tunneled { 0.0 } else { sent as f64 });
        assert_eq!(stats.tracked_samples, sent);
        assert!(
            stats.total_p50_ms.is_some_and(|ms| ms >= 0.0),
            "request start reaches its response receipt"
        );
        let samples = drain_venue_rest_failure_samples();
        if tunneled {
            assert!(stats.last_endpoint.is_some());
            assert!(samples.is_empty(), "HOW fulfills even on 404");
            assert_eq!(tunnel.seen.borrow().len(), sent);
            assert!(h.seen.borrow().is_empty());
        } else {
            assert_eq!(samples.len(), 1);
            assert!(samples[0].endpoint.is_some());
            assert_eq!(samples[0].status, Some(404.0));
            if hosted {
                assert!(
                    samples[0]
                        .signature_age_ms
                        .is_some_and(|age| (1500.0..10000.0).contains(&age))
                );
            } else {
                assert!(samples[0].signature_age_ms.is_none());
            }
            assert_eq!(samples[0].error_name.as_deref(), Some("AxiosError"));
            assert!(samples[0].path.as_deref().unwrap().contains(
                if name == safe_cex_types::ExchangeName::Woo {
                    "account"
                } else {
                    "info"
                }
            ));
            assert_eq!(samples[0].body_kind, "venue-json");
            assert_eq!(h.seen.borrow().len(), 1);
            assert!(tunnel.seen.borrow().is_empty());
        }
        h.exchange.dispose().unwrap();
    }
}

#[test]
fn typed_native_rest_diagnostics_follow_real_http_receipts() {
    run_local(native_rest_diagnostics(safe_cex_types::ExchangeName::Bybit));
}

#[test]
fn js_factory_rest_diagnostics_follow_real_http_receipts() {
    run_local(native_rest_diagnostics(safe_cex_types::ExchangeName::Woo));
}
