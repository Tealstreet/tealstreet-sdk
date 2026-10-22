mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::history::{Completion, CursorHistory, HistoryRows};
use safe_cex_engine::log::code;
use safe_cex_engine::paging::{CursorPage, CursorStep, CursorWalk, Stall};
use safe_cex_engine::{
    Action, Command, Cx, ErrorKind, Event, Level, Method, Notice, Query, Response, SocketId,
    TimerId, Value, Venue, VenueError,
};

enum Read {
    Direct(Completion),
    Cursor(CursorHistory<String>),
    Rows(HistoryRows<String>),
}

struct Fixture;
impl Venue for Fixture {
    type Pending = Read;
    fn start(&mut self, _: &mut Cx<'_, Read>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Read>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Read>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Read>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Read>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Read>, command: Command) {
        if let Command::Query { command, query } = command {
            let read = if let Query::Fills { symbol, .. } = &query {
                let encode: fn(&[String]) -> Result<String, VenueError> =
                    if symbol.as_deref() == Some("serialize-failure") {
                        |_| Err(VenueError::new("serialization failed").kind(ErrorKind::Internal))
                    } else {
                        |rows| Ok(format!("[{}]", rows.join(",")))
                    };
                Read::Rows(HistoryRows::new(Completion::new(command), encode))
            } else if matches!(query, Query::Income { .. }) {
                Read::Cursor(CursorHistory::new(
                    Completion::new(command),
                    CursorWalk::new(if matches!(&query, Query::Income { kind: Some(_), .. }) {
                        2
                    } else {
                        1
                    }),
                    |reason| {
                        assert_eq!(reason, Stall::PageBudget);
                        "page cap".into()
                    },
                    |rows| Ok(format!("[{}]", rows.join(","))),
                ))
            } else {
                Read::Direct(Completion::new(command))
            };
            cx.request(
                Method::Get,
                "https://history.test".into(),
                vec![],
                None,
                read,
            );
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Read>, read: Read, response: Response<'_>) {
        match read {
            Read::Rows(mut rows) => {
                if response.body.starts_with("finalizer-") {
                    let accepted = response.body != "finalizer-first";
                    if accepted {
                        for row in ["2", "1", "2"] {
                            rows.stage(row.into());
                        }
                        assert_eq!((rows.accepted_len(), rows.staged_len()), (0, 3));
                        rows.accept_page();
                    }
                    rows.stage("999".into());
                    assert_eq!(rows.accepted_len(), if accepted { 3 } else { 0 });
                    assert_eq!(rows.staged_len(), 1);
                    let calls = std::cell::Cell::new(0);
                    let limit = 1;
                    rows.finish_with(
                        cx,
                        Err(VenueError::new("failed page").kind(ErrorKind::Network)),
                        |mut accepted_rows| {
                            calls.set(calls.get() + 1);
                            assert!(accepted, "first failure must not invoke finalization");
                            assert_eq!(accepted_rows, ["2", "1", "2"]);
                            accepted_rows.sort();
                            accepted_rows.dedup();
                            accepted_rows.truncate(limit);
                            if response.body == "finalizer-encoding" {
                                Err(VenueError::new("context encoding failed")
                                    .kind(ErrorKind::Internal))
                            } else {
                                Ok(format!("[{}]", accepted_rows.join(",")))
                            }
                        },
                    );
                    assert_eq!(calls.get(), usize::from(accepted));
                    return;
                }
                if response.body == "later" {
                    rows.stage("1".into());
                    rows.accept_page();
                }
                rows.stage("999".into()); // From the response which failed validation.
                rows.finish(
                    cx,
                    Err(VenueError::new("page failed").kind(ErrorKind::Network)),
                );
            }
            Read::Direct(mut completion) => {
                if response.body != "complete" {
                    completion.incomplete("missing rows");
                    completion.incomplete("later cause must not erase the first");
                }
                let result = if response.body == "failed" {
                    Err(VenueError::new("serialization failed").kind(ErrorKind::Internal))
                } else {
                    Ok("[]".into())
                };
                completion.finish(cx, result);
            }
            Read::Cursor(mut history) => {
                if matches!(
                    response.body,
                    "discard-page" | "discard-after-finished" | "discard-after-poison"
                ) {
                    assert!(history.fresh("accepted".into()));
                    history.stage("1".into());
                    assert!(matches!(
                        history.advance_with_coverage(
                            CursorPage {
                                next: Some("next".into()),
                                rows: 1,
                                fresh: 1,
                                want_more: true,
                            },
                            Some("first known gap".into())
                        ),
                        CursorStep::More(_)
                    ));
                    history.stage("999".into());
                    if response.body == "discard-after-poison" {
                        assert!(!history.restart_stage());
                        assert!(!history.discard_staged_page());
                        history.finish(cx, Err("later failure cannot erase poisoning".into()));
                        return;
                    }
                    assert!(history.discard_staged_page());
                    assert_eq!(history.accepted_len(), 1);
                    assert_eq!(history.staged_len(), 0);
                    assert_eq!(history.pages(), 1);
                    assert_eq!(history.cursor(), Some("next"));
                    assert!(!history.fresh("accepted".into()));
                    history.stage("2".into());
                    assert_eq!(
                        history.advance(CursorPage {
                            next: None,
                            rows: 1,
                            fresh: 1,
                            want_more: false,
                        }),
                        CursorStep::Done
                    );
                    if response.body == "discard-after-finished" {
                        assert!(!history.discard_staged_page());
                    }
                    history.finish(cx, Ok(()));
                    return;
                }
                if matches!(response.body, "probe-staged" | "probe-admitted") {
                    history.stage("1".into());
                    if response.body == "probe-admitted" {
                        history.advance(CursorPage {
                            next: Some("a".into()),
                            rows: 1,
                            fresh: 1,
                            want_more: true,
                        });
                    }
                    for next in ["b", "c"] {
                        history.advance_probe(CursorPage {
                            next: Some(next.into()),
                            rows: 1,
                            fresh: 1,
                            want_more: true,
                        });
                    }
                    history.finish(cx, Ok(()));
                    return;
                }
                if matches!(
                    response.body,
                    "finalizer-policy" | "finalizer-poison" | "finalizer-active"
                ) {
                    history.stage("1".into());
                    assert_eq!(history.accepted_len(), 0);
                    assert_eq!(history.staged_len(), 1);
                    assert!(matches!(
                        history.advance(CursorPage {
                            next: Some("a".into()),
                            rows: 1,
                            fresh: 1,
                            want_more: true,
                        }),
                        CursorStep::More(_)
                    ));
                    assert_eq!(history.accepted_len(), 1);
                    if response.body == "finalizer-policy" {
                        history.stage("2".into());
                        assert_eq!(
                            history.advance(CursorPage {
                                next: Some("b".into()),
                                rows: 1,
                                fresh: 1,
                                want_more: true,
                            }),
                            CursorStep::Stalled(Stall::PageBudget)
                        );
                        history.reject(
                            cx,
                            VenueError::new("strict pagination policy")
                                .kind(ErrorKind::InvalidInput),
                        );
                    } else {
                        let result = if response.body == "finalizer-poison" {
                            let _ignored = history.restart_stage();
                            Err("later page failure".into())
                        } else {
                            Ok(())
                        };
                        history.finish_with(cx, result, |_| {
                            panic!("lifecycle must reject before finalization")
                        });
                    }
                    return;
                }
                if matches!(response.body, "restart-stage" | "premature-restart") {
                    assert!(history.fresh("shared".into()));
                    history.stage("1".into());
                    assert!(matches!(
                        history.advance(CursorPage {
                            next: Some("a".into()),
                            rows: 1,
                            fresh: 1,
                            want_more: true,
                        }),
                        CursorStep::More(_)
                    ));
                    if response.body == "premature-restart" {
                        let _ignored = history.restart_stage();
                        history.finish(
                            cx,
                            Err("later failure cannot hide a lifecycle error".into()),
                        );
                        return;
                    }
                    history.stage("2".into());
                    assert_eq!(
                        history.advance(CursorPage {
                            next: Some("b".into()),
                            rows: 1,
                            fresh: 1,
                            want_more: true,
                        }),
                        CursorStep::Stalled(Stall::PageBudget)
                    );
                    assert!(history.restart_stage());
                    assert!(!history.fresh("shared".into()));
                    assert_eq!(history.cursor(), None);
                    history.stage("3".into());
                    assert_eq!(
                        history.advance(CursorPage {
                            next: Some("a".into()),
                            rows: 1,
                            fresh: 1,
                            want_more: true,
                        }),
                        CursorStep::More("a".into())
                    );
                    history.stage("4".into());
                    assert_eq!(
                        history.advance(CursorPage {
                            next: None,
                            rows: 1,
                            fresh: 1,
                            want_more: false,
                        }),
                        CursorStep::Done
                    );
                    history.finish(cx, Ok(()));
                    return;
                }
                if response.body == "premature-after-accepted" {
                    history.stage("1".into());
                    assert!(matches!(
                        history.advance(CursorPage {
                            next: Some("next".into()),
                            rows: 1,
                            fresh: 1,
                            want_more: true,
                        }),
                        CursorStep::More(_)
                    ));
                    history.finish(cx, Ok(()));
                    return;
                }
                if response.body != "premature" {
                    let next = (response.body != "limit").then(|| "next".into());
                    let step = history.advance(CursorPage {
                        next,
                        rows: 1,
                        fresh: 1,
                        want_more: response.body != "limit",
                    });
                    if response.body == "limit" {
                        assert_eq!(step, CursorStep::Done);
                    } else {
                        assert_eq!(step, CursorStep::Stalled(Stall::PageBudget));
                    }
                }
                let result = if response.body == "failed" {
                    Err("failed after paging decision".into())
                } else {
                    Ok(())
                };
                history.finish(cx, result);
            }
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Read>, read: Read, error: &str) {
        match read {
            Read::Rows(history) => history.finish(cx, Err(error.into())),
            Read::Direct(history) => history.finish(cx, Err(error.into())),
            Read::Cursor(history) => history.finish(cx, Err(error.into())),
        }
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn query(command: u32, cursor: bool) -> Event<'static> {
    let query = if cursor {
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        }
    } else {
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        }
    };
    Event::Command(Command::Query { command, query })
}

#[test]
fn concurrent_commands_finish_out_of_order_with_owned_markers_at_error_only_level() {
    let mut engine = support::seeded_engine(Fixture);
    engine.set_log_level(Level::Error);
    engine.handle(0., query(71, false));
    engine.handle(0., query(72, false));
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 2);
    for request in requests.into_iter().rev() {
        engine.handle(
            0.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "partial",
                },
            },
        );
    }
    let notices: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(notice) => Some(notice),
            _ => None,
        })
        .collect();
    assert_eq!(notices.len(), 4);
    for (pair, command) in notices.as_chunks::<2>().0.iter().zip([72, 71]) {
        assert!(matches!(&pair[0], Notice::Event(event)
            if event.code == code::HISTORY_INCOMPLETE && event.level == Level::Warn
            && event.message == "missing rows"
            && event.fields == vec![("command", Value::Num(f64::from(command)))]));
        assert!(matches!(pair[1], Notice::CommandDone { command: id, .. } if id == command));
    }
}

#[test]
fn failure_never_emits_a_marker_or_partial_success_even_after_a_stall() {
    for cursor in [false, true] {
        let mut engine = support::seeded_engine(Fixture);
        engine.handle(0., query(9, cursor));
        let request = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::Http(request) => Some(request.id),
                _ => None,
            })
            .unwrap();
        engine.handle(
            0.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "failed",
                },
            },
        );
        let notices: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(notice) => Some(notice),
                _ => None,
            })
            .collect();
        assert_eq!(notices.len(), 1);
        assert!(matches!(
            notices[0],
            Notice::CommandFailed { command: 9, .. }
        ));
    }
}

#[test]
fn requested_limit_is_complete_but_pending_page_cannot_complete_silently() {
    for body in ["limit", "premature", "partial"] {
        let mut engine = support::seeded_engine(Fixture);
        engine.set_log_level(Level::Error);
        engine.handle(0., query(9, true));
        let request = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::Http(request) => Some(request.id),
                _ => None,
            })
            .unwrap();
        engine.handle(
            0.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        let notices: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(notice) => Some(notice),
                _ => None,
            })
            .collect();
        match body {
            "limit" => {
                assert_eq!(notices.len(), 1);
                assert!(matches!(notices[0], Notice::CommandDone { command: 9, .. }));
            }
            "premature" => {
                assert_eq!(notices.len(), 1);
                assert!(
                    matches!(&notices[0], Notice::CommandFailed { command: 9, error }
                    if error.kind == Some(ErrorKind::Internal))
                );
            }
            _ => {
                assert_eq!(notices.len(), 2);
                assert!(matches!(&notices[0], Notice::Event(event)
                    if event.code == code::HISTORY_INCOMPLETE));
                assert!(matches!(notices[1], Notice::CommandDone { command: 9, .. }));
            }
        }
    }
}

#[test]
fn only_accepted_pages_can_be_returned_after_failure_and_encoding_failure_still_rejects() {
    for (body, serialize) in [("first", false), ("later", false), ("later", true)] {
        let mut engine = support::seeded_engine(Fixture);
        engine.set_log_level(Level::Error);
        engine.handle(
            0.,
            Event::Command(Command::Query {
                command: 13,
                query: Query::Fills {
                    symbol: serialize.then(|| "serialize-failure".into()),
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            }),
        );
        let request = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::Http(request) => Some(request.id),
                _ => None,
            })
            .unwrap();
        engine.handle(
            0.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        let notices: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(notice) => Some(notice),
                _ => None,
            })
            .collect();
        if body == "first" || serialize {
            assert_eq!(notices.len(), 1);
            let kind = if serialize {
                ErrorKind::Internal
            } else {
                ErrorKind::Network
            };
            assert!(
                matches!(&notices[0], Notice::CommandFailed { command: 13, error }
                if error.kind == Some(kind))
            );
        } else {
            assert_eq!(notices.len(), 2);
            assert!(matches!(&notices[0], Notice::Event(event)
                if event.code == code::HISTORY_INCOMPLETE && event.message.contains("page failed")
                && event.fields == vec![("command", Value::Num(13.0))]));
            assert!(
                matches!(&notices[1], Notice::CommandDone { command: 13, result } if result == "[1]")
            );
        }
    }
}

#[test]
fn an_accepted_page_does_not_turn_premature_success_into_partial_history() {
    let mut engine = support::seeded_engine(Fixture);
    engine.handle(
        0.,
        Event::Command(Command::Query {
            command: 14,
            query: Query::Income {
                symbol: None,
                kind: Some("two-pages".into()),
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }),
    );
    let request = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "premature-after-accepted",
            },
        },
    );
    let notices: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(notice) => Some(notice),
            _ => None,
        })
        .collect();
    assert_eq!(notices.len(), 1);
    assert!(
        matches!(&notices[0], Notice::CommandFailed { command: 14, error }
        if error.kind == Some(ErrorKind::Internal))
    );
}

#[test]
fn stage_restart_keeps_owner_rows_and_incomplete_cause_but_not_cursor_or_budget() {
    for body in ["restart-stage", "premature-restart"] {
        let mut engine = support::seeded_engine(Fixture);
        engine.set_log_level(Level::Error);
        engine.handle(
            0.,
            Event::Command(Command::Query {
                command: 15,
                query: Query::Income {
                    symbol: None,
                    kind: Some("two-pages".into()),
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            }),
        );
        let request = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::Http(request) => Some(request.id),
                _ => None,
            })
            .unwrap();
        engine.handle(
            0.,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        let notices: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::Notice(notice) => Some(notice),
                _ => None,
            })
            .collect();
        if body == "restart-stage" {
            assert_eq!(notices.len(), 2);
            assert!(matches!(&notices[0], Notice::Event(event)
                if event.code == code::HISTORY_INCOMPLETE && event.message == "page cap"
                && event.fields == vec![("command", Value::Num(15.0))]));
            assert!(
                matches!(&notices[1], Notice::CommandDone { command: 15, result } if result == "[1,2,3,4]")
            );
        } else {
            assert_eq!(notices.len(), 1);
            assert!(
                matches!(&notices[0], Notice::CommandFailed { command: 15, error }
                if error.kind == Some(ErrorKind::Internal) && error.message.contains("restarted"))
            );
        }
    }
}

#[test]
fn contextual_finalization_sees_only_accepted_rows_and_encoding_errors_stay_strict() {
    for body in ["finalizer-first", "finalizer-later", "finalizer-encoding"] {
        let notices = finalizer_case(body, false);
        if body == "finalizer-later" {
            assert_eq!(notices.len(), 2);
            assert!(matches!(&notices[0], Notice::Event(event)
                if event.code == code::HISTORY_INCOMPLETE
                && event.fields == vec![("command", Value::Num(44.0))]));
            assert!(
                matches!(&notices[1], Notice::CommandDone { command: 44, result } if result == "[1]")
            );
        } else {
            assert_eq!(notices.len(), 1);
            let kind = if body == "finalizer-first" {
                ErrorKind::Network
            } else {
                ErrorKind::Internal
            };
            assert!(
                matches!(&notices[0], Notice::CommandFailed { command: 44, error } if error.kind == Some(kind))
            );
        }
    }
}

#[test]
fn discarding_a_retry_page_preserves_prior_rows_coverage_cursor_and_identity() {
    let notices = finalizer_case("discard-page", true);
    assert_eq!(notices.len(), 2);
    assert!(matches!(&notices[0], Notice::Event(event)
        if event.code == code::HISTORY_INCOMPLETE && event.message == "first known gap"));
    assert!(matches!(&notices[1], Notice::CommandDone { result, .. } if result == "[1,2]"));
}

#[test]
fn discarding_after_stage_completion_poisoned_the_owner_instead_of_reopening_it() {
    let notices = finalizer_case("discard-after-finished", true);
    assert_eq!(notices.len(), 1);
    assert!(matches!(&notices[0], Notice::CommandFailed { error, .. }
        if error.kind == Some(ErrorKind::Internal) && error.message.contains("discarded after completion")));
}

#[test]
fn discarding_an_already_poisoned_page_cannot_recover_rows_or_a_marker() {
    let notices = finalizer_case("discard-after-poison", true);
    assert_eq!(notices.len(), 1);
    assert!(matches!(&notices[0], Notice::CommandFailed { error, .. }
        if error.kind == Some(ErrorKind::Internal) && error.message.contains("restarted before completion")));
}

#[test]
fn strict_pagination_rejection_drops_accepted_rows_and_stored_marker() {
    let notices = finalizer_case("finalizer-policy", true);
    assert_eq!(notices.len(), 1);
    assert!(
        matches!(&notices[0], Notice::CommandFailed { command: 44, error }
        if error.kind == Some(ErrorKind::InvalidInput) && error.message == "strict pagination policy")
    );
}

#[test]
fn contextual_cursor_finalization_never_runs_after_lifecycle_failure() {
    for body in ["finalizer-poison", "finalizer-active"] {
        let notices = finalizer_case(body, true);
        assert_eq!(notices.len(), 1);
        assert!(
            matches!(&notices[0], Notice::CommandFailed { command: 44, error }
            if error.kind == Some(ErrorKind::Internal))
        );
    }
}

#[test]
fn probe_misuse_cannot_return_unmarked_partial_query_rows() {
    for body in ["probe-staged", "probe-admitted"] {
        let notices = finalizer_case(body, true);
        assert_eq!(notices.len(), 1);
        assert!(
            matches!(&notices[0],Notice::CommandFailed {command:44,error}
            if error.kind==Some(ErrorKind::Internal) && error.message=="history probe cannot admit result rows")
        );
    }
}

fn finalizer_case(body: &str, cursor: bool) -> Vec<Notice> {
    let mut engine = support::seeded_engine(Fixture);
    engine.set_log_level(Level::Error);
    let query = if cursor {
        Query::Income {
            symbol: None,
            kind: Some("two-pages".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        }
    } else {
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        }
    };
    engine.handle(0., Event::Command(Command::Query { command: 44, query }));
    let request = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(notice) => Some(notice),
            _ => None,
        })
        .collect()
}
