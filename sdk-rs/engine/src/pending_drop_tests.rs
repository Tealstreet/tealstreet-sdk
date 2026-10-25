//! Pending drop timing (docs/plans/RUST_ENGINE_PLAN_CORE.md, S1 review pins):
//! a venue pending drops exactly where it did before the core split, once.
use super::*;
use crate::request_context::{
    Context, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy, Scope,
};
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<String>>>;

struct Tracked {
    tag: &'static str,
    log: Log,
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.log.borrow_mut().push(format!("drop:{}", self.tag));
    }
}

struct Dropper {
    log: Log,
    /// Fail the other read's context from the first response.
    finish_on_first: bool,
    contexts: Vec<Context>,
}

impl Venue for Dropper {
    type Pending = Tracked;
    fn start(&mut self, cx: &mut Cx<'_, Tracked>) {
        for tag in ["A", "B"] {
            let context = cx
                .http_context(
                    Scope::Command(7),
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 1_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            self.contexts.push(context.clone());
            let pending = Tracked {
                tag,
                log: self.log.clone(),
            };
            cx.request_managed(
                context,
                None,
                Method::Get,
                "https://venue.test/read".into(),
                None,
                pending,
            )
            .map_err(|(_, error)| error)
            .unwrap();
        }
    }
    fn on_command(&mut self, _: &mut Cx<'_, Tracked>, _: Command) {}
    fn on_response(&mut self, cx: &mut Cx<'_, Tracked>, pending: Tracked, _: Response<'_>) {
        self.log
            .borrow_mut()
            .push(format!("response:{}", pending.tag));
        if self.finish_on_first {
            self.finish_on_first = false;
            let other = self.contexts[1].clone();
            cx.fail_context(&other, "sibling failed").unwrap();
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Tracked>, pending: Tracked, _: &str) {
        self.log
            .borrow_mut()
            .push(format!("failed:{}", pending.tag));
    }
    fn on_open(&mut self, _: &mut Cx<'_, Tracked>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Tracked>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Tracked>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Tracked>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&crate::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn started(finish_on_first: bool) -> (Engine<Dropper>, Log, Vec<Preparation>) {
    let log: Log = Rc::default();
    let mut e = crate::test_support::seeded_engine(Dropper {
        log: log.clone(),
        finish_on_first,
        contexts: Vec::new(),
    });
    e.start(0.);
    let tokens = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(tokens.len(), 2);
    (e, log, tokens)
}

fn release(e: &mut Engine<Dropper>, token: Preparation, at: f64) {
    e.http_prepared(
        at,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .expect("bind");
    e.claim_http(at, permit).unwrap();
}

fn respond(e: &mut Engine<Dropper>, token: Preparation, at: f64) {
    e.handle(
        at,
        Event::HttpResponse {
            request: token.attempt().request,
            response: Response {
                status: 200,
                headers: "",
                body: "[]",
            },
        },
    );
    e.drain_actions().for_each(drop);
}

fn drops(log: &Log, tag: &str) -> usize {
    log.borrow()
        .iter()
        .filter(|entry| *entry == &format!("drop:{tag}"))
        .count()
}

#[test]
fn a_delivered_pending_drops_after_its_callback_and_its_sibling_stays_alive() {
    let (mut e, log, tokens) = started(false);
    release(&mut e, tokens[0], 1.);
    release(&mut e, tokens[1], 1.);
    respond(&mut e, tokens[0], 2.);
    assert_eq!(*log.borrow(), ["response:A", "drop:A"]);
    respond(&mut e, tokens[1], 3.);
    assert_eq!(
        *log.borrow(),
        ["response:A", "drop:A", "response:B", "drop:B"]
    );
}

#[test]
fn a_failed_sibling_drops_inside_the_failing_callback_before_the_delivered_one() {
    let (mut e, log, tokens) = started(true);
    release(&mut e, tokens[0], 1.);
    release(&mut e, tokens[1], 1.);
    respond(&mut e, tokens[0], 2.);
    assert_eq!(*log.borrow(), ["response:A", "drop:B", "drop:A"]);
    // A late reply for the retired read reaches no callback and drops nothing.
    respond(&mut e, tokens[1], 3.);
    assert_eq!(*log.borrow(), ["response:A", "drop:B", "drop:A"]);
}

#[test]
fn every_pending_drops_exactly_once_including_at_engine_teardown() {
    let (mut e, log, tokens) = started(false);
    release(&mut e, tokens[0], 1.);
    respond(&mut e, tokens[0], 2.);
    drop(e);
    assert_eq!(drops(&log, "A"), 1);
    assert_eq!(drops(&log, "B"), 1);
}
