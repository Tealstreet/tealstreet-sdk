//! G1 original Context-chain evidence; no fake command or short-root substitute.
#![cfg(all(feature = "test-support", feature = "http-plans"))]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    CommandEntryError, Context, Error, Operation, Policy, PreflightPolicy, PreflightPurpose,
    RetryPolicy, Scope,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Method, Notice, Response, SocketId, TimerId,
    Venue,
};
fn policy(timeout_ms: u32) -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
#[derive(Default)]
struct Cycle {
    root: Option<Context>,
    contexts: Vec<Context>,
    unrelated: Option<TimerId>,
    replies: usize,
    outside_refusals: usize,
    attempted: Option<Result<(), Error>>,
}
impl Cycle {
    fn command_denied(&mut self, cx: &mut Cx<'_, Context>) {
        assert!(matches!(
            cx.begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None,
            }),
            Err(CommandEntryError::Revoked)
        ));
        self.outside_refusals += 1;
    }
    fn issue(&mut self, cx: &mut Cx<'_, Context>, path: &str) -> Result<(), Error> {
        let context = cx.http_leg_context(self.root.as_ref().unwrap(), policy(20_000))?;
        self.contexts.push(context.clone());
        cx.request_managed(
            context.clone(),
            Method::Get,
            format!("https://background.test/{path}"),
            None,
            context,
        )
        .map_err(|(_, error)| error)?;
        Ok(())
    }
}
impl Venue for Cycle {
    type Pending = Context;
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        self.command_denied(cx);
        self.unrelated = Some(cx.set_timer(1.));
        // Current API can honestly span two 20s calls; no deliberately short root.
        self.root = Some(cx.http_context(Scope::Background, policy(40_000)).unwrap());
        self.issue(cx, "margin-mode").unwrap();
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        cx.validate_http_context(&context).unwrap();
        assert_eq!(response.body, "validated native settings");
        self.replies += 1;
        if self.replies == 1 {
            self.issue(cx, "position-mode").unwrap();
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Context>, timer: TimerId) {
        if self.unrelated == Some(timer) {
            self.command_denied(cx);
            // Adversarial callback has a live handle, not the task activation loan.
            self.attempted = Some(self.issue(cx, "unrelated-timer"));
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, message: &str) {
        panic!("{message}")
    }
    fn on_command(&mut self, _: &mut Cx<'_, Context>, _: Command) {}
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn start() -> (Engine<Cycle>, HttpRequest) {
    let mut engine = support::seeded_engine(Cycle::default());
    engine.start(0.);
    let first = requests(&mut engine, 0.);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].url, "https://background.test/margin-mode");
    (engine, first.into_iter().next().unwrap())
}
fn requests(engine: &mut Engine<Cycle>, now: f64) -> Vec<HttpRequest> {
    let actions = prepared_actions(engine, now);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
    actions
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect()
}
fn reply(engine: &mut Engine<Cycle>, request: &HttpRequest, now: f64) {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "validated native settings",
            },
        },
    );
}
#[test]
fn existing_context_chain_can_span_two_physical_windows_without_fake_entry() {
    let (mut engine, first) = start();
    assert_eq!(engine.venue().root.as_ref().unwrap().deadline(), 40_000.);
    assert_eq!(engine.venue().contexts[0].deadline(), 20_000.);
    reply(&mut engine, &first, 19_000.);
    let second = requests(&mut engine, 19_000.);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].url, "https://background.test/position-mode");
    assert_eq!(engine.venue().contexts[1].deadline(), 39_000.);
    reply(&mut engine, &second[0], 38_000.);
    assert!(requests(&mut engine, 38_000.).is_empty());
    assert_eq!(engine.venue().replies, 2);
    assert_eq!(engine.venue().outside_refusals, 1);
}
#[test]
#[ignore = "G1 background Plan runtime adoption: codex-zs5e83, feat/rust-background-plan-admission"]
fn original_red_unrelated_timer_cannot_borrow_background_cycle_custody() {
    let (mut engine, _) = start();
    let timer = engine.venue().unrelated.unwrap();
    engine.handle(1., Event::TimerFired { timer });
    let extra = requests(&mut engine, 1.);
    assert_eq!(engine.venue().outside_refusals, 2);
    assert!(
        extra.is_empty(),
        "a live background handle is not an exact task activation loan"
    );
    assert_eq!(engine.venue().attempted, Some(Err(Error::Revoked)));
}
#[test]
#[ignore = "G1 background Plan runtime adoption: codex-zs5e83, feat/rust-background-plan-admission"]
fn original_red_completed_two_call_cycle_cannot_release_an_undeclared_third_call() {
    let (mut engine, first) = start();
    reply(&mut engine, &first, 19_000.);
    let second = requests(&mut engine, 19_000.);
    reply(&mut engine, &second[0], 38_000.);
    assert!(requests(&mut engine, 38_000.).is_empty());
    let timer = engine.venue().unrelated.unwrap();
    engine.handle(38_001., Event::TimerFired { timer });
    assert!(
        requests(&mut engine, 38_001.).is_empty(),
        "two declared calls exhaust the finite cycle; no third structural credit"
    );
}
