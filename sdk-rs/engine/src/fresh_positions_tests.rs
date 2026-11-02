//! The waiter contract through a real engine: a Background read's delivery
//! settles the command; a venue without the capability never sees it.
use crate::book::Book;
use crate::engine::{Action, Command, Cx, Engine, Event, Notice, Venue, VenueAction};
use crate::fresh_positions::FreshPositions;
use crate::request_context::{Operation, Owner, Policy, RetryPolicy, Scope};
use crate::{ErrorKind, Method, Outcome, RequestId, Response, SocketId, TimerId, VenueError};

#[derive(Default)]
struct Probe {
    capable: bool,
    fresh: FreshPositions,
    generation: u32,
    /// Per read number, the (key, skipped rows) it commits; one clean leg by default.
    steps: Vec<Vec<(u8, Vec<String>)>>,
    /// The legs a command waits on.
    keys: Vec<u8>,
}

impl Probe {
    fn read(&mut self, cx: &mut Cx<'_, u32>) {
        self.generation += 1;
        let context = cx
            .http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .expect("background read");
        let _ = cx.request_managed(
            context,
            None,
            Method::Get,
            "https://probe.test/positions".into(),
            None,
            self.generation,
        );
    }
}

impl Venue for Probe {
    type Pending = u32;
    fn capabilities(&self) -> crate::Capabilities {
        crate::Capabilities {
            fresh_positions: self.capable,
            ..crate::Capabilities::BASE
        }
    }
    fn start(&mut self, _: &mut Cx<'_, u32>) {}
    fn on_open(&mut self, _: &mut Cx<'_, u32>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, u32>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, u32>, _: SocketId, _: u16) {}
    fn on_response(&mut self, cx: &mut Cx<'_, u32>, generation: u32, response: Response<'_>) {
        if response.status == 200 {
            let step = self.steps.get(generation as usize - 1).cloned();
            for (key, skipped) in step.unwrap_or_else(|| vec![(0, Vec::new())]) {
                self.fresh.committed(cx, key, generation, &skipped);
            }
        } else {
            let error = VenueError::new("refused").kind(ErrorKind::AuthPermission);
            self.fresh.failed(cx, 0, generation, &error);
        }
        if self.fresh.predates(0, generation) || (generation as usize) < self.steps.len() {
            self.read(cx);
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, u32>, _: u32, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, u32>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, u32>, _: Command) {}
    fn on_venue_action(&mut self, cx: &mut Cx<'_, u32>, _: u32, action: VenueAction) {
        assert_eq!(action, VenueAction::ReadPositions);
        // The first command finds nothing in flight; a second joins behind it.
        let in_flight = self.generation > 0;
        let need = self.generation + 1;
        let keys = if self.keys.is_empty() {
            vec![0]
        } else {
            self.keys.clone()
        };
        let legs = keys.into_iter().map(|key| (key, need)).collect();
        if self.fresh.admit(cx, legs) && !in_flight {
            self.read(cx);
        }
    }
    fn read_root_retired(&mut self, owner: Owner, _: Vec<u32>) {
        self.fresh.retired(owner);
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

type Done = Vec<(u32, Result<String, VenueError>)>;

fn drain(engine: &mut Engine<Probe>, done: &mut Done, requests: &mut Vec<RequestId>) {
    for action in crate::test_support::prepared_actions(engine, 0.) {
        match action {
            Action::Http(request) => requests.push(request.id),
            Action::Notice(Notice::CommandDone { command, result }) => {
                done.push((command, Ok(result)))
            }
            Action::Notice(Notice::CommandFailed { command, error }) => {
                done.push((command, Err(error)))
            }
            _ => {}
        }
    }
}

fn engine(capable: bool) -> Engine<Probe> {
    crate::test_support::seeded_engine(Probe {
        capable,
        ..Probe::default()
    })
}

fn read_positions(engine: &mut Engine<Probe>, now: f64, command: u32) {
    engine.handle(
        now,
        Event::Command(Command::Venue {
            command,
            action: VenueAction::ReadPositions,
        }),
    );
}

fn reply(engine: &mut Engine<Probe>, now: f64, request: RequestId, status: u16) {
    engine.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status,
                headers: "",
                body: "[]",
            },
        },
    );
}

#[test]
fn a_venue_without_the_capability_answers_unsupported_not_sent() {
    let mut engine = engine(false);
    read_positions(&mut engine, 0., 9);
    let (mut done, mut requests) = (Vec::new(), Vec::new());
    drain(&mut engine, &mut done, &mut requests);
    let [(9, Err(error))] = done.as_slice() else {
        panic!("{done:?}");
    };
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(requests.is_empty());
}

#[test]
fn a_frame_fed_engine_answers_unsupported_even_when_the_venue_implements_it() {
    let mut engine = engine(true);
    engine.set_frame_fed().unwrap();
    read_positions(&mut engine, 0., 9);
    let (mut done, mut requests) = (Vec::new(), Vec::new());
    drain(&mut engine, &mut done, &mut requests);
    let [(9, Err(error))] = done.as_slice() else {
        panic!("{done:?}");
    };
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(requests.is_empty());
}

#[test]
fn a_background_delivery_settles_only_the_command_its_read_postdates() {
    let mut engine = engine(true);
    let (mut done, mut requests) = (Vec::new(), Vec::new());
    read_positions(&mut engine, 0., 1);
    drain(&mut engine, &mut done, &mut requests);
    assert_eq!(requests.len(), 1, "command 1 issues read 1");
    // Command 2 arrives while read 1 is out: read 1 cannot answer it.
    read_positions(&mut engine, 1., 2);
    drain(&mut engine, &mut done, &mut requests);
    assert_eq!(requests.len(), 1);
    reply(&mut engine, 2., requests[0], 200);
    drain(&mut engine, &mut done, &mut requests);
    assert_eq!(done.len(), 1, "{done:?}");
    assert_eq!(done[0], (1, Ok("[]".to_owned())));
    assert_eq!(requests.len(), 2, "read 2 follows for command 2");
    reply(&mut engine, 3., requests[1], 200);
    drain(&mut engine, &mut done, &mut requests);
    assert_eq!(done[1], (2, Ok("[]".to_owned())));
}

#[test]
fn a_failed_read_fails_the_command_not_sent_with_the_venues_kind() {
    let mut engine = engine(true);
    let (mut done, mut requests) = (Vec::new(), Vec::new());
    read_positions(&mut engine, 0., 1);
    drain(&mut engine, &mut done, &mut requests);
    reply(&mut engine, 1., requests[0], 403);
    drain(&mut engine, &mut done, &mut requests);
    let [(1, Err(error))] = done.as_slice() else {
        panic!("{done:?}");
    };
    assert_eq!(error.kind, Some(ErrorKind::AuthPermission));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

#[test]
fn the_callers_wait_bounds_a_command_whose_read_never_answers() {
    let mut engine = engine(true);
    let (mut done, mut requests) = (Vec::new(), Vec::new());
    let command = Command::Venue {
        command: 1,
        action: VenueAction::ReadPositions,
    };
    engine.handle_command(0., command, Some(500)).unwrap();
    drain(&mut engine, &mut done, &mut requests);
    assert!(done.is_empty());
    engine.handle(
        501.,
        Event::TimerFired {
            timer: TimerId(u64::MAX),
        },
    );
    drain(&mut engine, &mut done, &mut requests);
    let [(1, Err(error))] = done.as_slice() else {
        panic!("{done:?}");
    };
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert!(
        !engine.venue().fresh.owed(0),
        "the retired root leaves no waiter"
    );
}

fn scripted(keys: Vec<u8>, steps: Vec<Vec<(u8, Vec<String>)>>) -> Engine<Probe> {
    crate::test_support::seeded_engine(Probe {
        capable: true,
        keys,
        steps,
        ..Probe::default()
    })
}

fn names(symbols: &[&str]) -> Vec<String> {
    symbols.iter().map(|symbol| (*symbol).to_owned()).collect()
}

#[test]
fn the_answer_lists_skipped_rows_as_a_json_array_of_strings() {
    let quoted = "EVIL\"SYM".to_owned();
    let mut engine = scripted(vec![], vec![vec![(0, vec![quoted])]]);
    let (mut done, mut requests) = (Vec::new(), Vec::new());
    read_positions(&mut engine, 0., 1);
    drain(&mut engine, &mut done, &mut requests);
    reply(&mut engine, 1., requests[0], 200);
    drain(&mut engine, &mut done, &mut requests);
    assert_eq!(done, vec![(1, Ok(r#"["EVIL\"SYM"]"#.to_owned()))]);
}

#[test]
fn a_multi_leg_command_accumulates_skips_deduped_across_its_legs() {
    let step = vec![
        (0, names(&["A", "B"])),
        (1, names(&["B", "C"])),
        (2, Vec::new()),
    ];
    let mut engine = scripted(vec![0, 1, 2], vec![step]);
    let (mut done, mut requests) = (Vec::new(), Vec::new());
    read_positions(&mut engine, 0., 1);
    drain(&mut engine, &mut done, &mut requests);
    reply(&mut engine, 1., requests[0], 200);
    drain(&mut engine, &mut done, &mut requests);
    assert_eq!(done, vec![(1, Ok(r#"["A","B","C"]"#.to_owned()))]);
}

#[test]
fn a_read_that_does_not_satisfy_a_leg_lends_it_no_skips() {
    // Command 2 joins while read 1 is out: read 1's skip answers only command 1.
    let mut engine = scripted(vec![], vec![vec![(0, names(&["OLD"]))]]);
    let (mut done, mut requests) = (Vec::new(), Vec::new());
    read_positions(&mut engine, 0., 1);
    drain(&mut engine, &mut done, &mut requests);
    read_positions(&mut engine, 1., 2);
    reply(&mut engine, 2., requests[0], 200);
    drain(&mut engine, &mut done, &mut requests);
    assert_eq!(done, vec![(1, Ok(r#"["OLD"]"#.to_owned()))]);
    reply(&mut engine, 3., requests[1], 200);
    drain(&mut engine, &mut done, &mut requests);
    assert_eq!(done[1], (2, Ok("[]".to_owned())));
}

#[test]
fn a_leg_already_satisfied_takes_no_skips_from_a_later_read() {
    let steps = vec![
        vec![(0, names(&["FIRST"]))],
        vec![(0, names(&["LATE"])), (1, names(&["SECOND"]))],
    ];
    let mut engine = scripted(vec![0, 1], steps);
    let (mut done, mut requests) = (Vec::new(), Vec::new());
    read_positions(&mut engine, 0., 1);
    drain(&mut engine, &mut done, &mut requests);
    reply(&mut engine, 1., requests[0], 200);
    drain(&mut engine, &mut done, &mut requests);
    assert!(done.is_empty(), "leg 1 is still owed");
    reply(&mut engine, 2., requests[1], 200);
    drain(&mut engine, &mut done, &mut requests);
    assert_eq!(done, vec![(1, Ok(r#"["FIRST","SECOND"]"#.to_owned()))]);
}
