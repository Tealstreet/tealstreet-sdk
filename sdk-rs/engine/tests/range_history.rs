mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::history::{Completion, RangeHistory};
use safe_cex_engine::log::code;
use safe_cex_engine::paging::{PageRow, RangeWalk, Step};
use safe_cex_engine::{
    Action, Command, Cx, ErrorKind, Event, Level, Method, Notice, Outcome, Query, Response,
    SocketId, TimerId, Value, Venue, VenueError,
};

struct Read {
    history: RangeHistory<String>,
    scenario: String,
}
struct Fixture;

fn stage(history: &mut RangeHistory<String>, value: &str, time: f64, id: &str) {
    history.stage(
        value.into(),
        PageRow {
            timestamp: time,
            identity: id.into(),
        },
    );
}
fn page(history: &mut RangeHistory<String>, rows: &[(&str, f64, &str)]) {
    for &(value, time, id) in rows {
        stage(history, value, time, id);
    }
    history.accept_page();
}
fn failed() -> Result<(), VenueError> {
    Err(VenueError::new("actual page failed").kind(ErrorKind::Network))
}

impl Venue for Fixture {
    type Pending = Read;
    fn start(&mut self, _: &mut Cx<'_, Read>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Read>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Read>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Read>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Read>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Read>, command: Command) {
        if let Command::Query {
            command,
            query: Query::PublicTrades {
                symbol: scenario, ..
            },
        } = command
        {
            let chunk = if matches!(
                scenario.as_str(),
                "empty-failure" | "filtered-failure" | "chunks"
            ) {
                50.0
            } else {
                1000.0
            };
            let mut history = RangeHistory::new(
                Completion::new(command),
                RangeWalk::new(0.0, 100.0, chunk, 2),
                |gaps| format!("{gaps} known range gap(s)"),
                |rows| Ok(format!("[{}]", rows.join(","))),
            );
            assert_eq!(history.command(), command);
            assert_eq!(
                history.step(),
                Step::Fetch {
                    from: 0.0,
                    to: 100.0f64.min(chunk)
                }
            );
            cx.request(
                Method::Get,
                "https://history.test".into(),
                vec![],
                None,
                Read { history, scenario },
            );
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Read>, read: Read, _: Response<'_>) {
        let Read {
            mut history,
            scenario,
        } = read;
        match scenario.as_str() {
            "first-failure" => {
                stage(&mut history, "999", 30.0, "failed-prefix");
                history.finish_with(cx, failed(), |_| panic!("first failure cannot finalize"));
            }
            "empty-failure" | "filtered-failure" | "chunks" => {
                if scenario == "filtered-failure" {
                    stage(&mut history, "999", -1.0, "outside-range");
                }
                history.accept_page();
                assert_eq!(history.accepted_len(), 0);
                assert_eq!(history.step(), Step::ChunkDone);
                assert_eq!(
                    history.step(),
                    Step::Fetch {
                        from: 50.0,
                        to: 100.0
                    }
                );
                if scenario == "chunks" {
                    history.accept_page();
                    assert_eq!(history.step(), Step::Finished);
                    history.finish(cx, Ok(()));
                } else {
                    history.finish_with(cx, failed(), |_| {
                        panic!("excluded rows cannot enable recovery")
                    });
                }
            }
            "budget" => {
                for page_index in 0..50 {
                    if page_index > 0 {
                        assert_eq!(
                            history.step(),
                            Step::Fetch {
                                from: f64::from(page_index * 2),
                                to: 100.0
                            }
                        );
                    }
                    for value in [page_index * 2 + 1, page_index * 2 + 2] {
                        stage(
                            &mut history,
                            &value.to_string(),
                            f64::from(value),
                            &format!("id{value}"),
                        );
                    }
                    history.accept_page();
                }
                assert_eq!(history.accepted_len(), 100);
                assert_eq!(history.step(), Step::Finished);
                history.finish(cx, Ok(()));
            }
            "selected" => {
                page(
                    &mut history,
                    &[
                        ("1", 20.0, "a"),
                        ("2", 40.0, "b"),
                        ("999", -1.0, "excluded"),
                    ],
                );
                assert_eq!(history.accepted_len(), 2);
                assert_eq!(
                    history.step(),
                    Step::Fetch {
                        from: 40.0,
                        to: 100.0
                    }
                );
                page(&mut history, &[("888", 40.0, "b"), ("3", 90.0, "c")]);
                assert_eq!(history.accepted_len(), 3);
                assert_eq!(
                    history.step(),
                    Step::Fetch {
                        from: 90.0,
                        to: 100.0
                    }
                );
                page(&mut history, &[("777", 90.0, "c")]);
                assert_eq!(history.step(), Step::Finished);
                history.finish(cx, Ok(()));
            }
            "gap-failure" | "gap-finished" | "gap-encoder-failure" => {
                page(&mut history, &[("1", 20.0, "a"), ("2", 40.0, "b")]);
                assert_eq!(
                    history.step(),
                    Step::Fetch {
                        from: 40.0,
                        to: 100.0
                    }
                );
                page(&mut history, &[("888", 20.0, "a"), ("777", 40.0, "b")]);
                assert_eq!(history.accepted_len(), 2);
                assert_eq!(
                    history.step(),
                    Step::Fetch {
                        from: 41.0,
                        to: 100.0
                    }
                );
                if scenario == "gap-finished" {
                    history.accept_page();
                    assert_eq!(history.step(), Step::Finished);
                    history.finish(cx, Ok(()));
                } else {
                    stage(&mut history, "999", 50.0, "failed-prefix");
                    if scenario == "gap-encoder-failure" {
                        history.finish_with(cx, failed(), |_| {
                            Err(VenueError::new("encoder failed").kind(ErrorKind::Internal))
                        });
                    } else {
                        history.finish(cx, failed());
                    }
                }
            }
            "finished-stage" | "finished-step" => {
                history.accept_page();
                assert_eq!(history.step(), Step::Finished);
                if scenario == "finished-stage" {
                    stage(&mut history, "999", 30.0, "late");
                } else {
                    assert_eq!(history.step(), Step::Finished);
                }
                history.finish_with(cx, failed(), |_| panic!("lifecycle poison must reject"));
            }
            _ => {
                page(&mut history, &[("1", 20.0, "accepted")]);
                match scenario.as_str() {
                    "premature-finish" => {
                        history.finish_with(cx, Ok(()), |_| panic!("premature finalizer"))
                    }
                    "repeat-admission" => {
                        history.accept_page();
                        history
                            .finish_with(cx, failed(), |_| panic!("admission poison must reject"));
                    }
                    "stage-without-fetch" => {
                        stage(&mut history, "999", 30.0, "no-fetch");
                        history.finish_with(cx, failed(), |_| panic!("staging poison must reject"));
                    }
                    "failure-without-fetch" => {
                        history.finish_with(cx, failed(), |_| panic!("failure must have a fetch"))
                    }
                    _ => {
                        assert!(matches!(history.step(), Step::Fetch { .. }));
                        stage(&mut history, "999", 30.0, "failed-prefix");
                        if scenario == "pending-step" {
                            assert_eq!(history.step(), Step::Finished);
                            history.finish_with(cx, failed(), |_| {
                                panic!("pending fetch poison must reject")
                            });
                        } else if scenario == "strict-sign" {
                            history.reject(
                                cx,
                                VenueError::new("provider refused")
                                    .kind(ErrorKind::AuthPermission)
                                    .outcome(Outcome::NotSent)
                                    .venue_code("permission"),
                            );
                        } else {
                            history.finish_with(cx, failed(), |rows| {
                                Ok(format!("[{}]", rows.join(",")))
                            });
                        }
                    }
                }
            }
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Read>, read: Read, error: &str) {
        read.history
            .finish(cx, Err(VenueError::new(error).kind(ErrorKind::Network)));
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn run(scenario: &str) -> Vec<Notice> {
    let mut engine = support::seeded_engine(Fixture);
    engine.set_log_level(Level::Error);
    engine.handle(
        0.0,
        Event::Command(Command::Query {
            command: 91,
            query: Query::PublicTrades {
                symbol: scenario.into(),
                limit: None,
            },
        }),
    );
    let request = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "fixture",
            },
        },
    );
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(notice) => Some(notice),
            _ => None,
        })
        .collect()
}

#[test]
fn range_selection_preserves_order_and_excludes_duplicate_outside_rows() {
    let notices = run("selected");
    assert!(
        matches!(notices.as_slice(), [Notice::CommandDone { command: 91, result }] if result == "[1,2,3]")
    );
    let notices = run("chunks");
    assert!(
        matches!(notices.as_slice(), [Notice::CommandDone { command: 91, result }] if result == "[]")
    );
}
#[test]
fn selected_accepted_rows_only_enable_actual_later_page_recovery() {
    for scenario in ["first-failure", "empty-failure", "filtered-failure"] {
        let notices = run(scenario);
        assert!(
            matches!(notices.as_slice(), [Notice::CommandFailed { command: 91, error }] if error.kind == Some(ErrorKind::Network)),
            "{scenario}: {notices:?}"
        );
    }
    let notices = run("later-failure");
    assert!(
        matches!(notices.as_slice(), [Notice::Event(event), Notice::CommandDone { command: 91, result }] if result == "[1]" && event.code == code::HISTORY_INCOMPLETE && event.message.contains("actual page failed") && event.fields == [("command", Value::Num(91.0))])
    );
}
#[test]
fn current_chunk_gap_survives_failure_before_rollover_and_is_not_double_counted_after_finish() {
    for scenario in ["gap-failure", "gap-finished"] {
        let notices = run(scenario);
        assert!(
            matches!(notices.as_slice(), [Notice::Event(event), Notice::CommandDone { command: 91, result }] if result == "[1,2]" && event.message == "1 known range gap(s)" && event.level == Level::Warn && event.fields == [("command", Value::Num(91.0))]),
            "{scenario}: {notices:?}"
        );
    }
}
#[test]
fn range_lifecycle_poison_and_encoder_errors_never_publish_partial_success() {
    for scenario in [
        "premature-finish",
        "repeat-admission",
        "stage-without-fetch",
        "failure-without-fetch",
        "pending-step",
        "finished-stage",
        "finished-step",
        "gap-encoder-failure",
    ] {
        let notices = run(scenario);
        assert!(
            matches!(notices.as_slice(), [Notice::CommandFailed { command: 91, error }] if error.kind == Some(ErrorKind::Internal)),
            "{scenario}: {notices:?}"
        );
    }
}
#[test]
fn strict_signer_rejection_preserves_typed_details_after_accepted_rows() {
    let notices = run("strict-sign");
    assert!(
        matches!(notices.as_slice(), [Notice::CommandFailed { command: 91, error }] if error.kind == Some(ErrorKind::AuthPermission) && error.outcome == Some(Outcome::NotSent) && error.venue_code.as_deref() == Some("permission") && error.message == "provider refused")
    );
}

#[test]
fn actual_range_page_budget_retains_selected_rows_with_one_owned_gap() {
    let notices = run("budget");
    let [
        Notice::Event(event),
        Notice::CommandDone {
            command: 91,
            result,
        },
    ] = notices.as_slice()
    else {
        panic!("{notices:?}");
    };
    assert_eq!(event.code, code::HISTORY_INCOMPLETE);
    assert_eq!(event.message, "1 known range gap(s)");
    assert_eq!(event.fields, [("command", Value::Num(91.0))]);
    let rows: Vec<u32> = serde_json::from_str(result).unwrap();
    assert_eq!(rows, (1..=100).collect::<Vec<_>>());
}
