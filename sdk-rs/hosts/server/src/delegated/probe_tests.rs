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

const VENUES: &[&str] = &["binance", "okx", "bitget", "blofin", "kucoin"];
const KEY: &str = "synthetic-key";
const SECRET: &str = "synthetic-material";
const PASS: &str = "synthetic-passphrase";

#[derive(Default)]
struct Seen {
    signed_reads: usize,
    refused: usize,
    paths: Vec<String>,
}
#[derive(Clone)]
struct VenueIo {
    venue: &'static str,
    seen: Arc<Mutex<Seen>>,
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
impl VenueIo {
    fn authenticated(&self, r: &HttpRequest) -> Option<bool> {
        let method = if r.method == Method::Get {
            "GET"
        } else {
            "POST"
        };
        let suffix = r.body.as_deref().unwrap_or("");
        let mac = |payload: String, encoding| {
            hmac_sha256(SECRET.as_bytes(), payload.as_bytes(), encoding)
        };
        let h = |name: &str| header(r, name).unwrap_or("");
        match self.venue {
            "binance" => {
                let (_, query) = r.url.split_once('?')?;
                let (payload, sig) = query.rsplit_once("&signature=")?;
                Some(h("X-MBX-APIKEY") == KEY && sig == mac(payload.into(), Encoding::Hex))
            }
            "okx" | "bitget" => {
                let prefix = if self.venue == "okx" {
                    "OK-ACCESS-"
                } else {
                    "ACCESS-"
                };
                let sig = header(r, &(prefix.to_owned() + "SIGN"))?;
                Some(
                    h(&(prefix.to_owned() + "KEY")) == KEY
                        && h(&(prefix.to_owned() + "PASSPHRASE")) == PASS
                        && sig
                            == mac(
                                format!(
                                    "{}{method}{}{suffix}",
                                    h(&(prefix.to_owned() + "TIMESTAMP")),
                                    path(r)
                                ),
                                Encoding::Base64,
                            ),
                )
            }
            "blofin" => {
                let sig = header(r, "ACCESS-SIGN")?;
                let hex = mac(
                    format!(
                        "{}{method}{}{}{suffix}",
                        path(r),
                        h("ACCESS-TIMESTAMP"),
                        h("ACCESS-NONCE")
                    ),
                    Encoding::Hex,
                );
                Some(
                    h("ACCESS-KEY") == KEY
                        && h("ACCESS-PASSPHRASE") == PASS
                        && sig == safe_cex_crypto::encode(hex.as_bytes(), Encoding::Base64),
                )
            }
            "kucoin" => {
                let sig = header(r, "KC-API-SIGN")?;
                Some(
                    h("KC-API-KEY") == KEY
                        && sig
                            == mac(
                                format!("{}{method}{}{suffix}", h("KC-API-TIMESTAMP"), path(r)),
                                Encoding::Base64,
                            )
                        && h("KC-API-PASSPHRASE") == mac(PASS.into(), Encoding::Base64),
                )
            }
            _ => unreachable!(),
        }
    }
    fn response(&self, r: &HttpRequest) -> Value {
        if let Some(valid) = self.authenticated(r) {
            let mut seen = self.seen.lock().unwrap();
            seen.signed_reads += 1;
            if !valid {
                seen.refused += 1;
                return match self.venue {
                    "binance" => json!({"code":-2015,"msg":"synthetic refusal"}),
                    "okx" => json!({"code":"50113","msg":"synthetic refusal","data":[]}),
                    "bitget" => json!({"code":"40006","msg":"synthetic refusal","data":null}),
                    "blofin" => json!({"code":"152401","msg":"synthetic refusal","data":[]}),
                    "kucoin" => json!({"code":"400003","msg":"synthetic refusal","data":null}),
                    _ => unreachable!(),
                };
            }
        }
        let fixtures: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/admission-reads.json"))
                .unwrap();
        let bodies = fixtures[self.venue].as_object().unwrap();
        let url = path(r).split('?').next().unwrap();
        bodies
            .iter()
            .find(|(key, _)| key.as_str() != "*" && url.ends_with(key.as_str()))
            .map_or(&bodies["*"], |(_, body)| body)
            .clone()
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
        _: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let _ = events.send(HostEvent::WsOpened(socket));
        let _ = events.send(HostEvent::WsMessage(
            socket,
            json!({"type":"welcome"}).to_string(),
        ));
        tokio::task::spawn_local(async move {
            while let Some(text) = rx.recv().await {
                let Ok(v) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let reply = json!({"event":v["op"],"op":v["op"],"success":true,"code":"0","msg":"","type":"ack","id":v["id"]});
                let _ = events.send(HostEvent::WsMessage(socket, reply.to_string()));
            }
        });
        tx
    }
    fn request(&self, r: HttpRequest, events: Events) {
        assert!(
            r.method == Method::Get
                || (r.method == Method::Post
                    && ((self.venue == "binance" && path(&r).ends_with("listenKey"))
                        || (self.venue == "kucoin"
                            && ["bullet-private", "bullet-public"]
                                .iter()
                                .any(|endpoint| path(&r).ends_with(*endpoint))))),
            "admission may only read account state or obtain a read-feed token"
        );
        self.seen
            .lock()
            .unwrap()
            .paths
            .push(path(&r).split('?').next().unwrap().into());
        let body = self.response(&r).to_string();
        let _ = events.send(HostEvent::HttpStart {
            request: r.id,
            status: 200,
            headers: String::new(),
        });
        let _ = events.send(HostEvent::HttpChunk(r.id, body.into_bytes()));
        let _ = events.send(HostEvent::HttpEnd(r.id));
    }
}
#[derive(Clone)]
struct Prepared(VenueIo);
struct Bound(VenueIo, HttpRequest);
impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedAdmissionRead")
    }
}
impl std::fmt::Debug for Bound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BoundAdmissionRead")
    }
}
impl PreparedHttp for Prepared {
    fn bind(self: Box<Self>, r: HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        Ok(Box::new(Bound(self.0, r)))
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
async fn check(
    venue: &'static str,
    prepared: super::super::PreparedDelegated,
) -> (Result<(), ProbeRefusal>, Arc<Mutex<Seen>>) {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let io = VenueIo {
        venue,
        seen: seen.clone(),
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
fn operation(venue: &str, canonical: &str) -> String {
    if venue == "binance" {
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
                text.contains("/users/self/verify") || text.contains("/user/verify")
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
                    let (prepared, link, task) = remote(venue, answer, prior.clone()).await;
                    let (result, seen) = check(venue, prepared).await;
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
                                matches!(result, Err(ProbeRefusal { transient: false })),
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
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                let (result, seen) = check(venue, local(venue, "synthetic-other-material")).await;
                assert!(
                    matches!(result, Err(ProbeRefusal { transient: false })),
                    "{venue}: different delegated pair"
                );
                assert!(seen.lock().unwrap().refused > 0);
            }
        })
        .await;
}

#[test]
fn admission_hmac_handover_is_single_use_and_bound_to_the_attempt_and_pair() {
    for &venue in VENUES {
        let mut handovers = crate::delegated::Handovers::default();
        let offer = handovers.issue("pending", 0).unwrap();
        let bytes = json!({"key":KEY,"secret":SECRET,"passphrase":PASS}).to_string();
        let sealed = crate::delegated::tests::seal(&offer, venue, KEY, bytes.as_bytes());
        let opening = handovers.take("pending", &sealed, 1).unwrap();
        assert!(opening.open(venue, KEY, &sealed, 1).is_ok());
        assert!(
            handovers.take("pending", &sealed, 1).is_err(),
            "{venue}: consumed attempt"
        );
        let next = handovers.issue("pending", 2).unwrap();
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
        let offer = handovers.issue("pending", 4).unwrap();
        let other =
            json!({"key":"synthetic-other-key","secret":"synthetic-other-material"}).to_string();
        let sealed = crate::delegated::tests::seal(&offer, venue, KEY, other.as_bytes());
        assert!(
            handovers
                .take("pending", &sealed, 5)
                .unwrap()
                .open(venue, KEY, &sealed, 5)
                .is_err(),
            "{venue}: different pair"
        );
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
    for (venue, rule) in rules["venues"].as_object().unwrap() {
        if rule["delegated"]["enabled"] == true {
            assert!(
                venue == "bybit" || VENUES.contains(&venue.as_str()),
                "{venue}: add and pass admission checks before enabling"
            );
        }
    }
    for &venue in VENUES {
        assert_ne!(
            rules["venues"][venue]["delegated"]["enabled"], true,
            "{venue}: this change must keep rollout off"
        );
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
                        matches!(result, Err(ProbeRefusal { transient: false })),
                        "{venue}: {field} cannot satisfy private admission"
                    );
                    assert_eq!(seen.lock().unwrap().signed_reads, 0);
                }
            }
        })
        .await;
}
