mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Command, Cx, Event, LogEvent, Method, Notice, Outcome, RequestId, Response, SocketId,
    TimerId, UnknownOutcomeTelemetry, Venue,
};
use safe_cex_venue_kraken::Kraken;
use support::{NOW, Plain, ready};

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

#[test]
fn native_unknown_is_once_and_definite_refusal_is_silent() {
    for (status, body, count, outcome) in [
        (
            200,
            r#"{"result":"success","sendStatus":{"status":"futureStatus"}}"#,
            1,
            Outcome::Unknown,
        ),
        (
            503,
            "https://never-log.test/sendorder?signature=signed-payload",
            1,
            Outcome::Unknown,
        ),
        (
            200,
            r#"{"result":"success","sendStatus":{"status":"invalidSize"}}"#,
            0,
            Outcome::Rejected,
        ),
    ] {
        let (mut engine, _) = ready();
        engine.handle(NOW, Event::Command(Command::PlaceOrders {
            command: 7,
            orders: vec![serde_json::from_str(r#"{"symbol":"PF_XBTUSD","type":"limit","side":"buy","amount":0.123,"price":100}"#).unwrap()],
        }));
        let requests = support::requests(&mut engine);
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert!(request.url.ends_with("sendorder"));
        support::reply(&mut engine, request, status, body);
        let mut actions: Vec<_> = engine.drain_plain().collect();
        support::reply(&mut engine, request, status, body);
        actions.extend(engine.drain_plain());
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(outcome))).count(), 1);
        check(&actions, count, request.id);
    }
}

#[test]
fn native_partial_unknown_keeps_proven_ids_and_transport_omits_prior_status() {
    for transport in [false, true] {
        let (mut engine, _) = ready();
        let order = serde_json::from_str(
            r#"{"symbol":"PF_XBTUSD","type":"limit","side":"buy","amount":0.123,"price":100}"#,
        )
        .unwrap();
        engine.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 7,
                orders: vec![order; 3],
            }),
        );
        let first = support::requests(&mut engine);
        assert_eq!(first.len(), 1);
        support::reply(
            &mut engine,
            &first[0],
            200,
            r#"{"result":"success","sendStatus":{"status":"placed","order_id":"123"}}"#,
        );
        let second = support::requests(&mut engine);
        assert_eq!(second.len(), 1);
        if transport {
            engine.handle(
                NOW,
                Event::HttpFailed {
                    request: second[0].id,
                    error: "https://never-log.test/sendorder?signature=signed-payload",
                },
            );
        } else {
            support::reply(
                &mut engine,
                &second[0],
                200,
                r#"{"result":"success","sendStatus":{"status":"futureStatus"}}"#,
            );
        }
        let actions: Vec<_> = engine.drain_plain().collect();
        check(&actions, 1, second[0].id);
        let emitted = events(&actions);
        assert!(
            emitted[0]
                .fields
                .contains(&("placedOrderIds", "123".into()))
        );
        assert!(emitted[0].fields.contains(&("unknownIndices", "".into())));
        assert_eq!(
            emitted[0].fields.iter().any(|(key, _)| *key == "status"),
            !transport
        );
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert!(
            error
                .partial
                .as_ref()
                .unwrap()
                .order_ids
                .contains(&"123".to_owned())
        );
    }
}

// Exercise Kraken's registered static projection with an engine-owned mutation;
// its current Futures trading lane still uses legacy request delivery.
struct ManagedProbe;
impl Venue for ManagedProbe {
    type Pending = Context;
    fn unknown_outcome_telemetry() -> Option<UnknownOutcomeTelemetry> {
        Kraken::unknown_outcome_telemetry()
    }
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        safe_cex_engine::request_context::ReconciliationData::new("kraken-telemetry-probe".into())
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
                let (source, _) = ready();
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

#[test]
fn engine_transport_unknown_has_no_status_or_duplicate_event() {
    for frame_fed in [false, true] {
        let mut engine = safe_cex_engine::test_support::seeded_engine(ManagedProbe);
        if frame_fed {
            engine.set_frame_fed().unwrap();
        }
        let (source, _) = ready();
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
        let requests: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut engine, 2.)
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1);
        let request = requests[0].id;
        for _ in 0..2 {
            engine.handle(
                3.,
                Event::HttpFailed {
                    request,
                    error: "https://never-log.test/?signature=signed-payload",
                },
            );
        }
        let actions: Vec<_> = engine.drain_actions().collect();
        check(&actions, 1, request);
        assert!(
            !events(&actions)[0]
                .fields
                .iter()
                .any(|(key, _)| *key == "status")
        );
    }
}
