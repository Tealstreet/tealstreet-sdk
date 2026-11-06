//! `readPositions`: success only after a read issued after the command commits.
use support::Options;
mod support;
use safe_cex_engine::{Command, ErrorKind, Event, Outcome, VenueAction};
use safe_cex_venue_binance::Variant;
use support::Harness;
const FLAT_ANSWER: &str = r#"{"skipped":[],"open":[]}"#;

const USDM_RISK: &str = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0.010","entryPrice":"95000.0","markPrice":"96000.0","unRealizedProfit":"10.00","liquidationPrice":"80000.5","notional":"960.0","updateTime":1720736417660}]"#;
const COIN_RISK: &str = r#"[{"symbol":"BTCUSD_PERP","positionSide":"BOTH","positionAmt":"5","entryPrice":"9500.0","markPrice":"10000.0","unRealizedProfit":"0.01","leverage":"20","liquidationPrice":"8000","marginType":"cross","isolatedMargin":"0","notionalValue":"0.05","updateTime":1720736417660}]"#;
const CONFIG: &str = r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","isAutoAddMargin":false,"leverage":20,"maxNotionalValue":"$1000000"}]"#;
const USDM_ACCOUNT: &str = r#"{"totalInitialMargin":"48","totalWalletBalance":"1000","availableBalance":"952","assets":[{"asset":"USDT","walletBalance":"1000.0","unrealizedProfit":"10.0","marginBalance":"1010.0","availableBalance":"952.0"}]}"#;
const PM_BALANCE: &str = r#"[{"asset":"USDT","totalWalletBalance":"1000.0","crossMarginFree":"900.0","umWalletBalance":"1000.0","umUnrealizedPNL":"10.0","cmWalletBalance":"0","cmUnrealizedPNL":"0"}]"#;

const USDM: &str = "/fapi/v3/positionRisk";
const PM: &str = "/papi/v1/um/positionRisk";
const COIN: &str = "/dapi/v1/positionRisk";

fn keyed() -> Options {
    Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }
}

fn usdm() -> Harness {
    let mut h = Harness::started(keyed());
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond("/fapi/v1/leverageBracket", 200, r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125,"notionalCap":50000}]}]"#);
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond("/fapi/v1/listenKey", 200, r#"{"listenKey":"usdm-key"}"#);
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond(USDM, 200, USDM_RISK);
    h.respond("/fapi/v1/symbolConfig", 200, CONFIG);
    h.respond("/fapi/v3/account", 200, USDM_ACCOUNT);
    h.respond("/fapi/v1/openOrders", 200, "[]");
    h.respond("/fapi/v1/openAlgoOrders", 200, "[]");
    for path in ["/public/ws", "/market/ws", "/private/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    h.answer_open_position_read();
    h
}

fn pm() -> Harness {
    let mut h = Harness::started(Options {
        portfolio_margin: true,
        ..keyed()
    });
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond(
        "/papi/v1/um/leverageBracket",
        200,
        r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125}]}]"#,
    );
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond("/papi/v1/listenKey", 200, r#"{"listenKey":"pmkey"}"#);
    h.respond(
        "/papi/v1/um/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond(PM, 200, USDM_RISK);
    h.respond("/papi/v1/balance", 200, PM_BALANCE);
    h.respond("/papi/v1/um/openOrders", 200, "[]");
    h.respond("/papi/v1/um/algo/openAlgoOrders", 200, "[]");
    for path in ["/public/ws", "/market/ws", "/pm/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    h.answer_open_position_read();
    h
}

fn coinm() -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::CoinM,
        ..keyed()
    });
    h.respond("/dapi/v1/exchangeInfo",200,r#"{"symbols":[{"symbol":"BTCUSD_PERP","contractStatus":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"1000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"120"}]}]}"#);
    h.respond(
        "/dapi/v2/leverageBracket",
        200,
        r#"[{"symbol":"BTCUSD_PERP","brackets":[{"initialLeverage":125}]}]"#,
    );
    h.respond("/dapi/v1/ticker/24hr",200,r#"[{"symbol":"BTCUSD_PERP","lastPrice":"10000","priceChangePercent":"1","volume":"100","baseVolume":"1"}]"#);
    h.respond(
        "/dapi/v1/ticker/bookTicker",
        200,
        r#"[{"symbol":"BTCUSD_PERP","bidPrice":"9999","askPrice":"10001"}]"#,
    );
    h.respond("/dapi/v1/premiumIndex",200,r#"[{"symbol":"BTCUSD_PERP","markPrice":"10000","indexPrice":"10000","lastFundingRate":"0.0001","nextFundingTime":100000}]"#);
    h.respond("/dapi/v1/listenKey", 200, r#"{"listenKey":"coin-key"}"#);
    h.respond(
        "/dapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond(COIN, 200, COIN_RISK);
    h.respond("/dapi/v1/account",200,r#"{"assets":[{"asset":"BTC","walletBalance":"1","availableBalance":"0.8","initialMargin":"0.1","unrealizedProfit":"0.01"}]}"#);
    h.respond("/dapi/v1/openOrders", 200, "[]");
    let private = h.socket("/ws/coin-key").unwrap();
    h.event(Event::WsOpened { socket: private });
    h.answer_open_position_read();
    h
}

/// Answer every half of the positions read that is out.
fn answer(h: &mut Harness, risk_path: &str, risk: &str) {
    while h.has_request(risk_path) {
        h.respond(risk_path, 200, risk);
    }
    while h.has_request("/fapi/v1/symbolConfig") {
        h.respond("/fapi/v1/symbolConfig", 200, CONFIG);
    }
}

type Case = (&'static str, fn() -> Harness, &'static str);
const CASES: [Case; 3] = [("usdm", usdm, USDM), ("pm", pm, PM), ("coinm", coinm, COIN)];

fn read(h: &mut Harness, command: u32) {
    h.event(Event::Command(Command::Venue {
        command,
        action: VenueAction::ReadPositions,
    }));
}

fn open(h: &Harness) -> f64 {
    h.engine
        .store()
        .positions()
        .iter()
        .map(|p| p.contracts)
        .sum()
}

fn failure(h: &Harness, id: u32) -> &safe_cex_engine::VenueError {
    &h.failures
        .iter()
        .find(|(command, _)| *command == id)
        .expect("a failure")
        .1
}

#[test]
fn a_read_issued_after_the_command_commits_then_it_succeeds() {
    for (name, start, path) in CASES {
        let mut h = start();
        assert!(open(&h) > 0.0, "{name}");
        read(&mut h, 40);
        assert!(h.has_request(path), "{name}: a read is issued");
        assert_eq!(h.result(40), None, "{name}");
        answer(&mut h, path, "[]");
        assert_eq!(h.result(40), Some(&Ok(FLAT_ANSWER.to_owned())), "{name}");
        assert_eq!(open(&h), 0.0, "{name}: the store is flat");
    }
}

#[test]
fn a_read_in_flight_is_followed_by_a_new_one() {
    for (name, start, path) in CASES {
        let mut h = start();
        let before = open(&h);
        read(&mut h, 39);
        assert!(h.has_request(path), "{name}: a read is out");
        let stale = h.take(path);
        let stale_config = h
            .has_request("/fapi/v1/symbolConfig")
            .then(|| h.take("/fapi/v1/symbolConfig"));
        read(&mut h, 41);
        assert!(!h.has_request(path), "{name}: no second read yet");
        let risk = if path == COIN { COIN_RISK } else { USDM_RISK };
        h.reply(stale.id, 200, "", risk);
        if let Some(config) = stale_config {
            h.reply(config.id, 200, "", CONFIG);
        }
        assert!(
            matches!(h.result(39), Some(Ok(body)) if body.starts_with(r#"{"skipped":[],"open":[{"symbol":"#)),
            "{name}: its own read answers it: {:?}",
            h.result(39)
        );
        assert_eq!(h.result(41), None, "{name}: the older read cannot answer");
        assert_eq!(open(&h), before);
        assert!(h.has_request(path), "{name}: a new read follows");
        answer(&mut h, path, "[]");
        assert_eq!(h.result(41), Some(&Ok(FLAT_ANSWER.to_owned())), "{name}");
        assert_eq!(open(&h), 0.0);
    }
}

#[test]
fn a_failed_read_fails_the_command_with_the_venues_error() {
    for (name, start, path) in CASES {
        let mut h = start();
        let before = open(&h);
        read(&mut h, 42);
        h.respond(
            path,
            401,
            r#"{"code":-2015,"msg":"Invalid API-key, IP, or permissions for action"}"#,
        );
        let error = failure(&h, 42);
        assert!(
            error.message.contains("Invalid API-key"),
            "{name}: {error:?}"
        );
        assert_eq!(error.kind, Some(ErrorKind::AuthInvalid), "{name}");
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{name}");
        assert_eq!(open(&h), before, "{name}");
    }
}

#[test]
fn a_malformed_reply_or_an_unmappable_row_fails_the_command() {
    for (name, start, path) in CASES {
        let mut h = start();
        read(&mut h, 43);
        h.respond(path, 200, r#"{"unexpected":"envelope"}"#);
        assert_eq!(failure(&h, 43).outcome, Some(Outcome::NotSent), "{name}");

        let mut h = start();
        let symbol = if path == COIN {
            "BTCUSD_PERP"
        } else {
            "BTCUSDT"
        };
        let row = format!(r#"[{{"symbol":"{symbol}","positionSide":"BOTH"}}]"#);
        read(&mut h, 44);
        answer(&mut h, path, &row);
        assert!(
            failure(&h, 44).message.contains("could not be mapped"),
            "{name}: {:?}",
            failure(&h, 44)
        );
        assert_eq!(failure(&h, 44).outcome, Some(Outcome::NotSent));
    }
}

#[test]
fn a_read_a_newer_stream_event_discarded_is_not_a_commit() {
    let mut h = usdm();
    read(&mut h, 45);
    h.message("/private/ws", r#"{"e":"ACCOUNT_UPDATE","E":1564745798939,"T":1564745798938,"a":{"m":"ORDER","B":[],"P":[{"s":"BTCUSDT","pa":"0.020","ep":"95500.0","cr":"0","up":"20.0","mt":"cross","iw":"0","ps":"BOTH"}]}}"#);
    answer(&mut h, USDM, "[]");
    assert_eq!(h.result(45), None, "the discarded read answers nothing");
    assert_eq!(open(&h), 0.02, "the event stands");
    h.advance(500.0);
    assert!(
        h.has_request(USDM),
        "a new read follows well inside the repair throttle"
    );
    answer(&mut h, USDM, "[]");
    assert_eq!(h.result(45), Some(&Ok(FLAT_ANSWER.to_owned())));
    assert_eq!(open(&h), 0.0);
}

#[test]
fn near_the_weight_limit_the_command_refuses_without_sending() {
    let mut h = usdm();
    h.advance(27_000.0);
    let account = h.take("/fapi/v3/account");
    h.reply(account.id, 200, "x-mbx-used-weight-1m: 2000", USDM_ACCOUNT);
    h.requests.clear();
    read(&mut h, 46);
    let error = failure(&h, 46);
    assert_eq!(error.kind, Some(ErrorKind::RateLimited));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(!h.has_request(USDM));
}

#[test]
fn spot_options_and_a_public_engine_send_nothing() {
    for variant in [Variant::Spot, Variant::Options] {
        let mut h = Harness::started(Options { variant, ..keyed() });
        h.requests.clear();
        read(&mut h, 60);
        let error = failure(&h, 60);
        assert_eq!(error.kind, Some(ErrorKind::Unsupported), "{variant:?}");
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert!(h.requests.is_empty());
    }
    let mut h = Harness::new();
    read(&mut h, 61);
    let error = failure(&h, 61);
    assert_ne!(
        error.kind,
        Some(ErrorKind::Unsupported),
        "advertised, so a precondition"
    );
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

#[test]
fn an_owned_row_outside_the_coinm_catalogue_is_named_and_a_flat_one_is_not() {
    let unlisted = COIN_RISK.replace("BTCUSD_PERP", "ETHUSD_250926");
    let mut h = coinm();
    read(&mut h, 70);
    answer(&mut h, COIN, &unlisted);
    assert_eq!(
        h.result(70),
        Some(&Ok(r#"{"skipped":["ETHUSD_250926"],"open":[]}"#.to_owned()))
    );

    let flat = unlisted.replace("\"positionAmt\":\"5\"", "\"positionAmt\":\"0\"");
    let mut h = coinm();
    read(&mut h, 71);
    answer(&mut h, COIN, &flat);
    assert_eq!(h.result(71), Some(&Ok(FLAT_ANSWER.to_owned())));
}

#[test]
fn usdm_and_pm_commit_a_row_on_an_unknown_market_rather_than_dropping_it() {
    for (name, start, path) in [("usdm", usdm as fn() -> Harness, USDM), ("pm", pm, PM)] {
        let mut h = start();
        read(&mut h, 72);
        answer(&mut h, path, &USDM_RISK.replace("BTCUSDT", "DOGEUSDT"));
        assert_eq!(
            h.result(72),
            Some(&Ok(
                r#"{"skipped":[],"open":[{"symbol":"DOGEUSDT","side":"long","contracts":0.01}]}"#
                    .to_owned()
            )),
            "{name}"
        );
        assert!(open(&h) > 0.0, "{name}: the row is shown, not skipped");
    }
}
