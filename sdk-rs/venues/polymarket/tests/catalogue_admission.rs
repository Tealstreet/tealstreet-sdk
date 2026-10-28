//! Real shared pressure, returned-Pending cleanup and retired host ingress.
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Notice, Response, SocketId, TimerId, Venue,
    VenueError,
};
use safe_cex_venue_polymarket::{Options, Pending, Polymarket};

const GOOD: &str = r#"{"markets":[{"conditionId":"c","slug":"election","active":true,"clobTokenIds":"[\"1\"]","outcomes":"[\"Yes\"]"}]}"#;
struct Pressure {
    inner: Polymarket,
    peers: Vec<Context>,
}
impl Venue for Pressure {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SetTickerSymbols { symbols } if symbols == ["fill"] => {
                loop {
                    match cx.http_context(
                        Scope::Background,
                        Policy {
                            operation: Operation::Read,
                            credential: None,
                            timeout_ms: 60_000,
                            pacing: None,
                            retry: RetryPolicy::Never,
                        },
                    ) {
                        Ok(context) => self.peers.push(context),
                        Err(Error::Capacity) => break,
                        Err(error) => panic!("unexpected peer admission {error:?}"),
                    }
                }
                assert_eq!(self.peers.len(), 256, "every catalogue credit released");
                for context in &self.peers {
                    cx.validate_http_context(context).unwrap();
                }
            }
            Command::SetTickerSymbols { symbols } if symbols == ["release"] => {
                for context in self.peers.drain(..) {
                    cx.finish_http_context(&context).unwrap();
                }
            }
            command => self.inner.on_command(cx, command),
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.inner
            .on_http_preparation_failed(cx, pending, reason, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_http_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
}
fn public() -> Polymarket {
    Polymarket::new(Options {
        public_data_only: true,
        ..Default::default()
    })
}
fn actions<V: Venue>(e: &mut Engine<V>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn gamma(a: Vec<Action>) -> HttpRequest {
    a.into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("gamma-api.polymarket.com") => Some(r),
            _ => None,
        })
        .expect("genuine released Gamma request")
}
fn prepare<V: Venue>(e: &mut Engine<V>) -> Preparation {
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("genuine managed Gamma preparation")
}
fn retry(a: &[Action], now: f64) -> TimerId {
    a.iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (750.0..=1000.).contains(&(*at_ms - now)) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("native first-page backoff")
}
fn no_work(a: &[Action]) {
    assert!(!a.iter().any(|a| match a {
        Action::Http(_) | Action::Sign { .. } => true,
        Action::ManagedHttp(a) => !matches!(a.as_ref(), HttpAction::Cancel { .. }),
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. }) => true,
        _ => false,
    }));
}
fn answer<V: Venue>(e: &mut Engine<V>, r: &HttpRequest, now: f64, body: &str) -> Vec<Action> {
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
    actions(e, now)
}
fn complete<V: Venue>(e: &mut Engine<V>, r: &HttpRequest, now: f64) {
    let a = answer(e, r, now, GOOD);
    assert_eq!(e.store().markets().len(), 1);
    let mid = a
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/midpoints") => Some(r),
            _ => None,
        })
        .expect("native midpoint successor");
    answer(e, &mid, now + 1., r#"{"1":"0.5"}"#);
    assert!(e.store().loaded().tickers);
}
fn command(e: &mut Engine<Pressure>, now: f64, name: &str) {
    e.handle(
        now,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![name.into()],
        }),
    );
}
#[test]
fn full_registry_refuses_unissued_page_without_recursion_and_recovers_all_credits() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Pressure {
        inner: public(),
        peers: vec![],
    });
    command(&mut e, 0., "fill");
    assert!(actions(&mut e, 0.).is_empty());
    e.start(10.);
    let a = actions(&mut e, 10.);
    no_work(&a);
    assert!(!e.store().loaded().markets);
    let timer = retry(&a, 10.);
    assert_eq!(e.venue().peers.len(), 256);
    command(&mut e, 11., "release");
    assert!(actions(&mut e, 11.).is_empty());
    e.handle(1010., Event::TimerFired { timer });
    let r = gamma(actions(&mut e, 1010.));
    complete(&mut e, &r, 1011.);
    command(&mut e, 1013., "fill");
    assert_eq!(e.venue().peers.len(), 256);
}
#[test]
fn stale_failed_preparation_and_data_cannot_clear_new_retry_owner() {
    let mut e = safe_cex_engine::test_support::seeded_engine(public());
    e.start(10.);
    let old = prepare(&mut e);
    e.http_preparation_failed(11., old, Error::State, VenueError::new("route refused"));
    let a: Vec<_> = e.drain_actions().collect();
    no_work(&a);
    let timer = retry(&a, 11.);
    e.handle(1011., Event::TimerFired { timer });
    let current = prepare(&mut e);
    e.http_prepared(
        1012.,
        old,
        Prepared {
            id: old.attempt().request.0,
            expires_at: None,
        },
    );
    e.http_preparation_failed(
        1012.,
        old,
        Error::State,
        VenueError::new("stale route refused"),
    );
    e.handle(
        1012.,
        Event::HttpResponse {
            request: old.attempt().request,
            response: Response {
                status: 200,
                headers: "",
                body: GOOD,
            },
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    no_work(&a);
    assert!(e.http_preparation_pending(current));
    assert!(
        !a.iter().any(|a| matches!(a, Action::SetTimer { .. })),
        "stale failure cannot back off the new owner"
    );
    e.http_prepared(
        1013.,
        current,
        Prepared {
            id: current.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, r) = e
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
        .expect("new current owner binds");
    assert!(matches!(
        e.claim_http(1013., permit),
        Ok(Claim::Released(_))
    ));
    complete(&mut e, &r, 1014.);
}
#[test]
fn expired_stream_cannot_poison_retry_dedup_or_new_preparation() {
    let mut e = safe_cex_engine::test_support::seeded_engine(public());
    e.start(10.);
    let old = gamma(actions(&mut e, 10.));
    e.handle(
        11.,
        Event::HttpStart {
            request: old.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(12.,Event::HttpChunk{request:old.id,data:br#"{"markets":[{"conditionId":"c","slug":"election","clobTokenIds":"[\"1\"]","outcomes":"[\"Yes\"]"},"#});
    assert!(e.drain_actions().next().is_none());
    e.handle(
        20_010.,
        Event::HttpChunk {
            request: old.id,
            data: br#"{"slug":"late","clobTokenIds":"broken","outcomes":"[]"}]}"#,
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    no_work(&a);
    let timer = retry(&a, 20_010.);
    assert!(!a.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.message.contains("Skipped malformed public discovery row"))));
    e.handle(21_010., Event::TimerFired { timer });
    let current = prepare(&mut e);
    e.handle(21_011., Event::HttpEnd { request: old.id });
    assert!(e.http_preparation_pending(current));
    let a: Vec<_> = e.drain_actions().collect();
    no_work(&a);
    e.http_prepared(
        21_012.,
        current,
        Prepared {
            id: current.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, r) = e
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
    assert!(matches!(
        e.claim_http(21_012., permit),
        Ok(Claim::Released(_))
    ));
    complete(&mut e, &r, 21_013.);
    assert_eq!(e.store().markets()[0].id, "1");
}
#[test]
fn malformed_and_empty_page_cleanup_release_every_context_credit() {
    for body in [
        "bad",
        r#"{"error":"native refusal"}"#,
        r#"{"markets":[],"next_cursor":false}"#,
        r#"{"markets":[]}"#,
    ] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Pressure {
            inner: public(),
            peers: vec![],
        });
        e.start(10.);
        let r = gamma(actions(&mut e, 10.));
        let a = answer(&mut e, &r, 11., body);
        no_work(&a);
        let timer = retry(&a, 11.);
        command(&mut e, 12., "fill");
        assert_eq!(e.venue().peers.len(), 256);
        e.drain_actions().for_each(drop);
        command(&mut e, 13., "release");
        e.drain_actions().for_each(drop);
        e.handle(1011., Event::TimerFired { timer });
        let r = gamma(actions(&mut e, 1011.));
        complete(&mut e, &r, 1012.);
        command(&mut e, 1014., "fill");
        assert_eq!(e.venue().peers.len(), 256);
    }
}
