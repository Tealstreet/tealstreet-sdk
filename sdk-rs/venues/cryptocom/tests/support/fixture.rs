//! Trusted fixture setup consumes raw placeholders before constructing the venue.
#![allow(dead_code)]
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::{Engine, ReadCredentials};
use safe_cex_venue_cryptocom::{Cryptocom, Options as VenueOptions};
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct Options {
    pub read_credentials: Option<ReadCredentials>,
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        if let Some(read) = self.read_credentials.as_mut() {
            read.erase_secrets();
        }
    }
}
pub fn setup(mut options: Options) -> (VenueOptions, Arc<LocalSigner>) {
    let secondary_validation = options
        .read_credentials
        .as_ref()
        .map(ReadCredentials::validate);
    let primary = options
        .key
        .take()
        .filter(|s| !s.is_empty())
        .zip(options.secret.take().filter(|s| !s.is_empty()))
        .map(|(key, secret)| LocalIdentity {
            key: Secret::new(key),
            secret: Secret::new(secret),
            ..Default::default()
        });
    let (signer, identities) = LocalSigner::from_setup(LocalSetup {
        primary,
        secondary: None,
    })
    .unwrap();
    let result = VenueOptions {
        credentials: identities.primary,
        secondary_validation,
        testnet: options.testnet,
        public_data_only: options.public_data_only,
    };
    safe_cex_crypto::secrets::EraseSecrets::erase_secrets(&mut options);
    (result, Arc::new(signer))
}
pub fn venue(options: Options) -> Cryptocom {
    Cryptocom::new(setup(options).0)
}
pub fn engine(options: Options) -> Engine<Cryptocom> {
    let (options, signer) = setup(options);
    let mut engine = safe_cex_engine::test_support::seeded_engine(Cryptocom::new(options));
    engine.set_signer(Some(signer)).unwrap();
    engine
}

/// Minimal host handoff: async signing remains parked and visible to the test.
pub fn actions<V: safe_cex_engine::Venue>(
    engine: &mut Engine<V>,
    now: f64,
) -> Vec<safe_cex_engine::Action> {
    use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
    use safe_cex_engine::{Action, Event};
    let mut queue = std::collections::VecDeque::new();
    let mut output = Vec::new();
    engine.prepare_dispatch(now);
    queue.extend(engine.drain_actions());
    let mut count = 0;
    while let Some(action) = queue.pop_front() {
        count += 1;
        assert!(count <= 65_536, "fixture host handoff bound");
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    if engine.http_preparation_pending(preparation) {
                        engine.http_prepared(
                            now,
                            preparation,
                            Prepared {
                                id: request.id.0,
                                expires_at: None,
                            },
                        );
                    }
                }
                HttpAction::Bind {
                    permit, request, ..
                } => match engine.claim_http(now, permit) {
                    Ok(Claim::Released(_)) => output.push(Action::Http(request)),
                    Ok(Claim::WaitUntil(_)) => panic!("fixture requires explicit pacing wake"),
                    Err(_) => {}
                },
                HttpAction::Cancel { .. } => {}
            },
            Action::Sign { attempt, .. } if engine.signer().is_none() => {
                engine.handle(
                    now,
                    Event::Signed {
                        attempt,
                        result: Err(safe_cex_engine::signing::SignError::Unavailable),
                    },
                );
            }
            action => output.push(action),
        }
        for next in engine.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(next);
        }
    }
    output
}
