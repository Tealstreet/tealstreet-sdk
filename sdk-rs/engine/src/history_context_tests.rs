//! Core admission tests, deliberately separate from public constructor binding.
//! The private fixture wraps only the first root just issued at command entry.
//! Real managed prepare/bind/claim and delivery checks remain in force.
use super::*;
use crate::book::Book;
use crate::request_context::{
    Claim, HttpAction, Operation, Policy, Prepared, RetryEvidence, RetryPolicy, Scope,
};
use crate::{
    Action, Command, Engine, Event, Method, Notice, Query, RequestId, Response, SocketId, TimerId,
    Venue,
};
use std::cell::Cell;

struct Pending {
    root: Context,
    leg: Context,
    wrong_leg: Context,
    history: ReadCursorHistory<String>,
    duplicate: ReadCompletion,
    profile: String,
    second: bool,
    range_probe: bool,
    range: Option<ReadRangeHistory<String>>,
    rows: Option<ReadHistoryRows<String>>,
}
#[derive(Default)]
struct Fixture {
    timer: Option<Pending>,
    retired: Option<ReadCompletion>,
    held_root: Option<Context>,
    foreign: Option<Context>,
}
fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 100,
        pacing: None,
        retry: RetryPolicy::ReadFailure,
    }
}
fn encode(rows: &[String]) -> Result<String, VenueError> {
    Ok(format!("[{}]", rows.join(",")))
}
fn page(next: Option<&str>) -> CursorPage {
    CursorPage {
        next: next.map(str::to_owned),
        rows: 1,
        fresh: 1,
        want_more: true,
    }
}
// Test-private authority construction is NOT a public constructor proof. The
// caller supplies only its immediately-issued, genuine first command Read root.
fn first_root_owner(command: u32, root: Context) -> ReadCompletion {
    Completion {
        command,
        incomplete: None,
        authority: ReadAuthority {
            root: Box::new(root),
            page: None,
            identities: PageIdentities::default(),
        },
    }
}
fn queue(cx: &mut Cx<'_, Pending>, pending: Pending) {
    cx.request_managed(
        pending.leg.clone(),
        Method::Get,
        "https://history.test/read".into(),
        None,
        pending,
    )
    .unwrap_or_else(|_| panic!("managed fixture must queue"));
}
fn fail_after_page(cx: &mut Cx<'_, Pending>, pending: Pending) {
    pending
        .history
        .finish(cx, Err(VenueError::new("later page failed")))
        .unwrap();
}
impl Venue for Fixture {
    type Pending = Pending;
    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, _: TimerId) {
        let mut pending = self.timer.take().unwrap();
        assert!(
            pending.history.validate_delivery(cx, &pending.leg).is_err(),
            "timer cannot borrow a previous physical delivery"
        );
        // The completion root remains logical authority at a timer, even
        // after its original physical issuance bound expires. It grants no data.
        let owner = ReadCompletion::from_context(cx, 7, pending.root.clone()).unwrap();
        assert!(owner.validate_delivery(cx, &pending.leg).is_err());
        if pending.profile == "live-timer" {
            pending.leg = cx.http_leg_context(&pending.root, policy()).unwrap();
            pending.second = true;
            queue(cx, pending);
        } else {
            assert!(
                cx.http_leg_context(&pending.root, policy()).is_err(),
                "an expired root's timer cannot issue the next leg"
            );
            pending.history.finish(cx, Ok(())).unwrap();
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending, _: &str) {
        // A native failed leg also has exact managed delivery authority, but
        // admitting an empty transition must not rescue its streamed prefix.
        if pending.profile == "expired" {
            assert!(
                pending.history.admit(cx, &pending.leg).is_err(),
                "expired physical legs cannot admit response data"
            );
        } else {
            pending.history.admit(cx, &pending.leg).unwrap();
        }
        if pending.profile == "transport-staged" {
            pending
                .history
                .admit(cx, &pending.leg)
                .unwrap()
                .stage("999".into());
            // Even a codec mistakenly retaining a failed callback's prefix
            // cannot expose it through accepted-only recovery.
        } else {
            assert!(pending.history.discard_staged_page());
        }
        fail_after_page(cx, pending);
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Command::Query {
            command,
            query: Query::Income { kind, .. },
        } = command
        else {
            return;
        };
        let root = if matches!(
            kind.as_deref(),
            Some("constructor" | "constructor-mixed" | "capture-root")
        ) {
            cx.admit_http_command(policy()).unwrap()
        } else {
            cx.http_context(Scope::Command(command), policy()).unwrap()
        };
        if let Some(mut retired) = self.retired.take() {
            assert_ne!(
                retired.root().owner(),
                root.owner(),
                "reused id must have a fresh generation"
            );
            assert!(
                ReadCompletion::from_context(cx, command, retired.root().clone()).is_err(),
                "constructor cannot bind an old generation to the reused command"
            );
            retired.incomplete("old generation must stay silent");
            assert!(retired.finish(cx, Ok("[999]".into())).is_err());
        }
        if kind.as_deref() == Some("capture-root") {
            self.held_root = Some(root);
            return;
        }
        if matches!(kind.as_deref(), Some("constructor" | "constructor-mixed")) {
            assert!(matches!(
                ReadCompletion::from_context(cx, command + 1, root.clone()),
                Err(ContextError::Invalid)
            ));
            let child = cx.http_leg_context(&root, policy()).unwrap();
            assert!(ReadCompletion::from_context(cx, command, child).is_err());
            let background = cx.http_context(Scope::Background, policy()).unwrap();
            assert!(ReadCompletion::from_context(cx, command, background).is_err());
            if let Some(foreign) = &self.foreign {
                assert!(ReadCompletion::from_context(cx, command, foreign.clone()).is_err());
            }
            if kind.as_deref() == Some("constructor-mixed") {
                let mut mutation_policy = policy();
                mutation_policy.operation = Operation::Mutation;
                mutation_policy.retry = RetryPolicy::Never;
                let mutation = cx
                    .http_context(Scope::Command(command), mutation_policy)
                    .unwrap();
                assert!(ReadCompletion::from_context(cx, command, mutation.clone()).is_err());
                assert!(ReadCompletion::from_context(cx, command, root).is_err());
                cx.fail_context(&mutation, VenueError::new("proper mutation owner"))
                    .unwrap();
            } else {
                let owner = ReadCompletion::from_context(cx, command, root.clone()).unwrap();
                let duplicate = ReadCompletion::from_context(cx, command, root.clone()).unwrap();
                owner.finish(cx, Ok("[]".into())).unwrap();
                assert!(ReadCompletion::from_context(cx, command, root).is_err());
                assert!(duplicate.finish(cx, Ok("[999]".into())).is_err());
            }
            return;
        }
        let owner = first_root_owner(command, root.clone());
        if kind.as_deref() == Some("early-reject") {
            owner
                .finish(
                    cx,
                    Err(VenueError::new("invalid caller input").kind(ErrorKind::InvalidInput)),
                )
                .unwrap();
            return;
        }
        if kind.as_deref() == Some("early-empty") {
            owner.finish(cx, Ok("[]".into())).unwrap();
            return;
        }
        let duplicate = first_root_owner(command, root.clone());
        let leg = cx.http_leg_context(&root, policy()).unwrap();
        let wrong_leg = cx.http_leg_context(&root, policy()).unwrap();
        assert!(
            cx.read_http_delivery_attempt(&root, &leg).is_err(),
            "command entry has no physical delivery"
        );
        let history = CursorHistory::new(
            owner,
            CursorWalk::new(4),
            |_| "native page budget".into(),
            encode,
        );
        let range = kind
            .as_deref()
            .filter(|kind| kind.starts_with("range"))
            .map(|_| {
                let mut history = RangeHistory::new(
                    first_root_owner(command, root.clone()),
                    RangeWalk::new(0., 10., 10., 2),
                    |_| "walk gap".into(),
                    encode,
                );
                assert!(matches!(history.step(), Step::Fetch { .. }));
                history
            });
        let rows = (kind.as_deref() == Some("rows"))
            .then(|| HistoryRows::new(first_root_owner(command, root.clone()), encode));
        queue(
            cx,
            Pending {
                root,
                leg,
                wrong_leg,
                history,
                duplicate,
                profile: kind.unwrap(),
                second: false,
                range_probe: false,
                range,
                rows,
            },
        );
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending, _: Response<'_>) {
        assert!(
            pending.history.admit(cx, &pending.wrong_leg).is_err(),
            "a live sibling is not the current delivery"
        );
        if let Some(mut range) = pending.range.take() {
            if !pending.second {
                {
                    let mut admission = range.admit(cx, &pending.leg).unwrap();
                    for (value, time) in [("1", 0.), ("2", 10.)] {
                        admission.stage(
                            value.into(),
                            PageRow {
                                timestamp: time,
                                identity: value.into(),
                            },
                        );
                    }
                    if pending.profile == "range-filtered" {
                        admission.accept_page_where(|_| false);
                    } else {
                        admission.accept_page_with_coverage(Some("first range gap".into()));
                    }
                }
                assert!(
                    matches!(range.step(), Step::Fetch { .. }),
                    "inclusive boundary probe"
                );
                pending.second = true;
                pending.range_probe = true;
            } else if pending.range_probe {
                // A distinct physical empty response closes the inclusive probe.
                range.admit(cx, &pending.leg).unwrap().accept_page();
                assert_eq!(range.step(), Step::Finished);
                assert!(range.restart_stage(RangeWalk::new(10., 20., 10., 2)));
                assert!(matches!(range.step(), Step::Fetch { .. }));
                pending.range_probe = false;
            } else {
                range.admit(cx, &pending.leg).unwrap().stage(
                    "999".into(),
                    PageRow {
                        timestamp: 11.,
                        identity: "failed".into(),
                    },
                );
                range
                    .finish(cx, Err(VenueError::new("later range failed")))
                    .unwrap();
                return;
            }
            pending.range = Some(range);
            pending.leg = cx.http_leg_context(&pending.root, policy()).unwrap();
            queue(cx, pending);
            return;
        }
        if let Some(mut rows) = pending.rows.take() {
            rows.admit(cx, &pending.leg)
                .unwrap()
                .stage(if pending.second { "999" } else { "1" }.into());
            if pending.second {
                rows.finish(cx, Err(VenueError::new("later rows failed")))
                    .unwrap();
            } else {
                rows.admit(cx, &pending.leg).unwrap().accept_page();
                pending.rows = Some(rows);
                pending.second = true;
                pending.leg = cx.http_leg_context(&pending.root, policy()).unwrap();
                queue(cx, pending);
            }
            return;
        }
        if !pending.second {
            if pending.profile == "first-failure" {
                pending
                    .history
                    .admit(cx, &pending.leg)
                    .unwrap()
                    .stage("999".into());
                fail_after_page(cx, pending);
                return;
            }
            if matches!(
                pending.profile.as_str(),
                "retry" | "binding" | "physical-retry" | "physical-binding"
            ) {
                {
                    let mut admission = pending.history.admit(cx, &pending.leg).unwrap();
                    assert!(admission.fresh("same-id".into()));
                    assert!(admission.fresh("failed-only".into()));
                    admission.stage("999".into());
                }
                if !matches!(pending.profile.as_str(), "binding" | "physical-binding") {
                    assert!(pending.history.discard_staged_page());
                }
            } else {
                {
                    let mut admission = pending.history.admit(cx, &pending.leg).unwrap();
                    assert!(admission.fresh("accepted".into()));
                    admission.stage("1".into());
                }
                // Runtime pin for drop -> mutable diagnostic -> re-admit.
                if pending.profile == "diagnostic" {
                    cx.event(Level::Warn, "row.skipped", "native diagnostic", vec![]);
                }
                let next = if pending.profile == "timer" {
                    None
                } else {
                    Some("next")
                };
                pending
                    .history
                    .admit(cx, &pending.leg)
                    .unwrap()
                    .advance(page(next));
            }
            if matches!(pending.profile.as_str(), "timer" | "live-timer") {
                cx.set_timer(if pending.profile == "timer" {
                    120.
                } else {
                    10.
                });
                self.timer = Some(pending);
                return;
            }
            pending.second = true;
            pending.leg = if matches!(
                pending.profile.as_str(),
                "physical-retry" | "physical-binding"
            ) {
                let retry = cx
                    .http_retry_context(&pending.leg, RetryEvidence::ReadFailure)
                    .unwrap();
                assert_eq!(
                    retry, pending.leg,
                    "retry preserves the logical leg and raw deadline"
                );
                retry
            } else {
                cx.http_leg_context(&pending.root, policy()).unwrap()
            };
            queue(cx, pending);
            return;
        }
        if matches!(pending.profile.as_str(), "binding" | "physical-binding") {
            let before = (
                pending.history.accepted_len(),
                pending.history.staged_len(),
                pending.history.pages(),
                pending.history.cursor().map(str::to_owned),
                pending
                    .history
                    .rows
                    .completion
                    .authority
                    .identities
                    .0
                    .clone(),
                pending.history.rows.completion.authority.page.clone(),
            );
            assert!(
                matches!(
                    pending.history.admit(cx, &pending.leg),
                    Err(ContextError::State)
                ),
                "a different physical attempt or sibling cannot admit a previous staged page"
            );
            let after = (
                pending.history.accepted_len(),
                pending.history.staged_len(),
                pending.history.pages(),
                pending.history.cursor().map(str::to_owned),
                pending
                    .history
                    .rows
                    .completion
                    .authority
                    .identities
                    .0
                    .clone(),
                pending.history.rows.completion.authority.page.clone(),
            );
            assert_eq!(
                before, after,
                "denial leaves rows, ids, cursor and binding unchanged"
            );
            assert!(pending.history.discard_staged_page());
        }
        if matches!(
            pending.profile.as_str(),
            "retry" | "binding" | "physical-retry" | "physical-binding"
        ) {
            assert!(!pending.history.walk.contains_identity("failed-only"));
            let mut admission = pending.history.admit(cx, &pending.leg).unwrap();
            assert!(
                admission.fresh("same-id".into()),
                "discard rolls back unadmitted identity"
            );
            admission.stage("1".into());
            assert_eq!(admission.advance(page(None)), CursorStep::Done);
            assert!(!pending.history.walk.contains_identity("failed-only"));
            assert!(pending.history.rows.completion.authority.page.is_none());
            pending.history.finish(cx, Ok(())).unwrap();
            return;
        }
        if pending.profile.starts_with("mixed") {
            let mut mutation_policy = policy();
            mutation_policy.operation = Operation::Mutation;
            mutation_policy.retry = RetryPolicy::Never;
            let mutation = cx.http_context(Scope::Command(7), mutation_policy).unwrap();
            assert!(pending.history.admit(cx, &pending.leg).is_err());
            let called = Cell::new(false);
            let result = if pending.profile == "mixed-failure" {
                pending
                    .history
                    .reject(cx, VenueError::new("must not finish the write"))
            } else {
                pending
                    .history
                    .finish_with(cx, Err(VenueError::new("native failed")), |_| {
                        called.set(true);
                        Ok("[999]".into())
                    })
            };
            assert!(result.is_err());
            assert!(
                !called.get(),
                "denied owner cannot run the captured encoder/finalizer"
            );
            cx.fail_context(&mutation, VenueError::new("proper mutation owner"))
                .unwrap();
            return;
        }
        pending
            .history
            .admit(cx, &pending.leg)
            .unwrap()
            .stage("999".into());
        if pending.profile == "reject" {
            pending
                .history
                .reject(cx, VenueError::new("strict preparation refusal"))
                .unwrap();
        } else if pending.profile == "encode-error" {
            pending
                .history
                .finish_with(cx, Err(VenueError::new("native failed")), |_| {
                    Err(VenueError::new("encoder failed"))
                })
                .unwrap();
        } else {
            let Pending {
                history,
                mut duplicate,
                ..
            } = pending;
            history
                .finish(cx, Err(VenueError::new("later page failed")))
                .unwrap();
            if pending.profile == "reuse" {
                self.retired = Some(duplicate);
            } else {
                duplicate.incomplete("duplicate must stay silent");
                assert!(duplicate.finish(cx, Ok("[999]".into())).is_err());
            }
        }
    }
}
fn engine_with(fixture: Fixture, seed: u64) -> Engine<Fixture> {
    fn source(mut state: u64) -> crate::entropy::Fill {
        Box::new(move |bytes| {
            for byte in bytes {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state as u8;
            }
            Ok(())
        })
    }
    let entropy = crate::entropy::Entropy::new(source(13 + seed), source(29 + seed));
    let mut engine = Engine::new(fixture, Some(entropy)).unwrap();
    engine.set_log_level(Level::Error);
    engine
}
fn engine() -> Engine<Fixture> {
    engine_with(Fixture::default(), 0)
}
fn query(profile: &str) -> Event<'_> {
    Event::Command(Command::Query {
        command: 7,
        query: Query::Income {
            symbol: None,
            kind: Some(profile.into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    })
}
fn prepare(engine: &mut Engine<Fixture>, now: f64) -> RequestId {
    let preparation = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("real managed preparation");
    let request = preparation.attempt().request;
    engine.http_prepared(
        now,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let permit = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .expect("real managed binding");
    assert!(matches!(
        engine.claim_http(now + 1., permit),
        Ok(Claim::Released(_))
    ));
    engine.drain_actions().for_each(drop);
    request
}
fn deliver(engine: &mut Engine<Fixture>, request: RequestId, now: f64) {
    engine.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "[]",
            },
        },
    );
}
fn notices(engine: &mut Engine<Fixture>) -> Vec<Notice> {
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(notice) => Some(notice),
            _ => None,
        })
        .collect()
}
fn run(profile: &str) -> Vec<Notice> {
    let mut engine = engine();
    engine.handle(0., query(profile));
    let request = prepare(&mut engine, 1.);
    deliver(&mut engine, request, 3.);
    if profile == "first-failure" {
        return notices(&mut engine);
    }
    let request = prepare(&mut engine, 4.);
    deliver(&mut engine, request, 6.);
    if profile.starts_with("range") {
        let request = prepare(&mut engine, 7.);
        deliver(&mut engine, request, 9.);
    }
    notices(&mut engine)
}
fn assert_done(notices: &[Notice]) {
    assert!(
        matches!(notices.last(), Some(Notice::CommandDone { command: 7, result }) if result == "[1]"),
        "{notices:?}"
    );
}
#[test]
fn genuine_two_page_delivery_returns_only_accepted_rows() {
    let notices = run("partial");
    assert_eq!(notices.len(), 2);
    assert!(
        matches!(&notices[0], Notice::Event(event) if event.code == code::HISTORY_INCOMPLETE && event.level == Level::Warn
                && event.message.contains("later page failed")
                && event.fields == vec![("command", Value::Num(7.))])
    );
    assert_done(&notices);
}
#[test]
fn failed_prefix_identity_is_reusable_by_a_different_delivered_leg() {
    for profile in ["retry", "binding"] {
        let notices = run(profile);
        assert_eq!(notices.len(), 1, "{profile}: {notices:?}");
        assert_done(&notices);
    }
}
#[test]
fn first_failure_and_strict_refusal_never_publish_partial_or_marker() {
    for (profile, message) in [
        ("first-failure", "later page failed"),
        ("reject", "strict preparation refusal"),
        ("encode-error", "encoder failed"),
    ] {
        let notices = run(profile);
        assert_eq!(notices.len(), 1, "{profile}: {notices:?}");
        assert!(
            matches!(&notices[0], Notice::CommandFailed { command: 7, error }
            if error.message == message && error.kind == Some(ErrorKind::Internal))
        );
    }
}
#[test]
fn mutation_sibling_denies_both_failure_and_captured_finalizer() {
    for profile in ["mixed-failure", "mixed-finalizer"] {
        let notices = run(profile);
        assert_eq!(notices.len(), 1, "{profile}: {notices:?}");
        assert!(
            matches!(&notices[0], Notice::CommandFailed { command: 7, error }
            if error.message == "proper mutation owner")
        );
    }
}
#[test]
fn guard_can_drop_for_diagnostics_then_readmit_the_same_delivery() {
    // Error log filtering may hide the ordinary diagnostic; the owned history
    // incomplete remains Warn and precedes the context terminal.
    let notices = run("diagnostic");
    assert_eq!(notices.len(), 2);
    assert_done(&notices);
}

#[test]
fn failure_callback_has_delivery_but_does_not_admit_failed_rows() {
    for profile in ["partial", "transport-staged"] {
        let mut engine = engine();
        engine.handle(0., query(profile));
        let request = prepare(&mut engine, 1.);
        deliver(&mut engine, request, 3.);
        let request = prepare(&mut engine, 4.);
        engine.handle(
            6.,
            Event::HttpFailed {
                request,
                error: "transport failed",
            },
        );
        let notices = notices(&mut engine);
        assert_eq!(notices.len(), 2);
        assert_done(&notices);
    }
}

#[test]
fn timer_has_no_delivery_but_preserves_logical_terminal_authority() {
    let mut engine = engine();
    engine.handle(0., query("timer"));
    let request = prepare(&mut engine, 1.);
    deliver(&mut engine, request, 3.);
    let timer = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .expect("continuation timer");
    engine.handle(123., Event::TimerFired { timer });
    let notices = notices(&mut engine);
    assert_eq!(notices.len(), 1);
    assert_done(&notices);
}

#[test]
fn range_stages_keep_first_gap_and_exclude_failed_second_stage() {
    let notices = run("range");
    assert_eq!(notices.len(), 2);
    assert!(matches!(&notices[0], Notice::Event(event)
        if event.code == code::HISTORY_INCOMPLETE && event.message == "first range gap"));
    assert!(matches!(&notices[1], Notice::CommandDone { command: 7, result } if result == "[1,2]"));
}
#[test]
fn filtered_range_rows_cannot_authorize_later_failure_recovery() {
    let notices = run("range-filtered");
    assert_eq!(notices.len(), 1);
    assert!(
        matches!(&notices[0], Notice::CommandFailed { command: 7, error }
        if error.message == "later range failed" && error.kind == Some(ErrorKind::Internal))
    );
}
#[test]
fn rows_admission_accepts_only_validated_page() {
    let notices = run("rows");
    assert_eq!(notices.len(), 2);
    assert_done(&notices);
}
#[test]
fn expired_physical_delivery_cannot_admit_but_can_report_retained_partial() {
    let mut engine = engine();
    engine.handle(0., query("expired"));
    let request = prepare(&mut engine, 1.);
    deliver(&mut engine, request, 3.);
    let request = prepare(&mut engine, 4.);
    deliver(&mut engine, request, 101.);
    let notices = notices(&mut engine);
    assert_eq!(notices.len(), 2);
    assert_done(&notices);
}

#[test]
fn pre_request_input_refusal_and_keyless_empty_need_no_delivery() {
    for profile in ["early-reject", "early-empty"] {
        let mut engine = engine();
        engine.handle(0., query(profile));
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_eq!(actions.len(), 1, "no request, signer or companion action");
        if profile == "early-reject" {
            assert!(
                matches!(&actions[0], Action::Notice(Notice::CommandFailed { command: 7, error })
                if error.message == "invalid caller input" && error.kind == Some(ErrorKind::InvalidInput))
            );
        } else {
            assert!(
                matches!(&actions[0], Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[]")
            );
        }
    }
}
#[test]
fn a_reused_numeric_command_cannot_revive_old_generation_completion() {
    let mut engine = engine();
    engine.handle(0., query("reuse"));
    let request = prepare(&mut engine, 1.);
    deliver(&mut engine, request, 3.);
    let request = prepare(&mut engine, 4.);
    deliver(&mut engine, request, 6.);
    assert_done(&notices(&mut engine));
    engine.handle(7., query("early-empty"));
    let notices = notices(&mut engine);
    assert_eq!(notices.len(), 1);
    assert!(matches!(&notices[0], Notice::CommandDone { command: 7, result } if result == "[]"));
}

#[test]
fn a_new_physical_attempt_on_the_same_leg_drops_failed_identity_staging() {
    let notices = run("physical-retry");
    assert_eq!(notices.len(), 1);
    assert_done(&notices);
}

#[test]
fn new_physical_attempt_must_not_admit_the_previous_attempts_staged_page() {
    let notices = run("physical-binding");
    assert_eq!(notices.len(), 1);
    assert_done(&notices);
}

#[test]
fn public_constructor_binds_only_the_matching_live_original_read_root() {
    let mut foreign_engine = engine_with(Fixture::default(), 100);
    foreign_engine.handle(0., query("capture-root"));
    let foreign = foreign_engine.venue().held_root.clone().unwrap();
    let mut engine = engine_with(
        Fixture {
            foreign: Some(foreign),
            ..Fixture::default()
        },
        0,
    );
    engine.handle(0., query("constructor"));
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(actions.len(), 1, "constructor checks emit no I/O or marker");
    assert!(
        matches!(&actions[0], Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[]")
    );
}
#[test]
fn public_constructor_refuses_mutation_root_and_read_root_with_mutation_sibling() {
    let mut engine = engine();
    engine.handle(0., query("constructor-mixed"));
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(actions.len(), 1);
    assert!(
        matches!(&actions[0], Action::Notice(Notice::CommandFailed { command: 7, error })
        if error.message == "proper mutation owner")
    );
}

#[test]
fn a_live_logical_timer_can_issue_the_next_leg_but_cannot_admit_data() {
    let mut engine = engine();
    engine.handle(0., query("live-timer"));
    let request = prepare(&mut engine, 1.);
    deliver(&mut engine, request, 3.);
    let timer = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    engine.handle(13., Event::TimerFired { timer });
    let request = prepare(&mut engine, 14.);
    deliver(&mut engine, request, 16.);
    let notices = notices(&mut engine);
    assert_eq!(notices.len(), 2);
    assert_done(&notices);
}
