//! Real weighted info queue and native placement fence; no host cleanup claim.
use super::*;
use safe_cex_engine::request_context::Error;
use safe_cex_engine::socket_lifecycle::{
    EncodedBatch, Epoch, FeedId, Interest, LifecycleEvent, SocketError,
};
use safe_cex_engine::{Capabilities, Notice, Outcome, Store, VenueAction};

struct WeightedReads {
    inner: Hyperliquid,
}
impl Venue for WeightedReads {
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
    fn fence_uncertain_placements(&self) -> bool {
        self.inner.fence_uncertain_placements()
    }
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, symbols: Option<&[String]>) {
        self.inner.reconcile_uncertain(cx, symbols);
    }
    fn unproven_http_evidence(
        &self,
        pending: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        self.inner
            .unproven_http_evidence(pending, response, evidence);
    }
    fn read_root_retired(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
        pending: Vec<Pending>,
    ) {
        self.inner.read_root_retired(owner, pending);
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
        if matches!(&command, Command::SetTickerSymbols { symbols } if symbols == &["queue_orders"])
        {
            // Saturate through real native helpers, without changing queue or epoch state.
            for _ in 0..50 {
                self.inner.check_builder(cx);
            }
            self.inner.refresh_orders(cx);
        } else {
            self.inner.on_command(cx, command);
        }
    }
}

fn kind(request: &HttpRequest) -> String {
    #[derive(serde::Deserialize)]
    struct Read {
        #[serde(rename = "type")]
        kind: String,
    }
    serde_json::from_str::<Read>(request.body.as_deref().unwrap())
        .unwrap()
        .kind
}
fn bootstrap(spot: bool) -> Engine<WeightedReads> {
    let mut engine = connect_venue(spot, true, |inner| WeightedReads { inner });
    for _ in 0..20 {
        let actions = prepared_now(&mut engine);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::WsConnect { socket, .. } => {
                    engine.handle(1000., Event::WsOpened { socket })
                }
                Action::Http(request) => {
                    let body = match kind(&request).as_str() {
                        "metaAndAssetCtxs" => tuple(UNIVERSE, GOOD),
                        "spotMetaAndAssetCtxs" => tuple(SPOT_META, GOOD),
                        "perpDexs" => "[null]".into(),
                        "userAbstraction" => r#""disabled""#.into(),
                        "maxBuilderFee" => "30".into(),
                        "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#.into(),
                        "clearinghouseState" => r#"{"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"}}"#.into(),
                        "spotClearinghouseState" => r#"{"balances":[{"coin":"USDC","total":"1000","hold":"0"}]}"#.into(),
                        "frontendOpenOrders" => "[]".into(),
                        other => panic!("unexpected bootstrap {other}"),
                    };
                    reply(&mut engine, &request, 1000., &body);
                }
                _ => {}
            }
        }
    }
    assert!(!engine.store().markets().is_empty());
    engine
}
fn place(engine: &mut Engine<WeightedReads>, now: f64, command: u32, spot: bool) -> Vec<Action> {
    let symbol = if spot { "BTC/USDC" } else { "BTC-PERP" };
    engine.handle(
        now,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![
                serde_json::from_str(&format!(
                    r#"{{"symbol":"{symbol}","type":"limit","side":"buy","price":100,"amount":1}}"#
                ))
                .unwrap(),
            ],
        }),
    );
    prepared_now(engine)
}
fn assert_fenced(actions: &[Action], command: u32) {
    assert!(
        actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandFailed { command: id, error })
            if *id == command && error.outcome == Some(Outcome::NotSent)
                && error.message.contains("Placement outcome is uncertain")
        )),
        "queued pre-mark census must not release the fence: {actions:?}"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
}
fn queued_census_retains_its_creation_epoch(spot: bool) {
    let mut engine = bootstrap(spot);
    engine.handle(
        2000.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["queue_orders".into()],
        }),
    );
    let actions = prepared_now(&mut engine);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if kind(r) == "frontendOpenOrders"))
    );
    let (timer, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 60_000. => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("real weight-budget timer holds the logical Orders read");
    let mut builders = 0;
    for action in actions {
        if let Action::Http(request) = action {
            assert_eq!(kind(&request), "maxBuilderFee");
            builders += 1;
            reply(&mut engine, &request, 2000., "30");
        }
    }
    assert!(
        builders >= 40,
        "saturation must issue genuine weighted requests"
    );
    let actions = place(&mut engine, 3000., 101, spot);
    let write = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .expect("real signed placement before uncertainty");
    engine.handle(
        3400.,
        Event::HttpFailed {
            request: write.id,
            error: "placement reply lost",
        },
    );
    let actions = prepared_now(&mut engine);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 101, error }) if error.outcome == Some(Outcome::Unknown))));
    assert_fenced(&place(&mut engine, 4000., 102, spot), 102);
    engine.handle(at, Event::TimerFired { timer });
    let actions = prepared_now(&mut engine);
    let orders: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if kind(r) == "frontendOpenOrders" => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        orders.len(),
        1,
        "the queued logical read issues only after the mark"
    );
    for action in &actions {
        if let Action::Http(request) = action {
            match kind(request).as_str() {
                "maxBuilderFee" => reply(&mut engine, request, at, "30"),
                "spotClearinghouseState" => reply(&mut engine, request, at, r#"{"balances":[]}"#),
                "frontendOpenOrders" => {}
                other => panic!("unexpected queue drain {other}"),
            }
        }
    }
    reply(&mut engine, &orders[0], at + 1., "[]");
    let actions = prepared_now(&mut engine);
    let fresh = http(&actions, "frontendOpenOrders");
    assert_ne!(
        fresh.id, orders[0].id,
        "new owed successor has its own read epoch"
    );
    assert_fenced(&place(&mut engine, at + 2., 103, spot), 103);
    let coin = if spot { "@4" } else { "BTC" };
    let body = format!(
        r#"[{{"coin":"{coin}","oid":7,"side":"B","limitPx":"100","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"}}]"#
    );
    reply(&mut engine, &fresh, at + 3., &body);
    assert!(engine.store().orders().iter().any(|order| order.id == "7"));
    let actions = place(&mut engine, at + 400., 104, spot);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange"))),
        "fresh successor releases retry: {actions:?}"
    );
}
#[test]
fn perps_queued_census_cannot_release_a_newer_uncertain_placement() {
    queued_census_retains_its_creation_epoch(false);
}
#[test]
fn spot_queued_census_cannot_release_a_newer_uncertain_placement() {
    queued_census_retains_its_creation_epoch(true);
}
