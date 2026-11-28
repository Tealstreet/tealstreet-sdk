use safe_cex_engine::test_support::{ProbeVenue, seeded_engine};
use safe_cex_engine::write_plan::{Element, ElementResult, WritePlan};
use safe_cex_engine::{Action, Cx, EngineUnknownOutcome, Notice, RequestId, Venue, VenueError};
use safe_cex_venue_weex::WeexFutures;

fn emitted_ids(count: usize) -> String {
    let mut journal = WritePlan::new(vec![Element {
        index: 0,
        client_order_id: None,
        role: None,
    }]);
    journal
        .record(
            0,
            ElementResult::Placed {
                order_ids: vec!["9".repeat(32); count],
            },
        )
        .unwrap();
    let error = journal.attach(VenueError::new("synthetic uncertainty"));
    let callback = WeexFutures::unknown_outcome_telemetry().unwrap();
    let mut projected = Some(callback(&EngineUnknownOutcome {
        now: 1.0,
        command: 1,
        request: RequestId(1),
        status: Some(503),
        error: &error,
    }));
    let probe = ProbeVenue::new(
        WeexFutures::new(Default::default()),
        move |_: &mut WeexFutures, cx: &mut Cx<'_, <WeexFutures as Venue>::Pending>| {
            let event = projected.take().unwrap();
            cx.event(event.level, event.code, event.message, event.fields);
        },
    );
    let mut engine = seeded_engine(probe);
    engine.start(1.0);
    engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => event
                .fields
                .iter()
                .find(|(key, _)| *key == "placedOrderIds")
                .map(|(_, value)| value.as_str().unwrap().to_owned()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn control_fifteen_maximum_ids_survive_publication() {
    let ids = emitted_ids(15);
    assert_eq!(ids.split(',').count(), 15);
    assert!(
        ids.split(',')
            .all(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit()))
    );
}

#[test]
fn sixteen_maximum_ids_must_still_match_nx_after_publication() {
    let ids = emitted_ids(16);
    assert_eq!(
        ids.split(',').count(),
        15,
        "retain the longest whole-ID prefix"
    );
    assert!(ids.len() <= safe_cex_engine::log::MAX_LOG_BYTES);
    assert!(
        ids.split(',')
            .all(|id| !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_digit())),
        "published placedOrderIds violates NX: {ids}"
    );
}
