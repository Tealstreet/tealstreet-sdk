use super::*;
use crate::book::Book;
use crate::request_context::{Operation, Policy, RetryPolicy, Scope};

#[derive(Default)]
struct Probe {
    mutation: bool,
    refusals: usize,
}
fn read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        for _ in 0..256 {
            cx.http_context(Scope::Background, read_policy()).unwrap();
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        let command = command.completion_id().unwrap();
        let mut policy = read_policy();
        if self.mutation {
            policy.operation = Operation::Mutation;
            policy.credential = Some(crate::request_context::Credential {
                slot: CredentialSlot::Primary,
                reference: crate::signing::KeyRef("entry-fixture".into()),
            });
        }
        // Architectural RED: without the numeric bridge there is no existing
        // engine rejection for a Context that could not be issued.
        match cx.http_context(Scope::Command(command), policy) {
            Ok(context) => {
                cx.done_context(&context, "unexpected".into()).unwrap();
            }
            Err(_) => {
                self.refusals += 1;
            }
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn command() -> Command {
    Command::FetchCandles {
        command: 7,
        symbol: "BTCUSD".into(),
        interval: Interval::M1,
        limit: Some(2),
        from_ms: None,
        to_ms: None,
    }
}
#[test]
fn unissued_read_command_is_rejected_once_without_a_numeric_fallback() {
    let mut e = crate::test_support::seeded_engine(Probe::default());
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.handle(1., Event::Command(command()));
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(e.venue.refusals, 1);
    let errors = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        errors.len(),
        1,
        "entry refusal must not leave the command hanging"
    );
    assert_eq!(errors[0].kind, Some(ErrorKind::Internal));
    assert_eq!(errors[0].outcome, None);
    assert!(e.io.managed_http.is_empty());
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::ManagedHttp(_) | Action::SetTimer { .. } | Action::Sign { .. }
    )));
}
#[test]
fn unissued_write_command_is_not_sent_without_a_context_or_write_journal() {
    let mut e = crate::test_support::seeded_engine(Probe {
        mutation: true,
        ..Default::default()
    });
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.handle(1., Event::Command(command()));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let errors = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1, "write entry refusal must be engine-owned");
    assert_eq!(errors[0].outcome, Some(crate::Outcome::NotSent));
    assert!(e.io.managed_http.is_empty());
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::ManagedHttp(_) | Action::SetTimer { .. } | Action::Sign { .. }
    )));
}
