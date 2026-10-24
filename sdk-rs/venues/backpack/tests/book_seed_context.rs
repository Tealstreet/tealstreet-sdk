//! Unsigned subscribed-book seeds keep native book facts under managed custody.
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Response, SocketId, Venue,
};
use safe_cex_venue_backpack::{Backpack, Options};
type Driver = Engine<Backpack>;
const SYMBOL: &str = "SOL_USDC_PERP";
const SNAP: &str =
    r#"{"lastUpdateId":10,"bids":[["140","1"],["139","2"]],"asks":[["142","1"],["143","2"]]}"#;
fn actions(e: &mut Driver, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn http(e: &mut Driver, needle: &str, now: f64) -> HttpRequest {
    actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(needle) => Some(r),
            _ => None,
        })
        .expect(needle)
}
fn reply_at(e: &mut Driver, r: &HttpRequest, now: f64, body: &str) {
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
}
fn reply(e: &mut Driver, r: &HttpRequest, body: &str) {
    reply_at(e, r, 0., body);
}
fn seed() -> (Driver, SocketId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Backpack::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = http(&mut e, "markets", 0.);
    reply(
        &mut e,
        &r,
        r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","visible":true,"quantityIncrement":"0.01","priceIncrement":"0.01","minQuantity":"0.01","imfFunction":{"base":"0.02"}},{"symbol":"SOL_USDC","marketType":"SPOT","quantityIncrement":"0.01","priceIncrement":"0.01"}]"#,
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let requests: Vec<_> = actions
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect();
    for r in requests {
        if r.url.contains("tickers") {
            reply(
                &mut e,
                &r,
                r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.05","volume":"10","quoteVolume":"1410"}]"#,
            );
        } else {
            reply(
                &mut e,
                &r,
                r#"[{"symbol":"SOL_USDC_PERP","markPrice":"140","indexPrice":"139","fundingRate":"0.001","nextFundingTimestamp":1000}]"#,
            );
        }
    }
    e.handle(0.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    (e, socket)
}

fn subscribe(e: &mut Driver, now: f64) {
    e.handle(
        now,
        Event::Command(Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
}
fn request(e: &mut Driver, now: f64) -> HttpRequest {
    http(e, "/depth?", now)
}
#[test]
fn native_wire_seed_book_and_subscription_order_working_control() {
    let (mut e, _) = seed();
    subscribe(&mut e, 0.);
    let all = actions(&mut e, 0.);
    let frame = all
        .iter()
        .position(|a| matches!(a, Action::WsSend { .. }))
        .expect("genuine accepted-stream subscribe frame");
    let index = all
        .iter()
        .position(|a| matches!(a, Action::Http(r) if r.url.contains("/depth?")))
        .unwrap();
    assert!(
        frame < index,
        "native subscription precedes physical depth seed"
    );
    let r = all
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/depth?") => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        r.url,
        "https://api.backpack.exchange/api/v1/depth?symbol=SOL_USDC_PERP"
    );
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.body, None);
    assert_eq!(r.route, None);
    assert_eq!(r.headers, vec![("X-Broker-Id", "1200".into())]);
    reply_at(&mut e, &r, 1., SNAP);
    let book = e.venue().book(SYMBOL).unwrap();
    assert_eq!(book.bids(), [(140., 1.), (139., 2.)]);
    assert_eq!(book.asks(), [(142., 1.), (143., 2.)]);
}
#[test]
fn native_buffered_bridge_updates_seed_working_control() {
    let (mut e, socket) = seed();
    subscribe(&mut e, 0.);
    let r = request(&mut e, 0.);
    e.handle(1., Event::WsMessage { socket, text: r#"{"stream":"depth.SOL_USDC_PERP","data":{"e":"depth","s":"SOL_USDC_PERP","U":9,"u":11,"b":[["140","5"]],"a":[["142","0"]]}}"# });
    reply_at(&mut e, &r, 2., SNAP);
    let book = e.venue().book(SYMBOL).unwrap();
    assert_eq!(book.bids(), [(140., 5.), (139., 2.)]);
    assert_eq!(book.asks(), [(143., 2.)]);
}
#[test]
fn native_generation_rejects_unsubscribed_seed_working_control() {
    let (mut e, _) = seed();
    subscribe(&mut e, 0.);
    let old = request(&mut e, 0.);
    e.handle(
        1.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
    e.drain_actions().for_each(drop);
    subscribe(&mut e, 2.);
    let current = request(&mut e, 2.);
    reply_at(&mut e, &old, 3., SNAP);
    assert!(e.venue().book(SYMBOL).is_none());
    reply_at(&mut e, &current, 4., SNAP);
    assert!(e.venue().book(SYMBOL).is_some());
}
#[test]
fn subscribed_seed_cannot_release_before_provider_prepare_bind_claim() {
    let (mut e, _) = seed();
    subscribe(&mut e, 0.);
    let queued: Vec<_> = e.drain_actions().collect();
    assert!(
        !queued
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/depth?"))),
        "no physical seed before provider handoff"
    );
    assert_eq!(queued.iter().filter(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { request, .. } if request.url.contains("/depth?")))).count(), 1);
}
#[test]
fn invalid_background_clock_cannot_start_seed_transport() {
    let (mut e, _) = seed();
    subscribe(&mut e, f64::NAN);
    let queued: Vec<_> = e.drain_actions().collect();
    assert!(
        !queued
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/depth?"))),
        "invalid Read clock creates no physical seed"
    );
    assert!(!queued.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. }))));
    assert!(e.venue().book(SYMBOL).is_none());
}
#[test]
fn seed_data_after_new_admission_deadline_cannot_publish_a_book() {
    let (mut e, _) = seed();
    subscribe(&mut e, 0.);
    let r = request(&mut e, 0.);
    reply_at(&mut e, &r, 20_001., SNAP);
    assert!(
        e.venue().book(SYMBOL).is_none(),
        "new20s admission bound rejects late native seed DATA"
    );
}
