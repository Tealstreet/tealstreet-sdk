mod support;

use safe_cex_engine::book::Book;
use safe_cex_engine::history::{HistoryRows, ReadCompletion, ReadHistoryRows};
use safe_cex_engine::log::code;
use safe_cex_engine::request_context::{
    Claim, Context, HttpAction, Operation, Policy, Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Level, Method, Notice, Query, RequestId, Response,
    SocketId, TimerId, Value, Venue, VenueError,
};

struct Pending {
    root: Context,
    leg: Context,
    rows: ReadHistoryRows<String>,
    duplicate: ReadCompletion,
    profile: String,
    mutation: Option<Context>,
}

struct Fixture;

fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 100,
        pacing: None,
        retry: RetryPolicy::ReadFailure,
    }
}

// Corrected construction/admission seam. The original committed controls used
// numeric Completion here, while retaining the same genuine root and delivery.
fn owner(cx: &Cx<'_, Pending>, command: u32, context: Context) -> ReadCompletion {
    ReadCompletion::from_context(cx, command, context).expect("matching original Read root")
}

impl Venue for Fixture {
    type Pending = Pending;
    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
        panic!("fixture expects a delivered managed response");
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
        let root = cx.http_context(Scope::Command(command), policy()).unwrap();
        let completion = owner(cx, command, root.clone());
        let duplicate = owner(cx, command, root.clone());
        let profile = kind.unwrap();
        let mutation = if profile == "mixed" {
            let mut mutation_policy = policy();
            mutation_policy.operation = Operation::Mutation;
            mutation_policy.retry = RetryPolicy::Never;
            Some(
                cx.http_context(Scope::Command(command), mutation_policy)
                    .unwrap(),
            )
        } else {
            None
        };
        let leg = cx.http_leg_context(&root, policy()).unwrap();
        assert_eq!(leg.deadline(), root.deadline());
        assert_eq!(leg.owner(), root.owner());
        let pending = Pending {
            root: root.clone(),
            leg: leg.clone(),
            rows: HistoryRows::new(completion, |rows| Ok(format!("[{}]", rows.join(",")))),
            duplicate,
            profile,
            mutation,
        };
        cx.request_managed(
            leg,
            Method::Get,
            "https://history.test/read".into(),
            None,
            pending,
        )
        .unwrap_or_else(|_| panic!("valid public managed read must queue"));
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending, _: Response<'_>) {
        if pending.mutation.is_none() {
            cx.validate_read_http_delivery(&pending.root, &pending.leg)
                .expect("fixture uses the exact live original root/current managed leg");
        }
        if let Some(mutation) = pending.mutation {
            assert!(pending.rows.admit(cx, &pending.leg).is_err());
            assert!(
                pending
                    .rows
                    .finish(cx, Err(VenueError::new("later page failed")))
                    .is_err()
            );
            // A read cannot consume the sibling write's journal. Its proper
            // owner must still be able to issue the sole terminal failure.
            cx.fail_context(&mutation, VenueError::new("proper mutation owner"))
                .expect("history must leave mixed write authority untouched");
            return;
        }
        {
            let mut page = pending.rows.admit(cx, &pending.leg).unwrap();
            page.stage("1".into());
            page.accept_page();
            page.stage("999".into());
        }
        pending
            .rows
            .finish(cx, Err(VenueError::new("later page failed")))
            .unwrap();
        if pending.profile == "duplicate" {
            pending.duplicate.incomplete("duplicate must stay silent");
            assert!(pending.duplicate.finish(cx, Ok("[999]".into())).is_err());
        }
    }
}

fn query(command: u32, profile: &str) -> Event<'_> {
    Event::Command(Command::Query {
        command,
        query: Query::Income {
            symbol: None,
            kind: Some(profile.into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    })
}

fn prepare(engine: &mut Engine<Fixture>) -> RequestId {
    let token = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("real managed preparation");
    let request = token.attempt().request;
    engine.http_prepared(
        1.,
        token,
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
        .expect("prepared public packet");
    assert!(matches!(
        engine.claim_http(2., permit),
        Ok(Claim::Released(_))
    ));
    engine.drain_actions().for_each(drop);
    request
}

fn response(engine: &mut Engine<Fixture>, request: RequestId) -> Vec<Notice> {
    engine.handle(
        3.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "[]",
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

fn assert_partial(notices: &[Notice]) {
    assert_eq!(notices.len(), 2, "one companion followed by one terminal");
    assert!(matches!(&notices[0], Notice::Event(event)
        if event.code == code::HISTORY_INCOMPLETE && event.level == Level::Warn
        && event.message.contains("later page failed")
        && event.fields == vec![("command", Value::Num(7.))]));
    assert!(matches!(&notices[1], Notice::CommandDone { command: 7, result } if result == "[1]"));
}

#[test]
fn managed_delivery_keeps_owner_completion_and_excludes_failed_prefix() {
    let mut engine = support::seeded_engine(Fixture);
    engine.set_log_level(Level::Error);
    engine.handle(0., query(7, "partial"));
    let request = prepare(&mut engine);
    assert_partial(&response(&mut engine, request));
}

#[test]
fn cloned_authority_cannot_emit_a_second_marker_or_terminal() {
    let mut engine = support::seeded_engine(Fixture);
    engine.set_log_level(Level::Error);
    engine.handle(0., query(7, "duplicate"));
    let request = prepare(&mut engine);
    assert_partial(&response(&mut engine, request));
}

#[test]
fn a_later_sibling_mutation_forbids_read_companion_and_preserves_write_authority() {
    let mut engine = support::seeded_engine(Fixture);
    engine.set_log_level(Level::Error);
    engine.handle(0., query(7, "mixed"));
    let request = prepare(&mut engine);
    let notices = response(&mut engine, request);
    assert_eq!(
        notices.len(),
        1,
        "only the proper mutation owner may finish"
    );
    assert!(
        matches!(&notices[0], Notice::CommandFailed { command: 7, error }
        if error.message == "proper mutation owner")
    );
}
