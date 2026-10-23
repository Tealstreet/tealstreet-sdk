//! Independent startup reads use the shared prepare/release/deadline lifecycle.
mod support;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Engine, Event, HttpRequest, Method, Notice, Response, TimerId, VenueError,
};
use safe_cex_venue_paradex::{Options, Paradex};

const NOW: f64 = 1000.;
fn start(testnet: bool) -> (Engine<Paradex>, Vec<Action>) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Paradex::new(Options {
        public_data_only: true,
        testnet,
        ..Options::default()
    }));
    e.start(NOW);
    let actions = e.drain_actions().collect();
    (e, actions)
}
fn prepare(actions: &[Action], url: &str, issued_at: f64) -> (Preparation, HttpRequest, TimerId) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "startup GET must await preparation: {actions:?}"
    );
    let (token, request) = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("public seed Prepare");
    assert_eq!(request.url, url);
    assert_eq!(request.method, Method::Get);
    assert!(request.headers.is_empty());
    assert!(request.signer_headers.is_empty());
    assert!(request.body.is_none());
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == issued_at + 20_000. => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1, "one original deadline per attempt");
    (token, request, timers[0])
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
        .unwrap();
    assert!(request.headers.is_empty());
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
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
fn no_command(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}
fn retry_timer(actions: &[Action], now: f64) -> TimerId {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_)) || matches!(a, Action::ManagedHttp(h) if matches!(h.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))),
        "retry must yield to startup Backoff: {actions:?}"
    );
    no_command(actions);
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (750. ..=1000.).contains(&(*at_ms - now)) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("existing startup Backoff")
}

#[test]
fn independent_catalog_and_tickers_park_before_claim_and_preserve_native_publication() {
    for (testnet, root) in [
        (false, "https://api.prod.paradex.trade/v1"),
        (true, "https://api.testnet.paradex.trade/v1"),
    ] {
        let (mut e, actions) = start(testnet);
        let (catalog, unsigned, timer) = prepare(&actions, &format!("{root}/markets"), NOW);
        no_command(&actions);
        let early = reply(&mut e, NOW + 1., &unsigned, support::MARKETS);
        assert!(!e.store().loaded().markets);
        assert!(!early.iter().any(|a| matches!(a, Action::WsConnect { .. })));
        for event in [
            Event::HttpStart {
                request: unsigned.id,
                status: 200,
                headers: "",
            },
            Event::HttpChunk {
                request: unsigned.id,
                data: support::MARKETS.as_bytes(),
            },
            Event::HttpEnd {
                request: unsigned.id,
            },
        ] {
            e.handle(NOW + 1., event);
        }
        assert!(!e.store().loaded().markets);
        assert!(e.http_preparation_pending(catalog));
        e.drain_actions().for_each(drop);
        let r = release(&mut e, catalog, NOW + 2.);
        let actions = reply(&mut e, NOW + 3., &r, support::MARKETS);
        assert_eq!(e.store().markets().len(), 2);
        assert!(!e.store().loaded().tickers);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::ClearTimer { timer: t } if *t == timer))
        );
        let (tickers, unsigned, _) = prepare(
            &actions,
            &format!("{root}/markets/summary?market=ALL"),
            NOW + 3.,
        );
        assert!(!e.http_preparation_pending(catalog));
        reply(&mut e, NOW + 4., &unsigned, support::TICKERS);
        assert!(!e.store().loaded().tickers);
        let r = release(&mut e, tickers, NOW + 5.);
        let done = reply(&mut e, NOW + 6., &r, support::TICKERS);
        assert!(e.store().loaded().tickers);
        assert_eq!(e.store().tickers()[0].last, 100.);
        no_command(&done);
        assert!(reply(&mut e, NOW + 7., &r, support::TICKERS).is_empty());
    }
}

#[test]
fn preparation_denial_retires_before_backoff_and_stale_catalog_cannot_publish() {
    let (mut e, actions) = start(false);
    let (old, unsigned, _) = prepare(&actions, "https://api.prod.paradex.trade/v1/markets", NOW);
    e.http_preparation_failed(
        NOW + 1.,
        old,
        Error::State,
        VenueError::new("public route denied"),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let timer = retry_timer(&actions, NOW + 1.);
    assert!(!e.store().loaded().markets);
    assert!(!e.http_preparation_pending(old));
    e.handle(NOW + 1001., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let next = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { preparation, .. } => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert_ne!(next, old);
    reply(&mut e, NOW + 1002., &unsigned, support::MARKETS);
    assert!(!e.store().loaded().markets);
    let r = release(&mut e, next, NOW + 1003.);
    reply(&mut e, NOW + 1004., &r, support::MARKETS);
    assert!(e.store().loaded().markets);
}

#[test]
fn original_deadline_retires_parked_and_released_seed_without_publication() {
    for claimed in [false, true] {
        let (mut e, actions) = start(false);
        let (p, unsigned, timer) =
            prepare(&actions, "https://api.prod.paradex.trade/v1/markets", NOW);
        if claimed {
            release(&mut e, p, NOW + 15_000.);
        }
        e.handle(NOW + 20_000., Event::TimerFired { timer });
        let actions: Vec<_> = e.drain_actions().collect();
        retry_timer(&actions, NOW + 20_000.);
        assert!(!e.http_preparation_pending(p));
        assert!(!e.store().loaded().markets);
        reply(&mut e, NOW + 20_001., &unsigned, support::MARKETS);
        assert!(!e.store().loaded().markets);
    }
}

#[test]
fn streamed_valid_prefix_is_discarded_on_transport_failure() {
    let (mut e, actions) = start(false);
    let (p, _, _) = prepare(&actions, "https://api.prod.paradex.trade/v1/markets", NOW);
    let r = release(&mut e, p, NOW + 1.);
    e.handle(
        NOW + 2.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    let prefix = support::MARKETS.strip_suffix("]}").unwrap();
    e.handle(
        NOW + 2.,
        Event::HttpChunk {
            request: r.id,
            data: prefix.as_bytes(),
        },
    );
    assert!(!e.store().loaded().markets);
    e.handle(
        NOW + 3.,
        Event::HttpFailed {
            request: r.id,
            error: "failed after prefix",
        },
    );
    retry_timer(&e.drain_actions().collect::<Vec<_>>(), NOW + 3.);
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
    reply(&mut e, NOW + 4., &r, support::MARKETS);
    assert!(!e.store().loaded().markets);
}
