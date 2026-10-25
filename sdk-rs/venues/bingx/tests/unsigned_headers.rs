//! Static native codec compatibility plus an explicitly synthetic managed
//! recipe. This does not admit BingX production managed or hosted requests.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Credential, HttpAction, Operation, Policy, Prepared, RetryPolicy, Scope,
};
use safe_cex_engine::signing::{
    KeyRef, Operation as SignOperation, SignError, SignRequest, SignResult,
};
use safe_cex_engine::{
    Action, Command, CredentialSlot, Cx, DispatchContext, Method, Response, SignStep, SocketId,
    TimerId, Venue,
};
use safe_cex_venue_bingx::sign::{Credentials, Params, unsigned_headers};

struct NativeProbe {
    method: Method,
    path: &'static str,
    credentials: Option<Credentials>,
}
impl Venue for NativeProbe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        safe_cex_venue_bingx::stage(
            cx,
            self.credentials.as_ref(),
            self.method,
            self.path,
            Params::new(),
            None,
            (),
        );
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), error: &str) {
        panic!("{error}");
    }
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn native_request(
    method: Method,
    path: &'static str,
    signed: bool,
) -> safe_cex_engine::HttpRequest {
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-primary".into()),
            secret: Secret::new("fixture-signing-material".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let mut engine = safe_cex_engine::test_support::seeded_engine(NativeProbe {
        method,
        path,
        credentials: signed.then(|| Credentials {
            primary: identities.primary.unwrap(),
            read: None,
        }),
    });
    engine
        .set_signer(Some(std::sync::Arc::new(provider)))
        .unwrap();
    engine.start(12.);
    let mut requests = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(http) => Some(http),
            Action::Sign { .. } | Action::SignedOutbound { .. } => {
                panic!("local provider must complete inline")
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    requests.pop().unwrap()
}

#[test]
fn legacy_public_and_signed_vectors_keep_native_order_for_every_method_class() {
    for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
        let content = if method == Method::Post {
            "application/json"
        } else {
            "application/json, charset=utf-8"
        };
        let public = vec![
            ("X-SOURCE-KEY", "Tealstreet".into()),
            ("Content-Type", content.into()),
        ];
        assert_eq!(native_request(method, "/native", false).headers, public);
        let headers = native_request(method, "/openApi/user/auth/userDataStream", true).headers;
        assert_eq!(
            headers,
            vec![
                ("X-SOURCE-KEY", "Tealstreet".into()),
                ("X-BX-APIKEY", "fixture-primary".into()),
                ("Content-Type", content.into()),
            ]
        );
        let headers = native_request(method, "/native", true).headers;
        assert_eq!(
            headers,
            vec![
                ("X-SOURCE-KEY", "Tealstreet".into()),
                ("X-BX-APIKEY", "fixture-primary".into()),
                ("Content-Type", content.into()),
            ]
        );
    }
}

struct SyntheticRecipe(Method);
impl SignStep for SyntheticRecipe {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        unsigned_headers(self.0)
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        _: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let expected = unsigned_headers(self.0)
            .iter()
            .map(|&(name, value)| (name, value.into()))
            .collect::<Vec<_>>();
        if http.headers != expected || !http.signer_headers.is_empty() {
            return Err(SignError::InvalidRequest);
        }
        Ok(SignRequest {
            context: Default::default(),
            receipt: Default::default(),
            operations: vec![SignOperation::Present {
                key: KeyRef("ref:owned-primary".into()),
            }],
        })
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        1_000.
    }
}
struct SyntheticProbe(Method);
impl Venue for SyntheticProbe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let context = cx
            .http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: Some(Credential {
                        slot: CredentialSlot::Primary,
                        reference: KeyRef("ref:owned-primary".into()),
                    }),
                    timeout_ms: 1_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        cx.request_signed_managed(
            context,
            None,
            self.0,
            "https://venue.test/synthetic".into(),
            None,
            Box::new(SyntheticRecipe(self.0)),
            (),
        )
        .unwrap();
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), error: &str) {
        panic!("synthetic preparation failed: {error}");
    }
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn synthetic_managed_recipe_attests_the_same_bingx_public_declaration() {
    for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
        let mut engine = safe_cex_engine::test_support::seeded_engine(SyntheticProbe(method));
        engine.start(0.);
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(!actions.iter().any(|a| matches!(a, Action::Sign { .. })));
        let token = actions
            .iter()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match action.as_ref() {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        assert_eq!(
                            request.headers,
                            unsigned_headers(method)
                                .iter()
                                .map(|&(name, value)| (name, value.into()))
                                .collect::<Vec<_>>()
                        );
                        assert!(request.signer_headers.is_empty());
                        Some(*preparation)
                    }
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        engine.http_prepared(
            1.,
            token,
            Prepared {
                id: 9,
                expires_at: None,
            },
        );
        assert!(
            engine
                .drain_actions()
                .any(|action| matches!(action, Action::Sign { .. }))
        );
    }
}
