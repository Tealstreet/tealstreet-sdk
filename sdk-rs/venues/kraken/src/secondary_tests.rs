use super::*;
use safe_cex_engine::test_support::{ProbeVenue, http_requests};
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

// ProbeVenue ignores responses; this native carrier grants no Context authority.
fn probe_pending() -> Pending {
    Pending::Candles {
        command: None,
        symbol: String::new(),
        interval: Interval::M1,
        generation: None,
        rows: vec![],
        failed: false,
        page: CandlePage {
            from: 0.,
            to: 0.,
            step: 60_000.,
            pages: 0,
        },
    }
}

fn probe(venue: Kraken) -> ProbeVenue<Kraken, impl FnMut(&mut Kraken, &mut Cx<'_, Pending>)> {
    ProbeVenue::new(venue, |venue, cx| {
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            venue
                .signed_request(
                    cx,
                    method,
                    "/derivatives/api/v3/secondary-test",
                    String::new(),
                    probe_pending(),
                )
                .unwrap();
        }
        venue
            .signed_streamed(
                cx,
                Method::Get,
                "/api/history/v3/account-log",
                String::new(),
                safe_cex_engine::json::ArrayAt::Root,
                probe_pending(),
            )
            .unwrap();
    })
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    let options = options();
    let mut primary_signer = sign::Signer::new(options.secret.as_deref().unwrap()).unwrap();
    let mut read_signer =
        sign::Signer::new(&options.read_credentials.as_ref().unwrap().secret).unwrap();
    // Reserve bounds before dispatch: creating a signer must not reset its shared counter.
    let primary_bound: u64 = primary_signer
        .nonce(1700000000000.0)
        .unwrap()
        .parse()
        .unwrap();
    let read_bound: u64 = read_signer.nonce(1700000000000.0).unwrap().parse().unwrap();
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe(Kraken::new(options)));
    engine.start(1700000000000.0);
    let requests: Vec<_> = http_requests(&mut engine)
        .into_iter()
        .filter(|r| r.url.contains("secondary-test"))
        .collect();
    assert_eq!(requests.len(), 4);
    let mut previous = [read_bound, primary_bound];
    for request in requests {
        let expected = if request.method == Method::Get {
            "read-key"
        } else {
            "primary-key"
        };
        let (signer, bound) = if request.method == Method::Get {
            (&read_signer, read_bound)
        } else {
            (&primary_signer, primary_bound)
        };
        let nonce = request
            .headers
            .iter()
            .find(|(k, _)| *k == "Nonce")
            .unwrap()
            .1
            .as_str();
        let nonce_number = nonce.parse::<u64>().unwrap();
        assert!(nonce_number > bound);
        let index = usize::from(request.method != Method::Get);
        assert!(
            nonce_number > previous[index],
            "same signer nonce must increase for each request"
        );
        previous[index] = nonce_number;
        assert_eq!(
            request
                .headers
                .iter()
                .find(|(k, _)| *k == "Authent")
                .unwrap()
                .1,
            signer
                .signature("", nonce, "/derivatives/api/v3/secondary-test")
                .unwrap()
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("APIKey") && value == expected),
            "{:?}",
            request
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Kraken::new(opts));
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}

#[test]
fn equal_api_keys_share_nonce_clock_but_keep_selected_signing_secrets() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().key = "primary-key".into();
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe(Kraken::new(opts)));
    engine.start(1700000000000.0);
    let requests: Vec<_> = http_requests(&mut engine);
    assert_eq!(requests.len(), 5);
    let mut previous = 0;
    for request in requests {
        let nonce = request
            .headers
            .iter()
            .find(|(k, _)| *k == "Nonce")
            .unwrap()
            .1
            .as_str();
        let n = nonce.parse::<u64>().unwrap();
        assert!(n > previous, "same API key reused nonce");
        previous = n;
        assert!(request.headers.contains(&("APIKey", "primary-key".into())));
        let opts = options();
        let secret = if request.method == Method::Get && !request.url.contains("/api/history/") {
            &opts.read_credentials.as_ref().unwrap().secret
        } else {
            opts.secret.as_ref().unwrap()
        };
        let signature = sign::Signer::new(secret)
            .unwrap()
            .signature(
                "",
                nonce,
                if request.url.contains("/api/history/") {
                    "/api/history/v3/account-log"
                } else {
                    "/derivatives/api/v3/secondary-test"
                },
            )
            .unwrap();
        assert_eq!(
            request
                .headers
                .iter()
                .find(|(k, _)| *k == "Authent")
                .unwrap()
                .1,
            signature
        );
    }
}

#[test]
fn history_client_keeps_primary_credentials_and_explicit_primary_route() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe(Kraken::new(options())));
    engine.start(1700000000000.0);
    let requests: Vec<_> = http_requests(&mut engine)
        .into_iter()
        .filter(|r| r.url.contains("/api/history/"))
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].route,
        Some(safe_cex_engine::RequestRoute::Primary)
    );
    assert!(
        requests[0]
            .headers
            .contains(&("APIKey", "primary-key".into()))
    );
}
