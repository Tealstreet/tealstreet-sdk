use super::*;
use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{
    Action, Command, Engine, EngineUnknownOutcome, Event, HttpRequest, Level, LogEvent, Method,
    Notice, Outcome, RequestId, Response, UNKNOWN_OUTCOME_TEXT, Value, Venue, VenueError,
};
use safe_cex_venue_okx::Okx;

fn field<'a>(event: &'a LogEvent, key: &str) -> Option<&'a Value> {
    event
        .fields
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value)
}

pub(super) fn nx_schema(event: &LogEvent, status: Option<u16>) {
    assert_eq!(event.code, "order.outcome_unknown");
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
    assert_eq!(keys, expected, "closed NX #9262 field set");
    let request = field(event, "request").unwrap().as_str().unwrap();
    // Exact ^[0-9]{1,20}$ contract, including both anchors.
    assert!((1..=20).contains(&request.len()) && request.bytes().all(|b| b.is_ascii_digit()));
    let command = field(event, "command").unwrap().as_str().unwrap();
    assert!((1..=64).contains(&command.len()) && command.as_bytes()[0].is_ascii_alphabetic());
    assert!(
        command
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    );
    assert_eq!(field(event, "outcome").unwrap().as_str(), Some("unknown"));
    for (key, max_len) in [("placedOrderIds", 32), ("unknownIndices", 5)] {
        let value = field(event, key).unwrap().as_str().unwrap();
        let entries: Vec<_> = if value.is_empty() {
            vec![]
        } else {
            value.split(',').collect()
        };
        assert!(entries.len() <= 16);
        for entry in entries {
            assert!(
                (1..=max_len).contains(&entry.len()) && entry.bytes().all(|b| b.is_ascii_digit())
            );
            if key == "unknownIndices" {
                assert!(entry.parse::<u16>().is_ok());
            }
        }
    }
    assert_eq!(
        field(event, "status"),
        status.map(|s| Value::Num(s.into())).as_ref()
    );
    for (key, value) in &event.fields {
        let value = format!("{value:?}").to_ascii_lowercase();
        for forbidden in [
            "http",
            "/",
            "?",
            "=",
            "&",
            "{",
            "}",
            "[",
            "]",
            "body",
            "payload",
            "signature",
            "api_key",
            "sensitive-fixture",
        ] {
            assert!(!key.to_ascii_lowercase().contains(forbidden));
            assert!(!value.contains(forbidden), "unsafe field {key}");
        }
    }
}

fn events(out: &[Action]) -> Vec<&LogEvent> {
    out.iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .collect()
}

fn engine(frame_fed: bool) -> Engine<Okx> {
    let mut source = trading_engine();
    if !frame_fed {
        return source;
    }
    let mut page = support::engine(FixtureOptions {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        passphrase: Some("test-passphrase".into()),
        ..FixtureOptions::default()
    });
    page.set_frame_fed().unwrap();
    page.set_source_clock_offset(Some(0.));
    page.start(0.);
    let initial = prepared(&mut page, 0.);
    let instruments = initial
        .iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    reply(&mut page, instruments, &envelope(MARKET), 1.);
    prepared(&mut page, 1.);
    page.apply_frame(2., source.snapshot_frame(2.), true)
        .unwrap();
    page
}

fn released(frame_fed: bool) -> (Engine<Okx>, HttpRequest) {
    let mut engine = engine(frame_fed);
    engine.handle(5., Event::Command(Command::PlaceOrders {
        command: 7,
        orders: serde_json::from_str(r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000,"stopLoss":36000}]"#).unwrap(),
    }));
    let writes: Vec<_> = prepared(&mut engine, 5.)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(request) if request.method != Method::Get => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(writes.len(), 1);
    assert!(writes[0].url.ends_with("/api/v5/trade/batch-orders"));
    (engine, writes.into_iter().next().unwrap())
}

#[test]
fn engine_sealed_unknown_emits_once_in_whole_streamed_and_frame_fed_modes() {
    for frame_fed in [false, true] {
        for streamed in [false, true] {
            for status in [408, 429, 503] {
                let (mut engine, request) = released(frame_fed);
                let body = "https://sensitive-fixture.test/api/order?api_key=sensitive-fixture&signature=payload";
                if streamed {
                    engine.handle(
                        6.,
                        Event::HttpStart {
                            request: request.id,
                            status,
                            headers: "",
                        },
                    );
                    engine.handle(
                        6.,
                        Event::HttpChunk {
                            request: request.id,
                            data: body.as_bytes(),
                        },
                    );
                    engine.handle(
                        6.,
                        Event::HttpEnd {
                            request: request.id,
                        },
                    );
                } else {
                    engine.handle(
                        6.,
                        Event::HttpResponse {
                            request: request.id,
                            response: Response {
                                status,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                let mut out = prepared(&mut engine, 6.);
                let observed = events(&out);
                assert_eq!(
                    observed.len(),
                    1,
                    "status={status}, streamed={streamed}, frame_fed={frame_fed}"
                );
                nx_schema(observed[0], Some(status));
                assert_eq!(
                    field(observed[0], "request").unwrap().as_str(),
                    Some(request.id.0.to_string().as_str())
                );
                assert_eq!(
                    field(observed[0], "unknownIndices").unwrap().as_str(),
                    Some("0")
                );
                assert_eq!(
                    terminals(&out, 7)[0].as_ref().unwrap_err().outcome,
                    Some(Outcome::Unknown)
                );
                reply(
                    &mut engine,
                    &Action::Http(request),
                    &envelope(r#"{"ordId":"123","sCode":"0"}"#),
                    7.,
                );
                out.extend(prepared(&mut engine, 7.));
                out.extend(fire_armed(&mut engine, &out.clone(), 30_000.));
                assert_eq!(events(&out).len(), 1);
                assert!(
                    !out.iter()
                        .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
                );
            }
        }
    }
}

#[test]
fn released_transport_loss_emits_one_safe_event_without_status() {
    for frame_fed in [false, true] {
        let (mut engine, request) = released(frame_fed);
        engine.handle(
            6.,
            Event::HttpFailed {
                request: request.id,
                error: "sensitive-fixture payload signature",
            },
        );
        let mut out = prepared(&mut engine, 6.);
        let observed = events(&out);
        assert_eq!(observed.len(), 1);
        nx_schema(observed[0], None);
        reply(
            &mut engine,
            &Action::Http(request),
            &envelope(r#"{"ordId":"123","sCode":"0"}"#),
            7.,
        );
        out.extend(prepared(&mut engine, 7.));
        out.extend(fire_armed(&mut engine, &out.clone(), 30_000.));
        assert_eq!(events(&out).len(), 1);
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
        );
    }
}

#[test]
fn venue_decoded_unknown_stays_single_and_definite_refusal_emits_none() {
    for (code, count) in [("50004", 1), ("51412", 1), ("51008", 0)] {
        let (mut engine, request) = released(false);
        reply(
            &mut engine,
            &Action::Http(request),
            &envelope(&format!(r#"{{"sCode":"{code}","sMsg":"fixture"}}"#)),
            6.,
        );
        let out = prepared(&mut engine, 6.);
        assert_eq!(events(&out).len(), count);
        assert_eq!(
            terminals(&out, 7)[0].as_ref().unwrap_err().outcome,
            Some(if count == 0 {
                Outcome::Rejected
            } else {
                Outcome::Unknown
            })
        );
        assert!(
            !out.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
        );
    }
}

fn project(error: &VenueError, status: Option<u16>) -> LogEvent {
    Okx::unknown_outcome_telemetry().expect("OKX opts into #3359")(&EngineUnknownOutcome {
        now: f64::NAN,
        command: u32::MAX,
        request: RequestId(u64::MAX),
        status,
        error,
    })
}

fn indexed(error: &mut VenueError, indices: impl IntoIterator<Item = usize>) {
    error.elements = Some(Box::new(ElementOutcomes(
        indices
            .into_iter()
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: Some("sensitive-fixture?signature=body".into()),
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(VenueError::new("sensitive-fixture payload")),
                },
            })
            .collect(),
    )));
}

#[test]
fn projection_matches_nx_bounds_and_omits_invalid_status_and_indices() {
    let mut error = VenueError::new("https://sensitive-fixture.test/api/order?signature=payload")
        .venue_message("sensitive-fixture body")
        .partial_ids(&[
            "123".into(),
            "9".repeat(32),
            "9".repeat(33),
            "sensitive-fixture".into(),
        ]);
    indexed(&mut error, [0, 65535, 65536, usize::MAX]);
    for status in [
        None,
        Some(0),
        Some(99),
        Some(100),
        Some(599),
        Some(600),
        Some(u16::MAX),
    ] {
        let event = project(&error, status);
        nx_schema(&event, status.filter(|s| (100..=599).contains(s)));
        assert_eq!(
            field(&event, "request").unwrap().as_str(),
            Some("18446744073709551615")
        );
        assert_eq!(
            field(&event, "command").unwrap().as_str(),
            Some("okx-4294967295")
        );
        assert_eq!(
            field(&event, "placedOrderIds").unwrap().as_str(),
            Some(format!("123,{}", "9".repeat(32)).as_str())
        );
        assert_eq!(
            field(&event, "unknownIndices").unwrap().as_str(),
            Some("0,65535")
        );
    }
}

#[test]
fn projection_caps_both_lists_at_sixteen_and_never_relies_on_log_truncation() {
    let ids: Vec<_> = (0..17).map(|i| i.to_string()).collect();
    let mut error = VenueError::new("fixture").partial_ids(&ids);
    indexed(&mut error, 0..17);
    let event = project(&error, None);
    nx_schema(&event, None);
    let expected = ids[..16].join(",");
    for key in ["placedOrderIds", "unknownIndices"] {
        assert_eq!(
            field(&event, key).unwrap().as_str(),
            Some(expected.as_str())
        );
    }
    let ids: Vec<_> = (0..17).map(|i| format!("{i:032}")).collect();
    let error = VenueError::new("fixture").partial_ids(&ids);
    let event = project(&error, None);
    nx_schema(&event, None);
    assert_eq!(
        field(&event, "placedOrderIds").unwrap().as_str(),
        Some(ids[..15].join(",").as_str())
    );
}

#[test]
fn projection_preserves_proven_journal_ids_without_partial_and_ignores_unsent_rows() {
    let mut error = VenueError::new("sensitive-fixture");
    indexed(&mut error, [0, 1, 2]);
    let elements = &mut error.elements.as_mut().unwrap().0;
    elements[0].result = ElementResult::Placed {
        order_ids: vec![
            "123".into(),
            "123".into(),
            "sensitive-fixture?signature=body".into(),
        ],
    };
    elements[2].result = ElementResult::NotSent { error: None };
    let event = project(&error, None);
    nx_schema(&event, None);
    assert_eq!(
        field(&event, "placedOrderIds").unwrap().as_str(),
        Some("123")
    );
    assert_eq!(field(&event, "unknownIndices").unwrap().as_str(), Some("1"));
}
