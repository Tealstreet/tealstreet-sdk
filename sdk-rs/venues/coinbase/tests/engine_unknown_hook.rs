// Advanced Trade still dispatches legacy requests; this probe exercises the registered
// Coinbase callback through the real managed engine, without changing venue dispatch.
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Context, Operation, Policy, ReconciliationData, RetryPolicy,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, LogEvent, Method, Notice, Outcome, RequestId, Response,
    SocketId, TimerId, UnknownOutcomeTelemetry, Venue, VenueError,
};
use safe_cex_venue_coinbase::Coinbase;
#[path = "support/unknown_fields.rs"]
mod unknown_fields;
struct ManagedProbe;

impl Venue for ManagedProbe {
    type Pending = Context;
    fn unknown_outcome_telemetry() -> Option<UnknownOutcomeTelemetry> {
        Coinbase::unknown_outcome_telemetry()
    }
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
        panic!("managed failure uses its typed callback")
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, error: VenueError) {
        cx.fail_context(&context, error).unwrap();
    }
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {
        panic!("engine-sealed response must not enter a native decoder")
    }
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<ReconciliationData, safe_cex_engine::request_context::Error> {
        ReconciliationData::new("telemetry-demo".into())
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let context = cx
            .admit_http_command(Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 1000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap();
        cx.request_managed(
            context.clone(),
            None,
            Method::Post,
            "https://venue.test/order".into(),
            Some("{}".into()),
            context,
        )
        .unwrap();
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn released(frame_fed: bool) -> (Engine<ManagedProbe>, RequestId) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(ManagedProbe);
    if frame_fed {
        engine.set_frame_fed().unwrap();
    }
    engine
        .store_mut()
        .set_markets(vec![safe_cex_engine::model::Market {
            id: "BTCUSD".into(),
            symbol: "BTCUSD".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USD".into(),
            active: true,
            precision: safe_cex_engine::model::Precision {
                amount: 0.001,
                price: 0.1,
            },
            limits: safe_cex_engine::model::Limits {
                amount: safe_cex_engine::model::MinMax {
                    min: 0.001,
                    max: 100.,
                },
                leverage: safe_cex_engine::model::MinMax { min: 1., max: 100. },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine.handle(
        1.,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: vec![],
        }),
    );
    let requests: Vec<_> = prepared_actions(&mut engine, 2.)
        .into_iter()
        .filter_map(|action| {
            if let Action::Http(request) = action {
                Some(request.id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(requests.len(), 1);
    (engine, requests[0])
}

fn failure(actions: &[Action]) -> &VenueError {
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert!(!actions.iter().any(|a| match a {
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. }) => true,
        Action::ManagedHttp(action) => matches!(
            **action,
            safe_cex_engine::request_context::HttpAction::Prepare { .. }
                | safe_cex_engine::request_context::HttpAction::Bind { .. }
        ),
        _ => false,
    }));
    failures[0]
}

fn events(actions: &[Action]) -> Vec<&LogEvent> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .collect()
}

fn status_reply(
    engine: &mut Engine<impl Venue>,
    request: RequestId,
    status: u16,
    split: bool,
) -> Vec<Action> {
    if split {
        engine.handle(
            3.,
            Event::HttpStart {
                request,
                status,
                headers: "",
            },
        );
        engine.handle(
            4.,
            Event::HttpChunk {
                request,
                data: b"unproven",
            },
        );
        engine.handle(6., Event::HttpEnd { request });
    } else {
        engine.handle(
            6.,
            Event::HttpResponse {
                request,
                response: Response {
                    status,
                    headers: "",
                    body: "unproven",
                },
            },
        );
    }
    engine.drain_actions().collect()
}

#[test]
fn registered_coinbase_hook_emits_once_for_engine_seals_and_late_replies() {
    for frame_fed in [false, true] {
        for split in [false, true] {
            for status in [408, 429, 503] {
                let (mut engine, attempt) = released(frame_fed);
                let actions = status_reply(&mut engine, attempt, status, split);
                assert_eq!(failure(&actions).outcome, Some(Outcome::Unknown));
                let logs = events(&actions);
                assert_eq!(logs.len(), 1);
                unknown_fields::assert_field_rules(logs[0]);
                assert!(logs[0].fields.contains(&("command", "coinbase.7".into())));
                assert!(
                    logs[0]
                        .fields
                        .contains(&("request", attempt.0.to_string().into()))
                );
                assert!(
                    logs[0]
                        .fields
                        .contains(&("status", u32::from(status).into()))
                );
                let late = status_reply(&mut engine, attempt, 200, false);
                assert!(events(&late).is_empty());
                assert!(!late.iter().any(|a| matches!(a, Action::Http(_))));
            }
        }
    }
}

#[test]
fn registered_coinbase_hook_omits_status_for_engine_transport_unknown() {
    let (mut engine, attempt) = released(false);
    engine.handle(
        6.,
        Event::HttpFailed {
            request: attempt,
            error: "https://fixture.invalid/orders?sig=body",
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(failure(&actions).outcome, Some(Outcome::Unknown));
    let logs = events(&actions);
    assert_eq!(logs.len(), 1);
    unknown_fields::assert_field_rules(logs[0]);
    assert!(logs[0].fields.iter().all(|(key, _)| *key != "status"));
}
