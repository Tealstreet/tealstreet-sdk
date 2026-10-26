//! The venue FIFO must stay safe when readiness precedes a reconciliation notification.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{HttpOwnerRetirement, Plan, Preflight, ReconciliationData};
use safe_cex_engine::{
    Action, Command, Cx, Engine, HttpRequest, Notice, Response, SocketId, TimerId, Venue,
    VenueError,
};
use safe_cex_venue_lighter::Lighter;
use support::{META, NOW};
type Pending = <Lighter as Venue>::Pending;

// Hold only the reconciliation notification; admission, signing and retirement
// handles remain engine-issued. Disable the outer lock fence so both genuine
// commands exercise Lighter's own FIFO instead of the engine's deferred queue.
struct ReadyFirst {
    inner: Lighter,
    hold_reconcile: bool,
    retired_wake: bool,
    commands: usize,
}
impl Venue for ReadyFirst {
    type Pending = Pending;
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
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.inner.on_stream_error(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_http_failed(cx, pending, error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_sign_error(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.commands += 1;
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
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, symbols: Option<&[String]>) {
        if !self.hold_reconcile {
            self.inner.reconcile_uncertain(cx, symbols);
        }
    }
    fn guard_order_mutations(&self) -> bool {
        false
    }
}
fn http(engine: &mut Engine<ReadyFirst>, path: &str) -> HttpRequest {
    safe_cex_engine::test_support::prepared_actions(engine, NOW)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains(path) => Some(request),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {path}"))
}
#[test]
fn a_real_retired_setting_fences_its_fifo_before_the_reconcile_notification() {
    let mut options = support::options();
    options.testnet = true;
    let engine = support::try_engine_with(options, |inner| ReadyFirst {
        inner,
        hold_reconcile: true,
        retired_wake: false,
        commands: 0,
    })
    .unwrap();
    let (mut engine, _, _) = support::ready_engine_meta(engine, META);
    engine
        .handle_command(
            NOW,
            Command::SetLeverage {
                command: 401,
                symbol: "ETH".into(),
                leverage: 5.,
                side: None,
            },
            Some(30_000),
        )
        .unwrap();
    let nonce = http(&mut engine, "nextNonce");
    support::reply(&mut engine, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let write = http(&mut engine, "sendTx");
    engine
        .handle_command(
            NOW,
            Command::SetMarginType {
                command: 402,
                symbol: "ETH".into(),
                isolated: true,
            },
            Some(30_000),
        )
        .unwrap();
    assert_eq!(
        engine.venue().commands,
        2,
        "both genuine commands reached the venue FIFO"
    );
    let queued = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
    assert!(
        !queued.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.contains("sendTx"))
        )
    );
    support::reply(&mut engine, &write, 503, "unavailable");
    assert!(
        engine.venue().retired_wake,
        "genuine owner retirement returned the queued Preflight"
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
    assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 401, error }) if error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
    assert!(
        !actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.contains("sendTx"))
        )
    );
    let nonce = actions
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("nextNonce") => Some(request),
            _ => None,
        })
        .expect("retirement invalidates the consumed nonce before readiness");
    support::reply(&mut engine, &nonce, 200, r#"{"code":200,"nonce":20}"#);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
    assert!(
        !actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.contains("sendTx"))
        )
    );
    assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 402, error }) if error.message.contains("awaiting reconciliation"))));
}
