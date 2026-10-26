mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::history::{Completion, CursorHistory, RangeHistory};
use safe_cex_engine::log::code;
use safe_cex_engine::paging::{CursorPage, CursorWalk, PageRow, RangeWalk, Step};
use safe_cex_engine::{
    Action, Command, Cx, ErrorKind, Event, Level, Notice, Query, Response, SocketId, TimerId,
    Value, Venue, VenueError,
};

struct Fixture;
fn encode(rows: &[String]) -> Result<String, VenueError> {
    Ok(format!("[{}]", rows.join(",")))
}
impl Venue for Fixture {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        let Command::Query {
            command,
            query: Query::PublicTrades {
                symbol: scenario, ..
            },
        } = command
        else {
            return;
        };
        if scenario.starts_with("cursor-") {
            let mut owner = CursorHistory::new(
                Completion::new(command),
                CursorWalk::new(1),
                |_| "cap".into(),
                encode,
            );
            if scenario != "cursor-empty" {
                owner.stage("1".into());
            }
            owner.advance_with_coverage(
                CursorPage {
                    next: None,
                    rows: 1,
                    fresh: 1,
                    want_more: false,
                },
                Some("native cursor gap".into()),
            );
            if scenario == "cursor-after-finish" {
                owner.advance_with_coverage(
                    CursorPage {
                        next: None,
                        rows: 0,
                        fresh: 0,
                        want_more: false,
                    },
                    Some("invalid gap".into()),
                );
            }
            if scenario == "cursor-encode-error" {
                owner.finish_with(cx, Ok(()), |_| Err(VenueError::new("encoding failed")));
            } else {
                owner.finish(cx, Ok(()));
            }
            return;
        }
        let mut owner = RangeHistory::new(
            Completion::new(command),
            RangeWalk::new(0.0, 10.0, 10.0, 2),
            |_| "walk gap".into(),
            encode,
        );
        if matches!(
            scenario.as_str(),
            "budget-exact" | "budget-over" | "budget-gap" | "budget-zero"
        ) {
            let budget = match scenario.as_str() {
                "budget-gap" => 1,
                "budget-zero" => 0,
                _ => 2,
            };
            owner = owner.with_request_budget(budget, "lifetime budget");
        }
        let step = owner.step();
        if scenario == "budget-zero" {
            assert_eq!(step, Step::Finished);
            owner.finish(cx, Ok(()));
            return;
        }
        assert_eq!(
            step,
            Step::Fetch {
                from: 0.0,
                to: 10.0
            }
        );
        if scenario == "budget-late" {
            owner = owner.with_request_budget(2, "late");
            owner.finish(cx, Ok(()));
            return;
        }
        if scenario == "gap-after-finish" {
            owner.accept_page();
            assert_eq!(owner.step(), Step::Finished);
            owner.accept_page_with_coverage(Some("invalid gap".into()));
            owner.finish(cx, Ok(()));
            return;
        }
        if scenario == "gap-empty" {
            owner.accept_page_with_coverage(Some("native cursor gap".into()));
            assert_eq!(owner.step(), Step::Finished);
            owner.finish(cx, Ok(()));
            return;
        }
        owner.stage(
            "1".into(),
            PageRow {
                timestamp: 1.0,
                identity: "1".into(),
            },
        );
        if matches!(scenario.as_str(), "budget-exact" | "budget-over") {
            owner.stage(
                "2".into(),
                PageRow {
                    timestamp: 2.0,
                    identity: "2".into(),
                },
            );
        }
        if scenario == "pending-step" {
            assert_eq!(owner.step(), Step::Finished);
            owner.finish(cx, Ok(()));
            return;
        }
        if matches!(scenario.as_str(), "budget-gap" | "gap-later-failure") {
            owner.accept_page_with_coverage(Some("native cursor gap".into()));
        } else {
            owner.accept_page();
        }
        let step = owner.step();
        if scenario == "budget-gap" {
            assert_eq!(step, Step::Finished);
            owner.finish(cx, Ok(()));
            return;
        }
        assert_eq!(
            step,
            Step::Fetch {
                from: if matches!(scenario.as_str(), "budget-exact" | "budget-over") {
                    2.0
                } else {
                    1.0
                },
                to: 10.0
            }
        );
        if scenario == "gap-later-failure" {
            owner.stage(
                "999".into(),
                PageRow {
                    timestamp: 2.0,
                    identity: "failed-prefix".into(),
                },
            );
            owner.finish(cx, Err(VenueError::new("later failure")));
            return;
        }
        // Resume the selected second request without selecting/charging it again.
        if scenario == "budget-over" {
            owner.stage(
                "3".into(),
                PageRow {
                    timestamp: 3.0,
                    identity: "3".into(),
                },
            );
            owner.stage(
                "4".into(),
                PageRow {
                    timestamp: 4.0,
                    identity: "4".into(),
                },
            );
        }
        owner.accept_page();
        assert_eq!(owner.step(), Step::Finished);
        owner.finish(cx, Ok(()));
    }
}
fn notices(scenario: &str) -> Vec<Notice> {
    let mut engine = support::seeded_engine(Fixture);
    engine.set_log_level(Level::Error);
    engine.handle(
        0.0,
        Event::Command(Command::Query {
            command: 7,
            query: Query::PublicTrades {
                symbol: scenario.into(),
                limit: None,
            },
        }),
    );
    engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(n) => Some(n),
            _ => None,
        })
        .collect()
}
#[test]
fn exact_budget_completion_is_complete_but_request_n_plus_one_is_not_issued() {
    let n = notices("budget-exact");
    assert_eq!(n.len(), 1);
    assert!(matches!(&n[0],Notice::CommandDone{result,..} if result=="[1,2]"));
    let n = notices("budget-over");
    assert_eq!(n.len(), 2);
    assert!(matches!(&n[0],Notice::Event(e) if e.message=="lifetime budget"));
    assert!(matches!(&n[1],Notice::CommandDone{result,..} if result=="[1,2,3,4]"));
}
#[test]
fn validated_coverage_gaps_keep_the_first_cause_and_exact_owner_before_success() {
    for (scenario, expected) in [
        ("budget-gap", "[1]"),
        ("gap-later-failure", "[1]"),
        ("gap-empty", "[]"),
        ("cursor-empty", "[]"),
        ("cursor-gap", "[1]"),
    ] {
        let n = notices(scenario);
        assert_eq!(n.len(), 2, "{scenario}");
        assert!(
            matches!(&n[0],Notice::Event(e) if e.code==code::HISTORY_INCOMPLETE && e.level==Level::Warn && e.message=="native cursor gap" && e.fields==vec![("command",Value::Num(7.0))])
        );
        assert!(matches!(&n[1],Notice::CommandDone{command:7,result} if result==expected));
    }
}
#[test]
fn invalid_admission_or_budget_and_failed_encoding_never_publish_rows_or_gap() {
    for scenario in [
        "budget-zero",
        "budget-late",
        "gap-after-finish",
        "pending-step",
        "cursor-after-finish",
        "cursor-encode-error",
    ] {
        let n = notices(scenario);
        assert_eq!(n.len(), 1, "{scenario}");
        assert!(
            matches!(&n[0],Notice::CommandFailed{command:7,error} if error.kind==Some(ErrorKind::Internal)),
            "{scenario}"
        );
    }
}
