use super::*;
use crate::request_context::{Claim, HttpAction, Prepared, RetryPolicy};
use crate::{Action, Command, Engine, Event, Method, Notice, Response, SocketId, TimerId, Venue};

// The baseline checkpoint used logical liveness alone. Only the authority call
// changes here; the original native/Store publication assertions stay exact.
fn authority<P>(cx: &Cx<'_, P>, context: &Context) -> Result<(), Error> {
    let ids = cx.io.core.next_id;
    let actions = cx.io.core.actions.len();
    let contexts = cx.io.core.request_contexts.contexts.len();
    let commands = cx.io.core.request_contexts.commands.len();
    let before = cx
        .io
        .core
        .http_delivery
        .as_ref()
        .map(|d| (d.context.clone(), d.attempt));
    let result = cx.validate_background_read_http_delivery(context);
    assert_eq!(cx.io.core.next_id, ids);
    assert_eq!(cx.io.core.actions.len(), actions);
    assert_eq!(cx.io.core.request_contexts.contexts.len(), contexts);
    assert_eq!(cx.io.core.request_contexts.commands.len(), commands);
    assert_eq!(
        cx.io
            .core
            .http_delivery
            .as_ref()
            .map(|d| (d.context.clone(), d.attempt)),
        before
    );
    result
}

fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 50,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[derive(Default)]
struct Probe {
    execution_read: bool,
    execution: Option<crate::engine::ExecutionHandle>,
    wrong_peer: bool,
    argument: Option<Context>,
    socket: Option<SocketId>,
    callbacks: usize,
    native_publications: usize,
    checks: Vec<Result<(), Error>>,
}

impl Probe {
    fn check(&mut self, cx: &Cx<'_, Context>) -> bool {
        let result = authority(cx, self.argument.as_ref().unwrap());
        self.checks.push(result);
        result.is_ok()
    }
}

impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        if self.execution_read {
            return;
        }
        let actual = cx.http_context(Scope::Background, policy()).unwrap();
        let peer = cx.http_context(Scope::Background, policy()).unwrap();
        self.argument = Some(if self.wrong_peer {
            peer
        } else {
            actual.clone()
        });
        self.socket = Some(cx.connect("wss://background-read.test"));
        cx.request_managed(
            actual.clone(),
            None,
            Method::Get,
            "https://background-read.test/catalogue".into(),
            None,
            actual,
        )
        .unwrap();
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        self.callbacks += 1;
        if let Some(execution) = &self.execution {
            assert_eq!(
                cx.validate_execution_read_delivery(execution, &context),
                Ok(())
            );
        }
        // Native validity is deliberately independent of delivery ownership.
        if self.check(cx) && response.status == 200 && response.body == "{\"valid\":true}" {
            self.native_publications += 1;
            cx.store.set_hedged(true);
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, _: Context, _: &str) {
        self.check(cx);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Context>, _: SocketId, _: &str) {
        if self.check(cx) {
            self.native_publications += 1;
            cx.store.set_hedged(true);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        if !self.execution_read {
            return;
        }
        let root = cx.admit_http_command(policy()).unwrap();
        let execution = cx
            .begin_execution(
                &root,
                policy(),
                crate::engine::Window::UntilTerminal,
                "background-denial",
            )
            .unwrap();
        let leg = cx.http_leg_context(execution.context(), policy()).unwrap();
        self.execution = Some(execution);
        self.argument = Some(leg.clone());
        cx.request_managed(
            leg.clone(),
            None,
            Method::Get,
            "https://background-read.test/execution".into(),
            None,
            leg,
        )
        .unwrap();
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn released(wrong_peer: bool) -> (Engine<Probe>, RequestId) {
    released_probe(Probe {
        wrong_peer,
        ..Default::default()
    })
}

fn released_probe(probe: Probe) -> (Engine<Probe>, RequestId) {
    let execution_read = probe.execution_read;
    let mut engine = crate::test_support::seeded_engine(probe);
    engine.start(0.);
    if execution_read {
        engine.handle(
            0.,
            Event::Command(Command::FetchCandles {
                command: 7,
                symbol: "BTCUSD".into(),
                interval: crate::model::Interval::M1,
                limit: Some(2),
                from_ms: None,
                to_ms: None,
            }),
        );
    }
    let preparation = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.http_prepared(
        1.,
        preparation,
        Prepared {
            id: 55,
            expires_at: None,
        },
    );
    let permit = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        engine.claim_http(2., permit),
        Ok(Claim::Released(_))
    ));
    (engine, preparation.attempt().request)
}

fn response(engine: &mut Engine<Probe>, request: RequestId, now: f64, body: &str) {
    engine.handle(
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

#[test]
fn background_delivery_wrong_live_peer_cannot_publish_native_or_store_data() {
    let (mut engine, request) = released(true);
    response(&mut engine, request, 3., "{\"valid\":true}");
    assert_eq!(engine.venue().callbacks, 1);
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
    assert_eq!(engine.venue().checks, [Err(Error::Revoked)]);
}

#[test]
fn background_delivery_socket_cannot_borrow_a_live_or_completed_read() {
    for completed in [false, true] {
        let (mut engine, request) = released(false);
        if completed {
            response(&mut engine, request, 3., "{\"valid\":true}");
        }
        let socket = engine.venue().socket.unwrap();
        engine.handle(
            4.,
            Event::WsMessage {
                socket,
                text: "unrelated",
            },
        );
        assert_eq!(engine.venue().native_publications, usize::from(completed));
        assert_eq!(engine.store().hedged(), completed);
        assert_eq!(engine.venue().checks.last(), Some(&Err(Error::Revoked)));
    }
}

#[test]
fn background_delivery_matching_response_publishes_once_without_command_completion() {
    let (mut engine, request) = released(false);
    response(&mut engine, request, 3., "{\"valid\":true}");
    response(&mut engine, request, 4., "{\"valid\":true}");
    assert_eq!(engine.venue().callbacks, 1);
    assert_eq!(engine.venue().checks, [Ok(())]);
    assert_eq!(engine.venue().native_publications, 1);
    assert!(engine.store().hedged());
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}

#[test]
fn background_delivery_exact_expiry_and_late_duplicate_never_reach_native_codec() {
    let (mut engine, request) = released(false);
    response(&mut engine, request, 50., "{\"valid\":true}");
    response(&mut engine, request, 51., "{\"valid\":true}");
    assert_eq!(engine.venue().callbacks, 0);
    assert_eq!(engine.venue().checks, [Err(Error::TimedOut)]);
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
}

#[test]
fn background_delivery_failure_identity_does_not_prove_response_data() {
    let (mut engine, request) = released(false);
    engine.handle(
        3.,
        Event::HttpFailed {
            request,
            error: "network lost",
        },
    );
    assert_eq!(engine.venue().checks, [Ok(())]);
    assert_eq!(engine.venue().callbacks, 0);
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
}

#[test]
fn background_delivery_malformed_native_data_is_separately_refused() {
    let (mut engine, request) = released(false);
    response(&mut engine, request, 3., "malformed");
    assert_eq!(engine.venue().checks, [Ok(())]);
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
}

#[test]
fn background_delivery_refusals_preserve_existing_issuer_deadline_and_owner_rules() {
    for case in 0..9 {
        let mut io = super::super::Io::<()>::default();
        let mut store = crate::Store::default();
        let mut cx = Cx {
            now: 0.,
            io: &mut io,
            store: &mut store,
        };
        let actual = cx.http_context(Scope::Background, policy()).unwrap();
        let mut argument = actual.clone();
        cx.io.core.http_delivery = Some(Delivery {
            context: actual.clone(),
            attempt: Attempt {
                request: RequestId(99),
                generation: 100,
            },
        });
        let expected = match case {
            0 => Ok(()),
            1 => {
                cx.io.core.http_delivery = None;
                Err(Error::Revoked)
            }
            2 => {
                argument = ContextRegistry::default()
                    .issue(actual.leg(), Scope::Background, policy(), 0.)
                    .unwrap();
                assert_eq!(argument.leg(), actual.leg());
                assert_eq!(argument.owner(), actual.owner());
                assert_eq!(argument.deadline(), actual.deadline());
                Err(Error::Revoked)
            }
            3 => {
                cx.finish_http_context(&actual).unwrap();
                Err(Error::Revoked)
            }
            4 => {
                cx.now = 50.;
                Err(Error::TimedOut)
            }
            5 => {
                cx.now = f64::NAN;
                Err(Error::Invalid)
            }
            6 | 7 => {
                let mut p = policy();
                if case == 7 {
                    p.operation = Operation::Mutation;
                }
                argument = cx.http_context(Scope::Command(7), p).unwrap();
                cx.io.core.http_delivery.as_mut().unwrap().context = argument.clone();
                Err(Error::Invalid)
            }
            8 => {
                argument = cx.http_context(Scope::Background, policy()).unwrap();
                Err(Error::Revoked)
            }
            _ => unreachable!(),
        };
        assert_eq!(authority(&cx, &argument), expected, "case {case}");
    }
}

#[test]
fn background_delivery_rejects_matching_execution_read_before_publication() {
    let (mut engine, request) = released_probe(Probe {
        execution_read: true,
        ..Default::default()
    });
    assert!(matches!(
        engine.venue().argument.as_ref().unwrap().owner(),
        Owner::Execution { .. }
    ));
    response(&mut engine, request, 3., "{\"valid\":true}");
    assert_eq!(engine.venue().callbacks, 1);
    assert_eq!(engine.venue().checks, [Err(Error::Invalid)]);
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}
