use super::*;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::Action;
use safe_cex_engine::test_support::{ProbeVenue, prepared_actions};
const PRIMARY_SECRET: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";
const READ_SECRET: &str = "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=";
fn setup() -> (Options, safe_cex_engine::signing::Provider) {
    let (signer, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("primary-key".into()),
            secret: Secret::new(PRIMARY_SECRET.into()),
            ..Default::default()
        }),
        secondary: Some(LocalIdentity {
            key: Secret::new("read-key".into()),
            secret: Secret::new(READ_SECRET.into()),
            ..Default::default()
        }),
    })
    .unwrap();
    (
        Options {
            credentials: identities.primary,
            read_credentials: identities.secondary,
            ..Options::default()
        },
        std::sync::Arc::new(signer),
    )
}

fn probe(
    venue: BybitLinear,
) -> ProbeVenue<BybitLinear, impl FnMut(&mut BybitLinear, &mut Cx<'_, Pending>)> {
    ProbeVenue::new(venue, |venue, cx| {
        use safe_cex_engine::request_context::{Credential, Policy, Scope};
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            let rest = Rest {
                path: "/secondary-test",
                payload: if method == Method::Get {
                    "category=linear&symbol=BTC%26USDT".into()
                } else {
                    r#"{"category":"linear","clientId":"quote\"+&"}"#.into()
                },
                stream: false,
                signed: true,
                resent: false,
            };
            if method == Method::Get {
                venue.send_read(cx, rest, Then::Tickers(vec![]));
                continue;
            }
            // Bybit writes sign on a command plan; drive that recipe on a Background leg.
            let identity = venue.signing_identity(method).cloned().unwrap();
            let policy = Policy {
                credential: Some(Credential {
                    slot: safe_cex_engine::CredentialSlot::Primary,
                    reference: identity.credential.clone(),
                }),
                ..venue.read_policy(false, 20_000).unwrap()
            };
            let context = cx.http_context(Scope::Background, policy).unwrap();
            let step = sign::Http::new(identity, rest.payload.clone(), venue.recv_window)
                .with_header_policy(venue.signing_policy.header_policy());
            let url = format!("{}{}", venue.paths.rest, rest.path);
            let body = Some(rest.payload.clone());
            let pending = Pending {
                category: venue.category,
                rest,
                then: Then::Tickers(vec![]),
                read: Some(context.clone()),
            };
            assert!(
                cx.request_signed_managed(context, method, url, body, Box::new(step), pending)
                    .is_ok()
            );
        }
    })
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    let (options, signer) = setup();
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe(BybitLinear::new(options)));
    engine.set_signer(Some(signer)).unwrap();
    engine.start(1700000000000.0);
    let requests: Vec<_> = prepared_actions(&mut engine, 1700000001234.75)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.url.contains("secondary-test") => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 4);
    for request in requests {
        let expected = if request.method == Method::Get {
            "read-key"
        } else {
            "primary-key"
        };
        let secret = if request.method == Method::Get {
            READ_SECRET
        } else {
            PRIMARY_SECRET
        };
        let expected_headers = sign::oracle::Credentials {
            key: expected.into(),
            secret: secret.into(),
        }
        .headers(
            1700000001234.0,
            if request.method == Method::Get {
                request.url.split_once('?').unwrap().1
            } else {
                request.body.as_deref().unwrap()
            },
        );
        let headers = &request.headers;
        assert_eq!(*headers, expected_headers);
        assert!(
            headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("X-BAPI-API-KEY")
                    && value == expected),
            "{headers:?}"
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let (mut opts, _) = setup();
    opts.read_credentials.as_mut().unwrap().signing.0.clear();
    let mut engine = safe_cex_engine::test_support::seeded_engine(BybitLinear::new(opts));
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}
