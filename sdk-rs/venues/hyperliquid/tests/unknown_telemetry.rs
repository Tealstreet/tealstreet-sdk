mod support;
use safe_cex_engine::write_plan::{Element, ElementOutcome, ElementOutcomes, ElementResult};
use safe_cex_engine::{EngineUnknownOutcome, LogEvent, RequestId, Venue, VenueError};
use safe_cex_venue_hyperliquid::Hyperliquid;
use support::assert_unknown_projection;

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
    let event = Hyperliquid::unknown_outcome_telemetry().unwrap()(&EngineUnknownOutcome {
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
