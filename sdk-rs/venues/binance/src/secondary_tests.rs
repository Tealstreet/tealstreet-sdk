use super::*;
use crate::test_config::Options;
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared, RoutingIntent, Scope};
use safe_cex_engine::test_support::ProbeVenue;
use safe_cex_engine::{Action, HttpRequest, Method};
/// Primary plus a read identity; the fixture's strings live in test support.
fn options() -> Options {
    crate::test_config::with_read_identity()
}

/// One signed request on a Background leg, keyed by the declared role.
fn signed(
    venue: &BinanceFutures,
    cx: &mut Cx<'_, Pending>,
    role: CredentialRole,
    method: Method,
    path: &str,
) {
    let (selection, credentials) = venue.credentials.select(role).unwrap();
    let credential = selection.credential(credentials.credential.clone());
    let context = cx
        .http_context(Scope::Background, reads::policy(Some(credential)))
        .unwrap();
    let recipe = venue.signed_recipe(credentials.clone(), sign::Query::new());
    let url = format!("{}{path}", venue.urls.account_rest);
    assert!(
        cx.request_signed_managed(
            context,
            None,
            method,
            url,
            None,
            Box::new(recipe),
            Pending::Daily
        )
        .is_ok()
    );
}

/// Released requests, each with the egress intent it was prepared under.
fn sent(
    engine: &mut safe_cex_engine::Engine<impl Venue>,
    now: f64,
) -> Vec<(HttpRequest, RoutingIntent)> {
    engine.prepare_dispatch(now);
    let mut intents = std::collections::HashMap::new();
    let mut sent = Vec::new();
    loop {
        let actions: Vec<_> = engine.drain_actions().collect();
        if actions.is_empty() {
            return sent;
        }
        for action in actions {
            let Action::ManagedHttp(action) = action else {
                continue;
            };
            match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                    ..
                } => {
                    intents.insert(request.id, intent);
                    let id = request.id.0;
                    let prepared = Prepared {
                        id,
                        expires_at: None,
                    };
                    engine.http_prepared(now, preparation, prepared);
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    if let Ok(Claim::Released(_)) = engine.claim_http(now, permit) {
                        let intent = intents[&request.id];
                        sent.push((request, intent));
                    }
                }
                _ => {}
            }
        }
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
            signed(venue, cx, role, method, "/secondary-test");
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
            let requests: Vec<_> = sent(&mut engine, 1700000000000.0)
                .into_iter()
                .filter(|(r, _)| r.url.contains("secondary-test"))
                .collect();
            assert_eq!(requests.len(), 4);
            for (request, intent) in requests {
                let expected = if request.method == Method::Get {
                    "read-key"
                } else {
                    "primary-key"
                };
                // The exact signed payload; the HMAC bytes themselves are
                // proved through the provider in tests/signing_vectors.rs.
                let (payload, signature) = request
                    .url
                    .split_once('?')
                    .unwrap()
                    .1
                    .rsplit_once("&signature=")
                    .unwrap();
                assert_eq!(
                    payload,
                    format!(
                        "timestamp=1700000000000&recvWindow={}",
                        recv_window.unwrap_or(20000.0)
                    )
                );
                assert_eq!(signature.len(), 64, "a 64-digit hex signature");
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
                    RoutingIntent::Secondary
                } else {
                    RoutingIntent::Primary
                };
                assert_eq!(intent, route);
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
            signed(
                venue,
                cx,
                CredentialRole::Read { scope: None },
                Method::Post,
                "/declared-read",
            );
            signed(
                venue,
                cx,
                CredentialRole::PrimaryRead {
                    reason: safe_cex_engine::credential_selection::PrimaryReason::History,
                    scope: None,
                },
                Method::Get,
                "/declared-primary",
            );
        },
    );
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe);
    engine.set_signer(Some(signer)).unwrap();
    engine.start(1700000000000.0);
    let requests = sent(&mut engine, 1700000000000.0);
    let find = |path: &str| {
        requests
            .iter()
            .find(|(r, _)| r.url.contains(path))
            .unwrap_or_else(|| panic!("{path} not sent"))
    };
    let key = |request: &HttpRequest| {
        request
            .headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("X-MBX-APIKEY"))
            .map(|(_, value)| value.clone())
    };
    let (read, intent) = find("/declared-read");
    assert_eq!(
        (key(read).as_deref(), *intent),
        (Some("read-key"), RoutingIntent::Secondary)
    );
    let (primary, intent) = find("/declared-primary");
    assert_eq!(
        (key(primary).as_deref(), *intent),
        (Some("primary-key"), RoutingIntent::Primary)
    );
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let opts = crate::test_config::without_read_signing(options());
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
        let requests: Vec<_> = sent(&mut engine, 1_700_000_003_456.0)
            .into_iter()
            .filter(|(request, _)| request.url.contains("secondary-test"))
            .collect();
        assert_eq!(requests.len(), 4);
        for (request, _) in requests {
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
