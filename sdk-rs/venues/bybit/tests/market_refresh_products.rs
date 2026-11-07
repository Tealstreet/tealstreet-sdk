//! Refresh completes every enabled native category before publishing any change.
mod support;
use safe_cex_engine::{Command, Event, VenueAction};
use support::Harness;
use support::Options;
const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;
const OPTION: &str = "BTC-25DEC26-100000-C-USDT";
const OPTIONS: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTC-25DEC26-100000-C-USDT","status":"Trading","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","optionsType":"Call","deliveryTime":"1798185600000","lotSizeFilter":{"qtyStep":"0.01","minOrderQty":"0.01","maxOrderQty":"100"},"priceFilter":{"tickSize":"5","minPrice":"5"}},{"symbol":"ETH-25DEC26-3000-P-USDT","status":"Trading","baseCoin":"ETH","quoteCoin":"USDT","settleCoin":"USDT","optionsType":"Put","deliveryTime":"1798185600000","lotSizeFilter":{"qtyStep":"0.1","minOrderQty":"0.1","maxOrderQty":"100"},"priceFilter":{"tickSize":"0.5","minPrice":"0.5"}}],"nextPageCursor":""}}"#;
const QUOTE: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTC-25DEC26-100000-C-USDT","bid1Price":"1000","ask1Price":"1010","markPrice":"1005","lastPrice":"1000","markIv":"0.5","bid1Iv":"0.49","ask1Iv":"0.51","underlyingPrice":"99000","delta":"0.4","gamma":"0.01","vega":"12","theta":"-8"}]}}"#;

fn inverse_instruments() -> String {
    support::INSTRUMENTS
        .replace(r#""category":"linear""#, r#""category":"inverse""#)
        .replace("BTCUSDT", "BTCUSD")
        .replace("ETHUSDT", "ETHUSD")
        .replace("LinearPerpetual", "InversePerpetual")
        .replace("\"USDT\"", "\"USD\"")
        .replace("1190.000", "10000")
        .replace("0.001", "1")
}

fn options(private: bool) -> Options {
    Options {
        enable_inverse_markets: true,
        enable_option_markets: true,
        public_data_only: !private,
        key: private.then(|| "key".into()),
        secret: private.then(|| "secret".into()),
        ..Options::default()
    }
}

fn respond(h: &mut Harness, path: &str, category: &str, extra: Option<&str>, body: &str) {
    let index = h
        .requests
        .iter()
        .position(|request| {
            request.url.contains(path)
                && request.url.contains(&format!("category={category}"))
                && extra.is_none_or(|extra| request.url.contains(extra))
        })
        .unwrap_or_else(|| panic!("missing {path} {category} {extra:?}: {:?}", h.paths()));
    let request = h.requests.remove(index);
    h.reply(request.id, 200, body);
}

fn ready(private: bool) -> Harness {
    let mut h = Harness::started(options(private));
    if private {
        h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    }
    for category in ["linear", "inverse"] {
        respond(
            &mut h,
            "instruments-info",
            category,
            Some("status=PreLaunch"),
            EMPTY,
        );
        if private {
            respond(&mut h, "fee-rate", category, None, EMPTY);
        }
    }
    respond(&mut h, "instruments-info", "option", None, OPTIONS);
    respond(
        &mut h,
        "instruments-info",
        "inverse",
        None,
        &inverse_instruments(),
    );
    assert!(!h.engine.store().loaded().markets);
    let linear = support::INSTRUMENTS.replace(
        "\"symbolType\":\"\"",
        "\"symbolType\":\"xstock\",\"marketRegion\":\"US\",\"underlyingTicker\":\"BTC\"",
    );
    respond(&mut h, "instruments-info", "linear", None, &linear);
    assert!(h.engine.store().loaded().markets);
    assert_eq!(h.engine.store().markets().len(), 6);
    respond(&mut h, "tickers", "option", Some("baseCoin=ETH"), EMPTY);
    respond(
        &mut h,
        "tickers",
        "inverse",
        None,
        &support::TICKERS
            .replace("BTCUSDT", "BTCUSD")
            .replace("ETHUSDT", "ETHUSD"),
    );
    respond(&mut h, "tickers", "linear", None, support::TICKERS);
    assert!(!h.engine.store().loaded().tickers);
    respond(&mut h, "tickers", "option", Some("baseCoin=BTC"), QUOTE);
    assert!(h.engine.store().loaded().tickers);
    for category in ["linear", "inverse", "option"] {
        let socket = h.socket(&format!("public/{category}")).unwrap();
        h.event(Event::WsOpened { socket });
    }
    if private {
        assert_eq!(
            h.sockets
                .iter()
                .filter(|(_, url)| url.ends_with("/private"))
                .count(),
            1
        );
        let socket = h.socket("/private").unwrap();
        h.event(Event::WsOpened { socket });
        h.message("/private", r#"{"op":"auth","success":true}"#);
        h.message("/private", r#"{"op":"subscribe","success":true}"#);
    }
    h
}

#[test]
fn enabled_categories_option_pagination_and_reindexed_product_metadata_commit_together() {
    let mut h = ready(false);
    let old = h.engine.store().markets().to_vec();
    h.command(Command::Venue {
        command: 40,
        action: VenueAction::RefreshMarkets,
    });
    assert_eq!(h.requests.len(), 5);
    assert!(
        h.requests
            .iter()
            .all(|request| !request.url.contains("fee-rate")
                && Harness::header(request, "X-BAPI-SIGN").is_none())
    );
    assert!(
        h.requests
            .iter()
            .any(|request| request.url.contains("category=option")
                && request.url.contains("baseCoin=All"))
    );
    for category in ["linear", "inverse"] {
        respond(
            &mut h,
            "instruments-info",
            category,
            Some("status=PreLaunch"),
            EMPTY,
        );
    }
    respond(
        &mut h,
        "instruments-info",
        "linear",
        None,
        &support::INSTRUMENTS.replace("BTCUSDT", "SOLUSDT"),
    );
    respond(
        &mut h,
        "instruments-info",
        "inverse",
        None,
        &inverse_instruments(),
    );
    assert!(
        h.result(40).is_none(),
        "only the option response may finish this walk"
    );
    respond(
        &mut h,
        "instruments-info",
        "option",
        None,
        &OPTIONS.replace(
            "\"nextPageCursor\":\"\"",
            "\"nextPageCursor\":\"option-next\"",
        ),
    );
    assert_eq!(h.engine.store().markets(), old);
    assert!(h.result(40).is_none(), "{:?}", h.result(40));
    respond(
        &mut h,
        "instruments-info",
        "option",
        Some("cursor=option-next"),
        EMPTY,
    );
    assert!(h.result(40).unwrap().is_ok());
    assert!(h.engine.store().market("SOLUSDT").is_some());
    assert!(h.engine.store().market("BTCUSDT").is_none());
    let products = h.engine.store().derivative_products().unwrap();
    assert_eq!(products.len(), 2);
    assert!(
        products.iter().any(
            |row| h.engine.store().markets()[row.market as usize].symbol == OPTION
                && row.strike == Some(100000.)
        )
    );
    assert!(
        h.engine
            .store()
            .market("BTCUSD")
            .unwrap()
            .is_inverse
            .unwrap()
    );
    h.flush_subscriptions();
    assert!(
        h.sent_on("public/linear")
            .iter()
            .any(|text| text.contains("tickers.SOLUSDT"))
    );
}

#[test]
fn a_failed_option_category_preserves_all_old_products_and_never_deactivates() {
    let mut h = ready(false);
    let old = h.engine.store().markets().to_vec();
    let products = h.engine.store().derivative_products().unwrap().to_vec();
    h.command(Command::Venue {
        command: 41,
        action: VenueAction::RefreshMarkets,
    });
    let request = h.take("category=option");
    h.reply(
        request.id,
        200,
        r#"{"retCode":10003,"retMsg":"invalid key"}"#,
    );
    assert!(h.result(41).unwrap().is_err());
    assert_eq!(h.engine.store().markets(), old);
    assert_eq!(h.engine.store().derivative_products().unwrap(), products);
    assert!(!h.deactivated);
}

#[test]
fn malformed_trading_option_terms_do_not_delete_known_products_with_a_valid_peer() {
    for (field, value) in [
        ("optionsType", None),
        ("symbol", Some("BTC-25DEC26-BAD-C-USDT")),
        ("deliveryTime", None),
        ("deliveryTime", Some("bad")),
        ("deliveryTime", Some("NaN")),
        ("deliveryTime", Some("Infinity")),
        ("deliveryTime", Some("0")),
        ("deliveryTime", Some("-1")),
        ("baseCoin", None),
        ("baseCoin", Some("")),
        ("baseCoin", Some(" ")),
        ("quoteCoin", None),
        ("quoteCoin", Some("")),
        ("quoteCoin", Some(" ")),
        ("settleCoin", None),
        ("settleCoin", Some("")),
        ("settleCoin", Some(" ")),
    ] {
        let mut h = ready(false);
        let old = h.engine.store().markets().to_vec();
        let products = h.engine.store().derivative_products().unwrap().to_vec();
        h.command(Command::Venue {
            command: 42,
            action: VenueAction::RefreshMarkets,
        });
        for category in ["linear", "inverse"] {
            respond(
                &mut h,
                "instruments-info",
                category,
                Some("status=PreLaunch"),
                EMPTY,
            );
            let body = if category == "linear" {
                support::INSTRUMENTS.to_owned()
            } else {
                inverse_instruments()
            };
            respond(&mut h, "instruments-info", category, None, &body);
        }
        assert!(
            h.result(42).is_none(),
            "only the option response may fail this walk"
        );
        let mut payload: serde_json::Value = serde_json::from_str(OPTIONS).unwrap();
        let row = payload["result"]["list"][0].as_object_mut().unwrap();
        if let Some(value) = value {
            row.insert(field.to_owned(), serde_json::json!(value));
        } else {
            row.remove(field);
        }
        assert_eq!(payload["result"]["list"][0]["status"], "Trading");
        assert_eq!(
            payload["result"]["list"][1]["symbol"],
            "ETH-25DEC26-3000-P-USDT"
        );
        respond(
            &mut h,
            "instruments-info",
            "option",
            None,
            &payload.to_string(),
        );
        assert!(h.result(42).unwrap().is_err());
        assert_eq!(h.engine.store().markets(), old);
        assert_eq!(h.engine.store().derivative_products().unwrap(), products);
        assert!(h.engine.store().market(OPTION).is_some());
        assert!(!h.deactivated);
    }
}

#[test]
fn non_trading_options_remain_deliberately_filtered_without_required_trading_terms() {
    let mut h = ready(false);
    h.command(Command::Venue {
        command: 43,
        action: VenueAction::RefreshMarkets,
    });
    for category in ["linear", "inverse"] {
        respond(
            &mut h,
            "instruments-info",
            category,
            Some("status=PreLaunch"),
            EMPTY,
        );
        let body = if category == "linear" {
            support::INSTRUMENTS.to_owned()
        } else {
            inverse_instruments()
        };
        respond(&mut h, "instruments-info", category, None, &body);
    }
    assert!(
        h.result(43).is_none(),
        "only the option response may finish this walk"
    );
    let mut payload: serde_json::Value = serde_json::from_str(OPTIONS).unwrap();
    let row = payload["result"]["list"][0].as_object_mut().unwrap();
    row.insert("status".into(), serde_json::json!("Delivering"));
    for field in [
        "optionsType",
        "deliveryTime",
        "baseCoin",
        "quoteCoin",
        "settleCoin",
    ] {
        row.remove(field);
    }
    respond(
        &mut h,
        "instruments-info",
        "option",
        None,
        &payload.to_string(),
    );
    assert!(h.result(43).unwrap().is_ok(), "{:?}", h.result(43));
    assert!(h.engine.store().market(OPTION).is_none());
    assert_eq!(h.engine.store().derivative_products().unwrap().len(), 1);
    assert!(h.engine.store().market("ETH-25DEC26-3000-P-USDT").is_some());
}
