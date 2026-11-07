//! Spot and merged-spot candle requests require original Read custody.
use safe_cex_engine::Outcome;
use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy,
    RoutingIntent,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options, Pending};
const NOW: f64 = 1000.;
const OWNER: u32 = 700;
const SYMBOL: &str = "UBTC/USDC";
const META: &str = r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50}]},[{"midPx":"100","markPx":"100","oraclePx":"100"}]]"#;
const SPOT: &str = r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8},{"name":"UBTC","index":1,"szDecimals":5}],"universe":[{"name":"@1","tokens":[1,0],"index":1}]},[{"coin":"@1","midPx":"60000","markPx":"60010","prevDayPx":"59000","dayNtlVlm":"1000"}]]"#;
const BODY: &str = r#"{"type":"candleSnapshot","req":{"coin":"@1","interval":"1m","startTime":0,"endTime":60000}}"#;
const ROWS: &str = r#"[{"t":2000,"o":"2","h":"3","l":"1","c":"2","v":"4"},{"t":1000,"o":"1","h":"2","l":"1","c":"2","v":"3"}]"#;
struct Probe {
    inner: Hyperliquid,
    peers: Vec<Context>,
}
impl Venue for Probe {
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
        self.inner.start(cx)
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
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["retire"] => {
                cx.fail_context(&self.peers.remove(0), "original peer retired")
                    .unwrap();
            }
            _ => self.inner.on_command(cx, command),
        }
    }
}
fn reply(e: &mut Engine<Probe>, now: f64, r: &HttpRequest, status: u16, body: &str) {
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
    )
}
fn kind(r: &HttpRequest) -> String {
    serde_json::from_str::<serde_json::Value>(r.body.as_deref().unwrap()).unwrap()["type"]
        .as_str()
        .unwrap()
        .into()
}
fn boot(testnet: bool, merged: bool) -> Engine<Probe> {
    let options = Options {
        testnet,
        public_data_only: true,
        enable_spot_markets: merged,
        ..Default::default()
    };
    let inner = if merged {
        Hyperliquid::new(options)
    } else {
        Hyperliquid::new_spot(options)
    };
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner,
        peers: vec![],
    });
    e.start(NOW);
    let initial: Vec<_> = prepared_actions(&mut e, NOW)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    for r in &initial {
        match kind(r).as_str() {
            "metaAndAssetCtxs" => reply(&mut e, NOW, r, 200, META),
            "spotMetaAndAssetCtxs" => reply(&mut e, NOW, r, 200, SPOT),
            other => panic!("unexpected initial {other}"),
        }
    }
    let follow: Vec<_> = prepared_actions(&mut e, NOW)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    for r in &follow {
        assert_eq!(kind(r), "perpDexs");
        reply(&mut e, NOW, r, 200, "[null]")
    }
    e.drain_actions().for_each(drop);
    assert_eq!(e.store().market(SYMBOL).unwrap().id, "@1");
    e.take_frame(NOW);
    e
}
fn command(id: u32, symbol: &str, interval: Interval, from: f64, to: f64) -> Command {
    Command::FetchCandles {
        command: id,
        symbol: symbol.into(),
        interval,
        from_ms: Some(from),
        to_ms: Some(to),
        limit: None,
    }
}
fn ask(e: &mut Engine<Probe>, now: f64) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(command(OWNER, SYMBOL, Interval::M1, 0., 60000.)),
    );
    e.drain_actions().collect()
}
fn results(actions: &[Action], id: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn packet(actions: &[Action]) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            Action::ManagedHttp(h) => match h.as_ref() {
                HttpAction::Prepare { request, .. } => Some(request.clone()),
                _ => None,
            },
            _ => None,
        })
        .unwrap()
}
fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "spot candles must wait for managed preparation"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Sign { .. } | Action::SignedOutbound { .. }))
    );
    let p: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(h) => match h.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(p.len(), 1);
    p[0].clone()
}
fn wire(r: &HttpRequest, testnet: bool) {
    assert_eq!(r.method, Method::Post);
    assert_eq!(
        r.url,
        if testnet {
            "https://api.hyperliquid-testnet.xyz/info"
        } else {
            "https://api.hyperliquid.xyz/info"
        }
    );
    assert_eq!(r.body.as_deref(), Some(BODY));
    assert_eq!(r.headers, vec![("Content-Type", "application/json".into())]);
    assert!(r.signer_headers.is_empty())
}
fn release(e: &mut Engine<Probe>, p: Preparation, now: f64) -> HttpRequest {
    e.http_prepared(
        now,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, r) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(h) => match *h {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    r
}
fn result_rows(actions: &[Action]) {
    let out = results(actions, OWNER);
    assert_eq!(out.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(out[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows[0]["timestamp"], 2.);
    assert_eq!(rows[1]["timestamp"], 1.);
    assert_eq!(rows[0]["volume"], 4.)
}
fn error(actions: &[Action], message: &str, kind: Option<ErrorKind>) {
    let out = results(actions, OWNER);
    assert_eq!(out.len(), 1);
    let e = out[0].as_ref().unwrap_err();
    assert_eq!(e.message, message);
    assert_eq!(e.kind, kind);
    assert_eq!(e.outcome, None);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    )
}
fn peer(e: &mut Engine<Probe>, id: u32) {
    e.handle(
        NOW,
        Event::Command(command(id, "__peer", Interval::M1, 0., 60000.)),
    );
    assert!(e.drain_actions().next().is_none())
}
#[test]
fn native_wire_rows_and_validation_precedence_on_all_four_deployments() {
    for testnet in [false, true] {
        for merged in [false, true] {
            let mut e = boot(testnet, merged);
            e.handle(
                NOW,
                Event::Command(command(OWNER, "UNKNOWN", Interval::S1, 60001., 60000.)),
            );
            error(
                &e.drain_actions().collect::<Vec<_>>(),
                "Hyperliquid does not support 1s candles",
                None,
            );
            e.handle(
                NOW,
                Event::Command(command(OWNER, "UNKNOWN", Interval::M1, 60001., 60000.)),
            );
            error(
                &e.drain_actions().collect::<Vec<_>>(),
                "Market not found",
                None,
            );
            e.handle(
                NOW,
                Event::Command(command(OWNER, SYMBOL, Interval::M1, 60001., 60000.)),
            );
            error(
                &e.drain_actions().collect::<Vec<_>>(),
                "Invalid candle time range",
                None,
            );
            let raw = ask(&mut e, NOW);
            let request = packet(&raw);
            wire(&request, testnet);
            let actual = if let Some(p) = raw.iter().find_map(|a| match a {
                Action::ManagedHttp(h) => match h.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            }) {
                release(&mut e, p, NOW)
            } else {
                request
            };
            reply(&mut e, NOW + 1., &actual, 200, ROWS);
            result_rows(&e.drain_actions().collect::<Vec<_>>());
            assert!(e.take_frame(NOW + 1.).is_none());
        }
    }
}
fn parked(testnet: bool, merged: bool) {
    let mut e = boot(testnet, merged);
    let actions = ask(&mut e, NOW);
    let (p, r) = preparation(&actions);
    wire(&r, testnet);
    reply(&mut e, NOW + 1., &r, 200, ROWS);
    assert!(results(&e.drain_actions().collect::<Vec<_>>(), OWNER).is_empty());
    assert!(e.http_preparation_pending(p));
    let r = release(&mut e, p, NOW + 2.);
    wire(&r, testnet);
    reply(&mut e, NOW + 3., &r, 200, ROWS);
    result_rows(&e.drain_actions().collect::<Vec<_>>());
    reply(&mut e, NOW + 4., &r, 200, ROWS);
    assert!(results(&e.drain_actions().collect::<Vec<_>>(), OWNER).is_empty());
}
#[test]
fn spot_mainnet_prepare() {
    parked(false, false)
}
#[test]
fn spot_testnet_prepare() {
    parked(true, false)
}
#[test]
fn merged_mainnet_prepare() {
    parked(false, true)
}
#[test]
fn merged_testnet_prepare() {
    parked(true, true)
}
#[test]
fn preclaim_response_cannot_publish_or_consume_owner() {
    for merged in [false, true] {
        let mut e = boot(false, merged);
        let actions = ask(&mut e, NOW);
        let r = packet(&actions);
        reply(&mut e, NOW + 1., &r, 200, ROWS);
        assert!(
            results(&e.drain_actions().collect::<Vec<_>>(), OWNER).is_empty(),
            "unreleased spot response published DATA"
        );
        assert!(e.take_frame(NOW + 1.).is_none());
    }
}
#[test]
fn original_deadline_fences_preparation_and_released_callbacks() {
    for merged in [false, true] {
        for claimed in [false, true] {
            let mut e = boot(false, merged);
            let actions = ask(&mut e, NOW);
            let (p, r) = preparation(&actions);
            if claimed {
                release(&mut e, p, NOW + 1.);
            }
            reply(&mut e, NOW + 20_000., &r, 200, ROWS);
            let out = results(&e.drain_actions().collect::<Vec<_>>(), OWNER);
            assert_eq!(out.len(), 1);
            let er = out[0].as_ref().unwrap_err();
            assert_eq!(er.kind, Some(ErrorKind::Timeout));
            assert_eq!(
                er.outcome,
                if claimed {
                    None
                } else {
                    Some(Outcome::NotSent)
                }
            );
            assert!(!e.http_preparation_pending(p));
            reply(&mut e, NOW + 20_001., &r, 200, ROWS);
            assert!(results(&e.drain_actions().collect::<Vec<_>>(), OWNER).is_empty());
        }
    }
}
#[test]
fn preparation_refusal_is_typed_and_never_empty_success() {
    let mut e = boot(false, false);
    let actions = ask(&mut e, NOW);
    let (p, r) = preparation(&actions);
    e.http_preparation_failed(
        NOW + 1.,
        p,
        Error::State,
        VenueError::new("fixture preparation refused").kind(ErrorKind::InvalidInput),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let mut cancellations = 0;
    for action in &actions {
        match action {
            Action::Http(_) => panic!("refused preparation leaked HTTP"),
            Action::ManagedHttp(action) => {
                assert_eq!(
                    action.as_ref(),
                    &HttpAction::Cancel {
                        preparation: p,
                        signing: None
                    }
                );
                cancellations += 1;
            }
            _ => {}
        }
    }
    assert_eq!(
        cancellations, 1,
        "only the exact refused preparation is cancelled"
    );
    let out = results(&actions, OWNER);
    assert_eq!(out.len(), 1);
    let failure = out[0].as_ref().unwrap_err();
    assert_eq!(failure.message, "fixture preparation refused");
    assert_eq!(failure.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(failure.outcome, Some(Outcome::NotSent));
    reply(&mut e, NOW + 2., &r, 200, ROWS);
    assert!(results(&e.drain_actions().collect::<Vec<_>>(), OWNER).is_empty());
}
#[test]
fn invalid_clock_owns_local_refusal_instead_of_native_interval_error() {
    for merged in [false, true] {
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut e = boot(false, merged);
            e.handle(
                now,
                Event::Command(command(OWNER, "UNKNOWN", Interval::S1, 0., 60000.)),
            );
            error(
                &e.drain_actions().collect::<Vec<_>>(),
                "Invalid request context",
                Some(ErrorKind::InvalidInput),
            );
        }
    }
}
#[test]
fn saturation_owns_native_local_and_remote_read_then_reuses_freed_capacity() {
    for merged in [false, true] {
        let mut e = boot(false, merged);
        for id in 1..=256 {
            peer(&mut e, id)
        }
        e.handle(
            NOW,
            Event::Command(command(OWNER, "UNKNOWN", Interval::S1, 0., 60000.)),
        );
        error(
            &e.drain_actions().collect::<Vec<_>>(),
            "Engine request capacity reached",
            Some(ErrorKind::Internal),
        );
        error(
            &ask(&mut e, NOW),
            "Engine request capacity reached",
            Some(ErrorKind::Internal),
        );
        e.handle(
            NOW,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["retire".into()],
            }),
        );
        e.drain_actions().for_each(drop);
        let actions = ask(&mut e, NOW);
        let (p, _) = preparation(&actions);
        release(&mut e, p, NOW + 1.);
    }
}
#[test]
fn duplicate_command_leaves_retained_root_untouched() {
    for merged in [false, true] {
        let mut e = boot(false, merged);
        peer(&mut e, OWNER);
        let out = ask(&mut e, NOW);
        assert!(results(&out, OWNER).is_empty());
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
        e.handle(
            NOW,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["retire".into()],
            }),
        );
        let terminal: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            results(&terminal, OWNER)[0].as_ref().unwrap_err().message,
            "original peer retired"
        );
    }
}
