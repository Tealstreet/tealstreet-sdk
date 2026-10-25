use super::*;
use safe_cex_engine::request_context::{Error, HttpAction, Preparation, Prepared};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice};
use std::sync::{Arc, Mutex};

const MARKETS: &str = r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":20,"base_imr":0.01}]}}"#;

struct Probe {
    inner: Orderly,
    peers: Vec<Context>,
    errors: Arc<Mutex<Vec<VenueError>>>,
    pressure: bool,
}
impl Probe {
    fn fill(&mut self, cx: &mut Cx<'_, Pending>) {
        loop {
            match cx.http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 100_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            ) {
                Ok(context) => self.peers.push(context),
                Err(Error::Capacity) => break,
                Err(error) => panic!("peer admission: {error:?}"),
            }
        }
    }
}
impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.pressure {
            self.fill(cx);
            assert_eq!(self.peers.len(), 256);
        }
        self.inner.start(cx);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Command::SetTickerSymbols { symbols } = command else {
            panic!("fixture control")
        };
        match symbols[0].as_str() {
            "markets" => self.inner.seed(cx, Seed::Markets),
            "tickers" => self.inner.seed(cx, Seed::Tickers),
            "queue" => {
                let context = cx
                    .http_context(
                        Scope::Background,
                        Policy {
                            operation: Operation::Read,
                            credential: None,
                            timeout_ms: 100_000,
                            pacing: None,
                            retry: RetryPolicy::Never,
                        },
                    )
                    .unwrap();
                for n in 0..256 {
                    assert!(
                        cx.request_managed(
                            context.clone(),
                            Method::Get,
                            format!("https://fixture.invalid/{n}"),
                            None,
                            Pending::Markets {
                                context: context.clone(),
                                rows: Vec::new(),
                                invalid: false
                            }
                        )
                        .is_ok()
                    );
                }
                self.peers.push(context);
            }
            "release" => {
                for context in self.peers.drain(..) {
                    cx.finish_http_context(&context).unwrap();
                }
            }
            "credits" => {
                self.fill(cx);
                assert_eq!(self.peers.len(), 256, "all physical credits returned");
                for context in self.peers.drain(..) {
                    cx.finish_http_context(&context).unwrap();
                }
            }
            _ => panic!("unknown fixture control"),
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
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
        self.errors.lock().unwrap().push(error.clone());
        self.inner
            .on_http_preparation_failed(cx, pending, reason, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.errors.lock().unwrap().push(error.clone());
        self.inner.on_http_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
}
type Driver = Engine<Probe>;
fn engine(pressure: bool) -> (Driver, Arc<Mutex<Vec<VenueError>>>) {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Orderly::new(Options {
            public_data_only: true,
            ..Default::default()
        }),
        peers: Vec::new(),
        errors: errors.clone(),
        pressure,
    });
    (e, errors)
}
fn command(e: &mut Driver, now: f64, tag: &str) {
    e.handle(
        now,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![tag.into()],
        }),
    );
}
fn drain(e: &mut Driver) -> Vec<Action> {
    e.drain_actions().collect()
}
fn released(e: &mut Driver, now: f64) -> Vec<HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect()
}
fn no_work(out: &[Action]) {
    assert!(!out.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
    assert!(!out.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Sign { .. } | Action::SignPayload { .. }
    )));
    assert!(!out.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}
fn retry(out: &[Action], now: f64) -> TimerId {
    retry_after(out, now, 1000.)
}
fn retry_after(out: &[Action], now: f64, nominal: f64) -> TimerId {
    let timers: Vec<_> = out
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if (nominal * 0.75..=nominal).contains(&(at_ms - now)) =>
            {
                Some(*timer)
            }
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1, "exactly one native seed backoff: {out:?}");
    timers[0]
}
fn prepare(out: Vec<Action>) -> Preparation {
    out.into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("actual managed preparation")
}
fn respond(e: &mut Driver, now: f64, request: &HttpRequest, body: &str) {
    e.handle(
        now,
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

#[test]
fn context_pressure_retries_without_io_then_returns_all_credits() {
    let (mut e, _) = engine(true);
    e.start(0.);
    let out = drain(&mut e);
    no_work(&out);
    let timer = retry(&out, 0.);
    command(&mut e, 1., "release");
    drain(&mut e);
    e.handle(1000., Event::TimerFired { timer });
    let requests = released(&mut e, 1000.);
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.ends_with("/v1/public/info"));
    e.handle(
        1001.,
        Event::HttpFailed {
            request: requests[0].id,
            error: "network",
        },
    );
    let _ = retry_after(&drain(&mut e), 1001., 2000.);
    command(&mut e, 1002., "credits");
    no_work(&drain(&mut e));
}

#[test]
fn preparation_refusal_is_typed_once_and_late_prepare_cannot_release() {
    let (mut e, errors) = engine(false);
    e.start(0.);
    let p = prepare(drain(&mut e));
    e.http_preparation_failed(
        1.,
        p,
        Error::Invalid,
        VenueError::new("fixture prepare refusal").kind(safe_cex_engine::ErrorKind::InvalidInput),
    );
    let out = drain(&mut e);
    let _ = retry(&out, 1.);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    let seen = errors.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    assert_eq!(seen[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
    drop(seen);
    e.http_prepared(
        2.,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    no_work(&drain(&mut e));
    command(&mut e, 3., "credits");
    no_work(&drain(&mut e));
}

#[test]
fn streamed_item_then_exact_deadline_final_cannot_publish_and_recovers_once() {
    let (mut e, errors) = engine(false);
    e.start(0.);
    let r = released(&mut e, 0.).remove(0);
    let split = MARKETS.find("]}").unwrap();
    e.handle(
        19_999.,
        Event::HttpChunk {
            request: r.id,
            data: &MARKETS.as_bytes()[..split],
        },
    );
    drain(&mut e);
    assert!(!e.store().loaded().markets);
    respond(&mut e, 20_000., &r, "]}}");
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
    let out = drain(&mut e);
    let timer = retry(&out, 20_000.);
    let seen = errors.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].kind, Some(safe_cex_engine::ErrorKind::Timeout));
    assert_eq!(seen[0].outcome, None);
    drop(seen);
    respond(&mut e, 20_001., &r, MARKETS);
    no_work(&drain(&mut e));
    e.handle(21_000., Event::TimerFired { timer });
    let r2 = released(&mut e, 21_000.).remove(0);
    assert_ne!(r.id, r2.id);
    respond(&mut e, 21_001., &r2, MARKETS);
    assert!(e.store().loaded().markets);
    let tickers = released(&mut e, 21_001.);
    assert_eq!(tickers.len(), 1);
    assert!(tickers[0].url.ends_with("/v1/public/futures"));
}

#[test]
fn independently_admitted_market_reads_both_keep_their_delivery() {
    let (mut e, _) = engine(false);
    e.start(0.);
    let first = released(&mut e, 0.).remove(0);
    command(&mut e, 1., "markets");
    let second = released(&mut e, 1.).remove(0);
    assert_ne!(first.id, second.id);
    for (at, r) in [(2., &second), (3., &first)] {
        respond(&mut e, at, r, MARKETS);
        assert!(e.store().loaded().markets);
        let next = released(&mut e, at);
        assert_eq!(next.len(), 1);
        assert!(next[0].url.ends_with("/v1/public/futures"));
        e.handle(
            at,
            Event::HttpFailed {
                request: next[0].id,
                error: "fixture cleanup",
            },
        );
        let out = drain(&mut e);
        if at == 2. {
            let _ = retry(&out, at);
        } else {
            no_work(&out);
            assert!(!out.iter().any(|a| matches!(a, Action::SetTimer { .. })));
        }
    }
    command(&mut e, 4., "credits");
    no_work(&drain(&mut e));
}

fn ticker_failure(mode: u8) {
    let (mut e, errors) = engine(false);
    e.start(0.);
    let market = released(&mut e, 0.).remove(0);
    respond(&mut e, 1., &market, MARKETS);
    let (at, old) = if mode == 0 {
        let p = prepare(drain(&mut e));
        e.http_preparation_failed(
            2.,
            p,
            Error::Invalid,
            VenueError::new("fixture ticker preparation")
                .kind(safe_cex_engine::ErrorKind::Unsupported),
        );
        (2., p.attempt().request)
    } else {
        let ticker = released(&mut e, 1.).remove(0);
        if mode == 1 {
            e.handle(
                2.,
                Event::HttpFailed {
                    request: ticker.id,
                    error: "fixture ticker network",
                },
            );
            (2., ticker.id)
        } else {
            respond(&mut e, 20_001., &ticker, "{}");
            (20_001., ticker.id)
        }
    };
    let out = drain(&mut e);
    no_work(&out);
    let timer = retry(&out, at);
    assert!(e.store().loaded().markets);
    assert!(!e.store().loaded().tickers);
    let seen = errors.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].kind,
        Some(match mode {
            0 => safe_cex_engine::ErrorKind::Unsupported,
            1 => safe_cex_engine::ErrorKind::Network,
            _ => safe_cex_engine::ErrorKind::Timeout,
        })
    );
    assert_eq!(
        seen[0].outcome,
        (mode == 0).then_some(safe_cex_engine::Outcome::NotSent)
    );
    drop(seen);
    e.handle(
        at + 1.,
        Event::HttpResponse {
            request: old,
            response: Response {
                status: 429,
                headers: "Retry-After: 100",
                body: "{}",
            },
        },
    );
    no_work(&drain(&mut e));
    e.handle(at + 1000., Event::TimerFired { timer });
    let next = released(&mut e, at + 1000.);
    assert_eq!(next.len(), 1);
    assert!(next[0].url.ends_with("/v1/public/futures"));
    assert_ne!(next[0].id, old);
}

#[test]
fn ticker_preparation_failure_recovers_once_without_reloading_markets() {
    ticker_failure(0);
}
#[test]
fn ticker_network_failure_recovers_once_without_reloading_markets() {
    ticker_failure(1);
}
#[test]
fn ticker_timeout_recovers_once_without_reloading_markets() {
    ticker_failure(2);
}

#[test]
fn nonfinite_admission_has_no_io_or_nan_retry_and_preserves_credits() {
    let (mut e, _) = engine(false);
    for now in [f64::NAN, f64::INFINITY] {
        command(&mut e, now, "markets");
        let out = drain(&mut e);
        no_work(&out);
        assert!(!out.iter().any(|a| matches!(a, Action::SetTimer { .. })));
        assert!(!e.store().loaded().markets);
    }
    command(&mut e, 0., "credits");
    no_work(&drain(&mut e));
    command(&mut e, 1., "markets");
    assert_eq!(released(&mut e, 1.).len(), 1);
}

#[test]
fn physical_queue_refusal_retires_the_new_context_before_retry() {
    let (mut e, _) = engine(false);
    command(&mut e, 0., "queue");
    let peers = drain(&mut e);
    assert_eq!(
        peers
            .iter()
            .filter(|a| matches!(a, Action::ManagedHttp(a)
        if matches!(a.as_ref(), HttpAction::Prepare { .. })))
            .count(),
        256
    );
    e.start(1.);
    let out = drain(&mut e);
    no_work(&out);
    let timer = retry(&out, 1.);
    command(&mut e, 2., "release");
    let cancelled = drain(&mut e);
    no_work(&cancelled);
    assert_eq!(
        cancelled
            .iter()
            .filter(|a| matches!(a, Action::ManagedHttp(a)
        if matches!(a.as_ref(), HttpAction::Cancel { .. })))
            .count(),
        256
    );
    command(&mut e, 3., "credits");
    no_work(&drain(&mut e));
    e.handle(1001., Event::TimerFired { timer });
    let next = released(&mut e, 1001.);
    assert_eq!(next.len(), 1);
    assert!(next[0].url.ends_with("/v1/public/info"));
}
