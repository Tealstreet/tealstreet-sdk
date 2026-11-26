mod support;

use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{
    Action, Command, EngineUnknownOutcome, Event, HttpRequest, Level, LogEvent, Method, Notice,
    Outcome, RequestId, Response, SocketId, UNKNOWN_OUTCOME_TEXT, Value, Venue, VenueError,
};
use safe_cex_venue_paradex::{Options, Paradex};
use support::*;

fn account(frame_fed: bool) -> Harness {
    let mut e = engine(
        Options {
            paradex_address: Some("0x0123".into()),
            subkey_address: Some("0x456".into()),
            ..Options::default()
        },
        Some(STARK_KEY),
    );
    if frame_fed {
        e.set_frame_fed().unwrap();
    }
    let now = 1_700_000_000_000.;
    e.start(now);
    let mut h = Harness {
        e,
        pending: vec![],
        actions: vec![],
        failures: vec![],
        receipt_trace: vec![],
        public: SocketId(0),
        now,
    };
    h.collect();
    h.reply("/markets", MARKETS);
    if !frame_fed {
        h.reply("summary", TICKERS);
    }
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
    if !frame_fed {
        h.reply("/account/margin", r#"{"configs":[]}"#);
        h.reply("/positions", r#"{"results":[]}"#);
        h.reply(
            "/account",
            r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
        );
        h.reply("/orders", r#"{"results":[]}"#);
        h.reply("/account/margin", r#"{"configs":[]}"#);
    }
    h.actions.clear();
    h
}

fn placement() -> Command {
    Command::PlaceOrders { command: 91, orders: vec![serde_json::from_str(
        r#"{"symbol":"BTC-USD-PERP","type":"limit","side":"buy","amount":1,"price":100,"clientOrderId":"caller"}"#
    ).unwrap()] }
}

fn place(h: &mut Harness) -> HttpRequest {
    h.command(placement());
    let writes: Vec<_> = h
        .pending
        .iter()
        .filter(|r| r.method != Method::Get)
        .collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].method, Method::Post);
    assert!(writes[0].url.ends_with("/orders/batch"));
    let payload: serde_json::Value =
        serde_json::from_str(writes[0].body.as_ref().unwrap()).unwrap();
    assert_eq!(payload[0]["client_id"], "caller");
    h.take("/orders/batch")
}

fn events(h: &Harness) -> Vec<&LogEvent> {
    h.actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .collect()
}

fn text<'a>(event: &'a LogEvent, key: &str) -> &'a str {
    event
        .fields
        .iter()
        .find(|(name, _)| *name == key)
        .unwrap()
        .1
        .as_str()
        .unwrap()
}

fn assert_schema(event: &LogEvent, request: RequestId, status: Option<u16>) {
    assert_eq!(event.level, Level::Warn);
    assert_eq!(event.message, UNKNOWN_OUTCOME_TEXT);
    let mut keys: Vec<_> = event.fields.iter().map(|(key, _)| *key).collect();
    keys.sort_unstable();
    let mut expected = vec![
        "command",
        "outcome",
        "placedOrderIds",
        "request",
        "unknownIndices",
    ];
    if status.is_some() {
        expected.push("status");
    }
    expected.sort_unstable();
    assert_eq!(keys, expected, "NX #9262 admits exactly these fields");
    let id = text(event, "request");
    // Exact anchored request grammar: ^[0-9]{1,20}$, never a signed URL.
    assert!((1..=20).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_digit()));
    assert_eq!(id, request.0.to_string());
    let command = text(event, "command");
    assert!((1..=64).contains(&command.len()));
    assert!(command.as_bytes()[0].is_ascii_alphabetic());
    assert!(
        command
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    );
    assert_eq!(text(event, "outcome"), "unknown");
    for key in ["placedOrderIds", "unknownIndices"] {
        let list = text(event, key);
        let entries: Vec<_> = if list.is_empty() {
            vec![]
        } else {
            list.split(',').collect()
        };
        assert!(entries.len() <= 16);
        assert!(list.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
        for entry in entries {
            let bound = if key == "placedOrderIds" { 32 } else { 5 };
            assert!(
                (1..=bound).contains(&entry.len()) && entry.bytes().all(|b| b.is_ascii_digit())
            );
            if key == "unknownIndices" {
                assert!(entry.parse::<u16>().is_ok());
            }
        }
    }
    if let Some(status) = status {
        assert!((100..=599).contains(&status));
        assert!(
            event
                .fields
                .contains(&("status", Value::Num(f64::from(status))))
        );
    }
    // REQUIRED PIN: inspect every field, including hostile error-derived data.
    for (_, value) in &event.fields {
        if let Some(value) = value.as_str() {
            for forbidden in [
                "http",
                "wss",
                "://",
                "/",
                "?",
                "=",
                "&",
                "{",
                "}",
                "signature",
                "payload",
                "SYNTHETIC",
                "orders",
                "auth",
                "query",
                "body",
            ] {
                assert!(
                    !value
                        .to_ascii_lowercase()
                        .contains(&forbidden.to_ascii_lowercase()),
                    "unsafe field: {value}"
                );
            }
        }
    }
}

fn assert_unknown(h: &Harness, request: RequestId, status: Option<u16>) {
    let failures: Vec<_> = h
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 91, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].outcome, Some(Outcome::Unknown));
    // Master composes the guidance around the unconfirmed placements.
    assert!(
        failures[0]
            .message
            .starts_with("We couldn’t confirm this change")
            && failures[0]
                .message
                .ends_with("Check your open orders before trying again."),
        "{}",
        failures[0].message
    );
    let events = events(h);
    assert_eq!(
        events.len(),
        1,
        "exactly one Unknown event: {:?}",
        h.actions
    );
    assert_schema(events[0], request, status);
    assert!(
        h.pending.iter().all(|r| r.method == Method::Get),
        "no retry/continuation"
    );
}

#[test]
fn legacy_cancel_unknown_emits_once_for_each_cancel_endpoint() {
    for frame_fed in [false, true] {
        for operation in ["single", "batch", "symbol"] {
            for failure in ["http", "transport", "integrity"] {
                let mut h = account(frame_fed);
                h.command(if operation == "symbol" {
                    Command::CancelSymbolOrders {
                        command: 91,
                        symbol: "BTC-USD-PERP".into(),
                    }
                } else {
                    Command::CancelOrders {
                        command: 91,
                        ids: if operation == "batch" {
                            vec!["123".into(), "456".into()]
                        } else {
                            vec!["123".into()]
                        },
                    }
                });
                let request = h.take("/orders");
                assert_eq!(request.method, Method::Delete);
                let id = request.id;
                match failure {
                    "http" => h.respond(request.clone(), 503,
                        r#"{"message":"https://venue.test/orders?signature=SYNTHETIC","body":"signed-payload"}"#),
                    "transport" => {
                        h.e.handle(h.now, Event::HttpFailed { request: id, error: "https://venue.test/orders?signature=SYNTHETIC" });
                        h.collect();
                    }
                    _ => {
                        h.e.handle(h.now, Event::HttpStart { request: id, status: 200, headers: "" });
                        h.e.handle(h.now, Event::HttpChunk { request: id, data: &[0xff] });
                        h.e.handle(h.now, Event::HttpEnd { request: id });
                        h.collect();
                    }
                }
                assert_unknown(&h, id, (failure == "http").then_some(503));
                assert_eq!(text(events(&h)[0], "placedOrderIds"), "");
                assert_eq!(text(events(&h)[0], "unknownIndices"), "");
                h.respond(request, 204, "");
                h.e.handle(
                    h.now,
                    Event::HttpFailed {
                        request: id,
                        error: "duplicate",
                    },
                );
                h.collect();
                assert_unknown(&h, id, (failure == "http").then_some(503));
            }
        }
    }
}

#[test]
fn legacy_cancel_refusal_and_success_emit_no_unknown() {
    for frame_fed in [false, true] {
        for status in [204, 400] {
            let mut h = account(frame_fed);
            h.command(Command::CancelOrders {
                command: 91,
                ids: vec!["123".into()],
            });
            let request = h.take("/orders/123");
            let id = request.id;
            h.respond(
                request,
                status,
                if status == 400 {
                    r#"{"error":"INVALID_PRICE"}"#
                } else {
                    ""
                },
            );
            if status == 400 {
                assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::Rejected));
            } else {
                assert_eq!(h.result(91), Ok("[]"));
            }
            h.e.handle(
                h.now,
                Event::HttpFailed {
                    request: id,
                    error: "late",
                },
            );
            h.collect();
            assert!(events(&h).is_empty());
        }
    }
}

#[test]
fn legacy_http_unknown_emits_once_in_ordinary_and_frame_fed_modes() {
    for frame_fed in [false, true] {
        for status in [408, 429, 503] {
            for streamed in [false, true] {
                let mut h = account(frame_fed);
                let request = place(&mut h);
                let id = request.id;
                let body = r#"{"message":"https://venue.test/orders?signature=SYNTHETIC","body":"signed-payload"}"#;
                if streamed {
                    h.e.handle(
                        h.now,
                        Event::HttpStart {
                            request: id,
                            status,
                            headers: "",
                        },
                    );
                    h.e.handle(
                        h.now,
                        Event::HttpChunk {
                            request: id,
                            data: body.as_bytes(),
                        },
                    );
                    h.e.handle(h.now, Event::HttpEnd { request: id });
                    h.collect();
                } else {
                    h.respond(request, status, body);
                }
                assert_unknown(&h, id, Some(status));
                h.e.handle(
                    h.now,
                    Event::HttpFailed {
                        request: id,
                        error: "late duplicate",
                    },
                );
                h.e.handle(
                    h.now,
                    Event::HttpResponse {
                        request: id,
                        response: Response {
                            status: 200,
                            headers: "",
                            body: "{}",
                        },
                    },
                );
                h.collect();
                assert_unknown(&h, id, Some(status));
            }
        }
    }
}

#[test]
fn legacy_transport_and_integrity_unknown_emit_once_without_duplicate() {
    for frame_fed in [false, true] {
        for integrity in [false, true] {
            let mut h = account(frame_fed);
            let request = place(&mut h);
            if integrity {
                h.e.handle(
                    h.now,
                    Event::HttpStart {
                        request: request.id,
                        status: 200,
                        headers: "",
                    },
                );
                h.e.handle(
                    h.now,
                    Event::HttpChunk {
                        request: request.id,
                        data: &[0xff],
                    },
                );
                h.e.handle(
                    h.now,
                    Event::HttpEnd {
                        request: request.id,
                    },
                );
            } else {
                h.e.handle(
                    h.now,
                    Event::HttpFailed {
                        request: request.id,
                        error: "lost https://venue.test/orders?signature=SYNTHETIC",
                    },
                );
            }
            h.collect();
            assert_unknown(&h, request.id, None);
            h.e.handle(
                h.now,
                Event::HttpFailed {
                    request: request.id,
                    error: "duplicate",
                },
            );
            h.collect();
            assert_unknown(&h, request.id, None);
        }
    }
}

#[test]
fn native_unknown_stays_once_and_refusal_and_success_emit_none() {
    for frame_fed in [false, true] {
        for (status, body, unknown) in [
            (200, "{}", true),
            (
                400,
                r#"{"error":"INVALID_PRICE","message":"refused"}"#,
                false,
            ),
            (
                200,
                r#"{"orders":[{"id":"123","client_id":"caller"}]}"#,
                false,
            ),
        ] {
            let mut h = account(frame_fed);
            let request = place(&mut h);
            let id = request.id;
            h.respond(request, status, body);
            if unknown {
                assert_unknown(&h, id, Some(status));
                assert_eq!(text(events(&h)[0], "unknownIndices"), "0");
            } else {
                assert!(events(&h).is_empty());
                if status == 400 {
                    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::Rejected));
                } else {
                    assert_eq!(h.result(91), Ok("[\"123\"]"));
                }
            }
        }
    }
}

fn projection(request: RequestId, status: Option<u16>, error: &VenueError) -> LogEvent {
    Paradex::unknown_outcome_telemetry().expect("Paradex opts into engine sealing")(
        &EngineUnknownOutcome {
            now: f64::NAN,
            command: u32::MAX,
            request,
            status,
            error,
        },
    )
}

#[test]
fn projection_pins_request_grammar_status_bounds_and_hostile_field_exclusion() {
    let hostile = "https://venue.test/orders?api_key=SYNTHETIC&signature=signed-payload";
    let error = VenueError::new(hostile)
        .venue_code(hostile)
        .venue_message(hostile)
        .partial_ids(&[hostile.into(), "123".into(), "4".repeat(33), "0x123".into()]);
    for request in [RequestId(0), RequestId(1), RequestId(u64::MAX)] {
        for status in [
            None,
            Some(0),
            Some(99),
            Some(100),
            Some(200),
            Some(599),
            Some(600),
            Some(u16::MAX),
        ] {
            let event = projection(request, status, &error);
            assert_schema(&event, request, status.filter(|s| (100..=599).contains(s)));
            assert_eq!(text(&event, "command"), "paradex.4294967295");
            assert_eq!(text(&event, "placedOrderIds"), "123");
            assert_eq!(text(&event, "unknownIndices"), "");
        }
    }
}

#[test]
fn projection_caps_lists_and_indices_without_cutting_native_ids() {
    let mut error = VenueError::new("untrusted")
        .partial_ids(&(0..20).map(|i| i.to_string()).collect::<Vec<_>>());
    error.elements = Some(Box::new(ElementOutcomes(
        (0..20)
            .chain([65535, 65536, usize::MAX])
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: Some("https://venue.test?signature=SYNTHETIC".into()),
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(VenueError::new("signed-payload")),
                },
            })
            .collect(),
    )));
    let event = projection(RequestId(7), None, &error);
    assert_schema(&event, RequestId(7), None);
    let expected = (0..16).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    assert_eq!(text(&event, "placedOrderIds"), expected);
    assert_eq!(text(&event, "unknownIndices"), expected);
    error.elements.as_mut().unwrap().0.drain(..20);
    let event = projection(RequestId(7), None, &error);
    assert_schema(&event, RequestId(7), None);
    assert_eq!(text(&event, "unknownIndices"), "65535");
    error.partial.as_mut().unwrap().order_ids = vec!["9".repeat(32); 17];
    let event = projection(RequestId(7), None, &error);
    assert_schema(&event, RequestId(7), None);
    assert_eq!(text(&event, "placedOrderIds").split(',').count(), 15);
    assert!(
        text(&event, "placedOrderIds")
            .split(',')
            .all(|id| id.len() == 32)
    );
    error.partial = None;
    error.elements = None;
    let event = projection(RequestId(7), None, &error);
    assert_eq!(text(&event, "placedOrderIds"), "");
    assert_eq!(text(&event, "unknownIndices"), "");
}

// Exercise Paradex's registered projection at a real managed seal without
// migrating its legacy trading dispatch to a different request lifecycle.
struct ManagedProbe;
impl Venue for ManagedProbe {
    type Pending = safe_cex_engine::request_context::Context;
    fn unknown_outcome_telemetry() -> Option<safe_cex_engine::UnknownOutcomeTelemetry> {
        Paradex::unknown_outcome_telemetry()
    }
    fn start(&mut self, _: &mut safe_cex_engine::Cx<'_, Self::Pending>) {}
    fn on_open(&mut self, _: &mut safe_cex_engine::Cx<'_, Self::Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut safe_cex_engine::Cx<'_, Self::Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut safe_cex_engine::Cx<'_, Self::Pending>, _: SocketId, _: &str) {
    }
    fn on_timer(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        _: safe_cex_engine::TimerId,
    ) {
    }
    fn on_request_failed(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        _: Self::Pending,
        _: &str,
    ) {
        panic!("managed failure uses typed callback")
    }
    fn on_http_failed(
        &mut self,
        cx: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        context: Self::Pending,
        error: VenueError,
    ) {
        cx.fail_context(&context, error).unwrap();
    }
    fn on_response(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, Self::Pending>,
        _: Self::Pending,
        _: Response<'_>,
    ) {
        panic!("engine-sealed Unknown must not run the native decoder")
    }
    fn capture_http_reconciliation(
        &self,
        _: &Self::Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        safe_cex_engine::request_context::ReconciliationData::new("probe".into())
    }
    fn on_command(&mut self, cx: &mut safe_cex_engine::Cx<'_, Self::Pending>, _: Command) {
        use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
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
            "https://venue.test/orders?api_key=SYNTHETIC&signature=payload".into(),
            Some("{\"signed-payload\":\"SYNTHETIC\"}".into()),
            context,
        )
        .unwrap();
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn registered_paradex_hook_emits_once_for_engine_sealed_http_transport_and_integrity() {
    use safe_cex_engine::test_support::{prepared_actions, seeded_engine};
    for frame_fed in [false, true] {
        for mode in ["408", "429", "503", "transport", "integrity"] {
            let mut e = seeded_engine(ManagedProbe);
            e.store_mut()
                .set_markets(account(frame_fed).e.store().markets().to_vec());
            if frame_fed {
                e.set_frame_fed().unwrap();
            }
            e.start(1.);
            e.drain_actions().for_each(drop);
            e.handle(
                2.,
                Event::Command(Command::PlaceOrders {
                    command: 91,
                    orders: vec![],
                }),
            );
            let requests: Vec<_> = prepared_actions(&mut e, 2.)
                .into_iter()
                .filter_map(|a| match a {
                    Action::Http(request) => Some(request),
                    _ => None,
                })
                .collect();
            assert_eq!(requests.len(), 1);
            let id = requests[0].id;
            let status = mode.parse::<u16>().ok();
            if let Some(status) = status {
                e.handle(
                    3.,
                    Event::HttpResponse {
                        request: id,
                        response: Response {
                            status,
                            headers: "",
                            body: "signed-payload",
                        },
                    },
                );
            } else if mode == "transport" {
                e.handle(
                    3.,
                    Event::HttpFailed {
                        request: id,
                        error: "lost https://venue.test?signature=SYNTHETIC",
                    },
                );
            } else {
                e.handle(
                    3.,
                    Event::HttpStart {
                        request: id,
                        status: 200,
                        headers: "",
                    },
                );
                e.handle(
                    3.,
                    Event::HttpChunk {
                        request: id,
                        data: &[0xff],
                    },
                );
                e.handle(3., Event::HttpEnd { request: id });
            }
            let mut actions = prepared_actions(&mut e, 3.);
            e.handle(
                4.,
                Event::HttpFailed {
                    request: id,
                    error: "duplicate",
                },
            );
            e.handle(
                4.,
                Event::HttpResponse {
                    request: id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "{}",
                    },
                },
            );
            actions.extend(prepared_actions(&mut e, 4.));
            let events: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::Event(event))
                        if event.code == "order.outcome_unknown" =>
                    {
                        Some(event)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(events.len(), 1, "{frame_fed}/{mode}: {actions:?}");
            assert_schema(events[0], id, status);
            assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 91, error }) if error.outcome == Some(Outcome::Unknown))).count(), 1);
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
            );
        }
    }
}

#[test]
fn native_definite_refusal_control_emits_no_unknown_event() {
    let mut h = account(false);
    let request = place(&mut h);
    h.respond(
        request,
        400,
        r#"{"error":"INVALID_PRICE","message":"refused"}"#,
    );
    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::Rejected));
    assert!(events(&h).is_empty());
}

#[test]
fn two_accounts_waiting_on_jwt_keep_each_requests_correlation() {
    for frame_fed in [false, true] {
        let mut first = account(frame_fed);
        let mut second = account(frame_fed);
        for h in [&mut first, &mut second] {
            h.now += 181_000.;
            h.command(placement());
            assert!(!h.pending.iter().any(|r| r.url.ends_with("/orders/batch")));
            assert!(events(h).is_empty());
        }
        // Complete the other account first while the original request is deferred.
        let auth = second.take("/auth/");
        let auth_id = auth.id;
        second.respond(auth, 200, r#"{"jwt_token":"second"}"#);
        let write = second.take("/orders/batch");
        let id = write.id;
        assert_ne!(id, auth_id);
        second.respond(write, 200, "{}");
        assert_unknown(&second, id, Some(200));
        assert!(events(&first).is_empty());
        let auth = first.take("/auth/");
        let auth_id = auth.id;
        first.respond(auth, 200, r#"{"jwt_token":"first"}"#);
        let write = first.take("/orders/batch");
        let id = write.id;
        assert_ne!(id, auth_id);
        first.respond(write, 200, "{}");
        assert_unknown(&first, id, Some(200));
        assert_eq!(events(&second).len(), 1);
    }
}
