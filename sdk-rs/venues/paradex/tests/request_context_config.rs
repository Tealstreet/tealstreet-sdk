//! Public config is fenced before native private initialization.
mod support;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Engine, Event, HttpRequest, Method, Response, SocketChannel, TimerId, VenueError,
};
use safe_cex_venue_paradex::{Options, Paradex};
const NOW: f64 = 1_700_000_000_000.;
const CONFIG: &str = r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#;
fn start(testnet: bool) -> (Engine<Paradex>, Vec<Action>) {
    let mut e = support::engine(
        Options {
            testnet,
            paradex_address: Some("0x123".into()),
            subkey_address: Some("0x456".into()),
            ..Default::default()
        },
        Some(support::STARK_KEY),
    );
    e.start(NOW);
    let markets = prepared_actions(&mut e, NOW)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/markets") => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(
        NOW,
        Event::HttpResponse {
            request: markets.id,
            response: Response {
                status: 200,
                headers: "",
                body: support::MARKETS,
            },
        },
    );
    let actions = e.drain_actions().collect();
    (e, actions)
}
fn config_request(actions: &[Action]) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/system/config") => Some(r.clone()),
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { request, .. } if request.url.ends_with("/system/config") => {
                    Some(request.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("native config request")
}
fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("/system/config"))),
        "config must await genuine host Prepare/Bind/Claim"
    );
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } if request.url.ends_with("/system/config") => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("managed config Prepare")
}
fn private_effects(actions: &[Action]) -> usize {
    actions.iter().filter(|a|matches!(a,Action::WsConnect{channel:Some(SocketChannel::Private),..}|Action::SignPayload{..}) || matches!(a,Action::Http(r) if r.url.contains("/auth/")||r.url.ends_with("/account/margin"))).count()
}
fn reply(e: &mut Engine<Paradex>, now: f64, r: &HttpRequest, body: &str) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn release(e: &mut Engine<Paradex>, p: Preparation, now: f64) -> HttpRequest {
    e.http_prepared(
        now,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, request) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("config Bind");
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn retry(actions: &[Action], now: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == now + 2000. => Some(*timer),
            _ => None,
        })
        .expect("native 2s config retry")
}
#[test]
fn unsigned_native_config_wire_is_unchanged_on_both_networks() {
    for testnet in [false, true] {
        let (_, actions) = start(testnet);
        let r = config_request(&actions);
        assert_eq!(
            r.url,
            format!(
                "https://api.{}.paradex.trade/v1/system/config",
                if testnet { "testnet" } else { "prod" }
            )
        );
        assert_eq!(r.method, Method::Get);
        assert!(r.headers.is_empty());
        assert!(r.signer_headers.is_empty());
        assert!(r.body.is_none());
        assert_eq!(private_effects(&actions), 0);
    }
}
#[test]
fn config_prepare_and_claim_precede_credential_install_and_duplicate_data_is_inert() {
    for testnet in [false, true] {
        let (mut e, actions) = start(testnet);
        let (p, r) = preparation(&actions);
        assert_eq!(private_effects(&reply(&mut e, NOW + 1., &r, CONFIG)), 0);
        assert!(e.http_preparation_pending(p));
        let r = release(&mut e, p, NOW + 2.);
        let after = reply(&mut e, NOW + 3., &r, CONFIG);
        assert!(after.iter().any(|a| matches!(
            a,
            Action::WsConnect {
                channel: Some(SocketChannel::Private),
                ..
            }
        )));
        assert_eq!(private_effects(&reply(&mut e, NOW + 4., &r, CONFIG)), 0);
    }
}
#[test]
fn expired_preparation_never_installs_config_and_retry_gets_a_fresh_root() {
    let (mut e, actions) = start(false);
    let (p, r) = preparation(&actions);
    e.http_prepared(
        NOW + 20_000.,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    let after: Vec<_> = e.drain_actions().collect();
    assert_eq!(private_effects(&after), 0);
    let timer = retry(&after, NOW + 20_000.);
    assert_eq!(
        private_effects(&reply(&mut e, NOW + 20_001., &r, CONFIG)),
        0
    );
    e.handle(NOW + 22_000., Event::TimerFired { timer });
    let next: Vec<_> = e.drain_actions().collect();
    let (fresh, fresh_request) = preparation(&next);
    assert_ne!(fresh, p);
    assert_ne!(fresh_request.id, r.id);
    assert!(!e.http_preparation_pending(p));
    let fresh_request = release(&mut e, fresh, NOW + 22_001.);
    assert!(private_effects(&reply(&mut e, NOW + 22_002., &fresh_request, CONFIG)) > 0);
}
#[test]
fn preparation_refusal_retires_exact_root_and_late_http_has_no_private_effect() {
    let (mut e, actions) = start(false);
    let (p, r) = preparation(&actions);
    e.http_preparation_failed(
        NOW + 1.,
        p,
        Error::State,
        VenueError::new("fixture preparation refused"),
    );
    let after: Vec<_> = e.drain_actions().collect();
    retry(&after, NOW + 1.);
    assert!(!e.http_preparation_pending(p));
    assert_eq!(private_effects(&reply(&mut e, NOW + 2., &r, CONFIG)), 0);
}
#[test]
fn invalid_native_config_keeps_exact_two_second_retry_without_private_effect() {
    let (mut e, actions) = start(false);
    let r = config_request(&actions);
    let request = if let Some(p) = actions.iter().find_map(|a| match a {
        Action::ManagedHttp(h) => match h.as_ref() {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } if request.id == r.id => Some(*preparation),
            _ => None,
        },
        _ => None,
    }) {
        release(&mut e, p, NOW)
    } else {
        r
    };
    let after = reply(&mut e, NOW + 1., &request, "{}");
    assert_eq!(private_effects(&after), 0);
    retry(&after, NOW + 1.);
}

#[test]
fn released_config_response_after_its_original_deadline_cannot_install_credentials() {
    let (mut e, actions) = start(false);
    let (p, _) = preparation(&actions);
    let request = release(&mut e, p, NOW + 1.);
    let after = reply(&mut e, NOW + 20_000., &request, CONFIG);
    assert_eq!(private_effects(&after), 0);
    assert!(!e.http_preparation_pending(p));
    retry(&after, NOW + 20_000.);
}
