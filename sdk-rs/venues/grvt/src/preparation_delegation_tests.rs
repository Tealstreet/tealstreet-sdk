use super::*;
use safe_cex_engine::request_context::{Error, HttpAction, Operation, Policy, RetryPolicy};
use safe_cex_engine::{Action, Notice, VenueError};

const NOW: f64 = 1_700_000_000_000.;
const COMMAND: u32 = 731;

#[derive(Clone, Copy)]
enum Dispatch {
    Venue,
    Default,
    DirectFailure,
}
struct Probe {
    venue: Grvt,
    dispatch: Dispatch,
    failures: usize,
}
impl Venue for Probe {
    type Pending = Pending;

    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, _: Command) {
        let context = cx
            .admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap();
        assert!(
            cx.request_managed(
                context,
                Method::Post,
                "https://market-data.grvt.io/lite/v1/book".into(),
                Some("{}".into()),
                Pending {
                    path: "/lite/v1/book",
                    body: "{}".into(),
                    job: Job::BookQuery { command: COMMAND },
                    attempts: 0,
                    auth_resent: false,
                    auth_generation: 0,
                },
            )
            .is_ok()
        );
    }
    fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {}
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error);
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.failures += 1;
        match self.dispatch {
            Dispatch::Venue => self
                .venue
                .on_http_preparation_failed(cx, pending, reason, error),
            // The inherited engine default delegates to on_sign_error.
            Dispatch::Default => self.on_sign_error(cx, pending, error),
            Dispatch::DirectFailure => self.venue.fail_pending(cx, pending, &error.message),
        }
    }
}

fn refusal(dispatch: Dispatch) -> Vec<Action> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        venue: Grvt::new(Options::default()),
        dispatch,
        failures: 0,
    });
    engine
        .handle_command(
            NOW,
            Command::FetchCandles {
                command: COMMAND,
                symbol: "fixture".into(),
                interval: Interval::M1,
                limit: None,
                from_ms: None,
                to_ms: None,
            },
            Some(60_000),
        )
        .unwrap();
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let preparations = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match &**a {
                HttpAction::Prepare { preparation, .. } => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(preparations.len(), 1);
    let preparation = preparations[0];
    engine.http_preparation_failed(
        NOW,
        preparation,
        Error::Invalid,
        VenueError::new("non-candle preparation refused"),
    );
    assert_eq!(engine.venue().failures, 1);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a,
                Action::ManagedHttp(a) if matches!(&**a,
                    HttpAction::Cancel { preparation: p, signing: None } if *p == preparation)
            ))
            .count(),
        1
    );
    assert!(!actions.iter().any(|a| matches!(a,
        Action::ManagedHttp(a) if matches!(&**a, HttpAction::Prepare { .. } | HttpAction::Bind { .. })
    )));
    engine.http_preparation_failed(
        NOW,
        preparation,
        Error::Invalid,
        VenueError::new("late duplicate"),
    );
    assert_eq!(engine.venue().failures, 1);
    assert!(engine.drain_actions().next().is_none());
    actions
}

#[test]
fn non_candle_preparation_failure_preserves_the_engine_default_delegation() {
    let summarize = |actions: &[Action]| {
        let retries = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => {
                    Some((r.method, r.url.clone(), r.headers.clone(), r.body.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let errors = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    Some((*command, error.message.clone(), error.kind, error.outcome))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        (retries, errors)
    };
    let actual = summarize(&refusal(Dispatch::Venue));
    let default = summarize(&refusal(Dispatch::Default));
    assert_eq!(actual, default);
    assert_eq!(actual.0.len(), 1);
    assert_eq!(
        actual.0[0],
        (
            Method::Post,
            "https://market-data.grvt.io/lite/v1/book".into(),
            vec![("Content-Type", "application/json".into())],
            Some("{}".into()),
        )
    );
    assert!(actual.1.is_empty());
    let direct = summarize(&refusal(Dispatch::DirectFailure));
    assert!(direct.0.is_empty());
    assert_ne!(actual, direct);
}
