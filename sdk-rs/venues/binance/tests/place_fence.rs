//! An Unknown placement fences its symbol until a fresh, clean open-orders
//! snapshot proves what landed (engine #2279 placement fence; Binance has no
//! placement scope, so the mark comes from the Unknown write fact, Phase B).
use support::Options;
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Command, Event, HttpRequest, Method, Outcome};
use safe_cex_venue_binance::Variant;
use support::Harness;

const ALL: [Variant; 4] = [
    Variant::UsdM,
    Variant::PortfolioMargin,
    Variant::Spot,
    Variant::CoinM,
];

fn symbol(variant: Variant) -> &'static str {
    if variant == Variant::CoinM {
        "BTCUSD_PERP"
    } else {
        "BTCUSDT"
    }
}
fn order_path(variant: Variant) -> &'static str {
    match variant {
        Variant::UsdM => "/fapi/v1/order",
        Variant::PortfolioMargin => "/papi/v1/um/order",
        Variant::Spot => "/api/v3/order",
        Variant::CoinM => "/dapi/v1/order",
    }
}
/// The open-orders reads one Binance snapshot joins.
fn order_reads(variant: Variant) -> &'static [&'static str] {
    match variant {
        Variant::UsdM => &["/fapi/v1/openOrders", "/fapi/v1/openAlgoOrders"],
        Variant::PortfolioMargin => &[
            "/papi/v1/um/openOrders",
            "/papi/v1/um/conditional/openOrders",
        ],
        Variant::Spot => &["/api/v3/openOrders"],
        Variant::CoinM => &["/dapi/v1/openOrders"],
    }
}

fn futures_ready(variant: Variant) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    let pm = variant == Variant::PortfolioMargin;
    let root = if pm { "/papi/v1/um" } else { "/fapi/v1" };
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond(&format!("{root}/leverageBracket"), 200, "[]");
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond(
        if pm {
            "/papi/v1/listenKey"
        } else {
            "/fapi/v1/listenKey"
        },
        200,
        r#"{"listenKey":"private-key"}"#,
    );
    h.respond(
        &format!("{root}/positionSide/dual"),
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond(
        if pm {
            "/papi/v1/um/positionRisk"
        } else {
            "/fapi/v3/positionRisk"
        },
        200,
        "[]",
    );
    if pm {
        h.respond("/papi/v1/balance", 200, "[]");
    } else {
        h.respond("/fapi/v1/symbolConfig", 200, "[]");
        h.respond("/fapi/v3/account", 200, r#"{"assets":[]}"#);
    }
    for path in order_reads(variant) {
        h.respond(path, 200, "[]");
    }
    let private = h.socket(if pm { "/pm/ws" } else { "/private/ws" }).unwrap();
    h.event(Event::WsOpened { socket: private });
    h
}

fn coin_or_spot_ready(variant: Variant) -> Harness {
    let spot = variant == Variant::Spot;
    let mut h = Harness::started(Options {
        variant,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    let symbol = symbol(variant);
    let prefix = if spot { "/api/v3" } else { "/dapi/v1" };
    h.respond(&format!("{prefix}/exchangeInfo"), 200, &format!(r#"{{"symbols":[{{"symbol":"{symbol}","status":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{{"filterType":"PRICE_FILTER","tickSize":"0.1"}},{{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}},{{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}}]}}]}}"#));
    if !spot {
        h.respond("/dapi/v2/leverageBracket", 200, "[]");
    }
    h.respond(&format!("{prefix}/ticker/24hr"), 200, &format!(r#"[{{"symbol":"{symbol}","lastPrice":"10000","bidPrice":"9999","askPrice":"10001","priceChangePercent":"0","volume":"1","quoteVolume":"10000","baseVolume":"1"}}]"#));
    if !spot {
        h.respond(
            "/dapi/v1/ticker/bookTicker",
            200,
            r#"[{"symbol":"BTCUSD_PERP","bidPrice":"9999","askPrice":"10001"}]"#,
        );
        h.respond("/dapi/v1/premiumIndex", 200, r#"[{"symbol":"BTCUSD_PERP","markPrice":"10000","indexPrice":"10000","lastFundingRate":"0","nextFundingTime":100000}]"#);
        h.respond("/dapi/v1/listenKey", 200, r#"{"listenKey":"coin-key"}"#);
        h.respond(
            "/dapi/v1/positionSide/dual",
            200,
            r#"{"dualSidePosition":false}"#,
        );
        h.respond("/dapi/v1/positionRisk", 200, "[]");
    }
    h.respond(
        &format!("{prefix}/account"),
        200,
        if spot {
            r#"{"balances":[]}"#
        } else {
            r#"{"assets":[]}"#
        },
    );
    h.respond(&format!("{prefix}/openOrders"), 200, "[]");
    let socket = h
        .socket(if spot { "ws-api/v3" } else { "/ws/coin-key" })
        .unwrap();
    h.event(Event::WsOpened { socket });
    if spot {
        h.message(
            "ws-api/v3",
            r#"{"id":"tealstreet-user-data-10","status":200,"result":{"subscriptionId":0}}"#,
        );
        h.respond("/api/v3/account", 200, r#"{"balances":[]}"#);
        h.respond("/api/v3/openOrders", 200, "[]");
        h.message("ws-api/v3", r#"{"id":"tealstreet-user-data-heartbeat-11","status":200,"result":[{"subscriptionId":0}]}"#);
    }
    h
}

fn ready(variant: Variant) -> Harness {
    let mut h = if matches!(variant, Variant::Spot | Variant::CoinM) {
        coin_or_spot_ready(variant)
    } else {
        futures_ready(variant)
    };
    h.requests.clear();
    h
}

fn place(h: &mut Harness, variant: Variant, command: u32) {
    let (amount, price) = match variant {
        Variant::UsdM | Variant::PortfolioMargin => (0.01, 99.0),
        _ => (1.0, 9000.0),
    };
    let opts: PlaceOrderOpts = serde_json::from_str(&format!(
        r#"{{"symbol":"{}","type":"limit","side":"buy","amount":{amount},"price":{price}}}"#,
        symbol(variant)
    ))
    .unwrap();
    h.event(Event::Command(Command::PlaceOrders {
        command,
        orders: vec![opts],
    }));
}

fn posts(h: &Harness, variant: Variant) -> usize {
    h.requests
        .iter()
        .filter(|r| r.method == Method::Post && r.url.contains(order_path(variant)))
        .count()
}

fn take_reads(h: &mut Harness, variant: Variant) -> Vec<HttpRequest> {
    order_reads(variant)
        .iter()
        .map(|path| h.take(path))
        .collect()
}

fn answer(h: &mut Harness, reads: Vec<HttpRequest>, body: &str) {
    for read in reads {
        h.reply(read.id, 200, "", body);
    }
}

fn outcome(h: &Harness, command: u32) -> Option<Outcome> {
    h.failures
        .iter()
        .find(|(c, _)| *c == command)
        .and_then(|(_, e)| e.outcome)
}

fn refused_uncertain(h: &Harness, command: u32) -> bool {
    h.failures.iter().any(|(c, e)| {
        *c == command && e.outcome == Some(Outcome::NotSent) && e.message.contains("uncertain")
    })
}

/// A lost (5xx) placement: its outcome is Unknown and its symbol is fenced.
fn unknown_placement(h: &mut Harness, variant: Variant, command: u32) {
    place(h, variant, command);
    let post = h.take(order_path(variant));
    h.reply(post.id, 503, "", "Service Unavailable");
    assert_eq!(
        outcome(h, command),
        Some(Outcome::Unknown),
        "{variant:?} 5xx placement is Unknown"
    );
}

/// An order event schedules a REST snapshot; after the debounce its reads are in flight.
fn pre_mark_reads(h: &mut Harness, variant: Variant) -> Vec<HttpRequest> {
    match variant {
        Variant::Spot => h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"executionReport","i":99,"s":"BTCUSDT","c":"other","S":"BUY","o":"LIMIT","X":"NEW","x":"NEW","p":"9000","q":"1","z":"0","l":"0","L":"0"}}"#),
        Variant::CoinM => h.message("/ws/coin-key", r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":99,"s":"BTCUSD_PERP","S":"BUY","o":"LIMIT","X":"NEW","p":"9000","q":"1","z":"0"}}"#),
        Variant::PortfolioMargin => h.message("/pm/ws", r#"{"e":"ORDER_TRADE_UPDATE","fs":"UM","o":{"i":99,"c":"other","s":"BTCUSDT","S":"BUY","o":"LIMIT","X":"NEW","p":"99","q":"0.01","z":"0"}}"#),
        Variant::UsdM => h.message("/private/ws", r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":99,"c":"other","s":"BTCUSDT","S":"BUY","o":"LIMIT","X":"NEW","p":"99","q":"0.01","z":"0"}}"#),
    }
    h.advance(10_000.0);
    take_reads(h, variant)
}

/// Unknown placement -> retry refused NotSent with no POST -> pre-mark read
/// completes, still fenced -> owed fresh read releases -> retry places.
fn fence_sequence(variant: Variant) {
    let mut h = ready(variant);
    let stale = pre_mark_reads(&mut h, variant);
    unknown_placement(&mut h, variant, 70);
    // (1) The retry is refused before any request.
    place(&mut h, variant, 71);
    assert_eq!(posts(&h, variant), 0, "{variant:?} retry sends nothing");
    assert!(
        refused_uncertain(&h, 71),
        "{variant:?} retry refused NotSent"
    );
    // (2) The read issued before the Unknown completes cleanly: still fenced.
    answer(&mut h, stale, "[]");
    place(&mut h, variant, 72);
    assert_eq!(
        posts(&h, variant),
        0,
        "{variant:?} stale read does not release"
    );
    assert!(
        refused_uncertain(&h, 72),
        "{variant:?} still refused after stale read"
    );
    // (3) The owed fresh read releases it.
    let fresh = take_reads(&mut h, variant);
    answer(&mut h, fresh, "[]");
    // (4) The retry now places.
    place(&mut h, variant, 73);
    assert_eq!(
        posts(&h, variant),
        1,
        "{variant:?} retry places after release"
    );
}

#[test]
fn usdm_unknown_placement_fences_until_a_fresh_clean_snapshot() {
    fence_sequence(Variant::UsdM);
}
#[test]
fn pm_unknown_placement_fences_until_a_fresh_clean_snapshot() {
    fence_sequence(Variant::PortfolioMargin);
}
#[test]
fn spot_unknown_placement_fences_until_a_fresh_clean_snapshot() {
    fence_sequence(Variant::Spot);
}
#[test]
fn coinm_unknown_placement_fences_until_a_fresh_clean_snapshot() {
    fence_sequence(Variant::CoinM);
}

#[test]
fn a_snapshot_with_an_unmappable_row_does_not_release_the_fence() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        unknown_placement(&mut h, variant, 70);
        let reads = take_reads(&mut h, variant);
        let mut reads = reads.into_iter();
        let regular = reads.next().unwrap();
        h.reply(
            regular.id,
            200,
            "",
            r#"[{"symbol":"BTCUSDT","orderId":"not-a-number"}]"#,
        );
        answer(&mut h, reads.collect(), "[]");
        place(&mut h, variant, 71);
        assert_eq!(posts(&h, variant), 0, "{variant:?}");
        assert!(refused_uncertain(&h, 71), "{variant:?}");
    }
}

#[test]
fn cancel_by_id_stays_admitted_while_fenced() {
    for variant in ALL {
        let mut h = ready(variant);
        unknown_placement(&mut h, variant, 70);
        h.requests.clear();
        h.event(Event::Command(Command::CancelOrders {
            command: 80,
            ids: vec!["12".into()],
        }));
        assert!(
            !refused_uncertain(&h, 80),
            "{variant:?} cancel by id is not fenced"
        );
    }
}

#[test]
fn cancel_all_stays_refused_while_fenced() {
    // Default fence_uncertain_cancel_all: opting out would oblige the venue to
    // cancel a late-landing order itself once its fresh read proves it.
    for variant in ALL {
        let mut h = ready(variant);
        unknown_placement(&mut h, variant, 70);
        h.requests.clear();
        h.event(Event::Command(Command::CancelSymbolOrders {
            command: 81,
            symbol: symbol(variant).into(),
        }));
        assert!(
            !h.requests.iter().any(|r| r.method == Method::Delete),
            "{variant:?} cancel-all sends nothing while fenced"
        );
        assert!(
            refused_uncertain(&h, 81)
                || h.failures
                    .iter()
                    .any(|(c, e)| *c == 81 && e.message.contains("uncertain")),
            "{variant:?}"
        );
    }
}
