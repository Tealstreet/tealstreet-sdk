//! Native seed controls and NEW managed background-observation boundaries.
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Method, Response};
use safe_cex_venue_backpack::{Backpack, Options};

type Driver = Engine<Backpack>;
const SYMBOL: &str = "SOL_USDC_PERP";
const MARKETS: &str = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","visible":true,"quantityIncrement":"0.01","priceIncrement":"0.1","minQuantity":"0.02","imfFunction":{"base":"0.02"}}]"#;
const DAILY: &str = r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.05","volume":"10","quoteVolume":"1410"}]"#;
const MARKS: &str = r#"[{"symbol":"SOL_USDC_PERP","markPrice":"140","indexPrice":"139","fundingRate":"0.001","nextFundingTimestamp":123456}]"#;

#[derive(Clone, Copy, Debug)]
enum Seed {
    Markets,
    Daily,
    Marks,
}
impl Seed {
    fn path(self) -> &'static str {
        match self {
            Self::Markets => "/api/v1/markets",
            Self::Daily => "/api/v1/tickers",
            Self::Marks => "/api/v1/markPrices",
        }
    }
    fn body(self) -> &'static str {
        match self {
            Self::Markets => MARKETS,
            Self::Daily => DAILY,
            Self::Marks => MARKS,
        }
    }
}
struct Packet {
    request: HttpRequest,
    preparation: Option<Preparation>,
}
fn packets(e: &mut Driver) -> Vec<Packet> {
    e.drain_actions()
        .filter_map(|action| match action {
            Action::Http(request) => Some(Packet {
                request,
                preparation: None,
            }),
            Action::ManagedHttp(action) => match *action {
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
fn take(all: &mut Vec<Packet>, seed: Seed) -> Packet {
    let index = all
        .iter()
        .position(|p| p.request.url.ends_with(seed.path()))
        .expect("native seed route");
    all.remove(index)
}
fn release(e: &mut Driver, packet: &Packet, now: f64) {
    if let Some(preparation) = packet.preparation {
        e.http_prepared(
            now,
            preparation,
            Prepared {
                id: packet.request.id.0,
                expires_at: None,
            },
        );
        let permit = e
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Bind {
                        permit, request, ..
                    } if request.id == packet.request.id => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("exact Bind after preparation");
        assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    }
}
fn response(e: &mut Driver, packet: &Packet, now: f64, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: packet.request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn fresh() -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.);
    e
}
fn issued(seed: Seed, released: bool) -> (Driver, Packet) {
    let mut e = fresh();
    let mut all = packets(&mut e);
    let markets = take(&mut all, Seed::Markets);
    if matches!(seed, Seed::Markets) {
        if released {
            release(&mut e, &markets, 0.);
        }
        return (e, markets);
    }
    release(&mut e, &markets, 0.);
    response(&mut e, &markets, 0., 200, MARKETS);
    let mut all = packets(&mut e);
    let target = take(&mut all, seed);
    let other = all.pop().expect("both derived observations");
    release(&mut e, &other, 0.);
    let other_body = if matches!(seed, Seed::Daily) {
        MARKS
    } else {
        DAILY
    };
    response(&mut e, &other, 0., 200, other_body);
    if released {
        release(&mut e, &target, 0.);
    }
    (e, target)
}
fn assert_unpublished(e: &Driver, seed: Seed) {
    if matches!(seed, Seed::Markets) {
        assert!(
            !e.store().loaded().markets,
            "unadmitted catalogue must not publish"
        );
    } else if matches!(seed, Seed::Daily) {
        assert!(
            !e.store().loaded().tickers,
            "unadmitted daily seed must not complete startup"
        );
        assert!(e.store().tickers().is_empty());
    } else if let Some(ticker) = e.store().ticker(SYMBOL) {
        // A genuine failure may unblock startup with the native last-price fallback.
        assert_eq!(
            (ticker.mark, ticker.index, ticker.funding_rate),
            (141., 141., 0.)
        );
    }
}
fn managed(seed: Seed) {
    let (mut e, packet) = issued(seed, false);
    assert!(
        packet.preparation.is_some(),
        "{seed:?}: physical public observation must start with managed Prepare, not raw HTTP"
    );
    release(&mut e, &packet, 0.);
    response(&mut e, &packet, 0., 200, seed.body());
    if matches!(seed, Seed::Markets) {
        assert!(e.store().loaded().markets);
    } else {
        assert!(e.store().loaded().tickers);
    }
}
#[test]
fn markets_prepare_original() {
    managed(Seed::Markets);
}
#[test]
fn daily_prepare_original() {
    managed(Seed::Daily);
}
#[test]
fn marks_prepare_original() {
    managed(Seed::Marks);
}
fn held(seed: Seed) {
    let (mut e, packet) = issued(seed, false);
    response(&mut e, &packet, 1., 200, seed.body());
    assert_unpublished(&e, seed);
}
#[test]
fn markets_held_prepare_cannot_accept_data() {
    held(Seed::Markets);
}
#[test]
fn daily_held_prepare_cannot_accept_data() {
    held(Seed::Daily);
}
#[test]
fn marks_held_prepare_cannot_accept_data() {
    held(Seed::Marks);
}
fn equality(seed: Seed) {
    let (mut e, packet) = issued(seed, true);
    response(&mut e, &packet, 20_000., 200, seed.body());
    assert_unpublished(&e, seed);
}
// NEW policy: the legacy raw observations have no 20-second managed bound.
#[test]
fn markets_new_twenty_second_equality() {
    equality(Seed::Markets);
}
#[test]
fn daily_new_twenty_second_equality() {
    equality(Seed::Daily);
}
#[test]
fn marks_new_twenty_second_equality() {
    equality(Seed::Marks);
}
fn late_malformed(seed: Seed) {
    let (mut e, packet) = issued(seed, true);
    e.drain_actions().for_each(drop);
    response(
        &mut e,
        &packet,
        20_000.,
        200,
        "[{\"symbol\":\"SOL_USDC_PERP\",\"bad\":true}]",
    );
    assert!(
        !e.drain_actions().any(
            |a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(ref n)) if n.code == safe_cex_engine::log::code::ROW_SKIPPED)
        ),
        "expired DATA cannot emit row diagnostics"
    );
}
#[test]
fn markets_expired_malformed_data_is_inert() {
    late_malformed(Seed::Markets);
}
#[test]
fn daily_expired_malformed_data_is_inert() {
    late_malformed(Seed::Daily);
}
#[test]
fn marks_expired_malformed_data_is_inert() {
    late_malformed(Seed::Marks);
}

#[test]
fn native_three_route_wire_and_financial_mapping_working_control() {
    for seed in [Seed::Markets, Seed::Daily, Seed::Marks] {
        let (mut e, packet) = issued(seed, true);
        assert_eq!(packet.request.method, Method::Get);
        assert_eq!(
            packet.request.url,
            format!("https://api.backpack.exchange{}", seed.path())
        );
        assert_eq!(packet.request.headers, [("X-Broker-Id", "1200".into())]);
        assert!(packet.request.body.is_none());
        response(&mut e, &packet, 1., 200, seed.body());
        let market = e.store().market(SYMBOL).unwrap();
        assert_eq!(
            (
                market.precision.amount,
                market.precision.price,
                market.limits.amount.min,
                market.limits.leverage.max
            ),
            (0.01, 0.1, 0.02, 50.)
        );
        if !matches!(seed, Seed::Markets) {
            let ticker = e.store().ticker(SYMBOL).unwrap();
            assert_eq!(
                (
                    ticker.last,
                    ticker.mark,
                    ticker.index,
                    ticker.percentage,
                    ticker.volume,
                    ticker.quote_volume,
                    ticker.funding_rate,
                    ticker.funding_time
                ),
                (141., 140., 139., 5., 10., 1410., 0.001, 123456.)
            );
        }
    }
}
#[test]
fn native_empty_marks_succeed_with_last_price_fallback() {
    let (mut e, packet) = issued(Seed::Marks, true);
    response(&mut e, &packet, 1., 200, "[]");
    assert!(e.store().loaded().tickers);
    let ticker = e.store().ticker(SYMBOL).unwrap();
    assert_eq!((ticker.last, ticker.mark, ticker.index), (141., 141., 141.));
}
#[test]
fn native_failed_marks_fallback_then_independent_recovery() {
    let (mut e, packet) = issued(Seed::Marks, true);
    e.handle(
        1.,
        Event::HttpFailed {
            request: packet.request.id,
            error: "temporary outage",
        },
    );
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().ticker(SYMBOL).unwrap().mark, 141.);
    let actions: Vec<_> = e.drain_actions().collect();
    let (timer, due) = actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (751. ..=1001.).contains(&at_ms) => {
                Some((timer, at_ms))
            }
            _ => None,
        })
        .expect("native jittered retry");
    e.handle(due, Event::TimerFired { timer });
    let packet = take(&mut packets(&mut e), Seed::Marks);
    release(&mut e, &packet, due);
    response(&mut e, &packet, due + 1., 200, MARKS);
    let ticker = e.store().ticker(SYMBOL).unwrap();
    assert_eq!((ticker.last, ticker.mark, ticker.index), (141., 140., 139.));
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}
#[test]
fn native_duplicate_success_does_not_replace_accepted_marks() {
    let (mut e, packet) = issued(Seed::Marks, true);
    response(&mut e, &packet, 1., 200, MARKS);
    response(
        &mut e,
        &packet,
        2.,
        200,
        r#"[{"symbol":"SOL_USDC_PERP","markPrice":"999","indexPrice":"999","fundingRate":"1","nextFundingTimestamp":999}]"#,
    );
    assert_eq!(e.store().ticker(SYMBOL).unwrap().mark, 140.);
}
#[test]
fn native_empty_catalogue_and_daily_keep_retry_ownership() {
    for seed in [Seed::Markets, Seed::Daily] {
        let (mut e, packet) = issued(seed, true);
        response(&mut e, &packet, 1., 200, "[]");
        assert_unpublished(&e, seed);
        let actions: Vec<_> = e.drain_actions().collect();
        let (timer, due) = actions
            .into_iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if (751. ..=1001.).contains(&at_ms) => {
                    Some((timer, at_ms))
                }
                _ => None,
            })
            .expect("native retry remains scheduled");
        e.handle(due, Event::TimerFired { timer });
        let next = take(&mut packets(&mut e), seed);
        release(&mut e, &next, due);
        response(&mut e, &next, due + 1., 200, seed.body());
        if matches!(seed, Seed::Markets) {
            assert!(e.store().loaded().markets);
        } else {
            assert!(e.store().loaded().tickers);
        }
    }
}

fn nonfinite_released_event(failed: bool) {
    for seed in [Seed::Markets, Seed::Daily, Seed::Marks] {
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let (mut e, packet) = issued(seed, true);
            e.drain_actions().for_each(drop);
            if failed {
                e.handle(
                    now,
                    Event::HttpFailed {
                        request: packet.request.id,
                        error: "nonfinite callback",
                    },
                );
            } else {
                response(&mut e, &packet, now, 200, seed.body());
            }
            assert_unpublished(&e, seed);
            let all: Vec<_> = e.drain_actions().collect();
            assert!(
                !all.iter().any(|a| matches!(a, Action::SetTimer { .. })),
                "nonfinite callback cannot schedule recovery"
            );
            assert!(
                !all.iter()
                    .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_))),
                "nonfinite callback cannot reissue work"
            );
        }
    }
}
#[test]
fn nonfinite_released_response_retires_without_recovery_timer() {
    nonfinite_released_event(false);
}
#[test]
fn nonfinite_released_failure_retires_without_recovery_timer() {
    nonfinite_released_event(true);
}

mod pressure {
    use super::*;
    use safe_cex_engine::book::Book;
    use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
    use safe_cex_engine::{Command, Cx, SocketId, TimerId, Venue, VenueError};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn policy() -> Policy {
        Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 60_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }
    }
    struct Audit {
        native: Backpack,
        peers: Vec<Context>,
        credits: Rc<RefCell<Vec<usize>>>,
    }
    impl Audit {
        fn probe(&self, cx: &mut Cx<'_, safe_cex_venue_backpack::Pending>) {
            let mut held = vec![];
            while let Ok(context) = cx.http_context(Scope::Background, policy()) {
                held.push(context);
                assert!(held.len() <= 256);
            }
            self.credits.borrow_mut().push(held.len());
            for context in held {
                cx.finish_http_context(&context).unwrap();
            }
        }
    }
    impl Venue for Audit {
        type Pending = safe_cex_venue_backpack::Pending;
        fn start(&mut self, cx: &mut Cx<'_, Self::Pending>) {
            for _ in 0..254 {
                self.peers
                    .push(cx.http_context(Scope::Background, policy()).unwrap());
            }
            self.native.start(cx);
        }
        fn on_open(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId) {
            self.native.on_open(cx, socket);
        }
        fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, text: &str) {
            self.native.on_message(cx, socket, text);
        }
        fn on_close(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, code: u16) {
            self.native.on_close(cx, socket, code);
        }
        fn on_item(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: &mut Self::Pending,
            item: &str,
        ) {
            self.native.on_item(cx, pending, item);
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            response: Response<'_>,
        ) {
            self.native.on_response(cx, pending, response);
            self.probe(cx);
        }
        fn on_request_failed(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            error: &str,
        ) {
            self.native.on_request_failed(cx, pending, error);
        }
        fn on_http_failed(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            error: VenueError,
        ) {
            self.native.on_http_failed(cx, pending, error);
        }
        fn on_http_preparation_failed(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            reason: safe_cex_engine::request_context::Error,
            error: VenueError,
        ) {
            self.native
                .on_http_preparation_failed(cx, pending, reason, error);
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Self::Pending>, timer: TimerId) {
            self.native.on_timer(cx, timer);
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command) {
            self.native.on_command(cx, command);
        }
        fn book(&self, symbol: &str) -> Option<&Book> {
            self.native.book(symbol)
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            self.native.books()
        }
    }
    #[test]
    fn catalogue_retires_before_derived_observations_with_254_real_peers() {
        let credits = Rc::new(RefCell::new(vec![]));
        let mut e = safe_cex_engine::test_support::seeded_engine(Audit {
            native: Backpack::new(Options {
                public_data_only: true,
                ..Default::default()
            }),
            peers: vec![],
            credits: credits.clone(),
        });
        e.start(0.);
        let all = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
        let markets = all
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.ends_with(Seed::Markets.path()) => Some(r),
                _ => None,
            })
            .unwrap();
        e.handle(
            1.,
            Event::HttpResponse {
                request: markets.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: MARKETS,
                },
            },
        );
        assert!(e.store().loaded().markets);
        let all: Vec<_> = e.drain_actions().collect();
        let preparations: Vec<_> = all
            .iter()
            .filter_map(|a| match a {
                Action::ManagedHttp(action) => match &**action {
                    HttpAction::Prepare { request, .. } => Some(request.url.as_str()),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(
            preparations,
            [
                "https://api.backpack.exchange/api/v1/tickers",
                "https://api.backpack.exchange/api/v1/markPrices"
            ],
            "retire completing catalogue before admitting both successors"
        );
        assert_eq!(
            &*credits.borrow(),
            &[0],
            "both remaining credits belong to the derived observations"
        );
        let mut requests = vec![];
        for action in all {
            if let Action::ManagedHttp(action) = action {
                if let HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } = *action
                {
                    e.http_prepared(
                        1.,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                    requests.push(request);
                }
            }
        }
        let permits: Vec<_> = e
            .drain_actions()
            .filter_map(|a| match a {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(permits.len(), 2);
        for permit in permits {
            assert!(matches!(e.claim_http(1., permit), Ok(Claim::Released(_))));
        }
        for request in requests {
            let body = if request.url.ends_with(Seed::Daily.path()) {
                DAILY
            } else {
                MARKS
            };
            e.handle(
                2.,
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
        assert_eq!(
            &*credits.borrow(),
            &[0, 1, 2],
            "each exact derived terminal returns one credit"
        );
        assert!(e.store().loaded().tickers);
        assert_eq!(e.store().ticker(SYMBOL).unwrap().mark, 140.);
    }
}
