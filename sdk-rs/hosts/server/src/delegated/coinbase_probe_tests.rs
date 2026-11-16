//! Synthetic venue verification of the actual native CDP JWTs, without network IO.
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use p256::ecdsa::{Signature, SigningKey, signature::Verifier};
use safe_cex_engine::{HttpRequest, Method, SocketId};
use safe_cex_native::{
    BoundHttp, Events, HostEvent, HttpPrepareError, HttpPrepareFuture, PreparedHttp, Transport,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use tokio::sync::{Semaphore, mpsc};

pub(crate) const VENUES: &[&str] = &["coinbase", "coinbasespot"];

pub(crate) fn key(algorithm: &str, seed: u8) -> String {
    format!("organizations/fixture/apiKeys/{algorithm}-{seed}")
}

pub(crate) fn secret(algorithm: &str, seed: u8) -> String {
    if algorithm == "edDsa" {
        return STANDARD.encode([seed; 32]);
    }
    let mut der = vec![
        0x30, 0x41, 0x02, 0x01, 0x00, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02,
        0x01, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x04, 0x27, 0x30, 0x25,
        0x02, 0x01, 0x01, 0x04, 0x20,
    ];
    der.extend([seed; 32]);
    format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        STANDARD.encode(der)
    )
}

pub(crate) fn valid_jwt(jwt: &str, uri: Option<&str>) -> bool {
    let parts: Vec<_> = jwt.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    let parse = |part: &str| {
        URL_SAFE_NO_PAD
            .decode(part)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    };
    let (Some(header), Some(claims), Ok(signature)) = (
        parse(parts[0]),
        parse(parts[1]),
        URL_SAFE_NO_PAD.decode(parts[2]),
    ) else {
        return false;
    };
    let Some(kid) = header["kid"].as_str() else {
        return false;
    };
    let Some((algorithm, seed)) = ["es256", "edDsa"]
        .into_iter()
        .flat_map(|algorithm| [7, 8].map(move |seed| (algorithm, seed)))
        .find(|(algorithm, seed)| kid == key(algorithm, *seed))
    else {
        return false;
    };
    let (Some(nbf), Some(exp)) = (claims["nbf"].as_u64(), claims["exp"].as_u64()) else {
        return false;
    };
    let now = crate::accounts::now_ms() / 1000;
    if header["typ"] != "JWT"
        || claims["sub"] != kid
        || header["alg"]
            != if algorithm == "es256" {
                "ES256"
            } else {
                "EdDSA"
            }
        || header["nonce"].as_str().is_none_or(str::is_empty)
        || nbf.abs_diff(now) > 20
        || nbf.checked_add(120) != Some(exp)
        || exp <= now
        || claims["uri"].as_str() != uri
    {
        return false;
    }
    let cdp = uri == Some("POST drb.coinbase.com/api/v2/public/auth")
        || uri.is_none() && claims["iss"] == "cdp" && claims["aud"].is_null();
    if cdp {
        if claims["iss"] != "cdp" || !claims["aud"].is_null() {
            return false;
        }
    } else if algorithm == "es256" {
        if claims["iss"] != "coinbase-cloud" || !claims["aud"].is_null() {
            return false;
        }
    } else if claims["iss"] != "cdp" || claims["aud"] != json!(["cdp_service"]) {
        return false;
    }
    let input = format!("{}.{}", parts[0], parts[1]);
    if algorithm == "es256" {
        let key = SigningKey::from_slice(&[seed; 32]).unwrap();
        Signature::from_slice(&signature).is_ok_and(|signature| {
            key.verifying_key()
                .verify(input.as_bytes(), &signature)
                .is_ok()
        })
    } else {
        safe_cex_crypto::ed25519::verify(
            input.as_bytes(),
            &safe_cex_crypto::ed25519::public_key(&[seed; 32]).unwrap(),
            &signature,
        )
        .unwrap_or(false)
    }
}

#[derive(Default)]
pub(crate) struct Seen {
    pub(crate) signed_reads: usize,
    pub(crate) auth_exchanges: usize,
    pub(crate) private_reads: usize,
    pub(crate) socket_auths: usize,
    pub(crate) refused: usize,
    pub(crate) urls: Vec<String>,
    pub(crate) block_private: bool,
    pub(crate) blocked_reads: usize,
    tokens: HashSet<String>,
}

#[derive(Clone)]
pub(crate) struct CoinbaseIo {
    pub(crate) venue: &'static str,
    pub(crate) seen: Arc<Mutex<Seen>>,
    pub(crate) gate: Arc<Semaphore>,
}
impl CoinbaseIo {
    pub(crate) fn new(venue: &'static str) -> Self {
        Self {
            venue,
            seen: Arc::default(),
            gate: Arc::new(Semaphore::new(0)),
        }
    }
    fn private_census(&self, url: &str) -> bool {
        url.contains("/private/")
            || self.venue == "coinbasespot" && url.contains("/orders/historical/")
    }
    fn request(&self, request: HttpRequest, events: Events) {
        self.seen.lock().unwrap().urls.push(request.url.clone());
        let body = request
            .body
            .as_ref()
            .map(|body| serde_json::from_str::<Value>(body).unwrap());
        let mut status = 200;
        let value = if request
            .url
            .starts_with("https://api.coinbase.com/api/v3/brokerage/")
        {
            assert_eq!(request.method, Method::Get, "proof cannot trade");
            let jwt = request
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case("Authorization"))
                .and_then(|(_, value)| value.strip_prefix("Bearer "));
            let uri = format!(
                "GET {}",
                request
                    .url
                    .trim_start_matches("https://")
                    .split('?')
                    .next()
                    .unwrap()
            );
            if jwt.is_some() {
                self.seen.lock().unwrap().signed_reads += 1;
            }
            if !request.url.contains("/market/products")
                && !jwt.is_some_and(|jwt| valid_jwt(jwt, Some(&uri)))
            {
                self.seen.lock().unwrap().refused += 1;
                status = 401;
                json!({"error":"UNAUTHENTICATED","message":"synthetic signature refusal"})
            } else if request.url.ends_with("/portfolios") {
                json!({"portfolios":[{"uuid":"portfolio-one","type":if self.venue == "coinbase" {"INTX"} else {"DEFAULT"}}]})
            } else if request.url.contains("/portfolios/portfolio-one") {
                json!({"breakdown":{"spot_positions":[]}})
            } else if request.url.contains("/products") {
                json!({"products":[{"product_id":"BTC-USD","product_type":"SPOT","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.0001","price_increment":"0.1","base_min_size":"0.0001","base_max_size":"1000000000","price":"60000","volume_24h":"20","price_percentage_change_24h":"0"}]})
            } else if request.url.contains("/orders/historical/batch") {
                self.seen.lock().unwrap().private_reads += 1;
                json!({"orders":[],"has_next":false})
            } else {
                panic!("unexpected Coinbase read: {}", request.url);
            }
        } else {
            assert!(request.url.starts_with("https://drb.coinbase.com/api/v2/"));
            assert_eq!(request.method, Method::Post);
            let body = body.as_ref().expect("native JSON RPC");
            let method = body["method"].as_str().unwrap();
            let route = request.url.rsplit('/').next().unwrap();
            assert_eq!(method.rsplit('/').next(), Some(route));
            if method == "public/auth" {
                let jwt = body["params"]["jwt"].as_str().unwrap_or_default();
                if body["params"]["grant_type"] != "coinbase_cdp"
                    || !valid_jwt(jwt, Some("POST drb.coinbase.com/api/v2/public/auth"))
                {
                    self.seen.lock().unwrap().refused += 1;
                    status = 401;
                    json!({"jsonrpc":"2.0","id":body["id"],"error":{"code":13009,"message":"unauthorized"}})
                } else {
                    let token = format!(
                        "fixture-{}",
                        safe_cex_crypto::hex(&safe_cex_crypto::sha256(jwt.as_bytes()))
                    );
                    let mut seen = self.seen.lock().unwrap();
                    seen.auth_exchanges += 1;
                    seen.tokens.insert(token.clone());
                    json!({"jsonrpc":"2.0","id":body["id"],"result":{"access_token":token,"expires_in":900,"scope":"trade:read","token_type":"bearer"}})
                }
            } else if method.starts_with("private/") {
                assert!(
                    matches!(
                        method,
                        "private/get_account_summary"
                            | "private/get_positions"
                            | "private/get_open_orders_by_currency"
                            | "private/get_open_orders"
                    ),
                    "proof cannot trade"
                );
                let bearer = request
                    .headers
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case("Authorization"))
                    .and_then(|(_, value)| value.strip_prefix("Bearer "));
                let mut seen = self.seen.lock().unwrap();
                seen.private_reads += 1;
                if !bearer.is_some_and(|token| seen.tokens.contains(token)) {
                    seen.refused += 1;
                    status = 401;
                    json!({"jsonrpc":"2.0","id":body["id"],"error":{"code":13009,"message":"unauthorized"}})
                } else {
                    json!({"jsonrpc":"2.0","id":body["id"],"result":if method == "private/get_account_summary" {
                        json!({"currency":"USDC","balance":1000.,"equity":1000.,"available_funds":1000.,"initial_margin":0.,"maintenance_margin":0.,"futures_session_upl":0.,"margin_model":"cross_sm"})
                    } else { json!([]) }})
                }
            } else {
                let mut fixture: Value =
                    include_str!("../../../../venues/coinbase/tests/fixtures/drb-public.jsonl")
                        .lines()
                        .map(|line| serde_json::from_str::<Value>(line).unwrap())
                        .find(|row| row["method"] == method)
                        .and_then(|row| row["body"].as_str().map(str::to_owned))
                        .map(|body| serde_json::from_str(&body).unwrap())
                        .unwrap_or_else(|| panic!("unexpected DRB read: {method}"));
                fixture["id"] = body["id"].clone();
                fixture
            }
        };
        let _ = events.send(HostEvent::HttpStart {
            request: request.id,
            status,
            headers: String::new(),
        });
        let _ = events.send(HostEvent::HttpChunk(
            request.id,
            value.to_string().into_bytes(),
        ));
        let _ = events.send(HostEvent::HttpEnd(request.id));
    }
}

impl Transport for CoinbaseIo {
    fn request(&self, request: HttpRequest, events: Events) {
        CoinbaseIo::request(self, request, events);
    }
    fn prepare_http(
        &self,
        request: HttpRequest,
        _: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        let io = self.clone();
        let block = self.private_census(&request.url) && {
            let mut seen = self.seen.lock().unwrap();
            let block = seen.block_private;
            if block {
                seen.blocked_reads += 1;
            }
            block
        };
        Box::pin(async move {
            if block {
                io.gate.acquire().await.unwrap().forget();
            }
            Ok(Box::new(Prepared(io)) as Box<dyn PreparedHttp>)
        })
    }
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        assert!(matches!(
            url.as_str(),
            "wss://streams.drb.coinbase.com/ws/api/v2"
                | "wss://drb.coinbase.com/ws/api/v2"
                | "wss://advanced-trade-ws.coinbase.com"
                | "wss://advanced-trade-ws-user.coinbase.com"
        ));
        let io = self.clone();
        let (send, mut receive) = mpsc::unbounded_channel::<String>();
        events.send(HostEvent::WsOpened(socket)).unwrap();
        tokio::task::spawn_local(async move {
            while let Some(text) = receive.recv().await {
                let value: Value = serde_json::from_str(&text).unwrap();
                let answer = if let Some(method) = value["method"].as_str() {
                    let result = match method {
                        "public/auth" => {
                            let accepted = value["params"]["jwt"]
                                .as_str()
                                .is_some_and(|jwt| valid_jwt(jwt, None));
                            io.seen.lock().unwrap().socket_auths += 1;
                            if !accepted {
                                io.seen.lock().unwrap().refused += 1;
                            }
                            if !accepted {
                                let _=events.send(HostEvent::WsMessage(socket,json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":13009,"message":"unauthorized"}}).to_string()));
                                continue;
                            }
                            json!({"access_token":"fixture-ws","expires_in":900,"scope":"trade:read","token_type":"bearer"})
                        }
                        "private/subscribe" | "public/subscribe" => {
                            value["params"]["channels"].clone()
                        }
                        "public/set_heartbeat" | "public/test" => json!("ok"),
                        _ => panic!("unexpected Coinbase socket method: {method}"),
                    };
                    json!({"jsonrpc":"2.0","id":value["id"],"result":result})
                } else {
                    assert_eq!(value["type"], "subscribe");
                    if url.contains("ws-user") {
                        io.seen.lock().unwrap().socket_auths += 1;
                        assert!(
                            value["jwt"]
                                .as_str()
                                .is_some_and(|jwt| valid_jwt(jwt, None))
                        );
                    }
                    json!({"channel":"subscriptions","events":[{"subscriptions":{}}]})
                };
                let _ = events.send(HostEvent::WsMessage(socket, answer.to_string()));
            }
        });
        send
    }
}

struct Prepared(CoinbaseIo);
struct Bound(CoinbaseIo, HttpRequest);
impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CoinbaseFixture")
    }
}
impl std::fmt::Debug for Bound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CoinbaseFixture")
    }
}
impl PreparedHttp for Prepared {
    fn bind(self: Box<Self>, request: HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        Ok(Box::new(Bound(self.0, request)))
    }
}
impl BoundHttp for Bound {
    fn ready(&self) -> bool {
        true
    }
    fn dispatch(self: Box<Self>, events: Events) -> tokio::task::AbortHandle {
        self.0.request(self.1, events);
        tokio::spawn(async {}).abort_handle()
    }
}

#[test]
fn synthetic_coinbase_venue_accepts_both_keys_and_rejects_another_key_and_old_nonce_signature() {
    use safe_cex_venue_coinbase::sign::{JwtAlgorithm, signing_input_for};
    use safe_cex_venues::{EngineOptions, hosted_signing, local_signer_from_capsule};
    for venue in VENUES {
        for algorithm in ["es256", "edDsa"] {
            for seed in [7, 8] {
                for uri in [
                    Some("GET api.coinbase.com/api/v3/brokerage/portfolios"),
                    Some("POST drb.coinbase.com/api/v2/public/auth"),
                    None,
                ] {
                    let input = signing_input_for(
                        if algorithm == "es256" {
                            JwtAlgorithm::Es256
                        } else {
                            JwtAlgorithm::EdDsa
                        },
                        &key(algorithm, seed),
                        "fresh-server-nonce",
                        crate::accounts::now_ms() / 1000,
                        uri,
                        uri.is_none() || uri.is_some_and(|uri| uri.contains("drb.coinbase.com")),
                    )
                    .unwrap();
                    let sign = |scalar| {
                        let(provider,ids)=local_signer_from_capsule(venue,json!({"primary":{"key":key(algorithm,seed),"secret":secret(algorithm,scalar)}}).to_string(),
                    &mut serde_json::from_str::<EngineOptions>("{}").unwrap()).unwrap();
                        let (work, kind) = hosted_signing::prepare(
                            venue,
                            &ids,
                            hosted_signing::Input::WsAuth {
                                to_sign: input.clone(),
                                output_format: hosted_signing::OutputFormat::CoinbaseJws,
                            },
                        )
                        .unwrap();
                        hosted_signing::finish(kind, &provider.sign_ready(&work).unwrap().unwrap())
                            .unwrap()
                            .signature
                            .unwrap()
                    };
                    let signature = sign(seed);
                    assert!(valid_jwt(&format!("{input}.{signature}"), uri));
                    assert!(!valid_jwt(
                        &format!("{input}.{}", sign(if seed == 7 { 8 } else { 7 })),
                        uri
                    ));
                    let (header, claims) = input.split_once('.').unwrap();
                    let mut header: Value =
                        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header).unwrap()).unwrap();
                    header["nonce"] = "different-server-nonce".into();
                    let changed = format!(
                        "{}.{}",
                        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap()),
                        claims
                    );
                    assert!(!valid_jwt(&format!("{changed}.{signature}"), uri));
                }
            }
        }
    }
}
