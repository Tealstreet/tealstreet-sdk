//! The trading FIFO stays safe when a retired owner's queued successor wakes
//! before the engine's reconciliation notification (dazzy3's H1 control, after
//! Lighter's `retirement_ready.rs`).
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    ConditionOwner, HttpOwnerRetirement, Plan, Preflight, ReconciliationData,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Notice, Response, SocketId, TimerId, Venue,
    VenueError,
};
use safe_cex_venue_orderly::Orderly;
use support::{ORDER, POSITIONS, account_body};
type Pending = <Orderly as Venue>::Pending;

// Hold only the reconciliation notification until a retired owner's successor
// has woken; admission, signing and retirement stay engine-issued.
struct ReadyFirst {
    inner: Orderly,
    hold_reconcile: bool,
    retired_wake: bool,
}
impl Venue for ReadyFirst {
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.inner.capabilities()
    }
    fn cancel_needs_stored_order(&self) -> bool {
        self.inner.cancel_needs_stored_order()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: safe_cex_engine::request_context::Error,
        error: VenueError,
    ) {
        self.inner
            .on_http_preparation_failed(cx, pending, reason, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_http_failed(cx, pending, error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_sign_error(cx, pending, error);
    }
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        status: u16,
        headers: &str,
        error: &str,
    ) {
        self.inner
            .on_stream_malformed(cx, pending, status, headers, error);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.inner.on_command(cx, command);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
        self.inner.capture_http_reconciliation(pending)
    }
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &HttpOwnerRetirement,
    ) -> Option<Preflight> {
        let next = self.inner.on_http_owner_retired_preflight(owner);
        self.retired_wake |= next.is_some();
        next
    }
    fn on_http_preflight_ready(&mut self, cx: &mut Cx<'_, Pending>, preflight: &Preflight) {
        self.inner.on_http_preflight_ready(cx, preflight);
        if self.retired_wake {
            self.hold_reconcile = false;
        }
    }
    fn on_http_plan_ready(&mut self, cx: &mut Cx<'_, Pending>, plan: &Plan) {
        self.inner.on_http_plan_ready(cx, plan);
    }
    fn on_condition_wait_expired(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        owner: &ConditionOwner,
        error: VenueError,
    ) {
        self.inner.on_condition_wait_expired(cx, owner, error);
    }
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, symbols: Option<&[String]>) {
        if !self.hold_reconcile {
            self.inner.reconcile_uncertain(cx, symbols);
        }
    }
}

const NOW: f64 = 1000.0;

/// Prepared actions, firing engine wakes already due (a woken Preflight).
fn out(engine: &mut Engine<ReadyFirst>) -> Vec<Action> {
    let mut all = vec![];
    loop {
        let actions = safe_cex_engine::test_support::prepared_actions(engine, NOW);
        let due: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms <= NOW => Some(*timer),
                _ => None,
            })
            .collect();
        all.extend(actions);
        if due.is_empty() {
            return all;
        }
        for timer in due {
            engine.handle(NOW, Event::TimerFired { timer });
        }
    }
}
fn reply(engine: &mut Engine<ReadyFirst>, r: &HttpRequest, status: u16, body: &str) {
    engine.handle(
        NOW,
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
fn http(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect()
}
fn place(engine: &mut Engine<ReadyFirst>, command: u32, price: f64) {
    let orders = serde_json::from_str(&format!(
        r#"[{{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":{price}}}]"#
    ))
    .unwrap();
    engine.handle(
        NOW,
        Event::Command(Command::PlaceOrders { command, orders }),
    );
}

/// A keyed Orderly behind the wrapper, bootstrapped with every read answered.
fn ready() -> Engine<ReadyFirst> {
    let (signer, options) = support::trusted(support::options());
    let identities = options.identities.clone();
    let mut engine = safe_cex_engine::test_support::seeded_engine(ReadyFirst {
        inner: Orderly::new(options),
        hold_reconcile: true,
        retired_wake: false,
    });
    engine.set_signer(Some(signer.unwrap())).unwrap();
    engine.set_identities(identities).unwrap();
    engine.start(NOW);
    let mut private = None;
    for _ in 0..10 {
        let actions = out(&mut engine);
        if actions.is_empty() {
            break;
        }
        for a in &actions {
            if let Action::WsConnect { socket, url, .. } = a
                && url.contains("private")
            {
                private = Some(*socket);
            }
        }
        for r in http(&actions) {
            reply(
                &mut engine,
                &r,
                200,
                &account_body(&r.url, ORDER, "[]", POSITIONS),
            );
        }
    }
    let socket = private.expect("the private socket");
    engine.handle(NOW, Event::WsOpened { socket });
    out(&mut engine);
    engine.handle(
        NOW,
        Event::WsMessage {
            socket,
            text: r#"{"event":"auth","success":true}"#,
        },
    );
    for _ in 0..4 {
        let actions = out(&mut engine);
        let reads = http(&actions);
        if reads.is_empty() {
            break;
        }
        for r in reads {
            reply(
                &mut engine,
                &r,
                200,
                &account_body(&r.url, ORDER, "[]", POSITIONS),
            );
        }
    }
    engine
}

#[test]
fn a_retired_pair_fences_its_queued_protection_before_the_reconcile_notification() {
    let mut engine = ready();
    place(&mut engine, 1, 49000.0);
    let pair = http(&out(&mut engine))
        .into_iter()
        .find(|r| r.url.ends_with("/v1/algo/order"))
        .expect("the first positional pair");
    place(&mut engine, 2, 48000.0);
    assert!(http(&out(&mut engine)).is_empty(), "the protection parks");
    // The engine seals the lost pair itself and wakes the successor; the
    // reconciliation notification is held until after that wake.
    reply(&mut engine, &pair, 408, "request timeout");
    let after = out(&mut engine);
    assert!(
        !http(&after)
            .iter()
            .any(|r| r.method != safe_cex_engine::Method::Get),
        "no second TP/SL while the first pair's state is unknown: {after:?}"
    );
    assert!(
        after.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 2, error })
                if error.message.contains("protective state")
                    && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
        )),
        "the successor is fenced: {after:?}"
    );
}
