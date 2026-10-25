use super::*;
use safe_cex_engine::request_context::{Claim, Error, HttpAction, Preparation, Prepared};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice};

struct Fixture {
    inner: TxFlow,
    peers: Vec<HttpContext>,
    pressure: bool,
    recovered_credits: usize,
    response_callbacks: usize,
    failures: Vec<safe_cex_engine::VenueError>,
}
fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 120_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
impl Venue for Fixture {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.pressure {
            for _ in 0..256 {
                self.peers
                    .push(cx.http_context(Scope::Background, policy()).unwrap());
            }
        }
        self.inner.ticker_request(cx, 1);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Command::FetchLeverage { symbol: action, .. } = command else {
            panic!("test command")
        };
        match action.as_str() {
            "free-one" => {
                cx.finish_http_context(&self.peers.pop().unwrap()).unwrap();
            }
            "replace" => self.inner.ticker_request(cx, 1),
            "credits" => {
                for peer in self.peers.drain(..) {
                    cx.finish_http_context(&peer).unwrap();
                }
                for _ in 0..256 {
                    self.peers
                        .push(cx.http_context(Scope::Background, policy()).unwrap());
                }
                self.recovered_credits = self.peers.len();
                assert!(matches!(
                    cx.http_context(Scope::Background, policy()),
                    Err(Error::Capacity)
                ));
                for peer in self.peers.drain(..) {
                    cx.finish_http_context(&peer).unwrap();
                }
            }
            _ => panic!("unknown fixture action"),
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.response_callbacks += 1;
        self.inner.on_response(cx, pending, response);
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: safe_cex_engine::VenueError,
    ) {
        self.inner
            .on_http_preparation_failed(cx, pending, reason, error);
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        self.failures.push(error.clone());
        self.inner.on_http_failed(cx, pending, error);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn engine(pressure: bool, now: f64) -> Engine<Fixture> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Fixture {
        inner: TxFlow::new(Options {
            testnet: true,
            public_data_only: true,
            ..Default::default()
        }),
        peers: Vec::new(),
        pressure,
        recovered_credits: 0,
        response_callbacks: 0,
        failures: Vec::new(),
    });
    e.start(now);
    e
}
fn command(e: &mut Engine<Fixture>, now: f64, action: &str) {
    e.handle(
        now,
        Event::Command(Command::FetchLeverage {
            command: 91,
            symbol: action.into(),
        }),
    );
}
fn preparation(e: &mut Engine<Fixture>) -> (Preparation, HttpRequest) {
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(
                        intent,
                        safe_cex_engine::request_context::RoutingIntent::Public
                    );
                    assert_eq!(
                        request.route,
                        Some(safe_cex_engine::RequestRoute::PublicCors)
                    );
                    Some((preparation, request))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("real managed Prepare")
}
fn no_send(actions: &[Action]) {
    assert!(!actions.iter().any(|a| match a {
        Action::Http(_) => true,
        Action::ManagedHttp(a) => matches!(
            a.as_ref(),
            HttpAction::Prepare { .. } | HttpAction::Bind { .. }
        ),
        _ => false,
    }));
}
#[test]
fn pressure_uses_one_native_backoff_then_recovers_all_credits() {
    let mut e = engine(true, 1000.);
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_send(&actions);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code::REQUEST_FAILED && event.message == "Startup read failed; retry scheduled"))
            .count(),
        1
    );
    let (timer, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (1750. ..=2000.).contains(at_ms) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("single native keyed retry");
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::SetTimer{at_ms,..} if *at_ms==1025.))
            .count(),
        1
    );
    command(&mut e, 1001., "free-one");
    e.handle(at, Event::TimerFired { timer });
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, at)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(
        at,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"success":true,"code":0,"data":{"instrumentId":1}}"#,
            },
        },
    );
    assert!(e.venue().inner.ticker_contexts.is_empty());
    command(&mut e, at, "credits");
    assert_eq!(e.venue().recovered_credits, 256);
}
#[test]
fn nonfinite_issue_never_creates_recovery_timers_or_context() {
    for now in [f64::NAN, f64::INFINITY] {
        let mut e = engine(false, now);
        let actions = e.drain_actions().collect::<Vec<_>>();
        no_send(&actions);
        assert!(!actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
        assert!(e.venue().inner.ticker_contexts.is_empty());
    }
}
#[test]
fn late_prepare_expires_once_and_leaves_no_seed_credit() {
    let mut e = engine(false, 1000.);
    let (old, _) = preparation(&mut e);
    e.http_prepared(
        21000.,
        old,
        Prepared {
            id: 7,
            expires_at: None,
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_send(&actions);
    assert!(e.venue().inner.ticker_contexts.is_empty());
    assert!(!e.http_preparation_pending(old));
    command(&mut e, 21000., "credits");
    assert_eq!(e.venue().recovered_credits, 256);
}
#[test]
fn stale_prepare_and_failure_cannot_clear_a_replacement() {
    let mut e = engine(false, 1000.);
    let (old, old_request) = preparation(&mut e);
    command(&mut e, 1001., "replace");
    let (new, new_request) = preparation(&mut e);
    assert_ne!(old, new);
    e.http_prepared(
        1002.,
        old,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    e.http_preparation_failed(
        1002.,
        old,
        Error::State,
        safe_cex_engine::VenueError::new("old route"),
    );
    e.handle(
        1002.,
        Event::HttpFailed {
            request: old_request.id,
            error: "old",
        },
    );
    assert!(e.http_preparation_pending(new));
    assert_eq!(e.venue().inner.ticker_contexts.len(), 1);
    let stale = e.drain_actions().collect::<Vec<_>>();
    no_send(&stale);
    assert!(
        !stale
            .iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms==1027.))
    );
    e.http_prepared(
        1003.,
        new,
        Prepared {
            id: 2,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        e.claim_http(1003., permit),
        Ok(Claim::Released(_))
    ));
    e.handle(
        1004.,
        Event::HttpResponse {
            request: new_request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"success":true,"code":0,"data":{"instrumentId":1}}"#,
            },
        },
    );
    assert!(e.venue().inner.ticker_contexts.is_empty());
    assert!(e.venue().inner.ticker_seeded.contains(&1));
}

#[test]
fn typed_precommit_failure_retires_before_one_native_retry() {
    let mut e = engine(false, 1000.);
    let (old, _) = preparation(&mut e);
    e.http_preparation_failed(
        1001.,
        old,
        Error::State,
        safe_cex_engine::VenueError::new("host resource unavailable")
            .kind(safe_cex_engine::ErrorKind::Network),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_send(&actions);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.message=="host resource unavailable")));
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::SetTimer{at_ms,..} if *at_ms==1026.))
            .count(),
        1
    );
    assert!(e.venue().inner.ticker_contexts.is_empty());
    assert!(!e.http_preparation_pending(old));
    command(&mut e, 1001., "credits");
    assert_eq!(e.venue().recovered_credits, 256);
}

#[test]
fn unclaimed_bind_cannot_release_at_the_physical_bound() {
    let mut e = engine(false, 1000.);
    let (preparation, _) = preparation(&mut e);
    e.http_prepared(
        1001.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(e.claim_http(21000., permit).is_err());
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_send(&actions);
    assert!(e.venue().inner.ticker_contexts.is_empty());
    command(&mut e, 21000., "credits");
    assert_eq!(e.venue().recovered_credits, 256);
}

#[test]
fn deadline_data_uses_typed_timeout_without_entering_the_data_callback() {
    let mut e = engine(false, 1000.);
    let (preparation, request) = preparation(&mut e);
    e.http_prepared(
        1001.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        e.claim_http(1001., permit),
        Ok(Claim::Released(_))
    ));
    e.drain_actions().for_each(drop);
    for _ in 0..2 {
        e.handle(
            21000.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"success":true,"code":0,"data":{"instrumentId":1}}"#,
                },
            },
        );
    }
    assert_eq!(e.venue().response_callbacks, 0);
    assert_eq!(e.venue().failures.len(), 1);
    assert_eq!(
        e.venue().failures[0].kind,
        Some(safe_cex_engine::ErrorKind::Timeout)
    );
    assert!(e.venue().inner.ticker_contexts.is_empty());
    let actions = e.drain_actions().collect::<Vec<_>>();
    no_send(&actions);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 21025.))
            .count(),
        1
    );
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::SetTimer { at_ms, .. } if (21750. ..=22000.).contains(at_ms))).count(), 1);
    command(&mut e, 21000., "credits");
    assert_eq!(e.venue().recovered_credits, 256);
}
