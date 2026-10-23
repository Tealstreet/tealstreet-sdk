//! Native synchronous history replies must acquire genuine Read authority.
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, Notice, Query, RequestId, Response, VenueError,
};
use safe_cex_venue_bingx::Bingx;
mod support;

const NOW: f64 = 1_000.;
const COMMAND: u32 = 42;
const LOOKUP: &str = "BingX client order lookup is not supported by the Rust engine";

fn engine(private: bool) -> Engine<Bingx> {
    support::engine(support::Options {
        public_data_only: !private,
        key: private.then(|| "key".into()),
        secret: private.then(|| "secret".into()),
        ..Default::default()
    })
}
fn fills(symbol: Option<&str>, from: f64, to: f64) -> Query {
    Query::Fills {
        symbol: symbol.map(str::to_owned),
        order_id: None,
        from_ms: Some(from),
        to_ms: Some(to),
        limit: None,
    }
}
fn lookup() -> Query {
    Query::OrderByClientId {
        symbol: "BTCUSDT".into(),
        client_order_id: "native".into(),
    }
}
fn local_cases() -> [(bool, Query, Result<&'static str, &'static str>); 4] {
    [
        (false, fills(Some("BTCUSDT"), f64::NAN, 0.), Ok("[]")),
        (false, lookup(), Err(LOOKUP)),
        (true, fills(None, 2., 1.), Ok("[]")),
        (
            true,
            fills(Some("BTCUSDT"), 2., 1.),
            Err("invalid history time range"),
        ),
    ]
}
fn query(engine: &mut Engine<Bingx>, now: f64, query: Query) -> Vec<Action> {
    engine.handle(
        now,
        Event::Command(Command::Query {
            command: COMMAND,
            query,
        }),
    );
    safe_cex_engine::test_support::prepared_actions(engine, now)
}
fn result(actions: &[Action]) -> Result<&str, &VenueError> {
    let replies: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: COMMAND,
                result,
            }) => Some(Ok(result.as_str())),
            Action::Notice(Notice::CommandFailed {
                command: COMMAND,
                error,
            }) => Some(Err(error)),
            _ => None,
        })
        .collect();
    assert_eq!(
        replies.len(),
        1,
        "exactly one correlated terminal: {actions:?}"
    );
    assert!(
        actions
            .iter()
            .all(|action| matches!(action, Action::Notice(_))),
        "local terminal cannot acquire provider, transport or timer effects: {actions:?}"
    );
    replies[0]
}

#[test]
fn native_local_messages_precedence_and_reusable_ids_stay_unchanged() {
    for (private, q, expected) in local_cases() {
        let mut e = engine(private);
        let actions = query(&mut e, NOW, q);
        assert_eq!(result(&actions).map_err(|e| e.message.as_str()), expected);
        if let Err(error) = result(&actions) {
            assert_eq!(error.outcome, None);
        }
        let actions = query(&mut e, NOW + 1., fills(None, 2., 1.));
        assert_eq!(result(&actions).unwrap(), "[]");
    }
}

#[test]
fn invalid_clock_refuses_each_local_terminal_before_its_native_reply() {
    for (private, q, _) in local_cases() {
        let mut e = engine(private);
        let actions = query(&mut e, f64::NAN, q);
        let error = result(&actions).expect_err("invalid entry cannot publish a local result");
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
    }
}

fn fill_capacity(engine: &mut Engine<Bingx>) -> RequestId {
    let mut requests = Vec::new();
    for command in 1_000..1_256 {
        engine.handle(NOW, Event::Command(Command::FetchAccount { command }));
        for action in safe_cex_engine::test_support::prepared_actions(engine, NOW) {
            match action {
                Action::Http(request) => {
                    assert!(request.url.contains("/openApi/swap/v3/user/balance"));
                    requests.push(request.id);
                }
                Action::Notice(Notice::CommandFailed { error, .. }) => {
                    panic!("capacity setup refused: {error:?}")
                }
                _ => {}
            }
        }
    }
    assert_eq!(
        requests.len(),
        256,
        "all occupied roots are real managed reads"
    );
    requests[0]
}

#[test]
fn full_registry_refuses_unsupported_lookup_without_replacing_its_typed_error() {
    let mut e = engine(true);
    let occupied = fill_capacity(&mut e);
    let actions = query(&mut e, NOW, lookup());
    let error = result(&actions).expect_err("capacity must refuse");
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.message, "Engine request capacity reached");
    assert_eq!(error.outcome, None);
    e.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: occupied,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":0,"data":[{"asset":"USDT","userId":"original-owner"}]}"#,
            },
        },
    );
    let original = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.);
    assert_eq!(
        original
            .iter()
            .filter(|a| matches!(a,
        Action::Notice(Notice::CommandDone { command: 1_000, result })
            if result == r#"{"userId":"original-owner"}"#))
            .count(),
        1,
        "local capacity refusal must not retire an unrelated occupied owner"
    );
    let actions = query(&mut e, NOW + 2., lookup());
    assert_eq!(result(&actions).unwrap_err().message, LOOKUP);
}

#[test]
fn full_registry_refuses_a_missing_symbol_result_without_publishing_empty_rows() {
    let mut e = engine(true);
    fill_capacity(&mut e);
    let actions = query(&mut e, NOW, fills(None, 2., 1.));
    let error = result(&actions).expect_err("capacity cannot publish []");
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.message, "Engine request capacity reached");
    assert_eq!(error.outcome, None);
}

#[test]
fn full_registry_keeps_capacity_instead_of_a_native_range_error() {
    let mut e = engine(true);
    fill_capacity(&mut e);
    let actions = query(&mut e, NOW, fills(Some("BTCUSDT"), 2., 1.));
    let error = result(&actions).expect_err("capacity must refuse");
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.message, "Engine request capacity reached");
    assert_eq!(error.outcome, None);
}
