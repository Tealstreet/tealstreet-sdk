//! Product differences from Bybit's V5 instruments, demo and WebSocket docs.
mod support;

use safe_cex_engine::{Command, Event};
use safe_cex_venue_bybit::{Endpoints, Options, Variant};
use support::{Harness, INSTRUMENTS, PRELAUNCH, TICKERS};

// Spot Get Instruments Info example: basePrecision replaces derivatives' qtyStep.
const SPOT: &str = r#"{"retCode":0,"retMsg":"OK","result":{"category":"spot","list":[{"symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","status":"Trading","lotSizeFilter":{"basePrecision":"0.000001","quotePrecision":"0.00000001","minOrderQty":"0.00001","minOrderAmt":"5","maxLimitOrderQty":"100","maxMarketOrderQty":"10"},"priceFilter":{"tickSize":"0.01"}},{"symbol":"ETHBTC","baseCoin":"ETH","quoteCoin":"BTC","status":"Trading","lotSizeFilter":{"basePrecision":"0.0001","minOrderQty":"0.0001","maxOrderQty":"1000"},"priceFilter":{"tickSize":"0.000001"}}]}}"#;

fn started(variant: Variant) -> Harness {
    Harness::started(Options {
        variant,
        public_data_only: true,
        ..Options::default()
    })
}

fn load(h: &mut Harness, category: &str, instruments: &str, tickers: &str, prelaunch: bool) {
    if prelaunch {
        h.respond("status=PreLaunch", 200, PRELAUNCH);
    }
    h.respond("/v5/market/instruments-info", 200, instruments);
    h.respond("/v5/market/tickers", 200, tickers);
    let socket = h.socket(&format!("/v5/public/{category}")).unwrap();
    h.event(Event::WsOpened { socket });
    h.flush_subscriptions();
}

#[test]
fn inverse_uses_usd_instruments_native_sizes_and_inverse_topics() {
    let mut h = started(Variant::Inverse);
    assert!(
        h.requests
            .iter()
            .all(|r| r.url.contains("category=inverse"))
    );
    let instruments = INSTRUMENTS
        .replace("BTCUSDT", "BTCUSD")
        .replace("ETHUSDT", "ETHUSD")
        .replace("\"quoteCoin\":\"USDT\"", "\"quoteCoin\":\"USD\"")
        .replace("\"qtyStep\":\"0.001\"", "\"qtyStep\":\"1\"");
    let tickers = TICKERS
        .replace("BTCUSDT", "BTCUSD")
        .replace("ETHUSDT", "ETHUSD");
    load(&mut h, "inverse", &instruments, &tickers, true);
    let market = h.engine.store().market("BTCUSD").unwrap();
    assert_eq!(market.is_inverse, Some(true));
    assert_eq!(market.precision.amount, 1.0);
    assert!(h.sent_on("/v5/public/inverse")[0].contains("tickers.BTCUSD"));
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSD".into(),
    });
    assert!(
        h.sent_on("/v5/public/inverse")
            .iter()
            .any(|s| s.contains("orderbook.1000.BTCUSD"))
    );
}

#[test]
fn spot_skips_derivative_startup_and_maps_current_instrument_fields() {
    let mut h = Harness::started(Options {
        variant: Variant::Spot,
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        ..Options::default()
    });
    assert_eq!(
        h.requests.len(),
        2,
        "only instruments and tickers before spot account reconciliation"
    );
    assert!(h.requests.iter().all(|r| r.url.contains("category=spot")));
    assert!(
        !h.paths()
            .iter()
            .any(|r| r.contains("limit=") || r.contains("PreLaunch") || r.contains("fee-rate"))
    );
    let tickers = TICKERS.replace("ETHUSDT", "ETHBTC");
    load(&mut h, "spot", SPOT, &tickers, false);
    let market = h.engine.store().market("BTCUSDT").unwrap();
    assert_eq!(market.is_spot, Some(true));
    assert_eq!(market.is_expiring, Some(false));
    assert_eq!(market.precision.amount, 0.000001);
    assert_eq!(
        market.limits.amount.max, 10.0,
        "both market and limit orders stay below their native maxima"
    );
    assert_eq!(
        (market.limits.leverage.min, market.limits.leverage.max),
        (1.0, 1.0)
    );
    assert!(
        !h.paths()
            .iter()
            .any(|r| r.contains("/position/") || r.contains("account/info"))
    );
    assert!(h.sent_on("/v5/public/spot")[0].contains("tickers.ETHBTC"));
    let ticker = h.engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!((ticker.funding_rate, ticker.funding_time), (0.0, 0.0));
}

#[test]
fn spot_depth_survives_gap_recovery_and_reconnect() {
    let mut h = started(Variant::Spot);
    load(&mut h, "spot", SPOT, TICKERS, false);
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    h.message("/v5/public/spot", r#"{"topic":"orderbook.200.BTCUSDT","type":"snapshot","data":{"s":"BTCUSDT","b":[["100","1"]],"a":[["101","2"]],"u":10}}"#);
    assert!(h.engine.book("BTCUSDT").is_some());
    h.message("/v5/public/spot", r#"{"topic":"orderbook.200.BTCUSDT","type":"delta","data":{"s":"BTCUSDT","b":[],"a":[],"u":12}}"#);
    assert!(h.engine.book("BTCUSDT").is_none());
    h.advance(101.0);
    assert!(
        h.sent_on("/v5/public/spot")
            .iter()
            .all(|s| !s.contains("orderbook.1000"))
    );
    let socket = h.socket("/v5/public/spot").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    h.advance(501.0);
    let socket = h.socket("/v5/public/spot").unwrap();
    h.event(Event::WsOpened { socket });
    h.flush_subscriptions();
    assert!(
        h.sent_on("/v5/public/spot")
            .iter()
            .any(|s| s.contains("orderbook.200.BTCUSDT"))
    );
}

#[test]
fn demo_selects_demo_rest_and_mainnet_public_but_retains_testnet_fallback() {
    for testnet in [false, true] {
        let mut h = Harness::started(Options {
            variant: Variant::Demo,
            public_data_only: true,
            testnet,
            ..Options::default()
        });
        let expected = if testnet {
            "https://api-testnet.bybit.com/"
        } else {
            "https://api-demo.bybit.com/"
        };
        assert!(h.requests.iter().all(|r| r.url.starts_with(expected)));
        load(&mut h, "linear", INSTRUMENTS, TICKERS, true);
        let expected = if testnet {
            "wss://stream-testnet.bybit.com/"
        } else {
            "wss://stream.bybit.com/"
        };
        assert!(h.sockets.iter().all(|(_, url)| url.starts_with(expected)));
    }
}

#[test]
fn hyro_requires_explicit_endpoints_and_never_falls_back_to_bybit() {
    let h = started(Variant::Hyrotrader);
    assert!(h.requests.is_empty());
    assert!(h.sockets.is_empty());
    assert!(
        h.logs
            .iter()
            .any(|s| s.contains("endpoints are not configured"))
    );
    let mut h = Harness::started(Options {
        variant: Variant::Hyrotrader,
        public_data_only: true,
        endpoints: Some(Endpoints {
            rest: "http://127.0.0.1:8012/".into(),
            public_ws: "ws://127.0.0.1:8012/v5/public/linear".into(),
            private_ws: "ws://127.0.0.1:8012/v5/private".into(),
        }),
        ..Options::default()
    });
    assert_eq!(h.requests.len(), 2);
    assert!(
        h.requests
            .iter()
            .all(|r| r.url.starts_with("http://127.0.0.1:8012/v5/"))
    );
    let instruments = INSTRUMENTS.replace("\"quoteCoin\":\"USDT\"", "\"quoteCoin\":\"USDC\"");
    load(&mut h, "linear", &instruments, TICKERS, false);
    assert!(
        h.engine.store().markets().is_empty(),
        "Hyro exposes only USDT perpetuals"
    );
    assert!(
        h.sockets
            .iter()
            .all(|(_, url)| url.starts_with("ws://127.0.0.1:8012/"))
    );
}

#[test]
fn malformed_hyro_endpoints_are_rejected_before_io() {
    for (rest, public_ws) in [
        ("https:///", "wss://host/public"),
        ("https://?x", "wss://host/public"),
        ("https://host", "wss://#fragment"),
        ("https://host/path?query", "wss://host/public"),
        ("https://host:bad", "wss://host/public"),
        ("https://host", "wss://host:70000/private"),
    ] {
        let options = Options {
            variant: Variant::Hyrotrader,
            endpoints: Some(Endpoints {
                rest: rest.into(),
                public_ws: public_ws.into(),
                private_ws: "wss://host/private".into(),
            }),
            ..Options::default()
        };
        assert!(options.validate().is_err(), "reject {rest} / {public_ws}");
        let h = Harness::started(options);
        assert!(h.requests.is_empty() && h.sockets.is_empty());
    }
}
