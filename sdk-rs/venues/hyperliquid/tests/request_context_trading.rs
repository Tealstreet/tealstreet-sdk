//! Production custody controls with real setup, native replies and Prepare/Bind/Claim.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy,
};
use safe_cex_engine::socket_lifecycle::{
    EncodedBatch, Epoch, FeedId, Interest, LifecycleEvent, SocketError,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Outcome, Response, SocketId, Store, TimerId, Venue, VenueAction, VenueError,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options, Pending};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
const NOW: f64 = 2000.;
const OWNER: u32 = 700;
const CLOID: &str = "0x00000000000000000000000000000001";
const ACCEPT: &str =
    r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":900}}]}}}"#;
struct Probe {
    inner: Hyperliquid,
    peers: Vec<Context>,
    validated: usize,
}
impl Venue for Probe {
    fn address_nonce_domain(&self) -> Option<&'static safe_cex_engine::nonce::NonceDomain> {
        self.inner.address_nonce_domain()
    }
    fn public_address_slots(&self) -> safe_cex_engine::AddressSetupSlots {
        self.inner.public_address_slots()
    }
    fn dispatch_nonce() -> bool {
        Hyperliquid::dispatch_nonce()
    }
    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        safe_cex_engine::HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }
    fn http_pacing(&self) -> &[safe_cex_engine::request_context::Pacing] {
        self.inner.http_pacing()
    }
    fn http_release_domain() -> Option<safe_cex_engine::request_context::ReleaseDomain> {
        Hyperliquid::http_release_domain()
    }
    fn on_http_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &safe_cex_engine::request_context::Preflight,
    ) {
        self.inner.on_http_preflight_ready(cx, owner);
    }
    fn on_http_plan_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &safe_cex_engine::request_context::Plan,
    ) {
        self.inner.on_http_plan_ready(cx, owner);
    }
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) -> Option<safe_cex_engine::request_context::Preflight> {
        self.inner.on_http_owner_retired_preflight(owner)
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<safe_cex_engine::request_context::ReconciliationData, Error> {
        self.inner.capture_http_reconciliation(pending)
    }
    fn managed_sockets() -> bool {
        true
    }
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn guard_pending_placements(&self) -> bool {
        self.inner.guard_pending_placements()
    }
    fn guard_order_mutations(&self) -> bool {
        self.inner.guard_order_mutations()
    }
    fn fence_uncertain_cancel_all(&self) -> bool {
        self.inner.fence_uncertain_cancel_all()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId) {
        self.inner.on_open(cx, s)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, c: u16) {
        self.inner.on_close(cx, s, c)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, t: &str) {
        self.inner.on_message(cx, s, t)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, t: TimerId) {
        self.inner.on_timer(cx, t)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        self.inner.on_response(cx, p, r)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, s: &str) {
        self.inner.on_item(cx, p, s)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, s: &str) {
        self.inner.on_request_failed(cx, p, s)
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, e: VenueError) {
        self.inner.on_http_failed(cx, p, e)
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, e: VenueError) {
        self.inner.on_sign_error(cx, p, e)
    }
    fn book(&self, s: &str) -> Option<&Book> {
        self.inner.book(s)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
    fn cancel_needs_stored_order(&self) -> bool {
        self.inner.cancel_needs_stored_order()
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.on_public_feed(cx)
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        self.inner.on_socket_event(cx, event)
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        epoch: Epoch,
        pending: Pending,
    ) -> Result<(), SocketError> {
        self.inner.on_socket_interests_released(cx, epoch, pending)
    }
    fn encode_socket_interests(
        &self,
        store: &Store,
        feed: FeedId,
        subscribe: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        self.inner
            .encode_socket_interests(store, feed, subscribe, interests, emit)
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.inner
            .on_http_preparation_failed(cx, pending, reason, error)
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.inner.on_stream_error(cx, pending, response)
    }
    fn on_venue_action(&mut self, cx: &mut Cx<'_, Pending>, command: u32, action: VenueAction) {
        self.inner.on_venue_action(cx, command, action)
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchCandles { symbol, .. } if symbol == "__peer" => {
                if let Ok(c) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) {
                    self.peers.push(c)
                }
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["validate_peers"] => {
                for peer in &self.peers {
                    cx.validate_http_context(peer).unwrap();
                }
                self.validated = self.peers.len();
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["retire"] => {
                cx.fail_context(&self.peers.remove(0), "original peer retired")
                    .unwrap();
            }
            _ => self.inner.on_command(cx, command),
        }
    }
}
fn boot() -> (Engine<Probe>, SocketId) {
    boot_with_builder(true)
}
fn boot_with_builder(approval_ready: bool) -> (Engine<Probe>, SocketId) {
    // Independent fixtures use distinct real agents; native sharing tests stay unchanged.
    static AGENT: AtomicU64 = AtomicU64::new(100);
    let scalar = format!("{:064x}", AGENT.fetch_add(1, Ordering::Relaxed));
    let address = safe_cex_crypto::ethereum::private_key_address(&scalar).unwrap();
    let (provider, identities) = support::agent(&scalar);
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Hyperliquid::new(Options {
            key: Some(address.clone()),
            credentials: identities.primary.clone(),
            application_id: Some(address),
            ..Options::default()
        }),
        peers: vec![],
        validated: 0,
    });
    e.set_signer(Some(provider)).unwrap();
    e.set_identities(identities).unwrap();
    e.initialize_address_nonce(999.).unwrap();
    assert!(e.address_nonce_setup_ready());
    e.start(1000.);
    let mut socket = None;
    for _ in 0..30 {
        let actions = prepared_actions(&mut e, 1000.);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(r) => {
                    if !approval_ready && support::kind(&r) == "maxBuilderFee" {
                        continue;
                    }
                    let body = match support::kind(&r).as_str() {
                        "metaAndAssetCtxs" => support::META,
                        "perpDexs" => "[null]",
                        "userAbstraction" => "\"disabled\"",
                        "maxBuilderFee" => "30",
                        "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#,
                        "clearinghouseState" => support::STATE,
                        "frontendOpenOrders" => "[]",
                        other => panic!("unexpected native startup read {other}"),
                    };
                    respond(&mut e, 1000., &r, 200, body);
                }
                Action::WsConnect { socket: s, .. } => {
                    socket = Some(s);
                    e.handle(1000., Event::WsOpened { socket: s });
                }
                _ => {}
            }
        }
    }
    assert!(e.store().market("BTC-PERP").is_some());
    assert!(e.store().loaded().markets);
    e.take_frame(1000.);
    (e, socket.expect("native managed private socket"))
}
fn respond(e: &mut Engine<Probe>, now: f64, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn command(e: &mut Engine<Probe>, now: f64, c: Command) -> Vec<Action> {
    e.handle(now, Event::Command(c));
    e.drain_actions().collect()
}
fn place(command: u32) -> Command {
    Command::PlaceOrders { command, orders: vec![serde_json::from_value(json!({"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1,"clientOrderId":CLOID})).unwrap()] }
}
fn leverage(command: u32) -> Command {
    Command::SetLeverage {
        command,
        symbol: "BTC-PERP".into(),
        leverage: 5.,
        side: None,
    }
}
fn results(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == owner => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn exchange(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r.clone()),
            _ => None,
        })
        .collect()
}
fn prep(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        exchange(actions).is_empty(),
        "trade must Prepare before exchange HTTP"
    );
    let found: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match &**a {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.ends_with("/exchange") => Some((*preparation, request.clone())),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "one real trade Prepare");
    found[0].clone()
}
fn native_release(e: &mut Engine<Probe>, actions: &[Action]) -> HttpRequest {
    native_release_at(e, NOW, actions)
}
fn prepare_for_pacing(e: &mut Engine<Probe>, now: f64, actions: &[Action]) -> Vec<Action> {
    let (p, _) = prep(actions);
    e.http_prepared(
        now,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(exchange(&actions).is_empty());
    assert!(
        !actions.iter().any(
            |a| matches!(a, Action::ManagedHttp(a) if matches!(&**a, HttpAction::Bind { .. }))
        )
    );
    actions
}
fn native_release_at(e: &mut Engine<Probe>, now: f64, actions: &[Action]) -> HttpRequest {
    assert!(
        exchange(actions).is_empty(),
        "writes have no raw transport fallback"
    );
    let bound = actions.iter().find_map(|a| match a {
        Action::ManagedHttp(a) => match &**a {
            HttpAction::Bind {
                permit, request, ..
            } => Some((*permit, request.clone())),
            _ => None,
        },
        _ => None,
    });
    let (permit, request) = bound.unwrap_or_else(|| {
        let (p, _) = prep(actions);
        e.http_prepared(
            now,
            p,
            Prepared {
                id: p.attempt().request.0,
                expires_at: None,
            },
        );
        e.drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind {
                        permit, request, ..
                    } => Some((permit, request)),
                    _ => None,
                },
                _ => None,
            })
            .expect("real signed Bind")
    });
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn no_trade(actions: &[Action]) {
    assert!(
        exchange(actions).is_empty(),
        "shared refusal precedes native exchange"
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(&**a, HttpAction::Prepare { .. } | HttpAction::Bind { .. } | HttpAction::BindGroup { .. }))));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Sign { .. } | Action::SignPayload { .. } | Action::SignedOutbound { .. }
    )));
}
fn one_error(actions: &[Action], kind: ErrorKind, outcome: Outcome) {
    let r = results(actions, OWNER);
    assert_eq!(r.len(), 1);
    let error = r[0]
        .as_ref()
        .expect_err("the shared refusal must precede the native result");
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, Some(outcome));
}
fn seed_order(e: &mut Engine<Probe>, socket: SocketId) {
    e.handle(1500., Event::WsMessage { socket, text: r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"# });
    e.drain_actions().for_each(drop);
    assert!(e.store().order("9").is_some());
}
fn fill_peers(e: &mut Engine<Probe>) {
    for command in 1000..1256 {
        e.handle_command(
            NOW,
            Command::FetchCandles {
                command,
                symbol: "__peer".into(),
                interval: Interval::M1,
                limit: None,
                from_ms: None,
                to_ms: None,
            },
            Some(60_000),
        )
        .unwrap();
        let a: Vec<_> = e.drain_actions().collect();
        assert!(results(&a, command).is_empty());
        no_trade(&a);
    }
    assert_eq!(e.venue().peers.len(), 256);
}
#[derive(Clone, Copy)]
enum Case {
    Place,
    Cancel,
    Amend,
    Leverage,
    EmptyCancel,
}
impl Case {
    fn command(self) -> Command {
        match self {
            Self::Place => place(OWNER),
            Self::Cancel => Command::CancelOrders {
                command: OWNER,
                ids: vec!["9".into()],
            },
            Self::Amend => Command::AmendOrder {
                command: OWNER,
                id: "9".into(),
                price: Some(85.),
                amount: None,
            },
            Self::Leverage => leverage(OWNER),
            Self::EmptyCancel => Command::CancelSymbolOrders {
                command: OWNER,
                symbol: "ETH-PERP".into(),
            },
        }
    }
}
fn capacity(case: Case) {
    let (mut e, s) = boot();
    seed_order(&mut e, s);
    fill_peers(&mut e);
    let orders = serde_json::to_value(e.store().orders()).unwrap();
    let positions = serde_json::to_value(e.store().positions()).unwrap();
    let actions = command(&mut e, NOW, case.command());
    no_trade(&actions);
    one_error(&actions, ErrorKind::Internal, Outcome::NotSent);
    assert_eq!(serde_json::to_value(e.store().orders()).unwrap(), orders);
    assert_eq!(
        serde_json::to_value(e.store().positions()).unwrap(),
        positions
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandProgress { command: OWNER, .. })
    )));
    let retired = command(
        &mut e,
        NOW,
        Command::SetTickerSymbols {
            symbols: vec!["retire".into()],
        },
    );
    assert_eq!(results(&retired, 1000).len(), 1);
    assert_eq!(
        results(&retired, 1000)[0].as_ref().unwrap_err().message,
        "original peer retired"
    );
    let recovered = command(&mut e, NOW, case.command());
    if matches!(case, Case::EmptyCancel) {
        assert_eq!(results(&recovered, OWNER), vec![Ok("[]".into())]);
    } else {
        let _ = prep(&recovered);
        let r = native_release(&mut e, &recovered);
        let response = match case {
            Case::Cancel | Case::Amend => {
                r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#
            }
            Case::Leverage => r#"{"status":"ok","response":{"type":"default"}}"#,
            Case::Place => ACCEPT,
            Case::EmptyCancel => unreachable!(),
        };
        respond(&mut e, NOW + 1., &r, 200, response);
        let mut settled: Vec<_> = e.drain_actions().collect();
        if matches!(case, Case::Amend) {
            assert!(results(&settled, OWNER).is_empty());
            settled = prepare_for_pacing(&mut e, NOW + 1., &settled);
            let timer = settled
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if *at_ms == NOW + 334. => Some(*timer),
                    _ => None,
                })
                .expect("same-owner replacement pacing");
            e.handle(NOW + 334., Event::TimerFired { timer });
            let a: Vec<_> = e.drain_actions().collect();
            let r = native_release_at(&mut e, NOW + 334., &a);
            respond(&mut e, NOW + 335., &r, 200, ACCEPT);
            settled = e.drain_actions().collect();
        }
        let terminal = results(&settled, OWNER);
        assert_eq!(terminal.len(), 1);
        assert!(
            terminal[0].is_ok(),
            "recovered owner completes native operation"
        );
    }
    command(
        &mut e,
        NOW,
        Command::SetTickerSymbols {
            symbols: vec!["validate_peers".into()],
        },
    );
    assert_eq!(e.venue().validated, 255);
    e.handle_command(
        NOW + 500.,
        Command::FetchCandles {
            command: 1256,
            symbol: "__peer".into(),
            interval: Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
        },
        Some(60_000),
    )
    .unwrap();
    let admitted: Vec<_> = e.drain_actions().collect();
    no_trade(&admitted);
    assert!(
        results(&admitted, 1256).is_empty(),
        "fresh 256th peer is retained after the recovered owner completed"
    );
    assert_eq!(e.venue().peers.len(), 256);
    command(
        &mut e,
        NOW + 500.,
        Command::SetTickerSymbols {
            symbols: vec!["validate_peers".into()],
        },
    );
    assert_eq!(
        e.venue().validated,
        256,
        "recovered owner released its last registry credit"
    );
}
#[test]
fn capacity_place() {
    capacity(Case::Place);
}
#[test]
fn capacity_cancel() {
    capacity(Case::Cancel);
}
#[test]
fn capacity_amend() {
    capacity(Case::Amend);
}
#[test]
fn capacity_leverage() {
    capacity(Case::Leverage);
}
#[test]
fn capacity_empty_cancel() {
    capacity(Case::EmptyCancel);
}
#[test]
fn startup_forwards_real_socket_and_account() {
    let (e, _) = boot();
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().orders().len(), 0);
}
#[test]
fn exchange_requires_preparation_before_io() {
    let (mut e, _) = boot();
    let a = command(&mut e, NOW, place(OWNER));
    let _ = prep(&a);
}
#[test]
fn unclaimed_exchange_data_cannot_publish_or_finish() {
    let (mut e, _) = boot();
    let a = command(&mut e, NOW, place(OWNER));
    let r = exchange(&a)
        .into_iter()
        .next()
        .unwrap_or_else(|| prep(&a).1);
    respond(&mut e, NOW + 1., &r, 200, ACCEPT);
    assert!(
        results(&e.drain_actions().collect::<Vec<_>>(), OWNER).is_empty(),
        "unclaimed DATA cannot finish"
    );
    assert!(e.store().order("900").is_none());
}
#[test]
fn nan_entry_refuses_before_projection() {
    let (mut e, _) = boot();
    let a = command(&mut e, f64::NAN, place(OWNER));
    one_error(&a, ErrorKind::InvalidInput, Outcome::NotSent);
    no_trade(&a);
    assert!(e.store().orders().is_empty());
}
#[test]
fn zero_wait_is_refused_before_projection() {
    let (mut e, _) = boot();
    assert!(e.handle_command(NOW, place(OWNER), Some(0)).is_err());
    no_trade(&e.drain_actions().collect::<Vec<_>>());
    assert!(e.store().orders().is_empty());
}
#[test]
fn preparation_failure_is_terminal_and_duplicate_is_inert() {
    let (mut e, _) = boot();
    let a = command(&mut e, NOW, place(OWNER));
    let (p, _) = prep(&a);
    e.http_preparation_failed(
        NOW + 1.,
        p,
        Error::Invalid,
        VenueError::new("host refused").kind(ErrorKind::InvalidInput),
    );
    let a: Vec<_> = e.drain_actions().collect();
    one_error(&a, ErrorKind::InvalidInput, Outcome::NotSent);
    no_trade(&a);
    e.http_preparation_failed(NOW + 2., p, Error::Invalid, VenueError::new("duplicate"));
    assert!(e.drain_actions().next().is_none());
}
#[test]
fn delayed_prepare_expires_at_the_twenty_second_bound() {
    let (mut e, _) = boot();
    let a = command(&mut e, NOW, place(OWNER));
    let (p, r) = prep(&a);
    assert_eq!(r.timeout_ms, Some(20_000));
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("call timer includes Prepare");
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let a: Vec<_> = e.drain_actions().collect();
    one_error(&a, ErrorKind::Timeout, Outcome::NotSent);
    no_trade(&a);
    e.http_prepared(
        NOW + 20_001.,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(e.drain_actions().next().is_none());
}
#[test]
fn an_idless_status_on_an_ordinary_order_remains_unknown() {
    let (mut e, _) = boot();
    let a = command(&mut e, NOW, place(OWNER));
    let r = native_release(&mut e, &a);
    assert_eq!(r.method, Method::Post);
    assert_eq!(r.headers, vec![("Content-Type", "application/json".into())]);
    let b: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_eq!(b["action"]["type"], "order");
    assert_eq!(b["action"]["orders"][0]["a"], 0);
    assert_eq!(b["action"]["orders"][0]["c"], CLOID);
    assert_eq!(b["action"]["orders"][0]["p"], "90");
    assert!(b.get("vaultAddress").is_none());
    assert!(b["signature"]["r"].as_str().unwrap().starts_with("0x"));
    respond(
        &mut e,
        NOW + 1.,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#,
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        results(&a, OWNER)[0].as_ref().unwrap_err().outcome,
        Some(Outcome::Unknown)
    );
}
fn status_refusal(status: u16, outcome: Outcome) {
    let (mut e, _) = boot();
    let a = command(&mut e, NOW, place(OWNER));
    let r = native_release(&mut e, &a);
    respond(&mut e, NOW + 1., &r, status, "not an ACK");
    let a: Vec<_> = e.drain_actions().collect();
    let results = results(&a, OWNER);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].as_ref().unwrap_err().outcome, Some(outcome));
    no_trade(&a);
    respond(&mut e, NOW + 2., &r, 200, ACCEPT);
    assert!(e.drain_actions().next().is_none());
}
#[test]
fn unproven_http_401_is_unknown_without_resend() {
    status_refusal(401, Outcome::Unknown);
}
#[test]
fn unproven_http_403_is_unknown_without_resend() {
    status_refusal(403, Outcome::Unknown);
}
#[test]
fn unproven_http_429_is_unknown_without_resend() {
    status_refusal(429, Outcome::Unknown);
}
#[test]
fn native_408_is_unknown_without_resend() {
    status_refusal(408, Outcome::Unknown);
}
#[test]
fn native_503_is_unknown_without_resend() {
    status_refusal(503, Outcome::Unknown);
}
#[test]
fn lost_response_is_unknown_and_never_resends() {
    let (mut e, _) = boot();
    let a = command(&mut e, NOW, place(OWNER));
    let r = native_release(&mut e, &a);
    e.handle(
        NOW + 1.,
        Event::HttpFailed {
            request: r.id,
            error: "lost ACK",
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    one_error(&a, ErrorKind::Network, Outcome::Unknown);
    no_trade(&a);
}
#[test]
fn duplicate_owner_preserves_the_retained_peer() {
    let (mut e, _) = boot();
    let a = command(
        &mut e,
        NOW,
        Command::FetchCandles {
            command: OWNER,
            symbol: "__peer".into(),
            interval: Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
        },
    );
    assert!(results(&a, OWNER).is_empty());
    let a = command(&mut e, NOW, place(OWNER));
    no_trade(&a);
    assert!(results(&a, OWNER).is_empty());
    let retired = command(
        &mut e,
        NOW,
        Command::SetTickerSymbols {
            symbols: vec!["retire".into()],
        },
    );
    assert_eq!(results(&retired, OWNER).len(), 1);
}

#[test]
fn fifo_waits_334_ms_after_a_delayed_successful_final_claim() {
    let (mut e, _) = boot();
    let first = command(&mut e, NOW, leverage(OWNER));
    let (preparation, _) = prep(&first);
    e.http_prepared(
        NOW,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let bound: Vec<_> = e.drain_actions().collect();
    let queued = command(&mut e, NOW + 1., leverage(OWNER + 1));
    no_trade(&queued);
    assert!(results(&queued, OWNER + 1).is_empty());
    let first_claim_at = NOW + 500.;
    let r = native_release_at(&mut e, first_claim_at, &bound);
    respond(
        &mut e,
        first_claim_at + 1.,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(results(&actions, OWNER), vec![Ok("[]".into())]);
    let actions = prepare_for_pacing(&mut e, first_claim_at + 1., &actions);
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == first_claim_at + 334. => Some(*timer),
            _ => None,
        })
        .expect("334 ms pacing starts at the actual final Claim");
    e.handle(first_claim_at + 333., Event::TimerFired { timer });
    let early: Vec<_> = e.drain_actions().collect();
    assert!(
        !early.iter().any(
            |a| matches!(a, Action::ManagedHttp(a) if matches!(&**a, HttpAction::Bind { .. }))
        )
    );
    assert!(exchange(&early).is_empty());
    let timer = early
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == first_claim_at + 334. => Some(*timer),
            _ => None,
        })
        .expect("early wake preserves the final-release deadline");
    e.handle(first_claim_at + 334., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let second = native_release_at(&mut e, first_claim_at + 334., &actions);
    assert_eq!(
        serde_json::from_str::<Value>(second.body.as_deref().unwrap()).unwrap()["action"]["type"],
        "updateLeverage"
    );
}

#[test]
fn full_status_array_is_correlated_before_a_mixed_refusal() {
    let (mut e, _) = boot();
    let mut c = place(OWNER);
    if let Command::PlaceOrders { orders, .. } = &mut c {
        orders[0].stop_loss = Some(80.);
    }
    let actions = command(&mut e, NOW, c);
    let r = native_release(&mut e, &actions);
    let body: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_eq!(
        body["action"]["orders"].as_array().unwrap().len(),
        2,
        "two actual rows in one native call"
    );
    respond(
        &mut e,
        NOW + 1.,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":900}},{"error":"rejected row"}]}}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let outcome = results(&actions, OWNER);
    assert_eq!(outcome.len(), 1);
    let error = outcome[0].as_ref().unwrap_err();
    assert_eq!(error.message, "rejected row");
    assert_eq!(error.kind, Some(ErrorKind::Rejected));
    assert_eq!(
        error.outcome,
        Some(Outcome::Unknown),
        "mixed acceptance is not wholly rejected"
    );
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["900"]);
    assert!(e.store().order("900").is_some());
    no_trade(&actions);
}

#[test]
fn malformed_cancel_ack_cannot_release_a_replacement() {
    let (mut e, socket) = boot();
    seed_order(&mut e, socket);
    let a = command(&mut e, NOW, Case::Amend.command());
    let r = native_release(&mut e, &a);
    respond(
        &mut e,
        NOW + 1.,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":[]}}}"#,
    );
    let a: Vec<_> = e.drain_actions().collect();
    let outcome = results(&a, OWNER);
    assert_eq!(outcome.len(), 1);
    assert_eq!(
        outcome[0].as_ref().unwrap_err().outcome,
        Some(Outcome::Unknown)
    );
    no_trade(&a);
    assert!(e.store().order("9").is_some());
}

#[test]
fn schedule_cancel_keeps_its_control_specific_ack_proof() {
    let (mut e, _) = boot();
    let schedule =
        safe_cex_engine::command::decode_command(r#"{"op":"scheduleCancel","command":700}"#)
            .unwrap();
    let a = command(&mut e, NOW, schedule);
    let r = native_release(&mut e, &a);
    assert_eq!(
        serde_json::from_str::<Value>(r.body.as_deref().unwrap()).unwrap()["action"],
        json!({"type":"scheduleCancel"})
    );
    let positions = serde_json::to_value(e.store().positions()).unwrap();
    respond(
        &mut e,
        NOW + 1.,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert_eq!(results(&a, OWNER), vec![Ok("[]".into())]);
    assert_eq!(
        serde_json::to_value(e.store().positions()).unwrap(),
        positions
    );
    no_trade(&a);
}

fn builder_hold(reply_ready: bool) {
    let (mut e, _) = boot_with_builder(false);
    let actions = command(&mut e, NOW, place(OWNER));
    assert!(exchange(&actions).is_empty());
    assert!(results(&actions, OWNER).is_empty());
    let preparation = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match &**action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if support::kind(request) == "maxBuilderFee" => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("native builder prerequisite");
    e.http_prepared(
        NOW,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, read) = e
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } if support::kind(&request) == "maxBuilderFee" => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("builder read binds its prepared route");
    assert!(matches!(e.claim_http(NOW, permit), Ok(Claim::Released(_))));
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 8000. => Some(*timer),
            _ => None,
        })
        .expect("native eight-second builder hold");
    let now = if reply_ready { NOW + 1. } else { NOW + 8000. };
    if reply_ready {
        respond(&mut e, now, &read, 200, "30");
    } else {
        e.handle(now, Event::TimerFired { timer });
    }
    let actions: Vec<_> = e.drain_actions().collect();
    let request = native_release_at(&mut e, now, &actions);
    let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["action"].get("builder").is_some(), reply_ready);
}
#[test]
fn native_builder_reply_releases_the_same_job() {
    builder_hold(true);
}
#[test]
fn native_builder_expiry_proceeds_without_the_builder() {
    builder_hold(false);
}

#[test]
fn released_timeout_is_unknown_with_no_write_replay() {
    let (mut e, _) = boot();
    let a = command(&mut e, NOW, place(OWNER));
    let _ = prep(&a);
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("physical call bound");
    let r = native_release(&mut e, &a);
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let a: Vec<_> = e.drain_actions().collect();
    one_error(&a, ErrorKind::Timeout, Outcome::Unknown);
    no_trade(&a);
    respond(&mut e, NOW + 20_001., &r, 200, ACCEPT);
    let late: Vec<_> = e.drain_actions().collect();
    assert!(results(&late, OWNER).is_empty());
    no_trade(&late);
}

#[test]
fn delayed_claim_cannot_release_an_expired_packet() {
    let (mut e, _) = boot();
    let a = command(&mut e, NOW, place(OWNER));
    let (p, _) = prep(&a);
    e.http_prepared(
        NOW + 19_999.,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .expect("real signed Bind requires the installed provider fixture at source");
    assert!(!matches!(
        e.claim_http(NOW + 20_001., permit),
        Ok(Claim::Released(_))
    ));
    let a: Vec<_> = e.drain_actions().collect();
    one_error(&a, ErrorKind::Timeout, Outcome::NotSent);
    no_trade(&a);
}

#[test]
fn stale_old_reply_cannot_settle_the_next_fifo_owner() {
    let (mut e, _) = boot();
    let first = command(&mut e, NOW, place(OWNER));
    let _ = prep(&first);
    let r = native_release(&mut e, &first);
    let queued = command(&mut e, NOW + 1., leverage(OWNER + 1));
    no_trade(&queued);
    assert!(results(&queued, OWNER + 1).is_empty());
    respond(&mut e, NOW + 2., &r, 200, ACCEPT);
    let a: Vec<_> = e.drain_actions().collect();
    assert_eq!(results(&a, OWNER).len(), 1);
    let a = prepare_for_pacing(&mut e, NOW + 2., &a);
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 334. => Some(*timer),
            _ => None,
        })
        .expect("next FIFO owner waits for release pacing");
    e.handle(NOW + 334., Event::TimerFired { timer });
    let a: Vec<_> = e.drain_actions().collect();
    let second = native_release_at(&mut e, NOW + 334., &a);
    assert_ne!(r.id, second.id);
    respond(&mut e, NOW + 335., &r, 200, ACCEPT);
    let stale: Vec<_> = e.drain_actions().collect();
    assert!(results(&stale, OWNER).is_empty());
    assert!(results(&stale, OWNER + 1).is_empty());
    no_trade(&stale);
    respond(
        &mut e,
        NOW + 336.,
        &second,
        200,
        r#"{"status":"ok","response":{"type":"default"}}"#,
    );
    let done: Vec<_> = e.drain_actions().collect();
    assert_eq!(results(&done, OWNER + 1), vec![Ok("[]".into())]);
}

fn native_two_row_reply(body: &str) -> (Engine<Probe>, Vec<Action>) {
    let (mut e, _) = boot();
    let mut c = place(OWNER);
    if let Command::PlaceOrders { orders, .. } = &mut c {
        orders[0].stop_loss = Some(80.);
    }
    let a = command(&mut e, NOW, c);
    let request = native_release(&mut e, &a);
    let wire: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(wire["action"]["orders"].as_array().unwrap().len(), 2);
    respond(&mut e, NOW + 1., &request, 200, body);
    let actions = e.drain_actions().collect();
    (e, actions)
}

#[test]
fn deferred_attachment_returns_only_the_accepted_parent_id() {
    let (e, actions) = native_two_row_reply(
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":3001}},"waitingForFill"]}}}"#,
    );
    assert_eq!(results(&actions, OWNER), vec![Ok(r#"["3001"]"#.into())]);
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(e.store().order("3001").unwrap().id, "3001");
    no_trade(&actions);
}

#[test]
fn a_refused_attachment_keeps_the_real_parent_as_a_known_partial() {
    let (e, actions) = native_two_row_reply(
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":3001}},{"error":"Insufficient margin to place order."}]}}}"#,
    );
    let result = results(&actions, OWNER);
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["3001"]);
    assert!(e.store().order("3001").is_some());
    no_trade(&actions);
}

#[test]
fn an_unlisted_child_failure_wins_over_a_cited_refusal_without_losing_the_parent() {
    for errors in [
        [
            "Insufficient margin to place order.",
            "unlisted child failure",
        ],
        [
            "unlisted child failure",
            "Insufficient margin to place order.",
        ],
    ] {
        let (mut e, _) = boot();
        let mut c = place(OWNER);
        let Command::PlaceOrders { orders, .. } = &mut c else {
            unreachable!()
        };
        orders[0].stop_loss = Some(80.);
        orders[0].take_profit = Some(120.);
        let actions = command(&mut e, NOW, c);
        let request = native_release(&mut e, &actions);
        let response = json!({"status":"ok","response":{"type":"order","data":{"statuses":[
            {"resting":{"oid":3001}}, {"error":errors[0]}, {"error":errors[1]}
        ]}}})
        .to_string();
        respond(&mut e, NOW + 1., &request, 200, &response);
        let actions: Vec<_> = e.drain_actions().collect();
        let result = results(&actions, OWNER);
        let error = result[0].as_ref().unwrap_err();
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["3001"]);
        assert!(e.store().order("3001").is_some());
        no_trade(&actions);
    }
}

#[test]
fn independent_native_child_ids_are_preserved_without_logical_id_invention() {
    let (_, actions) = native_two_row_reply(
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":3001}},{"resting":{"oid":3002}}]}}}"#,
    );
    assert_eq!(
        results(&actions, OWNER),
        vec![Ok(r#"["3001","3002"]"#.into())]
    );
}

#[test]
fn a_deferred_attachment_never_hides_another_child_failure() {
    for (child, expected) in [
        ("Insufficient margin to place order.", None),
        ("unlisted child failure", Some(Outcome::Unknown)),
    ] {
        for deferred_first in [false, true] {
            let (mut e, _) = boot();
            let mut c = place(OWNER);
            let Command::PlaceOrders { orders, .. } = &mut c else {
                unreachable!()
            };
            orders[0].stop_loss = Some(80.);
            orders[0].take_profit = Some(120.);
            let actions = command(&mut e, NOW, c);
            let request = native_release(&mut e, &actions);
            let mut children = [json!({"error":child}), json!("waitingForFill")];
            if deferred_first {
                children.reverse();
            }
            let response = json!({"status":"ok","response":{"type":"order","data":{
                "statuses":[{"resting":{"oid":3001}}, children[0], children[1]]
            }}})
            .to_string();
            respond(&mut e, NOW + 1., &request, 200, &response);
            let actions: Vec<_> = e.drain_actions().collect();
            let result = results(&actions, OWNER);
            let error = result[0].as_ref().unwrap_err();
            assert_eq!(error.outcome, expected);
            assert_eq!(error.partial.as_ref().unwrap().order_ids, ["3001"]);
            assert!(e.store().order("3001").is_some());
            no_trade(&actions);
        }
    }
}

#[test]
fn a_deferred_attachment_requires_its_accepted_parent_in_the_parent_row() {
    for statuses in [
        json!(["waitingForFill", {"resting":{"oid":3001}}]),
        json!([{"error":"Insufficient margin to place order."}, "waitingForFill"]),
        json!([{"resting":{"oid":3001}}, "waitingForTrigger"]),
    ] {
        let body = json!({"status":"ok","response":{"type":"order","data":{
            "statuses":statuses
        }}})
        .to_string();
        let (_, actions) = native_two_row_reply(&body);
        let result = results(&actions, OWNER);
        assert_eq!(
            result[0].as_ref().unwrap_err().outcome,
            Some(Outcome::Unknown)
        );
        no_trade(&actions);
    }
}

#[test]
fn a_fixed_size_trigger_does_not_gain_the_full_position_setting_contract() {
    let (mut e, _) = boot();
    let mut c = full_position_trigger(OWNER);
    let Command::PlaceOrders { orders, .. } = &mut c else {
        unreachable!()
    };
    orders[0].amount = 0.5;
    let actions = command(&mut e, NOW, c);
    let request = native_release(&mut e, &actions);
    let wire: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(wire["action"]["grouping"], "na");
    assert_eq!(wire["action"]["orders"][0]["s"], "0.5");
    respond(
        &mut e,
        NOW + 1.,
        &request,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        results(&actions, OWNER)[0].as_ref().unwrap_err().outcome,
        Some(Outcome::Unknown)
    );
    no_trade(&actions);
}

fn full_position_trigger(command: u32) -> Command {
    Command::PlaceOrders {
        command,
        orders: vec![
            serde_json::from_value(json!({
                "symbol":"BTC-PERP", "type":"stop_market", "side":"sell",
                "price":80, "amount":0, "reduceOnly":true,
                "clientOrderId":"0x00000000000000000000000000000002"
            }))
            .unwrap(),
        ],
    }
}

#[test]
fn deferred_full_position_trigger_succeeds_without_an_order_id() {
    let (mut e, _) = boot();
    let actions = command(&mut e, NOW, full_position_trigger(OWNER));
    let request = native_release(&mut e, &actions);
    let wire: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(wire["action"]["grouping"], "positionTpsl");
    assert_eq!(wire["action"]["orders"][0]["s"], "0");
    respond(
        &mut e,
        NOW + 1.,
        &request,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(results(&actions, OWNER), vec![Ok("[]".into())]);
    assert!(e.store().orders().is_empty());
    no_trade(&actions);
}

fn deferred_child_terminal(status: &str, expire_record: bool, failed_leg: bool) {
    for materialized in [false, true] {
        let (mut e, socket) = boot();
        let mut c = place(OWNER);
        let Command::PlaceOrders { orders, .. } = &mut c else {
            unreachable!()
        };
        orders[0].stop_loss = Some(80.);
        let actions = command(&mut e, NOW, c);
        let request = native_release(&mut e, &actions);
        let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        let cloid = body["action"]["orders"][1]["c"].as_str().unwrap();
        respond(
            &mut e,
            NOW + 1.,
            &request,
            200,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":3001}},"waitingForFill"]}}}"#,
        );
        e.drain_actions().for_each(drop);
        let row = json!({"coin":"BTC", "oid":3002, "cloid":cloid,
            "side":"A", "limitPx":"90", "origSz":"1", "sz":"1",
            "isTrigger":true, "triggerPx":"80", "reduceOnly":true,
            "orderType":"Stop Market", "tif":""});
        if materialized {
            let text = json!({"channel":"orderUpdates", "data":[{"order":row,"status":"open"}]})
                .to_string();
            e.handle(
                NOW + 2.,
                Event::WsMessage {
                    socket,
                    text: &text,
                },
            );
            e.drain_actions().for_each(drop);
            assert!(e.store().order("3002").is_some());
        }
        let terminal_at = if expire_record {
            let heartbeat = format!(
                "{{\"channel\":\"clearinghouseState\",\"data\":{}}}",
                support::STATE
            );
            for tick in 1..=180 {
                e.handle(
                    NOW + f64::from(tick) * 10_000.,
                    Event::WsMessage {
                        socket,
                        text: &heartbeat,
                    },
                );
                e.drain_actions().for_each(drop);
            }
            let at = NOW + 1_800_001.;
            let unrelated = json!({"channel":"orderUpdates", "data":[{"order":{
                "coin":"BTC", "oid":7000, "side":"B", "limitPx":"200",
                "origSz":"1", "sz":"1", "orderType":"Limit", "tif":"Gtc"
            },"status":"open"}]})
            .to_string();
            e.handle(
                at,
                Event::WsMessage {
                    socket,
                    text: &unrelated,
                },
            );
            e.drain_actions().for_each(drop);
            assert!(e.store().order("7000").is_some());
            at + 1.
        } else {
            NOW + 3.
        };
        let text =
            json!({"channel":"orderUpdates", "data":[{"order":row,"status":status}]}).to_string();
        for delivery in 0..2 {
            e.handle(
                terminal_at + f64::from(delivery),
                Event::WsMessage {
                    socket,
                    text: &text,
                },
            );
            let actions: Vec<_> = e.drain_actions().collect();
            let terminal: Vec<_> = actions
                .iter()
                .filter_map(|action| match action {
                    Action::Notice(Notice::Event(event))
                        if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED =>
                    {
                        Some(event)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                terminal.len(),
                usize::from(failed_leg && !materialized && delivery == 0)
            );
            if let Some(event) = terminal.first() {
                for (field, value) in [
                    ("symbol", "BTC-PERP"),
                    ("orderId", "3002"),
                    ("clientOrderId", cloid),
                    ("status", status),
                ] {
                    assert!(
                        event
                            .fields
                            .contains(&(field, safe_cex_engine::log::Value::Str(value.into())))
                    );
                }
            }
            assert!(e.store().order("3002").is_none());
            assert!(e.store().order("3001").is_some());
            assert!(results(&actions, OWNER).is_empty());
            no_trade(&actions);
        }
        let heartbeat = format!(
            "{{\"channel\":\"clearinghouseState\",\"data\":{}}}",
            support::STATE
        );
        for tick in 1..=13 {
            e.handle(
                terminal_at + f64::from(tick) * 10_000.,
                Event::WsMessage {
                    socket,
                    text: &heartbeat,
                },
            );
            e.drain_actions().for_each(drop);
        }
        e.handle(
            terminal_at + 130_001.,
            Event::WsMessage {
                socket,
                text: &text,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED)));
        no_trade(&actions);
    }
}

#[test]
fn deferred_child_rejection_is_correlated_once_before_materialization() {
    deferred_child_terminal("perpMarginRejected", false, true);
}

#[test]
fn deferred_child_venue_forced_cancel_is_correlated_once_before_materialization() {
    deferred_child_terminal("marginCanceled", false, true);
}

#[test]
fn deferred_child_terminal_survives_record_expiry() {
    for status in ["perpMarginRejected", "marginCanceled"] {
        deferred_child_terminal(status, true, true);
    }
}

#[test]
fn normal_protective_terminal_user_cancel_is_not_a_failed_leg() {
    deferred_child_terminal("canceled", false, false);
    deferred_child_terminal("canceled", true, false);
}

#[test]
fn normal_protective_terminal_sibling_fill_cancel_is_not_a_failed_leg() {
    deferred_child_terminal("siblingFilledCanceled", false, false);
    deferred_child_terminal("siblingFilledCanceled", true, false);
}

#[test]
fn native_protective_terminal_evidence_does_not_need_a_placement_record() {
    for kind in [
        "Stop Market",
        "Stop Limit",
        "Take Profit Market",
        "Take Profit Limit",
    ] {
        let (mut e, socket) = boot();
        let text = json!({"channel":"orderUpdates","data":[{"order":{
            "coin":"BTC","oid":3002,"cloid":CLOID,"side":"A","sz":"1",
            "origSz":"1","limitPx":"90","triggerPx":"80",
            "reduceOnly":true,"isTrigger":true,"orderType":kind
        },"status":"perpMarginRejected"}]})
        .to_string();
        for delivery in 0..2 {
            e.handle(
                NOW + f64::from(delivery),
                Event::WsMessage {
                    socket,
                    text: &text,
                },
            );
            let actions: Vec<_> = e.drain_actions().collect();
            assert_eq!(
                actions
                    .iter()
                    .filter(|action| matches!(action,
                Action::Notice(Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED))
                    .count(),
                usize::from(delivery == 0)
            );
            assert!(e.store().orders().is_empty());
            no_trade(&actions);
        }
    }
}

#[test]
fn incomplete_or_ordinary_terminal_rows_do_not_report_a_failed_protective_child() {
    for (field, value) in [
        ("reduceOnly", json!(false)),
        ("isTrigger", json!(false)),
        ("orderType", json!("Limit")),
        ("orderType", json!("unknown")),
        ("cloid", json!("")),
        ("cloid", json!(null)),
        ("oid", json!(0)),
        ("coin", json!("unknown")),
    ] {
        let (mut e, socket) = boot();
        let mut row = json!({"coin":"BTC","oid":3002,"cloid":CLOID,"side":"A",
            "sz":"1","origSz":"1","limitPx":"90","triggerPx":"80",
            "reduceOnly":true,"isTrigger":true,"orderType":"Stop Market"});
        row[field] = value;
        let text =
            json!({"channel":"orderUpdates","data":[{"order":row,"status":"perpMarginRejected"}]})
                .to_string();
        e.handle(
            NOW,
            Event::WsMessage {
                socket,
                text: &text,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED)));
        no_trade(&actions);
    }
}

#[test]
fn native_protective_failure_statuses_form_a_closed_set() {
    for (failed, statuses) in [
        (
            true,
            &[
                "rejected",
                "tickRejected",
                "minTradeNtlRejected",
                "perpMarginRejected",
                "reduceOnlyRejected",
                "badAloPxRejected",
                "iocCancelRejected",
                "badTriggerPxRejected",
                "marketOrderNoLiquidityRejected",
                "positionIncreaseAtOpenInterestCapRejected",
                "positionFlipAtOpenInterestCapRejected",
                "tooAggressiveAtOpenInterestCapRejected",
                "openInterestIncreaseRejected",
                "insufficientSpotBalanceRejected",
                "oracleRejected",
                "perpMaxPositionRejected",
                "marginCanceled",
                "vaultWithdrawalCanceled",
                "openInterestCapCanceled",
                "selfTradeCanceled",
                "reduceOnlyCanceled",
                "delistedCanceled",
                "liquidatedCanceled",
            ][..],
        ),
        (
            false,
            &[
                "filled",
                "canceled",
                "siblingFilledCanceled",
                "scheduledCancel",
                "open",
                "triggered",
                "FooRejected",
                "FooCanceled",
            ][..],
        ),
    ] {
        for status in statuses {
            let (mut e, socket) = boot();
            let text = json!({"channel":"orderUpdates","data":[{"order":{
                "coin":"BTC","oid":3002,"cloid":CLOID,"side":"A","sz":"1",
                "origSz":"1","limitPx":"90","triggerPx":"80",
                "reduceOnly":true,"isTrigger":true,"orderType":"Stop Market"
            },"status":status}]})
            .to_string();
            e.handle(
                NOW,
                Event::WsMessage {
                    socket,
                    text: &text,
                },
            );
            let actions: Vec<_> = e.drain_actions().collect();
            assert_eq!(
                actions
                    .iter()
                    .filter(|action| matches!(action,
                Action::Notice(Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::CHILD_LEG_FAILED))
                    .count(),
                usize::from(failed),
                "{status}"
            );
            no_trade(&actions);
        }
    }
}

#[test]
fn deferred_full_position_effect_allows_one_successor_and_survives_its_refusal() {
    let (mut e, _) = boot();
    let mut first = full_position_trigger(OWNER);
    let Command::PlaceOrders { orders, .. } = &mut first else {
        unreachable!()
    };
    let Command::PlaceOrders {
        orders: following, ..
    } = place(OWNER)
    else {
        unreachable!()
    };
    orders.extend(following);
    let actions = command(&mut e, NOW, first);
    let request = native_release(&mut e, &actions);
    respond(
        &mut e,
        NOW + 1.,
        &request,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(results(&actions, OWNER).is_empty());
    let paced = prepare_for_pacing(&mut e, NOW + 1., &actions);
    let timer = paced
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 334. => Some(*timer),
            _ => None,
        })
        .expect("the accepted setting permits the next physical leg");
    e.handle(NOW + 334., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let following = native_release_at(&mut e, NOW + 334., &actions);
    assert_ne!(following.id, request.id);
    respond(
        &mut e,
        NOW + 335.,
        &following,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Insufficient margin to place order."}]}}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let result = results(&actions, OWNER);
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.outcome, None);
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
    no_trade(&actions);
}

#[test]
fn native_mixed_refusal_scans_accepted_rows_after_the_first_error() {
    let (e, a) = native_two_row_reply(
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"first rejected"},{"resting":{"oid":901}}]}}}"#,
    );
    let result = results(&a, OWNER);
    assert_eq!(result.len(), 1);
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.message, "first rejected");
    assert_eq!(error.kind, Some(ErrorKind::Rejected));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["901"]);
    assert!(e.store().order("901").is_some());
    no_trade(&a);
}

#[test]
fn native_all_rejected_rows_keep_the_full_ordered_diagnostic() {
    let (e, a) = native_two_row_reply(
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"first rejected"},{"error":"second rejected"}]}}}"#,
    );
    let result = results(&a, OWNER);
    assert_eq!(result.len(), 1);
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.message, "first rejected, second rejected");
    assert_eq!(error.kind, Some(ErrorKind::Rejected));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(e.store().orders().is_empty());
    no_trade(&a);
}

#[test]
fn native_short_and_long_status_arrays_publish_no_prefix() {
    for statuses in [
        json!([{"resting":{"oid":900}}]),
        json!([{"resting":{"oid":900}}, {"resting":{"oid":901}}, {"resting":{"oid":902}}]),
    ] {
        let reply = json!({"status":"ok","response":{"type":"order","data":{"statuses":statuses}}});
        let (e, a) = native_two_row_reply(&reply.to_string());
        let result = results(&a, OWNER);
        assert_eq!(result.len(), 1);
        let error = result[0].as_ref().unwrap_err();
        assert_eq!(error.message, "Invalid Hyperliquid command response");
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert!(e.store().orders().is_empty());
        no_trade(&a);
    }
}

#[test]
fn native_unknown_suffix_keeps_the_already_published_prefix() {
    let (e, a) = native_two_row_reply(
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":900}},"mystery"]}}}"#,
    );
    let result = results(&a, OWNER);
    assert_eq!(result.len(), 1);
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.message, "Unknown Hyperliquid order status");
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(e.store().order("900").unwrap().id, "900");
    no_trade(&a);
}

#[test]
fn native_wrong_response_kind_cannot_publish_matching_rows() {
    let (e, a) = native_two_row_reply(
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":[{"resting":{"oid":900}},{"resting":{"oid":901}}]}}}"#,
    );
    let result = results(&a, OWNER);
    assert_eq!(result.len(), 1);
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.message, "Invalid Hyperliquid command response");
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(e.store().orders().is_empty());
    no_trade(&a);
}

#[test]
fn unmatched_pending_rows_are_unknown_without_an_accepted_parent() {
    let (e, a) = native_two_row_reply(
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForFill","waitingForTrigger"]}}}"#,
    );
    assert_eq!(
        results(&a, OWNER)[0].as_ref().unwrap_err().outcome,
        Some(Outcome::Unknown)
    );
    assert!(e.store().orders().is_empty());
    no_trade(&a);
}

#[test]
fn native_terminal_cancel_diagnostic_is_not_amendment_proof() {
    for amend in [false, true] {
        for reply in [
            r#"{"status":"err","response":"Order was never placed, already canceled, or filled. asset=0"}"#,
            r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":[{"error":"Order was never placed, already canceled, or filled. asset=0"}]}}}"#,
        ] {
            let (mut e, socket) = boot();
            seed_order(&mut e, socket);
            let c = if amend {
                Case::Amend.command()
            } else {
                Case::Cancel.command()
            };
            let a = command(&mut e, NOW, c);
            let cancel = native_release(&mut e, &a);
            respond(&mut e, NOW + 1., &cancel, 200, reply);
            let a: Vec<_> = e.drain_actions().collect();
            let result = results(&a, OWNER);
            assert_eq!(result.len(), 1);
            if amend {
                assert!(
                    result[0]
                        .as_ref()
                        .unwrap_err()
                        .message
                        .contains("already canceled")
                );
                assert!(e.store().order("9").is_some());
            } else {
                assert_eq!(result, vec![Ok("[]".into())]);
                assert!(e.store().order("9").is_none());
            }
            no_trade(&a);
        }
    }
}

#[test]
fn native_late_source_fill_keeps_the_confirmed_replacement_identity() {
    let (mut e, socket) = boot();
    seed_order(&mut e, socket);
    let a = command(&mut e, NOW, Case::Amend.command());
    let cancel = native_release(&mut e, &a);
    respond(
        &mut e,
        NOW + 1.,
        &cancel,
        200,
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(results(&a, OWNER).is_empty());
    let a = prepare_for_pacing(&mut e, NOW + 1., &a);
    let timer = a
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 334. => Some(*timer),
            _ => None,
        })
        .expect("native replacement pacing timer");
    e.handle(NOW + 334., Event::TimerFired { timer });
    let a: Vec<_> = e.drain_actions().collect();
    let replacement = native_release_at(&mut e, NOW + 334., &a);
    let wire: Value = serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    let cloid = wire["action"]["orders"][0]["c"]
        .as_str()
        .unwrap()
        .to_owned();
    e.handle(NOW + 335., Event::WsMessage { socket, text: r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"0"},"status":"filled"}]}"# });
    let a: Vec<_> = e.drain_actions().collect();
    assert!(results(&a, OWNER).is_empty());
    no_trade(&a);
    respond(
        &mut e,
        NOW + 336.,
        &replacement,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":10}}]}}}"#,
    );
    let a: Vec<_> = e.drain_actions().collect();
    let result = results(&a, OWNER);
    assert_eq!(result.len(), 1);
    let ids: Value = serde_json::from_str(result[0].as_ref().unwrap()).unwrap();
    assert_eq!(ids["orderIds"], json!(["10"]));
    assert_eq!(ids["clientOrderId"], cloid);
    assert!(e.store().order("10").is_some());
    no_trade(&a);
}

#[test]
fn cited_margin_refusal_with_an_accepted_row_is_known_partial() {
    let (e, actions) = native_two_row_reply(
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":900}},{"error":"Insufficient margin to place order."}]}}}"#,
    );
    let terminal = results(&actions, OWNER);
    assert_eq!(terminal.len(), 1);
    let error = terminal[0].as_ref().unwrap_err();
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["900"]);
    assert!(e.store().order("900").is_some());
    no_trade(&actions);
}
