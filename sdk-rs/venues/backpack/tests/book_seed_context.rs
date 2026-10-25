//! Unsigned subscribed-book seeds keep native book facts under managed custody.
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Response, SocketId, Venue,
};
use safe_cex_venue_backpack::{Backpack, Options};
type Driver = Engine<Backpack>;
const SYMBOL: &str = "SOL_USDC_PERP";
const SNAP: &str =
    r#"{"lastUpdateId":10,"bids":[["140","1"],["139","2"]],"asks":[["142","1"],["143","2"]]}"#;
fn snapshot_handoff<V: Venue>(
    e: &mut Engine<V>,
    now: f64,
    mut queued: Vec<Action>,
    excluded: Option<safe_cex_engine::TimerId>,
) -> Vec<Action> {
    let timers: Vec<_> = queued
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == now && Some(*timer) != excluded => {
                Some(*timer)
            }
            _ => None,
        })
        .collect();
    for timer in timers {
        e.handle(now, Event::TimerFired { timer });
        queued.extend(e.drain_actions());
    }
    queued
}
fn actions(e: &mut Driver, now: f64) -> Vec<Action> {
    let mut queued = safe_cex_engine::test_support::prepared_actions(e, now);
    let timers: Vec<_> = queued
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == now => Some(*timer),
            _ => None,
        })
        .collect();
    for timer in timers {
        e.handle(now, Event::TimerFired { timer });
        queued.extend(safe_cex_engine::test_support::prepared_actions(e, now));
    }
    queued
}

fn http(e: &mut Driver, needle: &str, now: f64) -> HttpRequest {
    actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(needle) => Some(r),
            _ => None,
        })
        .expect(needle)
}
fn reply_at(e: &mut Driver, r: &HttpRequest, now: f64, body: &str) {
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
    );
}
fn reply(e: &mut Driver, r: &HttpRequest, body: &str) {
    reply_at(e, r, 0., body);
}
fn seed() -> (Driver, SocketId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = http(&mut e, "markets", 0.);
    reply(
        &mut e,
        &r,
        r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","visible":true,"quantityIncrement":"0.01","priceIncrement":"0.01","minQuantity":"0.01","imfFunction":{"base":"0.02"}},{"symbol":"SOL_USDC","marketType":"SPOT","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#,
    );
    let actions = actions(&mut e, 0.0);
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let requests: Vec<_> = actions
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    for r in requests {
        if r.url.contains("tickers") {
            reply(
                &mut e,
                &r,
                r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.05","volume":"10","quoteVolume":"1410"}]"#,
            );
        } else {
            reply(
                &mut e,
                &r,
                r#"[{"symbol":"SOL_USDC_PERP","markPrice":"140","indexPrice":"139","fundingRate":"0.001","nextFundingTimestamp":1000}]"#,
            );
        }
    }
    e.handle(0.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    (e, socket)
}

fn subscribe(e: &mut Driver, now: f64) {
    e.handle(
        now,
        Event::Command(Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
}
fn request(e: &mut Driver, now: f64) -> HttpRequest {
    http(e, "/depth?", now)
}
#[test]
fn native_wire_seed_book_and_subscription_order_working_control() {
    let (mut e, _) = seed();
    subscribe(&mut e, 0.);
    let all = actions(&mut e, 0.);
    let frame = all
        .iter()
        .position(|a| matches!(a, Action::WsSend { .. }))
        .expect("genuine accepted-stream subscribe frame");
    let index = all
        .iter()
        .position(|a| matches!(a, Action::Http(r) if r.url.contains("/depth?")))
        .unwrap();
    assert!(
        frame < index,
        "native subscription precedes physical depth seed"
    );
    let r = all
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/depth?") => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        r.url,
        "https://api.backpack.exchange/api/v1/depth?symbol=SOL_USDC_PERP"
    );
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.body, None);
    assert_eq!(r.route, None);
    assert_eq!(r.headers, vec![("X-Broker-Id", "1200".into())]);
    reply_at(&mut e, &r, 1., SNAP);
    let book = e.venue().book(SYMBOL).unwrap();
    assert_eq!(book.bids(), [(140., 1.), (139., 2.)]);
    assert_eq!(book.asks(), [(142., 1.), (143., 2.)]);
}
#[test]
fn native_buffered_bridge_updates_seed_working_control() {
    let (mut e, socket) = seed();
    subscribe(&mut e, 0.);
    let r = request(&mut e, 0.);
    e.handle(1., Event::WsMessage { socket, text: r#"{"stream":"depth.SOL_USDC_PERP","data":{"e":"depth","s":"SOL_USDC_PERP","U":9,"u":11,"b":[["140","5"]],"a":[["142","0"]]}}"# });
    reply_at(&mut e, &r, 2., SNAP);
    let book = e.venue().book(SYMBOL).unwrap();
    assert_eq!(book.bids(), [(140., 5.), (139., 2.)]);
    assert_eq!(book.asks(), [(143., 2.)]);
}
#[test]
fn native_generation_rejects_unsubscribed_seed_working_control() {
    let (mut e, _) = seed();
    subscribe(&mut e, 0.);
    let old = request(&mut e, 0.);
    e.handle(
        1.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
    e.drain_actions().for_each(drop);
    subscribe(&mut e, 2.);
    let current = request(&mut e, 2.);
    reply_at(&mut e, &old, 3., SNAP);
    assert!(e.venue().book(SYMBOL).is_none());
    reply_at(&mut e, &current, 4., SNAP);
    assert!(e.venue().book(SYMBOL).is_some());
}
#[test]
fn subscribed_seed_cannot_release_before_provider_prepare_bind_claim() {
    let (mut e, _) = seed();
    subscribe(&mut e, 0.);
    let queued: Vec<_> = e.drain_actions().collect();
    let queued = snapshot_handoff(&mut e, 0., queued, None);
    assert!(
        !queued
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/depth?"))),
        "no physical seed before provider handoff"
    );
    assert_eq!(queued.iter().filter(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { request, .. } if request.url.contains("/depth?")))).count(), 1);
}
#[test]
fn invalid_background_clock_cannot_start_seed_transport() {
    let (mut e, _) = seed();
    subscribe(&mut e, f64::NAN);
    let queued: Vec<_> = e.drain_actions().collect();
    assert!(
        !queued
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/depth?"))),
        "invalid Read clock creates no physical seed"
    );
    assert!(
        queued
            .iter()
            .all(|a| !matches!(a, Action::SetTimer { at_ms, .. } if !at_ms.is_finite())),
        "invalid admission cannot schedule an invalid recovery timer"
    );
    assert!(!queued.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. }))));
    assert!(e.venue().book(SYMBOL).is_none());
}
#[test]
fn seed_data_after_new_admission_deadline_cannot_publish_a_book() {
    let (mut e, _) = seed();
    subscribe(&mut e, 0.);
    let r = request(&mut e, 0.);
    reply_at(&mut e, &r, 20_001., SNAP);
    assert!(
        e.venue().book(SYMBOL).is_none(),
        "new20s admission bound rejects late native seed DATA"
    );
}

mod lifecycle {
    use super::*;
    use safe_cex_engine::book::Book;
    use safe_cex_engine::request_context::{
        Claim, Context, Error, Operation, Policy, Preparation, Prepared, RetryPolicy, Scope,
    };
    use safe_cex_engine::{Cx, ErrorKind, Outcome, TimerId, VenueError};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    #[derive(Default)]
    struct Evidence {
        errors: RefCell<Vec<(Option<Error>, VenueError)>>,
        reserve: Cell<usize>,
        reserved: RefCell<Vec<Result<(), Error>>>,
        timer: Cell<Option<TimerId>>,
    }
    struct Audit {
        native: Backpack,
        evidence: Rc<Evidence>,
        held: Vec<Context>,
    }
    impl Audit {
        fn reserve(&mut self, cx: &mut Cx<'_, safe_cex_venue_backpack::Pending>) {
            self.evidence.reserved.borrow_mut().clear();
            for _ in 0..self.evidence.reserve.get() {
                let result = cx.http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 60_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                );
                match result {
                    Ok(context) => {
                        self.held.push(context);
                        self.evidence.reserved.borrow_mut().push(Ok(()));
                    }
                    Err(error) => self.evidence.reserved.borrow_mut().push(Err(error)),
                }
            }
        }
    }
    impl Venue for Audit {
        type Pending = safe_cex_venue_backpack::Pending;
        fn start(&mut self, cx: &mut Cx<'_, Self::Pending>) {
            self.reserve(cx);
            self.evidence.timer.set(Some(cx.set_timer(0.)));
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
            )
                -> Result<(), safe_cex_engine::socket_lifecycle::SocketError>,
        ) -> Result<(), safe_cex_engine::socket_lifecycle::SocketError> {
            self.native
                .encode_socket_interests(store, feed, subscribe, items, emit)
        }
        fn on_open(&mut self, cx: &mut Cx<'_, Self::Pending>, id: SocketId) {
            self.native.on_open(cx, id)
        }
        fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, id: SocketId, text: &str) {
            self.native.on_message(cx, id, text)
        }
        fn on_close(&mut self, cx: &mut Cx<'_, Self::Pending>, id: SocketId, code: u16) {
            self.native.on_close(cx, id, code)
        }
        fn on_item(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: &mut Self::Pending,
            item: &str,
        ) {
            self.native.on_item(cx, pending, item)
        }
        fn on_response(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            response: Response<'_>,
        ) {
            self.native.on_response(cx, pending, response)
        }
        fn on_request_failed(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            error: &str,
        ) {
            self.native.on_request_failed(cx, pending, error)
        }
        fn on_http_failed(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            error: VenueError,
        ) {
            self.evidence
                .errors
                .borrow_mut()
                .push((None, error.clone()));
            self.native.on_http_failed(cx, pending, error)
        }
        fn on_http_preparation_failed(
            &mut self,
            cx: &mut Cx<'_, Self::Pending>,
            pending: Self::Pending,
            reason: Error,
            error: VenueError,
        ) {
            self.evidence
                .errors
                .borrow_mut()
                .push((Some(reason), error.clone()));
            self.native
                .on_http_preparation_failed(cx, pending, reason, error)
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Self::Pending>, timer: TimerId) {
            if self.evidence.timer.get() == Some(timer) {
                self.reserve(cx);
                self.evidence.timer.set(Some(cx.set_timer(0.)));
            } else {
                self.native.on_timer(cx, timer)
            }
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command) {
            self.native.on_command(cx, command)
        }
        fn book(&self, symbol: &str) -> Option<&Book> {
            self.native.book(symbol)
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            self.native.books()
        }
    }
    fn setup(reserve: usize) -> (Engine<Audit>, Rc<Evidence>) {
        let evidence = Rc::new(Evidence::default());
        evidence.reserve.set(0);
        let mut e = safe_cex_engine::test_support::seeded_engine(Audit {
            native: Backpack::new(Options {
                public_data_only: true,
                ..Default::default()
            }),
            evidence: evidence.clone(),
            held: vec![],
        });
        e.start(0.);
        let market = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.ends_with("/markets") => Some(r),
                _ => None,
            })
            .unwrap();
        e.handle(0.,Event::HttpResponse{request:market.id,response:Response{status:200,headers:"",body:r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#}});
        let initial = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
        let socket = initial
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(*socket),
                _ => None,
            })
            .unwrap();
        for a in initial {
            if let Action::Http(r) = a {
                let body = if r.url.contains("tickers") {
                    r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.01","volume":"10","quoteVolume":"1410"}]"#
                } else {
                    r#"[{"symbol":"SOL_USDC_PERP","markPrice":"140","indexPrice":"139","fundingRate":"0.001","nextFundingTimestamp":1000}]"#
                };
                e.handle(
                    0.,
                    Event::HttpResponse {
                        request: r.id,
                        response: Response {
                            status: 200,
                            headers: "",
                            body,
                        },
                    },
                );
            }
        }
        e.handle(0., Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        evidence.reserve.set(reserve);
        e.handle(
            0.0,
            Event::TimerFired {
                timer: evidence.timer.get().unwrap(),
            },
        );
        e.drain_actions().for_each(drop);
        (e, evidence)
    }
    fn queued(e: &mut Engine<Audit>, now: f64) -> Vec<Action> {
        e.handle(
            now,
            Event::Command(Command::SubscribeOrderBook {
                symbol: SYMBOL.into(),
            }),
        );
        let excluded = e.venue().evidence.timer.get();
        let queued = e.drain_actions().collect();
        snapshot_handoff(e, now, queued, excluded)
    }
    fn preparation(actions: &[Action]) -> Preparation {
        actions
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .expect("real managed depth preparation")
    }
    fn release(e: &mut Engine<Audit>, token: Preparation, now: f64) -> HttpRequest {
        e.http_prepared(
            now,
            token,
            Prepared {
                id: token.attempt().request.0,
                expires_at: None,
            },
        );
        let (permit, request) = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind {
                        permit, request, ..
                    } => Some((permit, request)),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
        request
    }
    fn response(e: &mut Engine<Audit>, r: &HttpRequest, now: f64) {
        e.handle(
            now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: SNAP,
                },
            },
        )
    }
    fn no_physical_or_new_prepare(actions: &[Action]) {
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(!actions.iter().any(
            |a| matches!(a,Action::ManagedHttp(a)if matches!(a.as_ref(),HttpAction::Prepare{..}))
        ));
    }
    #[test]
    fn typed_precommit_failure_recovers_only_after_native_retry_timer() {
        let (mut e, proof) = setup(0);
        let token = preparation(&queued(&mut e, 0.));
        e.http_preparation_failed(
            1.,
            token,
            Error::Invalid,
            VenueError::new("route refused")
                .kind(ErrorKind::Unsupported)
                .venue_code("route.test"),
        );
        {
            let errors = proof.errors.borrow();
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].0, Some(Error::Invalid));
            assert_eq!(errors[0].1.kind, Some(ErrorKind::Unsupported));
            assert_eq!(errors[0].1.outcome, Some(Outcome::NotSent));
            assert_eq!(errors[0].1.venue_code.as_deref(), Some("route.test"));
        }
        let actions: Vec<_> = e.drain_actions().collect();
        no_physical_or_new_prepare(&actions);
        assert!(e.venue().book(SYMBOL).is_none());
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == 1001. => Some(*timer),
                _ => None,
            })
            .expect("original1000ms recovery delay");
        e.handle(1001., Event::TimerFired { timer });
        let excluded = e.venue().evidence.timer.get();
        let retry = e.drain_actions().collect();
        let retry = snapshot_handoff(&mut e, 1001., retry, excluded);
        let new = preparation(&retry);
        assert_ne!(new, token);
    }
    #[test]
    fn released_network_and_deadline_failures_keep_typed_read_outcomes() {
        for timeout in [false, true] {
            let (mut e, proof) = setup(0);
            let token = preparation(&queued(&mut e, 0.));
            let r = release(&mut e, token, 1.);
            if timeout {
                response(&mut e, &r, 20_001.);
            } else {
                e.handle(
                    2.,
                    Event::HttpFailed {
                        request: r.id,
                        error: "network lost",
                    },
                );
            }
            let errors = proof.errors.borrow();
            assert_eq!(errors.len(), 1);
            assert_eq!(
                errors[0].1.kind,
                Some(if timeout {
                    ErrorKind::Timeout
                } else {
                    ErrorKind::Network
                })
            );
            assert_eq!(errors[0].1.outcome, None);
            assert!(e.venue().book(SYMBOL).is_none());
        }
    }
    #[test]
    fn context_capacity_refuses_transport_and_uses_native_recovery() {
        let (mut e, proof) = setup(256);
        assert_eq!(
            proof.reserved.borrow().iter().filter(|r| r.is_ok()).count(),
            256
        );
        let actions = queued(&mut e, 0.);
        no_physical_or_new_prepare(&actions);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a,Action::SetTimer{at_ms,..}if *at_ms==1000.))
        );
        proof.reserve.set(1);
        e.handle(
            0.,
            Event::TimerFired {
                timer: proof.timer.get().unwrap(),
            },
        );
        assert_eq!(&*proof.reserved.borrow(), &[Err(Error::Capacity)]);
        assert!(e.venue().book(SYMBOL).is_none());
    }
    #[test]
    fn stale_generation_retires_exact_context_without_retiring_current_seed() {
        let (mut e, proof) = setup(0);
        let old_preparation = preparation(&queued(&mut e, 0.));
        let old = release(&mut e, old_preparation, 1.);
        e.handle(
            2.,
            Event::Command(Command::UnsubscribeOrderBook {
                symbol: SYMBOL.into(),
            }),
        );
        e.drain_actions().for_each(drop);
        let current_preparation = preparation(&queued(&mut e, 2.));
        let current = release(&mut e, current_preparation, 2.);
        response(&mut e, &old, 3.);
        assert!(e.venue().book(SYMBOL).is_none());
        proof.reserve.set(255);
        e.handle(
            3.,
            Event::TimerFired {
                timer: proof.timer.get().unwrap(),
            },
        );
        assert_eq!(proof.reserved.borrow().len(), 255);
        assert!(
            proof.reserved.borrow().iter().all(|r| r.is_ok()),
            "stale seed Context was retired"
        );
        proof.reserve.set(1);
        e.handle(
            3.,
            Event::TimerFired {
                timer: proof.timer.get().unwrap(),
            },
        );
        assert_eq!(
            &*proof.reserved.borrow(),
            &[Err(Error::Capacity)],
            "current seed still owns its slot"
        );
        response(&mut e, &current, 4.);
        assert!(e.venue().book(SYMBOL).is_some());
        e.handle(
            4.,
            Event::TimerFired {
                timer: proof.timer.get().unwrap(),
            },
        );
        assert_eq!(
            &*proof.reserved.borrow(),
            &[Ok(())],
            "current terminal releases only its own slot"
        );
    }
}
