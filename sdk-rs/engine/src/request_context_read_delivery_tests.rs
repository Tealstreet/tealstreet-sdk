use super::*;
use crate::request_context::{Claim, HttpAction, Prepared, RetryPolicy};
use crate::{Action, Engine, Event, Method, Notice, Response, SocketId, TimerId, Venue};

#[derive(Default)]
struct ReadPublicationProbe {
    wrong_leg: bool,
    root: Option<Context>,
    argument: Option<Context>,
    socket: Option<SocketId>,
    native_publications: usize,
    callbacks: usize,
    failure_authority: Option<Result<(), Error>>,
    failure_kind: Option<crate::ErrorKind>,
}

fn read_policy(timeout_ms: u32) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms,
        pacing: None,
        retry: RetryPolicy::ReadFailure,
    }
}

impl ReadPublicationProbe {
    fn publish(&mut self, cx: &mut Cx<'_, Context>) {
        let root = self.root.as_ref().unwrap();
        let leg = self.argument.as_ref().unwrap();
        let authority = cx.validate_read_http_delivery(root, leg);
        if authority.is_ok() {
            self.native_publications += 1;
            cx.store.set_hedged(true);
            cx.done_context(root, "{}".into()).unwrap();
        }
    }
}

impl Venue for ReadPublicationProbe {
    type Pending = Context;
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        let root = cx
            .http_context(Scope::Command(7), read_policy(100))
            .unwrap();
        let actual = cx.http_leg_context(&root, read_policy(50)).unwrap();
        let sibling = cx.http_leg_context(&root, read_policy(50)).unwrap();
        self.argument = Some(if self.wrong_leg {
            sibling
        } else {
            actual.clone()
        });
        self.socket = Some(cx.connect("wss://read-publication.test"));
        cx.request_managed(
            actual.clone(),
            None,
            Method::Get,
            "https://read-publication.test/settings".into(),
            None,
            actual,
        )
        .unwrap();
        self.root = Some(root);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {
        self.callbacks += 1;
        self.publish(cx);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Context>, _: SocketId, _: &str) {
        self.callbacks += 1;
        self.publish(cx);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, leg: Context, _: &str) {
        self.failure_authority =
            Some(cx.validate_read_http_delivery(self.root.as_ref().unwrap(), &leg));
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, leg: Context, error: VenueError) {
        self.failure_kind = error.kind;
        self.on_request_failed(cx, leg, &error.message);
    }
    fn on_command(&mut self, _: &mut Cx<'_, Context>, _: crate::Command) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn released_read(wrong_leg: bool) -> (Engine<ReadPublicationProbe>, RequestId) {
    let mut engine = crate::test_support::seeded_engine(ReadPublicationProbe {
        wrong_leg,
        ..Default::default()
    });
    engine.start(0.);
    let preparation = engine
        .drain_actions()
        .find_map(|action| match action {
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
        .find_map(|action| match action {
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

#[test]
fn read_publication_rejects_a_live_same_owner_sibling_before_native_and_store_effects() {
    for wrong_leg in [true, false] {
        let (mut engine, request) = released_read(wrong_leg);
        engine.handle(
            3.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "{}",
                },
            },
        );
        assert_eq!(engine.venue().callbacks, 1);
        assert_eq!(engine.venue().native_publications, usize::from(!wrong_leg));
        assert_eq!(engine.store().hedged(), !wrong_leg);
        let completions = engine
            .drain_actions()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
            .count();
        assert_eq!(completions, usize::from(!wrong_leg));
    }
}

#[test]
fn read_publication_cannot_borrow_live_read_handles_from_a_socket_callback() {
    let (mut engine, _) = released_read(false);
    let socket = engine.venue().socket.unwrap();
    engine.handle(
        3.,
        Event::WsMessage {
            socket,
            text: "unrelated",
        },
    );
    assert_eq!(engine.venue().callbacks, 1);
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
}

#[test]
fn read_publication_exact_physical_expiry_never_reaches_the_codec() {
    let (mut engine, request) = released_read(false);
    engine.handle(
        50.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    assert_eq!(engine.venue().callbacks, 0);
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
}

#[test]
fn read_delivery_failure_ownership_does_not_prove_native_response_data() {
    let (mut engine, request) = released_read(false);
    engine.handle(
        3.,
        Event::HttpFailed {
            request,
            error: "network lost",
        },
    );
    assert_eq!(engine.venue().failure_authority, Some(Ok(())));
    assert_eq!(engine.venue().native_publications, 0);
    assert!(!engine.store().hedged());
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
}

#[test]
fn released_deadline_expiry_is_a_typed_timeout_and_host_loss_stays_network() {
    let (mut engine, request) = released_read(false);
    // Managed identities are reserved as request, generation, timer.
    engine.handle(
        50.,
        Event::TimerFired {
            timer: TimerId(request.0 + 2),
        },
    );
    assert_eq!(engine.venue().failure_kind, Some(crate::ErrorKind::Timeout));

    let (mut engine, request) = released_read(false);
    engine.handle(
        3.,
        Event::HttpFailed {
            request,
            error: "network lost",
        },
    );
    assert_eq!(engine.venue().failure_kind, Some(crate::ErrorKind::Network));
}

#[test]
fn a_delivered_body_that_is_not_utf8_is_a_typed_malformed_reply() {
    let (mut engine, request) = released_read(false);
    engine.handle(
        3.,
        Event::HttpStart {
            request,
            status: 200,
            headers: "",
        },
    );
    engine.handle(
        3.,
        Event::HttpChunk {
            request,
            data: &[0xff, 0xfe],
        },
    );
    engine.handle(3., Event::HttpEnd { request });
    assert_eq!(
        engine.venue().failure_kind,
        Some(crate::ErrorKind::Internal)
    );
    assert_eq!(engine.venue().native_publications, 0);
}

#[test]
fn a_stream_that_breaks_mid_array_is_a_typed_malformed_reply() {
    let (mut engine, request) = released_read(false);
    {
        let mut cx = Cx {
            now: 2.,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        cx.stream_response(request, crate::json::ArrayAt::Root);
    }
    engine.handle(
        3.,
        Event::HttpStart {
            request,
            status: 200,
            headers: "",
        },
    );
    engine.handle(
        3.,
        Event::HttpChunk {
            request,
            data: &[b'[', 0xff],
        },
    );
    engine.handle(3., Event::HttpEnd { request });
    assert_eq!(
        engine.venue().failure_kind,
        Some(crate::ErrorKind::Internal)
    );
    assert_eq!(engine.venue().native_publications, 0);
}

#[test]
fn read_publication_pair_authority_is_exact_and_read_only() {
    for case in 0..16 {
        let mut io = super::super::Io::<()>::default();
        let mut store = crate::Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        let root = cx
            .http_context(Scope::Command(7), read_policy(100))
            .unwrap();
        let leg = cx.http_leg_context(&root, read_policy(50)).unwrap();
        let sibling = cx.http_leg_context(&root, read_policy(50)).unwrap();
        let mut root_argument = root.clone();
        let mut leg_argument = leg.clone();
        cx.io.core.http_delivery = Some(Delivery {
            context: leg.clone(),
            attempt: crate::request_context::Attempt {
                request: RequestId(99),
                generation: 100,
            },
        });
        let expected = match case {
            0 => {
                // The original root survives logical handle retirement.
                cx.finish_http_context(&root).unwrap();
                Ok(())
            }
            1 => {
                root_argument = sibling;
                Err(Error::Revoked)
            }
            2 => {
                root_argument = ContextRegistry::default()
                    .issue(1, Scope::Command(7), read_policy(100), 0.)
                    .unwrap();
                Err(Error::Revoked)
            }
            3 => {
                leg_argument = ContextRegistry::default()
                    .issue(2, Scope::Command(7), read_policy(50), 0.)
                    .unwrap();
                Err(Error::Revoked)
            }
            4 => {
                root_argument = cx
                    .http_context(Scope::Command(8), read_policy(100))
                    .unwrap();
                Err(Error::Revoked)
            }
            5 => {
                let mut mutation = read_policy(100);
                mutation.operation = Operation::Mutation;
                mutation.retry = RetryPolicy::Never;
                cx.http_context(Scope::Command(7), mutation).unwrap();
                Err(Error::Invalid)
            }
            6 => {
                cx.now = leg.deadline();
                Err(Error::TimedOut)
            }
            7 => {
                cx.io.core.http_delivery = None;
                Err(Error::Revoked)
            }
            8 => {
                cx.io
                    .core
                    .request_contexts
                    .commands
                    .get_mut(&7)
                    .unwrap()
                    .generation += 1;
                Err(Error::Revoked)
            }
            9 => {
                root_argument = cx
                    .http_context(Scope::Background, read_policy(100))
                    .unwrap();
                Err(Error::Invalid)
            }
            10 => {
                leg_argument = root.clone();
                cx.io.core.http_delivery.as_mut().unwrap().context = root.clone();
                Ok(())
            }
            11 => {
                cx.finish_http_context(&leg).unwrap();
                Err(Error::Revoked)
            }
            12 => {
                leg_argument = cx
                    .http_context(Scope::Command(7), read_policy(200))
                    .unwrap();
                cx.io.core.http_delivery.as_mut().unwrap().context = leg_argument.clone();
                Err(Error::Revoked)
            }
            13 => {
                root_argument = root.clone().with_retry(RetryPolicy::Never).unwrap();
                Err(Error::Revoked)
            }
            14 => {
                let mut mutation = read_policy(100);
                mutation.operation = Operation::Mutation;
                mutation.retry = RetryPolicy::Never;
                let mutation = cx.http_context(Scope::Command(7), mutation).unwrap();
                cx.finish_http_context(&mutation).unwrap();
                Err(Error::Invalid)
            }
            15 => {
                cx.now = f64::NAN;
                Err(Error::Invalid)
            }
            _ => unreachable!(),
        };
        let contexts = cx.io.core.request_contexts.contexts.len();
        let commands = cx.io.core.request_contexts.commands.len();
        let delivery = cx
            .io
            .core
            .http_delivery
            .as_ref()
            .map(|d| (d.context.clone(), d.attempt));
        assert_eq!(
            cx.validate_read_http_delivery(&root_argument, &leg_argument),
            expected,
            "case {case}"
        );
        assert_eq!(
            cx.validate_read_http_delivery(&root_argument, &leg_argument),
            expected,
            "repeat {case}"
        );
        assert_eq!(cx.io.core.request_contexts.contexts.len(), contexts);
        assert_eq!(cx.io.core.request_contexts.commands.len(), commands);
        assert_eq!(
            cx.io
                .core
                .http_delivery
                .as_ref()
                .map(|d| (d.context.clone(), d.attempt)),
            delivery
        );
        assert!(cx.io.core.actions.is_empty());
        assert!(cx.io.pending.is_empty());
        assert!(cx.io.core.timers.is_empty());
        assert!(!cx.store.hedged());
    }
}
