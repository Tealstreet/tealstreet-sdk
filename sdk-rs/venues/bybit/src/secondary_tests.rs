use super::*;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::Action;
use safe_cex_engine::test_support::{ProbeVenue, http_requests};
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
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            let rest = Rest {
                method,
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
            venue.send(cx, rest, Then::Tickers(vec![]));
        }
    })
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    let (options, signer) = setup();
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe(BybitLinear::new(options)));
    engine.set_signer(Some(signer)).unwrap();
    engine.start(1700000000000.0);
    engine.prepare_dispatch(1700000001234.75);
    let requests: Vec<_> = http_requests(&mut engine)
        .into_iter()
        .filter(|r| r.url.contains("secondary-test"))
        .collect();
    assert_eq!(requests.len(), 4);
    for sent in requests {
        let expected = if sent.method == Method::Get {
            "read-key"
        } else {
            "primary-key"
        };
        let secret = if sent.method == Method::Get {
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
            if sent.method == Method::Get {
                sent.url.split_once('?').unwrap().1
            } else {
                sent.body.as_deref().unwrap()
            },
        );
        assert_eq!(sent.headers, expected_headers);
        assert!(
            sent.headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("X-BAPI-API-KEY")
                    && value == expected),
            "{:?}",
            sent
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
