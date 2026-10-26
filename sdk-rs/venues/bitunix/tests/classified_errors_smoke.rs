mod support;

use safe_cex_engine::{Action, Command, ErrorKind, Event, Notice, Outcome, VenueError};
use support::*;

fn failure(engine: &mut TestEngine, command: u32) -> VenueError {
    let failures: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: id, error }) if id == command => {
                Some(error)
            }
            Action::Notice(Notice::CommandDone { command: id, .. }) if id == command => {
                panic!("failed command completed successfully")
            }
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    failures.into_iter().next().unwrap()
}

#[test]
fn missing_cancel_identity_proves_no_write_was_sent() {
    let (mut engine, _) = startup();
    engine.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 991,
            ids: vec!["missing-native-order".into()],
        }),
    );
    let error = failure(&mut engine, 991);
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(error.message.contains("missing-native-order"));
}

#[test]
fn dispatched_write_transport_loss_is_unknown() {
    let (mut engine, _) = startup();
    engine.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 992,
            orders: vec![serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500,"amount":0.002}"#,
            )
            .unwrap()],
        }),
    );
    let request = requests(&mut engine).remove(0);
    assert!(request.url.ends_with("place_order"));
    engine.handle(
        2001.0,
        Event::HttpFailed {
            request: request.id,
            error: "response lost",
        },
    );
    let error = failure(&mut engine, 992);
    assert_eq!(error.message, "response lost");
    assert_eq!(error.kind, Some(ErrorKind::Network));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
}
