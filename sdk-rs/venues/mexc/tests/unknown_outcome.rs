mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Command, Cx, Event, LogEvent, Method, Notice, Outcome, RequestId, Response, SocketId,
    TimerId, UnknownOutcomeTelemetry, Venue,
};
use safe_cex_venue_mexc::MexcFutures;

fn events(actions: &[Action]) -> Vec<&LogEvent> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(e)) if e.code == "order.outcome_unknown" => Some(e),
            _ => None,
        })
        .collect()
}

fn check(actions: &[Action], expected: usize, request: RequestId) {
    let events = events(actions);
    assert_eq!(events.len(), expected);
    for event in events {
        assert_eq!(event.message, safe_cex_engine::UNKNOWN_OUTCOME_TEXT);
        let id = event
            .fields
            .iter()
            .find(|(k, _)| *k == "request")
            .unwrap()
            .1
            .as_str()
            .unwrap();
        assert_eq!(id, request.0.to_string());
        // Exactly ^[0-9]{1,20}$, including the u64 boundary in projection tests.
        assert!((1..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()));
        for (key, value) in &event.fields {
            assert!(
                [
                    "command",
                    "request",
                    "outcome",
                    "placedOrderIds",
                    "unknownIndices",
                    "status"
                ]
                .contains(key)
            );
            let text = format!("{key} {value:?}").to_lowercase();
            for forbidden in [
                "http",
                "/sendorder",
                "?",
                "&",
                "=",
                "signature",
                "apikey",
                "authent",
                "signed-payload",
                "never-log",
                "endpoint",
                "query",
                "body",
            ] {
                assert!(!text.contains(forbidden), "{text}");
            }
        }
    }
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
    );
}

// Exercise MEXC's registered static projection with an engine-owned mutation;
// MEXC's account-state protective lane is covered separately in trading.rs.
struct ManagedProbe;
impl Venue for ManagedProbe {
    type Pending = Context;
    fn unknown_outcome_telemetry() -> Option<UnknownOutcomeTelemetry> {
        MexcFutures::unknown_outcome_telemetry()
    }
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        safe_cex_engine::request_context::ReconciliationData::new("mexc-telemetry-probe".into())
    }
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
        panic!("engine seals proof failure");
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        context: Context,
        error: safe_cex_engine::VenueError,
    ) {
        cx.fail_context(&context, error).unwrap();
    }
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {
        panic!("engine seals proof failure");
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
            "https://never-log.test/sendorder?signature=signed-payload".into(),
            Some("body=never-log".into()),
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

#[test]
fn engine_sealed_unknown_is_once_in_whole_streamed_and_frame_fed_modes() {
    for frame_fed in [false, true] {
        for streamed in [false, true] {
            for status in [408, 429, 503] {
                let mut engine = safe_cex_engine::test_support::seeded_engine(ManagedProbe);
                if frame_fed {
                    engine.set_frame_fed().unwrap();
                }
                let (source, _) = support::started();
                engine
                    .store_mut()
                    .set_markets(source.store().markets().to_vec());
                engine.start(0.);
                engine.drain_actions().for_each(drop);
                engine.handle(
                    1.,
                    Event::Command(Command::PlaceOrders {
                        command: 7,
                        orders: vec![],
                    }),
                );
                let requests: Vec<_> =
                    safe_cex_engine::test_support::prepared_actions(&mut engine, 2.)
                        .into_iter()
                        .filter_map(|a| match a {
                            Action::Http(r) => Some(r),
                            _ => None,
                        })
                        .collect();
                assert_eq!(requests.len(), 1);
                let request = &requests[0];
                let response = Response {
                    status,
                    headers: "",
                    body: "https://never-log.test/?signature=signed-payload",
                };
                if streamed {
                    engine.handle(
                        3.,
                        Event::HttpStart {
                            request: request.id,
                            status,
                            headers: "",
                        },
                    );
                    engine.handle(
                        4.,
                        Event::HttpChunk {
                            request: request.id,
                            data: response.body.as_bytes(),
                        },
                    );
                    engine.handle(
                        5.,
                        Event::HttpEnd {
                            request: request.id,
                        },
                    );
                } else {
                    engine.handle(
                        3.,
                        Event::HttpResponse {
                            request: request.id,
                            response,
                        },
                    );
                }
                let mut actions: Vec<_> = engine.drain_actions().collect();
                engine.handle(
                    6.,
                    Event::HttpResponse {
                        request: request.id,
                        response,
                    },
                );
                actions.extend(engine.drain_actions());
                check(&actions, 1, request.id);
                assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(Outcome::Unknown))).count(), 1);
            }
        }
    }
}
