use super::*;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Claim, Error, HttpAction, Prepared, RoutingIntent};
use safe_cex_engine::{Action, Command, Engine, Event, Notice, SocketId, Venue, VenueError};

const NOW: f64 = 1_700_000_000_000.;
struct Probe {
    venue: Grvt,
    retired: usize,
}
impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.login(cx);
    }
    fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response);
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
        assert!(matches!(pending.job, Job::Account(Request::Login)));
        let old = self
            .venue
            .account
            .login_session
            .clone()
            .expect("native login Session");
        assert!(cx.session_live(&old));
        self.venue
            .on_http_preparation_failed(cx, pending, reason, error);
        assert!(
            !cx.session_live(&old),
            "default delegation retires the old login Session"
        );
        let new = self
            .venue
            .account
            .login_session
            .as_ref()
            .expect("native reissued login");
        assert!(new != &old, "the retry uses a different Session generation");
        assert!(cx.session_live(new));
        self.retired += 1;
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn next_login(
    engine: &mut Engine<Probe>,
) -> (
    safe_cex_engine::request_context::Preparation,
    safe_cex_engine::HttpRequest,
) {
    engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } if request.url.ends_with("/auth/api_key/login") => {
                    assert_eq!(intent, RoutingIntent::Primary);
                    Some((preparation, request))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("real native login Prepare")
}
#[test]
fn candle_callback_composition_preserves_login_preparation_failure_delegation() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        venue: Grvt::new(crate::Options {
            key: Some("fixture-api-key".into()),
            trading_account_id: Some("777".into()),
            ..crate::Options::default()
        }),
        retired: 0,
    });
    engine.start(NOW);
    let first = next_login(&mut engine);
    engine.http_preparation_failed(
        NOW + 1.,
        first.0,
        Error::Invalid,
        VenueError::new("login route refused"),
    );
    assert_eq!(engine.venue().retired, 1);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(actions.iter().filter(|action| matches!(action, Action::ManagedHttp(action) if matches!(&**action, HttpAction::Cancel { preparation, .. } if *preparation == first.0))).count(), 1);
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
    )));
    assert!(!actions.iter().any(|action| matches!(action, Action::ManagedHttp(action) if matches!(&**action, HttpAction::Bind { .. }))));
    let mut second = None;
    for action in actions {
        if let Action::ManagedHttp(action) = action
            && let HttpAction::Prepare {
                preparation,
                request,
                intent,
            } = *action
        {
            assert!(second.is_none(), "exactly one native reissue");
            assert_eq!(intent, RoutingIntent::Primary);
            second = Some((preparation, request));
        }
    }
    let second = second.expect("new native login Prepare");
    assert_ne!(first.0, second.0);
    engine.handle(
        NOW + 2.,
        Event::HttpResponse {
            request: first.1.id,
            response: Response {
                status: 200,
                headers: "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
                body: r#"{"status":"success","sub_account_id":"777"}"#,
            },
        },
    );
    assert!(
        engine.drain_actions().next().is_none(),
        "old refused login is inert"
    );
    engine.http_preparation_failed(
        NOW + 2.,
        first.0,
        Error::Invalid,
        VenueError::new("late duplicate"),
    );
    assert_eq!(engine.venue().retired, 1);
    assert!(engine.drain_actions().next().is_none());
    engine.http_prepared(
        NOW + 3.,
        second.0,
        Prepared {
            id: second.1.id.0,
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
        .expect("reissued login Bind");
    assert!(matches!(
        engine.claim_http(NOW + 3., permit),
        Ok(Claim::Released(_))
    ));
}
