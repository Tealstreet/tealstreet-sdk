//! Venue admission: slot selection, setup mapping and the dispatch recipe
//! agree. Each venue request is replayed through `Recipe` directly with the
//! expected refs and wallets, so only the selection under test can differ.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{Identity, KeyRef, Provider};
use safe_cex_engine::test_support::{prepared_actions, seeded_entropy};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Method, Response, SignStep, SocketId, TimerId,
    Venue,
};
use safe_cex_venue_asterdex::AsterDex;
use safe_cex_venue_asterdex::sign::{Auth, Query, Recipe};
use support::Options;

const ORIGIN: &str = "https://fapi.asterdex.com";
const NOW: f64 = 1_700_000_000_000.0;
const USER: &str = "0x63DD5aCC6b1aa0f563956C0e534DD30B6dcF7C4e";
const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","pricePrecision":1,"quantityPrecision":3,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"100"},{"filterType":"MARKET_LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"100"}]}]}"#;

/// A distinct wallet per test keeps the process-global agent nonces apart.
fn wallet(byte: u8) -> (String, String) {
    let key = format!("0x{}", format!("{byte:02x}").repeat(32));
    let address = safe_cex_crypto::ethereum::private_key_address(&key).unwrap();
    (key, address)
}

fn identity(credential: &str, signing: &str) -> Identity {
    Identity::new(KeyRef(credential.into()), Some(KeyRef(signing.into())))
}

struct Issue(Option<(Method, String, Box<dyn SignStep>)>);
impl Venue for Issue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let (method, path, step) = self.0.take().unwrap();
        let id = cx.request(method, format!("{ORIGIN}{path}"), vec![], None, ());
        cx.sign_http(id, step);
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn param<'a>(url: &'a str, name: &str) -> Option<&'a str> {
    url.split_once('?')?
        .1
        .split('&')
        .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
}

/// The same request through `Recipe` with the expected identity and wallets,
/// pinned to the nonce and stamp the venue used.
fn replay(provider: &Provider, request: &HttpRequest, identity: Identity, signer: &str) -> String {
    let path = request.url[ORIGIN.len()..].split('?').next().unwrap();
    let nonce: u64 = param(&request.url, "nonce").unwrap().parse().unwrap();
    let now = param(&request.url, "timestamp").map_or(NOW, |t| t.parse().unwrap());
    let auth = Auth::V3 {
        user: USER.into(),
        signer: signer.into(),
        nonce: Box::new(move |_, _| Ok(nonce)),
    };
    let recipe = Recipe::new(
        identity,
        auth,
        request.method,
        ORIGIN,
        path,
        Query::new(),
        false,
    )
    .unwrap();
    let mut engine = Engine::new(
        Issue(Some((request.method, path.into(), Box::new(recipe)))),
        Some(seeded_entropy(9)),
    )
    .unwrap();
    engine.set_signer(Some(provider.clone())).unwrap();
    engine.start(now);
    let http = prepared_actions(&mut engine, now)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .unwrap();
    assert_eq!(http.headers, request.headers);
    http.url
}

/// Start the venue, sync the clock and markets, and return signed requests.
fn signed_requests(options: Options, command: Option<Command>) -> (Vec<HttpRequest>, Provider) {
    let (venue, provider) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(provider.clone())).unwrap();
    engine.start(NOW);
    let mut signed = Vec::new();
    let mut pending: Vec<HttpRequest> = Vec::new();
    let collect = |engine: &mut Engine<AsterDex>, pending: &mut Vec<HttpRequest>| {
        for action in prepared_actions(engine, NOW) {
            if let Action::Http(request) = action {
                pending.push(request);
            }
        }
    };
    collect(&mut engine, &mut pending);
    if let Some(command) = command {
        engine.handle(NOW, Event::Command(command));
        collect(&mut engine, &mut pending);
    }
    for _ in 0..16 {
        let Some(request) = pending.pop() else { break };
        if request.url.contains("signature=") {
            signed.push(request);
            continue;
        }
        let body = if request.url.ends_with("/time") {
            r#"{"serverTime":1700000000000}"#
        } else if request.url.contains("/exchangeInfo") {
            MARKETS
        } else {
            "[]"
        };
        engine.handle(
            NOW,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        collect(&mut engine, &mut pending);
    }
    (signed, provider)
}

#[test]
fn v3_read_agent_presents_its_key_and_signs_with_the_aliased_0x_wallet() {
    let (key, signer) = wallet(0x31);
    let (signed, provider) = signed_requests(
        Options {
            key: Some(signer.clone()),
            application_id: Some(USER.into()),
            auth_private_key: Some(key),
            read_key: Some("0x2222222222222222222222222222222222222222".into()),
            read_secret: Some("ignored-when-aliased".into()),
            ..Options::default()
        },
        None,
    );
    let read = signed
        .iter()
        .find(|r| r.method == Method::Get && r.url.contains("/fapi/v3/account?"))
        .expect("V3 account read");
    assert_eq!(
        read.headers,
        vec![(
            "X-MBX-APIKEY",
            "0x2222222222222222222222222222222222222222".into()
        )]
    );
    // The read agent wallet defaults to the read key; the signature is the
    // primary wallet's, through the secondary slot's aliased signing ref.
    let read_signer = "0x2222222222222222222222222222222222222222";
    assert!(read.url.contains(&format!("signer={read_signer}")));
    let expected = identity("secondary.credential", "primary.signing");
    assert_eq!(replay(&provider, read, expected, read_signer), read.url);
    let listen = signed
        .iter()
        .find(|r| r.method == Method::Post && r.url.contains("/fapi/v3/listenKey?"))
        .expect("listen key through the recipe");
    let expected = identity("primary.credential", "primary.signing");
    assert_eq!(replay(&provider, listen, expected, &signer), listen.url);
}

#[test]
fn public_data_only_keeps_signing_reads_with_the_read_identity() {
    let (key, signer) = wallet(0x32);
    let (signed, provider) = signed_requests(
        Options {
            public_data_only: true,
            key: Some(signer.clone()),
            secret: Some(key),
            application_id: Some(USER.into()),
            auth_signer_address: Some(signer.clone()),
            read_key: Some("read-agent-key".into()),
            read_secret: Some(format!("0x{}", "33".repeat(32))),
            ..Options::default()
        },
        Some(Command::FetchAccount { command: 1 }),
    );
    assert!(!signed.iter().any(|r| r.url.contains("/listenKey")));
    let read = signed
        .iter()
        .find(|r| r.url.contains("/fapi/v3/account?"))
        .expect("signed account read");
    assert_eq!(
        read.headers,
        vec![("X-MBX-APIKEY", "read-agent-key".into())]
    );
    let expected = identity("secondary.credential", "secondary.signing");
    assert_eq!(replay(&provider, read, expected, &signer), read.url);
}

/// FetchAccount over V1 or V3 with the given account body; returns the result.
fn account_user_id(options: Options, body: &str) -> Result<String, String> {
    let (venue, provider) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(provider)).unwrap();
    engine.start(NOW);
    engine.handle(NOW, Event::Command(Command::FetchAccount { command: 1 }));
    let mut pending: Vec<_> = prepared_actions(&mut engine, NOW);
    for _ in 0..32 {
        let Some(action) = pending.pop() else { break };
        match action {
            Action::Notice(safe_cex_engine::Notice::CommandDone { command: 1, result }) => {
                return Ok(
                    serde_json::from_str::<serde_json::Value>(&result).unwrap()["userId"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                );
            }
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 1, error }) => {
                return Err(error.message);
            }
            Action::Http(request) => {
                let reply = if request.url.ends_with("/time") {
                    r#"{"serverTime":1700000000000}"#
                } else if request.url.contains("/exchangeInfo") {
                    MARKETS
                } else if request.url.contains("/account?") {
                    body
                } else {
                    "[]"
                };
                engine.handle(
                    NOW,
                    Event::HttpResponse {
                        request: request.id,
                        response: Response {
                            status: 200,
                            headers: "",
                            body: reply,
                        },
                    },
                );
                pending.extend(prepared_actions(&mut engine, NOW));
            }
            _ => {}
        }
    }
    panic!("account command did not finish");
}

#[test]
fn account_identity_never_falls_back_to_the_api_key() {
    let v1 = |stable_id: Option<&str>| Options {
        key: Some("v1-api-key".into()),
        secret: Some("v1-api-secret".into()),
        stable_id: stable_id.map(str::to_owned),
        ..Options::default()
    };
    assert_eq!(
        account_user_id(v1(None), r#"{"accountAlias":"alias-7"}"#),
        Ok("alias-7".into())
    );
    assert_eq!(
        account_user_id(v1(Some("account/aster-1")), r#"{"accountAlias":""}"#),
        Ok("account/aster-1".into())
    );
    let error = account_user_id(v1(None), "{}").unwrap_err();
    assert!(error.contains("trusted primary stableId"), "{error}");
    assert!(!error.contains("v1-api-key"));
    // V3 keys are public wallet addresses, so the signer address remains the fallback.
    let (key, signer) = wallet(0x34);
    assert_eq!(
        account_user_id(
            Options {
                key: Some(signer.clone()),
                secret: Some(key),
                application_id: Some(USER.into()),
                ..Options::default()
            },
            "{}"
        ),
        Ok(signer)
    );
}
