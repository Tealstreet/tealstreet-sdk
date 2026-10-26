//! Real installed identities, physical preparation and original Read custody.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared};
use safe_cex_engine::signing::{CredentialSetupPolicy, IdentitySetupPolicy, SigningPolicy};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response};
use safe_cex_venue_mexc::{MexcFutures, Options};
use std::sync::Arc;
const NOW: f64 = 1_700_000_000_000.;
const EMPTY: &str = r#"{"success":true,"code":0,"data":[]}"#;
mod support;
fn engine(secondary: bool, primary_signing: bool) -> Engine<MexcFutures> {
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("primary-key".into()),
                secret: Secret::new(
                    if primary_signing {
                        "primary-secret"
                    } else {
                        ""
                    }
                    .into(),
                ),
                stable_id: Some("mexc:64a04c6ee432d0c9370920d0".into()),
                ..Default::default()
            }),
            secondary: secondary.then(|| LocalIdentity {
                key: Secret::new("read-key".into()),
                secret: Secret::new("read-secret".into()),
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
    assert_eq!(
        identities.primary.as_ref().unwrap().signing_ref().is_ok(),
        primary_signing
    );
    let mut e = safe_cex_engine::test_support::seeded_engine(MexcFutures::new(Options {
        identities: identities.clone(),
        read_credentials_valid: secondary.then_some(true),
        ..Default::default()
    }));
    e.set_signer(Some(Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    e
}
fn ask(e: &mut Engine<MexcFutures>, command: u32) -> (Preparation, HttpRequest) {
    ask_at(e, command, NOW)
}
fn ask_at(e: &mut Engine<MexcFutures>, command: u32, now: f64) -> (Preparation, HttpRequest) {
    e.handle(now, Event::Command(Command::FetchAccount { command }));
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::Sign { .. }))
    );
    let mut packets = actions.iter().filter_map(|action| match action {
        Action::ManagedHttp(action) => match action.as_ref() {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } => Some((*preparation, request.clone())),
            _ => None,
        },
        _ => None,
    });
    let packet = packets.next().expect("real unsigned preparation");
    assert!(packets.next().is_none());
    assert_eq!(
        packet.1.headers,
        vec![("Content-Type", "application/json".into())]
    );
    packet
}
fn bind(
    e: &mut Engine<MexcFutures>,
    packet: (Preparation, HttpRequest),
    at: f64,
) -> (safe_cex_engine::request_context::Permit, HttpRequest) {
    e.http_prepared(
        at,
        packet.0,
        Prepared {
            id: packet.1.id.0,
            expires_at: None,
        },
    );
    let mut packets = e.drain_actions().filter_map(|action| match action {
        Action::ManagedHttp(action) => match *action {
            HttpAction::Bind {
                permit, request, ..
            } => Some((permit, request)),
            _ => None,
        },
        _ => None,
    });
    let result = packets.next().expect("actual provider-backed binding");
    assert!(packets.next().is_none());
    result
}
fn release(
    e: &mut Engine<MexcFutures>,
    packet: (Preparation, HttpRequest),
    at: f64,
) -> HttpRequest {
    let (permit, request) = bind(e, packet, at);
    assert!(matches!(e.claim_http(at, permit), Ok(Claim::Released(_))));
    assert!(e.http_release(request.id).is_some());
    request
}
fn reply(e: &mut Engine<MexcFutures>, request: &HttpRequest, at: f64) -> Vec<Action> {
    e.handle(
        at,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: EMPTY,
            },
        },
    );
    e.drain_actions().collect()
}
fn header<'a>(request: &'a HttpRequest, name: &str) -> &'a str {
    &request
        .headers
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
}
fn done(actions: &[Action], command: u32) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: id,
                result,
            }) if *id == command => Some(result.as_str()),
            _ => None,
        })
        .collect()
}
fn mode_and_peer(
    e: &mut Engine<MexcFutures>,
) -> ((Preparation, HttpRequest), (Preparation, HttpRequest)) {
    e.start(NOW);
    let catalogue = safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .unwrap();
    e.handle(
        NOW,
        Event::HttpResponse {
            request: catalogue.id,
            response: Response {
                status: 200,
                headers: "",
                body: support::MARKETS,
            },
        },
    );
    let mut mode = None;
    let mut ticker = None;
    for action in e.drain_actions() {
        if let Action::ManagedHttp(action) = action
            && let HttpAction::Prepare {
                preparation,
                request,
                ..
            } = *action
        {
            if request.url.contains("position_mode") {
                assert!(mode.replace((preparation, request)).is_none());
            } else if request.url.contains("contract/ticker") {
                assert!(ticker.replace((preparation, request)).is_none());
            } else {
                panic!("unexpected startup request");
            }
        }
    }
    (mode.unwrap(), ticker.unwrap())
}
#[test]
fn precommit_failure_matrix_preserves_real_peer_and_once_only_recovery() {
    use safe_cex_engine::request_context::Error;
    use safe_cex_engine::{ErrorKind, VenueError};
    for (reason, kind, recovery) in [
        (Error::Capacity, ErrorKind::Internal, true),
        (Error::TimedOut, ErrorKind::Timeout, true),
        (Error::Invalid, ErrorKind::Network, true),
        (Error::Invalid, ErrorKind::Internal, true),
        (Error::Invalid, ErrorKind::Timeout, true),
        (Error::Invalid, ErrorKind::InvalidInput, false),
        (Error::Invalid, ErrorKind::Unsupported, false),
        (Error::State, ErrorKind::Internal, false),
        (Error::Duplicate, ErrorKind::Internal, false),
        (Error::Revoked, ErrorKind::Internal, false),
        (Error::RetryForbidden, ErrorKind::Internal, false),
    ] {
        let mut e = engine(false, true);
        let (mode, peer) = mode_and_peer(&mut e);
        e.http_preparation_failed(
            NOW + 1.,
            mode.0,
            reason,
            VenueError::new("same failure text").kind(kind),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, Action::SetTimer { .. }))
                .count(),
            usize::from(recovery)
        );
        assert!(!actions.iter().any(|action| matches!(action, Action::Http(_))
            || matches!(action, Action::ManagedHttp(action) if !matches!(action.as_ref(), HttpAction::Cancel { preparation, .. } if *preparation == mode.0))));
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, Action::Notice(Notice::Event(_))))
                .count(),
            1
        );
        assert!(e.http_preparation_pending(peer.0));
        e.http_preparation_failed(
            NOW + 2.,
            mode.0,
            reason,
            VenueError::new("same failure text").kind(kind),
        );
        assert!(e.drain_actions().next().is_none());
        let peer = release(&mut e, peer, NOW + 2.);
        e.handle(
            NOW + 3.,
            Event::HttpResponse {
                request: peer.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: support::TICKERS,
                },
            },
        );
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().last, 60000.);
    }
}
#[test]
fn installed_primary_literal_signature_and_identity_are_store_inert() {
    let mut e = engine(false, true);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let packet = ask(&mut e, 42);
    let request = release(&mut e, packet, NOW);
    assert_eq!(header(&request, "ApiKey"), "primary-key");
    assert_eq!(header(&request, "Request-Time"), "1700000000000");
    assert_eq!(
        header(&request, "Signature"),
        "7aab839b166e3a062398132a47634cced8ff9e1f5a7e31028e986e33c04549e4"
    );
    assert_eq!(request.route, None);
    assert_eq!(request.body, None);
    assert_eq!(
        done(&reply(&mut e, &request, NOW), 42),
        [r#"{"userId":"mexc:64a04c6ee432d0c9370920d0"}"#]
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert!(reply(&mut e, &request, NOW).is_empty());
}
#[test]
fn secondary_read_preserves_credential_only_primary_identity_capability() {
    let mut e = engine(true, false);
    let packet = ask(&mut e, 42);
    let request = release(&mut e, packet, NOW);
    assert_eq!(header(&request, "ApiKey"), "read-key");
    assert_eq!(
        header(&request, "Signature"),
        "1504972030dc652723afa2f090b303172cdd621de85ab3f304bc291e467287ec"
    );
    assert_eq!(
        done(&reply(&mut e, &request, NOW), 42),
        [r#"{"userId":"mexc:64a04c6ee432d0c9370920d0"}"#]
    );
}
#[test]
fn delayed_preparation_uses_actual_protocol_stamp() {
    let mut e = engine(false, true);
    let packet = ask(&mut e, 42);
    let request = release(&mut e, packet, NOW + 800.);
    assert_eq!(header(&request, "Request-Time"), "1700000000800");
    assert_eq!(done(&reply(&mut e, &request, NOW + 801.), 42).len(), 1);
}
#[test]
fn held_preparation_expiry_cannot_bind_or_publish() {
    let mut e = engine(false, true);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let packet = ask(&mut e, 42);
    e.http_prepared(
        NOW + 20_000.,
        packet.0,
        Prepared {
            id: packet.1.id.0,
            expires_at: None,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|action|matches!(action,Action::Http(_))||matches!(action,Action::ManagedHttp(action) if matches!(action.as_ref(),HttpAction::Bind {..}))));
    assert!(done(&actions, 42).is_empty());
    assert_eq!(e.account_snapshot_frame(NOW), before);
}
#[test]
fn held_bind_claim_at_deadline_and_late_data_are_inert() {
    let mut e = engine(false, true);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let packet = ask(&mut e, 42);
    let (permit, request) = bind(&mut e, packet, NOW);
    assert!(e.claim_http(NOW + 20_000., permit).is_err());
    let _ = e.drain_actions().collect::<Vec<_>>();
    assert!(done(&reply(&mut e, &request, NOW + 20_000.), 42).is_empty());
    assert_eq!(e.account_snapshot_frame(NOW), before);
}
#[test]
fn expiry_of_one_original_preserves_an_independent_live_read() {
    let mut e = engine(false, true);
    let first = ask(&mut e, 42);
    let second = ask_at(&mut e, 43, NOW + 1000.);
    let second = release(&mut e, second, NOW + 1001.);
    e.http_prepared(
        NOW + 20_000.,
        first.0,
        Prepared {
            id: first.1.id.0,
            expires_at: None,
        },
    );
    let _ = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(done(&reply(&mut e, &second, NOW + 20_000.), 43).len(), 1);
}

fn history(e: &mut Engine<MexcFutures>, command: u32) -> (Preparation, HttpRequest) {
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command,
            query: safe_cex_engine::Query::Fills {
                symbol: None,
                order_id: Some("123".into()),
                from_ms: None,
                to_ms: None,
                limit: Some(10),
            },
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    let mut packets = actions.iter().filter_map(|action| match action {
        Action::ManagedHttp(action) => match action.as_ref() {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } => Some((*preparation, request.clone())),
            _ => None,
        },
        _ => None,
    });
    let packet = packets.next().expect("real history preparation");
    assert!(packets.next().is_none());
    packet
}
#[test]
fn private_history_retains_primary_with_a_secondary_installed() {
    let mut e = engine(true, true);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let packet = history(&mut e, 44);
    let request = release(&mut e, packet, NOW);
    assert_eq!(
        request.url,
        "https://api.mexc.com/api/v1/private/order/deal_details/123?"
    );
    assert_eq!(header(&request, "ApiKey"), "primary-key");
    assert_eq!(
        header(&request, "Signature"),
        "7aab839b166e3a062398132a47634cced8ff9e1f5a7e31028e986e33c04549e4"
    );
    assert_eq!(done(&reply(&mut e, &request, NOW), 44), ["[]"]);
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert!(reply(&mut e, &request, NOW).is_empty());
}
#[test]
fn private_history_held_binding_expiry_cannot_publish_a_page() {
    let mut e = engine(true, true);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let packet = history(&mut e, 44);
    let (permit, request) = bind(&mut e, packet, NOW);
    assert!(e.claim_http(NOW + 20_000., permit).is_err());
    let _ = e.drain_actions().collect::<Vec<_>>();
    assert!(done(&reply(&mut e, &request, NOW + 20_000.), 44).is_empty());
    assert_eq!(e.account_snapshot_frame(NOW), before);
}

#[test]
fn background_released_expiry_retires_once_and_preserves_later_peer() {
    for failed in [false, true] {
        let mut e = engine(false, true);
        let (mode, ticker) = mode_and_peer(&mut e);
        let mode = release(&mut e, mode, NOW);
        let ticker = release(&mut e, ticker, NOW);
        e.handle(
            NOW,
            Event::HttpResponse {
                request: ticker.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: support::TICKERS,
                },
            },
        );
        e.drain_actions().for_each(drop);
        e.handle(
            NOW + 1000.,
            Event::Command(Command::FetchOrderBook {
                command: 99,
                symbol: "BTCUSDT".into(),
                limit: Some(1),
            }),
        );
        let peer = e
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => Some((preparation, request)),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        assert!(e.store().hedged());
        if failed {
            e.handle(
                NOW + 20_000.,
                Event::HttpFailed {
                    request: mode.id,
                    error: "expired observation",
                },
            );
        } else {
            e.handle(
                NOW + 20_000.,
                Event::HttpResponse {
                    request: mode.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: r#"{"success":true,"code":0,"data":2}"#,
                    },
                },
            );
        }
        let cleanup: Vec<_> = e.drain_actions().collect();
        assert!(
            e.store().hedged(),
            "expired mode cannot publish its payload"
        );
        assert!(e.http_preparation_pending(peer.0));
        assert!(!cleanup.iter().any(|action| matches!(action, Action::Http(_) | Action::Notice(Notice::AccountAuth { .. })) || matches!(action, Action::ManagedHttp(action) if !matches!(action.as_ref(),HttpAction::Cancel { .. }))));
        assert_eq!(
            cleanup
                .iter()
                .filter(|action| matches!(action, Action::SetTimer { .. }))
                .count(),
            1,
            "one native poll recovery"
        );
        assert!(reply(&mut e, &mode, NOW + 20_000.).is_empty());
        let peer = release(&mut e, peer, NOW + 20_001.);
        e.handle(NOW + 20_002., Event::HttpResponse { request: peer.id, response: Response { status: 200, headers: "", body: r#"{"success":true,"code":0,"data":{"version":1,"bids":[[60000,10,1]],"asks":[[60001,20,1]]}}"# } });
        assert!(e.drain_actions().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { command: 99, .. })
        )));
    }
}

#[test]
fn nonfinite_owned_identity_delivery_settles_without_leaking_the_original_root() {
    for failed in [false, true] {
        let mut e = engine(false, true);
        let packet = ask(&mut e, 42);
        let request = release(&mut e, packet, NOW);
        let before = e.account_snapshot_frame(NOW).to_vec();
        if failed {
            e.handle(
                f64::NAN,
                Event::HttpFailed {
                    request: request.id,
                    error: "invalid observation clock",
                },
            );
        } else {
            e.handle(
                f64::NAN,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: EMPTY,
                    },
                },
            );
        }
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 42, .. })))
                .count(),
            1
        );
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_)
                | Action::Sign { .. }
                | Action::SetTimer { .. }
                | Action::WsConnect { .. }
                | Action::WsSend { .. }
                | Action::Notice(Notice::AccountAuth { .. })
        )));
        assert_eq!(e.account_snapshot_frame(NOW), before);
        let next = ask_at(&mut e, 42, NOW + 1.);
        assert!(e.http_preparation_pending(next.0));
    }
}
