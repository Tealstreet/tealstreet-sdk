use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Error, HttpAction, Preparation, RoutingIntent};
use safe_cex_engine::{
    Action, Command, Cx, Engine, ErrorKind, Event, Notice, Outcome, RequestId, Response, SocketId,
    TimerId, Venue, VenueError,
};
use safe_cex_venue_blofin::{Blofin, Options};

const INSTRUMENT: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;

fn engine() -> Engine<Blofin> {
    safe_cex_engine::test_support::seeded_engine(Blofin::new(Options {
        public_data_only: true,
        ..Options::default()
    }))
}

fn reply(e: &mut Engine<Blofin>, now: f64, request: RequestId, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}

fn preparation(actions: &[Action]) -> Preparation {
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "public HTTP must wait for prepared transport admission"
    );
    actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    assert!(request.headers.is_empty());
                    assert!(request.signer_headers.is_empty());
                    Some(*preparation)
                }
                _ => None,
            },
            _ => None,
        })
        .expect("a public managed preparation")
}

fn ready() -> Engine<Blofin> {
    let mut e = engine();
    e.start(0.);
    for body in [INSTRUMENT, r#"{"code":"0","data":[]}"#] {
        let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
            .into_iter()
            .find_map(|action| match action {
                Action::Http(request) => Some(request.id),
                _ => None,
            })
            .expect("startup HTTP");
        reply(&mut e, 0., request, body);
    }
    e.drain_actions().for_each(drop);
    e
}

fn candles(e: &mut Engine<Blofin>, now: f64, command: u32, symbol: &str, interval: Interval) {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: symbol.into(),
            interval,
            limit: Some(2),
            from_ms: None,
            to_ms: None,
        }),
    );
}

#[test]
fn discovery_waits_for_preparation_and_ignores_unreleased_response() {
    let mut e = engine();
    e.start(100.);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let token = preparation(&actions);
    reply(&mut e, 101., token.attempt().request, INSTRUMENT);
    assert!(e.store().markets().is_empty());
    assert!(!e.store().loaded().markets);
    assert!(
        !e.drain_actions()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

#[test]
fn unclaimed_catalogue_response_cannot_change_the_store() {
    let mut e = engine();
    e.start(100.);
    let request = e
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.id),
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation.attempt().request),
                _ => None,
            },
            _ => None,
        })
        .expect("catalogue request identity");
    reply(&mut e, 101., request, INSTRUMENT);
    assert!(
        e.store().markets().is_empty(),
        "an identity is not a release permit"
    );
    assert!(!e.store().loaded().markets);
}

#[test]
fn parked_candle_preparation_expires_and_late_success_cannot_publish() {
    let mut e = ready();
    candles(&mut e, 100., 71, "BTCUSDT", Interval::M1);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let token = preparation(&actions);
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 20_100. => Some(*timer),
            _ => None,
        })
        .expect("original preparation-inclusive deadline");
    e.handle(20_100., Event::TimerFired { timer });
    let failures = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 71, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(failures[0].outcome, Some(Outcome::NotSent));
    assert!(!e.http_preparation_pending(token));
    reply(
        &mut e,
        20_101.,
        token.attempt().request,
        r#"{"code":"0","data":[[1000,1,2,1,2,4,0,4]]}"#,
    );
    assert!(!e.drain_actions().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 71, .. })
            | Action::Notice(Notice::CommandFailed { command: 71, .. })
    )));
}

#[test]
fn unsupported_prepared_transport_keeps_typed_candle_failure() {
    let mut e = ready();
    candles(&mut e, 100., 72, "BTCUSDT", Interval::M1);
    let token = preparation(&e.drain_actions().collect::<Vec<_>>());
    e.http_preparation_failed(
        101.,
        token,
        Error::Invalid,
        VenueError::new("prepared HTTP unsupported").kind(ErrorKind::Unsupported),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    let failures = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 72, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(ErrorKind::Unsupported));
    assert_eq!(failures[0].outcome, Some(Outcome::NotSent));
}

#[test]
fn candle_validation_is_typed_without_io_and_keeps_market_precedence() {
    let mut e = ready();
    for (command, symbol, kind, message) in [
        (
            73,
            "MISSING",
            ErrorKind::InvalidInput,
            "Unknown market MISSING",
        ),
        (
            74,
            "BTCUSDT",
            ErrorKind::Unsupported,
            "BloFin does not support 1M candles",
        ),
    ] {
        candles(&mut e, 100., command, symbol, Interval::Mo1);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_)))
        );
        let error = actions
            .iter()
            .find_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                    Some(error)
                }
                _ => None,
            })
            .expect("one terminal validation failure");
        assert_eq!(error.kind, Some(kind));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert_eq!(error.message, message);
    }
}

#[test]
fn candle_success_keeps_sorting_and_reused_command_rejects_old_response() {
    let mut e = ready();
    candles(&mut e, 100., 75, "BTCUSDT", Interval::M1);
    let first = safe_cex_engine::test_support::prepared_actions(&mut e, 100.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .expect("released candle request");
    let body = r#"{"code":"0","data":[[2000,2,3,1,2,4,0,5],[1000,1,2,1,2,4,0,6]]}"#;
    reply(&mut e, 101., first, body);
    let results = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 75,
                result,
            }) => Some(result),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 1);
    let rows: serde_json::Value = serde_json::from_str(&results[0]).unwrap();
    assert_eq!(rows[0]["timestamp"], 1.);
    assert_eq!(rows[1]["timestamp"], 2.);
    assert_eq!(rows[0]["volume"], 6.);
    candles(&mut e, 102., 75, "BTCUSDT", Interval::M1);
    let second = safe_cex_engine::test_support::prepared_actions(&mut e, 102.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .expect("new generation's candle request");
    assert_ne!(first, second);
    reply(&mut e, 103., first, body);
    assert!(!e.drain_actions().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 75, .. })
            | Action::Notice(Notice::CommandFailed { command: 75, .. })
    )));
    reply(&mut e, 104., second, r#"{"code":"0","data":[]}"#);
    assert_eq!(
        e.drain_actions()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { command: 75, result }) if result == "[]"
            ))
            .count(),
        1
    );
}

// Re-enter the real discovery callback to exercise logical-handle retirement
// within one deadline, independently of the native retry timer's backoff.
struct Discovery(Blofin);
impl Venue for Discovery {
    type Pending = <Blofin as Venue>::Pending;
    fn start(&mut self, cx: &mut Cx<'_, Self::Pending>) {
        self.0.start(cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Self::Pending>, _: SocketId, _: u16) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Self::Pending>, pending: &mut Self::Pending, item: &str) {
        self.0.on_item(cx, pending, item);
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.0.on_response(cx, pending, response);
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        self.0.on_request_failed(cx, pending, error);
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        self.0.on_sign_error(cx, pending, error);
    }
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        status: u16,
        headers: &str,
        error: &str,
    ) {
        self.0
            .on_stream_malformed(cx, pending, status, headers, error);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        response: Response<'_>,
    ) {
        self.0.on_stream_error(cx, pending, response);
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Self::Pending>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, _: Command) {
        self.0.start(cx);
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn every_discovery_terminal_path_retires_handles_before_the_256_context_bound() {
    for outcome in 0..7 {
        let mut e = safe_cex_engine::test_support::seeded_engine(Discovery(Blofin::new(Options {
            public_data_only: true,
            ..Options::default()
        })));
        e.start(0.);
        for attempt in 0..270 {
            if outcome == 6 {
                let token = preparation(&e.drain_actions().collect::<Vec<_>>());
                e.http_preparation_failed(
                    0.,
                    token,
                    Error::Invalid,
                    VenueError::new("unsupported").kind(ErrorKind::Unsupported),
                );
            } else {
                let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
                    .into_iter()
                    .find_map(|action| match action {
                        Action::Http(request) if request.url.contains("instruments") => {
                            Some(request.id)
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("outcome {outcome} exhausted at attempt {attempt}"));
                if outcome == 5 {
                    e.handle(
                        0.,
                        Event::HttpFailed {
                            request,
                            error: "offline",
                        },
                    );
                } else {
                    let body = match outcome {
                        0 => INSTRUMENT,
                        1 => "not JSON",
                        2 => r#"{"code":"0"}"#,
                        3 => r#"{"code":"1","msg":"refused","data":[]}"#,
                        4 => r#"{"code":"0","data":[]}"#,
                        _ => unreachable!(),
                    };
                    e.handle(
                        0.,
                        Event::HttpResponse {
                            request,
                            response: Response {
                                status: 200,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
            }
            let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
            for action in actions {
                if let Action::Http(request) = action {
                    assert!(request.url.contains("tickers"));
                    e.handle(
                        0.,
                        Event::HttpResponse {
                            request: request.id,
                            response: Response {
                                status: 200,
                                headers: "",
                                body: r#"{"code":"0","data":[]}"#,
                            },
                        },
                    );
                }
            }
            e.drain_actions().for_each(drop);
            e.handle(
                0.,
                Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
            );
        }
    }
}
