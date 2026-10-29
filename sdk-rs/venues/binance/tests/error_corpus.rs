//! Recorded Binance error replies (tests/fixtures/errors), each driven through
//! the production path that consumes it: the background account read, the
//! klines command, an order placement, or the order-book snapshot.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts};
use safe_cex_engine::test_support::error_corpus::{Fixture, Pinned, load};
use safe_cex_engine::{Command, ErrorKind, Event, Outcome};
use safe_cex_venue_binance::Variant;
use std::collections::BTreeMap;
use support::{Harness, Options};

const POSITION_RISK: &str = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0.010","entryPrice":"95000.0","breakEvenPrice":"95010.0","markPrice":"96000.0","unRealizedProfit":"10.00","liquidationPrice":"80000.5","isolatedMargin":"0","notional":"960.0","marginAsset":"USDT","isolatedWallet":"0","initialMargin":"48","maintMargin":"3.8","positionInitialMargin":"48","openOrderInitialMargin":"0","adl":1,"bidNotional":"0","askNotional":"0","updateTime":1720736417660}]"#;
const SYMBOL_CONFIG: &str = r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","isAutoAddMargin":false,"leverage":20,"maxNotionalValue":"$1000000"}]"#;
const OPEN_ORDERS: &str = r#"[{"avgPrice":"0.00000","clientOrderId":"abc","cumQuote":"0","executedQty":"0.001","orderId":8389765519993909000,"origQty":"0.004","origType":"LIMIT","price":"90000","reduceOnly":false,"side":"BUY","positionSide":"BOTH","status":"PARTIALLY_FILLED","stopPrice":"0","closePosition":false,"symbol":"BTCUSDT","time":1579276756075,"timeInForce":"GTC","type":"LIMIT","updateTime":1579276756075,"workingType":"CONTRACT_PRICE","priceProtect":false}]"#;
const OPEN_ALGO_ORDERS: &str = r#"[{"algoId":2146760,"clientAlgoId":"x","algoType":"CONDITIONAL","orderType":"STOP_MARKET","symbol":"BTCUSDT","side":"SELL","positionSide":"BOTH","timeInForce":"GTC","quantity":"0","algoStatus":"NEW","triggerPrice":"85000","price":"0","workingType":"MARK_PRICE","reduceOnly":false,"closePosition":true,"createTime":1750485492076}]"#;
const SPOT_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","baseAssetPrecision":8,"quoteAssetPrecision":8,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.01"},{"filterType":"LOT_SIZE","stepSize":"0.00001","minQty":"0.00001","maxQty":"9000"}]}]}"#;

fn corpus() -> BTreeMap<String, Fixture> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/errors");
    load(&dir).into_iter().collect()
}

fn keyed(variant: Variant, portfolio_margin: bool) -> Options {
    Options {
        variant,
        portfolio_margin,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }
}

/// A USD-M account with markets, account and user stream loaded.
fn ready_usdm() -> Harness {
    let mut h = Harness::started(keyed(Variant::UsdM, false));
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond("/fapi/v1/leverageBracket", 200, r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125,"notionalCap":50000}]}]"#);
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"key"}"#);
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/fapi/v3/positionRisk", 200, POSITION_RISK);
    h.respond("/fapi/v1/symbolConfig", 200, SYMBOL_CONFIG);
    h.respond("/fapi/v3/account", 200, r#"{"totalInitialMargin":"0","totalWalletBalance":"1000","availableBalance":"1000","assets":[],"positions":[]}"#);
    h.respond("/fapi/v1/openOrders", 200, OPEN_ORDERS);
    h.respond("/fapi/v1/openAlgoOrders", 200, OPEN_ALGO_ORDERS);
    for path in ["/public/ws", "/market/ws", "/private/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    h
}

/// The fixture's status and body, as the venue answered them.
fn reply(h: &mut Harness, path: &str, fixture: &Fixture) {
    let request = h.take(path);
    h.reply(request.id, fixture.http_status.unwrap(), "", &fixture.body);
}

/// The background read's notice carries the kind. A read refused for the key
/// is a visible `auth.rejected` Error; any other refusal stays `request.failed`.
fn read_notice(h: &Harness, prefix: &str, fixture: &Fixture) {
    let native: serde_json::Value = serde_json::from_str(&fixture.body).unwrap();
    let event = h
        .events
        .iter()
        .find(|event| event.message.starts_with(prefix))
        .unwrap_or_else(|| panic!("no {prefix} notice: {:?}", h.events));
    let auth = matches!(
        fixture.expect.kind,
        Some(ErrorKind::AuthInvalid | ErrorKind::AuthExpired | ErrorKind::AuthPermission)
    );
    assert_eq!(
        event.code,
        if auth {
            code::AUTH_REJECTED
        } else {
            code::REQUEST_FAILED
        },
        "{prefix}"
    );
    assert_eq!(event.level, safe_cex_engine::Level::Error, "{prefix}");
    let kind = fixture.expect.kind.unwrap().as_str();
    assert!(
        event.fields.contains(&("kind", kind.into())),
        "{:?}",
        event.fields
    );
    let venue_code = native["code"].as_i64().unwrap().to_string();
    assert!(
        event
            .fields
            .contains(&("venueCode", venue_code.as_str().into())),
        "{:?}",
        event.fields
    );
    assert_eq!(
        event.message,
        format!("{prefix}{}", native["msg"].as_str().unwrap())
    );
}

#[test]
fn the_corpus_covers_every_recorded_case_with_its_production_path() {
    let names: Vec<_> = corpus().into_keys().collect();
    assert_eq!(
        names,
        [
            "coinm-depth-invalid-symbol",
            "pm-account-bogus-key",
            "spot-account-bogus-key",
            "spot-depth-invalid-symbol",
            "usdm-account-bogus-key",
            "usdm-depth-illegal-limit",
            "usdm-depth-invalid-symbol",
            "usdm-klines-invalid-interval",
            "usdm-order-bogus-key",
        ]
    );
    for (name, fixture) in corpus() {
        let expected = if name.contains("order") {
            Pinned::Write
        } else {
            Pinned::Read
        };
        assert_eq!(fixture.expect.path, expected, "{name}");
    }
}

#[test]
fn a_rejected_key_on_the_usdm_account_read_is_classified_auth_invalid() {
    let fixture = &corpus()["usdm-account-bogus-key"];
    let mut h = Harness::started(keyed(Variant::UsdM, false));
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    // Captured on /fapi/v2/account; production reads balances from /fapi/v3/account.
    reply(&mut h, "/fapi/v3/account", fixture);
    read_notice(&h, "Binance account: ", fixture);
}

#[test]
fn a_rejected_key_on_the_portfolio_margin_balance_read_is_classified_auth_invalid() {
    let fixture = &corpus()["pm-account-bogus-key"];
    let mut h = Harness::started(keyed(Variant::UsdM, true));
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    // Captured on /papi/v1/account; production reads balances from /papi/v1/balance.
    reply(&mut h, "/papi/v1/balance", fixture);
    read_notice(&h, "Binance account: ", fixture);
}

/// Spot checks the request timestamp before the key, so a stale stamp answers
/// -1021 even with a bogus key; it is an unlisted, definitive refusal.
#[test]
fn a_spot_clock_refusal_on_the_account_read_is_a_plain_rejection() {
    let fixture = &corpus()["spot-account-bogus-key"];
    let mut h = Harness::started(keyed(Variant::Spot, false));
    h.respond("/api/v3/exchangeInfo", 200, SPOT_INFO);
    h.respond("/api/v3/ticker/24hr", 200, r#"[{"symbol":"BTCUSDT","lastPrice":"100","bidPrice":"99","askPrice":"101","priceChangePercent":"2","volume":"10","quoteVolume":"1000"}]"#);
    let private = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsOpened { socket: private });
    reply(&mut h, "/api/v3/account", fixture);
    assert_eq!(fixture.expect.kind, Some(ErrorKind::Rejected));
    read_notice(&h, "Binance account: ", fixture);
}

#[test]
fn an_invalid_kline_interval_fails_the_candles_command_as_invalid_input() {
    let fixture = &corpus()["usdm-klines-invalid-interval"];
    let mut h = Harness::started(Options {
        variant: Variant::UsdM,
        public_data_only: true,
        ..Options::default()
    });
    h.event(Event::Command(Command::FetchCandles {
        command: 5,
        symbol: "BTCUSDT".into(),
        interval: safe_cex_engine::model::Interval::parse("1m").unwrap(),
        limit: Some(10),
        from_ms: None,
        to_ms: None,
    }));
    reply(&mut h, "/klines", fixture);
    let [(5, error)] = h.failures.as_slice() else {
        panic!("{:?}", h.failures);
    };
    assert_eq!(
        (error.kind, error.outcome, error.venue_code.as_deref()),
        (fixture.expect.kind, fixture.expect.outcome, Some("-1120"))
    );
    assert_eq!(
        error.message,
        format!("Binance klines HTTP 400: {}", fixture.body)
    );
}

#[test]
fn a_rejected_key_on_an_order_is_a_rejected_auth_failure() {
    let fixture = &corpus()["usdm-order-bogus-key"];
    let mut h = ready_usdm();
    h.event(Event::Command(Command::PlaceOrders {
        command: 3,
        orders: vec![PlaceOrderOpts {
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            amount: 0.001,
            price: Some(1000.0),
            trigger_price: None,
            stop_loss: None,
            take_profit: None,
            reduce_only: None,
            time_in_force: None,
            trigger_type: None,
            tp_trigger_type: None,
            sl_trigger_type: None,
            client_order_id: None,
            max_slippage_pct: None,
            bybit_bbo: None,
            bybit_slippage_tolerance_ticks: None,
            implied_volatility: None,
            close: None,
            stp_mode: None,
        }],
    }));
    reply(&mut h, "/fapi/v1/order", fixture);
    let [(3, error)] = h.failures.as_slice() else {
        panic!("{:?}", h.failures);
    };
    assert_eq!(
        (error.kind, error.outcome, error.venue_code.as_deref()),
        (
            Some(ErrorKind::AuthInvalid),
            Some(Outcome::Rejected),
            Some("-2014")
        )
    );
    assert_eq!(fixture.expect.outcome, Some(Outcome::Rejected));
}

/// A refused depth snapshot surfaces nothing: the book stays empty and the next
/// update requests a fresh snapshot, as before. Every product answers with the
/// same `{code,msg}` shape, so each recorded body replays on the USD-M book.
#[test]
fn refused_depth_snapshots_retry_without_a_notice() {
    for name in [
        "usdm-depth-invalid-symbol",
        "usdm-depth-illegal-limit",
        "coinm-depth-invalid-symbol",
        "spot-depth-invalid-symbol",
    ] {
        let fixture = &corpus()[name];
        assert_eq!(
            fixture.expect.kind, None,
            "{name}: the path surfaces no kind"
        );
        let mut h = Harness::new();
        h.diff(90, 95, 89, "98.0");
        reply(&mut h, "/fapi/v1/depth", fixture);
        assert_eq!(h.best_bid(), None, "{name}");
        assert!(h.failures.is_empty(), "{name}");
        assert!(
            !h.events.iter().any(|e| e.code == code::REQUEST_FAILED),
            "{name}: {:?}",
            h.events
        );
        // The first backoff is about 1 s; stay inside the socket's 20 s idle window.
        h.advance(3_000.0);
        h.diff(96, 97, 95, "98.0");
        assert_eq!(h.snapshot_requests(), 1, "{name}: the snapshot is retried");
    }
}
