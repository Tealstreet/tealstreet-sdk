//! Native wallet amounts precede public quotes; no real account or transport.
use support::Options;
mod support;
use safe_cex_engine::model::{Balance, CoinBalance};
use safe_cex_engine::{Command, Event};
use safe_cex_venue_binance::Variant;
use support::Harness;
const INVERSE: &str = r#"{"symbols":[{"symbol":"BTCUSD_PERP","contractStatus":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"1000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"120"}]}]}"#;
const LINEAR_WALLET: &str = r#"{"assets":[{"asset":"USDT","walletBalance":"5000","availableBalance":"4952","initialMargin":"48","unrealizedProfit":"10"},{"asset":"BTC","walletBalance":"0.01","availableBalance":"0.01","initialMargin":"0","unrealizedProfit":"0"}]}"#;
const INVERSE_WALLET: &str = r#"{"assets":[{"asset":"BTC","walletBalance":"1","availableBalance":"0.8","initialMargin":"0.2","unrealizedProfit":"0.01"}]}"#;
fn pending(inverse: bool) -> Harness {
    let mut h = Harness::started(Options {
        variant: if inverse {
            Variant::CoinM
        } else {
            Variant::UsdM
        },
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        ..Default::default()
    });
    h.respond(
        "exchangeInfo",
        200,
        if inverse {
            INVERSE
        } else {
            support::EXCHANGE_INFO
        },
    );
    h
}
fn wallet(inverse: bool) -> &'static str {
    if inverse {
        INVERSE_WALLET
    } else {
        LINEAR_WALLET
    }
}
fn risk(inverse: bool, priced: bool) -> String {
    let symbol = if inverse { "BTCUSD_PERP" } else { "BTCUSDT" };
    let mark = if priced {
        if inverse {
            r#", "markPrice":"10000""#
        } else {
            r#", "markPrice":"96000""#
        }
    } else {
        ""
    };
    // Entry is deliberately unrelated: it cannot supply a current wallet price.
    format!(
        r#"[{{"symbol":"{symbol}","positionSide":"BOTH","positionAmt":"1","entryPrice":"5","unRealizedProfit":"0","liquidationPrice":"1","leverage":"20","marginType":"cross"{mark}}}]"#
    )
}
fn expected(inverse: bool) -> Balance {
    if inverse {
        Balance {
            total: 10000.,
            free: 8000.,
            used: 2000.,
            upnl: 100.,
        }
    } else {
        Balance {
            total: 5960.,
            free: 5912.,
            used: 48.,
            upnl: 10.,
        }
    }
}
fn seed(h: &mut Harness, inverse: bool) {
    let symbol = if inverse { "BTCUSD_PERP" } else { "BTCUSDT" };
    let price = if inverse { 10000 } else { 96000 };
    seed_symbol(h, symbol, price);
}
fn seed_symbol(h: &mut Harness, symbol: &str, price: u32) {
    h.respond("ticker/24hr", 200, &format!(r#"[{{"symbol":"{symbol}","lastPrice":"{price}","priceChangePercent":"0","volume":"1","quoteVolume":"1","baseVolume":"1"}}]"#));
    h.respond(
        "ticker/bookTicker",
        200,
        &format!(r#"[{{"symbol":"{symbol}","bidPrice":"{price}","askPrice":"{price}"}}]"#),
    );
    h.respond("premiumIndex", 200, &format!(r#"[{{"symbol":"{symbol}","markPrice":"{price}","indexPrice":"{price}","lastFundingRate":"0","nextFundingTime":0}}]"#));
}
#[test]
fn native_risk_mark_values_early_wallet_in_either_arrival_order_without_public_tickers() {
    for inverse in [false, true] {
        for wallet_first in [false, true] {
            let mut h = pending(inverse);
            let daily = h.take("ticker/24hr");
            h.event(Event::HttpFailed {
                request: daily.id,
                error: "scripted public outage",
            });
            if wallet_first {
                h.respond(
                    if inverse {
                        "/dapi/v1/account"
                    } else {
                        "/fapi/v3/account"
                    },
                    200,
                    wallet(inverse),
                );
                assert!(!h.engine.store().loaded().balance);
            }
            h.respond("positionRisk", 200, &risk(inverse, true));
            if !wallet_first {
                h.respond(
                    if inverse {
                        "/dapi/v1/account"
                    } else {
                        "/fapi/v3/account"
                    },
                    200,
                    wallet(inverse),
                );
            }
            assert!(!h.engine.store().loaded().tickers);
            assert!(h.engine.store().loaded().balance);
            assert_eq!(*h.engine.store().balance(), expected(inverse));
        }
    }
}
#[test]
fn missing_real_price_retains_current_balance_then_seed_revalues_cached_native_wallet() {
    for inverse in [false, true] {
        for previous in [false, true] {
            let mut h = pending(inverse);
            let old = Balance {
                total: 1234.,
                free: 1200.,
                used: 34.,
                upnl: 7.,
            };
            if previous {
                h.engine.store_mut().set_balance(old.clone());
                if inverse {
                    h.engine.store_mut().set_coin_balances(vec![CoinBalance {
                        base: "BTC".into(),
                        amount: 0.5,
                        notional: 1234.,
                    }]);
                }
            }
            h.respond(
                if inverse {
                    "/dapi/v1/account"
                } else {
                    "/fapi/v3/account"
                },
                200,
                wallet(inverse),
            );
            h.respond("positionRisk", 200, &risk(inverse, false));
            assert_eq!(h.engine.store().loaded().balance, previous);
            assert_eq!(
                *h.engine.store().balance(),
                if previous { old } else { Balance::default() }
            );
            assert!(
                h.logs
                    .iter()
                    .any(|log| log.contains("valuation price unavailable"))
            );
            if inverse {
                let coin = &h.engine.store().coin_balances().unwrap()[0];
                assert_eq!((coin.base.as_str(), coin.amount), ("BTC", 1.));
                assert_eq!(coin.notional, if previous { 1234. } else { 0. });
            }
            seed(&mut h, inverse);
            assert_eq!(*h.engine.store().balance(), expected(inverse));
            assert!(h.engine.store().loaded().balance);
        }
    }
}
#[test]
fn a_late_real_live_mark_recovers_pending_valuation_without_another_wallet_request() {
    for inverse in [false, true] {
        let mut h = pending(inverse);
        seed(&mut h, inverse);
        h.engine.store_mut().set_tickers(vec![]);
        h.respond(
            if inverse {
                "/dapi/v1/account"
            } else {
                "/fapi/v3/account"
            },
            200,
            wallet(inverse),
        );
        assert!(!h.engine.store().loaded().balance);
        let socket = h
            .socket(if inverse {
                "dstream.binance.com/ws"
            } else {
                "/market/ws"
            })
            .unwrap();
        h.event(Event::WsOpened { socket });
        let symbol = if inverse { "BTCUSD_PERP" } else { "BTCUSDT" };
        let price = if inverse { 10000 } else { 96000 };
        h.event(Event::WsMessage{socket,text:&format!(r#"[{{"e":"markPriceUpdate","s":"{symbol}","p":"{price}","i":"{price}","r":"0","T":0}}]"#)});
        assert_eq!(*h.engine.store().balance(), expected(inverse));
    }
}

#[test]
fn real_public_seed_supersedes_earlier_native_mark_for_cached_wallet() {
    for inverse in [false, true] {
        let mut h = pending(inverse);
        h.respond(
            "positionRisk",
            200,
            &risk(inverse, true).replace(if inverse { "10000" } else { "96000" }, "1000"),
        );
        h.respond(
            if inverse {
                "/dapi/v1/account"
            } else {
                "/fapi/v3/account"
            },
            200,
            wallet(inverse),
        );
        assert_ne!(*h.engine.store().balance(), expected(inverse));
        seed(&mut h, inverse);
        assert_eq!(*h.engine.store().balance(), expected(inverse));
    }
}

#[test]
fn a_failed_risk_response_cannot_supply_a_wallet_price_from_its_streamed_rows() {
    for inverse in [false, true] {
        let mut h = pending(inverse);
        h.respond(
            if inverse {
                "/dapi/v1/account"
            } else {
                "/fapi/v3/account"
            },
            200,
            wallet(inverse),
        );
        h.respond("positionRisk", 503, &risk(inverse, true));
        assert!(!h.engine.store().loaded().balance);
        assert_eq!(*h.engine.store().balance(), Balance::default());
        seed(&mut h, inverse);
        assert_eq!(*h.engine.store().balance(), expected(inverse));
    }
}

#[test]
fn partial_catalogue_cannot_prove_an_unpriced_wallet_asset_is_unsupported_dust() {
    let mut h = Harness::started(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    });
    let eth = support::EXCHANGE_INFO.replace("BTC", "ETH");
    let partial = eth.replacen("[", r#"[{"symbol":"BTCUSDT","broken":true},"#, 1);
    h.respond("exchangeInfo", 200, &partial);
    h.respond("/fapi/v3/account", 200, LINEAR_WALLET);
    assert!(!h.engine.store().loaded().balance);
    assert_eq!(*h.engine.store().balance(), Balance::default());
    assert!(
        h.logs
            .iter()
            .any(|log| log.contains("valuation price unavailable"))
    );
}
#[test]
fn a_clean_catalogue_keeps_the_native_unsupported_asset_exclusion_without_startup_barrier() {
    let mut h = pending(true);
    h.respond("positionRisk", 200, &risk(true, true));
    let wallet = INVERSE_WALLET.replace("}]",r#"},{"asset":"XYZ","walletBalance":"42","availableBalance":"42","initialMargin":"0","unrealizedProfit":"0"}]"#);
    h.respond("/dapi/v1/account", 200, &wallet);
    assert!(h.engine.store().loaded().balance);
    assert_eq!(*h.engine.store().balance(), expected(true));
    assert!(
        h.engine
            .store()
            .coin_balances()
            .unwrap()
            .iter()
            .any(|coin| coin.base == "XYZ" && coin.amount == 42.)
    );
    assert!(
        h.logs
            .iter()
            .any(|log| log.contains("no supported valuation market"))
    );
}

#[test]
fn partial_catalogue_repair_restores_wallet_without_resetting_sockets_live_quotes_or_market_indices()
 {
    for inverse in [false, true] {
        for previous in [false, true] {
            for recovery in ["native", "rest", "live"] {
                let mut h = Harness::started(Options {
                    variant: if inverse {
                        Variant::CoinM
                    } else {
                        Variant::UsdM
                    },
                    key: Some("key".into()),
                    secret: Some("secret".into()),
                    ..Default::default()
                });
                let btc = if inverse {
                    INVERSE
                } else {
                    support::EXCHANGE_INFO
                };
                let eth = btc.replace("BTC", "ETH");
                let partial = eth.replacen("[", r#"[{"symbol":"BTC_MALFORMED","broken":true},"#, 1);
                h.respond("exchangeInfo", 200, &partial);
                let eth_symbol = if inverse { "ETHUSD_PERP" } else { "ETHUSDT" };
                if previous {
                    h.engine.store_mut().set_balance(Balance {
                        total: 1234.,
                        free: 1200.,
                        used: 34.,
                        upnl: 7.,
                    });
                }
                h.respond(
                    if inverse {
                        "/dapi/v1/account"
                    } else {
                        "/fapi/v3/account"
                    },
                    200,
                    wallet(inverse),
                );
                h.respond("positionRisk", 200, &risk(inverse, false));
                if !inverse {
                    h.respond("symbolConfig", 200, "[]");
                }
                h.engine
                    .store_mut()
                    .update_setting(eth_symbol, |setting| setting.leverage = 77.);
                assert_eq!(h.engine.store().loaded().balance, previous);
                h.advance(1000.);
                let rows = |catalogue: &str| {
                    catalogue
                        .strip_prefix(r#"{"symbols":["#)
                        .unwrap()
                        .strip_suffix("]}")
                        .unwrap()
                        .to_string()
                };
                h.respond(
                    "exchangeInfo",
                    200,
                    &format!(r#"{{"symbols":[{},{}]}}"#, rows(btc), rows(&eth)),
                );
                assert_eq!(h.engine.store().markets()[0].symbol, eth_symbol);
                assert_eq!(h.engine.store().market_index(eth_symbol), Some(0));
                assert_eq!(h.engine.store().setting(eth_symbol).unwrap().leverage, 77.);
                assert_eq!(
                    h.requests
                        .iter()
                        .filter(|request| request.url.contains("listenKey"))
                        .count(),
                    1
                );
                assert!(!h.has_request(if inverse {
                    "/dapi/v1/account"
                } else {
                    "/fapi/v3/account"
                }));
                h.respond("positionRisk", 200, &risk(inverse, recovery == "native"));
                if recovery == "native" {
                    assert_eq!(*h.engine.store().balance(), expected(inverse));
                }
                // The original ETH-only ticker walk finishes before the queued full reseed.
                seed_symbol(&mut h, eth_symbol, 20);
                let sockets = h.sockets.clone();
                for (socket, _) in &sockets {
                    h.event(Event::WsOpened { socket: *socket });
                }
                let market_path = if inverse {
                    "dstream.binance.com/ws"
                } else {
                    "/market/ws"
                };
                h.message(market_path,&format!(r#"[{{"e":"24hrTicker","s":"{eth_symbol}","c":"777","P":"0","v":"1","q":"777"}}]"#));
                let btc_symbol = if inverse { "BTCUSD_PERP" } else { "BTCUSDT" };
                if recovery == "live" {
                    let price = if inverse { 10000 } else { 96000 };
                    h.message(market_path,&format!(r#"[{{"e":"markPriceUpdate","s":"{btc_symbol}","p":"{price}","i":"{price}","r":"0","T":0}}]"#));
                    assert_eq!(*h.engine.store().balance(), expected(inverse));
                }
                // An anchored pre-existing book must not reset when repaired discovery finishes.
                h.event(Event::Command(Command::SubscribeOrderBook {
                    symbol: eth_symbol.into(),
                }));
                let public_path = if inverse {
                    "dstream.binance.com/ws"
                } else {
                    "/public/ws"
                };
                h.message(public_path,&format!(r#"{{"e":"depthUpdate","s":"{eth_symbol}","U":101,"u":101,"pu":100,"b":[["50","2"]],"a":[]}}"#));
                h.respond(
                    "/depth",
                    200,
                    r#"{"lastUpdateId":100,"bids":[["50","1"]],"asks":[["51","1"]]}"#,
                );
                assert_eq!(h.engine.book(eth_symbol).unwrap().best_bid(), Some(50.));
                seed(&mut h, inverse);
                assert_eq!(*h.engine.store().balance(), expected(inverse));
                assert_eq!(h.engine.store().ticker(eth_symbol).unwrap().last, 777.);
                assert_eq!(h.engine.book(eth_symbol).unwrap().best_bid(), Some(50.));
                assert_eq!(h.sockets, sockets, "recovery={recovery}");
                assert_eq!(h.engine.store().market_index(btc_symbol), Some(1));
            }
        }
    }
}
#[test]
fn spot_catalogue_repair_reuses_its_shard_and_adds_only_the_new_ticker_topic() {
    let mut h = Harness::started(Options {
        variant: Variant::Spot,
        public_data_only: true,
        ..Default::default()
    });
    let eth = support::EXCHANGE_INFO.replace("BTC", "ETH");
    h.respond(
        "exchangeInfo",
        200,
        &eth.replacen("[", r#"[{"broken":true},"#, 1),
    );
    h.respond("ticker/24hr",200,r#"[{"symbol":"ETHUSDT","lastPrice":"20","priceChangePercent":"0","volume":"1","quoteVolume":"20"}]"#);
    let sockets = h.sockets.clone();
    for (socket, _) in &sockets {
        h.event(Event::WsOpened { socket: *socket });
    }
    h.advance(1000.);
    let btc_rows = support::EXCHANGE_INFO
        .strip_prefix(r#"{"symbols":["#)
        .unwrap()
        .strip_suffix("]}")
        .unwrap();
    let eth_rows = eth
        .strip_prefix(r#"{"symbols":["#)
        .unwrap()
        .strip_suffix("]}")
        .unwrap();
    h.respond(
        "exchangeInfo",
        200,
        &format!(r#"{{"symbols":[{btc_rows},{eth_rows}]}}"#),
    );
    let shard = sockets[1].0;
    h.event(Event::WsMessage {
        socket: shard,
        text: r#"{"e":"24hrTicker","s":"ETHUSDT","c":"777","P":"0","v":"1","q":"777"}"#,
    });
    h.respond("ticker/24hr", 200, support::DAILY);
    assert_eq!(h.sockets, sockets);
    assert_eq!(h.engine.store().ticker("ETHUSDT").unwrap().last, 777.);
    assert_eq!(
        h.sent
            .iter()
            .map(|(_, text)| text.matches("ethusdt@ticker").count())
            .sum::<usize>(),
        1
    );
    assert_eq!(
        h.sent
            .iter()
            .map(|(_, text)| text.matches("btcusdt@ticker").count())
            .sum::<usize>(),
        1
    );
}

#[test]
fn unsupported_btc_only_conversion_does_not_hold_spot_or_linear_wallets() {
    for spot in [false, true] {
        let mut h = Harness::started(Options {
            variant: if spot { Variant::Spot } else { Variant::UsdM },
            key: Some("key".into()),
            secret: Some("secret".into()),
            ..Default::default()
        });
        let market = support::EXCHANGE_INFO
            .replace("BTCUSDT", "XYZBTC")
            .replace("\"baseAsset\":\"BTC\"", "\"baseAsset\":\"XYZ\"")
            .replace("\"quoteAsset\":\"USDT\"", "\"quoteAsset\":\"BTC\"");
        h.respond("exchangeInfo", 200, &market);
        h.respond(if spot { "/api/v3/account" } else { "/fapi/v3/account" }, 200,
            if spot { r#"{"balances":[{"asset":"USDT","free":"5000","locked":"0"},{"asset":"XYZ","free":"42","locked":"0"}]}"# }
            else { r#"{"assets":[{"asset":"USDT","walletBalance":"5000","availableBalance":"5000","initialMargin":"0","unrealizedProfit":"0"},{"asset":"XYZ","walletBalance":"42","availableBalance":"42","initialMargin":"0","unrealizedProfit":"0"}]}"# });
        assert!(h.engine.store().loaded().balance, "spot={spot}");
        assert_eq!(h.engine.store().balance().total, 5000.);
    }
}

#[test]
fn complete_repair_can_prove_absent_route_without_removing_existing_market_indices() {
    let mut h = Harness::started(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    });
    h.respond(
        "exchangeInfo",
        200,
        &support::EXCHANGE_INFO.replacen("[", r#"[{"broken":true},"#, 1),
    );
    h.respond("/fapi/v3/account", 200, LINEAR_WALLET);
    assert!(!h.engine.store().loaded().balance);
    h.advance(1000.);
    h.respond(
        "exchangeInfo",
        200,
        &support::EXCHANGE_INFO.replace("BTC", "ETH"),
    );
    assert_eq!(h.engine.store().market_index("BTCUSDT"), Some(0));
    assert_eq!(h.engine.store().market_index("ETHUSDT"), Some(1));
    assert!(h.engine.store().loaded().balance);
    assert_eq!(h.engine.store().balance().total, 5000.);
}

#[test]
fn newer_private_wallet_event_fences_pending_cached_and_inflight_wallet_valuations() {
    for inverse in [false, true] {
        for cached in [false, true] {
            let mut h = pending(inverse);
            let account_path = if inverse {
                "/dapi/v1/account"
            } else {
                "/fapi/v3/account"
            };
            if cached {
                h.respond(account_path, 200, wallet(inverse));
            }
            h.respond("listenKey", 200, r#"{"listenKey":"scripted-listen-key"}"#);
            let socket = h.socket("scripted-listen-key").unwrap();
            h.event(Event::WsOpened { socket });
            h.message("scripted-listen-key", r#"{"e":"ACCOUNT_UPDATE","a":{"B":[{"a":"BTC","wb":"2","cw":"2","bc":"1"}],"P":[]}}"#);
            if !cached {
                // This HTTP request started before the newer private event.
                h.respond(account_path, 200, wallet(inverse));
            }
            seed(&mut h, inverse);
            assert!(
                !h.engine.store().loaded().balance,
                "old wallet replayed inverse={inverse}, cached={cached}"
            );
            h.advance(3000.);
            let fresh = if inverse {
                INVERSE_WALLET.replace("\"walletBalance\":\"1\"", "\"walletBalance\":\"2\"")
            } else {
                LINEAR_WALLET.replace("\"walletBalance\":\"0.01\"", "\"walletBalance\":\"0.02\"")
            };
            h.respond(account_path, 200, &fresh);
            assert!(h.engine.store().loaded().balance);
            assert_eq!(
                h.engine.store().balance().total,
                if inverse { 20000. } else { 6920. }
            );
        }
    }
}

#[test]
fn priced_wallet_read_in_flight_during_private_event_applies_without_extra_refetch() {
    for inverse in [false, true] {
        let mut h = pending(inverse);
        let account_path = if inverse {
            "/dapi/v1/account"
        } else {
            "/fapi/v3/account"
        };
        seed(&mut h, inverse);
        let read = h.take(account_path);
        h.respond("listenKey", 200, r#"{"listenKey":"scripted-listen-key"}"#);
        let socket = h.socket("scripted-listen-key").unwrap();
        h.event(Event::WsOpened { socket });
        h.message(
            "scripted-listen-key",
            r#"{"e":"ACCOUNT_UPDATE","a":{"B":[{"a":"BTC","wb":"2","cw":"2","bc":"1"}],"P":[]}}"#,
        );
        h.reply(read.id, 200, "", wallet(inverse));
        assert_eq!(*h.engine.store().balance(), expected(inverse));
        h.advance(3000.);
        assert!(!h.has_request(account_path), "inverse={inverse}");
    }
}

fn dust_wallet(spot: bool, status: &str) -> Harness {
    let mut h = Harness::started(Options {
        variant: if spot { Variant::Spot } else { Variant::UsdM },
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    });
    let dust = support::EXCHANGE_INFO
        .replace("BTCUSDT", "DUSTUSDT")
        .replace("\"baseAsset\":\"BTC\"", "\"baseAsset\":\"DUST\"")
        .replace("TRADING", status);
    let rows = |info: &str| {
        info.strip_prefix(r#"{"symbols":["#)
            .unwrap()
            .strip_suffix("]}")
            .unwrap()
            .to_string()
    };
    h.respond(
        "exchangeInfo",
        200,
        &format!(
            r#"{{"symbols":[{},{}]}}"#,
            rows(support::EXCHANGE_INFO),
            rows(&dust)
        ),
    );
    h
}
fn respond_dust_wallet(h: &mut Harness, spot: bool) {
    if spot {
        h.respond("/api/v3/account", 200, r#"{"balances":[{"asset":"USDT","free":"100","locked":"0"},{"asset":"DUST","free":"5","locked":"0"}]}"#);
    } else {
        h.respond("/fapi/v3/account", 200, r#"{"assets":[{"asset":"USDT","walletBalance":"100","availableBalance":"100","initialMargin":"0","unrealizedProfit":"0"},{"asset":"DUST","walletBalance":"5","availableBalance":"5","initialMargin":"0","unrealizedProfit":"0"}]}"#);
    }
}
fn seed_dust(h: &mut Harness, spot: bool, dust_row: bool) {
    let dust = if dust_row {
        r#",{"symbol":"DUSTUSDT","lastPrice":"0","priceChangePercent":"0","volume":"0","quoteVolume":"0"}"#
    } else {
        ""
    };
    h.respond("ticker/24hr", 200, &format!(r#"[{{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"1"}}{dust}]"#));
    if !spot {
        h.respond("ticker/bookTicker", 200, support::BOOK_TICKERS);
        h.respond("premiumIndex", 200, support::PREMIUM);
    }
}

#[test]
fn delisted_only_dust_commits_at_zero_once_the_ticker_seed_loads() {
    for spot in [true, false] {
        for dust_row in [false, true] {
            for wallet_first in [true, false] {
                let case = format!("spot={spot} dust_row={dust_row} wallet_first={wallet_first}");
                let status = if spot { "BREAK" } else { "SETTLING" };
                let mut h = dust_wallet(spot, status);
                if wallet_first {
                    respond_dust_wallet(&mut h, spot);
                    if !spot {
                        h.respond("positionRisk", 200, "[]");
                    }
                    assert!(!h.engine.store().loaded().balance, "{case}");
                    seed_dust(&mut h, spot, dust_row);
                } else {
                    seed_dust(&mut h, spot, dust_row);
                    respond_dust_wallet(&mut h, spot);
                    if !spot {
                        h.respond("positionRisk", 200, "[]");
                    }
                }
                assert!(h.engine.store().loaded().balance, "{case}");
                assert_eq!(h.engine.store().balance().total, 100., "{case}");
                if spot {
                    let dust = h
                        .engine
                        .store()
                        .currency_balances()
                        .unwrap()
                        .iter()
                        .find(|currency| currency.currency == "DUST")
                        .unwrap()
                        .balance
                        .clone();
                    assert_eq!(dust.total, 5., "{case}");
                    let coin = h
                        .engine
                        .store()
                        .coin_balances()
                        .unwrap()
                        .iter()
                        .find(|coin| coin.base == "DUST")
                        .unwrap()
                        .clone();
                    assert_eq!((coin.amount, coin.notional), (5., 0.), "{case}");
                }
                respond_dust_wallet_again(&mut h, spot);
                assert_eq!(
                    h.logs
                        .iter()
                        .filter(|log| log.contains("DUST has no supported valuation market"))
                        .count(),
                    1,
                    "{case}"
                );
            }
        }
    }
}
fn respond_dust_wallet_again(h: &mut Harness, spot: bool) {
    h.advance(30_000.);
    if h.has_request(if spot {
        "/api/v3/account"
    } else {
        "/fapi/v3/account"
    }) {
        respond_dust_wallet(h, spot);
    }
}

#[test]
fn tradable_pair_with_a_late_quote_holds_previous_usd_then_values_on_the_quote() {
    for previous in [false, true] {
        let mut h = dust_wallet(true, "TRADING");
        let old = Balance {
            total: 1234.,
            free: 1234.,
            used: 0.,
            upnl: 0.,
        };
        if previous {
            h.engine.store_mut().set_balance(old.clone());
        }
        seed_dust(&mut h, true, false);
        respond_dust_wallet(&mut h, true);
        // Live streams open at once; supervised sockets drop unopened connections.
        for socket in h
            .sockets
            .iter()
            .map(|(socket, _)| *socket)
            .collect::<Vec<_>>()
        {
            h.event(Event::WsOpened { socket });
        }
        let public = h
            .sockets
            .iter()
            .find(|(_, url)| url.contains("9443/ws"))
            .map(|(socket, _)| *socket)
            .unwrap();
        assert_eq!(h.engine.store().loaded().balance, previous);
        assert_eq!(
            *h.engine.store().balance(),
            if previous {
                old.clone()
            } else {
                Balance::default()
            }
        );
        h.advance(10_000.);
        assert_eq!(h.engine.store().loaded().balance, previous);
        h.event(Event::WsMessage {
            socket: public,
            text: r#"{"e":"24hrTicker","s":"DUSTUSDT","c":"2","P":"0","v":"1","q":"2"}"#,
        });
        assert!(h.engine.store().loaded().balance);
        assert_eq!(h.engine.store().balance().total, 110.);
    }
}

#[test]
fn tradable_pair_that_never_quotes_commits_at_zero_after_a_bounded_wait() {
    let mut h = dust_wallet(true, "TRADING");
    seed_dust(&mut h, true, true);
    respond_dust_wallet(&mut h, true);
    assert!(!h.engine.store().loaded().balance);
    h.advance(60_000.);
    assert!(h.engine.store().loaded().balance);
    assert_eq!(h.engine.store().balance().total, 100.);
    assert!(
        h.logs
            .iter()
            .any(|log| log.contains("DUST has no supported valuation market"))
    );
}
