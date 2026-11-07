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

static MODES: [AtomicBool; 6] = [const { AtomicBool::new(false) }; 6];
static CAPTURES: [AtomicUsize; 6] = [const { AtomicUsize::new(0) }; 6];
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
            HttpRelease::Mutable(|this, _, release| {
                this.counters.mutable.fetch_add(1, Ordering::SeqCst);
                match ID {
                    3 => release.response_lease(200.).map(Admission::Scheduled),
                    4 => Ok(Admission::Denied),
                    5 => Err(Error::Invalid),
                    _ => Ok(Admission::Unscheduled),
                }
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
                None,
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

fn explicit_claims<const ID: usize>() -> (
    Vec<Result<safe_cex_engine::request_context::Claim, Error>>,
    Arc<Counters>,
) {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    MODES[ID].store(false, Ordering::SeqCst);
    let counters = Arc::new(Counters::default());
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe::<ID> {
        counters: counters.clone(),
        deny: false,
    });
    engine.start(0.);
    let preparations: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(preparations.len(), 2);
    for preparation in preparations {
        engine.http_prepared(
            0.,
            preparation,
            Prepared {
                id: preparation.attempt().request.0,
                expires_at: None,
            },
        );
    }
    let permits: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(permits.len(), 2);
    let claims = permits
        .into_iter()
        .map(|permit| engine.claim_http(0., permit))
        .collect();
    assert_eq!(CAPTURES[ID].load(Ordering::SeqCst), 1);
    assert_eq!(counters.mutable.load(Ordering::SeqCst), 2);
    assert_eq!(counters.pure.load(Ordering::SeqCst), 0);
    (claims, counters)
}

#[test]
fn mutable_scheduled_admission_preserves_the_physical_response_lease() {
    use safe_cex_engine::request_context::Claim;
    let (claims, counts) = explicit_claims::<3>();
    for claim in claims {
        let Claim::Released(release) = claim.unwrap() else {
            panic!("scheduled admission did not release");
        };
        assert_eq!(release.response_lease, Some(200.));
    }
    assert_eq!(counts.failures.load(Ordering::SeqCst), 0);
}

#[test]
fn mutable_denied_admission_refuses_without_a_release() {
    let (claims, counts) = explicit_claims::<4>();
    assert_eq!(claims, vec![Err(Error::Revoked), Err(Error::Revoked)]);
    assert_eq!(counts.failures.load(Ordering::SeqCst), 2);
}

#[test]
fn mutable_admission_error_keeps_its_original_kind() {
    let (claims, counts) = explicit_claims::<5>();
    assert_eq!(claims, vec![Err(Error::Invalid), Err(Error::Invalid)]);
    assert_eq!(counts.failures.load(Ordering::SeqCst), 2);
}

#[test]
fn default_nonce_capability_preserves_venue_trait_object_compatibility() {
    let _: Option<&dyn Venue<Pending = ()>> = None;
    assert!(!Probe::<0>::dispatch_nonce());
}
