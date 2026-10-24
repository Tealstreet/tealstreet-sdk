mod support;

use safe_cex_engine::{Action, Event, HttpRequest, PublicFeedPolicy, SocketChannel, SocketId};
use safe_cex_venue_apexomni::Options;
use support::*;

const ACCOUNT: &str = r#"{"code":0,"data":{"id":"611779989544632629","positions":[{"symbol":"BTC-USDT","side":"LONG","entryPrice":"100","size":"2","customImr":"0.1","unrealisedPnl":"4"},{"symbol":"BTC-USDT","side":"SHORT","entryPrice":"120","size":"1","customImr":"0.1","unrealisedPnl":"3"}],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#;
const BALANCE: &str = r#"{"code":0,"data":{"totalEquityValue":"1000","availableBalance":"800","initialMargin":"100","maintenanceMargin":"20"}}"#;

fn pending(policy: PublicFeedPolicy) -> (TestEngine, SocketId, Option<SocketId>, Vec<HttpRequest>) {
    let mut engine = private_engine(
        Options {
            ..Default::default()
        },
        &Material {
            key: "key",
            signing: "secret",
            passphrase: Some("passphrase"),
            seed: Some(("11".repeat(65)).to_string()),
        },
    );
    engine.set_public_feed_policy(policy).unwrap();
    engine.start(0.);
    let metadata = requests(&mut engine, 0.).remove(0);
    response(&mut engine, &metadata, 10., MARKETS);
    let mut public = None;
    let mut private = None;
    let mut http = Vec::new();
    for action in engine.prepared(10.) {
        match action {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => private = Some(socket),
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                ..
            } => public = Some(socket),
            Action::Http(request) => http.push(request),
            _ => {}
        }
    }
    let private = private.expect("private socket survives the public policy");
    engine.handle(20., Event::WsOpened { socket: private });
    if let Some(socket) = public {
        engine.handle(20., Event::WsOpened { socket });
    }
    engine.prepared(20.).for_each(drop);
    (engine, private, public, http)
}

fn private_seed(engine: &mut TestEngine, http: Vec<HttpRequest>) {
    for request in http {
        let body = if request.url.ends_with("/v3/account") {
            ACCOUNT
        } else {
            r#"{"code":0,"data":[]}"#
        };
        response(engine, &request, 30., body);
    }
    let balance = requests(engine)
        .into_iter()
        .find(|request| request.url.ends_with("/v3/account-balance"))
        .unwrap();
    response(engine, &balance, 40., BALANCE);
    assert!(engine.store().loaded().positions);
    assert!(engine.store().loaded().orders);
    assert!(engine.store().loaded().balance);
}

fn shared_price(engine: &mut TestEngine, json: &str) {
    engine
        .apply_public_feed(50., serde_json::from_str(json).unwrap())
        .unwrap();
}

#[test]
fn shared_prices_match_direct_hedged_position_valuation_without_changing_balance() {
    let (mut direct, _, public, http) = pending(PublicFeedPolicy::Direct);
    private_seed(&mut direct, http);
    let (mut shared, _, absent_public, http) = pending(PublicFeedPolicy::Shared);
    assert!(absent_public.is_none());
    private_seed(&mut shared, http);
    let balance = serde_json::to_value(shared.store().balance()).unwrap();
    ws(
        &mut direct,
        public.unwrap(),
        50.,
        r#"{"topic":"instrumentInfo.all","data":[{"s":"BTCUSDT","p":"108","mp":"110","xp":"109"}]}"#,
    );
    shared_price(
        &mut shared,
        r#"{"tickers":[{"symbol":"BTCUSDT","last":108,"mark":110,"index":109}]}"#,
    );
    for engine in [&direct, &shared] {
        assert_eq!(
            engine
                .store()
                .position("BTCUSDT:long")
                .unwrap()
                .unrealized_pnl,
            20.
        );
        assert_eq!(
            engine
                .store()
                .position("BTCUSDT:short")
                .unwrap()
                .unrealized_pnl,
            10.
        );
    }
    assert_eq!(
        serde_json::to_value(shared.store().balance()).unwrap(),
        balance
    );
    // Shared snapshots retain authoritative marks across last-only patches.
    shared_price(
        &mut shared,
        r#"{"tickers":[{"symbol":"BTCUSDT","last":105}]}"#,
    );
    assert_eq!(
        shared
            .store()
            .position("BTCUSDT:long")
            .unwrap()
            .unrealized_pnl,
        20.
    );
    assert_eq!(
        shared
            .store()
            .position("BTCUSDT:short")
            .unwrap()
            .unrealized_pnl,
        10.
    );
    // An explicit zero mark falls back to last, even if index is present.
    ws(
        &mut direct,
        public.unwrap(),
        60.,
        r#"{"topic":"instrumentInfo.all","data":[{"s":"BTCUSDT","p":"105","mp":"0","xp":"109"}]}"#,
    );
    shared_price(
        &mut shared,
        r#"{"tickers":[{"symbol":"BTCUSDT","last":105,"mark":0,"index":109}]}"#,
    );
    for engine in [&direct, &shared] {
        assert_eq!(
            engine
                .store()
                .position("BTCUSDT:long")
                .unwrap()
                .unrealized_pnl,
            10.
        );
        assert_eq!(
            engine
                .store()
                .position("BTCUSDT:short")
                .unwrap()
                .unrealized_pnl,
            15.
        );
    }
}

#[test]
fn shared_quotes_do_not_seed_private_state_and_survive_delayed_local_prices() {
    let (mut engine, private, _, http) = pending(PublicFeedPolicy::Shared);
    shared_price(
        &mut engine,
        r#"{"tickers":[{"symbol":"BTCUSDT","last":108,"mark":110,"index":109}]}"#,
    );
    assert!(!engine.store().loaded().positions);
    assert!(!engine.store().loaded().orders);
    assert!(!engine.store().loaded().balance);
    assert!(engine.store().positions().is_empty());
    private_seed(&mut engine, http);
    // Model a late native REST quote seed, then process an actual private position update.
    // The engine restores provider quotes before invoking the venue valuation hook.
    engine.store_mut().update_ticker("BTCUSDT", |ticker| {
        ticker.mark = 999.;
        ticker.last = 999.;
    });
    ws(
        &mut engine,
        private,
        100.,
        r#"{"topic":"ws_zk_accounts_v3","type":"delta","contents":{"positions":[{"symbol":"BTC-USDT","side":"LONG","entryPrice":"100","size":"3","customImr":"0.1","unrealisedPnl":"0"}]}}"#,
    );
    assert_eq!(engine.store().ticker("BTCUSDT").unwrap().mark, 110.);
    assert_eq!(engine.store().ticker("BTCUSDT").unwrap().last, 108.);
    assert_eq!(
        engine.store().position("BTCUSDT:long").unwrap().contracts,
        3.
    );
    assert_eq!(
        engine
            .store()
            .position("BTCUSDT:long")
            .unwrap()
            .unrealized_pnl,
        30.
    );
    assert_eq!(
        engine
            .store()
            .position("BTCUSDT:short")
            .unwrap()
            .unrealized_pnl,
        10.
    );
}

#[test]
fn invalid_or_overflowing_public_reference_retains_native_position_pnl() {
    let (mut engine, _, public, http) = pending(PublicFeedPolicy::Direct);
    private_seed(&mut engine, http);
    ws(
        &mut engine,
        public.unwrap(),
        50.,
        r#"{"topic":"instrumentInfo.all","data":[{"s":"BTCUSDT","p":"-1","mp":"-2"}]}"#,
    );
    assert_eq!(
        engine
            .store()
            .position("BTCUSDT:long")
            .unwrap()
            .unrealized_pnl,
        4.0
    );
    assert_eq!(
        engine
            .store()
            .position("BTCUSDT:short")
            .unwrap()
            .unrealized_pnl,
        3.0
    );
    ws(
        &mut engine,
        public.unwrap(),
        60.,
        r#"{"topic":"instrumentInfo.all","data":[{"s":"BTCUSDT","p":"1e308","mp":"1e308"}]}"#,
    );
    assert_eq!(
        engine
            .store()
            .position("BTCUSDT:long")
            .unwrap()
            .unrealized_pnl,
        4.0
    );
    assert!(
        engine
            .store()
            .positions()
            .iter()
            .all(|position| position.unrealized_pnl.is_finite())
    );
}
