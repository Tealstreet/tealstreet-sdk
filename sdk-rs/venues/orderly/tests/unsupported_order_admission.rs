use safe_cex_engine::model::{BybitBbo, PlaceOrderOpts};
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{Action, Command, Cx, Engine, Event, Notice, Venue};
use safe_cex_venue_orderly::{Options, Orderly, Pending};

const COMMAND: u32 = 71;

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
                    cx.fail_context(
                        &context,
                        safe_cex_engine::VenueError::new(
                            "fixture original owner failed before write",
                        ),
                    )
                    .unwrap();
                }
                _ => panic!("unknown fixture control"),
            }
            return;
        }
        if self.hold_next {
            self.hold_next = false;
            let mut mutation = policy();
            mutation.operation = Operation::Mutation;
            self.original = Some(cx.admit_http_command(mutation).unwrap());
            return;
        }
        self.inner.on_command(cx, command);
    }
}
fn engine() -> Engine<Probe> {
    safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Orderly::new(Options {
            public_data_only: true,
            ..Default::default()
        }),
        peers: Vec::new(),
        original: None,
        hold_next: false,
    })
}
fn run(e: &mut Engine<Probe>, command: Command) -> Vec<Action> {
    let before = format!(
        "{:?}{:?}{:?}{:?}",
        e.store().loaded(),
        e.store().orders(),
        e.store().positions(),
        e.store().balance()
    );
    e.handle(100., Event::Command(command));
    let out = e.drain_actions().collect();
    assert_eq!(
        format!(
            "{:?}{:?}{:?}{:?}",
            e.store().loaded(),
            e.store().orders(),
            e.store().positions(),
            e.store().balance()
        ),
        before
    );
    out
}
fn control(e: &mut Engine<Probe>, tag: &str) -> Vec<Action> {
    run(
        e,
        Command::SetTickerSymbols {
            symbols: vec![tag.into()],
        },
    )
}
fn command(case: u8) -> Command {
    let mut order: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":1,"price":50000}"#,
    )
    .unwrap();
    match case {
        0 => {
            order.bybit_bbo = Some(BybitBbo {
                side_type: "Queue".into(),
                level: 1.,
            })
        }
        1 => order.bybit_slippage_tolerance_ticks = Some(1.),
        _ => panic!("unknown control"),
    }
    Command::PlaceOrders {
        command: COMMAND,
        orders: vec![order],
    }
}
const MESSAGE: &str = "Bybit BBO and TickSize controls are not supported by Orderly";
fn failure(out: &[Action]) -> &safe_cex_engine::VenueError {
    match out {
        [
            Action::Notice(Notice::CommandFailed {
                command: COMMAND,
                error,
            }),
        ] => {
            assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
            error
        }
        _ => panic!("one terminal without any physical work: {out:?}"),
    }
}
fn capacity(case: u8) {
    let mut e = engine();
    assert!(control(&mut e, "saturate").is_empty());
    let expected = Error::Capacity.venue_error();
    assert_eq!(failure(&run(&mut e, command(case))), &expected);
    assert_eq!(e.venue().peers.len(), 256);
    assert!(control(&mut e, "release").is_empty());
    assert_eq!(failure(&run(&mut e, command(case))).legacy_text(), MESSAGE);
}
#[test]
fn bbo_capacity_refuses_once_then_recovers() {
    capacity(0);
}
#[test]
fn ticks_capacity_refuses_once_then_recovers() {
    capacity(1);
}

#[test]
fn duplicate_cannot_finish_an_existing_owner_and_fixed_clock_reuse_releases_credits() {
    let mut e = engine();
    assert!(control(&mut e, "hold").is_empty());
    assert!(run(&mut e, command(0)).is_empty());
    for case in 0..2 {
        let out = run(&mut e, command(case));
        assert!(
            matches!(&out[..], [Action::Notice(Notice::Event(event))] if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE),
            "{out:?}"
        );
        assert!(e.venue().original.is_some());
    }
    let out = control(&mut e, "finish");
    assert!(
        matches!(&out[..], [Action::Notice(Notice::CommandFailed { command: COMMAND, error })] if error.message == "fixture original owner failed before write" && error.outcome == Some(safe_cex_engine::Outcome::NotSent)),
        "{out:?}"
    );
    assert!(e.venue().original.is_none());
    for _ in 0..270 {
        for case in 0..2 {
            assert_eq!(failure(&run(&mut e, command(case))).legacy_text(), MESSAGE);
        }
    }
    assert!(control(&mut e, "saturate").is_empty());
    assert_eq!(
        e.venue().peers.len(),
        256,
        "local terminals release every Context credit"
    );
    assert!(control(&mut e, "release").is_empty());
}
