//! Public catalog snapshots retain their exact managed Background attempt.
mod support;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Permit, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Method, Notice, TimerId, VenueError};
use safe_cex_venue_lighter::{Lighter, Options};
use support::{NOW, reply_at};
const META: &str = r#"{"code":200,"order_book_details":[{"market_id":1,"symbol":"ETH","status":"active","market_type":"perp","supported_size_decimals":3,"supported_price_decimals":2,"min_base_amount":"0.001","min_quote_amount":"1","min_initial_margin_fraction":500,"maintenance_margin_fraction":250},{"market_id":2,"symbol":"BTC","status":"active","market_type":"perp","supported_size_decimals":4,"supported_price_decimals":1,"min_base_amount":"0.0001","min_quote_amount":"1","min_initial_margin_fraction":500,"maintenance_margin_fraction":250},{"market_id":2048,"symbol":"ETH/USDC","status":"active","market_type":"spot"}]}"#;
fn engine(robinhood: bool, testnet: bool) -> Engine<Lighter> {
    safe_cex_engine::test_support::seeded_engine(Lighter::new(Options {
        robinhood,
        testnet,
        public_data_only: true,
        ..Default::default()
    }))
}
fn start(e: &mut Engine<Lighter>) -> Vec<Action> {
    e.start(NOW);
    e.drain_actions().collect()
}
fn no_command(a: &[Action]) {
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
}
fn prep(a: &[Action], fallback: bool) -> (Preparation, HttpRequest) {
    no_command(a);
    assert!(
        !a.iter().any(|a| matches!(a, Action::Http(_))),
        "catalog waits for managed claim"
    );
    let entries: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(entries.len(), 1, "one catalog preparation");
    let (p, r) = entries.into_iter().next().unwrap();
    assert!(r.url.ends_with(if fallback {
        "/orderBooks"
    } else {
        "/orderBookDetails"
    }));
    (p, r)
}
fn bind(e: &mut Engine<Lighter>, now: f64, p: Preparation) -> (Permit, HttpRequest) {
    e.http_prepared(
        now,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    a.into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("catalog binds exact preparation")
}
fn release(e: &mut Engine<Lighter>, now: f64, p: Preparation) -> HttpRequest {
    let (permit, r) = bind(e, now, p);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    r
}
fn wire(r: &HttpRequest, robinhood: bool, testnet: bool, fallback: bool) {
    let host = match (robinhood, testnet) {
        (false, false) => "https://mainnet.zklighter.elliot.ai",
        (false, true) => "https://testnet-api.lighter.xyz",
        (true, false) => "https://api.rh.lighter.xyz",
        (true, true) => "https://api.rh-testnet.lighter.xyz",
    };
    assert_eq!(
        r.url,
        format!(
            "{host}/api/v1/{}",
            if fallback {
                "orderBooks"
            } else {
                "orderBookDetails"
            }
        )
    );
    assert_eq!(r.method, Method::Get);
    assert!(r.headers.is_empty());
    assert!(r.signer_headers.is_empty());
    assert!(r.body.is_none());
}
fn snapshot(e: &Engine<Lighter>) {
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 2);
    let eth = e.store().market("ETH").unwrap();
    assert_eq!(eth.id, "1");
    assert_eq!(eth.precision.amount, 0.001);
    assert_eq!(eth.precision.price, 0.01);
    assert_eq!(eth.limits.leverage.max, 20.);
    assert!(e.store().market("ETH/USDC").is_none());
}
fn empty(e: &mut Engine<Lighter>, now: f64) {
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
    assert!(e.take_frame(now).is_none());
}
fn deadline(a: &[Action]) -> TimerId {
    a.iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20000. => Some(*timer),
            _ => None,
        })
        .expect("original twenty-second attempt deadline")
}
#[test]
fn native_catalog_wire_and_codec_in_all_deployments() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let mut e = engine(robinhood, testnet);
            e.start(NOW);
            let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
            let r = a
                .into_iter()
                .find_map(|a| {
                    if let Action::Http(r) = a {
                        Some(r)
                    } else {
                        None
                    }
                })
                .unwrap();
            wire(&r, robinhood, testnet, false);
            reply_at(&mut e, NOW + 1., &r, 200, META);
            snapshot(&e);
            no_command(&e.drain_actions().collect::<Vec<_>>());
        }
    }
}
#[test]
fn native_primary_failure_uses_order_books_then_existing_backoff() {
    let mut e = engine(false, false);
    e.start(NOW);
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let r = a
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    reply_at(&mut e, NOW + 1., &r, 500, r#"{"code":500}"#);
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.);
    let fallback = a
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    wire(&fallback, false, false, true);
    reply_at(&mut e, NOW + 2., &fallback, 500, r#"{"code":500}"#);
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        a.iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..}if *at_ms>=NOW+752.&&*at_ms<=NOW+1002.))
    );
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    empty(&mut e, NOW + 2.);
    no_command(&a);
}
#[test]
fn native_fallback_alias_keeps_snapshot_schema() {
    let mut e = engine(false, false);
    e.start(NOW);
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    reply_at(
        &mut e,
        NOW + 1.,
        &r,
        200,
        r#"{"code":200,"order_book_details":[]}"#,
    );
    let fallback = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    wire(&fallback, false, false, true);
    reply_at(
        &mut e,
        NOW + 2.,
        &fallback,
        200,
        &META.replace("order_book_details", "order_books"),
    );
    snapshot(&e);
}
#[test]
fn managed_catalog_waits_for_claim_and_uses_exact_wire() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let mut e = engine(robinhood, testnet);
            let a = start(&mut e);
            let (p, unsigned) = prep(&a, false);
            wire(&unsigned, robinhood, testnet, false);
            empty(&mut e, NOW);
            let r = release(&mut e, NOW + 1., p);
            wire(&r, robinhood, testnet, false);
            reply_at(&mut e, NOW + 2., &r, 200, META);
            snapshot(&e);
        }
    }
}
#[test]
fn response_before_claim_cannot_publish_catalog() {
    let mut e = engine(false, false);
    let (p, r) = prep(&start(&mut e), false);
    reply_at(&mut e, NOW + 1., &r, 200, META);
    no_command(&e.drain_actions().collect::<Vec<_>>());
    empty(&mut e, NOW + 1.);
    let r = release(&mut e, NOW + 2., p);
    reply_at(&mut e, NOW + 3., &r, 200, META);
    snapshot(&e);
}
#[test]
fn preparation_refusal_keeps_native_fallback_and_discards_prefix() {
    let mut e = engine(false, false);
    let (p, _) = prep(&start(&mut e), false);
    e.http_preparation_failed(
        NOW + 1.,
        p,
        Error::State,
        VenueError::new("catalog fixture refusal"),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let (fallback, _) = prep(&a, true);
    empty(&mut e, NOW + 1.);
    let r = release(&mut e, NOW + 2., fallback);
    reply_at(
        &mut e,
        NOW + 3.,
        &r,
        200,
        &META.replace("order_book_details", "order_books"),
    );
    snapshot(&e);
}
#[test]
fn bound_catalog_expires_before_claim_and_late_claim_is_inert() {
    let mut e = engine(false, false);
    let a = start(&mut e);
    let timer = deadline(&a);
    let (p, _) = prep(&a, false);
    let (permit, r) = bind(&mut e, NOW + 1., p);
    e.handle(NOW + 20000., Event::TimerFired { timer });
    let a: Vec<_> = e.drain_actions().collect();
    let (fallback, _) = prep(&a, true);
    assert!(e.claim_http(NOW + 20000., permit).is_err());
    reply_at(&mut e, NOW + 20001., &r, 200, META);
    empty(&mut e, NOW + 20001.);
    let a: Vec<_> = e.drain_actions().collect();
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    let next = release(&mut e, NOW + 20002., fallback);
    reply_at(&mut e, NOW + 20003., &next, 200, META);
    snapshot(&e);
}
#[test]
fn released_timeout_and_late_response_cannot_replace_fresh_fallback() {
    let mut e = engine(false, false);
    let a = start(&mut e);
    let timer = deadline(&a);
    let (p, _) = prep(&a, false);
    let old = release(&mut e, NOW + 1., p);
    e.handle(NOW + 20000., Event::TimerFired { timer });
    let (p, _) = prep(&e.drain_actions().collect::<Vec<_>>(), true);
    reply_at(&mut e, NOW + 20001., &old, 200, META);
    empty(&mut e, NOW + 20001.);
    e.drain_actions().for_each(drop);
    let fresh = release(&mut e, NOW + 20002., p);
    reply_at(&mut e, NOW + 20003., &fresh, 200, META);
    snapshot(&e);
    reply_at(&mut e, NOW + 20004., &old, 500, r#"{"code":500}"#);
    assert!(e.drain_actions().next().is_none());
    snapshot(&e);
}
#[test]
fn malformed_catalog_prefix_never_reaches_store() {
    let mut e = engine(false, false);
    let (p, _) = prep(&start(&mut e), false);
    let r = release(&mut e, NOW + 1., p);
    let malformed = format!("{}x", META);
    reply_at(&mut e, NOW + 2., &r, 200, &malformed);
    empty(&mut e, NOW + 2.);
    let a: Vec<_> = e.drain_actions().collect();
    let _ = prep(&a, true);
}
#[test]
fn completed_catalog_duplicate_response_is_inert() {
    let mut e = engine(false, false);
    let (p, _) = prep(&start(&mut e), false);
    let r = release(&mut e, NOW + 1., p);
    reply_at(&mut e, NOW + 2., &r, 200, META);
    snapshot(&e);
    e.drain_actions().for_each(drop);
    e.take_frame(NOW + 2.);
    reply_at(&mut e, NOW + 3., &r, 500, r#"{"code":500}"#);
    assert!(e.drain_actions().next().is_none());
    assert!(e.take_frame(NOW + 3.).is_none());
    snapshot(&e);
}
