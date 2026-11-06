use super::*;
use crate::request_context::{Context, HttpAction, Operation, Policy, RetryPolicy, Scope};
use crate::startup::Sections;

#[derive(Debug)]
struct Read {
    sections: Sections,
    context: Context,
}
struct Probe;

fn issue(cx: &mut Cx<'_, Read>, sections: Sections, timeout_ms: u32) {
    let context = cx
        .http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        )
        .unwrap();
    cx.request_managed(
        context.clone(),
        None,
        Method::Get,
        "https://startup.test/read".into(),
        None,
        Read { sections, context },
    )
    .unwrap();
}

impl Venue for Probe {
    type Pending = Read;
    fn startup_reads(read: &Read) -> Sections {
        read.sections
    }
    fn start(&mut self, cx: &mut Cx<'_, Read>) {
        for sections in [
            Sections::MARKETS,
            Sections::TICKERS,
            Sections::BALANCE,
            Sections::NONE,
        ] {
            issue(cx, sections, 120_000);
        }
        issue(cx, Sections::ORDERS, 100);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Read>, read: Read, response: Response<'_>) {
        if response.body == "refused" {
            cx.notice_error(
                Level::Warn,
                log_code::REQUEST_FAILED,
                &VenueError::new("private message not copied")
                    .kind(ErrorKind::AuthPermission)
                    .venue_code("10006"),
            );
            let _ = cx.finish_http_context(&read.context);
            issue(cx, read.sections, 120_000);
        } else {
            cx.notice_error(
                Level::Warn,
                log_code::REQUEST_FAILED,
                &VenueError::new("unrelated candle error")
                    .kind(ErrorKind::Rejected)
                    .venue_code("90000"),
            );
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Read>, _: Read, error: &str) {
        cx.notice(Level::Warn, log_code::REQUEST_FAILED, error);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Read>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Read>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Read>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Read>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, Read>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn requests(engine: &mut Engine<Probe>, now: f64) -> Vec<HttpRequest> {
    crate::test_support::prepared_actions(engine, now)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect()
}

fn fields(engine: &mut Engine<Probe>, now: f64) -> Vec<(&'static str, Value)> {
    engine.handle(now, Event::PollScaleChanged { scale: 1.0 });
    engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::Event(e)) if e.code == log_code::STARTUP_INCOMPLETE => {
                Some(e.fields)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn native_failure_transport_timeout_retry_and_unrelated_read_keep_their_own_causes() {
    let mut engine = crate::test_support::seeded_engine(Probe);
    engine.set_account_startup(true);
    engine.start(0.);
    let requests = requests(&mut engine, 0.);
    assert_eq!(requests.len(), 5);
    engine.handle(
        1.,
        Event::HttpResponse {
            request: requests[0].id,
            response: Response {
                status: 403,
                headers: "",
                body: "refused",
            },
        },
    );
    let retry = super::startup_cause_tests::requests(&mut engine, 1.);
    assert_eq!(retry.len(), 1);
    engine.handle(
        2.,
        Event::HttpFailed {
            request: requests[1].id,
            error: "host disconnected",
        },
    );
    engine.handle(
        3.,
        Event::HttpResponse {
            request: requests[3].id,
            response: Response {
                status: 200,
                headers: "",
                body: "unrelated",
            },
        },
    );
    let timer = engine
        .io
        .core
        .managed_http
        .get(&requests[4].id)
        .unwrap()
        .timer;
    engine.handle(100., Event::TimerFired { timer });
    let got = fields(&mut engine, STARTUP_WATCH_MS);
    for pair in [
        ("startupMarketsState", "refused".into()),
        ("startupMarketsKind", "authPermission".into()),
        ("startupMarketsVenueCode", "10006".into()),
        ("startupMarketsHttpStatus", 403_u16.into()),
        ("startupTickersState", "failed".into()),
        ("startupTickersKind", "network".into()),
        ("startupBalanceState", "started".into()),
        ("startupOrdersState", "timedOut".into()),
        ("startupOrdersKind", "timeout".into()),
        ("startupPositionsState", "neverStarted".into()),
    ] {
        assert!(got.contains(&pair), "missing {pair:?} in {got:?}");
    }
    assert!(!got.iter().any(|(_, v)| v == &Value::from("90000")));
    assert!(!got.iter().any(|(_, v)| matches!(v, Value::Str(s) if s.contains("private message") || s.contains("startup.test"))));
}

#[test]
fn real_preparation_refusal_is_retained_without_http_or_invented_status() {
    let mut engine = crate::test_support::seeded_engine(Probe);
    engine.set_account_startup(true);
    engine.start(0.);
    let preparations: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect();
    engine.http_preparation_failed(
        1.,
        preparations[0],
        crate::request_context::Error::Invalid,
        VenueError::new("policy refused").kind(ErrorKind::InvalidInput),
    );
    let got = fields(&mut engine, STARTUP_WATCH_MS);
    assert!(
        got.contains(&("startupMarketsState", "refused".into())),
        "{got:?}"
    );
    assert!(got.contains(&("startupMarketsKind", "invalidInput".into())));
    assert!(
        !got.iter()
            .any(|(key, _)| *key == "startupMarketsHttpStatus")
    );
}

#[test]
fn loaded_and_nonrequired_sections_have_no_retained_failure_fields() {
    let mut engine = crate::test_support::seeded_engine(Probe);
    engine.start(0.);
    let requests = requests(&mut engine, 0.);
    engine.handle(
        1.,
        Event::HttpFailed {
            request: requests[2].id,
            error: "private transport",
        },
    );
    engine
        .store_mut()
        .set_markets(vec![super::streaming_tests::market("A")]);
    let got = fields(&mut engine, STARTUP_WATCH_MS);
    assert!(got.contains(&("startupTickersState", "started".into())));
    assert!(!got.iter().any(|(key, _)| key.starts_with("startupMarkets")
        || key.starts_with("startupBalance")
        || key.starts_with("startupOrders")
        || key.starts_with("startupPositions")));
}
