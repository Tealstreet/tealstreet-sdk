//! Genuine repair notices, installed signing identity and held-leg revocation.
mod support;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::repair::{Effect, RepairNotice, SubjectFact};
use safe_cex_engine::request_context::{
    Claim, Context, HttpAction, Operation, Policy, Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::signing::{
    Algorithm, CredentialSetupPolicy, IdentitySetupPolicy, KeyEncoding,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Method, Notice, ReadCredentials, Response,
    SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_backpack::{Backpack, Options, Pending};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[derive(Default)]
struct State {
    facts: RefCell<Vec<Vec<SubjectFact>>>,
    refused: Cell<usize>,
    reserve: Cell<usize>,
    peers: RefCell<Vec<Context>>,
    release_peers: Cell<bool>,
    probe_capacity: Cell<Option<usize>>,
    check_fence: Cell<bool>,
    checked_fences: Cell<usize>,
}
struct Audit {
    native: Backpack,
    state: Rc<State>,
}
type Driver = Engine<Audit>;
impl Venue for Audit {
    type Pending = Pending;
    fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
        self.native.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.native.books()
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.native.capabilities()
    }
    fn guard_order_mutations(&self) -> bool {
        self.native.guard_order_mutations()
    }
    fn cancel_needs_stored_order(&self) -> bool {
        self.native.cancel_needs_stored_order()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.native.start(cx)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId) {
        self.native.on_open(cx, s)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, c: u16) {
        self.native.on_close(cx, s, c)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, t: &str) {
        self.native.on_message(cx, s, t)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, t: TimerId) {
        if self.state.release_peers.replace(false) {
            for c in self.state.peers.borrow_mut().drain(..) {
                cx.finish_http_context(&c).unwrap();
            }
        }
        self.native.on_timer(cx, t)
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, c: Command) {
        self.native.on_command(cx, c)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, t: &str) {
        self.native.on_item(cx, p, t)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        if self.state.release_peers.replace(false) {
            for c in self.state.peers.borrow_mut().drain(..) {
                cx.finish_http_context(&c).unwrap();
            }
        }
        for _ in 0..self.state.reserve.replace(0) {
            self.state.peers.borrow_mut().push(
                cx.http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 120000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap(),
            );
        }
        let fence = self.state.check_fence.get().then(|| {
            cx.store
                .begin_orders_snapshot()
                .expect("genuine Store fence probe")
        });
        self.native.on_response(cx, p, r);
        if self.state.check_fence.replace(false) {
            assert!(
                !cx.store
                    .orders_snapshot_current(&fence.expect("issued probe")),
                "accepted write invalidates older Orders BEFORE notice refresh, even if mapping failed"
            );
            self.state
                .checked_fences
                .set(self.state.checked_fences.get() + 1);
        }
        for c in self.state.peers.borrow().iter() {
            cx.validate_http_context(c).unwrap();
        }
        if let Some(expected) = self.state.probe_capacity.take() {
            let mut issued = vec![];
            loop {
                match cx.http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 120000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                ) {
                    Ok(context) => issued.push(context),
                    Err(safe_cex_engine::request_context::Error::Capacity) => break,
                    Err(error) => panic!("unexpected capacity probe: {error:?}"),
                }
            }
            assert_eq!(
                issued.len(),
                expected,
                "all retired physical credits are reusable"
            );
            for context in issued {
                cx.finish_http_context(&context).unwrap();
            }
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
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, e: VenueError) {
        self.native.on_sign_error(cx, p, e)
    }
    fn capture_http_reconciliation(
        &self,
        p: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        self.native.capture_http_reconciliation(p)
    }
    fn unproven_http_evidence(
        &self,
        p: &Pending,
        r: Response<'_>,
        e: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        self.native.unproven_http_evidence(p, r, e)
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
    fn on_repair_notice(&mut self, cx: &mut Cx<'_, Pending>, n: RepairNotice<'_>) {
        match &n {
            RepairNotice::Settled { subjects, .. } => {
                self.state.facts.borrow_mut().push(subjects.to_vec())
            }
            RepairNotice::MintRefused { .. } => {
                self.state.refused.set(self.state.refused.get() + 1)
            }
        }
        self.native.on_repair_notice(cx, n)
    }
    fn socket_control(
        &self,
        s: &safe_cex_engine::Store,
        f: safe_cex_engine::socket_lifecycle::FeedId,
        a: bool,
        t: &str,
    ) -> safe_cex_engine::socket_lifecycle::Control {
        self.native.socket_control(s, f, a, t)
    }
    fn socket_startup_ready(&self, s: &safe_cex_engine::Store) -> bool {
        self.native.socket_startup_ready(s)
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        e: safe_cex_engine::socket_lifecycle::LifecycleEvent,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.native.on_socket_event(cx, e)
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        e: safe_cex_engine::socket_lifecycle::Epoch,
        p: Pending,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.native.on_socket_interests_released(cx, e, p)
    }
    fn encode_socket_interests(
        &self,
        s: &safe_cex_engine::Store,
        f: safe_cex_engine::socket_lifecycle::FeedId,
        b: bool,
        i: &[safe_cex_engine::socket_lifecycle::Interest],
        e: &mut dyn FnMut(
            safe_cex_engine::socket_lifecycle::EncodedBatch,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
    ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
        self.native.encode_socket_interests(s, f, b, i, e)
    }
}
struct Packet {
    request: HttpRequest,
    preparation: safe_cex_engine::request_context::Preparation,
    timer: TimerId,
}
fn capture(actions: Vec<Action>) -> Vec<Packet> {
    let mut timer = None;
    let mut result = vec![];
    for action in actions {
        match action {
            Action::SetTimer { timer: t, .. } => timer = Some(t),
            Action::Http(r) => assert_eq!(r.method, Method::Get, "no raw mutation"),
            Action::ManagedHttp(a) => {
                if let HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } = *a
                {
                    result.push(Packet {
                        request,
                        preparation,
                        timer: timer.expect("physical deadline precedes actual Prepare"),
                    });
                }
            }
            _ => {}
        }
    }
    result
}
fn drain(e: &mut Driver) -> Vec<Packet> {
    capture(e.drain_actions().collect())
}
fn bind(
    e: &mut Driver,
    p: &Packet,
    now: f64,
) -> (safe_cex_engine::request_context::Permit, HttpRequest) {
    e.http_prepared(
        now,
        p.preparation,
        Prepared {
            id: p.request.id.0,
            expires_at: None,
        },
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } if request.id == p.request.id => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("real Bind")
}
fn release(e: &mut Driver, p: &Packet, now: f64) -> HttpRequest {
    let (permit, r) = bind(e, p, now);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    r
}
fn reply(e: &mut Driver, r: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    )
}
fn old(fill: &str) -> String {
    support::ORDER
        .replace(
            "\"orderType\":\"Limit\"",
            "\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\"",
        )
        .replace(
            "\"executedQuantity\":\"0\"",
            &format!("\"executedQuantity\":\"{fill}\""),
        )
}
fn setup(primary: bool) -> (Driver, Rc<State>) {
    let seed = |key: &str, n: u8| LocalIdentity {
        key: Secret::new(key.into()),
        secret: Secret::new(STANDARD.encode([n; 32])),
        secret_encoding: KeyEncoding::Base64,
        ..Default::default()
    };
    let slot = IdentitySetupPolicy {
        signing_algorithm: Some(Algorithm::Ed25519),
        ..Default::default()
    };
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: primary.then(|| seed("installed-primary", 7)),
            secondary: Some(seed("installed-read", 9)),
        },
        CredentialSetupPolicy {
            primary: Some(slot),
            secondary: Some(slot),
        },
    )
    .unwrap();
    let options = Options {
        identity: identities.primary.clone(),
        read_identity: identities.secondary.clone(),
        read_credentials_status: Some(Ok(())),
        read_credentials: Some(ReadCredentials {
            key: "different-raw-read".into(),
            secret: STANDARD.encode([8; 32]),
            passphrase: None,
        }),
        key: Some("different-raw-primary".into()),
        secret: Some(STANDARD.encode([6; 32])),
        ..Default::default()
    };
    let state = Rc::new(State::default());
    let mut e = safe_cex_engine::test_support::seeded_engine(Audit {
        native: Backpack::new(options),
        state: state.clone(),
    });
    e.set_signer(Some(Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    e.start(1000.);
    (e, state)
}
fn discovery(primary: bool, body: &str) -> (Driver, Rc<State>, Vec<Packet>, Vec<String>, SocketId) {
    discovery_reserved(primary, body, 0)
}
fn discovery_reserved(
    primary: bool,
    body: &str,
    reserve: usize,
) -> (Driver, Rc<State>, Vec<Packet>, Vec<String>, SocketId) {
    let (mut e, state) = setup(primary);
    let first = drain(&mut e).pop().unwrap();
    let r = release(&mut e, &first, 1000.);
    reply(&mut e, &r, 1000., support::MARKETS);
    let mut queue: std::collections::VecDeque<_> = drain(&mut e).into();
    let mut socket = None;
    let mut work = vec![];
    let mut diagnostics = vec![];
    while let Some(p) = queue.pop_front() {
        assert_eq!(p.request.method, Method::Get);
        let r = release(&mut e, &p, 1000.);
        let response = if r.url.ends_with("/account") {
            r#"{"leverageLimit":"10"}"#
        } else if r.url.ends_with("/position") {
            support::POSITIONS
        } else if r.url.ends_with("/collateral") {
            support::BALANCE
        } else if r.url.ends_with("/orders") {
            body
        } else if r.url.ends_with("/tickers") {
            r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.01","volume":"10","quoteVolume":"1410"}]"#
        } else {
            "[]"
        };
        if r.url.ends_with("/orders") {
            assert!(
                r.headers
                    .iter()
                    .any(|(k, v)| *k == "X-API-Key" && v == "installed-read")
            );
        }
        if r.url.ends_with("/orders") {
            state.reserve.set(reserve);
        }
        reply(&mut e, &r, 1000., response);
        let mut actions = vec![];
        for a in e.drain_actions() {
            match &a {
                Action::WsConnect {
                    socket: s,
                    channel: Some(safe_cex_engine::SocketChannel::Private),
                    ..
                } => socket = Some(*s),
                Action::Notice(Notice::Event(event)) => diagnostics.push(event.message.clone()),
                _ => {}
            }
            actions.push(a);
        }
        for p in capture(actions) {
            if p.request.method == Method::Get {
                queue.push_back(p);
            } else {
                work.push(p);
            }
        }
    }
    (
        e,
        state,
        work,
        diagnostics,
        socket.expect("native private connect"),
    )
}
fn accepted_cancel(fill: &str) -> (Driver, Rc<State>, Packet, SocketId) {
    let (mut e, state, work, _, socket) = discovery(true, &format!("[{}]", old(fill)));
    let p = work
        .into_iter()
        .find(|p| p.request.method == Method::Delete)
        .expect("real cancel preparation");
    let r = release(&mut e, &p, 1000.);
    reply(
        &mut e,
        &r,
        1000.,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let p = drain(&mut e)
        .into_iter()
        .find(|p| p.request.method == Method::Post)
        .expect("real held successor");
    (e, state, p, socket)
}
fn open(e: &mut Driver, socket: SocketId) {
    e.handle(1000., Event::WsOpened { socket });
    // Opening produces real account refreshes; finish them before the experiment.
    let mut queue: std::collections::VecDeque<_> = drain(e).into();
    while let Some(p) = queue.pop_front() {
        let r = release(e, &p, 1000.);
        let body = if r.url.ends_with("/orders") {
            format!("[{}]", old("0"))
        } else if r.url.ends_with("/position") {
            support::POSITIONS.into()
        } else {
            support::BALANCE.into()
        };
        reply(e, &r, 1000., &body);
        queue.extend(drain(e));
    }
}
fn stream(e: &mut Driver, socket: SocketId, fill: &str) {
    stream_symbol(e, socket, fill, "SOL_USDC_PERP");
}
fn stream_symbol(e: &mut Driver, socket: SocketId, fill: &str, symbol: &str) {
    let text = format!(
        r#"{{"stream":"account.orderUpdate","data":{{"e":"orderUpdate","i":"123","E":2000000,"X":"New","s":"{symbol}","o":"Market","S":"Bid","P":"130","Y":"5","q":"5","z":"{fill}"}}}}"#
    );
    e.handle(
        2000.,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
}
fn exact_retirement(actions: &[Action], p: &Packet) {
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a,Action::ClearTimer{timer} if *timer==p.timer))
            .count(),
        1
    );
    assert_eq!(actions.iter().filter(|a|matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Cancel{preparation,..} if *preparation==p.preparation))).count(),1);
    assert!(!actions.iter().any(|a| match a {
        Action::Http(r) => r.method != Method::Get,
        Action::ManagedHttp(a) => match a.as_ref() {
            HttpAction::Prepare { request, .. } | HttpAction::Bind { request, .. } =>
                request.method != Method::Get,
            HttpAction::BindGroup { .. } => true,
            _ => false,
        },
        _ => false,
    }));
}
#[test]
fn secondary_only_snapshot_refuses_locally_once_for_two_candidates() {
    let second = old("0").replace("\"123\"", "\"456\"");
    let (e, state, work, diagnostics, _) = discovery(false, &format!("[{},{}]", old("0"), second));
    assert_eq!(
        e.store().orders().len(),
        2,
        "real accepted snapshot published"
    );
    assert!(work.is_empty());
    assert_eq!(
        state.refused.get(),
        0,
        "no engine mint call even to provoke NoPrimary"
    );
    assert!(state.facts.borrow().is_empty());
    assert_eq!(
        diagnostics
            .iter()
            .filter(|s| s.as_str() == "Automatic repair authority unavailable: NoPrimary")
            .count(),
        1
    );
}
#[test]
fn secondary_discovery_cancel_signs_with_the_installed_primary() {
    let (mut e, _, work, _, _) = discovery(true, &format!("[{}]", old("0")));
    let p = work
        .into_iter()
        .find(|p| p.request.method == Method::Delete)
        .unwrap();
    let r = release(&mut e, &p, 1000.);
    let rows: Vec<safe_cex_venue_backpack::sign::Params> = vec![
        [
            ("symbol".into(), "SOL_USDC_PERP".into()),
            ("orderId".into(), "123".into()),
        ]
        .into(),
    ];
    let good = safe_cex_venue_backpack::sign::Credentials::new(
        "installed-primary".into(),
        &STANDARD.encode([7; 32]),
    )
    .unwrap()
    .headers("orderCancel", &rows, 1000.);
    let raw = safe_cex_venue_backpack::sign::Credentials::new(
        "different-raw-primary".into(),
        &STANDARD.encode([6; 32]),
    )
    .unwrap()
    .headers("orderCancel", &rows, 1000.);
    assert_ne!(good, raw);
    assert_eq!(r.headers, good);
}
#[test]
fn suppression_projects_no_issued_placement_and_one_manual_notice() {
    let (mut e, state, work, _, _) = discovery(true, &format!("[{}]", old("2")));
    let p = work
        .into_iter()
        .find(|p| p.request.method == Method::Delete)
        .unwrap();
    let r = release(&mut e, &p, 1000.);
    reply(
        &mut e,
        &r,
        1000.,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"2"}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(state.facts.borrow().len(), 1);
    let facts = state.facts.borrow();
    assert_eq!(facts[0][0].cancel, Effect::Accepted);
    assert_eq!(facts[0][0].place, None);
    assert_eq!(actions.iter().filter(|a|matches!(a,Action::Notice(Notice::Event(ev)) if ev.message=="Backpack trigger repair could not prove a safe replacement; check the order manually")).count(),1);
    assert!(
        !capture(actions)
            .iter()
            .any(|p| p.request.method == Method::Post)
    );
}
#[test]
fn stream_evidence_revokes_the_exact_held_prepare_and_does_not_bind_it() {
    let (mut e, state, p, socket) = accepted_cancel("0");
    open(&mut e, socket);
    stream(&mut e, socket, "1");
    let actions: Vec<_> = e.drain_actions().collect();
    exact_retirement(&actions, &p);
    assert_eq!(state.facts.borrow().len(), 1);
    assert_eq!(state.facts.borrow()[0][0].cancel, Effect::Accepted);
    assert_eq!(state.facts.borrow()[0][0].place, Some(Effect::NotSent));
    e.http_prepared(
        2000.,
        p.preparation,
        Prepared {
            id: p.request.id.0,
            expires_at: None,
        },
    );
    assert!(!e.drain_actions().any(|a| match a {
        Action::Http(r) => r.method == Method::Post,
        Action::ManagedHttp(a) =>
            matches!(a.as_ref(),HttpAction::Bind{request,..} if request.method==Method::Post),
        _ => false,
    }));
}
#[test]
fn stream_evidence_revokes_an_issued_bind_and_old_claim_cannot_release() {
    let (mut e, state, p, socket) = accepted_cancel("0");
    open(&mut e, socket);
    let (permit, _) = bind(&mut e, &p, 1000.);
    stream(&mut e, socket, "1");
    let actions: Vec<_> = e.drain_actions().collect();
    exact_retirement(&actions, &p);
    assert_eq!(state.facts.borrow()[0][0].place, Some(Effect::NotSent));
    assert!(e.claim_http(2000., permit).is_err());
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a,Action::Http(r) if r.method==Method::Post))
    );
}

fn open_observation(
    e: &mut Driver,
    socket: SocketId,
    orders: &str,
    hold_peer: bool,
) -> Option<Packet> {
    e.handle(1000., Event::WsOpened { socket });
    let mut queue: std::collections::VecDeque<_> = drain(e).into();
    let mut peer = None;
    while let Some(p) = queue.pop_front() {
        assert_eq!(
            p.request.method,
            Method::Get,
            "opening helper never releases an unexpected mutation"
        );
        let r = release(e, &p, 1000.);
        if hold_peer && r.url.ends_with("/position") {
            peer = Some(p);
            continue;
        }
        let body = if r.url.ends_with("/orders") {
            orders
        } else if r.url.ends_with("/position") {
            support::POSITIONS
        } else {
            support::BALANCE
        };
        reply(e, &r, 1000., body);
        queue.extend(drain(e));
    }
    peer
}
fn rest_before_cancel(row: &str, expect_place: bool) {
    let (mut e, state, work, _, socket) = discovery(true, &format!("[{}]", old("0")));
    let cancel = work
        .into_iter()
        .find(|p| p.request.method == Method::Delete)
        .unwrap();
    let request = release(&mut e, &cancel, 1000.);
    open_observation(&mut e, socket, &format!("[{row}]"), false);
    assert_eq!(
        e.store().order("123").unwrap().filled,
        0.,
        "retained Store zero is not raw fill proof"
    );
    reply(
        &mut e,
        &request,
        1000.,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let work = capture(actions);
    assert_eq!(
        work.iter()
            .filter(|p| p.request.method == Method::Post)
            .count(),
        usize::from(expect_place)
    );
    if !expect_place {
        assert_eq!(state.facts.borrow().len(), 1);
        assert_eq!(state.facts.borrow()[0][0].cancel, Effect::Accepted);
        assert_eq!(state.facts.borrow()[0][0].place, None);
    }
}
fn ineligible() -> String {
    old("0").replacen("{", r#"{"triggerBy":"MarkPrice","#, 1)
}
#[test]
fn rest_candidate_ineligible_explicit_zero_still_permits_the_frozen_zero_fill_wire() {
    rest_before_cancel(&ineligible(), true);
}
#[test]
fn rest_candidate_ineligible_missing_fill_is_sticky_unknown() {
    rest_before_cancel(
        &ineligible().replace(r#""executedQuantity":"0","#, ""),
        false,
    );
}
#[test]
fn rest_candidate_ineligible_null_fill_is_sticky_unknown() {
    rest_before_cancel(
        &ineligible().replace(r#""executedQuantity":"0""#, r#""executedQuantity":null"#),
        false,
    );
}
#[test]
fn rest_candidate_ineligible_nonfinite_fill_is_sticky_unknown() {
    for n in ["NaN", "Infinity", "-Infinity"] {
        rest_before_cancel(
            &ineligible().replace(
                r#""executedQuantity":"0""#,
                &format!(r#""executedQuantity":"{n}""#),
            ),
            false,
        );
    }
}
#[test]
fn rest_rejected_row_retaining_store_zero_is_sticky_unknown() {
    rest_before_cancel(
        &ineligible().replace(r#""quantity":"5""#, r#""quantity":"bad""#),
        false,
    );
}
fn unlisted_stream_revokes(bind_first: bool) {
    let (mut e, state, p, socket) = accepted_cancel("0");
    let peer = open_observation(&mut e, socket, &format!("[{}]", old("0")), true).unwrap();
    let permit = bind_first.then(|| bind(&mut e, &p, 1000.).0);
    stream_symbol(&mut e, socket, "1", "UNLISTED_PERP");
    let actions: Vec<_> = e.drain_actions().collect();
    exact_retirement(&actions, &p);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::ClearTimer{timer} if *timer==peer.timer)),
        "independent peer deadline survives"
    );
    assert_eq!(state.facts.borrow().len(), 1);
    assert_eq!(state.facts.borrow()[0][0].cancel, Effect::Accepted);
    assert_eq!(state.facts.borrow()[0][0].place, Some(Effect::NotSent));
    if let Some(permit) = permit {
        assert!(e.claim_http(2000., permit).is_err());
    }
    e.http_prepared(
        2000.,
        p.preparation,
        Prepared {
            id: p.request.id.0,
            expires_at: None,
        },
    );
    assert!(!e.drain_actions().any(|a| match a {
        Action::Http(r) => r.method == Method::Post,
        Action::ManagedHttp(a) =>
            matches!(a.as_ref(),HttpAction::Bind{request,..} if request.method==Method::Post),
        _ => false,
    }));
    reply(&mut e, &peer.request, 2000., support::POSITIONS);
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a,Action::ClearTimer{timer} if *timer==peer.timer))
            .count(),
        1
    );
    assert!(
        !e.store().positions().is_empty(),
        "real independent peer publishes after revocation"
    );
    assert_eq!(
        state.facts.borrow().len(),
        1,
        "cleanup and duplicate prepare never resettle"
    );
}
#[test]
fn unlisted_symbol_revokes_held_prepare_before_early_return_and_peer_survives() {
    unlisted_stream_revokes(false);
}
#[test]
fn unlisted_symbol_revokes_held_bind_before_early_return_and_old_claim_fails() {
    unlisted_stream_revokes(true);
}

fn suppressed_initial(body: &str) {
    let (mut e, state, work, _, _) = discovery(true, body);
    assert_eq!(
        work.iter()
            .filter(|p| p.request.method == Method::Delete)
            .count(),
        1
    );
    let p = work
        .into_iter()
        .find(|p| p.request.method == Method::Delete)
        .unwrap();
    let r = release(&mut e, &p, 1000.);
    reply(
        &mut e,
        &r,
        1000.,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    assert!(
        !drain(&mut e)
            .iter()
            .any(|p| p.request.method == Method::Post)
    );
    assert_eq!(state.facts.borrow()[0][0].cancel, Effect::Accepted);
    assert_eq!(state.facts.borrow()[0][0].place, None);
}
#[test]
fn initial_duplicate_zero_then_two_preserves_every_rows_fill_evidence() {
    suppressed_initial(&format!("[{},{}]", old("0"), old("2")));
}
#[test]
fn initial_unidentified_row_cannot_lend_default_zero_proof_to_another_candidate() {
    suppressed_initial(&format!(
        r#"[{},{{"symbol":"SOL_USDC_PERP","executedQuantity":"0"}}]"#,
        old("0")
    ));
}

#[test]
fn completing_read_and_cancel_reuse_the_sole_credit_beside_255_genuine_peers() {
    let (mut e, state, work, _, _) = discovery_reserved(true, &format!("[{}]", old("0")), 255);
    assert_eq!(state.peers.borrow().len(), 255);
    let cancel = work
        .into_iter()
        .find(|p| p.request.method == Method::Delete)
        .expect("completing Read frees sole credit for cancel");
    let r = release(&mut e, &cancel, 1000.);
    reply(
        &mut e,
        &r,
        1000.,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let place = drain(&mut e)
        .into_iter()
        .find(|p| p.request.method == Method::Post)
        .expect("cancel retirement frees sole successor credit");
    assert_eq!(state.peers.borrow().len(), 255);
    let r = release(&mut e, &place, 1000.);
    state.release_peers.set(true);
    let replacement = old("0").replace(r#""123""#, r#""replacement""#).replacen(
        "{",
        r#"{"triggerBy":"MarkPrice","#,
        1,
    );
    reply(&mut e, &r, 1000., &replacement);
    assert!(state.peers.borrow().is_empty());
    assert_eq!(state.facts.borrow().len(), 1);
    assert_eq!(state.facts.borrow()[0][0].cancel, Effect::Accepted);
    assert_eq!(state.facts.borrow()[0][0].place, Some(Effect::Accepted));
    let mut refreshes = drain(&mut e);
    assert_eq!(refreshes.len(), 3);
    while let Some(p) = refreshes.pop() {
        let r = release(&mut e, &p, 1000.);
        let body = if r.url.ends_with("/orders") {
            format!("[{replacement}]")
        } else if r.url.ends_with("/position") {
            support::POSITIONS.into()
        } else {
            support::BALANCE.into()
        };
        if refreshes.is_empty() {
            state.probe_capacity.set(Some(256));
        }
        reply(&mut e, &r, 1000., &body);
        assert!(drain(&mut e).is_empty());
    }
    assert!(state.probe_capacity.get().is_none());
}

#[test]
fn already_released_successor_keeps_unknown_evidence_and_never_becomes_not_sent() {
    let (mut e, state, p, socket) = accepted_cancel("0");
    open(&mut e, socket);
    let request = release(&mut e, &p, 1000.);
    stream(&mut e, socket, "1");
    let actions: Vec<_> = e.drain_actions().collect();
    exact_retirement(&actions, &p);
    assert_eq!(state.facts.borrow().len(), 1);
    assert_eq!(state.facts.borrow()[0][0].cancel, Effect::Accepted);
    assert_eq!(state.facts.borrow()[0][0].place, Some(Effect::Unknown));
    assert_eq!(e.store().order("123").unwrap().filled, 1.);
    reply(
        &mut e,
        &request,
        2000.,
        &old("0").replace(r#""123""#, r#""late-replacement""#),
    );
    assert!(e.store().order("late-replacement").is_none());
    assert!(
        !drain(&mut e)
            .iter()
            .any(|p| p.request.method == Method::Post)
    );
    assert_eq!(state.facts.borrow().len(), 1);
}

#[test]
fn accepted_place_with_malformed_mapping_keeps_identity_and_fences_an_older_orders_read() {
    let (mut e, state, p, socket) = accepted_cancel("0");
    e.handle(1000., Event::WsOpened { socket });
    let work = drain(&mut e);
    let mut held = None;
    for packet in work {
        let r = release(&mut e, &packet, 1000.);
        if r.url.ends_with("/orders") {
            held = Some(r);
            continue;
        }
        reply(
            &mut e,
            &r,
            1000.,
            if r.url.ends_with("/position") {
                support::POSITIONS
            } else {
                support::BALANCE
            },
        );
        assert!(drain(&mut e).is_empty());
    }
    let held = held.expect("genuine older released Orders observation");
    let r = release(&mut e, &p, 1000.);
    let malformed = old("0")
        .replace(r#""123""#, r#""replacement""#)
        .replace(r#""quantity":"5""#, r#""quantity":null"#)
        .replace(r#","triggerQuantity":"5""#, "");
    state.check_fence.set(true);
    reply(&mut e, &r, 1000., &malformed);
    assert_eq!(state.checked_fences.get(), 1);
    assert!(e.store().order("123").is_some());
    assert!(e.store().order("replacement").is_none());
    assert_eq!(state.facts.borrow()[0][0].place, Some(Effect::Accepted));
    e.drain_actions().for_each(drop);
    reply(&mut e, &held, 1000., &format!("[{}]", old("0")));
    assert_eq!(
        e.store().order("123").unwrap().status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
    assert!(
        !drain(&mut e)
            .iter()
            .any(|p| p.request.method != Method::Get)
    );
}

fn background_delivery_revokes_successor(bind_first: bool) {
    let (mut e, state, p, socket) = accepted_cancel("0");
    e.handle(1000., Event::WsOpened { socket });
    let work = drain(&mut e);
    let mut orders = None;
    for packet in work {
        let r = release(&mut e, &packet, 1000.);
        if r.url.ends_with("/orders") {
            orders = Some(r);
            continue;
        }
        reply(
            &mut e,
            &r,
            1000.,
            if r.url.ends_with("/position") {
                support::POSITIONS
            } else {
                support::BALANCE
            },
        );
        assert!(drain(&mut e).is_empty());
    }
    let permit = bind_first.then(|| bind(&mut e, &p, 1000.).0);
    let body = old("1").replacen("{", r#"{"triggerBy":"MarkPrice","#, 1);
    reply(&mut e, &orders.unwrap(), 1000., &format!("[{body}]"));
    let actions: Vec<_> = e.drain_actions().collect();
    exact_retirement(&actions, &p);
    assert_eq!(e.store().order("123").unwrap().filled, 1.);
    if let Some(permit) = permit {
        assert!(e.claim_http(1000., permit).is_err());
    }
    e.http_prepared(
        1000.,
        p.preparation,
        Prepared {
            id: p.request.id.0,
            expires_at: None,
        },
    );
    assert!(!e.drain_actions().any(|a| match a {
        Action::Http(r) => r.method == Method::Post,
        Action::ManagedHttp(a) =>
            matches!(a.as_ref(),HttpAction::Bind{request,..} if request.method==Method::Post),
        _ => false,
    }));
    assert_eq!(
        state.facts.borrow().len(),
        1,
        "Background delivery must settle the OTHER repair owner once (#2202)"
    );
    assert_eq!(state.facts.borrow()[0][0].cancel, Effect::Accepted);
    assert_eq!(state.facts.borrow()[0][0].place, Some(Effect::NotSent));
}
#[test]
fn background_fill_revokes_held_prepare_and_settles_the_other_root() {
    background_delivery_revokes_successor(false);
}
#[test]
fn background_fill_revokes_held_bind_and_settles_the_other_root() {
    background_delivery_revokes_successor(true);
}

#[test]
fn held_repair_refuses_a_conflicting_real_command_without_losing_its_own_custody() {
    let (mut e, state, work, _, _) = discovery(true, &format!("[{}]", old("0")));
    let p = work
        .into_iter()
        .find(|p| p.request.method == Method::Delete)
        .unwrap();
    e.handle(
        1000.,
        Event::Command(Command::CancelOrders {
            command: 901,
            ids: vec!["123".into()],
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 901, .. })
    )));
    assert!(
        capture(actions).is_empty(),
        "conflicting command never issues a physical mutation"
    );
    assert!(e.http_preparation_pending(p.preparation));
    assert!(state.facts.borrow().is_empty());
    let r = release(&mut e, &p, 1000.);
    reply(
        &mut e,
        &r,
        1000.,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    assert_eq!(
        drain(&mut e)
            .iter()
            .filter(|p| p.request.method == Method::Post)
            .count(),
        1
    );
}
#[test]
fn held_real_command_refuses_repair_admission_without_consuming_command_work() {
    let (mut e, state, work, _, socket) = discovery(true, &format!("[{}]", ineligible()));
    assert!(work.is_empty());
    e.handle(
        1000.,
        Event::Command(Command::CancelOrders {
            command: 902,
            ids: vec!["123".into()],
        }),
    );
    let p = drain(&mut e)
        .into_iter()
        .find(|p| p.request.method == Method::Delete)
        .expect("real foreground cancel");
    open_observation(&mut e, socket, &format!("[{}]", old("0")), false);
    assert!(state.facts.borrow().is_empty());
    assert!(e.http_preparation_pending(p.preparation));
    let r = release(&mut e, &p, 1000.);
    reply(
        &mut e,
        &r,
        1000.,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 902, .. })))
    );
    assert!(
        !capture(actions)
            .iter()
            .any(|p| p.request.method != Method::Get)
    );
    assert!(state.facts.borrow().is_empty());
}
