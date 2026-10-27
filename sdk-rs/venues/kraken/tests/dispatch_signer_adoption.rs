//! Original custody assertions against the unchanged native Futures/Spot signer.
mod spot_support;
mod support;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_crypto::signer::{KeyEncoding, LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, KeyRef, SignError, SignFuture, SignRequest, SignResult,
    Signer as Provider,
};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Query, ReadCredentials, Response,
    SocketChannel, SocketId, Venue,
};
use safe_cex_venue_kraken::{Kraken, KrakenSpot, Options, sign};
use std::sync::{Arc, Mutex};
use support::NOW;
const KEY: &str = "fixture-kraken-primary";
const READ_KEY: &str = "fixture-kraken-read";
const SECRET: &str = "a3Jha2VuLXRlc3Qtc2VjcmV0";
const READ_SECRET: &str = "Zml4dHVyZS1rcmFrZW4tcmVhZC1zZWNyZXQ=";
const CHALLENGE: &str = "c100b894-1729-464d-ace1-52dbce11db42";
struct Recording {
    inner: LocalSigner,
    batches: Mutex<Vec<SignRequest>>,
    refuse: bool,
}
impl Recording {
    fn count(&self) -> usize {
        self.batches.lock().unwrap().len()
    }
}
impl Provider for Recording {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, r: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.batches.lock().unwrap().push(r.clone());
        if self.refuse {
            Some(Err(SignError::Unsupported))
        } else {
            self.inner.sign_ready(r)
        }
    }
    fn sign(&self, r: SignRequest) -> SignFuture {
        self.batches.lock().unwrap().push(r.clone());
        if self.refuse {
            Box::pin(async { Err(SignError::Unsupported) })
        } else {
            self.inner.sign(r)
        }
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}
fn options(read: bool) -> Options {
    Options {
        key: Some(KEY.into()),
        secret: Some(SECRET.into()),
        read_credentials: read.then(|| ReadCredentials {
            key: READ_KEY.into(),
            secret: READ_SECRET.into(),
            passphrase: None,
        }),
        ..Options::default()
    }
}
fn identity(key: &str, secret: &str, stable: Option<&str>) -> LocalIdentity {
    LocalIdentity {
        key: Secret::new(key.into()),
        secret: Secret::new(secret.into()),
        stable_id: stable.map(str::to_owned),
        secret_encoding: KeyEncoding::Base64,
        ..LocalIdentity::default()
    }
}
fn provider(opts: &Options, stable: Option<&str>, refuse: bool) -> (Arc<Recording>, Identities) {
    let (inner, ids) = LocalSigner::from_setup(LocalSetup {
        primary: opts
            .key
            .as_ref()
            .zip(opts.secret.as_ref())
            .map(|(k, s)| identity(k, s, stable)),
        secondary: opts
            .read_credentials
            .as_ref()
            .map(|r| identity(&r.key, &r.secret, Some("fixture-read-subject"))),
    })
    .unwrap();
    (
        Arc::new(Recording {
            inner,
            batches: Mutex::new(vec![]),
            refuse,
        }),
        ids,
    )
}
fn engine<V: Venue>(venue: V, p: &Arc<Recording>, ids: Identities) -> Engine<V> {
    let mut e = safe_cex_engine::test_support::seeded_engine(venue);
    e.set_signer(Some(p.clone())).unwrap();
    e.set_identities(ids).unwrap();
    e.start(NOW);
    e
}
fn boot<V: Venue>(e: &mut Engine<V>, spot: bool) -> (Vec<HttpRequest>, Vec<SocketId>) {
    let mut requests = vec![];
    let mut sockets = vec![];
    for _ in 0..16 {
        let actions = safe_cex_engine::test_support::prepared_actions(e, NOW);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(r) => {
                    let path = r.url.as_str();
                    let body = if path.ends_with("AssetPairs") {
                        spot_support::META
                    } else if path.ends_with("Ticker") {
                        spot_support::TICKERS
                    } else if path.ends_with("Balance") {
                        spot_support::BALANCE
                    } else if path.ends_with("OpenOrders") {
                        spot_support::OPEN
                    } else if path.ends_with("GetWebSocketsToken") {
                        r#"{"error":[],"result":{"token":"fixture-token","expires":900}}"#
                    } else if path.contains("/instruments") {
                        support::META
                    } else if path.contains("/tickers") {
                        support::TICKERS
                    } else if path.contains("/openorders") {
                        support::OPEN
                    } else if path.contains("/openpositions") {
                        r#"{"result":"success","openPositions":[]}"#
                    } else if path.contains("/leveragepreferences") {
                        r#"{"result":"success","leveragePreferences":[]}"#
                    } else if path.contains("/accounts") {
                        support::ACCOUNT
                    } else {
                        panic!("unhandled bootstrap {}", r.url)
                    };
                    e.handle(
                        NOW,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                    requests.push(r);
                }
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => {
                    sockets.push(socket);
                    if spot {
                        e.handle(NOW, Event::WsOpened { socket });
                    }
                }
                _ => {}
            }
        }
    }
    (requests, sockets)
}
fn header<'a>(r: &'a HttpRequest, name: &str) -> &'a str {
    r.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .unwrap()
        .1
        .as_str()
}
fn futures(
    read: bool,
) -> (
    Engine<Kraken>,
    Arc<Recording>,
    Vec<HttpRequest>,
    Vec<SocketId>,
) {
    let opts = options(read);
    let (p, ids) = provider(&opts, Some("fixture-primary-subject"), false);
    let mut e = engine(Kraken::new(opts), &p, ids);
    let (r, s) = boot(&mut e, false);
    (e, p, r, s)
}
fn spot(read: bool) -> (Engine<KrakenSpot>, Arc<Recording>, Vec<HttpRequest>) {
    let opts = options(read);
    let (p, ids) = provider(&opts, Some("fixture-primary-subject"), false);
    let mut e = engine(KrakenSpot::new(opts), &p, ids);
    let (r, _) = boot(&mut e, true);
    (e, p, r)
}
#[test]
fn futures_account_reads_consult_the_installed_provider() {
    let (_, p, r, _) = futures(true);
    assert!(r.iter().any(|r| r.url.contains("/accounts")));
    assert!(
        p.count() >= 4,
        "four account reads require atomic provider batches"
    );
}
#[test]
fn spot_balance_orders_token_consult_the_installed_provider() {
    let (_, p, r) = spot(true);
    assert!(r.iter().any(|r| r.url.ends_with("Balance")));
    assert!(r.iter().any(|r| r.url.ends_with("GetWebSocketsToken")));
    assert!(p.count() >= 3, "Spot balance/orders/token bypass provider");
}
#[test]
fn futures_challenge_key_presentation_uses_provider() {
    let (mut e, p, _, s) = futures(false);
    let before = p.count();
    e.handle(NOW, Event::WsOpened { socket: s[0] });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        p.count() > before,
        "challenge key presentation must use provider"
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::WsSend{text,..} if text.contains("challenge")))
    );
}
#[test]
fn futures_challenge_mac_uses_current_session_provider() {
    let (mut e, p, _, s) = futures(false);
    e.handle(NOW, Event::WsOpened { socket: s[0] });
    e.drain_actions().for_each(drop);
    let before = p.count();
    e.handle(
        NOW,
        Event::WsMessage {
            socket: s[0],
            text: &format!(r#"{{"event":"challenge","message":"{CHALLENGE}"}}"#),
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(p.count() > before, "challenge MAC must use provider");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::WsSend{text,..} if text.contains("signed_challenge")))
    );
}
#[test]
fn futures_order_history_uses_primary_provider_batch() {
    let (mut e, p, _, _) = futures(true);
    let before = p.count();
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 71,
            query: Query::OrderHistory {
                symbol: None,
                from_ms: Some(NOW - 60000.),
                to_ms: Some(NOW),
                limit: None,
            },
        }),
    );
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(p.count() > before, "history bypassed provider");
    let r = a
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(header(r, "APIKey"), KEY);
}
#[test]
fn futures_write_uses_fresh_primary_provider_batch() {
    let (mut e, p, _, _) = futures(true);
    let before = p.count();
    let order=serde_json::from_value(serde_json::json!({"symbol":"PF_XBTUSD","type":"limit","side":"buy","amount":1,"price":100})).unwrap();
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 72,
            orders: vec![order],
        }),
    );
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(p.count() > before, "write bypassed provider");
    let r = a
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                r.url.ends_with("sendorder").then_some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(header(r, "APIKey"), KEY);
}
#[test]
fn refusing_provider_prevents_futures_signed_http() {
    let opts = options(false);
    let (p, ids) = provider(&opts, Some("fixture-refused-futures"), true);
    let mut e = engine(Kraken::new(opts), &p, ids);
    let (r, _) = boot(&mut e, false);
    assert!(
        !r.iter()
            .any(|r| r.headers.iter().any(|(k, _)| *k == "Authent")),
        "provider refusal must block signed HTTP"
    );
}
#[test]
fn refusing_provider_prevents_spot_signed_http() {
    let opts = options(false);
    let (p, ids) = provider(&opts, Some("fixture-refused-spot"), true);
    let mut e = engine(KrakenSpot::new(opts), &p, ids);
    let (r, _) = boot(&mut e, true);
    assert!(
        !r.iter().any(|r| r.url.contains("/0/private/")),
        "provider refusal must block signed HTTP"
    );
}
#[test]
fn missing_stable_subject_refuses_before_private_io() {
    let opts = options(false);
    let (p, ids) = provider(&opts, None, false);
    let mut e = engine(Kraken::new(opts), &p, ids);
    let (r, _) = boot(&mut e, false);
    assert!(
        !r.iter()
            .any(|r| r.headers.iter().any(|(k, _)| *k == "Nonce")),
        "missing stable_id cannot use secret digest fallback"
    );
    assert_eq!(p.count(), 0);
}
#[test]
fn native_futures_secondary_wire_and_mac() {
    let (_, _, r, _) = futures(true);
    let r = r.iter().find(|r| r.url.ends_with("/accounts")).unwrap();
    assert_eq!(r.method, Method::Get);
    assert!(r.body.is_none());
    assert_eq!(header(r, "APIKey"), READ_KEY);
    assert_eq!(
        r.headers.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        ["APIKey", "Authent", "Nonce", "Content-Type"]
    );
    assert_eq!(
        header(r, "Authent"),
        sign::Signer::new(READ_SECRET)
            .unwrap()
            .signature("", header(r, "Nonce"), "/derivatives/api/v3/accounts")
            .unwrap()
    );
}
#[test]
fn native_spot_ignores_read_key_nonce_body_and_timeout() {
    let (_, _, r) = spot(true);
    let r = r.iter().find(|r| r.url.ends_with("Balance")).unwrap();
    assert_eq!(header(r, "API-Key"), KEY);
    assert_eq!(r.method, Method::Post);
    assert_eq!(r.timeout_ms, Some(20000));
    assert_eq!(
        r.headers.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        ["API-Key", "API-Sign", "Content-Type", "Accept"]
    );
    let b = r.body.as_ref().unwrap();
    assert!(b.starts_with("nonce="));
    assert_eq!(
        header(r, "API-Sign"),
        sign::Signer::new(SECRET)
            .unwrap()
            .spot_signature(b, b.strip_prefix("nonce=").unwrap(), "/0/private/Balance")
            .unwrap()
    );
}
#[test]
fn native_read_only_futures_does_not_start_private_account() {
    let mut opts = options(true);
    opts.key = None;
    opts.secret = None;
    let (p, ids) = provider(&opts, None, false);
    let mut e = engine(Kraken::new(opts), &p, ids);
    let (r, _) = boot(&mut e, false);
    assert!(
        !r.iter()
            .any(|r| r.headers.iter().any(|(key, _)| *key == "APIKey"))
    );
    assert_eq!(p.count(), 0);
}
#[test]
fn native_independent_futures_vector() {
    assert_eq!(
        sign::Signer::new(SECRET)
            .unwrap()
            .signature(
                "limitPrice=100&orderType=lmt&side=buy&size=1&symbol=PF_XBTUSD",
                "1700000000000000",
                "/derivatives/api/v3/sendorder"
            )
            .unwrap(),
        "UJjfP/I++ZnynbKoDc73lGxQMzNrzWTg9iEv5CkmFUNfPn/nZqGrCzwzPcX9HB6SFP1CuPhKFMBI5Ar52tJP4Q=="
    );
}
#[test]
fn native_independent_spot_vector() {
    assert_eq!(
        sign::Signer::new(SECRET)
            .unwrap()
            .spot_signature(
                "nonce=1700000000000000",
                "1700000000000000",
                "/0/private/Balance"
            )
            .unwrap(),
        "vkWVoPHiCpSwNZz8duXYFcqT2SZq3GwKIFzE3Ef0mcI1ibEAdkyAYiTVQ5NS/i/mNblY1rKxDOWnSQNWna9beA=="
    );
}
#[test]
fn native_independent_challenge_vector() {
    assert_eq!(
        sign::Signer::new(SECRET).unwrap().challenge(CHALLENGE),
        "RBpQqz7vfqXJ38JklfnuiQNVp5NimxtQPdKwk8kPa9pK1yz62RP2yMEqeYgIzfYbdUF/vQlZvIUASliWvHOXcw=="
    );
}
#[test]
fn native_sorted_form_unicode_and_parentheses() {
    assert_eq!(
        sign::form(&[("z", "é +/(x)".into()), ("a", "1".into())]),
        "a=1&z=%C3%A9+%2B%2F(x)"
    );
}
#[test]
fn native_nonce_recreation_and_backward_clock() {
    let secret = STANDARD.encode("fixture-kraken-recreation-original");
    let mut a = sign::Signer::new(&secret).unwrap();
    let n: u64 = a.nonce(NOW + 0.75).unwrap().parse().unwrap();
    let mut b = sign::Signer::new(&secret).unwrap();
    let m: u64 = b.nonce(1.).unwrap().parse().unwrap();
    assert_eq!(n, NOW as u64 * 1000);
    assert_eq!(m, n + 1);
}
#[test]
fn native_nonce_thousand_sends_one_ms() {
    let mut s = sign::Signer::new(&STANDARD.encode("fixture-kraken-many-original")).unwrap();
    let mut last = 0;
    for _ in 0..1002 {
        let n: u64 = s.nonce(NOW).unwrap().parse().unwrap();
        assert!(n > last);
        last = n;
    }
    assert_eq!(last, NOW as u64 * 1000 + 1001);
}
#[test]
fn native_bad_secret_path_and_clock_refuse() {
    assert!(sign::Signer::new("*").is_err());
    assert!(sign::Signer::new("").is_err());
    let mut s = sign::Signer::new(SECRET).unwrap();
    for t in [f64::NAN, f64::INFINITY, -1., f64::MAX] {
        assert!(s.nonce(t).is_err());
    }
    assert!(s.signature("", "1", "/api/v3/orders?x=1").is_err());
    assert!(s.spot_signature("", "1", "/api/v3/orders").is_err());
}
#[test]
fn native_public_only_futures_provider_and_auth_unused() {
    let mut opts = options(true);
    opts.public_data_only = true;
    let (p, ids) = provider(&opts, Some("fixture-public-futures"), false);
    let mut e = engine(Kraken::new(opts), &p, ids);
    let (r, _) = boot(&mut e, false);
    assert!(!r.is_empty());
    assert_eq!(p.count(), 0);
    assert!(
        !r.iter()
            .any(|r| r.headers.iter().any(|(k, _)| *k == "Authent"))
    );
}

struct SpotDeliveryProbe {
    venue: KrakenSpot,
    delivered: usize,
}
impl Venue for SpotDeliveryProbe {
    type Pending = <KrakenSpot as Venue>::Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.venue.capabilities()
    }
    fn socket_control(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        authenticated: bool,
        text: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.venue.socket_control(store, feed, authenticated, text)
    }
    fn on_socket_event(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.venue.on_socket_event(cx, event)
    }
    fn encode_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        feed: safe_cex_engine::socket_lifecycle::FeedId,
        subscribe: bool,
        items: &[safe_cex_engine::socket_lifecycle::Interest],
        emit: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.venue
            .encode_socket_interests(store, feed, subscribe, items, emit)
    }
    fn start(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>) {
        self.venue.start(cx);
    }
    fn on_open(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>, socket: SocketId) {
        self.venue.on_open(cx, socket);
    }
    fn on_message(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        socket: SocketId,
        text: &str,
    ) {
        self.venue.on_message(cx, socket, text);
    }
    fn on_close(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        socket: SocketId,
        code: u16,
    ) {
        self.venue.on_close(cx, socket, code);
    }
    fn on_response(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.delivered += 1;
        self.venue.on_response(cx, pending, response);
    }
    fn on_item(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        pending: &mut Self::Pending,
        item: &str,
    ) {
        self.venue.on_item(cx, pending, item);
    }
    fn on_request_failed(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        self.venue.on_request_failed(cx, pending, error);
    }
    fn on_http_failed(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: safe_cex_engine::VenueError,
    ) {
        self.venue.on_http_failed(cx, pending, error);
    }
    fn on_sign_error(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: safe_cex_engine::VenueError,
    ) {
        self.venue.on_sign_error(cx, pending, error);
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        pending: Self::Pending,
        reason: safe_cex_engine::request_context::Error,
        error: safe_cex_engine::VenueError,
    ) {
        self.venue
            .on_http_preparation_failed(cx, pending, reason, error);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.venue.on_stream_error(cx, pending, response);
    }
    fn on_timer(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        timer: safe_cex_engine::TimerId,
    ) {
        self.venue.on_timer(cx, timer);
    }
    fn on_command(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>, command: Command) {
        self.venue.on_command(cx, command);
    }
    fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.venue.books()
    }
}
fn held_spot_token() -> (Engine<SpotDeliveryProbe>, HttpRequest, SocketId) {
    let opts = options(false);
    let (p, ids) = provider(&opts, Some("fixture-spot-token-retirement"), false);
    let mut e = engine(
        SpotDeliveryProbe {
            venue: KrakenSpot::new(opts),
            delivered: 0,
        },
        &p,
        ids,
    );
    let mut token = None;
    let mut private = None;
    for _ in 0..16 {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(r) if r.url.ends_with("GetWebSocketsToken") => {
                    assert!(token.replace(r).is_none(), "exactly one current token call");
                }
                Action::Http(r) => {
                    let body = if r.url.ends_with("AssetPairs") {
                        spot_support::META
                    } else if r.url.ends_with("Ticker") {
                        spot_support::TICKERS
                    } else if r.url.ends_with("Balance") {
                        spot_support::BALANCE
                    } else if r.url.ends_with("OpenOrders") {
                        spot_support::OPEN
                    } else {
                        panic!("unexpected token bootstrap {}", r.url)
                    };
                    e.handle(
                        NOW,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => {
                    assert!(private.replace(socket).is_none());
                    e.handle(NOW, Event::WsOpened { socket });
                }
                _ => {}
            }
        }
    }
    let token = token.expect("genuine signed token request");
    assert_eq!(header(&token, "API-Key"), KEY);
    assert_eq!(token.method, Method::Post);
    (e, token, private.expect("genuine private socket"))
}
fn token_response(e: &mut Engine<SpotDeliveryProbe>, r: &HttpRequest) -> Vec<Action> {
    e.handle(
        NOW + 2.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"error":[],"result":{"token":"fixture-held-token","expires":900}}"#,
            },
        },
    );
    e.drain_actions().collect()
}
#[test]
fn native_live_spot_token_subscribes_both_channels() {
    let (mut e, token, socket) = held_spot_token();
    let before = e.venue().delivered;
    let actions = token_response(&mut e, &token);
    assert_eq!(e.venue().delivered, before + 1);
    let frames: Vec<serde_json::Value> = actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend {
                socket: current,
                text,
            } if *current == socket => Some(serde_json::from_str(text).unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["params"]["channel"], "executions");
    assert_eq!(frames[1]["params"]["channel"], "balances");
    for frame in frames {
        assert_eq!(frame["params"]["token"], "fixture-held-token");
    }
}
#[test]
fn native_closed_spot_socket_never_sends_late_token_subscriptions() {
    let (mut e, token, socket) = held_spot_token();
    e.handle(NOW + 1., Event::WsClosed { socket, code: 1000 });
    e.drain_actions().for_each(drop);
    assert!(
        !token_response(&mut e, &token)
            .iter()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
}
#[test]
fn new_contract_closed_spot_socket_retires_token_callback_authority() {
    let (mut e, token, socket) = held_spot_token();
    e.handle(NOW + 1., Event::WsClosed { socket, code: 1000 });
    e.drain_actions().for_each(drop);
    let before = e.venue().delivered;
    let actions = token_response(&mut e, &token);
    assert!(!actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
    assert_eq!(
        e.venue().delivered,
        before,
        "a retired token Session must fence delivery before the venue callback"
    );
}

fn private_boundary<V: Venue>(engine: &mut Engine<V>) -> Vec<(HttpRequest, bool)> {
    use safe_cex_engine::request_context::HttpAction;
    let mut observed = vec![];
    for _ in 0..16 {
        engine.prepare_dispatch(NOW);
        let actions = engine.drain_actions().collect::<Vec<_>>();
        if actions.is_empty() {
            return observed;
        }
        for action in actions {
            let request = match action {
                Action::Http(request) => {
                    if is_private_http(&request) {
                        observed.push((request, false));
                        continue;
                    }
                    request
                }
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        if is_private_http(&request) {
                            observed.push((request, true));
                        } else {
                            engine.http_prepared(
                                NOW,
                                preparation,
                                safe_cex_engine::request_context::Prepared {
                                    id: request.id.0,
                                    expires_at: None,
                                },
                            );
                        }
                        continue;
                    }
                    HttpAction::Bind {
                        permit, request, ..
                    } => {
                        assert!(!is_private_http(&request), "held private preparation bound");
                        assert!(matches!(
                            engine.claim_http(NOW, permit),
                            Ok(safe_cex_engine::request_context::Claim::Released(_))
                        ));
                        request
                    }
                    HttpAction::BindGroup { .. } => {
                        panic!("public bootstrap declares no mutation barrier")
                    }
                    HttpAction::Cancel { .. } => continue,
                },
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => {
                    engine.handle(NOW, Event::WsOpened { socket });
                    continue;
                }
                _ => continue,
            };
            let body = if request.url.ends_with("AssetPairs") {
                spot_support::META
            } else if request.url.ends_with("Ticker") {
                spot_support::TICKERS
            } else if request.url.ends_with("/instruments") {
                support::META
            } else if request.url.ends_with("/tickers") {
                support::TICKERS
            } else {
                panic!("unexpected public bootstrap request");
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
        }
    }
    panic!("private preparation fixture did not settle");
}
fn is_private_http(request: &HttpRequest) -> bool {
    request.url.contains("/0/private/")
        || request.url.contains("/api/history/")
        || request.url.contains("/accounts")
        || request.url.contains("/openorders")
        || request.url.contains("/openpositions")
        || request.url.contains("/leveragepreferences")
        || request.url.contains("/sendorder")
        || request.url.contains("/cancelorder")
        || request.url.contains("/batchorder")
        || request.url.contains("/editorder")
}

fn assert_private_preparation(boundary: &[(HttpRequest, bool)], path: &str) {
    let requests: Vec<_> = boundary
        .iter()
        .filter(|(request, _)| request.url.contains(path))
        .collect();
    assert_eq!(requests.len(), 1, "the production path must be reached");
    let (request, prepared) = requests[0];
    assert!(prepared, "private work escaped before host preparation");
    assert!(request.signer_headers.is_empty());
    for (name, _) in &request.headers {
        assert!(
            !["apikey", "authent", "nonce", "api-key", "api-sign"]
                .contains(&name.to_ascii_lowercase().as_str()),
            "private preparation already contains authentication"
        );
    }
}

#[test]
fn futures_mutation_waits_for_host_preparation() {
    let (mut engine, provider, _, _) = futures(true);
    let before = provider.count();
    engine.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 73,
            orders: vec![
                serde_json::from_value(serde_json::json!({
                    "symbol": "PF_XBTUSD", "type": "limit", "side": "buy",
                    "amount": 1, "price": 100
                }))
                .unwrap(),
            ],
        }),
    );
    let boundary = private_boundary(&mut engine);
    assert_private_preparation(&boundary, "/derivatives/api/v3/sendorder");
    assert_eq!(provider.count(), before, "held preparation invoked signing");
}

#[test]
fn spot_mutation_waits_for_host_preparation() {
    let (mut engine, provider, _) = spot(false);
    let before = provider.count();
    engine.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 74,
            orders: vec![
                serde_json::from_value(serde_json::json!({
                    "symbol": "XBT/USD", "type": "limit", "side": "buy",
                    "amount": 0.1, "price": 100
                }))
                .unwrap(),
            ],
        }),
    );
    let boundary = private_boundary(&mut engine);
    assert_private_preparation(&boundary, "/0/private/AddOrder");
    assert_eq!(provider.count(), before, "held preparation invoked signing");
}

#[test]
fn futures_history_waits_for_host_preparation() {
    let (mut engine, provider, _, _) = futures(true);
    let before = provider.count();
    engine.handle(
        NOW,
        Event::Command(Command::Query {
            command: 75,
            query: Query::OrderHistory {
                symbol: None,
                from_ms: Some(NOW - 60_000.),
                to_ms: Some(NOW),
                limit: None,
            },
        }),
    );
    let boundary = private_boundary(&mut engine);
    assert_private_preparation(&boundary, "/api/history/v3/orders");
    assert_eq!(provider.count(), before, "held preparation invoked signing");
}
#[test]
fn futures_ordinary_private_send_waits_for_host_preparation() {
    let opts = options(true);
    let (provider, identities) = provider(&opts, Some("ordinary-preparation-subject"), false);
    let mut engine = engine(Kraken::new(opts), &provider, identities);
    let boundary = private_boundary(&mut engine);
    let accounts = boundary
        .iter()
        .filter(|(request, _)| request.url.ends_with("/accounts"))
        .collect::<Vec<_>>();
    assert_eq!(
        accounts.len(),
        1,
        "native account bootstrap must be reached"
    );
    assert!(
        accounts[0].1,
        "ordinary private HTTP escaped before Prepare/Bind/Claim"
    );
}
#[test]
fn futures_streamed_private_send_waits_for_host_preparation() {
    let opts = options(true);
    let (provider, identities) = provider(&opts, Some("streamed-preparation-subject"), false);
    let mut engine = engine(Kraken::new(opts), &provider, identities);
    let boundary = private_boundary(&mut engine);
    for endpoint in ["/openorders", "/openpositions", "/leveragepreferences"] {
        let requests = boundary
            .iter()
            .filter(|(request, _)| request.url.ends_with(endpoint))
            .collect::<Vec<_>>();
        assert_eq!(
            requests.len(),
            1,
            "native streamed account bootstrap must be reached"
        );
        assert!(
            requests[0].1,
            "streamed private HTTP escaped before Prepare/Bind/Claim"
        );
    }
}
#[test]
fn spot_private_post_waits_for_host_preparation() {
    let opts = options(true);
    let (provider, identities) = provider(&opts, Some("spot-preparation-subject"), false);
    let mut engine = engine(KrakenSpot::new(opts), &provider, identities);
    let boundary = private_boundary(&mut engine);
    for endpoint in ["Balance", "OpenOrders", "GetWebSocketsToken"] {
        let requests = boundary
            .iter()
            .filter(|(request, _)| request.url.ends_with(endpoint))
            .collect::<Vec<_>>();
        assert_eq!(
            requests.len(),
            1,
            "native Spot private bootstrap must be reached"
        );
        assert!(
            requests[0].1,
            "Spot private POST escaped before Prepare/Bind/Claim"
        );
    }
}
