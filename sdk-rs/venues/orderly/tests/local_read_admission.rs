mod support;

use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{Action, Command, Cx, Engine, Event, Notice, Response, Venue};
use safe_cex_venue_orderly::{Options, Orderly, Pending};

const COMMAND: u32 = 971;

fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

struct Probe {
    inner: Orderly,
    peers: Vec<Context>,
    original: Option<Context>,
    hold_next: bool,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.inner.capabilities()
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item);
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: safe_cex_engine::SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: safe_cex_engine::SocketId,
        text: &str,
    ) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: safe_cex_engine::SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: safe_cex_engine::Response<'_>,
    ) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, reason: &str) {
        self.inner.on_request_failed(cx, pending, reason);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: safe_cex_engine::TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.inner.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = &command {
            match symbols.first().map(String::as_str) {
                Some("saturate") => {
                    loop {
                        match cx.http_context(Scope::Background, policy()) {
                            Ok(context) => self.peers.push(context),
                            Err(Error::Capacity) => break,
                            Err(error) => panic!("unexpected peer admission: {error:?}"),
                        }
                    }
                    assert_eq!(self.peers.len(), 256);
                }
                Some("release") => {
                    for context in self.peers.drain(..) {
                        cx.finish_http_context(&context).unwrap();
                    }
                }
                Some("hold") => self.hold_next = true,
                Some("finish") => {
                    let context = self.original.take().expect("original root is still held");
                    cx.done_context(&context, "\"original\"".into()).unwrap();
                }
                _ => panic!("unknown fixture control"),
            }
            return;
        }
        if self.hold_next {
            self.hold_next = false;
            self.original = Some(cx.admit_http_command(policy()).unwrap());
            return;
        }
        self.inner.on_command(cx, command);
    }
}
fn engine(case: u8) -> Engine<Probe> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Orderly::new(Options {
            public_data_only: case < 4,
            application_id: (case == 4).then(|| "fixture-broker".into()),
            ..Options::default()
        }),
        peers: Vec::new(),
        original: None,
        hold_next: false,
    });
    if case < 4 {
        e.start(0.0);
        for (suffix, body) in [
            ("/v1/public/info", support::MARKETS),
            ("/v1/public/futures", support::TICKERS),
        ] {
            let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0)
                .into_iter()
                .find_map(|action| match action {
                    Action::Http(request) if request.url.ends_with(suffix) => Some(request),
                    _ => None,
                })
                .expect("real managed startup");
            e.handle(
                0.0,
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
        let _ = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
        assert!(e.store().loaded().markets && e.store().loaded().tickers);
    }
    e
}
fn run(e: &mut Engine<Probe>, command: Command) -> Vec<Action> {
    let before = format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}",
        e.store().loaded(),
        e.store().markets(),
        e.store().tickers(),
        e.store().orders(),
        e.store().positions(),
        e.store().balance()
    );
    e.handle(100.0, Event::Command(command));
    let out = e.drain_actions().collect();
    assert_eq!(
        format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}",
            e.store().loaded(),
            e.store().markets(),
            e.store().tickers(),
            e.store().orders(),
            e.store().positions(),
            e.store().balance()
        ),
        before
    );
    out
}
fn control(e: &mut Engine<Probe>, name: &str) -> Vec<Action> {
    run(
        e,
        Command::SetTickerSymbols {
            symbols: vec![name.into()],
        },
    )
}
fn command(case: u8) -> Command {
    if case >= 4 {
        return Command::FetchAccount { command: COMMAND };
    }
    Command::FetchCandles {
        command: COMMAND,
        symbol: if case == 0 { "UNKNOWN" } else { "BTCUSDC" }.into(),
        interval: if case == 1 {
            Interval::S1
        } else {
            Interval::M1
        },
        limit: Some(if case == 2 { 0 } else { 1 }),
        from_ms: (case == 3).then_some(101.0),
        to_ms: None,
    }
}
fn assert_local(out: &[Action], expected: Result<&str, &str>) {
    let mut timers = Vec::new();
    let mut terminal = None;
    for action in out {
        match action {
            Action::SetTimer { timer, at_ms } => {
                assert!(at_ms.is_finite());
                timers.push(*timer);
            }
            Action::ClearTimer { timer } => {
                let index = timers
                    .iter()
                    .position(|value| value == timer)
                    .expect("matching local timer");
                timers.remove(index);
            }
            Action::Notice(Notice::CommandDone {
                command: COMMAND,
                result,
            }) => {
                assert!(terminal.replace(Ok(result.as_str())).is_none());
            }
            Action::Notice(Notice::CommandFailed {
                command: COMMAND,
                error,
            }) => {
                assert_eq!(error.kind, None);
                assert_eq!(error.outcome, None);
                assert!(terminal.replace(Err(error.message.as_str())).is_none());
            }
            _ => panic!("no transport/sign/socket/unrelated work: {out:?}"),
        }
    }
    assert!(timers.is_empty());
    assert_eq!(terminal, Some(expected));
}
fn expected(case: u8) -> Result<&'static str, &'static str> {
    match case {
        0 => Err("Orderly unknown symbol"),
        1 => Err("Orderly does not support 1s candles"),
        2 => Err("Orderly invalid candle parameters"),
        3 => Ok("[]"),
        4 => Ok(r#"{"userId":"fixture-broker","affiliateId":"fixture-broker"}"#),
        5 => Err("Orderly private account ID is required"),
        _ => unreachable!(),
    }
}
fn capacity(case: u8) {
    let mut e = engine(case);
    assert!(control(&mut e, "saturate").is_empty());
    let out = run(&mut e, command(case));
    let mut expected_error = Error::Capacity.venue_error();
    expected_error.outcome = None;
    assert!(
        matches!(&out[..], [Action::Notice(Notice::CommandFailed { command: COMMAND, error })] if error == &expected_error),
        "shared admission must refuse once, including local success: {out:?}"
    );
    assert_eq!(e.venue().peers.len(), 256);
    assert!(control(&mut e, "release").is_empty());
    assert_local(&run(&mut e, command(case)), expected(case));
}
#[test]
fn unknown_symbol_capacity_refuses_then_recovers() {
    capacity(0);
}
#[test]
fn unsupported_interval_capacity_refuses_then_recovers() {
    capacity(1);
}
#[test]
fn invalid_parameters_capacity_refuses_then_recovers() {
    capacity(2);
}
#[test]
fn empty_candle_success_requires_capacity_then_recovers() {
    capacity(3);
}
#[test]
fn identity_success_requires_capacity_then_recovers() {
    capacity(4);
}
#[test]
fn identity_failure_capacity_refuses_then_recovers() {
    capacity(5);
}

#[test]
fn peer_capacity_is_real_and_releasing_it_recovers_every_slot() {
    let mut e = engine(4);
    assert!(control(&mut e, "saturate").is_empty());
    assert_eq!(e.venue().peers.len(), 256);
    assert!(control(&mut e, "release").is_empty());
    assert!(e.venue().peers.is_empty());
    assert!(control(&mut e, "saturate").is_empty());
    assert_eq!(e.venue().peers.len(), 256);
    assert!(control(&mut e, "release").is_empty());
}

#[test]
fn retained_identity_owner_rejects_success_and_failure_duplicates() {
    for case in [4, 5] {
        let mut e = engine(case);
        assert!(control(&mut e, "hold").is_empty());
        assert!(run(&mut e, command(case)).is_empty());
        let held = e.venue().original.clone();
        for _ in 0..3 {
            let out = run(&mut e, command(case));
            assert!(
                matches!(&out[..], [Action::Notice(Notice::Event(event))] if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE),
                "{out:?}"
            );
            assert_eq!(e.venue().original, held);
        }
        let out = control(&mut e, "finish");
        assert_local(&out, Ok("\"original\""));
        for _ in 0..300 {
            assert_local(&run(&mut e, command(case)), expected(case));
        }
        assert!(control(&mut e, "saturate").is_empty());
        assert_eq!(e.venue().peers.len(), 256);
        assert!(control(&mut e, "release").is_empty());
    }
}
