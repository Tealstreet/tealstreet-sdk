mod support;
use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{EngineUnknownOutcome, LogEvent, RequestId, Venue, VenueError};
use safe_cex_venue_backpack::Backpack;

fn project(ids: Vec<String>, indices: Vec<usize>, request: u64, status: Option<u16>) -> LogEvent {
    let poison = "https://venue.test/exchange?api_key=SYNTHETIC&signature=SYNTHETIC_BODY";
    let mut error = VenueError::new(poison)
        .venue_code(poison)
        .venue_message(poison);
    error.partial = Some(Box::new(safe_cex_engine::Partial {
        order_ids: ids,
        total: None,
        failed_index: None,
    }));
    error.elements = Some(Box::new(ElementOutcomes(
        indices
            .into_iter()
            .map(|index| ElementOutcome {
                element: Element {
                    index,
                    client_order_id: Some(poison.into()),
                    role: None,
                },
                result: ElementResult::Unknown {
                    error: Box::new(VenueError::new(poison)),
                },
            })
            .collect(),
    )));
    let event = Backpack::unknown_outcome_telemetry().unwrap()(&EngineUnknownOutcome {
        now: 0.,
        command: u32::MAX,
        request: RequestId(request),
        status,
        error: &error,
    });
    assert_unknown_projection(&event);
    event
}

fn text<'a>(event: &'a LogEvent, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| value.as_str())
}

#[test]
fn projection_matches_nx_and_excludes_error_payloads() {
    for request in [0, 1, u64::MAX] {
        let event = project(
            vec!["1".into(), "9".repeat(32)],
            vec![0, 65535],
            request,
            Some(503),
        );
        assert_unknown_projection(&event);
        assert_eq!(text(&event, "command"), Some("command.4294967295"));
        assert_eq!(text(&event, "request"), Some(request.to_string().as_str()));
        assert_eq!(
            text(&event, "placedOrderIds"),
            Some(format!("1,{}", "9".repeat(32)).as_str())
        );
        assert_eq!(text(&event, "unknownIndices"), Some("0,65535"));
        assert_eq!(event.fields.len(), 6);
    }
}

#[test]
fn projection_status_bounds_and_absence() {
    for status in [
        None,
        Some(0),
        Some(99),
        Some(100),
        Some(599),
        Some(600),
        Some(u16::MAX),
    ] {
        let event = project(vec![], vec![], 1, status);
        let actual = event
            .fields
            .iter()
            .find(|(key, _)| *key == "status")
            .map(|(_, value)| value);
        let expected = status
            .filter(|s| (100..=599).contains(s))
            .map(safe_cex_engine::log::Value::from);
        assert_eq!(actual, expected.as_ref(), "status {status:?}");
        assert_eq!(text(&event, "placedOrderIds"), Some(""));
        assert_eq!(text(&event, "unknownIndices"), Some(""));
    }
}

#[test]
fn projection_caps_lists_at_sixteen_without_changing_evidence() {
    for count in [16, 17] {
        let event = project(
            vec![u64::MAX.to_string(); count],
            vec![65535; count],
            1,
            None,
        );
        assert_eq!(
            text(&event, "unknownIndices").unwrap().split(',').count(),
            16
        );
        assert_eq!(
            text(&event, "placedOrderIds").unwrap().split(',').count(),
            16
        );
        assert_eq!(event.fields.len(), 5);
    }
}

#[test]
fn projection_omits_invalid_lists_as_a_whole() {
    for invalid in [
        "",
        "abc",
        "0x123",
        "1,2",
        "123\n",
        "https://venue.test/exchange?signature=SYNTHETIC_BODY",
        &"9".repeat(33),
    ] {
        let event = project(vec!["1".into(), invalid.into()], vec![0], 1, None);
        assert_eq!(text(&event, "placedOrderIds"), None, "invalid native ID");
    }
    let event = project(vec!["1".into()], vec![0, 65536], 1, None);
    assert_eq!(text(&event, "unknownIndices"), None);
}

pub fn assert_unknown_projection(event: &safe_cex_engine::LogEvent) {
    let command = regex::Regex::new(r"\A[a-zA-Z][a-zA-Z0-9_.-]{0,63}\z").unwrap();
    let request = regex::Regex::new(r"^[0-9]{1,20}$").unwrap();
    let order = regex::Regex::new(r"\A[0-9]{1,32}\z").unwrap();
    let index = regex::Regex::new(r"\A[0-9]{1,5}\z").unwrap();
    let forbidden = regex::Regex::new(
        r"(?i)[a-z]+://|/api/|\?|&|=|body|payload|signature|signed|query|endpoint|api[_-]?key|SYNTHETIC",
    )
    .unwrap();
    let mut names = std::collections::HashSet::new();
    for (key, value) in &event.fields {
        assert!(names.insert(key), "duplicate field");
        assert!(
            matches!(
                *key,
                "command" | "outcome" | "placedOrderIds" | "unknownIndices" | "request" | "status"
            ),
            "extra field"
        );
        assert!(!forbidden.is_match(key));
        assert!(
            !forbidden.is_match(&format!("{value:?}")),
            "unsafe field {key}"
        );
        match *key {
            "command" => assert!(command.is_match(value.as_str().unwrap())),
            "request" => assert!(request.is_match(value.as_str().unwrap())),
            "outcome" => assert_eq!(value.as_str(), Some("unknown")),
            "placedOrderIds" | "unknownIndices" => {
                let value = value.as_str().unwrap();
                let entries: Vec<_> = if value.is_empty() {
                    vec![]
                } else {
                    value.split(',').collect()
                };
                assert!(entries.len() <= 16, "NX list cap");
                for entry in entries {
                    if *key == "placedOrderIds" {
                        assert!(order.is_match(entry));
                    } else {
                        assert!(index.is_match(entry));
                        assert!(entry.parse::<u16>().is_ok());
                    }
                }
            }
            "status" => match value {
                safe_cex_engine::log::Value::Num(status) => {
                    assert!(status.fract() == 0. && (100. ..=599.).contains(status))
                }
                _ => panic!("status must be a number"),
            },
            _ => unreachable!(),
        }
    }
    assert_eq!(
        event
            .fields
            .iter()
            .find(|(key, _)| *key == "outcome")
            .and_then(|(_, value)| value.as_str()),
        Some("unknown")
    );
}

fn ready_for(frame_fed: bool) -> support::managed::Driver {
    use base64::Engine as _;
    use support::managed::*;
    let (mut source, _) = ready();
    if !frame_fed {
        return source;
    }
    let frame = source.snapshot_frame(1500.).to_vec();
    let mut engine = Driver::new(safe_cex_venue_backpack::Options {
        key: Some("test-public-key".into()),
        secret: Some(base64::engine::general_purpose::STANDARD.encode([7u8; 32])),
        ..Default::default()
    });
    engine.set_frame_fed().unwrap();
    engine.start(1000.);
    let sent = requests(&mut engine);
    assert_eq!(sent.len(), 1);
    assert!(sent[0].url.ends_with("/markets"));
    engine
        .store_mut()
        .set_markets(source.store().markets().to_vec());
    engine.apply_frame(1500., &frame, true).unwrap();
    engine
}

fn place_one(engine: &mut support::managed::Driver) -> safe_cex_engine::HttpRequest {
    use safe_cex_engine::{Command, Event};
    use support::managed::*;
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: serde_json::from_value(serde_json::json!([{
                "symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140
            }]))
            .unwrap(),
        }),
    );
    let sent = requests(engine);
    assert_eq!(sent.len(), 1);
    sent.into_iter().next().unwrap()
}

fn unknown_events(actions: &[safe_cex_engine::Action]) -> Vec<&LogEvent> {
    actions
        .iter()
        .filter_map(|a| match a {
            safe_cex_engine::Action::Notice(safe_cex_engine::Notice::Event(e))
                if e.code == "order.outcome_unknown" =>
            {
                assert_unknown_projection(e);
                Some(e)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn engine_unknown_emits_once_in_managed_and_frame_fed_modes() {
    unknown_cases(&[Some(408), Some(429), Some(503), None]);
}

#[test]
fn decoded_unknown_emits_once_in_managed_and_frame_fed_modes() {
    unknown_cases(&[Some(200), Some(400)]);
}

fn unknown_cases(statuses: &[Option<u16>]) {
    use safe_cex_engine::{Action, Event, Notice, Outcome};
    use support::managed::*;
    for frame_fed in [false, true] {
        for &status in statuses {
            let mut engine = ready_for(frame_fed);
            let sent = place_one(&mut engine);
            if let Some(status) = status {
                reply(&mut engine, &sent, status, "{}");
            } else {
                engine.handle(2000., Event::HttpFailed { request: sent.id,
                    error: "https://venue.test/api/v1/order?api_key=SYNTHETIC&signature=SYNTHETIC_BODY" });
            }
            let out = actions(&mut engine);
            let failures: Vec<_> = out
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(failures.len(), 1, "{frame_fed} {status:?}");
            assert_eq!(failures[0].outcome, Some(Outcome::Unknown));
            let events = unknown_events(&out);
            assert_eq!(events.len(), 1, "{frame_fed} {status:?}");
            assert_eq!(text(events[0], "command"), Some("command.7"));
            assert_eq!(text(events[0], "placedOrderIds"), Some(""));
            assert_eq!(text(events[0], "unknownIndices"), Some("0"));
            if status.is_none() || status.is_some_and(|s| matches!(s, 408 | 429 | 500..=599)) {
                assert_eq!(
                    text(events[0], "request"),
                    Some(sent.id.0.to_string().as_str())
                );
                let actual = events[0]
                    .fields
                    .iter()
                    .find(|(key, _)| *key == "status")
                    .map(|(_, v)| v);
                let expected = status.map(safe_cex_engine::log::Value::from);
                assert_eq!(actual, expected.as_ref());
            }
            assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
            reply(&mut engine, &sent, 200, r#"{"id":"99"}"#);
            engine.handle(
                2000.,
                Event::HttpFailed {
                    request: sent.id,
                    error: "late duplicate",
                },
            );
            let late = actions(&mut engine);
            assert!(unknown_events(&late).is_empty());
            assert!(!late.iter().any(|a| matches!(
                a,
                Action::Http(_)
                    | Action::Notice(
                        Notice::CommandDone { command: 7, .. }
                            | Notice::CommandFailed { command: 7, .. }
                    )
            )));
        }
    }
}

#[test]
fn definite_local_refusal_emits_no_unknown_event() {
    use safe_cex_engine::{Action, Notice, Outcome};
    use support::managed::*;
    for frame_fed in [false, true] {
        let mut engine = ready_for(frame_fed);
        engine.handle(
            2000.,
            safe_cex_engine::Event::Command(safe_cex_engine::Command::PlaceOrders {
                command: 7,
                orders: serde_json::from_value(serde_json::json!([{
                    "symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140,
                    "clientOrderId":"invalid"
                }]))
                .unwrap(),
            }),
        );
        let out = actions(&mut engine);
        assert!(unknown_events(&out).is_empty());
        assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:7,error}) if error.outcome == Some(Outcome::NotSent))));
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn unknown_after_an_accepted_entry_retains_prefix_and_complete_journal() {
    use safe_cex_engine::{Action, Command, Event, Notice, Outcome};
    use support::managed::*;
    for frame_fed in [false, true] {
        for status in [200, 503] {
            let mut engine = ready_for(frame_fed);
            engine.handle(
                2000.,
                Event::Command(Command::PlaceOrders {
                    command: 7,
                    orders: serde_json::from_value(serde_json::json!([{
                        "symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140,
                        "takeProfit":150,"stopLoss":130
                    }]))
                    .unwrap(),
                }),
            );
            let entry = requests(&mut engine);
            assert_eq!(entry.len(), 1);
            reply(&mut engine, &entry[0], 200, r#"{"id":"91"}"#);
            let child = requests(&mut engine);
            assert_eq!(child.len(), 1);
            reply(&mut engine, &child[0], status, "{}");
            let out = actions(&mut engine);
            let events = unknown_events(&out);
            assert_eq!(events.len(), 1);
            assert_eq!(text(events[0], "placedOrderIds"), Some("91"));
            assert_eq!(text(events[0], "unknownIndices"), Some("1"));
            let error = out
                .iter()
                .find_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                    _ => None,
                })
                .unwrap();
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            let journal = error.elements.as_ref().unwrap();
            assert_eq!(journal.len(), 3);
            assert!(
                matches!(&journal[0].result,ElementResult::Placed{order_ids} if order_ids==&["91"])
            );
            assert!(matches!(journal[1].result, ElementResult::Unknown { .. }));
            assert!(matches!(
                journal[2].result,
                ElementResult::NotSent { error: None }
            ));
            assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
        }
    }
}

#[test]
fn engine_reports_cancel_and_setting_unknown_once() {
    use safe_cex_engine::{Action, Command, Event};
    use support::managed::*;
    for frame_fed in [false, true] {
        for setting in [false, true] {
            for status in [Some(503), None] {
                let mut engine = ready_for(frame_fed);
                let command = if setting {
                    Command::SetLeverage {
                        command: 7,
                        symbol: SYMBOL.into(),
                        leverage: 5.,
                        side: None,
                    }
                } else {
                    Command::CancelSymbolOrders {
                        command: 7,
                        symbol: SYMBOL.into(),
                    }
                };
                engine.handle(2000., Event::Command(command));
                let sent = requests(&mut engine);
                assert_eq!(sent.len(), 1);
                if let Some(status) = status {
                    reply(&mut engine, &sent[0], status, "{}");
                } else {
                    engine.handle(
                        2000.,
                        Event::HttpFailed {
                            request: sent[0].id,
                            error: "lost response",
                        },
                    );
                }
                let out = actions(&mut engine);
                assert_eq!(unknown_events(&out).len(), 1);
                assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
            }
        }
    }
}

struct PublishedProjection(Vec<String>);
impl safe_cex_engine::Venue for PublishedProjection {
    type Pending = ();
    fn start(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::SocketId) {}
    fn on_message(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, ()>,
        _: safe_cex_engine::SocketId,
        _: &str,
    ) {
    }
    fn on_close(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, ()>,
        _: safe_cex_engine::SocketId,
        _: u16,
    ) {
    }
    fn on_timer(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::TimerId) {}
    fn on_request_failed(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: (), _: &str) {}
    fn on_response(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, ()>,
        _: (),
        _: safe_cex_engine::Response<'_>,
    ) {
    }
    fn on_command(&mut self, cx: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::Command) {
        let event = project(self.0.clone(), (0..17).collect(), u64::MAX, Some(503));
        cx.event(event.level, event.code, event.message, event.fields);
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn published(ids: Vec<String>) -> LogEvent {
    let mut engine = safe_cex_engine::test_support::seeded_engine(PublishedProjection(ids));
    engine.handle(
        0.,
        safe_cex_engine::Event::Command(safe_cex_engine::Command::PlaceOrders {
            command: 1,
            orders: vec![],
        }),
    );
    let events: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            safe_cex_engine::Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == "order.outcome_unknown" =>
            {
                Some(event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(events.len(), 1);
    let event = events.into_iter().next().unwrap();
    assert_unknown_projection(&event);
    event
}
#[test]
fn published_projection_retains_fifteen_full_width_ids() {
    let ids = vec!["9".repeat(32); 15];
    let event = published(ids.clone());
    assert_eq!(text(&event, "placedOrderIds"), Some(ids.join(",").as_str()));
    assert_eq!(text(&event, "placedOrderIds").unwrap().len(), 494);
}
#[test]
fn published_projection_keeps_whole_ids_within_byte_and_entry_limits() {
    for count in [16, 17] {
        let event = published(vec!["9".repeat(32); count]);
        assert_eq!(
            text(&event, "placedOrderIds"),
            Some(vec!["9".repeat(32); 15].join(",").as_str())
        );
        assert_eq!(
            text(&event, "unknownIndices").unwrap().split(',').count(),
            16
        );
    }
    let short = vec![u64::MAX.to_string(); 17];
    let event = published(short.clone());
    assert_eq!(
        text(&event, "placedOrderIds"),
        Some(short[..16].join(",").as_str())
    );
    for final_width in [17, 18] {
        let mut ids = vec!["9".repeat(32); 15];
        ids.push("8".repeat(final_width));
        let expected = if final_width == 17 {
            ids.join(",")
        } else {
            ids[..15].join(",")
        };
        let event = published(ids);
        assert_eq!(text(&event, "placedOrderIds"), Some(expected.as_str()));
        assert!(expected.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
        if final_width == 17 {
            assert_eq!(expected.len(), 512);
        }
    }
}
