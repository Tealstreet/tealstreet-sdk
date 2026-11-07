//! Reuse actual native public intent in the shared managed transport boundary.
//! This does not migrate the native venue's command or completion owner.
use safe_cex_engine::request_context::{
    Claim, HttpAction, Operation, Policy, Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::{
    Action, Command, Cx, Event, HttpRequest, Method, Response, SocketId, TimerId, Venue, book::Book,
};

const JSON: &[(&str, &str)] = &[("Content-Type", "application/json")];

struct Intent {
    native: HttpRequest,
    replies: usize,
}
impl Venue for Intent {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let context = cx
            .http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    timeout_ms: 1_000,
                    credential: None,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        cx.request_managed_with_headers(
            context,
            None,
            self.native.method,
            self.native.url.clone(),
            JSON,
            self.native.body.clone(),
            (),
        )
        .unwrap();
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), response: Response<'_>) {
        assert_eq!(response.body, "{}");
        self.replies += 1;
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), error: &str) {
        panic!("unexpected fixture failure: {error}");
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn same_intent(request: &HttpRequest, native: &HttpRequest) {
    assert_eq!(request.method, Method::Post);
    assert_eq!(request.method, native.method);
    assert_eq!(request.url, native.url);
    assert_eq!(request.body, native.body);
    assert_eq!(request.headers, native.headers);
    assert_eq!(request.route, None);
    assert!(request.signer_headers.is_empty());
}

pub fn assert_native_public_intent_survives_managed_release(native: &HttpRequest) {
    assert_eq!(
        native.headers,
        vec![("Content-Type", "application/json".into())]
    );
    let mut engine = safe_cex_engine::test_support::seeded_engine(Intent {
        native: native.clone(),
        replies: 0,
    });
    engine.start(0.);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::Sign { .. }))
    );
    let prepares = actions
        .iter()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(prepares.len(), 1);
    let (preparation, request) = prepares[0];
    same_intent(request, native);
    assert_eq!(request.timeout_ms, Some(1_000));
    engine.http_prepared(
        1.,
        preparation,
        Prepared {
            id: 7,
            expires_at: None,
        },
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::Sign { .. }))
    );
    let bindings = actions
        .into_iter()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    preparation: token,
                    permit,
                    request,
                } => {
                    assert_eq!(token, preparation);
                    Some((permit, request))
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 1);
    let (permit, request) = &bindings[0];
    same_intent(request, native);
    assert!(matches!(
        engine.claim_http(2., *permit),
        Ok(Claim::Released(_))
    ));
    engine.handle(
        3.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    engine.drain_actions().for_each(drop);
    assert_eq!(engine.venue().replies, 1);
}
