use safe_cex_venue_hyperliquid::Options;

#[test]
fn public_account_options_keep_redacted_debug() {
    let options = Options {
        key: Some("audit-secret-sentinel".into()),
        ..Options::default()
    };
    assert!(!format!("{options:?}").contains("audit-secret-sentinel"));
    assert!(options.credentials.is_none());
}

mod support;

#[test]
fn installed_options_retain_only_refs_and_public_account_configuration() {
    let scalar = support::TEST_SECRET;
    let (_, identities) = support::agent(scalar);
    let identity = identities.primary.clone().unwrap();
    assert!(identity.credential_absent());
    assert!(identity.credential_ref().is_none());
    let options = Options {
        key: Some("0x1111111111111111111111111111111111111111".into()),
        application_id: Some("0x2222222222222222222222222222222222222222".into()),
        vault: Some("0x3333333333333333333333333333333333333333".into()),
        credentials: Some(identity),
        ..Default::default()
    };
    assert!(!format!("{options:?}").contains(scalar));
    assert!(!format!("{options:?}").contains(&scalar[2..]));
}

#[test]
fn failed_engine_owned_address_derivation_cannot_admit_a_private_write() {
    use safe_cex_engine::signing::*;
    use safe_cex_engine::{Action, Command, Event, Notice, Outcome, VenueAction};
    use std::{
        sync::Arc,
        task::{Context, Poll},
    };
    struct FailedSetup {
        started: bool,
    }
    impl OwnedSetupExecution for FailedSetup {
        fn start(&mut self) -> Result<(), SignError> {
            self.started = true;
            Ok(())
        }
        fn cancel(&mut self) -> Result<(), SignError> {
            self.started = true;
            Ok(())
        }
        fn poll_result(&mut self, _: &mut Context<'_>) -> Poll<Result<SignResult, SignError>> {
            if self.started {
                Poll::Ready(Err(SignError::InvalidResult))
            } else {
                Poll::Pending
            }
        }
        fn poll_joined(&mut self, _: &mut Context<'_>) -> Poll<Result<(), SignError>> {
            if self.started {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }
    }
    struct FailingAddress(Provider);
    impl Signer for FailingAddress {
        fn sign(&self, request: SignRequest) -> SignFuture {
            self.0.sign(request)
        }
        fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
            self.0.can_sign(key, algorithm)
        }
        fn owned_public_address_capability(&self) -> Capability {
            Capability::Supported
        }
        fn create_public_address_setup(
            &self,
            request: SignRequest,
        ) -> Option<Box<dyn OwnedSetupExecution>> {
            assert!(matches!(
                request.operations.as_slice(),
                [Operation::PublicAddress { .. }]
            ));
            Some(Box::new(FailedSetup { started: false }))
        }
    }
    let (provider, identities) = support::agent(support::TEST_SECRET);
    let mut engine = safe_cex_engine::test_support::seeded_engine(
        safe_cex_venue_hyperliquid::Hyperliquid::new(Options {
            credentials: identities.primary.clone(),
            ..Default::default()
        }),
    );
    engine
        .set_signer(Some(Arc::new(FailingAddress(provider))))
        .unwrap();
    engine.set_identities(identities).unwrap();
    engine.initialize_address_nonce(999.0).unwrap();
    assert!(
        engine.address_nonce_setup_ready(),
        "failed setup has actually joined"
    );
    engine.start(1000.0);
    let startup = support::prepared_now(&mut engine);
    assert!(
        startup
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/info"))),
        "public work continues"
    );
    engine.handle(
        2000.0,
        Event::Command(Command::Venue {
            command: 77,
            action: VenueAction::ScheduleCancel { time_ms: None },
        }),
    );
    let actions = support::prepared_now(&mut engine);
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 77, error }) if error.outcome == Some(Outcome::NotSent))));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Sign { .. } | Action::ManagedHttp(_)))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
}
