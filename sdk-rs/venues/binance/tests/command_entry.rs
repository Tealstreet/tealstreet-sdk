//! Real native command planners encounter registry refusal under engine custody.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{Action, ErrorKind, Event, Notice, Outcome};
use safe_cex_engine::{Command, Cx, Response, SocketId, TimerId, Venue, book::Book};
use safe_cex_venue_binance::{BinanceFutures, Pending};

struct FullRegistry(BinanceFutures);
impl Venue for FullRegistry {
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.0.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        for _ in 0..256 {
            cx.http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.0.on_command(cx, command);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {
        panic!("no issued request");
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
        panic!("no issued request");
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.0.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.0.books()
    }
}
#[test]
fn native_command_entry_refusal_is_one_typed_result_before_any_transport() {
    for command in [
        Command::FetchCandles {
            command: 7,
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        },
        Command::FetchAccount { command: 7 },
        Command::SetPositionMode {
            command: 7,
            hedged: true,
            symbol: None,
        },
    ] {
        let write = matches!(command, Command::SetPositionMode { .. });
        let (venue, signer) = support::Options {
            key: Some("entry-public-fixture".into()),
            secret: Some("entry-private-fixture".into()),
            ..Default::default()
        }
        .build();
        let mut engine = safe_cex_engine::test_support::seeded_engine(FullRegistry(venue));
        engine.set_signer(Some(signer)).unwrap();
        engine.start(0.);
        engine.drain_actions().for_each(drop);
        engine.handle(1., Event::Command(command));
        let actions = engine.drain_actions().collect::<Vec<_>>();
        let errors = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].kind, Some(ErrorKind::Internal));
        assert_eq!(errors[0].message, "Engine request capacity reached");
        assert_eq!(errors[0].outcome, write.then_some(Outcome::NotSent));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_)
                | Action::ManagedHttp(_)
                | Action::Sign { .. }
                | Action::Notice(Notice::CommandDone { .. } | Notice::CommandError { .. })
        )));
    }
}
