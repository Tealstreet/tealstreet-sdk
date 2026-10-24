//! Real shared admission pressure and stale host callbacks around midpoint batches.
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Notice, Response, TimerId, Venue, VenueError,
};
use safe_cex_venue_polymarket::{Options, Pending, Polymarket};

fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
struct Capacity {
    inner: Polymarket,
    peers: Vec<Context>,
}
impl Venue for Capacity {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = command {
            if symbols == ["saturate"] {
                loop {
                    match cx.http_context(Scope::Background, policy()) {
                        Ok(c) => self.peers.push(c),
                        Err(Error::Capacity) => break,
                        Err(e) => panic!("unexpected admission {e:?}"),
                    }
                }
                assert_eq!(self.peers.len(), 256);
            } else {
                assert_eq!(symbols, ["release"]);
                for context in self.peers.drain(..) {
                    cx.finish_http_context(&context).unwrap();
                }
            }
        } else {
            self.inner.on_command(cx, command);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item);
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
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId, _: &str) {}
    fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.inner.books()
    }
}
fn venue() -> Polymarket {
    Polymarket::new(Options {
        public_data_only: true,
        ..Options::default()
    })
}
fn actions<V: Venue>(e: &mut Engine<V>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn start<V: Venue>(e: &mut Engine<V>) -> HttpRequest {
    e.start(0.);
    actions(e, 0.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap()
}
fn catalogue<V: Venue>(e: &mut Engine<V>, r: HttpRequest, count: usize) {
    let ids: Vec<_> = (1..=count).map(|n| n.to_string()).collect();
    let outcomes: Vec<_> = (1..=count).map(|n| format!("Outcome{n}")).collect();
    let body=serde_json::json!({"markets":[{"conditionId":"0x123","slug":"election","active":true,"closed":false,"clobTokenIds":serde_json::to_string(&ids).unwrap(),"outcomes":serde_json::to_string(&outcomes).unwrap()}]}).to_string();
    e.handle(
        10.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    assert_eq!(e.store().markets().len(), count);
}
fn retry(a: &[Action], now: f64) -> TimerId {
    a.iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (750.0..=1000.).contains(&(*at_ms - now)) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("native jittered retry")
}
fn request(a: Vec<Action>) -> HttpRequest {
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
            | Action::Sign { .. }
    )));
    a.into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/midpoints") => Some(r),
            _ => None,
        })
        .expect("released midpoint")
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
fn token<V: Venue>(e: &mut Engine<V>) -> Preparation {
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("real managed preparation")
}
#[test]
fn full_admission_backs_off_unissued_batch_then_recovers_and_releases_all_credits() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Capacity {
        inner: venue(),
        peers: vec![],
    });
    let gamma = start(&mut e);
    catalogue(&mut e, gamma, 102);
    let first = token(&mut e);
    e.http_preparation_failed(
        10.,
        first,
        Error::Invalid,
        Error::Invalid.read_venue_error(),
    );
    let refused: Vec<_> = e.drain_actions().collect();
    let first_retry = retry(&refused, 10.);
    let second = refused
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("native failure advances to the last midpoint offset");
    e.http_preparation_failed(
        10.,
        second,
        Error::Invalid,
        Error::Invalid.read_venue_error(),
    );
    let second_retry = retry(&actions(&mut e, 10.), 10.);
    e.handle(
        11.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["saturate".into()],
        }),
    );
    assert!(actions(&mut e, 11.).is_empty());
    e.handle(1010., Event::TimerFired { timer: first_retry });
    let a = actions(&mut e, 1010.);
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (1500.0..=2000.).contains(&(*at_ms - 1010.)) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("capacity refusal retains the failed key's second native backoff");
    assert_eq!(e.venue().peers.len(), 256);
    assert!(
        a.iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == second_retry))
    );
    let second_retry = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 1010. => Some(*timer),
            _ => None,
        })
        .expect("native Backoff replaces the overdue sibling key's timer");
    e.handle(
        1011.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["release".into()],
        }),
    );
    assert!(actions(&mut e, 1011.).is_empty());
    e.handle(3010., Event::TimerFired { timer });
    let first = request(actions(&mut e, 3010.));
    let first_body = first.body.as_deref().unwrap();
    assert!(first_body.starts_with(r#"[{"token_id":"1"},"#));
    assert!(first_body.ends_with(r#"{"token_id":"100"}]"#));
    answer(&mut e, &first, 3011., r#"{"1":"0.4"}"#);
    assert_eq!(e.store().tickers().len(), 1);
    assert_eq!(e.store().tickers()[0].id, "1");
    e.handle(
        3012.,
        Event::TimerFired {
            timer: second_retry,
        },
    );
    let next = request(actions(&mut e, 3012.));
    assert_eq!(
        next.body.as_deref(),
        Some(r#"[{"token_id":"101"},{"token_id":"102"}]"#)
    );
    answer(&mut e, &next, 3013., "{}");
    assert!(e.store().loaded().tickers);
    e.handle(
        3014.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["saturate".into()],
        }),
    );
    assert!(actions(&mut e, 3014.).is_empty());
    assert_eq!(
        e.venue().peers.len(),
        256,
        "every completed batch released its credit"
    );
    e.handle(
        3015.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["release".into()],
        }),
    );
    assert!(actions(&mut e, 3015.).is_empty());
}
#[test]
fn stale_preparation_and_response_cannot_clear_the_retried_offsets_owner() {
    let mut e = safe_cex_engine::test_support::seeded_engine(venue());
    let gamma = start(&mut e);
    catalogue(&mut e, gamma, 2);
    let old = token(&mut e);
    e.http_preparation_failed(11., old, Error::Invalid, Error::Invalid.read_venue_error());
    let a = actions(&mut e, 11.);
    let timer = retry(&a, 11.);
    assert!(!e.http_preparation_pending(old));
    e.handle(1011., Event::TimerFired { timer });
    let current = token(&mut e);
    assert!(e.http_preparation_pending(current));
    e.http_prepared(
        1012.,
        old,
        Prepared {
            id: old.attempt().request.0,
            expires_at: None,
        },
    );
    e.handle(
        1012.,
        Event::HttpResponse {
            request: old.attempt().request,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"1":"0.99"}"#,
            },
        },
    );
    assert!(e.drain_actions().next().is_none());
    assert!(e.http_preparation_pending(current));
    assert!(e.store().tickers().is_empty());
    e.http_prepared(
        1013.,
        current,
        Prepared {
            id: current.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, packet) = e
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
        e.claim_http(1013., permit),
        Ok(Claim::Released(_))
    ));
    answer(&mut e, &packet, 1014., r#"{"1":"0.4"}"#);
    assert!(e.store().loaded().tickers);
    assert_eq!(e.store().tickers()[0].last, 0.4);
}
