//! Offline admission checks use actual native startup and signing recipes.
use super::super::ResidentCopy;
use super::*;
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::{HttpRequest, Method, SocketId};
use safe_cex_native::{
    BoundHttp, Events, HostEvent, HttpPrepareError, HttpPrepareFuture, PreparedHttp,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::mpsc;

const VENUES: &[&str] = &[
    "bybitinverse",
    "bybitdemo",
    "bybitspot",
    "binance",
    "binancepm",
    "binanceinverse",
    "binancespot",
    "binanceoptions",
    "okx",
    "okxdemo",
    "okxeu",
    "bitget",
    "bitgetusdc",
    "blofin",
    "kucoin",
    "phemex",
    "woo",
    "bingx",
    "mexc",
    "bitunix",
];
const KEY: &str = "synthetic-key";
const SECRET: &str = "synthetic-material";
const PASS: &str = "synthetic-passphrase";

#[derive(Default)]
pub(crate) struct Seen {
    pub(crate) signed_reads: usize,
    pub(crate) config_refusals_remaining: usize,
    pub(crate) config_refusal_status: Option<u16>,
    pub(crate) refused: usize,
    pub(crate) inverse_private_reads: usize,
    pub(crate) urls: Vec<String>,
    pub(crate) demo_private_reads: usize,
    pub(crate) phemex_socket_auths: usize,
    pub(crate) woo_socket_auths: usize,
    pub(crate) bingx_private_sockets: usize,
    pub(crate) mexc_socket_auths: usize,
    pub(crate) bitunix_socket_auths: usize,
    paths: Vec<String>,
}
#[derive(Clone)]
pub(crate) struct VenueIo {
    pub(crate) venue: &'static str,
    pub(crate) seen: Arc<Mutex<Seen>>,
}
fn header<'a>(r: &'a HttpRequest, name: &str) -> Option<&'a str> {
    r.headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}
fn path(r: &HttpRequest) -> &str {
    let after_host = r.url.split_once("://").unwrap().1;
    &after_host[after_host.find('/').unwrap()..]
}
fn family(venue: &str) -> &str {
    match venue {
        "bybitinverse" | "bybitdemo" | "bybitspot" => "bybit",
        "binance" | "binancepm" | "binanceinverse" | "binancespot" | "binanceoptions" => "binance",
        "okx" | "okxdemo" | "okxeu" => "okx",
        "bitget" | "bitgetusdc" => "bitget",
        "blofin" | "kucoin" | "woo" | "bingx" | "mexc" => venue,
        _ => panic!("no private-read fixture for {venue}"),
    }
}

fn spot_subscription_reply(value: &Value) -> Value {
    let refused = || json!({"id":value["id"],"status":401,"error":{"code":-2015,"msg":"synthetic signature refusal"}});
    if value["method"] != "userDataStream.subscribe.signature" {
        return refused();
    }
    let params = &value["params"];
    let Some(key) = params["apiKey"].as_str() else {
        return refused();
    };
    let material = match key {
        KEY => SECRET,
        "synthetic-other-key" => "synthetic-other-material",
        _ => return refused(),
    };
    let (Some(window), Some(stamp)) = (params["recvWindow"].as_u64(), params["timestamp"].as_u64())
    else {
        return refused();
    };
    let preimage = format!("apiKey={key}&recvWindow={window}&timestamp={stamp}");
    if params["signature"] != hmac_sha256(material.as_bytes(), preimage.as_bytes(), Encoding::Hex) {
        return refused();
    }
    json!({"id":value["id"],"status":200,"result":{"subscriptionId":1}})
}

#[test]
fn admission_binance_spot_subscription_checks_valid_and_invalid_hmac() {
    for (key, material) in [
        (KEY, SECRET),
        ("synthetic-other-key", "synthetic-other-material"),
    ] {
        let preimage = format!("apiKey={key}&recvWindow=5000&timestamp=1");
        let valid = json!({"id":"fixture","method":"userDataStream.subscribe.signature","params":{"apiKey":key,"recvWindow":5000,"timestamp":1,"signature":hmac_sha256(material.as_bytes(),preimage.as_bytes(),Encoding::Hex)}});
        assert_eq!(spot_subscription_reply(&valid)["status"], 200);
        for field in ["signature", "apiKey", "timestamp"] {
            let mut invalid = valid.clone();
            invalid["params"][field] = "invalid".into();
            let reply = spot_subscription_reply(&invalid);
            assert_eq!(reply["status"], 401, "{field}");
            assert!(reply.get("result").is_none());
        }
    }
}

pub(crate) fn assert_variant_startup(venue: &str, seen: &Seen) {
    if venue == "binanceoptions" {
        binanceoptions::assert_startup(seen);
        return;
    }
    if venue == "mexc" {
        mexc::assert_startup(seen);
        return;
    }
    let (origin, private, product) = match venue {
        "bybitdemo" => (
            "https://api-demo.bybit.com",
            "/v5/account/wallet-balance",
            "category=linear",
        ),
        "bybitspot" => (
            "https://api.bybit.com",
            "/v5/account/wallet-balance",
            "category=spot",
        ),
        "binancepm" => (
            "https://papi.binance.com",
            "/papi/v1/balance",
            "/papi/v1/um/openOrders",
        ),
        "binanceinverse" => (
            "https://dapi.binance.com",
            "/dapi/v1/account",
            "/dapi/v1/openOrders",
        ),
        "binancespot" => (
            "https://api.binance.com",
            "/api/v3/account",
            "/api/v3/openOrders",
        ),
        "okxdemo" => (
            "https://openapi.okx.com",
            "/api/v5/account/config",
            "/api/v5/trade/orders-pending",
        ),
        "okxeu" => (
            "https://eea.okx.com",
            "/api/v5/account/config",
            "/api/v5/account/instruments?instType=FUTURES",
        ),
        "bitgetusdc" => (
            "https://api.bitget.com",
            "/api/v2/mix/account/accounts",
            "productType=usdc-futures",
        ),
        _ => return,
    };
    assert_eq!(
        safe_cex_native::resolve::rest_base_url(venue, false).unwrap(),
        origin
    );
    assert!(
        seen.urls
            .iter()
            .any(|url| url.split('?').next() == Some(format!("{origin}{private}").as_str())),
        "{venue}: missing private native route: {:?}",
        seen.urls
    );
    assert!(
        seen.urls
            .iter()
            .any(|url| url.starts_with(origin) && url.contains(product)),
        "{venue}: missing exact product: {:?}",
        seen.urls
    );
    if venue == "okxeu" {
        assert_eq!(
            seen.paths.first().map(String::as_str),
            Some("/api/v5/account/config")
        );
        assert!(
            seen.urls
                .iter()
                .any(|url| url == "https://eea.okx.com/api/v5/account/instruments?instType=SWAP")
        );
    }
    if venue == "okxdemo" {
        assert_eq!(
            seen.demo_private_reads, seen.signed_reads,
            "every demo private read needs the simulated header"
        );
    }
}
impl VenueIo {
    fn authenticated(&self, r: &HttpRequest) -> Option<bool> {
        let route = path(r).split('?').next().unwrap();
        // Native startup can attach credentials to public discovery. Public
        // responses cannot verify the joined key; only private reads count.
        let public = match family(self.venue) {
            "bybit" => route.starts_with("/v5/market/"),
            "binance" => [
                "exchangeInfo",
                "ticker/24hr",
                "ticker/bookTicker",
                "premiumIndex",
                "listenKey",
            ]
            .iter()
            .any(|suffix| route.ends_with(*suffix)),
            "okx" => route.starts_with("/api/v5/public/") || route.starts_with("/api/v5/market/"),
            "bitget" => route.starts_with("/api/v2/mix/market/"),
            "blofin" => route.starts_with("/api/v1/market/"),
            "kucoin" => ["contracts/active", "allTickers", "bullet-public"]
                .iter()
                .any(|suffix| route.ends_with(*suffix)),
            _ => unreachable!(),
        };
        if public {
            return None;
        }
        Some(self.valid_private_signature(r))
    }
    fn valid_private_signature(&self, r: &HttpRequest) -> bool {
        let method = if r.method == Method::Get {
            "GET"
        } else {
            "POST"
        };
        let suffix = r.body.as_deref().unwrap_or("");
        let h = |name: &str| header(r, name).unwrap_or("");
        let key = h(match family(self.venue) {
            "bybit" => "X-BAPI-API-KEY",
            "binance" => "X-MBX-APIKEY",
            "okx" => "OK-ACCESS-KEY",
            "bitget" | "blofin" => "ACCESS-KEY",
            "kucoin" => "KC-API-KEY",
            _ => unreachable!(),
        });
        let material = match key {
            KEY => SECRET,
            "synthetic-other-key" => "synthetic-other-material",
            _ => return false,
        };
        let mac = |payload: String, encoding| {
            hmac_sha256(material.as_bytes(), payload.as_bytes(), encoding)
        };
        match family(self.venue) {
            "bybit" => {
                let payload = r.url.split_once('?').map_or("", |(_, query)| query);
                h("X-BAPI-SIGN")
                    == mac(
                        format!(
                            "{}{}{}{payload}",
                            h("X-BAPI-TIMESTAMP"),
                            key,
                            h("X-BAPI-RECV-WINDOW")
                        ),
                        Encoding::Hex,
                    )
            }
            "binance" => {
                let Some((_, query)) = r.url.split_once('?') else {
                    return false;
                };
                let Some((payload, sig)) = query.rsplit_once("&signature=") else {
                    return false;
                };
                h("X-MBX-APIKEY") == key && sig == mac(payload.into(), Encoding::Hex)
            }
            "okx" | "bitget" => {
                let prefix = if family(self.venue) == "okx" {
                    "OK-ACCESS-"
                } else {
                    "ACCESS-"
                };
                let sig = h(&(prefix.to_owned() + "SIGN"));
                h(&(prefix.to_owned() + "KEY")) == key
                    && h(&(prefix.to_owned() + "PASSPHRASE")) == PASS
                    && sig
                        == mac(
                            format!(
                                "{}{method}{}{suffix}",
                                h(&(prefix.to_owned() + "TIMESTAMP")),
                                path(r)
                            ),
                            Encoding::Base64,
                        )
            }
            "blofin" => {
                let sig = h("ACCESS-SIGN");
                let hex = mac(
                    format!(
                        "{}{method}{}{}{suffix}",
                        path(r),
                        h("ACCESS-TIMESTAMP"),
                        h("ACCESS-NONCE")
                    ),
                    Encoding::Hex,
                );
                h("ACCESS-KEY") == key
                    && h("ACCESS-PASSPHRASE") == PASS
                    && sig == safe_cex_crypto::encode(hex.as_bytes(), Encoding::Base64)
            }
            "kucoin" => {
                let sig = h("KC-API-SIGN");
                h("KC-API-KEY") == key
                    && sig
                        == mac(
                            format!("{}{method}{}{suffix}", h("KC-API-TIMESTAMP"), path(r)),
                            Encoding::Base64,
                        )
                    && h("KC-API-PASSPHRASE") == mac(PASS.into(), Encoding::Base64)
            }
            _ => unreachable!(),
        }
    }
    fn response(&self, r: &HttpRequest) -> Value {
        if self.venue == "binanceoptions" {
            return binanceoptions::response(self, r);
        }
        if self.venue == "mexc" {
            return mexc::response(self, r);
        }
        if self.venue == "apexomni" {
            return apexomni::response(self, r);
        }
        if self.venue == "woo" {
            return woo::response(self, r);
        }
        if self.venue == "phemex" {
            return phemex::response(self, r);
        }
        if self.venue == "bingx" {
            return bingx::response(self, r);
        }
        if self.venue == "bitunix" {
            return bitunix::response(self, r);
        }
        if let Some(valid) = self.authenticated(r) {
            let mut seen = self.seen.lock().unwrap();
            seen.signed_reads += 1;
            seen.inverse_private_reads += usize::from(r.url.contains("category=inverse"));
            seen.demo_private_reads += usize::from(header(r, "x-simulated-trading") == Some("1"));
            if !valid {
                seen.refused += 1;
                return match family(self.venue) {
                    "bybit" => {
                        json!({"retCode":10004,"retMsg":"synthetic signature refusal","result":{}})
                    }
                    "binance" => json!({"code":-2015,"msg":"synthetic refusal"}),
                    "okx" => json!({"code":"50113","msg":"synthetic refusal","data":[]}),
                    "bitget" => json!({"code":"40006","msg":"synthetic refusal","data":null}),
                    "blofin" => json!({"code":"152401","msg":"synthetic refusal","data":[]}),
                    "kucoin" => json!({"code":"400003","msg":"synthetic refusal","data":null}),
                    _ => unreachable!(),
                };
            }
        }
        if self.venue == "okxeu" && path(r) == "/api/v5/account/config" {
            let mut seen = self.seen.lock().unwrap();
            if seen.config_refusals_remaining > 0 {
                seen.config_refusals_remaining -= 1;
                seen.refused += 1;
                return json!({"code":"50113","msg":"synthetic first signature refusal","data":[]});
            }
        }
        if self.venue == "okxeu" && path(r) == "/api/v5/account/instruments?instType=FUTURES" {
            return json!({"code":"0","msg":"","data":[]});
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/admission-reads.json"))
                .unwrap();
        let variants: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/admission-variant-reads.json"
        ))
        .unwrap();
        let bodies = variants
            .get(self.venue)
            .or_else(|| fixtures.get(family(self.venue)))
            .or_else(|| fixtures.get(self.venue))
            .and_then(Value::as_object)
            .unwrap();
        let url = path(r).split('?').next().unwrap();
        let fixture_url = if self.venue == "okxeu" && url == "/api/v5/account/instruments" {
            "/api/v5/public/instruments"
        } else {
            url
        };
        let mut response = bodies
            .iter()
            .find(|(key, _)| key.as_str() != "*" && fixture_url.ends_with(key.as_str()))
            .map_or(&bodies["*"], |(_, body)| body)
            .clone();
        if family(self.venue) == "bybit" {
            // Bybit syncs its signing clock from this field. A frozen capture repeats
            // signing bytes across distinct attempts, unlike a live venue clock.
            response["time"] = crate::accounts::now_ms().into();
        }
        response
    }
}
impl Transport for VenueIo {
    fn prepare_http(
        &self,
        _: HttpRequest,
        _: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        let io = self.clone();
        Box::pin(async move { Ok(Box::new(Prepared(io)) as Box<dyn PreparedHttp>) })
    }
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        assert_ne!(self.venue, "binanceoptions", "Options startup is REST-only");
        if self.venue == "mexc" {
            return mexc::connect(self, socket, &url, events);
        }
        if self.venue == "apexomni" {
            return apexomni::connect(self, socket, &url, events);
        }
        if self.venue == "woo" {
            return woo::connect(self, socket, &url, events);
        }
        if self.venue == "phemex" {
            return phemex::connect(self, socket, &url, events);
        }
        if self.venue == "bingx" {
            return bingx::connect(self, socket, &url, events);
        }
        if self.venue == "bitunix" {
            return bitunix::connect(self, socket, &url, events);
        }
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let _ = events.send(HostEvent::WsOpened(socket));
        let _ = events.send(HostEvent::WsMessage(
            socket,
            json!({"type":"welcome"}).to_string(),
        ));
        let venue = self.venue;
        let spot_private = venue == "binancespot" && url.contains("/ws-api/");
        tokio::task::spawn_local(async move {
            while let Some(text) = rx.recv().await {
                let Ok(v) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let reply = if spot_private {
                    spot_subscription_reply(&v)
                } else {
                    json!({"event":v["op"],"op":v["op"],"success":true,"code":"0","msg":"","type":"ack","id":v["id"]})
                };
                let _ = events.send(HostEvent::WsMessage(socket, reply.to_string()));
            }
        });
        tx
    }
    fn request(&self, r: HttpRequest, events: Events) {
        assert!(
            r.method == Method::Get
                || (r.method == Method::Post
                    && ((family(self.venue) == "binance" && path(&r).ends_with("listenKey"))
                        || (self.venue == "bingx"
                            && path(&r) == "/openApi/user/auth/userDataStream")
                        || (self.venue == "kucoin"
                            && ["bullet-private", "bullet-public"]
                                .iter()
                                .any(|endpoint| path(&r).ends_with(*endpoint))))),
            "admission may only read account state or obtain a read-feed token"
        );
        {
            let mut seen = self.seen.lock().unwrap();
            seen.paths.push(path(&r).split('?').next().unwrap().into());
            seen.urls.push(r.url.clone());
        }
        let response = self.response(&r);
        let status = if self.venue == "okxeu"
            && path(&r) == "/api/v5/account/config"
            && response["code"] == "50113"
        {
            self.seen
                .lock()
                .unwrap()
                .config_refusal_status
                .unwrap_or(200)
        } else if self.venue == "phemex" && response["code"] == 401 {
            401
        } else {
            200
        };
        let body = response.to_string();
        let _ = events.send(HostEvent::HttpStart {
            request: r.id,
            status,
            headers: String::new(),
        });
        let _ = events.send(HostEvent::HttpChunk(r.id, body.into_bytes()));
        let _ = events.send(HostEvent::HttpEnd(r.id));
    }
}
#[derive(Clone)]
struct Prepared<T = VenueIo>(T);
struct Bound<T = VenueIo>(T, HttpRequest);
impl<T> std::fmt::Debug for Prepared<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedAdmissionRead")
    }
}
impl<T> std::fmt::Debug for Bound<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BoundAdmissionRead")
    }
}
impl<T: Transport + Send + 'static> PreparedHttp for Prepared<T> {
    fn bind(self: Box<Self>, r: HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        Ok(Box::new(Bound(self.0, r)))
    }
}
impl<T: Transport + Send> BoundHttp for Bound<T> {
    fn ready(&self) -> bool {
        true
    }
    fn dispatch(self: Box<Self>, events: Events) -> tokio::task::AbortHandle {
        self.0.request(self.1, events);
        tokio::spawn(async {}).abort_handle()
    }
}
fn local(venue: &str, material: &str) -> super::super::PreparedDelegated {
    let mut options: safe_cex_venues::EngineOptions =
        serde_json::from_value(json!({"key":KEY,"secret":material,"passphrase":PASS})).unwrap();
    let (provider, identities) =
        safe_cex_venues::prepare_local_signer_for(venue, &mut options).unwrap();
    // Test setup uses the registry's real credential normalization. Runtime
    // admission remains gated separately; no test declaration enables a venue.
    super::super::PreparedDelegated {
        plan: Default::default(),
        options: json!({"signing":identities,"publicDataOnly":false,"accountStartup":true,"remoteSigning":false,"payloadSigner":false}).to_string(),
        resident: ResidentCopy::test_provider(provider),
    }
}
const KUCOIN_STARTUP_OPERATIONS: &[&str] = &[
    "GET/api/v1/contracts/active",
    "GET/api/v1/allTickers",
    "POST/api/v1/bullet-public",
    "GET/api/v1/positions",
    "GET/api/v1/orders?status=active&currentPage=1&pageSize=100",
    "GET/api/v1/stopOrders?status=active&currentPage=1&pageSize=100",
    "GET/api/v1/account-overview?currency=USDT",
    "GET/api/v2/position/getPositionMode",
    "POST/api/v1/bullet-private",
];

struct KucoinStartupCapture {
    prior: Arc<Mutex<HashMap<String, String>>>,
    held: Mutex<Vec<(HttpRequest, Events)>>,
    balance: Mutex<Option<HttpRequest>>,
}
impl KucoinStartupCapture {
    fn new(prior: Arc<Mutex<HashMap<String, String>>>) -> Self {
        Self {
            prior,
            held: Mutex::default(),
            balance: Mutex::default(),
        }
    }
    fn complete(&self) -> bool {
        let prior = self.prior.lock().unwrap();
        KUCOIN_STARTUP_OPERATIONS
            .iter()
            .all(|key| prior.contains_key(*key))
    }
    fn release(&self, io: &VenueIo) {
        if self.complete() {
            let held = std::mem::take(&mut *self.held.lock().unwrap());
            for (request, events) in held {
                io.request(request, events);
            }
        }
    }
}

#[derive(Clone)]
struct ReplayCaptureIo {
    io: VenueIo,
    capture: Option<Arc<KucoinStartupCapture>>,
}
impl Transport for ReplayCaptureIo {
    fn prepare_http(
        &self,
        _: HttpRequest,
        _: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        let io = self.clone();
        Box::pin(async move { Ok(Box::new(Prepared(io)) as Box<dyn PreparedHttp>) })
    }
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.io.connect(socket, url, events)
    }
    fn request(&self, request: HttpRequest, events: Events) {
        if let Some(capture) = &self.capture
            && path(&request).starts_with("/api/v1/account-overview?")
        {
            *capture.balance.lock().unwrap() = Some(request.clone());
            // Keep the positive proof alive until its actual startup signatures exist.
            if !capture.complete() {
                capture.held.lock().unwrap().push((request, events));
                return;
            }
        }
        self.io.request(request, events);
        if let Some(capture) = &self.capture {
            capture.release(&self.io);
        }
    }
}

async fn check(
    venue: &'static str,
    prepared: super::super::PreparedDelegated,
) -> (Result<(), ProbeRefusal>, Arc<Mutex<Seen>>) {
    check_captured(venue, prepared, None).await
}

async fn check_captured(
    venue: &'static str,
    prepared: super::super::PreparedDelegated,
    capture: Option<Arc<KucoinStartupCapture>>,
) -> (Result<(), ProbeRefusal>, Arc<Mutex<Seen>>) {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let io = ReplayCaptureIo {
        io: VenueIo {
            venue,
            seen: seen.clone(),
        },
        capture: capture.clone(),
    };
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        run(
            venue,
            &prepared.options,
            prepared.resident.provider(),
            io,
            ServerConfig::default(),
            Arc::new(|| true),
        ),
    )
    .await;
    assert!(
        result.is_ok(),
        "{venue} startup did not settle: {:?}",
        seen.lock().unwrap().paths
    );
    if let Some(capture) = capture
        && matches!(&result, Ok(Ok(())))
    {
        assert!(
            capture.complete(),
            "positive probe ended before every startup signature was captured"
        );
    }
    (result.unwrap(), seen)
}
#[tokio::test]
async fn admission_hmac_private_startup_accepts_valid_pairs() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for &venue in VENUES {
                let (answer, seen) = check(venue, local(venue, SECRET)).await;
                let seen = seen.lock().unwrap();
                assert!(
                    answer.is_ok(),
                    "{venue}: {answer:?}, paths={:?}",
                    seen.paths
                );
                assert!(
                    seen.signed_reads > 0,
                    "{venue} must perform a signed private read"
                );
                assert_eq!(seen.refused, 0);
                assert_variant_startup(venue, &seen);
            }
        })
        .await;
}

#[tokio::test]
async fn hosted_venues_use_real_delegated_construction_and_the_existing_probe() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for &venue in VENUES {
                if safe_cex_native::resolve::rest_base_url(venue, false).is_err() {
                    continue;
                }
                let init = crate::envelope::InitPayload {
                    exchange_name: venue.into(),
                    account_name: "fixture".into(),
                    key: KEY.into(),
                    signing_mode: Some("delegated".into()),
                    ..Default::default()
                };
                for (material, accepted) in [(SECRET, true), ("different-material", false)] {
                    let pair = crate::delegated::DelegatedPair::new(
                        KEY.into(),
                        material.into(),
                        Some(PASS.into()),
                    );
                    let prepared = crate::delegated::prepare_delegated(&init, pair)
                        .unwrap_or_else(|_| panic!("{venue}: hosted venue must construct"));
                    let (result, seen) = check(venue, prepared).await;
                    assert_eq!(result.is_ok(), accepted, "{venue}: {result:?}");
                    assert!(seen.lock().unwrap().signed_reads > 0);
                    if !accepted {
                        assert!(matches!(
                            result,
                            Err(ProbeRefusal {
                                transient: false,
                                ..
                            })
                        ));
                    }
                }
            }
        })
        .await;
}

#[derive(Clone, Copy)]
enum DeviceAnswer {
    Correct,
    DifferentKey,
    Replay,
}
// Remove only the attempt's changing stamp/nonce, keeping the same native
// operation and read parameters when looking up its earlier valid signature.
pub(crate) fn operation(venue: &str, canonical: &str) -> String {
    if venue == "mexc" {
        return mexc::operation(canonical);
    }
    if venue == "woo" {
        return if canonical.contains('|') {
            canonical.rsplit_once('|').unwrap().0.to_owned()
        } else {
            canonical
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .into()
        };
    }
    if venue == "bitunix" {
        // Bitunix signs nonce + stamp + key + sorted query/body, without a route.
        return canonical
            .split_once("synthetic-other-key")
            .or_else(|| canonical.split_once(KEY))
            .expect("native Bitunix preimage contains its presented key")
            .1
            .into();
    }
    if venue == "phemex" {
        // Native Phemex signs path + query + whole-second expiry; keep the read scope.
        return canonical
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .into();
    }
    if family(venue) == "bybit" {
        return canonical
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .to_owned();
    }
    if family(venue) == "binance" || venue == "bingx" {
        return canonical
            .split('&')
            .filter(|part| !part.starts_with("timestamp="))
            .collect::<Vec<_>>()
            .join("&");
    }
    let payload = if venue == "kucoin" {
        serde_json::from_str::<Value>(canonical).unwrap()["payload"]
            .as_str()
            .unwrap()
            .to_owned()
    } else {
        canonical.to_owned()
    };
    let at = payload
        .find("GET")
        .or_else(|| payload.find("POST"))
        .unwrap();
    if venue == "blofin" {
        payload[..at].to_owned()
    } else {
        payload[at..].to_owned()
    }
}
async fn remote(
    venue: &'static str,
    answer: DeviceAnswer,
    prior: Arc<Mutex<HashMap<String, String>>>,
) -> (
    super::super::PreparedDelegated,
    Arc<safe_cex_remote_signer::SignerLink>,
    tokio::task::JoinHandle<()>,
) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let link =
        safe_cex_remote_signer::SignerLink::new(77, Arc::new(move |frame| tx.send(frame).is_ok()));
    let pending = Arc::downgrade(&link);
    let task = tokio::task::spawn_local(async move {
        let mut options: safe_cex_venues::EngineOptions = serde_json::from_value(json!({"key":KEY,"secret":if matches!(answer, DeviceAnswer::DifferentKey) { "synthetic-other-material" } else { SECRET },"passphrase":PASS})).unwrap();
        let (provider, identities) =
            safe_cex_venues::prepare_local_signer_for(venue, &mut options).unwrap();
        while let Some(frame) = rx.recv().await {
            let frame: Value = serde_json::from_str(&frame).unwrap();
            let request = &frame["d"];
            assert_eq!(request["type"], "ws-auth");
            let input = serde_json::from_value(json!({"kind":"wsAuth","toSign":request["wsAuth"]["toSign"],"outputFormat":request["wsAuth"]["outputFormat"]})).unwrap();
            let (work, output) =
                safe_cex_venues::hosted_signing::prepare(venue, &identities, input).unwrap();
            let signed = provider.sign(work).await.unwrap();
            let fresh = safe_cex_venues::hosted_signing::finish(output, &signed)
                .unwrap()
                .signature
                .unwrap();
            let is_ws = request["wsAuth"]["toSign"].as_str().is_some_and(|text| {
                text.starts_with("GET/realtime")
                    || text.contains("/users/self/verify")
                    || text.contains("/user/verify")
                    || venue == "binancespot" && text.starts_with("apiKey=")
                    || venue == "woo" && text.starts_with('|')
            });
            let canonical = request["wsAuth"]["toSign"].as_str().unwrap();
            let operation = operation(venue, canonical);
            let signature = if !is_ws && matches!(answer, DeviceAnswer::Replay) {
                let old = prior
                    .lock()
                    .unwrap()
                    .get(&operation)
                    .expect("accepted attempt signed the same startup operation")
                    .clone();
                assert_ne!(old, fresh, "a new attempt must sign fresh bytes");
                old
            } else {
                if !is_ws && matches!(answer, DeviceAnswer::Correct) {
                    prior.lock().unwrap().insert(operation, fresh.clone());
                }
                fresh
            };
            let response = json!({"id":request["id"],"wsAuthSignature":signature});
            let Some(link) = pending.upgrade() else {
                break;
            };
            assert!(
                !link.resolve(78, response.clone()),
                "another socket cannot answer"
            );
            if !link.resolve(77, response.clone()) {
                break;
            }
            assert!(
                !link.resolve(77, response),
                "consumed response cannot resolve twice"
            );
        }
    });
    let account = safe_cex_remote_signer::remote_account(
        link.clone(),
        venue,
        Some("synthetic-pending".into()),
        &json!({"key":KEY,"passphrase":PASS}).to_string(),
    )
    .unwrap();
    (
        super::super::PreparedDelegated {
            plan: Default::default(),
            options: account.options_json,
            resident: ResidentCopy::test_provider(account.provider),
        },
        link,
        task,
    )
}

#[tokio::test]
async fn admission_hmac_negative_suite_uses_the_shared_probe_for_both_modes() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for &venue in VENUES {
                let prior = Arc::new(Mutex::new(HashMap::new()));
                for answer in [
                    DeviceAnswer::Correct,
                    DeviceAnswer::DifferentKey,
                    DeviceAnswer::Replay,
                ] {
                    if venue == "phemex" && matches!(answer, DeviceAnswer::Replay) {
                        // Phemex signs whole-second expiry; old-attempt bytes must really differ.
                        tokio::time::sleep(Duration::from_millis(1100)).await;
                    }
                    let (prepared, link, task) = remote(venue, answer, prior.clone()).await;
                    let capture = (venue == "kucoin" && matches!(answer, DeviceAnswer::Correct))
                        .then(|| Arc::new(KucoinStartupCapture::new(prior.clone())));
                    let (result, seen) = check_captured(venue, prepared, capture).await;
                    {
                        let seen = seen.lock().unwrap();
                        assert!(
                            seen.signed_reads > 0,
                            "{venue}: real signed startup required"
                        );
                        if matches!(answer, DeviceAnswer::Correct) {
                            assert!(result.is_ok(), "{venue}: valid remote proof");
                            assert_eq!(seen.refused, 0);
                        } else {
                            assert!(
                                matches!(
                                    result,
                                    Err(ProbeRefusal {
                                        transient: false,
                                        ..
                                    })
                                ),
                                "{venue}: {result:?}"
                            );
                            assert!(seen.refused > 0, "{venue}: fixture verified the signature");
                        }
                    }
                    link.mark_unavailable();
                    task.abort();
                    if let Err(error) = task.await {
                        assert!(error.is_cancelled(), "device responder failed: {error}");
                    }
                }
                let (result, seen) = check(venue, local(venue, "synthetic-other-material")).await;
                assert!(
                    matches!(
                        result,
                        Err(ProbeRefusal {
                            transient: false,
                            ..
                        })
                    ),
                    "{venue}: different delegated pair"
                );
                assert!(seen.lock().unwrap().refused > 0);
            }
        })
        .await;
}

#[tokio::test]
async fn kucoin_positive_capture_holds_balance_for_every_missing_startup_signature() {
    tokio::task::LocalSet::new().run_until(async {
        let prior = Arc::new(Mutex::new(HashMap::new()));
        let capture = Arc::new(KucoinStartupCapture::new(prior.clone()));
        let (prepared, link, task) = remote("kucoin", DeviceAnswer::Correct, prior.clone()).await;
        let (result, seen) = check_captured("kucoin", prepared, Some(capture.clone())).await;
        assert!(result.is_ok());
        assert!(capture.complete());
        link.mark_unavailable();
        task.abort();
        if let Err(error) = task.await { assert!(error.is_cancelled(), "device responder failed: {error}"); }
        // Exercise the response gate with the actual packet from the accepted native run.
        let balance = capture.balance.lock().unwrap().clone().expect("native balance request captured");
        let io = ReplayCaptureIo { io: VenueIo { venue: "kucoin", seen }, capture: Some(capture.clone()) };
        assert_eq!(io.io.authenticated(&balance), Some(true));
        for &operation in KUCOIN_STARTUP_OPERATIONS {
            let genuine = prior.lock().unwrap().remove(operation).unwrap();
            let (events, mut received) = mpsc::unbounded_channel();
            io.request(balance.clone(), events);
            assert!(received.try_recv().is_err(), "balance released without {operation} capture");
            assert_eq!(capture.held.lock().unwrap().len(), 1);
            prior.lock().unwrap().insert(operation.into(), genuine);
            capture.release(&io.io);
            assert!(matches!(received.try_recv().unwrap(), HostEvent::HttpStart { request, status:200, .. } if request == balance.id));
            let HostEvent::HttpChunk(id, body) = received.try_recv().unwrap() else { panic!("native venue body required") };
            assert_eq!(id, balance.id);
            assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["code"], "200000");
            assert!(matches!(received.try_recv().unwrap(), HostEvent::HttpEnd(id) if id == balance.id));
            assert!(capture.held.lock().unwrap().is_empty());
            assert_eq!(io.io.seen.lock().unwrap().refused, 0);
        }
    }).await;
}

#[tokio::test]
async fn admission_hmac_handover_is_single_use_and_bound_to_the_attempt_and_pair() {
    for &venue in VENUES {
        if safe_cex_native::resolve::rest_base_url(venue, false).is_err() {
            assert!(
                crate::delegated::Handovers::default()
                    .issue("pending", venue, 0)
                    .is_err()
            );
            continue;
        }
        crate::delegated::with_test_declaration(
            json!({"venues": {venue: {"remoteSigning": {"enabled": true}}}}),
            async {
                let mut handovers = crate::delegated::Handovers::default();
                let offer = handovers.issue("pending", venue, 0).unwrap();
                let bytes = json!({"key":KEY,"secret":SECRET,"passphrase":PASS}).to_string();
                let sealed = crate::delegated::tests::seal(&offer, venue, KEY, bytes.as_bytes());
                let opening = handovers.take("pending", &sealed, 1).unwrap();
                assert!(opening.open(venue, KEY, &sealed, 1).is_ok());
                assert!(
                    handovers.take("pending", &sealed, 1).is_err(),
                    "{venue}: consumed attempt"
                );
                let next = handovers.issue("pending", venue, 2).unwrap();
                assert!(
                    handovers.take("pending", &sealed, 3).is_err(),
                    "{venue}: earlier attempt"
                );
                let mut rebound = sealed.clone();
                rebound.attempt_id = next.attempt_id;
                let opening = handovers.take("pending", &rebound, 3).unwrap();
                assert!(
                    opening.open(venue, KEY, &rebound, 3).is_err(),
                    "{venue}: new attempt cannot open prior handover"
                );
                let offer = handovers.issue("pending", venue, 4).unwrap();
                let other =
                    json!({"key":"synthetic-other-key","secret":"synthetic-other-material"})
                        .to_string();
                let sealed = crate::delegated::tests::seal(&offer, venue, KEY, other.as_bytes());
                assert!(
                    handovers
                        .take("pending", &sealed, 5)
                        .unwrap()
                        .open(venue, KEY, &sealed, 5)
                        .is_err(),
                    "{venue}: different pair"
                );
            },
        )
        .await;
    }
}

#[test]
fn admission_enabled_venues_require_an_offline_suite() {
    let rules: Value = serde_json::from_str(include_str!(
        "../../../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
    ))
    .unwrap();
    assert_eq!(PRIVATE_STARTUP_VENUES.len(), VENUES.len() + 1);
    for &venue in PRIVATE_STARTUP_VENUES {
        assert!(
            venue == "bybit" || VENUES.contains(&venue),
            "{venue}: missing offline suite"
        );
    }
    // DELEGATED uses shared hosted support rather than a separate rollout list.
    // The catalog above pins COSIGNED proof coverage; it does not gate DELEGATED.
    for rule in rules["venues"].as_object().unwrap().values() {
        assert!(rule.get("delegated").is_none());
    }
}

#[tokio::test]
async fn admission_hmac_public_or_frame_fed_startup_cannot_prove_a_private_key() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for &venue in VENUES {
                for field in ["publicDataOnly", "frameFed", "publicMode"] {
                    let mut prepared = local(venue, SECRET);
                    let mut options: Value = serde_json::from_str(&prepared.options).unwrap();
                    options[field] = if field == "publicMode" {
                        json!({"requested":true})
                    } else {
                        true.into()
                    };
                    prepared.options = options.to_string();
                    let (result, seen) = check(venue, prepared).await;
                    assert!(
                        matches!(
                            result,
                            Err(ProbeRefusal {
                                transient: false,
                                ..
                            })
                        ),
                        "{venue}: {field} cannot satisfy private admission"
                    );
                    assert_eq!(seen.lock().unwrap().signed_reads, 0);
                }
            }
        })
        .await;
}

#[test]
fn admission_fixture_checks_private_routes_even_when_signature_headers_are_missing() {
    for (venue, public, private, signature_header) in [
        (
            "bybitinverse",
            "/v5/market/instruments-info",
            "/v5/account/wallet-balance",
            "X-BAPI-SIGN",
        ),
        (
            "binance",
            "/fapi/v1/exchangeInfo",
            "/fapi/v3/account",
            "X-MBX-APIKEY",
        ),
        (
            "okx",
            "/api/v5/public/instruments",
            "/api/v5/account/config",
            "OK-ACCESS-SIGN",
        ),
        (
            "bitget",
            "/api/v2/mix/market/contracts",
            "/api/v2/account/settings",
            "ACCESS-SIGN",
        ),
        (
            "bybitdemo",
            "/v5/market/instruments-info",
            "/v5/account/wallet-balance",
            "X-BAPI-SIGN",
        ),
        (
            "bybitspot",
            "/v5/market/instruments-info",
            "/v5/account/wallet-balance",
            "X-BAPI-SIGN",
        ),
        (
            "binancepm",
            "/fapi/v1/exchangeInfo",
            "/papi/v1/balance",
            "X-MBX-APIKEY",
        ),
        (
            "binanceinverse",
            "/dapi/v1/exchangeInfo",
            "/dapi/v1/account",
            "X-MBX-APIKEY",
        ),
        (
            "binancespot",
            "/api/v3/exchangeInfo",
            "/api/v3/account",
            "X-MBX-APIKEY",
        ),
        (
            "okxdemo",
            "/api/v5/public/instruments",
            "/api/v5/account/config",
            "OK-ACCESS-SIGN",
        ),
        (
            "bitgetusdc",
            "/api/v2/mix/market/contracts",
            "/api/v2/mix/account/accounts",
            "ACCESS-SIGN",
        ),
        (
            "blofin",
            "/api/v1/market/instruments",
            "/api/v1/account/margin-mode",
            "ACCESS-SIGN",
        ),
        (
            "kucoin",
            "/api/v1/contracts/active",
            "/api/v1/account-overview",
            "KC-API-SIGN",
        ),
    ] {
        let io = VenueIo {
            venue,
            seen: Arc::default(),
        };
        let mut request = HttpRequest {
            id: safe_cex_engine::RequestId(1),
            method: Method::Get,
            route: None,
            timeout_ms: None,
            url: format!("https://example.test{public}?timestamp=1&signature=invalid"),
            headers: vec![(signature_header, "invalid".into())],
            signer_headers: vec![],
            body: None,
        };
        assert_eq!(
            io.authenticated(&request),
            None,
            "{venue}: public discovery cannot authenticate a key"
        );
        request.url = format!("https://example.test{private}");
        request.headers.clear();
        assert_eq!(
            io.authenticated(&request),
            Some(false),
            "{venue}: an unsigned private read must refuse"
        );
    }
}

#[tokio::test]
async fn admission_bybitinverse_native_private_startup_validates_key() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for material in [SECRET, "synthetic-other-material"] {
                let (result, seen) = check("bybitinverse", local("bybitinverse", material)).await;
                assert_eq!(result.is_ok(), material == SECRET);
                let seen = seen.lock().unwrap();
                assert!(seen.signed_reads > 0);
                assert_eq!(seen.refused > 0, material != SECRET);
                if material == SECRET {
                    assert!(
                        seen.paths
                            .iter()
                            .any(|path| path.ends_with("/account/wallet-balance"))
                    );
                }
            }
        })
        .await;
}

#[path = "phemex_probe_tests.rs"]
mod phemex;

#[path = "apexomni_probe_tests.rs"]
mod apexomni;
#[path = "bingx_probe_tests.rs"]
mod bingx;

#[path = "bitunix_probe_tests.rs"]
mod bitunix;
#[path = "mexc_probe_tests.rs"]
mod mexc;
#[path = "woo_probe_tests.rs"]
mod woo;

#[path = "binanceoptions_probe_tests.rs"]
mod binanceoptions;
