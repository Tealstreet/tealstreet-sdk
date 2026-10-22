//! REST-only delegated signing retains real account reads without private sessions.
mod support;

use safe_cex_engine::signing_policy::PrivateFeedPolicy;
use safe_cex_venue_binance::Variant;
use support::{Harness, Options};

const COIN_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSD_PERP","contractStatus":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"1000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"120"}]}]}"#;
const SPOT_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","baseAssetPrecision":8,"quoteAssetPrecision":8,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.01"},{"filterType":"LOT_SIZE","stepSize":"0.00001","minQty":"0.00001","maxQty":"9000"}]}]}"#;

fn assert_no_private_session(h: &Harness) {
    assert!(h.requests.iter().all(|r| !r.url.contains("listenKey")));
    assert!(h.sockets.iter().all(|(_, url)| !url.contains("/private/")
        && !url.contains("/pm/ws")
        && !url.contains("ws-api")
        && !url.contains("/ws/")));
    assert!(h.sent.iter().all(
        |(_, text)| !text.contains("userDataStream") && !text.contains("session.subscriptions")
    ));
}

fn reply_reads(h: &mut Harness, variant: Variant, portfolio: bool) -> usize {
    let mut reads = 0;
    while !h.requests.is_empty() {
        assert_no_private_session(h);
        let request = h.requests.remove(0);
        let path = request.url.split('?').next().unwrap();
        let symbol = if variant == Variant::CoinM {
            "BTCUSD_PERP"
        } else {
            "BTCUSDT"
        };
        let body = if path.ends_with("exchangeInfo") {
            match variant {
                Variant::CoinM => COIN_INFO.to_owned(),
                Variant::Spot => SPOT_INFO.to_owned(),
                Variant::UsdM | Variant::PortfolioMargin => support::EXCHANGE_INFO.to_owned(),
            }
        } else if path.ends_with("ticker/24hr") {
            format!(
                r#"[{{"symbol":"{symbol}","lastPrice":"100","bidPrice":"99","askPrice":"101","priceChangePercent":"0","volume":"1","quoteVolume":"100","baseVolume":"1"}}]"#
            )
        } else if path.ends_with("ticker/bookTicker") {
            format!(r#"[{{"symbol":"{symbol}","bidPrice":"99","askPrice":"101"}}]"#)
        } else if path.ends_with("premiumIndex") {
            format!(
                r#"[{{"symbol":"{symbol}","markPrice":"100","indexPrice":"100","lastFundingRate":"0","nextFundingTime":0}}]"#
            )
        } else if path.ends_with("leverageBracket") || path.ends_with("symbolConfig") {
            "[]".into()
        } else if path.ends_with("positionSide/dual") {
            r#"{"dualSidePosition":false}"#.into()
        } else if path.ends_with("positionRisk")
            || path.ends_with("openOrders")
            || path.ends_with("openAlgoOrders")
            || path.ends_with("conditional/openOrders")
        {
            reads += 1;
            "[]".into()
        } else if path.ends_with("/account") || path.ends_with("/balance") {
            reads += 1;
            if portfolio {
                r#"[{"asset":"USDT","totalWalletBalance":"100","crossMarginFree":"100","umUnrealizedPNL":"0","cmWalletBalance":"0","cmUnrealizedPNL":"0"}]"#.into()
            } else if variant == Variant::Spot {
                r#"{"balances":[{"asset":"USDT","free":"100","locked":"0"}]}"#.into()
            } else {
                let asset = if variant == Variant::CoinM {
                    "BTC"
                } else {
                    "USDT"
                };
                format!(
                    r#"{{"assets":[{{"asset":"{asset}","walletBalance":"100","availableBalance":"100","initialMargin":"0","unrealizedProfit":"0"}}]}}"#
                )
            }
        } else {
            panic!("unexpected REST-only request: {path}");
        };
        h.reply(request.id, 200, "", &body);
    }
    reads
}

#[test]
fn all_four_aliases_load_and_poll_private_rest_without_any_session_work() {
    for (variant, portfolio) in [
        (Variant::UsdM, false),
        (Variant::PortfolioMargin, true),
        (Variant::CoinM, false),
        (Variant::Spot, false),
    ] {
        let mut engine = support::engine(Options {
            variant,
            portfolio_margin: portfolio,
            key: Some("scripted-key".into()),
            secret: Some("scripted-secret".into()),
            ..Default::default()
        });
        engine
            .set_private_feed_policy(PrivateFeedPolicy::RestOnly)
            .unwrap();
        let mut h = Harness::from_engine(engine);
        assert!(reply_reads(&mut h, variant, portfolio) >= 2);
        let loaded = h.engine.store().loaded();
        assert!(
            loaded.balance && loaded.positions && loaded.orders,
            "{variant:?} portfolio={portfolio}"
        );
        assert_no_private_session(&h);
        // The policy is fixed: a late caller cannot silently create a private stream.
        assert!(
            h.engine
                .set_private_feed_policy(PrivateFeedPolicy::Streaming)
                .is_err()
        );
        let mut polled = 0;
        for _ in 0..24 {
            h.advance(1_000.0);
            polled += reply_reads(&mut h, variant, portfolio);
            assert_no_private_session(&h);
        }
        assert!(polled >= 2, "REST reconciliation must remain active");
    }
}
