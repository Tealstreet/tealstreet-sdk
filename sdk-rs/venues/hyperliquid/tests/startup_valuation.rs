//! Funded unified/portfolio wallets cannot be valued from absent ticker prices.
mod support;
use safe_cex_engine::{Action, Engine, Event, PublicFeedPolicy};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use support::*;

fn collateral() -> &'static str {
    r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"BTC","token":1,"total":"2","hold":"0.5"},{"coin":"USDC","token":0,"total":"100","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"300"]]}}}"#
}

fn clear_prices(engine: &mut Engine<Hyperliquid>) {
    let mut tickers = engine.store().tickers().to_vec();
    for ticker in &mut tickers {
        ticker.last = 0.0;
        ticker.mark = 0.0;
    }
    engine.store_mut().set_tickers(tickers);
}

#[test]
fn unpriced_collateral_retains_prior_usd_until_a_real_live_price_arrives() {
    for mode in ["unifiedAccount", "portfolioMargin"] {
        let (mut engine, socket) = ready(mode);
        clear_prices(&mut engine);
        let old = engine.store().balance().clone();
        message(&mut engine, socket, 2000.0, collateral());
        assert_eq!(engine.store().balance(), &old);
        // A private position update must not value pending collateral as zero.
        message(
            &mut engine,
            socket,
            2100.0,
            &format!(
                r#"{{"channel":"clearinghouseState","data":{}}}"#,
                STATE.replace("1000}", "2100}")
            ),
        );
        assert_eq!(engine.store().balance(), &old);
        message(
            &mut engine,
            socket,
            2200.0,
            r#"{"channel":"allMids","data":{"mids":{"BTC":"120"}}}"#,
        );
        assert_eq!(engine.store().balance().total, 300.0);
        assert_eq!(engine.store().balance().free, 300.0);
        assert_eq!(engine.store().balance().used, 40.0);
        assert_eq!(engine.store().balance().upnl, 40.0);
    }
}

#[test]
fn shared_quote_recovers_pending_native_collateral_without_owned_public_feed() {
    let (mut engine, socket, _) = ready_with_policy("portfolioMargin", PublicFeedPolicy::Shared);
    clear_prices(&mut engine);
    let old = engine.store().balance().clone();
    message(&mut engine, socket, 2000.0, collateral());
    engine
        .apply_public_feed(
            2100.0,
            serde_json::from_str(r#"{"tickers":[{"symbol":"BTC-PERP","bid":119,"ask":121}]}"#)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(engine.store().balance(), &old);
    engine
        .apply_public_feed(
            2200.0,
            serde_json::from_str(r#"{"tickers":[{"symbol":"BTC-PERP","last":120}]}"#).unwrap(),
        )
        .unwrap();
    assert_eq!(engine.store().balance().total, 300.0);
    assert_eq!(engine.store().balance().free, 300.0);
}

#[test]
fn pending_live_collateral_fences_older_rest_and_uses_latest_position_pnl() {
    let (mut engine, socket) = ready("portfolioMargin");
    engine.drain_actions().for_each(drop);
    engine.handle(1500.0, Event::WsOpened { socket });
    let requests = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    clear_prices(&mut engine);
    message(&mut engine, socket, 2000.0, collateral());
    let newer = STATE
        .replace("\"szi\":\"2\"", "\"szi\":\"3\"")
        .replace("\"positionValue\":\"200\"", "\"positionValue\":\"300\"")
        .replace("\"time\":1000", "\"time\":3000");
    message(
        &mut engine,
        socket,
        3000.0,
        &format!(r#"{{"channel":"clearinghouseState","data":{newer}}}"#),
    );
    for request in requests {
        let body = match kind(&request).as_str() {
            "spotClearinghouseState" => {
                r#"{"balances":[{"coin":"USDC","total":"9000","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"8999"]]}"#
            }
            "clearinghouseState" => STATE,
            _ => "[]",
        };
        reply(&mut engine, &request, 200, body);
    }
    message(
        &mut engine,
        socket,
        4000.0,
        r#"{"channel":"allMids","data":{"mids":{"BTC":"120"}}}"#,
    );
    assert_eq!(engine.store().balance().total, 280.0);
    assert_eq!(engine.store().balance().upnl, 60.0);
    assert_eq!(engine.store().balance().free, 300.0);
}

#[test]
fn cold_portfolio_wallet_does_not_load_a_fabricated_zero_collateral_balance() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        key: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some("0x14791697260e4c9a71f18484c9f997b308e59325".into()),
        ..Options::default()
    }));
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    engine.start(1000.0);
    let mut socket = None;
    let mut core_reads = Vec::new();
    for _ in 0..20 {
        let mut actions = engine.drain_actions().collect::<Vec<_>>();
        if actions.is_empty() {
            break;
        }
        actions.sort_by_key(|action| match action {
            Action::Http(request) if kind(request) == "userAbstraction" => 0,
            _ => 1,
        });
        for action in actions {
            match action {
                Action::Http(request) => {
                    if kind(&request) == "clearinghouseState" {
                        core_reads.push(request);
                        continue;
                    }
                    let body = match kind(&request).as_str() {
                        "metaAndAssetCtxs" => META,
                        "perpDexs" => "[null]",
                        "userAbstraction" => "\"portfolioMargin\"",
                        "maxBuilderFee" => "30",
                        "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#,
                        "clearinghouseState" => STATE,
                        "frontendOpenOrders" => "[]",
                        "spotClearinghouseState" => {
                            r#"{"balances":[{"coin":"BTC","token":1,"total":"2","hold":"0.5"},{"coin":"USDC","token":0,"total":"100","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"300"]]}"#
                        }
                        other => panic!("unexpected read {other}"),
                    };
                    reply(&mut engine, &request, 200, body);
                    if kind(&request) == "metaAndAssetCtxs" {
                        clear_prices(&mut engine);
                    }
                }
                Action::WsConnect {
                    socket: connected, ..
                } => {
                    socket = Some(connected);
                    engine.handle(1000.0, Event::WsOpened { socket: connected });
                }
                _ => {}
            }
        }
    }
    assert!(socket.is_some());
    assert!(engine.store().loaded().positions && engine.store().loaded().orders);
    assert!(!engine.store().loaded().balance);
    engine
        .apply_public_feed(
            2000.0,
            serde_json::from_str(r#"{"tickers":[{"symbol":"BTC-PERP","last":120}]}"#).unwrap(),
        )
        .unwrap();
    assert!(
        !engine.store().loaded().balance,
        "a quote cannot establish private exposure"
    );
    assert!(!core_reads.is_empty());
    for request in core_reads {
        reply(&mut engine, &request, 200, STATE);
    }
    assert!(engine.store().loaded().balance);
    assert_eq!(engine.store().balance().total, 300.0);
    assert_eq!(engine.store().balance().free, 300.0);
}

#[test]
fn collateral_tokens_without_a_valuation_market_keep_native_amount_and_load_balance() {
    for mode in ["unifiedAccount", "portfolioMargin"] {
        let (mut engine, socket) = ready(mode);
        // Perp mode has no spot meta: UBTC and airdropped tokens have no mappable market.
        let state = r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"USDC","token":0,"total":"100","hold":"0"},{"coin":"BTC","token":1,"total":"2","hold":"0.5"},{"coin":"UBTC","token":197,"total":"1","hold":"0"},{"coin":"PURRDROP","token":411,"total":"5000","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"300"]]}}}"#;
        message(&mut engine, socket, 2000.0, state);
        let warnings = engine
            .drain_actions()
            .filter(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("no valuation market")))
            .count();
        assert_eq!(engine.store().balance().total, 300.0);
        assert_eq!(engine.store().balance().free, 300.0);
        assert_eq!(warnings, 1);
        message(&mut engine, socket, 2100.0, state);
        assert!(!engine.drain_actions().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("no valuation market"))));
    }
}

#[test]
fn quoted_market_with_missing_price_still_holds_collateral_pending() {
    let (mut engine, socket) = ready("unifiedAccount");
    clear_prices(&mut engine);
    let old = engine.store().balance().clone();
    message(
        &mut engine,
        socket,
        2000.0,
        r#"{"channel":"spotState","data":{"spotState":{"balances":[{"coin":"BTC","token":1,"total":"2","hold":"0"},{"coin":"UBTC","token":197,"total":"1","hold":"0"}],"tokenToAvailableAfterMaintenance":[[0,"300"]]}}}"#,
    );
    assert_eq!(engine.store().balance(), &old);
}
