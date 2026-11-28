use safe_cex_engine::{Action, Notice, Outcome, VenueError};
use safe_cex_venue_txflow::TxFlow;
#[path = "support/unknown_outcome.rs"]
mod support;
use support::{assert_unknown_event, unknown_events};

#[test]
fn engine_sealing_uses_txflow_hook_once_without_changing_the_failure() {
    for frame_fed in [false, true] {
        for streamed in [false, true] {
            for status in [408, 429, 503] {
                let (mut engine, request) = released::<true>(frame_fed);
                let actions = status_reply(&mut engine, request, status, streamed);
                assert_unknown_event(&actions, Some(request), 7);
                assert!(
                    unknown_events(&actions)[0]
                        .fields
                        .contains(&("status", u32::from(status).into()))
                );
                let error = sealed_error(&actions);
                let (mut control, control_request) = released::<false>(frame_fed);
                let control_actions = status_reply(&mut control, control_request, status, streamed);
                assert!(unknown_events(&control_actions).is_empty());
                assert_eq!(
                    serde_json::to_value(error).unwrap(),
                    serde_json::to_value(sealed_error(&control_actions)).unwrap()
                );
                assert!(
                    unknown_events(&status_reply(&mut engine, request, status, streamed))
                        .is_empty()
                );
            }
        }
        let (mut engine, request) = released::<true>(frame_fed);
        engine.handle(
            3.,
            Event::HttpFailed {
                request,
                error: "hostile https://venue.test/exchange?signature=payload body={}",
            },
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_unknown_event(&actions, Some(request), 7);
        assert_eq!(sealed_error(&actions).outcome, Some(Outcome::Unknown));
        assert!(
            !unknown_events(&actions)[0]
                .fields
                .iter()
                .any(|(key, _)| *key == "status")
        );
        engine.handle(
            4.,
            Event::HttpFailed {
                request,
                error: "duplicate",
            },
        );
        assert!(unknown_events(&engine.drain_actions().collect::<Vec<_>>()).is_empty());
    }
}

fn sealed_error(actions: &[Action]) -> &VenueError {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(Outcome::Unknown));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. })
    )));
    errors[0]
}

fn status_reply(
    engine: &mut Engine<impl Venue>,
    request: RequestId,
    status: u16,
    streamed: bool,
) -> Vec<Action> {
    if streamed {
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
                data: b"hostile?signature=payload body={}",
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
                    body: "hostile?signature=payload body={}",
                },
            },
        );
    }
    engine.drain_actions().collect()
}

#[test]
fn unknown_hook_nx_schema_caps_lists_and_omits_invalid_status() {
    use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
    let callback = <TxFlow as safe_cex_engine::Venue>::unknown_outcome_telemetry().unwrap();
    let mut error = VenueError::new("hostile https://venue.test?signature=payload body={}")
        .outcome(Outcome::Unknown);
    let ids = [
        "".into(),
        "https://venue.test?signature=payload".into(),
        "1".repeat(33),
        "7".repeat(32),
    ]
    .into_iter()
    .chain((0..20).map(|i| i.to_string()))
    .collect();
    error.partial = Some(Box::new(safe_cex_engine::Partial {
        order_ids: ids,
        total: None,
        failed_index: None,
    }));
    error.elements = Some(Box::new(ElementOutcomes(
        [65_536, 65_535]
            .into_iter()
            .chain(0..20)
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: Some("hostile?signature=payload".into()),
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(VenueError::new("hostile body={}")),
                },
            })
            .collect(),
    )));
    for status in [
        None,
        Some(0),
        Some(99),
        Some(100),
        Some(599),
        Some(600),
        Some(u16::MAX),
    ] {
        let event = callback(&safe_cex_engine::EngineUnknownOutcome {
            now: 0.,
            command: u32::MAX,
            request: safe_cex_engine::RequestId(u64::MAX),
            status,
            error: &error,
        });
        assert_unknown_event(
            &[Action::Notice(Notice::Event(event.clone()))],
            Some(safe_cex_engine::RequestId(u64::MAX)),
            u32::MAX,
        );
        let field = |name| &event.fields.iter().find(|(key, _)| *key == name).unwrap().1;
        let indices = field("unknownIndices")
            .as_str()
            .unwrap()
            .split(',')
            .collect::<Vec<_>>();
        assert_eq!(indices.len(), 16, "NX list cap");
        assert_eq!(indices[0], "65535");
        assert_eq!(indices[15], "14");
        let ids = field("placedOrderIds")
            .as_str()
            .unwrap()
            .split(',')
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 16, "NX list cap");
        assert_eq!(ids[0], "7".repeat(32));
        assert_eq!(ids[15], "14");
        let observed_status = event
            .fields
            .iter()
            .find(|(key, _)| *key == "status")
            .map(|(_, value)| value.clone());
        assert_eq!(
            observed_status,
            status
                .filter(|s| (100..=599).contains(s))
                .map(|s| u32::from(s).into())
        );
    }
    error.partial.as_mut().unwrap().order_ids = vec!["7".repeat(32); 20];
    let event = callback(&safe_cex_engine::EngineUnknownOutcome {
        now: 0.,
        command: u32::MAX,
        request: safe_cex_engine::RequestId(0),
        status: None,
        error: &error,
    });
    let ids = event
        .fields
        .iter()
        .find(|(key, _)| *key == "placedOrderIds")
        .unwrap()
        .1
        .as_str()
        .unwrap();
    assert!(ids.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
    assert_eq!(
        ids.split(',').count(),
        15,
        "full-width IDs must not hit log truncation"
    );
    assert_unknown_event(
        &[Action::Notice(Notice::Event(event))],
        Some(safe_cex_engine::RequestId(0)),
        u32::MAX,
    );
}

// TxFlow's production writes remain legacy. This fixture exercises its registered
// static projection through real managed engine sealing without changing those writes.
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Context, Operation, Policy, ReconciliationData, RetryPolicy,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Command, Cx, Engine, Event, Method, RequestId, Response, SocketId, TimerId,
    UnknownOutcomeTelemetry, Venue,
};
struct HookHarness<const REGISTERED: bool>;

impl<const REGISTERED: bool> Venue for HookHarness<REGISTERED> {
    type Pending = Context;
    fn unknown_outcome_telemetry() -> Option<UnknownOutcomeTelemetry> {
        if REGISTERED {
            <TxFlow as Venue>::unknown_outcome_telemetry()
        } else {
            None
        }
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

fn released<const REGISTERED: bool>(
    frame_fed: bool,
) -> (Engine<HookHarness<REGISTERED>>, RequestId) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(HookHarness::<REGISTERED>);
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
