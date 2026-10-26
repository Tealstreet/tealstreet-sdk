//! Installed-provider probe of the native private signing helper.
use super::*;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{CredentialSetupPolicy, IdentitySetupPolicy, SigningPolicy};
use safe_cex_engine::{Action, Command, Event, ReadCredentials, Venue};
use std::sync::Arc;
#[derive(Default)]
struct Options {
    key: Option<String>,
    secret: Option<String>,
    read_credentials: Option<ReadCredentials>,
}
fn options() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=".into(),
            passphrase: Some("read-pass".into()),
        }),
    }
}

fn setup(
    mut opts: Options,
) -> (
    MexcFutures,
    safe_cex_engine::signing::Provider,
    safe_cex_engine::signing::Identities,
) {
    let status = opts
        .read_credentials
        .as_ref()
        .map(|read| read.validate().is_ok());
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new(opts.key.take().unwrap()),
                secret: Secret::new(opts.secret.take().unwrap()),
                ..Default::default()
            }),
            secondary: opts
                .read_credentials
                .take()
                .filter(|_| status == Some(true))
                .map(|mut read| LocalIdentity {
                    key: Secret::new(std::mem::take(&mut read.key)),
                    secret: Secret::new(std::mem::take(&mut read.secret)),
                    ..Default::default()
                }),
        },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                trim_credential: Some(true),
                signing: SigningPolicy::Optional,
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap();
    (
        MexcFutures::new(crate::Options {
            identities: identities.clone(),
            read_credentials_valid: status,
            ..Default::default()
        }),
        Arc::new(provider),
        identities,
    )
}
struct Probe {
    venue: MexcFutures,
    method: Method,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.venue.capabilities()
    }
    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, _: Command) {
        self.venue.signed(
            cx,
            self.method,
            "/secondary-test",
            String::new(),
            false,
            Pending::Identity(1),
        );
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    let mut requests = Vec::new();
    for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
        let (venue, provider, identities) = setup(options());
        let mut engine = safe_cex_engine::test_support::seeded_engine(Probe { venue, method });
        engine.set_signer(Some(provider)).unwrap();
        engine.set_identities(identities).unwrap();
        let command = if method == Method::Get {
            Command::FetchAccount { command: 1 }
        } else {
            Command::SetPositionMode {
                command: 1,
                symbol: None,
                hedged: true,
            }
        };
        engine.handle(1700000000000., Event::Command(command));
        requests.extend(
            safe_cex_engine::test_support::prepared_actions(&mut engine, 1700000000000.)
                .into_iter()
                .filter_map(|a| match a {
                    Action::Http(request) => Some(request),
                    _ => None,
                }),
        );
    }
    assert_eq!(requests.len(), 4);
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
        let expected_headers = sign::Credentials {
            key: expected.into(),
            secret: secret.into(),
        }
        .headers(1700000000000.0, "");
        assert_eq!(request.headers, expected_headers);
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("ApiKey") && value == expected),
            "{:?}",
            request
        );
    }
}
#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let (venue, provider, identities) = setup(opts);
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(provider)).unwrap();
    engine.set_identities(identities).unwrap();
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}
