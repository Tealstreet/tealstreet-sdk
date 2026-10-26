//! Public catalog snapshots retain their exact managed Background attempt.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Permit, Policy, Preparation, Prepared,
    RetryPolicy, RoutingIntent,
};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Method, Notice, TimerId, VenueError};
use safe_cex_engine::{Command, Cx, Response, SocketId, Venue};
use safe_cex_venue_lighter::{Lighter, Pending};
use support::Options;
use support::{NOW, reply_at};
const META: &str = r#"{"code":200,"order_book_details":[{"market_id":1,"symbol":"ETH","status":"active","market_type":"perp","supported_size_decimals":3,"supported_price_decimals":2,"min_base_amount":"0.001","min_quote_amount":"1","min_initial_margin_fraction":500,"maintenance_margin_fraction":250},{"market_id":2,"symbol":"BTC","status":"active","market_type":"perp","supported_size_decimals":4,"supported_price_decimals":1,"min_base_amount":"0.0001","min_quote_amount":"1","min_initial_margin_fraction":500,"maintenance_margin_fraction":250},{"market_id":2048,"symbol":"ETH/USDC","status":"active","market_type":"spot"}]}"#;
fn engine(robinhood: bool, testnet: bool) -> Engine<Lighter> {
    support::engine(Options {
        robinhood,
        testnet,
        public_data_only: true,
        ..Default::default()
    })
}
fn start<V: Venue<Pending = Pending>>(e: &mut Engine<V>) -> Vec<Action> {
    e.start(NOW);
    e.drain_actions().collect()
}
fn no_command(a: &[Action]) {
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}
fn prep(a: &[Action], fallback: bool) -> (Preparation, HttpRequest) {
    no_command(a);
    assert!(
        !a.iter().any(|a| matches!(a, Action::Http(_))),
        "catalog waits for managed claim"
    );
    let entries: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(entries.len(), 1, "one catalog preparation");
    let (p, r) = entries.into_iter().next().unwrap();
    assert!(r.url.ends_with(if fallback {
        "/orderBooks"
    } else {
        "/orderBookDetails"
    }));
    (p, r)
}
fn bind<V: Venue<Pending = Pending>>(
    e: &mut Engine<V>,
    now: f64,
    p: Preparation,
) -> (Permit, HttpRequest) {
    e.http_prepared(
        now,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    a.into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("catalog binds exact preparation")
}
fn release<V: Venue<Pending = Pending>>(
    e: &mut Engine<V>,
    now: f64,
    p: Preparation,
) -> HttpRequest {
    let (permit, r) = bind(e, now, p);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    r
}
fn wire(r: &HttpRequest, robinhood: bool, testnet: bool, fallback: bool) {
    let host = match (robinhood, testnet) {
        (false, false) => "https://mainnet.zklighter.elliot.ai",
        (false, true) => "https://testnet-api.lighter.xyz",
        (true, false) => "https://api.rh.lighter.xyz",
        (true, true) => "https://api.rh-testnet.lighter.xyz",
    };
    assert_eq!(
        r.url,
        format!(
            "{host}/api/v1/{}",
            if fallback {
                "orderBooks"
            } else {
                "orderBookDetails"
            }
        )
    );
    assert_eq!(r.method, Method::Get);
    assert!(r.headers.is_empty());
    assert!(r.signer_headers.is_empty());
    assert!(r.body.is_none());
}
fn snapshot<V: Venue<Pending = Pending>>(e: &Engine<V>) {
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 2);
    let eth = e.store().market("ETH").unwrap();
    assert_eq!(eth.id, "1");
    assert_eq!(eth.precision.amount, 0.001);
    assert_eq!(eth.precision.price, 0.01);
    assert_eq!(eth.limits.leverage.max, 20.);
    assert!(e.store().market("ETH/USDC").is_none());
}
fn empty<V: Venue<Pending = Pending>>(e: &mut Engine<V>, now: f64) {
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
    assert!(e.take_frame(now).is_none());
}
fn deadline(a: &[Action]) -> TimerId {
    a.iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20000. => Some(*timer),
            _ => None,
        })
        .expect("original twenty-second attempt deadline")
}
#[test]
fn native_catalog_wire_and_codec_in_all_deployments() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let mut e = engine(robinhood, testnet);
            e.start(NOW);
            let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
            let r = a
                .into_iter()
                .find_map(|a| {
                    if let Action::Http(r) = a {
                        Some(r)
                    } else {
                        None
                    }
                })
                .unwrap();
            wire(&r, robinhood, testnet, false);
            reply_at(&mut e, NOW + 1., &r, 200, META);
            snapshot(&e);
            no_command(&e.drain_actions().collect::<Vec<_>>());
        }
    }
}
#[test]
fn native_primary_failure_uses_order_books_then_existing_backoff() {
    let mut e = engine(false, false);
    e.start(NOW);
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let r = a
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    reply_at(&mut e, NOW + 1., &r, 500, r#"{"code":500}"#);
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.);
    let fallback = a
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    wire(&fallback, false, false, true);
    reply_at(&mut e, NOW + 2., &fallback, 500, r#"{"code":500}"#);
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        a.iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..}if *at_ms>=NOW+752.&&*at_ms<=NOW+1002.))
    );
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    empty(&mut e, NOW + 2.);
    no_command(&a);
}
#[test]
fn native_fallback_alias_keeps_snapshot_schema() {
    let mut e = engine(false, false);
    e.start(NOW);
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    reply_at(
        &mut e,
        NOW + 1.,
        &r,
        200,
        r#"{"code":200,"order_book_details":[]}"#,
    );
    let fallback = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    wire(&fallback, false, false, true);
    reply_at(
        &mut e,
        NOW + 2.,
        &fallback,
        200,
        &META.replace("order_book_details", "order_books"),
    );
    snapshot(&e);
}
#[test]
fn managed_catalog_waits_for_claim_and_uses_exact_wire() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let mut e = engine(robinhood, testnet);
            let a = start(&mut e);
            let (p, unsigned) = prep(&a, false);
            wire(&unsigned, robinhood, testnet, false);
            empty(&mut e, NOW);
            let r = release(&mut e, NOW + 1., p);
            wire(&r, robinhood, testnet, false);
            reply_at(&mut e, NOW + 2., &r, 200, META);
            snapshot(&e);
        }
    }
}
#[test]
fn response_before_claim_cannot_publish_catalog() {
    let mut e = engine(false, false);
    let (p, r) = prep(&start(&mut e), false);
    reply_at(&mut e, NOW + 1., &r, 200, META);
    no_command(&e.drain_actions().collect::<Vec<_>>());
    empty(&mut e, NOW + 1.);
    let r = release(&mut e, NOW + 2., p);
    reply_at(&mut e, NOW + 3., &r, 200, META);
    snapshot(&e);
}
#[test]
fn preparation_refusal_keeps_native_fallback_and_discards_prefix() {
    let mut e = engine(false, false);
    let (p, _) = prep(&start(&mut e), false);
    e.http_preparation_failed(
        NOW + 1.,
        p,
        Error::State,
        VenueError::new("catalog fixture refusal"),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let (fallback, _) = prep(&a, true);
    empty(&mut e, NOW + 1.);
    let r = release(&mut e, NOW + 2., fallback);
    reply_at(
        &mut e,
        NOW + 3.,
        &r,
        200,
        &META.replace("order_book_details", "order_books"),
    );
    snapshot(&e);
}
#[test]
fn bound_catalog_expires_before_claim_and_late_claim_is_inert() {
    let mut e = engine(false, false);
    let a = start(&mut e);
    let timer = deadline(&a);
    let (p, _) = prep(&a, false);
    let (permit, r) = bind(&mut e, NOW + 1., p);
    e.handle(NOW + 20000., Event::TimerFired { timer });
    let a: Vec<_> = e.drain_actions().collect();
    let (fallback, _) = prep(&a, true);
    assert!(e.claim_http(NOW + 20000., permit).is_err());
    reply_at(&mut e, NOW + 20001., &r, 200, META);
    empty(&mut e, NOW + 20001.);
    let a: Vec<_> = e.drain_actions().collect();
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    let next = release(&mut e, NOW + 20002., fallback);
    reply_at(&mut e, NOW + 20003., &next, 200, META);
    snapshot(&e);
}
#[test]
fn released_timeout_and_late_response_cannot_replace_fresh_fallback() {
    let mut e = engine(false, false);
    let a = start(&mut e);
    let timer = deadline(&a);
    let (p, _) = prep(&a, false);
    let old = release(&mut e, NOW + 1., p);
    e.handle(NOW + 20000., Event::TimerFired { timer });
    let (p, _) = prep(&e.drain_actions().collect::<Vec<_>>(), true);
    reply_at(&mut e, NOW + 20001., &old, 200, META);
    empty(&mut e, NOW + 20001.);
    e.drain_actions().for_each(drop);
    let fresh = release(&mut e, NOW + 20002., p);
    reply_at(&mut e, NOW + 20003., &fresh, 200, META);
    snapshot(&e);
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. })),
        "fresh catalog completed native startup"
    );
    reply_at(&mut e, NOW + 20004., &old, 500, r#"{"code":500}"#);
    assert!(e.drain_actions().next().is_none());
    snapshot(&e);
}
#[test]
fn malformed_catalog_prefix_never_reaches_store() {
    let mut e = engine(false, false);
    let (p, _) = prep(&start(&mut e), false);
    let r = release(&mut e, NOW + 1., p);
    let malformed = format!("{}x", META);
    reply_at(&mut e, NOW + 2., &r, 200, &malformed);
    empty(&mut e, NOW + 2.);
    let a: Vec<_> = e.drain_actions().collect();
    let _ = prep(&a, true);
}
#[test]
fn completed_catalog_duplicate_response_is_inert() {
    let mut e = engine(false, false);
    let (p, _) = prep(&start(&mut e), false);
    let r = release(&mut e, NOW + 1., p);
    reply_at(&mut e, NOW + 2., &r, 200, META);
    snapshot(&e);
    e.drain_actions().for_each(drop);
    e.take_frame(NOW + 2.);
    reply_at(&mut e, NOW + 3., &r, 500, r#"{"code":500}"#);
    assert!(e.drain_actions().next().is_none());
    assert!(e.take_frame(NOW + 3.).is_none());
    snapshot(&e);
}

#[test]
fn streamed_prefix_then_transport_failure_is_discarded_before_fallback() {
    let mut e = engine(false, false);
    let (p, _) = prep(&start(&mut e), false);
    let old = release(&mut e, NOW + 1., p);
    e.handle(
        NOW + 2.,
        Event::HttpStart {
            request: old.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 2.,
        Event::HttpChunk {
            request: old.id,
            data: &META.as_bytes()[..META.len() - 2],
        },
    );
    empty(&mut e, NOW + 2.);
    no_command(&e.drain_actions().collect::<Vec<_>>());
    e.handle(
        NOW + 3.,
        Event::HttpFailed {
            request: old.id,
            error: "fixture disconnected",
        },
    );
    let (p, _) = prep(&e.drain_actions().collect::<Vec<_>>(), true);
    e.handle(NOW + 4., Event::HttpEnd { request: old.id });
    empty(&mut e, NOW + 4.);
    assert!(e.drain_actions().next().is_none());
    let fresh = release(&mut e, NOW + 5., p);
    reply_at(&mut e, NOW + 6., &fresh, 200, META);
    snapshot(&e);
}
struct CapacityProbe {
    venue: Lighter,
    roots: Vec<Context>,
}
impl Venue for CapacityProbe {
    type Pending = Pending;
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId) {
        self.venue.on_open(cx, s)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, text: &str) {
        self.venue.on_message(cx, s, text)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, code: u16) {
        self.venue.on_close(cx, s, code)
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx)
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if matches!(&command,Command::FetchOrderBook{symbol,..}if symbol=="__catalog_peer__") {
            if let Ok(root) = cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20000,
                pacing: None,
                retry: RetryPolicy::Never,
            }) {
                self.roots.push(root);
            }
        } else {
            self.venue.on_command(cx, command)
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response)
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_http_failed(cx, pending, error)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error)
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_sign_error(cx, pending, error)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer)
    }
}
#[test]
fn saturated_registry_frees_each_catalog_root_before_fallback_retry_and_success() {
    let mut e = safe_cex_engine::test_support::seeded_engine(CapacityProbe {
        venue: Lighter::new(
            (Options {
                public_data_only: true,
                ..Default::default()
            })
            .into(),
        ),
        roots: vec![],
    });
    for command in 1..=255 {
        e.handle(
            NOW,
            Event::Command(Command::FetchOrderBook {
                command,
                symbol: "__catalog_peer__".into(),
                limit: None,
            }),
        );
        assert!(e.drain_actions().next().is_none());
    }
    let (p, _) = prep(&start(&mut e), false);
    let r = release(&mut e, NOW + 1., p);
    e.handle(
        NOW + 2.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 500,
                headers: "",
                body: r#"{"code":500}"#,
            },
        },
    );
    let (p, _) = prep(&e.drain_actions().collect::<Vec<_>>(), true);
    let r = release(&mut e, NOW + 3., p);
    e.handle(
        NOW + 4.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 500,
                headers: "",
                body: r#"{"code":500}"#,
            },
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    let (timer, at) = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms < NOW + 20000. => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    let (p, _) = prep(&e.drain_actions().collect::<Vec<_>>(), false);
    let r = release(&mut e, at, p);
    e.handle(
        at,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: META,
            },
        },
    );
    snapshot(&e);
    e.drain_actions().for_each(drop);
    e.handle(
        at,
        Event::Command(Command::FetchOrderBook {
            command: 900,
            symbol: "__catalog_peer__".into(),
            limit: None,
        }),
    );
    assert!(
        e.drain_actions().next().is_none(),
        "success retired the remaining catalog root before20s"
    );
    e.handle(
        at,
        Event::Command(Command::FetchOrderBook {
            command: 901,
            symbol: "__catalog_peer__".into(),
            limit: None,
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:901,error})if error.kind==Some(safe_cex_engine::ErrorKind::Internal))));
}
