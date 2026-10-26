use super::*;
use safe_cex_engine::{Action, ReadCredentials};
fn options() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=".into(),
            passphrase: Some("read-pass".into()),
        }),

        ..Options::default()
    }
}

struct SigningProbe {
    slots: safe_cex_engine::credential_selection::Slots<safe_cex_engine::signing::Identity>,
    step: usize,
}
impl safe_cex_engine::Venue for SigningProbe {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::TimerId) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: safe_cex_engine::Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), error: &str) {
        panic!("signing probe failed: {error}");
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: safe_cex_engine::Command) {
        use safe_cex_engine::credential_selection::CredentialRole;
        use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
        let method = [Method::Get, Method::Post, Method::Delete][self.step];
        self.step += 1;
        let read = method == Method::Get;
        let (selected, identity) = self
            .slots
            .select(if read {
                CredentialRole::Read { scope: None }
            } else {
                CredentialRole::Mutation
            })
            .unwrap();
        let context = cx
            .admit_http_command(Policy {
                operation: if read {
                    Operation::Read
                } else {
                    Operation::Mutation
                },
                credential: Some(selected.credential(identity.credential.clone())),
                timeout_ms: 20000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap();
        let path = if read {
            "/api/v1/orders"
        } else {
            "/api/v1/order"
        };
        let body = (!read).then(|| "{}".to_owned());
        cx.request_signed_managed(
            context,
            None,
            method,
            format!("{REST}{path}"),
            body.clone(),
            Box::new(sign::Http::new(
                identity.clone(),
                sign::Request {
                    method,
                    path: path.into(),
                    rows: vec![Default::default()],
                    body,
                    window: 60000,
                },
            )),
            (),
        )
        .unwrap();
    }
    fn capture_http_reconciliation(
        &self,
        _: &(),
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        safe_cex_engine::request_context::ReconciliationData::new(
            "backpack-signing-probe".to_owned(),
        )
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn probe(venue: Backpack) -> Vec<safe_cex_engine::HttpRequest> {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::{
        Algorithm, CredentialSetupPolicy, IdentitySetupPolicy, KeyEncoding,
    };
    use safe_cex_engine::{Command, Event};
    let opts = &venue.options;
    let read = opts.read_credentials.as_ref().unwrap();
    let seed = |key: String, secret: String| LocalIdentity {
        key: Secret::new(key),
        secret: Secret::new(secret),
        secret_encoding: KeyEncoding::Base64,
        ..Default::default()
    };
    let policy = IdentitySetupPolicy {
        signing_algorithm: Some(Algorithm::Ed25519),
        ..Default::default()
    };
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(seed(opts.key.clone().unwrap(), opts.secret.clone().unwrap())),
            secondary: Some(seed(read.key.clone(), read.secret.clone())),
        },
        CredentialSetupPolicy {
            primary: Some(policy),
            secondary: Some(policy),
        },
    )
    .unwrap();
    let slots = safe_cex_engine::credential_selection::Slots::new(
        identities.primary.clone(),
        identities.secondary.clone(),
        Default::default(),
    );
    let mut e = safe_cex_engine::test_support::seeded_engine(SigningProbe { slots, step: 0 });
    e.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    e.start(1700000000000.);
    let mut requests = vec![];
    for command in [
        Command::FetchLeverage {
            command: 1,
            symbol: "SOL_USDC_PERP".into(),
        },
        Command::CancelOrders {
            command: 2,
            ids: vec![],
        },
        Command::CancelOrders {
            command: 3,
            ids: vec![],
        },
    ] {
        e.handle(1700000000000., Event::Command(command));
        requests.extend(
            safe_cex_engine::test_support::prepared_actions(&mut e, 1700000000000.)
                .into_iter()
                .filter_map(|a| match a {
                    Action::Http(r) => Some(r),
                    _ => None,
                }),
        );
    }
    requests
}

#[test]
fn secondary_get_keeps_primary_mutations() {
    let requests: Vec<_> = probe(Backpack::new(options()))
        .into_iter()
        .filter(|r| r.url.ends_with("/api/v1/orders") || r.url.ends_with("/api/v1/order"))
        .collect();
    assert_eq!(requests.len(), 3);
    for request in requests {
        let expected = if request.method == Method::Get {
            "read-key"
        } else {
            "primary-key"
        };
        let opts = options();
        let secret = if request.method == Method::Get {
            opts.read_credentials.as_ref().unwrap().secret.as_str()
        } else {
            opts.secret.as_deref().unwrap()
        };
        let credentials = sign::Credentials::new(expected.into(), secret).unwrap();
        let instruction = match request.method {
            Method::Get => "orderQueryAll",
            Method::Post => "orderExecute",
            _ => "orderCancel",
        };
        assert_eq!(
            request.headers,
            credentials.headers(instruction, &[Default::default()], 1700000000000.0)
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("X-API-Key") && value == expected),
            "{:?}",
            request
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Backpack::new(opts));
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}
