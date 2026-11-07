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
    fn enforces_reduce_only(&self, store: &safe_cex_engine::Store, symbol: &str) -> bool {
        self.inner.enforces_reduce_only(store, symbol)
    }
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, symbols: Option<&[String]>) {
        self.inner.reconcile_uncertain(cx, symbols);
    }
    fn refreshes_orders(&self) -> bool {
        self.inner.refreshes_orders()
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
            self.inner.refresh_orders(cx);
        } else if matches!(&command, Command::SetTickerSymbols { symbols } if symbols == &["queue_builders"])
        {
            for _ in 0..60 {
                self.inner.check_builder(cx);
            }
        } else if matches!(&command, Command::SetTickerSymbols { symbols } if symbols == &["metadata_xyz"])
        {
            self.inner.meta(cx, "xyz".into(), 110000);
        } else if matches!(&command, Command::SetTickerSymbols { symbols } if symbols == &["metadata_core_shift"])
        {
            self.inner.meta(cx, String::new(), 0);
        } else if matches!(&command, Command::SetTickerSymbols { symbols } if symbols == &["queue_modes"])
        {
            for _ in 0..60 {
                self.inner
                    .account_read(cx, "userAbstraction", None, account::Request::Mode, false);
            }
        } else if matches!(&command, Command::SetTickerSymbols { symbols } if symbols == &["builder_unapproved"])
        {
            self.inner.account.builder_approved = Some(false);
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
    bootstrap_with_dexes(spot, &[])
}
fn bootstrap_with_dexes(spot: bool, dexes: &[&str]) -> Engine<WeightedReads> {
    #[derive(serde::Serialize)]
    struct NamedDex<'a> {
        name: &'a str,
    }

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
                        "perpDexs" => serde_json::to_string(&std::iter::once(None).chain(dexes.iter().map(|name| Some(NamedDex { name }))).collect::<Vec<_>>()).unwrap(),
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
fn saturate(engine: &mut Engine<WeightedReads>, now: f64) -> (Vec<Action>, usize) {
    saturate_kind(engine, now, false)
}
fn saturate_kind(
    engine: &mut Engine<WeightedReads>,
    now: f64,
    financial: bool,
) -> (Vec<Action>, usize) {
    let mut next = symbols(
        engine,
        now,
        if financial {
            "queue_modes"
        } else {
            "queue_builders"
        },
    );
    let mut retained = Vec::new();
    let mut builders = 0;
    for _ in 0..100 {
        if next.is_empty() {
            return (retained, builders);
        }
        for action in next {
            if let Action::Http(request) = action {
                assert_eq!(
                    kind(&request),
                    if financial {
                        "userAbstraction"
                    } else {
                        "maxBuilderFee"
                    }
                );
                builders += 1;
                reply(
                    engine,
                    &request,
                    now,
                    if financial { r#""disabled""# } else { "30" },
                );
            } else {
                retained.push(action);
            }
        }
        next = prepared_now(engine);
    }
    panic!("weighted reads did not settle");
}

fn await_census(
    engine: &mut Engine<WeightedReads>,
    mut actions: Vec<Action>,
    timers: &mut Vec<(TimerId, f64)>,
    now: &mut f64,
) -> HttpRequest {
    let deadline = *now + 2000.;
    for _ in 0..100 {
        let mut census = None;
        for action in actions {
            match action {
                Action::Http(request) if request.url.ends_with("/info") => match kind(&request)
                    .as_str()
                {
                    "frontendOpenOrders" => {
                        assert!(census.replace(request).is_none(), "single-flight census")
                    }
                    "userAbstraction" => reply(engine, &request, *now, r#""disabled""#),
                    "maxBuilderFee" => reply(engine, &request, *now, "30"),
                    "spotClearinghouseState" => reply(engine, &request, *now, r#"{"balances":[]}"#),
                    other => panic!("unexpected queued peer {other}"),
                },
                Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                _ => {}
            }
        }
        if let Some(census) = census {
            return census;
        }
        actions = prepared_now(engine);
        if actions.is_empty() {
            timers.sort_by(|a, b| b.1.total_cmp(&a.1));
            let (timer, at) = timers.pop().expect("queued census has a refill timer");
            *now = now.max(at);
            assert!(
                *now <= deadline,
                "full-budget census refills within two seconds"
            );
            engine.handle(*now, Event::TimerFired { timer });
            actions = prepared_now(engine);
        }
    }
    panic!("queued census did not settle");
}

fn queued_census_retains_its_creation_epoch(spot: bool) {
    let mut engine = bootstrap(spot);
    let (saturated, financial) = saturate_kind(&mut engine, 2000., true);
    assert!(
        financial > 40 && financial < 50,
        "reconciliation shares the full 1000-weight quota"
    );
    assert!(
        saturated.iter().any(
            |a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms > 2000. && *at_ms <= 3200.)
        ),
        "continuous refill arms a bounded weight timer"
    );
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
            .any(|a| matches!(a, Action::Http(r) if kind(r) == "frontendOpenOrders")),
        "the reconciliation census is genuinely queued before uncertainty"
    );
    let placement_actions = place(&mut engine, 3000., 101, spot);
    let write = placement_actions
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
    let mut actions = prepared_now(&mut engine);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 101, error }) if error.outcome == Some(Outcome::Unknown))));
    let mut timers: Vec<_> = saturated
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    actions.extend(placement_actions);
    let mut at = 3400.;
    let old = await_census(&mut engine, actions, &mut timers, &mut at);
    assert_fenced(&place(&mut engine, at + 1., 102, spot), 102);
    at += 2.;
    reply(&mut engine, &old, at, "[]");
    assert_fenced(&place(&mut engine, at + 1., 103, spot), 103);
    at += 2.;
    let actions = prepared_now(&mut engine);
    let fresh = await_census(&mut engine, actions, &mut timers, &mut at);
    assert_ne!(
        fresh.id, old.id,
        "new owed successor has its own read epoch"
    );
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

fn symbols(engine: &mut Engine<WeightedReads>, now: f64, name: &str) -> Vec<Action> {
    engine.handle(
        now,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![name.to_owned()],
        }),
    );
    prepared_now(engine)
}

#[test]
fn a_fence_read_goes_ahead_of_queued_routine_reads() {
    let mut engine = bootstrap(false);
    let (actions, builders) = saturate(&mut engine, 2000.);
    assert!(
        builders > 0 && builders < 40,
        "routine reads cannot consume the critical reserve"
    );
    assert!(
        actions.iter().any(
            |a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms > 2000. && *at_ms <= 3200.)
        )
    );
    let actions = place(&mut engine, 3000., 101, false);
    let write = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(
        3400.,
        Event::HttpFailed {
            request: write.id,
            error: "placement reply lost",
        },
    );
    let first = prepared_now(&mut engine)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(kind(&r)),
            _ => None,
        })
        .expect("fence read uses reserved weight without waiting for a routine refill");
    assert_eq!(first, "frontendOpenOrders");
}

#[test]
fn a_placement_re_reads_a_stale_unapproved_builder_without_waiting() {
    let mut engine = bootstrap(false);
    symbols(&mut engine, 2000., "builder_unapproved");
    let builder_read = |actions: &[Action]| {
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/info") && kind(r) == "maxBuilderFee"))
    };
    let placed = |actions: &[Action]| {
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    };
    let actions = place(&mut engine, 120_000., 101, false);
    assert!(builder_read(&actions), "stale: re-read");
    assert!(placed(&actions), "the order is not held for it");
    for action in actions {
        if let Action::Http(request) = action {
            if request.url.ends_with("/info") {
                reply(&mut engine, &request, 120_001., "30");
            } else {
                reply(
                    &mut engine,
                    &request,
                    120_001.,
                    r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":5}}]}}}"#,
                );
            }
        }
    }
    assert_eq!(engine.venue().inner.account.builder_approved, Some(true));
}

fn queued_census_uses_a_bounded_slot(spot: bool, refresh_ask: bool) {
    let mut engine = bootstrap(spot);
    let blocked = symbols(&mut engine, 2000., "queue_builders");
    assert_eq!(
        blocked
            .iter()
            .filter(|action| matches!(action, Action::Http(_)))
            .count(),
        3
    );
    let queued = symbols(&mut engine, 2000., "queue_orders");
    assert!(
        !queued
            .iter()
            .any(|action| matches!(action, Action::Http(r) if kind(r) == "frontendOpenOrders"))
    );
    let placed = place(&mut engine, 3000., 101, spot);
    let write = placed
        .iter()
        .find_map(|action| match action {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .unwrap();
    engine.handle(
        3400.,
        Event::HttpFailed {
            request: write.id,
            error: "placement reply lost",
        },
    );
    if refresh_ask {
        engine.handle(
            3500.,
            Event::Command(Command::Venue {
                command: 200,
                action: VenueAction::RefreshOrders {
                    symbols: Some(vec!["BTC-PERP".into()]),
                },
            }),
        );
    }
    let actions = prepared_now(&mut engine);
    assert!(
        !actions.iter().any(|action| matches!(action,
        Action::Http(r) if kind(r) == "frontendOpenOrders")),
        "foreground Orders also obey the cap"
    );
    let builder = blocked
        .iter()
        .find_map(|action| match action {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    reply(&mut engine, builder, 3600., "30");
    let actions = prepared_now(&mut engine);
    let older = http(&actions, "frontendOpenOrders");
    assert_fenced(&place(&mut engine, 4000., 102, spot), 102);
    reply(&mut engine, &older, 4001., "[]");
    let actions = prepared_now(&mut engine);
    let fresh = http(&actions, "frontendOpenOrders");
    assert_ne!(fresh.id, older.id);
    assert_fenced(&place(&mut engine, 4002., 103, spot), 103);
    reply(&mut engine, &fresh, 4003., "[]");
    if refresh_ask {
        let frame = engine.snapshot_frame(4003.).to_vec();
        let (_, data) = safe_cex_engine::wire::sections(&frame)
            .find(|(section, _)| *section == safe_cex_engine::wire::section::ORDERS_READ)
            .unwrap();
        assert_eq!(
            f64::from_le_bytes(data[8..16].try_into().unwrap()),
            4001.,
            "fresh owed successor publishes its own attributable send time"
        );
    }
    let actions = place(&mut engine, 4400., 104, spot);
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, Action::Http(r) if r.url.ends_with("/exchange")))
    );
}

#[test]
fn perps_queued_census_and_fresh_successor_take_priority_within_the_cap() {
    queued_census_uses_a_bounded_slot(false, false);
}
#[test]
fn spot_queued_census_and_fresh_successor_take_priority_within_the_cap() {
    queued_census_uses_a_bounded_slot(true, false);
}

#[test]
fn queued_before_refresh_ask_keeps_its_epoch_and_owes_a_fresh_attributable_successor() {
    queued_census_uses_a_bounded_slot(false, true);
}

#[test]
fn local_catalogue_refresh_stays_scoped_but_asset_index_remaps_reconcile_every_dex() {
    let mut engine = bootstrap_with_dexes(false, &["xyz", "flx"]);
    assert!(engine.venue().inner.assets.contains_key("XYZ|BTC-PERP"));
    assert!(engine.venue().inner.assets.contains_key("FLX|BTC-PERP"));
    for (command, expected) in [
        ("metadata_xyz", vec!["xyz"]),
        ("metadata_core_shift", vec!["", "xyz", "flx"]),
    ] {
        let actions = symbols(&mut engine, 2000., command);
        let metadata = http(&actions, "metaAndAssetCtxs");
        let universe = if command == "metadata_core_shift" {
            r#"{"universe":[{"name":"ETH","szDecimals":4,"maxLeverage":20},{"name":"BTC","szDecimals":5,"maxLeverage":50}]}"#
        } else {
            UNIVERSE
        };
        reply(&mut engine, &metadata, 2000., &tuple(universe, GOOD));
        let mut scopes = std::collections::HashSet::new();
        for _ in 0..20 {
            let actions = prepared_now(&mut engine);
            if actions.is_empty() {
                break;
            }
            for action in actions {
                if let Action::Http(request) = action {
                    let request_kind = kind(&request);
                    #[derive(serde::Deserialize)]
                    struct Scope<'a> {
                        #[serde(borrow, default)]
                        dex: &'a str,
                    }
                    let body: Scope<'_> =
                        serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                    let dex = body.dex.to_owned();
                    let response = match request_kind.as_str() {
                        "clearinghouseState" => {
                            r#"{"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"}}"#
                        }
                        "frontendOpenOrders" => "[]",
                        other => panic!("unexpected metadata successor {other}"),
                    };
                    scopes.insert((request_kind, dex));
                    reply(&mut engine, &request, 2000., response);
                }
            }
        }
        let expected: std::collections::HashSet<_> = expected
            .into_iter()
            .flat_map(|dex| {
                ["clearinghouseState", "frontendOpenOrders"]
                    .map(|kind| (kind.to_owned(), dex.to_owned()))
            })
            .collect();
        assert_eq!(scopes, expected, "{command}");
    }
    assert_eq!(engine.venue().inner.assets["BTC-PERP"], 1);
    assert_eq!(engine.venue().inner.assets["ETH-PERP"], 0);
    assert_eq!(engine.venue().inner.assets["XYZ|BTC-PERP"], 110000);
    assert_eq!(engine.venue().inner.assets["FLX|BTC-PERP"], 120000);
}
