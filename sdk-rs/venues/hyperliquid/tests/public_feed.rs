//! Mixed public/private websocket ownership follows engine policy across reconnects.
mod support;
use safe_cex_engine::command::{HostInput, decode_host_input};
use safe_cex_engine::{Action, Command, Event, PublicFeedPolicy, SocketChannel, SocketId};
use support::*;

fn assert_private_only(messages: &[String]) {
    let subscriptions: Vec<serde_json::Value> = messages
        .iter()
        .filter_map(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .filter(|message| message["method"] == "subscribe")
        .collect();
    for kind in [
        "clearinghouseState",
        "orderUpdates",
        "userFills",
        "userEvents",
        "activeAssetData",
    ] {
        assert!(
            subscriptions
                .iter()
                .any(|message| message["subscription"]["type"] == kind),
            "missing private {kind}"
        );
    }
    assert!(
        subscriptions.iter().any(
            |message| message["subscription"]["type"] == "clearinghouseState"
                && message["subscription"]["dex"] == "xyz"
        )
    );
    assert!(
        subscriptions.iter().all(|message| ![
            "allMids",
            "fastAssetCtxs",
            "l2Book",
            "bbo",
            "candle"
        ]
        .iter()
        .any(|kind| message["subscription"]["type"] == *kind)),
        "unexpected public subscriptions: {subscriptions:?}"
    );
}
fn sends(actions: Vec<Action>) -> Vec<String> {
    actions
        .into_iter()
        .filter_map(|action| {
            if let Action::WsSend { text, .. } = action {
                Some(text)
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn non_direct_account_socket_retains_private_topics_and_skips_public_topics_on_reconnect() {
    for policy in [PublicFeedPolicy::Disabled, PublicFeedPolicy::Shared] {
        let (mut engine, socket, initial) = ready_with_policy("disabled", policy);
        assert_private_only(&initial);
        assert_eq!(
            engine.store().markets().len(),
            3,
            "native core and HIP-3 metadata still loaded"
        );
        assert_eq!(engine.store().market("BTC-PERP").unwrap().id, "BTC");
        assert_eq!(engine.store().balance().total, 1000.);
        assert_eq!(engine.store().positions()[0].contracts, 2.);
        assert_eq!(engine.store().setting("BTC-PERP").unwrap().leverage, 10.);
        for command in [
            Command::SubscribeOrderBook {
                symbol: "BTC-PERP".into(),
            },
            Command::SubscribeCandles {
                symbol: "BTC-PERP".into(),
                interval: "1m".into(),
            },
        ] {
            engine.handle(2000., Event::Command(command));
        }
        assert!(sends(engine.drain_actions().collect()).is_empty());
        assert!(engine.book("BTC-PERP").is_none());
        message(
            &mut engine,
            socket,
            2000.,
            r#"{"channel":"allMids","data":{"mids":{"BTC":"999"}}}"#,
        );
        assert_eq!(
            engine.store().ticker("BTC-PERP").unwrap().last,
            100.,
            "unsolicited public mids cannot bypass ownership"
        );
        assert_eq!(engine.store().balance().upnl, 0.);
        // A live account frame still updates native positions, settings and balance.
        message(
            &mut engine,
            socket,
            3000.,
            r#"{"channel":"clearinghouseState","data":{"user":"x","dex":"","clearinghouseState":{"assetPositions":[{"position":{"coin":"BTC","szi":"3","entryPx":"100","positionValue":"300","unrealizedPnl":"0","marginUsed":"30","leverage":{"type":"isolated","value":8}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"30"},"time":3000}}}"#,
        );
        assert_eq!(engine.store().positions()[0].contracts, 3.);
        assert_eq!(engine.store().setting("BTC-PERP").unwrap().leverage, 8.);
        assert_eq!(engine.store().balance().used, 30.);
        engine.drain_actions().for_each(drop);
        let closed_at = engine.now.max(4000.);
        engine.handle(closed_at, Event::WsClosed { socket, code: 1006 });
        let timer = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if at_ms == closed_at + 1000. => Some(timer),
                _ => None,
            })
            .unwrap();
        engine.handle(closed_at + 2000., Event::TimerFired { timer });
        let replacement = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::WsConnect {
                    socket, channel, ..
                } => {
                    assert_eq!(channel, Some(SocketChannel::Private));
                    Some(socket)
                }
                _ => None,
            })
            .unwrap();
        assert_ne!(replacement, SocketId(0));
        engine.handle(
            closed_at + 2000.,
            Event::WsOpened {
                socket: replacement,
            },
        );
        assert_private_only(&sends(engine.drain_actions().collect()));
    }
}

#[test]
fn shared_native_prices_revalue_private_account_without_venue_public_subscriptions() {
    let (mut engine, socket, _) = ready_with_policy("disabled", PublicFeedPolicy::Shared);
    let HostInput::PublicFeedUpdate(update) = decode_host_input(
        r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"BTC-PERP","last":110}]}"#,
    )
    .unwrap() else {
        panic!("shared update")
    };
    let now = engine.now;
    engine.apply_public_feed(now, update).unwrap();
    assert_eq!(engine.store().balance().total, 1000.);
    assert_eq!(engine.store().balance().upnl, 20.);
    assert_eq!(engine.store().balance().free, 1000.);
    assert_eq!(engine.store().positions()[0].contracts, 2.);
    assert_eq!(engine.store().setting("BTC-PERP").unwrap().leverage, 10.);
    assert_eq!(engine.store().market("BTC-PERP").unwrap().id, "BTC");
    message(
        &mut engine,
        socket,
        2000.,
        r#"{"channel":"allMids","data":{"mids":{"BTC":"999"}}}"#,
    );
    assert_eq!(engine.store().ticker("BTC-PERP").unwrap().last, 110.);
    assert_eq!(engine.store().balance().upnl, 20.);
    // A delayed native REST seed can replace quotes before a private reconciliation.
    let mut native_seed = engine.store().tickers().to_vec();
    native_seed
        .iter_mut()
        .find(|ticker| ticker.symbol == "BTC-PERP")
        .unwrap()
        .last = 456.;
    engine.store_mut().set_tickers(native_seed);
    message(
        &mut engine,
        socket,
        3000.,
        r#"{"channel":"clearinghouseState","data":{"user":"x","dex":"","clearinghouseState":{"assetPositions":[{"position":{"coin":"BTC","szi":"3","entryPx":"100","positionValue":"300","unrealizedPnl":"0","marginUsed":"30","leverage":{"type":"isolated","value":8}}}],"marginSummary":{"accountValue":"1000","totalMarginUsed":"30"},"time":3000}}}"#,
    );
    assert_eq!(engine.store().balance().total, 1000.);
    assert_eq!(
        engine.store().balance().upnl,
        30.,
        "new private position is valued at provider last110"
    );
    assert_eq!(engine.store().balance().free, 1000.);
    assert_eq!(engine.store().positions()[0].contracts, 3.);
}

#[test]
fn shared_prices_cannot_mark_a_cold_private_account_loaded() {
    use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
    use support::TestEngine as Engine;
    let mut engine = Engine::new(Hyperliquid::new(Options {
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    engine.start(1000.);
    let mode = engine.wait_http(|request| kind(request) == "userAbstraction");
    reply(&mut engine, &mode, 200, r#""disabled""#);
    let metadata = engine.wait_http(|request| kind(request) == "metaAndAssetCtxs");
    reply(&mut engine, &metadata, 200, META);
    assert!(!engine.store().loaded().markets);
    // Keep real fee/dex discovery pending, so account_start has not installed
    // even its default private balance. Seed only a typed cached public catalog
    // to exercise the shared-price hook against a genuinely cold account.
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
    engine.store_mut().set_markets(vec![Market {
        id: "BTC".into(),
        symbol: "BTC-PERP".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDC".into(),
        active: true,
        precision: Precision {
            amount: 0.00001,
            price: 1.,
        },
        limits: Limits {
            amount: MinMax { min: 0., max: 1e9 },
            leverage: MinMax { min: 1., max: 50. },
            price: None,
        },
        contract_size: Some(1.),
        is_expiring: None,
        is_inverse: Some(false),
        is_spot: Some(false),
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    engine.store_mut().set_tickers(vec![Ticker {
        id: "BTC".into(),
        symbol: "BTC-PERP".into(),
        last: 100.,
        mark: 100.,
        ..Default::default()
    }]);
    assert!(engine.store().loaded().markets);
    assert!(!engine.store().loaded().balance);
    let HostInput::PublicFeedUpdate(update) = decode_host_input(
        r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"BTC-PERP","last":110}]}"#,
    )
    .unwrap() else {
        panic!("shared update")
    };
    let now = engine.now;
    engine.apply_public_feed(now, update).unwrap();
    assert_eq!(engine.store().ticker("BTC-PERP").unwrap().last, 110.);
    assert!(
        !engine.store().loaded().balance,
        "shared prices do not substitute for private account success"
    );
}
