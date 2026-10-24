use super::*;
use safe_cex_engine::ReadCredentials;
use safe_cex_engine::test_support::{ProbeVenue, http_requests};
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

fn probe(
    venue: Cryptocom,
) -> ProbeVenue<Cryptocom, impl FnMut(&mut Cryptocom, &mut Cx<'_, Pending>)> {
    ProbeVenue::new(venue, |venue, cx| {
        // ProbeVenue ignores responses; this genuine fixture Context is only
        // the carrier field, never authority for the legacy signed request.
        let context = cx.http_context(Scope::Background, read_policy()).unwrap();
        venue.signed(
            cx,
            "private/get-trades",
            &sign::Params::default(),
            Pending::Markets {
                context,
                rows: vec![],
                indexes: Default::default(),
                skipped: Default::default(),
                malformed: Default::default(),
            },
            false,
        );
    })
}
#[test]
fn posted_read_keeps_primary_identity_with_secondary_configured() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe(Cryptocom::new(options())));
    engine.start(1700000000000.0);
    let requests: Vec<_> = http_requests(&mut engine);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, Method::Post);
    let expected = sign::Credentials {
        key: "primary-key".into(),
        secret: options().secret.unwrap(),
    }
    .envelope(
        "private/get-trades",
        &sign::Params::default(),
        1700000000000.0,
        false,
    );
    assert_eq!(requests[0].body.as_deref(), Some(expected.as_str()));
}
