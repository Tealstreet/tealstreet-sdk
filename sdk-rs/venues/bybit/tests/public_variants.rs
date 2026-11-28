//! Product differences from Bybit's V5 instruments, demo and WebSocket docs.
mod support;

use safe_cex_engine::{Command, Event, SocketChannel};
use safe_cex_venue_bybit::{Endpoints, Variant};
use support::Options;
use support::{ACCOUNT_INFO, FEE_RATES, Harness, INSTRUMENTS, PRELAUNCH, TICKERS};

// Spot Get Instruments Info example: basePrecision replaces derivatives' qtyStep.
const SPOT: &str = r#"{"retCode":0,"retMsg":"OK","result":{"category":"spot","list":[{"symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","status":"Trading","lotSizeFilter":{"basePrecision":"0.000001","quotePrecision":"0.00000001","minOrderQty":"0.00001","minOrderAmt":"5","maxLimitOrderQty":"100","maxMarketOrderQty":"10"},"priceFilter":{"tickSize":"0.01"}},{"symbol":"ETHBTC","baseCoin":"ETH","quoteCoin":"BTC","status":"Trading","lotSizeFilter":{"basePrecision":"0.0001","minOrderQty":"0.0001","maxOrderQty":"1000"},"priceFilter":{"tickSize":"0.000001"}}]}}"#;

fn started(variant: Variant) -> Harness {
    Harness::started(Options {
        variant,
        public_data_only: true,
        ..Options::default()
    })
}

fn public_options(variant: Variant) -> Options {
    Options {
        variant,
        public_data_only: true,
        endpoints: variant.is_hyro().then(|| Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    }
}

#[test]
fn every_public_profile_requests_its_complete_market_sources() {
    for (variant, category) in [
        (Variant::Linear, "linear"),
        (Variant::Inverse, "inverse"),
        (Variant::Spot, "spot"),
        (Variant::Demo, "linear"),
        (Variant::Hyrotrader, "linear"),
    ] {
        let h = Harness::started(public_options(variant));
        let mut sent: Vec<_> = h
            .requests
            .iter()
            .map(|request| {
                let path = request.url.split_once("/v5/").unwrap().1;
                format!("/v5/{path}")
            })
            .collect();
        sent.sort();
        let mut expected = vec![format!("/v5/market/tickers?category={category}")];
        if variant.is_spot() {
            expected.push("/v5/market/instruments-info?category=spot".into());
        } else {
            expected.push(format!(
                "/v5/market/instruments-info?category={category}&limit=1000"
            ));
            expected.push(format!(
                "/v5/market/instruments-info?category={category}&limit=1000&status=PreLaunch"
            ));
        }
        expected.sort();
        assert_eq!(sent, expected, "startup source mismatch for {variant:?}");
    }
}

#[test]
fn every_profile_preserves_socket_roles_and_origins_through_reconnect() {
    for variant in [
        Variant::Linear,
        Variant::Inverse,
        Variant::Spot,
        Variant::Demo,
        Variant::Hyrotrader,
    ] {
        for testnet in [false, true] {
            let mut options = public_options(variant);
            options.public_data_only = false;
            options.key = Some("test-key".into());
            options.secret = Some("test-secret".into());
            options.testnet = testnet;
            if let Some(endpoints) = &mut options.endpoints {
                // Roles must survive even when a compatible API shares a URL.
                endpoints.public_ws = "wss://hyro.example/shared-feed".into();
                endpoints.private_ws = endpoints.public_ws.clone();
            }
            let mut h = Harness::started(options);
            if h.has_request("status=PreLaunch") {
                h.respond("status=PreLaunch", 200, PRELAUNCH);
            }
            if h.has_request("/v5/account/fee-rate") {
                h.respond("/v5/account/fee-rate", 200, FEE_RATES);
            }
            if h.has_request("/v5/account/info") {
                h.respond("/v5/account/info", 200, ACCOUNT_INFO);
            }
            h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
            h.respond("/v5/market/tickers", 200, TICKERS);
            let category = if variant.is_spot() {
                "spot"
            } else if variant.is_inverse() {
                "inverse"
            } else {
                "linear"
            };
            let network = if testnet { "-testnet" } else { "" };
            let expected_public = if variant.is_hyro() {
                "wss://hyro.example/shared-feed".into()
            } else {
                format!("wss://stream{network}.bybit.com/v5/public/{category}")
            };
            let expected_private = if variant.is_hyro() {
                "wss://hyro.example/shared-feed".into()
            } else if variant == Variant::Demo && !testnet {
                "wss://stream-demo.bybit.com/v5/private".into()
            } else {
                format!("wss://stream{network}.bybit.com/v5/private")
            };
            for (role, expected) in [
                (SocketChannel::Public, expected_public),
                (SocketChannel::Private, expected_private),
            ] {
                let socket = h
                    .socket_channels
                    .iter()
                    .find(|(_, channel)| channel.as_ref() == Some(&role))
                    .map(|(socket, _)| *socket)
                    .expect("declared role");
                assert_eq!(
                    h.sockets.iter().find(|(id, _)| *id == socket).unwrap().1,
                    expected,
                    "{variant:?} testnet={testnet}"
                );
                h.event(Event::WsClosed { socket, code: 1006 });
                h.advance(501.0);
                let (new_socket, new_role) = h.socket_channels.last().unwrap();
                assert_ne!(*new_socket, socket);
                assert_eq!(new_role.as_ref(), Some(&role));
                assert_eq!(h.sockets.last().unwrap().1, expected);
            }
        }
    }
}

#[test]
fn hyro_merges_paginated_prelaunch_markets_and_deduplicates_by_symbol() {
    let mut h = Harness::started(public_options(Variant::Hyrotrader));
    h.respond(
        "/v5/market/instruments-info?category=linear&limit=1000",
        200,
        INSTRUMENTS,
    );
    h.respond("/v5/market/tickers", 200, TICKERS);
    assert!(!h.engine.store().loaded().markets);
    let prelaunch = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"NEWUSDT","status":"PreLaunch","baseCoin":"NEW","quoteCoin":"USDT","contractType":"LinearPerpetual","lotSizeFilter":{"qtyStep":"1","minOrderQty":"1","maxOrderQty":"1000"},"priceFilter":{"tickSize":"0.01"}},{"symbol":"NEWUSDC","status":"PreLaunch","baseCoin":"NEW","quoteCoin":"USDC"}],"nextPageCursor":"second"}}"#;
    h.respond("status=PreLaunch", 200, prelaunch);
    assert!(
        !h.engine.store().loaded().markets,
        "first page is not a complete snapshot"
    );
    assert!(h.paths().iter().any(|path| path.ends_with("category=linear&limit=1000&status=PreLaunch&cursor=second")));
    let duplicate = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTCUSDT","status":"PreLaunch","baseCoin":"BTC","quoteCoin":"USDT","contractType":"LinearPerpetual","lotSizeFilter":{"qtyStep":"0.01","minOrderQty":"0.01","maxOrderQty":"10"},"priceFilter":{"tickSize":"0.1"}}],"nextPageCursor":""}}"#;
    h.respond("cursor=second", 200, duplicate);
    let markets = h.engine.store().markets();
    assert_eq!(
        markets
            .iter()
            .map(|m| m.symbol.as_str())
            .collect::<Vec<_>>(),
        ["BTCUSDT", "ETHUSDT", "NEWUSDT"]
    );
    assert!(h.engine.store().market("NEWUSDT").unwrap().active);
    assert_eq!(
        h.engine.store().market("BTCUSDT").unwrap().precision.amount,
        0.01,
        "the later row replaces the duplicate without changing insertion order"
    );
    assert!(h.engine.store().market("NEWUSDC").is_none());
    assert!(h.engine.store().loaded().markets && h.engine.store().loaded().tickers);
}

#[test]
fn hyro_optional_prelaunch_failure_preserves_successful_trading_startup() {
    for (status, body) in [
        (404, r#"{"retCode":10001,"retMsg":"not supported"}"#),
        (200, r#"{"retCode":10001,"retMsg":"not supported"}"#),
    ] {
        let mut h = Harness::started(public_options(Variant::Hyrotrader));
        h.respond("status=PreLaunch", status, body);
        h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
        h.respond("/v5/market/tickers", 200, TICKERS);
        assert_eq!(h.engine.store().markets().len(), 2);
        assert!(h.engine.store().loaded().markets && h.engine.store().loaded().tickers);
        assert!(
            h.logs
                .iter()
                .any(|message| message.contains("PreLaunch markets, continuing"))
        );
        assert!(!h.has_request("/v5/market/instruments-info"));
    }
}

#[test]
fn optional_prelaunch_grace_starts_after_trading_and_ignores_late_pages() {
    for variant in [
        Variant::Linear,
        Variant::Inverse,
        Variant::Demo,
        Variant::Hyrotrader,
    ] {
        let mut h = Harness::started(public_options(variant));
        let optional = h.take("status=PreLaunch");
        h.respond("/v5/market/tickers", 200, TICKERS);
        h.advance(5_000.0);
        assert!(!h.engine.store().loaded().markets);
        h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
        h.advance(1_499.0);
        assert!(
            !h.engine.store().loaded().markets,
            "full grace for {variant:?}"
        );
        h.advance(1.0);
        assert!(h.engine.store().loaded().markets && h.engine.store().loaded().tickers);
        assert_eq!(h.engine.store().markets().len(), 2);
        let logs = h.logs.len();
        let late = INSTRUMENTS
            .replace("BTCUSDT", "LATEUSDT")
            .replace("\"nextPageCursor\":\"\"", "\"nextPageCursor\":\"late\"");
        h.reply(optional.id, 200, &late);
        assert_eq!(h.engine.store().markets().len(), 2);
        assert!(h.engine.store().market("LATEUSDT").is_none());
        assert!(
            !h.has_request("/v5/market/instruments-info"),
            "late pages stay fenced"
        );
        assert_eq!(h.logs.len(), logs);
    }
}

#[test]
fn prelaunch_success_cancels_grace_and_stale_timer_cannot_replace_markets() {
    for optional_first in [false, true] {
        let mut h = Harness::started(public_options(Variant::Hyrotrader));
        if optional_first {
            h.respond("status=PreLaunch", 200, PRELAUNCH);
        }
        let trading = h.take("/v5/market/instruments-info");
        h.reply(trading.id, 200, INSTRUMENTS);
        h.respond("/v5/market/tickers", 200, TICKERS);
        let grace = h
            .timers
            .iter()
            .find(|(_, at)| *at == h.now + 1_500.0)
            .map(|(id, _)| *id);
        assert_eq!(grace.is_some(), !optional_first);
        if !optional_first {
            h.advance(1_499.0);
            h.respond("status=PreLaunch", 200, PRELAUNCH);
        }
        assert!(h.engine.store().loaded().markets);
        let logs = h.logs.len();
        if let Some(timer) = grace {
            assert!(!h.timers.iter().any(|(id, _)| *id == timer));
            h.event(Event::TimerFired { timer });
        }
        h.advance(1_501.0);
        assert_eq!(h.engine.store().markets().len(), 2);
        assert_eq!(h.logs.len(), logs);
    }
}

#[test]
fn trading_retry_retains_full_grace_and_timeout_discards_partial_optional_pages() {
    let mut h = Harness::started(public_options(Variant::Hyrotrader));
    let first_optional = h.take("status=PreLaunch");
    h.respond("/v5/market/instruments-info", 503, "unavailable");
    h.respond("/v5/market/tickers", 200, TICKERS);
    h.advance(1_000.0);
    let trading_page = INSTRUMENTS.replace(
        "\"nextPageCursor\":\"\"",
        "\"nextPageCursor\":\"trading-next\"",
    );
    h.respond("/v5/market/instruments-info", 200, &trading_page);
    h.advance(2_000.0);
    assert!(!h.engine.store().loaded().markets);
    h.respond("cursor=trading-next", 200, PRELAUNCH);
    let partial = INSTRUMENTS
        .replace("BTCUSDT", "PARTIALUSDT")
        .replace("\"nextPageCursor\":\"\"", "\"nextPageCursor\":\"next\"");
    h.reply(first_optional.id, 200, &partial);
    let late_page = h.take("cursor=next");
    h.advance(1_499.0);
    assert!(!h.engine.store().loaded().markets);
    h.advance(1.0);
    assert!(h.engine.store().loaded().markets);
    assert!(h.engine.store().market("PARTIALUSDT").is_none());
    let logs = h.logs.len();
    h.reply(late_page.id, 503, "late failure");
    h.advance(2_000.0);
    assert_eq!(h.engine.store().markets().len(), 2);
    assert!(!h.has_request("/v5/market/instruments-info"));
    assert_eq!(
        h.logs.len(),
        logs,
        "late failures neither warn nor schedule retries"
    );
}

#[test]
fn timed_out_optional_result_stays_fenced_while_fees_are_pending() {
    let mut h = Harness::started(Options {
        key: Some("test-key".into()),
        secret: Some("test-secret".into()),
        ..Options::default()
    });
    let optional = h.take("status=PreLaunch");
    h.respond("/v5/account/info", 200, ACCOUNT_INFO);
    h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
    h.respond("/v5/market/tickers", 200, TICKERS);
    h.advance(1_500.0);
    assert!(!h.engine.store().loaded().markets, "fees remain required");
    let late = INSTRUMENTS
        .replace("BTCUSDT", "LATEUSDT")
        .replace("\"nextPageCursor\":\"\"", "\"nextPageCursor\":\"late\"");
    h.reply(optional.id, 200, &late);
    assert!(!h.has_request("cursor=late"));
    h.respond("/v5/account/fee-rate", 200, FEE_RATES);
    assert!(h.engine.store().loaded().markets);
    assert_eq!(h.engine.store().markets().len(), 2);
    assert!(h.engine.store().market("LATEUSDT").is_none());
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
        3,
        "instruments, tickers and account metadata precede spot reconciliation"
    );
    h.respond("/v5/account/info", 200, ACCOUNT_INFO);
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
    assert!(!h.paths().iter().any(|r| r.contains("/position/")));
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
    assert_eq!(h.requests.len(), 3);
    assert!(
        h.requests
            .iter()
            .all(|r| r.url.starts_with("http://127.0.0.1:8012/v5/"))
    );
    let instruments = INSTRUMENTS.replace("\"quoteCoin\":\"USDT\"", "\"quoteCoin\":\"USDC\"");
    load(&mut h, "linear", &instruments, TICKERS, true);
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

#[test]
fn hyro_v1_public_subscribe_and_unsubscribe_batches_stay_within_ten_topics() {
    use safe_cex_engine::socket_lifecycle::Interest;
    use safe_cex_engine::{SubscriptionTopic, Venue};
    let mut h = Harness::started(public_options(Variant::Hyrotrader));
    load(&mut h, "linear", INSTRUMENTS, TICKERS, true);
    let venue = h.engine.venue();
    let (feed, _) = venue
        .subscription_interest(
            h.engine.store(),
            &SubscriptionTopic::Candles {
                symbol: "BTCUSDT".into(),
                interval: safe_cex_engine::model::Interval::M1,
            },
        )
        .unwrap();
    let interests: Vec<_> = (0..33)
        .map(|i| Interest::from_wire_args(format!("tickers.S{i}USDT")))
        .collect();
    for subscribe in [true, false] {
        let mut lengths = Vec::new();
        venue
            .encode_socket_interests(
                h.engine.store(),
                feed,
                subscribe,
                &interests,
                &mut |batch| {
                    #[derive(serde::Deserialize)]
                    struct Frame {
                        op: String,
                        args: Vec<String>,
                    }
                    let sent: Frame = serde_json::from_str(&batch.text).unwrap();
                    assert_eq!(
                        sent.op,
                        if subscribe {
                            "subscribe"
                        } else {
                            "unsubscribe"
                        }
                    );
                    assert_eq!(sent.args.len(), batch.covered_keys.len());
                    lengths.push(sent.args.len());
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(lengths, [10, 10, 10, 3]);
    }
}

#[test]
fn hyro_v1_configured_simulator_hosts_override_bybit_demo_and_testnet() {
    for testnet in [false, true] {
        let mut options = public_options(Variant::Hyrotrader);
        options.testnet = testnet;
        options.endpoints = Some(Endpoints {
            rest: "http://simulator.invalid:8012/".into(),
            public_ws: "ws://simulator.invalid:8012/v5/public/linear".into(),
            private_ws: "ws://simulator.invalid:8012/v5/private".into(),
        });
        let mut h = Harness::started(options);
        assert!(
            h.requests
                .iter()
                .all(|request| request.url.starts_with("http://simulator.invalid:8012/v5/"))
        );
        load(&mut h, "linear", INSTRUMENTS, TICKERS, true);
        assert!(
            h.sockets
                .iter()
                .all(|(_, url)| url.starts_with("ws://simulator.invalid:8012/"))
        );
    }
}

/// `markets` linear USDT perpetuals named `M0USDT`, `M1USDT`, ...
fn many_instruments(markets: usize) -> String {
    let rows: Vec<String> = (0..markets)
        .map(|i| {
            format!(
                r#"{{"symbol":"M{i}USDT","contractType":"LinearPerpetual","status":"Trading","baseCoin":"M{i}","quoteCoin":"USDT","deliveryTime":"0","leverageFilter":{{"minLeverage":"1","maxLeverage":"100.00","leverageStep":"0.01"}},"priceFilter":{{"minPrice":"0.01","maxPrice":"199999.98","tickSize":"0.01"}},"lotSizeFilter":{{"maxOrderQty":"7240.00","minOrderQty":"0.01","qtyStep":"0.01"}},"settleCoin":"USDT"}}"#
            )
        })
        .collect();
    format!(
        r#"{{"retCode":0,"retMsg":"OK","result":{{"category":"linear","list":[{}],"nextPageCursor":""}},"retExtInfo":{{}},"time":1735809771618}}"#,
        rows.join(",")
    )
}

/// Replays the subscribe/unsubscribe frames sent on the public socket and
/// returns the live topic set after the last frame and its largest size.
fn live_public_topics(h: &Harness) -> (std::collections::BTreeSet<String>, usize) {
    let mut live = std::collections::BTreeSet::new();
    let mut peak = 0;
    for text in h.sent_on("/v5/public/linear") {
        let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
        let Some(args) = frame["args"].as_array() else {
            continue;
        };
        for arg in args.iter().map(|a| a.as_str().unwrap().to_owned()) {
            match frame["op"].as_str() {
                Some("subscribe") => live.insert(arg),
                Some("unsubscribe") => live.remove(&arg),
                _ => false,
            };
        }
        peak = peak.max(live.len());
    }
    (live, peak)
}

/// Hyro refuses the subscribe that takes one connection past 32 topics, and
/// every one after it (live 2026-11-28: 32 accepted, the 33rd refused with
/// "at most 32 topics per connection", TEAL-20). Forty markets would ask for
/// 41 topics; the live set must stay at 32 and keep the book being viewed.
#[test]
fn hyro_public_feed_never_asks_one_connection_for_more_than_32_topics() {
    let mut h = Harness::started(public_options(Variant::Hyrotrader));
    load(&mut h, "linear", &many_instruments(40), TICKERS, true);
    h.command(Command::SubscribeOrderBook {
        symbol: "M39USDT".into(),
    });
    h.advance(2_000.0);
    let (live, peak) = live_public_topics(&h);
    assert!(peak <= 32, "Hyro connection reached {peak} topics");
    assert_eq!(live.len(), 32);
    assert!(
        live.iter()
            .any(|t| t.starts_with("orderbook.") && t.ends_with(".M39USDT")),
        "{live:?}"
    );
    // The viewed symbol's ticker is kept ahead of idle markets: sizing reads it.
    assert!(live.contains("tickers.M39USDT"), "{live:?}");
    let notices: Vec<_> = h
        .logs
        .iter()
        .filter(|m| m.contains("exceed the socket interest limit"))
        .collect();
    assert_eq!(notices.len(), 1, "{:?}", h.logs);
}

/// The 32-topic cap is Hyro's alone: Bybit streams every ticker it lists.
#[test]
fn bybit_public_feed_is_not_held_to_hyros_connection_cap() {
    let mut h = Harness::started(public_options(Variant::Linear));
    load(&mut h, "linear", &many_instruments(40), TICKERS, true);
    h.command(Command::SubscribeOrderBook {
        symbol: "M39USDT".into(),
    });
    h.advance(2_000.0);
    let (live, _) = live_public_topics(&h);
    assert_eq!(live.len(), 41, "{live:?}");
}

fn has_book(live: &std::collections::BTreeSet<String>, symbol: &str) -> bool {
    live.iter()
        .any(|t| t.starts_with("orderbook.") && t.ends_with(&format!(".{symbol}")))
}

/// Past the cap, opening a book evicts the OLDEST streams, never an arbitrary
/// live book or the one just opened, and each kept book keeps its own ticker
/// (review of #3837: HashMap order once evicted M19, M17, then M32 and M33).
#[test]
fn hyro_capped_feed_keeps_the_newest_books_each_with_its_ticker() {
    let mut h = Harness::started(public_options(Variant::Hyrotrader));
    load(&mut h, "linear", &many_instruments(40), TICKERS, true);
    for i in 0..36 {
        let symbol = format!("M{i}USDT");
        h.command(Command::SubscribeOrderBook {
            symbol: symbol.clone(),
        });
        h.advance(500.0);
        let (live, peak) = live_public_topics(&h);
        assert!(peak <= 32, "Hyro connection reached {peak} topics");
        assert!(has_book(&live, &symbol), "opened {symbol}: {live:?}");
        assert!(
            live.contains(&format!("tickers.{symbol}")),
            "opened {symbol} without its ticker: {live:?}"
        );
    }
    // Sixteen book-and-ticker pairs fill 32 topics: exactly the sixteen newest.
    let (live, _) = live_public_topics(&h);
    for i in 0..36 {
        let symbol = format!("M{i}USDT");
        assert_eq!(has_book(&live, &symbol), i >= 20, "{symbol}: {live:?}");
        assert_eq!(
            live.contains(&format!("tickers.{symbol}")),
            i >= 20,
            "{symbol}: {live:?}"
        );
    }
    assert_eq!(live.len(), 32);
}

/// With 31 other books open, a newly opened book streams with its own ticker
/// (review of #3837: it used to stream `orderbook.*.M39USDT` without one).
#[test]
fn hyro_book_opened_beside_31_others_streams_with_its_ticker() {
    let mut h = Harness::started(public_options(Variant::Hyrotrader));
    load(&mut h, "linear", &many_instruments(40), TICKERS, true);
    for i in 0..31 {
        h.command(Command::SubscribeOrderBook {
            symbol: format!("M{i}USDT"),
        });
    }
    h.advance(2_000.0);
    h.command(Command::SubscribeOrderBook {
        symbol: "M39USDT".into(),
    });
    h.advance(2_000.0);
    let (live, peak) = live_public_topics(&h);
    assert!(peak <= 32, "Hyro connection reached {peak} topics");
    assert!(has_book(&live, "M39USDT"), "{live:?}");
    assert!(live.contains("tickers.M39USDT"), "{live:?}");
}

/// Every (op, topic) the public socket was sent, in order.
fn public_ops(h: &Harness) -> Vec<(String, String)> {
    let mut ops = Vec::new();
    for text in h.sent_on("/v5/public/linear") {
        let frame: serde_json::Value = serde_json::from_str(&text).unwrap();
        if let (Some(op), Some(args)) = (frame["op"].as_str(), frame["args"].as_array()) {
            for arg in args {
                ops.push((op.to_owned(), arg.as_str().unwrap().to_owned()));
            }
        }
    }
    ops
}

/// A sequence gap takes the book out of the set only for its fresh snapshot.
/// At the cap its ticker stays and its slot is held, so the gap sends the
/// book's own unsubscribe and subscribe and nothing else (review of #3837: the
/// slot went to the oldest evicted book and ticker, eight frames per gap).
#[test]
fn hyro_sequence_gap_at_the_cap_churns_only_the_gapped_book() {
    let mut h = Harness::started(public_options(Variant::Hyrotrader));
    load(&mut h, "linear", &many_instruments(40), TICKERS, true);
    for i in 0..36 {
        h.command(Command::SubscribeOrderBook {
            symbol: format!("M{i}USDT"),
        });
        h.advance(500.0);
    }
    let (live, _) = live_public_topics(&h);
    let book = live
        .iter()
        .find(|t| t.starts_with("orderbook.") && t.ends_with(".M35USDT"))
        .expect("the newest book is live")
        .clone();
    h.message(
        "/v5/public/linear",
        &format!(
            r#"{{"topic":"{book}","type":"snapshot","ts":1,"data":{{"s":"M35USDT","b":[["1.00","1"]],"a":[["1.01","1"]],"u":10,"seq":1}},"cts":1}}"#
        ),
    );
    let before = public_ops(&h).len();
    h.message(
        "/v5/public/linear",
        &format!(
            r#"{{"topic":"{book}","type":"delta","ts":2,"data":{{"s":"M35USDT","b":[],"a":[],"u":13,"seq":2}},"cts":2}}"#
        ),
    );
    h.advance(50.0);
    let (during, _) = live_public_topics(&h);
    assert!(
        !during.contains(&book),
        "the gapped book awaits its snapshot"
    );
    assert!(during.contains("tickers.M35USDT"), "{during:?}");
    assert!(!has_book(&during, "M19USDT") && !during.contains("tickers.M19USDT"));
    h.advance(500.0);
    let (after, peak) = live_public_topics(&h);
    assert!(peak <= 32, "Hyro connection reached {peak} topics");
    assert_eq!(after, live);
    assert_eq!(
        public_ops(&h)[before..],
        [
            ("unsubscribe".to_owned(), book.clone()),
            ("subscribe".to_owned(), book),
        ]
    );
}
