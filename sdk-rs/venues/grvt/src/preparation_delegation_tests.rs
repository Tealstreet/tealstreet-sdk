use super::*;
use safe_cex_engine::request_context::{
    Context, Error, HttpAction, Operation, Policy, RetryPolicy,
};
use safe_cex_engine::{Action, Event, Notice, Query, VenueError};

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
    root: Option<Context>,
    extracting: bool,
}
impl Venue for Probe {
    type Pending = Pending;

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx);
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.venue.capabilities()
    }
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
        self.root = Some(
            cx.admit_http_command(Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap(),
        );
        self.venue.start_query(
            cx,
            COMMAND,
            Query::PublicTrades {
                symbol: "BTC_USDT_PERP".into(),
                limit: Some(1),
            },
        );
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        if self.extracting {
            self.extracting = false;
            if let Job::History(history) = &pending.job {
                assert!(history.is_public());
            } else {
                panic!("expected public history pending");
            }
            let context = self.root.take().unwrap();
            let url = format!("{}{}", self.venue.mdg(), pending.path);
            let body = Some(pending.body.clone());
            assert!(
                cx.request_managed_with_headers(
                    context,
                    Method::Post,
                    url,
                    &[("Content-Type", "application/json")],
                    body,
                    pending
                )
                .is_ok()
            );
        } else {
            self.venue.on_http_failed(cx, pending, error);
        }
    }
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
        venue: Grvt::new(Options {
            public_data_only: true,
            ..Options::default()
        }),
        dispatch,
        failures: 0,
        root: None,
        extracting: true,
    });
    engine.start(NOW);
    let catalog = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    engine.handle(NOW, Event::HttpResponse { request: catalog.id, response: Response { status: 200, headers: "", body: r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"# } });
    engine.drain_actions().for_each(drop);
    engine
        .handle_command(
            NOW,
            Command::Query {
                command: COMMAND,
                query: Query::PublicTrades {
                    symbol: "BTC_USDT_PERP".into(),
                    limit: Some(1),
                },
            },
            Some(60_000),
        )
        .unwrap();
    let initial_actions = engine.drain_actions().collect::<Vec<_>>();
    let initial = initial_actions
        .iter()
        .find_map(|action| match action {
            Action::Http(http) => Some(http.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("public history fixture emitted {initial_actions:?}"));
    assert!(initial.url.ends_with("/lite/v1/trade_history"));
    engine.handle(
        NOW,
        Event::HttpFailed {
            request: initial.id,
            error: "extract native public pending",
        },
    );
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
            "https://market-data.grvt.io/lite/v1/trade_history".into(),
            vec![("Content-Type", "application/json".into())],
            Some(r#"{"i":"BTC_USDT_Perp","et":"1700000000000000000","l":1000,"c":""}"#.into()),
        )
    );
    assert!(actual.1.is_empty());
    let direct = summarize(&refusal(Dispatch::DirectFailure));
    assert!(direct.0.is_empty());
    assert_ne!(actual, direct);
}
