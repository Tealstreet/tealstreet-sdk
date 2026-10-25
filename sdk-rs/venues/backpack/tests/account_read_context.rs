//! Native account controls and NEW managed physical Read boundaries.
mod support;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::request_context::{
    Claim, Context, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Method, Notice, Response, SocketId, TimerId,
    Venue, VenueError,
};
use safe_cex_venue_backpack::{Backpack, Options, Pending};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
use support::{BALANCE, MARKETS, ORDER, POSITIONS, SYMBOL};
const START: f64 = 1000.;
const INFO: &str = r#"{"leverageLimit":"10"}"#;
fn ed25519_seeds() -> safe_cex_engine::signing::CredentialSetupPolicy {
    let slot = safe_cex_engine::signing::IdentitySetupPolicy {
        signing_algorithm: Some(safe_cex_engine::signing::Algorithm::Ed25519),
        ..Default::default()
    };
    safe_cex_engine::signing::CredentialSetupPolicy {
        primary: Some(slot),
        secondary: Some(slot),
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Route {
    Info,
    Positions,
    Balance,
    Orders,
}
const ROUTES: [Route; 4] = [Route::Info, Route::Positions, Route::Balance, Route::Orders];
impl Route {
    fn path(self) -> &'static str {
        match self {
            Self::Info => "/api/v1/account",
            Self::Positions => "/api/v1/position",
            Self::Balance => "/api/v1/capital/collateral",
            Self::Orders => "/api/v1/orders",
        }
    }
    fn body(self) -> &'static str {
        match self {
            Self::Info => INFO,
            Self::Positions => POSITIONS,
            Self::Balance => BALANCE,
            Self::Orders => "[]",
        }
    }
    fn instruction(self) -> &'static str {
        match self {
            Self::Info => "accountQuery",
            Self::Positions => "positionQuery",
            Self::Balance => "collateralQuery",
            Self::Orders => "orderQueryAll",
        }
    }
    fn next(self) -> Self {
        ROUTES[ROUTES.iter().position(|r| *r == self).unwrap() + 1]
    }
}
struct Packet {
    request: HttpRequest,
    preparation: Option<Preparation>,
}
fn packets(actions: Vec<Action>) -> Vec<Packet> {
    actions
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(Packet {
                request,
                preparation: None,
            }),
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some(Packet {
                    request,
                    preparation: Some(preparation),
                }),
                _ => None,
            },
            _ => None,
        })
        .collect()
}
fn drain(e: &mut Driver) -> Vec<Packet> {
    packets(e.drain_actions().collect())
}
fn take(all: &mut Vec<Packet>, route: Route) -> Packet {
    let i = all
        .iter()
        .position(|p| p.request.url.ends_with(route.path()))
        .expect("genuine native account route");
    all.remove(i)
}
fn release(e: &mut Driver, p: &Packet, now: f64) -> HttpRequest {
    let Some(preparation) = p.preparation else {
        return p.request.clone();
    };
    e.http_prepared(
        now,
        preparation,
        Prepared {
            id: p.request.id.0,
            expires_at: None,
        },
    );
    let (permit, request) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } if request.id == p.request.id => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("exact prepared Bind");
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn respond(e: &mut Driver, p: &Packet, now: f64, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: p.request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    )
}
fn complete(e: &mut Driver, p: &Packet, now: f64, route: Route) {
    release(e, p, now);
    respond(e, p, now, 200, route.body());
}
fn paths(all: &[Packet]) -> Vec<&str> {
    all.iter()
        .map(|p| {
            p.request
                .url
                .strip_prefix("https://api.backpack.exchange")
                .unwrap()
        })
        .collect()
}
fn no_physical(all: &[Action]) {
    assert!(
        !all.iter().any(|a| match a {
            Action::Http(_) => true,
            Action::ManagedHttp(a) => matches!(
                **a,
                HttpAction::Prepare { .. } | HttpAction::Bind { .. } | HttpAction::BindGroup { .. }
            ),
            _ => false,
        }),
        "no new raw or managed work"
    );
}
fn polls(all: &[Action], now: f64) -> Vec<(TimerId, f64)> {
    all.iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if support::jittered(*at_ms - now, 10_000.) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .collect()
}
#[derive(Default)]
struct State {
    reserve: Cell<Option<usize>>,
    release_one: Cell<bool>,
    before: RefCell<Vec<usize>>,
    after: RefCell<Vec<usize>>,
    released: Cell<Option<usize>>,
    peer_checks: Cell<usize>,
    cleanup: Cell<Option<(TimerId, f64)>>,
}
struct Audit {
    native: Backpack,
    peers: Vec<Context>,
    state: Rc<State>,
}
type Driver = Engine<Audit>;
fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 120_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
impl Audit {
    fn capacity(&self, cx: &mut Cx<'_, Pending>) -> usize {
        let mut held = vec![];
        while let Ok(c) = cx.http_context(Scope::Background, policy()) {
            held.push(c);
            assert!(held.len() <= 256);
        }
        let n = held.len();
        for c in held {
            cx.finish_http_context(&c).unwrap();
        }
        n
    }
    fn check_peers(&self, cx: &Cx<'_, Pending>) {
        for c in &self.peers {
            cx.validate_http_context(c).unwrap();
        }
        self.state
            .peer_checks
            .set(self.state.peer_checks.get() + self.peers.len());
    }
}
impl Venue for Audit {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        let timer = cx.set_timer(60_000.);
        self.state.cleanup.set(Some((timer, cx.now + 60_000.)));
        self.native.start(cx);
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        event: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.native.on_socket_event(cx, event)
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        epoch: safe_cex_engine::socket_lifecycle::Epoch,
        pending: Self::Pending,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.native.on_socket_interests_released(cx, epoch, pending)
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
        self.native
            .encode_socket_interests(store, feed, subscribe, items, emit)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.native.on_open(cx, socket)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.native.on_message(cx, socket, text)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.native.on_close(cx, socket, code)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, item: &str) {
        self.native.on_item(cx, p, item)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        let probe = if let Some(n) = self.state.reserve.take() {
            for _ in 0..n {
                self.peers
                    .push(cx.http_context(Scope::Background, policy()).unwrap());
            }
            self.check_peers(cx);
            self.state.before.borrow_mut().push(self.capacity(cx));
            true
        } else {
            false
        };
        self.native.on_response(cx, p, r);
        if probe {
            self.check_peers(cx);
            self.state.after.borrow_mut().push(self.capacity(cx));
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, e: &str) {
        self.native.on_request_failed(cx, p, e)
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, e: VenueError) {
        self.native.on_http_failed(cx, p, e)
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        p: Pending,
        r: safe_cex_engine::request_context::Error,
        e: VenueError,
    ) {
        self.native.on_http_preparation_failed(cx, p, r, e)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, t: TimerId) {
        if self.state.cleanup.get().is_some_and(|(id, _)| id == t) {
            self.check_peers(cx);
            for c in self.peers.drain(..) {
                cx.finish_http_context(&c).unwrap();
            }
            self.state.released.set(Some(self.capacity(cx)));
        } else {
            if let Some(n) = self.state.reserve.take() {
                for _ in 0..n {
                    self.peers
                        .push(cx.http_context(Scope::Background, policy()).unwrap());
                }
                self.check_peers(cx);
            }
            if self.state.release_one.replace(false) {
                cx.finish_http_context(&self.peers.pop().unwrap()).unwrap();
                self.check_peers(cx);
            }
            self.native.on_timer(cx, t)
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, c: Command) {
        self.native.on_command(cx, c)
    }
    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        plan: &safe_cex_engine::request_context::Plan,
    ) {
        self.native.on_http_plan_ready(cx, plan);
    }
    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.native.on_http_owner_retired(owner);
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        self.native.capture_http_reconciliation(pending)
    }
    fn unproven_http_evidence(
        &self,
        pending: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        self.native
            .unproven_http_evidence(pending, response, evidence);
    }
    fn book(&self, s: &str) -> Option<&safe_cex_engine::book::Book> {
        self.native.book(s)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.native.books()
    }
}
fn fresh(secondary: bool) -> (Driver, Rc<State>) {
    fresh_material(secondary, false)
}
fn fresh_material(secondary: bool, different_raw: bool) -> (Driver, Rc<State>) {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    let primary = STANDARD.encode([7; 32]);
    let read = STANDARD.encode([9; 32]);
    let local = |key: &str, seed: &str| LocalIdentity {
        key: Secret::new(key.into()),
        secret: Secret::new(seed.into()),
        secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
        ..Default::default()
    };
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(local("test-public-key", &primary)),
            secondary: secondary.then(|| local("account-read-key", &read)),
        },
        ed25519_seeds(),
    )
    .unwrap();
    let options = Options {
        identity: identities.primary.clone(),
        read_identity: identities.secondary.clone(),
        read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
            key: if different_raw {
                "different-raw-key"
            } else {
                "account-read-key"
            }
            .into(),
            secret: if different_raw {
                STANDARD.encode([8; 32])
            } else {
                read
            },
            passphrase: None,
        }),
        read_credentials_status: secondary.then_some(Ok(())),
        key: Some("test-public-key".into()),
        secret: Some(primary),
        recv_window: Some(5000.),
        ..Default::default()
    };
    let state = Rc::new(State::default());
    let mut e = safe_cex_engine::test_support::seeded_engine(Audit {
        native: Backpack::new(options),
        peers: vec![],
        state: state.clone(),
    });
    e.set_signer(Some(Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    e.start(START);
    (e, state)
}
fn catalogue(e: &mut Driver) -> Packet {
    let market = drain(e).pop().unwrap();
    assert!(market.request.url.ends_with("/markets"));
    release(e, &market, START);
    respond(e, &market, START, 200, MARKETS);
    let mut all = drain(e);
    let info = take(&mut all, Route::Info);
    assert_eq!(all.len(), 2);
    for p in all {
        release(e, &p, START);
        respond(
            e,
            &p,
            START,
            200,
            if p.request.url.ends_with("/tickers") {
                r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.01","volume":"10","quoteVolume":"1410"}]"#
            } else {
                "[]"
            },
        );
        e.drain_actions().for_each(drop);
    }
    info
}
fn issued(route: Route, released: bool) -> (Driver, Packet, Rc<State>) {
    let (mut e, state) = fresh(false);
    let mut p = catalogue(&mut e);
    for current in ROUTES {
        if current == route {
            if released {
                release(&mut e, &p, START);
            }
            return (e, p, state);
        }
        complete(&mut e, &p, START, current);
        p = take(&mut drain(&mut e), current.next());
    }
    unreachable!()
}
fn unpublished(e: &Driver, route: Route) {
    match route {
        Route::Info => assert_eq!(
            e.store().setting(SYMBOL).unwrap().leverage,
            1.,
            "unadmitted Info cannot publish leverage"
        ),
        Route::Positions => assert!(
            !e.store().loaded().positions,
            "unadmitted positions cannot publish"
        ),
        Route::Balance => assert!(
            !e.store().loaded().balance,
            "unadmitted balance cannot publish"
        ),
        Route::Orders => assert!(
            !e.store().loaded().orders,
            "unadmitted orders cannot publish"
        ),
    }
}
fn prepared(route: Route) {
    let (mut e, p, _) = issued(route, false);
    assert!(
        p.preparation.is_some(),
        "{route:?} must issue genuine managed Prepare"
    );
    complete(&mut e, &p, START, route);
}
fn held(route: Route) {
    let (mut e, p, _) = issued(route, false);
    respond(&mut e, &p, START + 1., 200, route.body());
    unpublished(&e, route);
}
fn equality(route: Route) {
    let (mut e, p, _) = issued(route, true);
    respond(&mut e, &p, START + 20_000., 200, route.body());
    unpublished(&e, route);
    let all = e.drain_actions().collect::<Vec<_>>();
    no_physical(&all);
    assert_eq!(
        polls(&all, START + 20_000.).len(),
        1,
        "expired owned Pending must schedule one delayed observation"
    );
}
fn nonfinite(route: Route) {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let (mut e, p, _) = issued(route, true);
        respond(&mut e, &p, now, 200, route.body());
        unpublished(&e, route);
        let all = e.drain_actions().collect::<Vec<_>>();
        no_physical(&all);
        assert!(!all.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    }
}
macro_rules! originals {
    ($p:ident,$h:ident,$x:ident,$n:ident,$route:expr) => {
        #[test]
        fn $p() {
            prepared($route)
        }
        #[test]
        fn $h() {
            held($route)
        }
        #[test]
        fn $x() {
            equality($route)
        }
        #[test]
        fn $n() {
            nonfinite($route)
        }
    };
}
originals!(
    info_prepare_original,
    info_held_data_original,
    info_new_expiry_original,
    info_nonfinite_original,
    Route::Info
);
originals!(
    positions_prepare_original,
    positions_held_data_original,
    positions_new_expiry_original,
    positions_nonfinite_original,
    Route::Positions
);
originals!(
    balance_prepare_original,
    balance_held_data_original,
    balance_new_expiry_original,
    balance_nonfinite_original,
    Route::Balance
);
originals!(
    orders_prepare_original,
    orders_held_data_original,
    orders_new_expiry_original,
    orders_nonfinite_original,
    Route::Orders
);
#[test]
fn native_all_four_routes_keep_real_key_window_wire_and_chain() {
    for secondary in [false, true] {
        let (mut e, _) = fresh(secondary);
        let mut p = catalogue(&mut e);
        let mut native = safe_cex_venue_backpack::sign::Credentials::new(
            if secondary {
                "account-read-key"
            } else {
                "test-public-key"
            }
            .into(),
            &STANDARD.encode([if secondary { 9 } else { 7 }; 32]),
        )
        .unwrap();
        native.window = 5000;
        for route in ROUTES {
            assert_eq!(p.request.method, Method::Get);
            assert_eq!(
                p.request.url,
                format!("https://api.backpack.exchange{}", route.path())
            );
            assert_eq!(p.request.body, None);
            let wire = release(&mut e, &p, START);
            assert_eq!(
                wire.headers,
                native.headers(route.instruction(), &[Default::default()], START)
            );
            respond(&mut e, &p, START, 200, route.body());
            let all = e.drain_actions().collect::<Vec<_>>();
            if route != Route::Orders {
                let mut next = packets(all);
                assert_eq!(next.len(), 1);
                p = next.pop().unwrap();
            } else {
                assert_eq!(polls(&all, START).len(), 1);
                assert!(all.iter().any(|a| matches!(
                    a,
                    Action::WsConnect {
                        channel: Some(safe_cex_engine::SocketChannel::Private),
                        ..
                    }
                )));
            }
        }
        assert!(
            e.store().loaded().positions && e.store().loaded().balance && e.store().loaded().orders
        );
        assert_eq!(e.store().balance().total, 1005.);
        assert_eq!(e.store().positions()[0].contracts, 5.);
    }
}
#[test]
fn native_public_only_and_missing_primary_do_not_start_account_chain() {
    for public_data_only in [false, true] {
        let state = Rc::new(State::default());
        let mut e = safe_cex_engine::test_support::seeded_engine(Audit {
            native: Backpack::new(Options {
                public_data_only,
                ..Default::default()
            }),
            peers: vec![],
            state,
        });
        e.start(START);
        let market = drain(&mut e).pop().unwrap();
        release(&mut e, &market, START);
        respond(&mut e, &market, START, 200, MARKETS);
        let all = e.drain_actions().collect::<Vec<_>>();
        assert!(!all.iter().any(|a|matches!(a,Action::Notice(Notice::Event(n))if n.message.contains("credentials")||n.message.contains("account:613"))));
        let work = packets(all);
        assert_eq!(work.len(), 2);
        assert!(
            work.iter()
                .all(|p| p.request.url.ends_with("/tickers")
                    || p.request.url.ends_with("/markPrices"))
        );
        assert_eq!(e.store().balance().total, 0.);
        assert!(e.store().positions().is_empty() && e.store().orders().is_empty());
    }
}
#[test]
fn native_bad_read_seed_keeps_startup_config_refusal_without_account_recovery() {
    let seed = STANDARD.encode([7; 32]);
    for invalid in ["%%%".into(), STANDARD.encode([9; 31])] {
        let state = Rc::new(State::default());
        let mut e = safe_cex_engine::test_support::seeded_engine(Audit {
            native: Backpack::new(Options {
                key: Some("test-public-key".into()),
                secret: Some(seed.clone()),
                read_credentials: Some(safe_cex_engine::ReadCredentials {
                    key: "read".into(),
                    secret: invalid,
                    passphrase: None,
                }),
                ..Default::default()
            }),
            peers: vec![],
            state,
        });
        e.start(START);
        let all = e.drain_actions().collect::<Vec<_>>();
        no_physical(&all);
        assert_eq!(polls(&all, START).len(), 0);
        let config = all
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(n))
                    if n.code == safe_cex_engine::log::code::CONFIG_INVALID =>
                {
                    Some(n)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(config.len(), 1);
        assert_eq!(config[0].level, safe_cex_engine::Level::Error);
        assert!(matches!(
            config[0].message.as_str(),
            "Invalid Backpack base64 seed" | "Ed25519 seed must be 32 bytes"
        ));
    }
}
#[test]
fn native_transport_status_and_parse_failures_keep_next_and_notice() {
    for route in ROUTES {
        for failure in 0..3 {
            let (mut e, p, _) = issued(route, true);
            if failure == 0 {
                e.handle(
                    START + 1.,
                    Event::HttpFailed {
                        request: p.request.id,
                        error: "native transport sentinel",
                    },
                );
            } else {
                respond(
                    &mut e,
                    &p,
                    START + 1.,
                    if failure == 1 { 503 } else { 200 },
                    if failure == 1 { "unavailable" } else { "{" },
                );
            }
            let all = e.drain_actions().collect::<Vec<_>>();
            let events = all
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::Event(n)) => Some(n),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(!events.is_empty(), "{route:?} failure{failure}");
            assert_eq!(events.len(), 1, "{route:?} failure{failure}: {events:?}");
            let expected_code = if failure == 2 && matches!(route, Route::Info | Route::Balance) {
                safe_cex_engine::log::code::STREAM_MALFORMED
            } else {
                safe_cex_engine::log::code::REQUEST_FAILED
            };
            let expected_message = match failure {
                0 => "native transport sentinel".to_owned(),
                1 => "Backpack HTTP 503: unavailable".to_owned(),
                _ if matches!(route, Route::Positions | Route::Orders) => {
                    "response did not contain the expected list: {".to_owned()
                }
                _ => serde_json::from_str::<serde_json::Value>("{")
                    .unwrap_err()
                    .to_string(),
            };
            assert_eq!(events[0].code, expected_code);
            assert_eq!(events[0].message, expected_message);
            assert_eq!(events[0].level, safe_cex_engine::Level::Error);
            let work = packets(all);
            if route == Route::Orders {
                assert!(work.is_empty());
            } else {
                assert_eq!(work.len(), 1);
                assert_eq!(paths(&work), [route.next().path()]);
            }
        }
    }
}
fn complete_startup() -> (Driver, SocketId, (TimerId, f64), Rc<State>) {
    let (mut e, state) = fresh(false);
    let mut p = catalogue(&mut e);
    for route in ROUTES {
        complete(&mut e, &p, START, route);
        let all = e.drain_actions().collect::<Vec<_>>();
        if route == Route::Orders {
            let socket = all
                .iter()
                .find_map(|a| match a {
                    Action::WsConnect {
                        socket,
                        channel: Some(safe_cex_engine::SocketChannel::Private),
                        ..
                    } => Some(*socket),
                    _ => None,
                })
                .unwrap();
            return (e, socket, polls(&all, START).pop().unwrap(), state);
        }
        p = packets(all).pop().unwrap();
    }
    unreachable!()
}
fn open_for_overlap(e: &mut Driver, socket: SocketId) {
    e.handle(START, Event::WsOpened { socket });
    for packet in drain(e) {
        let route = if packet.request.url.ends_with(Route::Orders.path()) {
            Route::Orders
        } else if packet.request.url.ends_with(Route::Positions.path()) {
            Route::Positions
        } else {
            Route::Balance
        };
        complete(e, &packet, START, route);
        e.drain_actions().for_each(drop);
    }
}
fn reopen_for_overlap(e: &mut Driver, socket: SocketId, now: f64) -> (SocketId, f64) {
    e.handle(now, Event::WsClosed { socket, code: 1006 });
    let (timer, at) = e
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .expect("actual private reconnect timer");
    e.handle(at, Event::TimerFired { timer });
    let socket = e
        .drain_actions()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(socket),
            _ => None,
        })
        .expect("actual replacement private socket");
    e.handle(at, Event::WsOpened { socket });
    (socket, at)
}
fn poll_to(e: &mut Driver, tick: (TimerId, f64), route: Route) -> Packet {
    e.handle(tick.1, Event::TimerFired { timer: tick.0 });
    let mut p = take(&mut drain(e), Route::Info);
    for current in ROUTES {
        if current == route {
            return p;
        }
        complete(e, &p, tick.1, current);
        p = take(&mut drain(e), current.next());
    }
    unreachable!()
}
fn fill(e: &mut Driver, socket: SocketId, now: f64, id: u32) -> Vec<Packet> {
    let text = format!(
        r#"{{"stream":"account.orderUpdate","data":{{"e":"orderFill","i":"123","c":42,"E":{},"X":"PartiallyFilled","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"{id}","l":"1","L":"139.5","t":{id},"f":"GTC"}}}}"#,
        now * 1000.
    );
    e.handle(
        now,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    drain(e)
}
#[test]
fn native_live_stale_positions_keep_next_without_publication() {
    let (mut e, socket, tick, _) = complete_startup();
    open_for_overlap(&mut e, socket);
    let p = poll_to(&mut e, tick, Route::Positions);
    let (_, observation) = reopen_for_overlap(&mut e, socket, tick.1 + 1.);
    let newer = drain(&mut e);
    assert!(
        newer
            .iter()
            .any(|p| p.request.url.ends_with(Route::Positions.path()))
    );
    release(&mut e, &p, observation + 1.);
    respond(
        &mut e,
        &p,
        observation + 1.,
        200,
        &POSITIONS.replace("\"netQuantity\":\"5\"", "\"netQuantity\":\"9\""),
    );
    assert_eq!(e.store().positions()[0].contracts, 5.);
    assert_eq!(paths(&drain(&mut e)), [Route::Balance.path()]);
}
#[test]
fn native_live_stale_info_keeps_positions_after_leverage_write() {
    let (mut e, _, tick, _) = complete_startup();
    let p = poll_to(&mut e, tick, Route::Info);
    e.handle(
        tick.1 + 1.,
        Event::Command(Command::SetLeverage {
            command: 71,
            symbol: SYMBOL.into(),
            leverage: 20.,
            side: None,
        }),
    );
    let mut writes = drain(&mut e);
    assert_eq!(writes.len(), 1);
    let write = writes.pop().unwrap();
    assert_eq!(write.request.method, Method::Patch);
    release(&mut e, &write, tick.1 + 1.);
    respond(&mut e, &write, tick.1 + 2., 200, "{}");
    e.drain_actions().for_each(drop);
    release(&mut e, &p, tick.1 + 3.);
    respond(&mut e, &p, tick.1 + 3., 200, INFO);
    assert_eq!(e.store().setting(SYMBOL).unwrap().leverage, 20.);
    assert_eq!(paths(&drain(&mut e)), [Route::Positions.path()]);
}
#[test]
fn native_rejected_order_token_keeps_finish_and_suppresses_repairs() {
    let (mut e, _, tick, _) = complete_startup();
    let p = poll_to(&mut e, tick, Route::Orders);
    let before = e.store().orders().to_vec();
    e.store_mut().invalidate_order_snapshots();
    let repair = ORDER.replace(
        "\"orderType\":\"Limit\"",
        "\"orderType\":\"Market\",\"triggerPrice\":\"130\"",
    );
    release(&mut e, &p, tick.1 + 1.);
    respond(&mut e, &p, tick.1 + 1., 200, &format!("[{repair}]"));
    assert_eq!(e.store().orders(), before);
    let all = e.drain_actions().collect::<Vec<_>>();
    assert!(
        packets(all.clone()).is_empty(),
        "rejected token cannot start financial repairs"
    );
    assert_eq!(
        polls(&all, tick.1 + 1.).len(),
        1,
        "live rejected snapshot keeps Finish"
    );
}
fn balance_overlap() -> (Driver, Packet, Rc<State>, f64) {
    let (mut e, socket, tick, state) = complete_startup();
    open_for_overlap(&mut e, socket);
    let p = poll_to(&mut e, tick, Route::Balance);
    let (socket, observation) = reopen_for_overlap(&mut e, socket, tick.1 + 1.);
    let origin = observation - 1.;
    let mut work = drain(&mut e);
    work.extend(fill(&mut e, socket, origin + 1000., 1));
    work.extend(fill(&mut e, socket, origin + 2000., 2));
    let mut balances = vec![];
    let now = origin + 2001.;
    for packet in work {
        if packet.request.url.ends_with(Route::Balance.path()) {
            balances.push(packet);
        } else {
            let route = if packet.request.url.ends_with(Route::Orders.path()) {
                Route::Orders
            } else {
                Route::Positions
            };
            complete(&mut e, &packet, now, route);
            e.drain_actions().for_each(drop);
        }
    }
    assert_eq!(balances.len(), 3);
    for packet in balances.drain(..2) {
        release(&mut e, &packet, now);
        respond(&mut e, &packet, now, 200, &BALANCE.replace("1005", "9000"));
        assert_eq!(e.store().balance().total, 1005.);
        assert!(
            drain(&mut e).is_empty(),
            "two stale independent observations are discarded"
        );
    }
    let last = balances.pop().unwrap();
    release(&mut e, &last, now);
    e.handle(
        now,
        Event::HttpFailed {
            request: last.request.id,
            error: "retire independent fill observation",
        },
    );
    e.drain_actions().for_each(drop);
    (e, p, state, now)
}
#[test]
fn native_balance_two_discards_then_stale_acceptance_keeps_corrective_before_next() {
    let (mut e, p, _, now) = balance_overlap();
    release(&mut e, &p, now);
    respond(&mut e, &p, now, 200, &BALANCE.replace("1005", "2003"));
    assert_eq!(e.store().balance().total, 2003.);
    assert_eq!(
        paths(&drain(&mut e)),
        [Route::Balance.path(), Route::Orders.path()]
    );
}
#[test]
fn single_credit_retires_predecessor_before_successor_with_255_real_peers() {
    let (mut e, p, state) = issued(Route::Info, true);
    state.reserve.set(Some(255));
    respond(&mut e, &p, START + 1., 200, INFO);
    assert_eq!(
        e.store().setting(SYMBOL).unwrap().leverage,
        10.,
        "publication happens under live DATA"
    );
    assert_eq!(
        &*state.before.borrow(),
        &[0],
        "255 peers plus predecessor fill all credits"
    );
    assert_eq!(
        &*state.after.borrow(),
        &[0],
        "successor uses the freed credit"
    );
    let mut next = take(&mut drain(&mut e), Route::Positions);
    assert!(next.preparation.is_some());
    for route in [Route::Positions, Route::Balance, Route::Orders] {
        complete(&mut e, &next, START + 2., route);
        let actions = e.drain_actions().collect::<Vec<_>>();
        if route != Route::Orders {
            next = packets(actions).pop().unwrap();
        }
    }
    let (timer, due) = state.cleanup.get().unwrap();
    e.handle(due, Event::TimerFired { timer });
    assert_eq!(state.released.get(), Some(256));
    assert!(state.peer_checks.get() >= 255 * 3);
}
#[test]
fn balance_retires_before_ordered_two_credit_fanout_with_254_real_peers() {
    let (mut e, p, state, now) = balance_overlap();
    release(&mut e, &p, now);
    state.reserve.set(Some(254));
    respond(&mut e, &p, now, 200, &BALANCE.replace("1005", "2003"));
    assert_eq!(e.store().balance().total, 2003.);
    assert_eq!(
        &*state.before.borrow(),
        &[1],
        "254 peers plus predecessor leave one spare"
    );
    assert_eq!(
        &*state.after.borrow(),
        &[0],
        "retirement provides second fanout credit"
    );
    let work = drain(&mut e);
    assert_eq!(paths(&work), [Route::Balance.path(), Route::Orders.path()]);
    assert!(work.iter().all(|p| p.preparation.is_some()));
    for packet in work {
        complete(
            &mut e,
            &packet,
            now,
            if packet.request.url.ends_with(Route::Balance.path()) {
                Route::Balance
            } else {
                Route::Orders
            },
        );
        e.drain_actions().for_each(drop);
    }
    let (timer, due) = state.cleanup.get().unwrap();
    e.handle(due, Event::TimerFired { timer });
    assert_eq!(state.released.get(), Some(256));
}
#[test]
fn new_capacity_admission_delays_observation_without_immediate_account_chain() {
    let (mut e, _, original_tick, state) = complete_startup();
    state.reserve.set(Some(256));
    e.handle(
        original_tick.1,
        Event::TimerFired {
            timer: original_tick.0,
        },
    );
    let all = e.drain_actions().collect::<Vec<_>>();
    no_physical(&all);
    let tick = polls(&all, original_tick.1);
    assert_eq!(
        tick.len(),
        1,
        "full capacity coalesces one delayed observation, no immediate chain"
    );
    assert!(state.peer_checks.get() >= 256);
    state.release_one.set(true);
    e.handle(tick[0].1, Event::TimerFired { timer: tick[0].0 });
    let info = take(&mut drain(&mut e), Route::Info);
    assert!(
        info.preparation.is_some(),
        "a distinct later observation may use the genuinely returned credit"
    );
}

#[test]
fn new_precommit_failure_recovers_once_without_data_or_immediate_next() {
    let (mut e, p, _) = issued(Route::Info, false);
    assert!(
        p.preparation.is_some(),
        "precommit control needs managed physical boundary"
    );
    let preparation = p.preparation.unwrap();
    e.http_preparation_failed(
        START + 1.,
        preparation,
        safe_cex_engine::request_context::Error::Capacity,
        VenueError::new("transient prepared resource refusal"),
    );
    unpublished(&e, Route::Info);
    let all = e.drain_actions().collect::<Vec<_>>();
    no_physical(&all);
    assert_eq!(polls(&all, START + 1.).len(), 1);
    e.http_preparation_failed(
        START + 2.,
        preparation,
        safe_cex_engine::request_context::Error::Capacity,
        VenueError::new("duplicate resource refusal"),
    );
    let all = e.drain_actions().collect::<Vec<_>>();
    no_physical(&all);
    assert!(!all.iter().any(|a| matches!(a, Action::SetTimer { .. })));
}
#[test]
fn engine_owned_expiry_uses_returned_pending_once_after_data_authority_ends() {
    let (mut e, p, _) = issued(Route::Info, true);
    respond(&mut e, &p, START + 20_000., 200, INFO);
    unpublished(&e, Route::Info);
    let all = e.drain_actions().collect::<Vec<_>>();
    no_physical(&all);
    let tick = polls(&all, START + 20_000.);
    assert_eq!(
        tick.len(),
        1,
        "engine-owned expiry keeps once-only failure cleanup"
    );
    respond(&mut e, &p, START + 20_001., 200, INFO);
    let all = e.drain_actions().collect::<Vec<_>>();
    no_physical(&all);
    assert!(!all.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    e.handle(tick[0].1, Event::TimerFired { timer: tick[0].0 });
    let next = take(&mut drain(&mut e), Route::Info);
    assert_ne!(next.request.id, p.request.id);
    assert!(next.preparation.is_some());
}

fn provider_material(route: Route) {
    let (mut e, _) = fresh_material(true, true);
    let mut p = catalogue(&mut e);
    for current in ROUTES {
        if current == route {
            break;
        }
        complete(&mut e, &p, START, current);
        p = take(&mut drain(&mut e), current.next());
    }
    let mut selected = safe_cex_venue_backpack::sign::Credentials::new(
        "account-read-key".into(),
        &STANDARD.encode([9; 32]),
    )
    .unwrap();
    let mut raw = safe_cex_venue_backpack::sign::Credentials::new(
        "different-raw-key".into(),
        &STANDARD.encode([8; 32]),
    )
    .unwrap();
    selected.window = 5000;
    raw.window = 5000;
    let expected = selected.headers(route.instruction(), &[Default::default()], START);
    assert_ne!(
        expected,
        raw.headers(route.instruction(), &[Default::default()], START)
    );
    let request = release(&mut e, &p, START);
    assert_eq!(
        request.headers, expected,
        "physical account GET must sign with the selected installed provider, not raw read material"
    );
    respond(&mut e, &p, START, 200, route.body());
}
#[test]
fn info_selected_provider_material_original() {
    provider_material(Route::Info);
}
#[test]
fn positions_selected_provider_material_original() {
    provider_material(Route::Positions);
}
#[test]
fn balance_selected_provider_material_original() {
    provider_material(Route::Balance);
}
#[test]
fn orders_selected_provider_material_original() {
    provider_material(Route::Orders);
}

#[test]
fn native_balance_older_than_committed_never_wins_after_two_more_discards() {
    let (mut e, socket, tick, _) = complete_startup();
    open_for_overlap(&mut e, socket);
    let old = poll_to(&mut e, tick, Route::Balance);
    let (socket, observation) = reopen_for_overlap(&mut e, socket, tick.1 + 1.);
    let origin = observation - 1.;
    let mut work = drain(&mut e);
    work.extend(fill(&mut e, socket, origin + 1000., 1));
    work.extend(fill(&mut e, socket, origin + 2000., 2));
    let mut balances = Vec::new();
    let now = origin + 2001.;
    for packet in work {
        if packet.request.url.ends_with(Route::Balance.path()) {
            balances.push(packet);
        } else {
            let route = if packet.request.url.ends_with(Route::Orders.path()) {
                Route::Orders
            } else {
                Route::Positions
            };
            complete(&mut e, &packet, now, route);
            e.drain_actions().for_each(drop);
        }
    }
    assert_eq!(balances.len(), 3);
    for p in balances.drain(..2) {
        complete(&mut e, &p, now, Route::Balance);
        assert_eq!(e.store().balance().total, 1005.);
        assert!(drain(&mut e).is_empty());
    }
    let mut third = fill(&mut e, socket, origin + 3000., 3);
    let third_balance = take(&mut third, Route::Balance);
    let newer = balances.pop().unwrap();
    release(&mut e, &newer, origin + 3001.);
    respond(
        &mut e,
        &newer,
        origin + 3001.,
        200,
        &BALANCE.replace("1005", "2005"),
    );
    assert_eq!(e.store().balance().total, 2005.);
    let corrective = take(&mut drain(&mut e), Route::Balance);
    let fourth = fill(&mut e, socket, origin + 4000., 4);
    assert!(
        fourth
            .iter()
            .any(|p| p.request.url.ends_with(Route::Balance.path()))
    );
    for p in [corrective, third_balance] {
        complete(&mut e, &p, origin + 4001., Route::Balance);
        assert_eq!(e.store().balance().total, 2005.);
        assert!(drain(&mut e).is_empty());
    }
    release(&mut e, &old, origin + 4002.);
    respond(
        &mut e,
        &old,
        origin + 4002.,
        200,
        &BALANCE.replace("1005", "9999"),
    );
    assert_eq!(
        e.store().balance().total,
        2005.,
        "older-than-committed cannot use the two-discard escape"
    );
    assert_eq!(
        paths(&drain(&mut e)),
        [Route::Orders.path()],
        "live old request still keeps its original Next"
    );
}
#[test]
fn held_streamed_rows_cannot_publish_diagnostics() {
    for route in [Route::Positions, Route::Orders] {
        let (mut e, p, _) = issued(route, false);
        respond(&mut e, &p, START + 1., 200, "[{}]");
        let all = e.drain_actions().collect::<Vec<_>>();
        assert!(!all.iter().any(|a|matches!(a,Action::Notice(Notice::Event(n)) if n.code==safe_cex_engine::log::code::ROW_SKIPPED)),"held DATA cannot publish row diagnostics");
    }
}
#[test]
fn native_duplicate_response_cannot_republish_or_replay_continuation() {
    for route in ROUTES {
        let (mut e, p, _) = issued(route, true);
        respond(&mut e, &p, START + 1., 200, route.body());
        e.drain_actions().for_each(drop);
        let before = e.account_snapshot_frame(START + 2.).to_vec();
        respond(&mut e, &p, START + 2., 200, "[{}]");
        assert_eq!(e.account_snapshot_frame(START + 2.), before);
        let all = e.drain_actions().collect::<Vec<_>>();
        no_physical(&all);
        assert!(!all.iter().any(|a| matches!(
            a,
            Action::SetTimer { .. } | Action::Notice(Notice::Event(_))
        )));
    }
}

#[test]
fn native_public_only_with_real_saved_primary_and_secondary_never_starts_account() {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    for secondary in [false, true] {
        let primary = STANDARD.encode([7; 32]);
        let read = STANDARD.encode([9; 32]);
        let local = |key: &str, seed: &str| LocalIdentity {
            key: Secret::new(key.into()),
            secret: Secret::new(seed.into()),
            secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
            ..Default::default()
        };
        let (provider, identities) = LocalSigner::from_setup_with_policy(
            LocalSetup {
                primary: Some(local("saved-primary", &primary)),
                secondary: secondary.then(|| local("saved-read", &read)),
            },
            ed25519_seeds(),
        )
        .unwrap();
        assert!(identities.primary.is_some());
        assert_eq!(identities.secondary.is_some(), secondary);
        let options = Options {
            public_data_only: true,
            identity: identities.primary.clone(),
            read_identity: identities.secondary.clone(),
            key: Some("saved-primary".into()),
            secret: Some(primary),
            read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
                key: "saved-read".into(),
                secret: read,
                passphrase: None,
            }),
            read_credentials_status: secondary.then_some(Ok(())),
            ..Default::default()
        };
        let state = Rc::new(State::default());
        let mut e = safe_cex_engine::test_support::seeded_engine(Audit {
            native: Backpack::new(options),
            peers: vec![],
            state,
        });
        e.set_signer(Some(Arc::new(provider))).unwrap();
        e.set_identities(identities).unwrap();
        e.start(START);
        let market = drain(&mut e).pop().unwrap();
        assert!(market.request.url.ends_with("/markets"));
        release(&mut e, &market, START);
        respond(&mut e, &market, START, 200, MARKETS);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::WsConnect {
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            }
        )));
        assert!(polls(&actions, START).is_empty());
        let work = packets(actions);
        assert_eq!(work.len(), 2);
        for packet in work {
            assert!(
                packet.request.url.ends_with("/tickers")
                    || packet.request.url.ends_with("/markPrices")
            );
            assert!(
                !packet
                    .request
                    .headers
                    .iter()
                    .any(|(key, _)| key.eq_ignore_ascii_case("X-API-Key"))
            );
            release(&mut e, &packet, START);
            respond(
                &mut e,
                &packet,
                START,
                200,
                if packet.request.url.ends_with("/tickers") {
                    r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.01","volume":"10","quoteVolume":"1410"}]"#
                } else {
                    "[]"
                },
            );
            let after = e.drain_actions().collect::<Vec<_>>();
            no_physical(&after);
            assert!(!after.iter().any(|a| matches!(
                a,
                Action::WsConnect {
                    channel: Some(safe_cex_engine::SocketChannel::Private),
                    ..
                }
            )));
            assert!(polls(&after, START).is_empty());
        }
        assert!(e.store().loaded().markets && e.store().loaded().tickers);
        assert_eq!(e.store().balance().total, 0.);
        assert!(e.store().orders().is_empty() && e.store().positions().is_empty());
    }
}

#[test]
fn typed_precommit_matrix_preserves_permanent_refusal_and_new_delayed_recovery() {
    use safe_cex_engine::{ErrorKind, request_context::Error};
    for route in [Route::Info, Route::Positions, Route::Balance, Route::Orders] {
        for (reason, kind, recovery) in [
            (Error::Capacity, None, true),
            (Error::TimedOut, None, true),
            (Error::Invalid, Some(ErrorKind::Internal), true),
            (Error::Invalid, Some(ErrorKind::Network), true),
            (Error::Invalid, Some(ErrorKind::Timeout), true),
            (Error::Invalid, Some(ErrorKind::InvalidInput), false),
            (Error::Invalid, Some(ErrorKind::Unsupported), false),
            (Error::State, Some(ErrorKind::Internal), false),
            (Error::Revoked, Some(ErrorKind::Network), false),
            (Error::Stale, None, false),
            (Error::Duplicate, None, false),
            (Error::RetryForbidden, None, false),
        ] {
            let (mut e, packet, _) = issued(route, false);
            let preparation = packet.preparation.expect("actual managed preparation");
            let mut error = VenueError::new("same message for every typed disposition");
            error.kind = kind;
            e.http_preparation_failed(START + 1., preparation, reason, error);
            unpublished(&e, route);
            let actions = e.drain_actions().collect::<Vec<_>>();
            no_physical(&actions);
            assert_eq!(
                polls(&actions, START + 1.).len(),
                usize::from(recovery),
                "{route:?} {reason:?} {kind:?}"
            );
            e.http_preparation_failed(
                START + 2.,
                preparation,
                reason,
                VenueError::new("duplicate"),
            );
            let duplicate = e.drain_actions().collect::<Vec<_>>();
            no_physical(&duplicate);
            assert!(
                !duplicate
                    .iter()
                    .any(|action| matches!(action, Action::SetTimer { .. }))
            );
        }
    }
}

#[test]
fn typed_precommit_nonfinite_clocks_never_schedule_recovery() {
    use safe_cex_engine::request_context::Error;
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for route in [Route::Info, Route::Positions, Route::Balance, Route::Orders] {
            let (mut e, packet, _) = issued(route, false);
            e.http_preparation_failed(
                now,
                packet.preparation.unwrap(),
                Error::Capacity,
                VenueError::new("capacity"),
            );
            unpublished(&e, route);
            let actions = e.drain_actions().collect::<Vec<_>>();
            no_physical(&actions);
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::SetTimer { .. }))
            );
        }
    }
}

struct AccountProviderFailure {
    inner: safe_cex_engine::signing::Provider,
    error: safe_cex_engine::signing::SignError,
}
impl safe_cex_engine::signing::Signer for AccountProviderFailure {
    fn sign(
        &self,
        _request: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        let error = self.error;
        Box::pin(async move { Err(error) })
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(
        &self,
        _request: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        Some(Err(self.error))
    }
    fn can_sign(
        &self,
        key: &safe_cex_engine::signing::KeyRef,
        algorithm: safe_cex_engine::signing::Algorithm,
    ) -> safe_cex_engine::signing::Capability {
        self.inner.can_sign(key, algorithm)
    }
}
fn fresh_provider_failure(error: safe_cex_engine::signing::SignError) -> (Driver, Rc<State>) {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    let secondary = true;
    let different_raw = false;
    let primary = STANDARD.encode([7; 32]);
    let read = STANDARD.encode([9; 32]);
    let local = |key: &str, seed: &str| LocalIdentity {
        key: Secret::new(key.into()),
        secret: Secret::new(seed.into()),
        secret_encoding: safe_cex_engine::signing::KeyEncoding::Base64,
        ..Default::default()
    };
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(local("test-public-key", &primary)),
            secondary: secondary.then(|| local("account-read-key", &read)),
        },
        ed25519_seeds(),
    )
    .unwrap();
    let options = Options {
        identity: identities.primary.clone(),
        read_identity: identities.secondary.clone(),
        read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
            key: if different_raw {
                "different-raw-key"
            } else {
                "account-read-key"
            }
            .into(),
            secret: if different_raw {
                STANDARD.encode([8; 32])
            } else {
                read
            },
            passphrase: None,
        }),
        read_credentials_status: secondary.then_some(Ok(())),
        key: Some("test-public-key".into()),
        secret: Some(primary),
        recv_window: Some(5000.),
        ..Default::default()
    };
    let state = Rc::new(State::default());
    let mut e = safe_cex_engine::test_support::seeded_engine(Audit {
        native: Backpack::new(options),
        peers: vec![],
        state: state.clone(),
    });
    e.set_signer(Some(Arc::new(AccountProviderFailure {
        inner: Arc::new(provider),
        error,
    })))
    .unwrap();
    e.set_identities(identities).unwrap();
    e.start(START);
    (e, state)
}
#[test]
fn real_provider_failures_follow_the_engine_typed_reason_without_renewal() {
    use safe_cex_engine::signing::SignError;
    for error in [
        SignError::Refused,
        SignError::Unsupported,
        SignError::InvalidRequest,
        SignError::MissingKey,
        SignError::Unavailable,
        SignError::InvalidResult,
        SignError::TimedOut,
    ] {
        let (mut e, _) = fresh_provider_failure(error);
        let packet = catalogue(&mut e);
        let preparation = packet
            .preparation
            .expect("actual managed account preparation");
        e.http_prepared(
            START + 1.,
            preparation,
            Prepared {
                id: packet.request.id.0,
                expires_at: None,
            },
        );
        unpublished(&e, Route::Info);
        let actions = e.drain_actions().collect::<Vec<_>>();
        no_physical(&actions);
        assert_eq!(
            polls(&actions, START + 1.).len(),
            usize::from(error == SignError::TimedOut),
            "{error:?}"
        );
        e.http_prepared(
            START + 2.,
            preparation,
            Prepared {
                id: packet.request.id.0,
                expires_at: None,
            },
        );
        let duplicate = e.drain_actions().collect::<Vec<_>>();
        no_physical(&duplicate);
        assert!(
            !duplicate
                .iter()
                .any(|action| matches!(action, Action::SetTimer { .. }))
        );
    }
}

#[test]
fn native_fractional_window_signs_selected_account_get_with_exact_zero_wire() {
    let seed = STANDARD.encode([7; 32]);
    let mut e = support::configured_account(Options {
        key: Some("test-public-key".into()),
        secret: Some(seed.clone()),
        recv_window: Some(0.5),
        ..Default::default()
    });
    e.start(START);
    let markets = safe_cex_engine::test_support::prepared_actions(&mut e, START)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/markets") => Some(request),
            _ => None,
        })
        .unwrap();
    e.handle(
        START,
        Event::HttpResponse {
            request: markets.id,
            response: Response {
                status: 200,
                headers: "",
                body: MARKETS,
            },
        },
    );
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, START)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with(Route::Info.path()) => Some(request),
            _ => None,
        })
        .expect("genuine selected account GET released");
    let mut native =
        safe_cex_venue_backpack::sign::Credentials::new("test-public-key".into(), &seed).unwrap();
    native.window = 0;
    let expected = native.headers("accountQuery", &[Default::default()], START);
    assert_eq!(request.headers, expected);
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| *name == "X-Window" && value == "0")
    );
    e.handle(
        START,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: INFO,
            },
        },
    );
    assert_eq!(e.store().setting(SYMBOL).unwrap().leverage, 10.);
}
