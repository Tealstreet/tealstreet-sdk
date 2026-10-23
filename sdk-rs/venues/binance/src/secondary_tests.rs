use super::*;
use crate::sign::oracle::Signed;
use crate::test_config::Options;
use safe_cex_engine::test_support::{ProbeVenue, http_requests};
use safe_cex_engine::{Action, HttpRequest, ReadCredentials, RequestRoute};
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
    venue: BinanceFutures,
) -> ProbeVenue<BinanceFutures, impl FnMut(&mut BinanceFutures, &mut Cx<'_, Pending>)> {
    ProbeVenue::new(venue, |venue, cx| {
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            let role = if method == Method::Get {
                CredentialRole::Read { scope: None }
            } else {
                CredentialRole::Mutation
            };
            venue.signed(
                cx,
                role,
                method,
                "/secondary-test",
                sign::Query::new(),
                Pending::Daily,
            );
        }
    })
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    for variant in [
        Variant::UsdM,
        Variant::PortfolioMargin,
        Variant::Spot,
        Variant::CoinM,
    ] {
        for recv_window in [None, Some(4321.0), Some(0.0), Some(4321.125)] {
            let mut opts = options();
            opts.variant = variant;
            opts.recv_window = recv_window;
            let (venue, signer) = opts.build();
            let mut engine = safe_cex_engine::test_support::seeded_engine(probe(venue));
            engine.set_signer(Some(signer)).unwrap();
            engine.start(1700000000000.0);
            let requests: Vec<_> = http_requests(&mut engine)
                .into_iter()
                .filter(|r| r.url.contains("secondary-test"))
                .collect();
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
                let credentials = sign::oracle::LegacyCredentials {
                    key: expected.into(),
                    secret: secret.into(),
                };
                assert!(request.url.ends_with(&sign::Query::new().signed(
                    &credentials,
                    1700000000000.0,
                    recv_window
                )));
                let (payload, signature) = request
                    .url
                    .split_once('?')
                    .unwrap()
                    .1
                    .rsplit_once("&signature=")
                    .unwrap();
                let expected_payload = format!(
                    "timestamp=1700000000000&recvWindow={}",
                    recv_window.unwrap_or(20000.0)
                );
                assert_eq!(payload, expected_payload);
                assert_eq!(
                    signature,
                    safe_cex_crypto::hmac_sha256(
                        secret.as_bytes(),
                        expected_payload.as_bytes(),
                        safe_cex_crypto::Encoding::Hex
                    )
                );
                assert!(
                    request
                        .headers
                        .iter()
                        .any(|(name, value)| name.eq_ignore_ascii_case("X-MBX-APIKEY")
                            && value == expected),
                    "{:?}",
                    request
                );
                let route = if request.method == Method::Get {
                    RequestRoute::Secondary
                } else {
                    RequestRoute::Primary
                };
                let declared = request.route;
                assert_eq!(declared, Some(route));
            }
        }
    }
}

/// The role, not the HTTP method, selects key and egress.
#[test]
fn declared_role_selects_key_and_egress_for_any_method() {
    let (venue, signer) = options().build();
    let probe = ProbeVenue::new(
        venue,
        |venue: &mut BinanceFutures, cx: &mut Cx<'_, Pending>| {
            venue.signed(
                cx,
                CredentialRole::Read { scope: None },
                Method::Post,
                "/declared-read",
                sign::Query::new(),
                Pending::Daily,
            );
            venue.signed(
                cx,
                CredentialRole::PrimaryRead {
                    reason: safe_cex_engine::credential_selection::PrimaryReason::History,
                    scope: None,
                },
                Method::Get,
                "/declared-primary",
                sign::Query::new(),
                Pending::Daily,
            );
        },
    );
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe);
    engine.set_signer(Some(signer)).unwrap();
    engine.start(1700000000000.0);
    let requests = http_requests(&mut engine);
    let find = |path: &str| {
        requests
            .iter()
            .find(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("{path} not sent"))
    };
    let key = |request: &HttpRequest| {
        request
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("X-MBX-APIKEY"))
            .map(|(_, value)| value.clone())
    };
    let read = find("/declared-read");
    assert_eq!(
        (key(read).as_deref(), read.route),
        (Some("read-key"), Some(RequestRoute::Secondary))
    );
    let primary = find("/declared-primary");
    assert_eq!(
        (key(primary).as_deref(), primary.route),
        (Some("primary-key"), Some(RequestRoute::Primary))
    );
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = crate::test_config::engine(opts);
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}

#[test]
fn signed_reads_use_the_dispatch_clock_after_queueing_for_all_binance_variants() {
    for variant in [
        Variant::UsdM,
        Variant::PortfolioMargin,
        Variant::CoinM,
        Variant::Spot,
    ] {
        let mut opts = options();
        opts.variant = variant;
        let (venue, signer) = opts.build();
        let mut engine = safe_cex_engine::test_support::seeded_engine(probe(venue));
        engine.set_signer(Some(signer)).unwrap();
        engine.start(1_700_000_000_000.0);
        engine.prepare_dispatch(1_700_000_003_456.0);
        let requests: Vec<_> = http_requests(&mut engine)
            .into_iter()
            .filter(|request| request.url.contains("secondary-test"))
            .collect();
        assert_eq!(requests.len(), 4);
        for request in requests {
            assert!(
                request.url.contains("timestamp=1700000003456"),
                "{variant:?}: {}",
                request.url
            );
            let identity = if request.method == Method::Get {
                "read-key"
            } else {
                "primary-key"
            };
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(name, value)| *name == "X-MBX-APIKEY" && value == identity)
            );
        }
    }
}
