//! Registration is fixed at construction; each final admission has one mode.
#![cfg(feature = "test-support")]
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Admission, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Command, Cx, HttpRelease, Method, Response, SocketId, TimerId, Venue, VenueError,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

static MODES: [AtomicBool; 3] = [const { AtomicBool::new(false) }; 3];
static CAPTURES: [AtomicUsize; 3] = [const { AtomicUsize::new(0) }; 3];
#[derive(Default)]
struct Counters {
    pure: AtomicUsize,
    mutable: AtomicUsize,
    failures: AtomicUsize,
}
struct Probe<const ID: usize> {
    counters: Arc<Counters>,
    deny: bool,
}
impl<const ID: usize> Venue for Probe<ID> {
    type Pending = ();
    fn http_release() -> HttpRelease<Self> {
        CAPTURES[ID].fetch_add(1, Ordering::SeqCst);
        if MODES[ID].load(Ordering::SeqCst) {
            HttpRelease::NonceUnscheduled(|this, _, _| {
                this.counters.pure.fetch_add(1, Ordering::SeqCst);
                if this.deny {
                    Err(Error::Invalid)
                } else {
                    Ok(())
                }
            })
        } else {
            HttpRelease::Mutable(|this, _, _| {
                this.counters.mutable.fetch_add(1, Ordering::SeqCst);
                Ok(Admission::Unscheduled)
            })
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        for _ in 0..2 {
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 1000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_managed(
                context,
                Method::Get,
                "https://venue.test/read".into(),
                None,
                (),
            )
            .unwrap();
        }
    }
    fn on_http_preparation_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: Error, _: VenueError) {
        self.counters.failures.fetch_add(1, Ordering::SeqCst);
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn exercise<const ID: usize>(pure: bool, deny: bool) -> (usize, Arc<Counters>) {
    MODES[ID].store(pure, Ordering::SeqCst);
    let counters = Arc::new(Counters::default());
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe::<ID> {
        counters: counters.clone(),
        deny,
    });
    assert_eq!(CAPTURES[ID].load(Ordering::SeqCst), 1);
    // A later getter would now choose the other mode; the engine must use its capture.
    MODES[ID].store(!pure, Ordering::SeqCst);
    engine.start(0.);
    let packets = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.)
        .iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .count();
    assert_eq!(CAPTURES[ID].load(Ordering::SeqCst), 1);
    (packets, counters)
}
#[test]
fn pure_registration_is_captured_once_and_never_runs_mutable_admission() {
    let (packets, counts) = exercise::<0>(true, false);
    assert_eq!(
        (
            packets,
            counts.pure.load(Ordering::SeqCst),
            counts.mutable.load(Ordering::SeqCst)
        ),
        (2, 2, 0)
    );
}
#[test]
fn mutable_registration_is_captured_once_and_never_runs_pure_admission() {
    let (packets, counts) = exercise::<1>(false, false);
    assert_eq!(
        (
            packets,
            counts.pure.load(Ordering::SeqCst),
            counts.mutable.load(Ordering::SeqCst)
        ),
        (2, 0, 2)
    );
}
#[test]
fn refused_pure_admission_sends_nothing_and_never_calls_mutable_admission() {
    let (packets, counts) = exercise::<2>(true, true);
    assert_eq!(
        (
            packets,
            counts.pure.load(Ordering::SeqCst),
            counts.mutable.load(Ordering::SeqCst),
            counts.failures.load(Ordering::SeqCst)
        ),
        (0, 2, 0, 2)
    );
}
