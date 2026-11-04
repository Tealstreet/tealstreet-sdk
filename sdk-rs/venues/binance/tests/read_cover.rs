//! Read covers on a real Binance USD-M source (docs/plans/RUST_FRAME_FED_READ_COVER.md
//! step 1d): the open-orders read commits its rows with the cover it proves, and
//! native order state changes only for the symbols it may replace.
mod support;
use safe_cex_engine::Event;
use safe_cex_engine::wire::section;
use support::Harness;
use support::Options;

const POSITION_RISK: &str = r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0.010","entryPrice":"95000.0","breakEvenPrice":"95010.0","markPrice":"96000.0","unRealizedProfit":"10.00","liquidationPrice":"80000.5","isolatedMargin":"0","notional":"960.0","marginAsset":"USDT","isolatedWallet":"0","initialMargin":"48","maintMargin":"3.8","positionInitialMargin":"48","openOrderInitialMargin":"0","adl":1,"bidNotional":"0","askNotional":"0","updateTime":1720736417660}]"#;
const SYMBOL_CONFIG: &str = r#"[{"symbol":"BTCUSDT","marginType":"CROSSED","isAutoAddMargin":false,"leverage":20,"maxNotionalValue":"$1000000"}]"#;
const ACCOUNT: &str = r#"{"totalInitialMargin":"48","totalWalletBalance":"1000","availableBalance":"952","assets":[{"asset":"USDT","walletBalance":"1000.0","unrealizedProfit":"10.0","marginBalance":"1010.0","maintMargin":"3.8","initialMargin":"48.0","availableBalance":"952.0","updateTime":1625474304765},{"asset":"BTC","walletBalance":"0.0","unrealizedProfit":"0.0","initialMargin":"0","availableBalance":"0"}],"positions":[]}"#;
const NEW_ORDER: &str = r#"{"e":"ORDER_TRADE_UPDATE","E":1568879465651,"T":1568879465650,"o":{"s":"BTCUSDT","c":"TEST","S":"SELL","o":"LIMIT","f":"GTC","q":"0.002","p":"97000","ap":"0","sp":"0","x":"NEW","X":"NEW","i":8886774,"l":"0","z":"0","L":"0","T":1568879465650,"t":0,"b":"0","a":"0","m":false,"R":false,"wt":"CONTRACT_PRICE","ot":"LIMIT","ps":"BOTH","cp":false,"rp":"0"}}"#;

fn credentials() -> Options {
    Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    }
}

/// A private source with every startup read answered but its open-orders reads,
/// and its sockets open.
fn orders_read_out() -> Harness {
    let mut h = Harness::started(credentials());
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond(
        "/fapi/v1/leverageBracket",
        200,
        r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125,"notionalCap":50000}]}]"#,
    );
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond(
        "/fapi/v1/listenKey",
        200,
        r#"{"listenKey":"pqia91ma19a5s61cv6a81va65sdf19v8a65a1a5s61cv6a81va65sdf19v8a65a1"}"#,
    );
    h.respond(
        "/fapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/fapi/v3/positionRisk", 200, POSITION_RISK);
    h.respond("/fapi/v1/symbolConfig", 200, SYMBOL_CONFIG);
    h.respond("/fapi/v3/account", 200, ACCOUNT);
    for path in ["/public/ws", "/market/ws", "/private/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    assert!(h.has_request("/fapi/v1/openOrders"), "{:?}", h.paths());
    h
}

fn answer_orders(h: &mut Harness, regular: &str) {
    h.respond("/fapi/v1/openOrders", 200, regular);
    h.respond("/fapi/v1/openAlgoOrders", 200, "[]");
}

fn account_cover(h: &mut Harness) -> (Vec<u16>, Option<f64>) {
    let frame = h.engine.snapshot_frame(h.now).to_vec();
    let kinds = safe_cex_engine::wire::sections(&frame)
        .map(|(kind, _)| kind)
        .collect();
    let cover = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::ORDERS_READ)
        .map(|(_, payload)| f64::from_le_bytes(payload[8..16].try_into().unwrap()))
        .filter(|cover| !cover.is_nan());
    (kinds, cover)
}

#[test]
fn a_complete_open_orders_read_sends_its_cover_beside_orders() {
    let mut h = orders_read_out();
    let observed = h.now;
    h.advance(4_000.0);
    answer_orders(&mut h, "[]");
    let (kinds, cover) = account_cover(&mut h);
    let orders = kinds
        .iter()
        .position(|kind| *kind == section::ORDERS)
        .unwrap();
    assert_eq!(kinds[orders + 1], section::ORDERS_READ, "{kinds:?}");
    let cover = cover.unwrap();
    assert!(
        cover <= observed && cover < h.now,
        "send time, not landing: {cover}"
    );
}

/// H10: the user stream reports an order after the read's release; the read
/// lacks it, keeps the row, and proves no cover.
#[test]
fn a_stream_order_after_the_reads_release_leaves_no_cover() {
    let mut h = orders_read_out();
    h.message("/private/ws", NEW_ORDER);
    answer_orders(&mut h, "[]");
    assert!(
        h.engine.store().order("8886774").is_some(),
        "the stream row stays"
    );
    assert!(
        h.engine.store().order("7").is_none(),
        "the row was not mapped"
    );
    assert_eq!(account_cover(&mut h).1, None);
}

/// A row the read cannot map (an unsupported native type) proves no account cover.
#[test]
fn an_unmappable_open_order_proves_no_cover() {
    let mut h = orders_read_out();
    answer_orders(
        &mut h,
        r#"[{"symbol":"BTCUSDT","orderId":7,"clientOrderId":"bad","price":"100","origQty":"1","executedQty":"0","side":"BUY","positionSide":"BOTH","status":"NEW","type":"NOT_A_BINANCE_TYPE","timeInForce":"GTC","stopPrice":"0","reduceOnly":false,"closePosition":false,"workingType":"CONTRACT_PRICE","updateTime":1}]"#,
    );
    assert!(
        h.engine.store().order("7").is_none(),
        "the row was not mapped"
    );
    assert_eq!(account_cover(&mut h).1, None);
}

#[test]
fn split_reads_cover_uses_earliest_actual_claim() {
    let mut h = Harness::started_deferred(credentials());
    h.hold_signatures(true);
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.sign("/fapi/v1/openOrders");
    let early = h.now;
    h.respond("/fapi/v1/openOrders", 200, "[]");
    h.now += 1000.;
    h.sign("/fapi/v1/openAlgoOrders");
    h.respond("/fapi/v1/openAlgoOrders", 200, "[]");
    assert_eq!(
        account_cover(&mut h).1,
        Some(early),
        "all rows are only as fresh as the earliest actual leg release"
    );
}
fn live_stop(h: &mut Harness) {
    let stop = NEW_ORDER
        .replace(r#""o":"LIMIT""#, r#""o":"STOP""#)
        .replace(r#""ot":"LIMIT""#, r#""ot":"STOP""#)
        .replace(r#""sp":"0""#, r#""sp":"90000""#);
    h.message("/private/ws", &stop);
    assert_eq!(
        h.engine.store().order("8886774").unwrap().kind,
        safe_cex_engine::model::OrderType::StopLoss
    );
}
fn amend_must_stay_refused(h: &mut Harness) {
    h.event(Event::Command(safe_cex_engine::Command::AmendOrder {
        command: 900,
        id: "8886774".into(),
        price: Some(91000.),
        amount: None,
    }));
    assert!(
        !h.requests
            .iter()
            .any(|r| r.method != safe_cex_engine::Method::Get),
        "refused stale read cannot remove stop-limit protection: {:?}",
        h.paths()
    );
    assert!(
        h.failures
            .iter()
            .any(|(id, e)| *id == 900 && e.kind == Some(safe_cex_engine::ErrorKind::Unsupported))
    );
}
#[test]
fn stream_stop_limit_amend_is_refused_control() {
    let mut h = orders_read_out();
    live_stop(&mut h);
    amend_must_stay_refused(&mut h);
}
#[test]
fn refused_old_read_cannot_change_stop_limit_classification() {
    let mut h = orders_read_out();
    live_stop(&mut h);
    answer_orders(
        &mut h,
        r#"[{"symbol":"BTCUSDT","orderId":8886774,"clientOrderId":"TEST","price":"97000","origQty":"0.002","executedQty":"0","side":"SELL","positionSide":"BOTH","status":"NEW","type":"STOP_MARKET","timeInForce":"GTC","stopPrice":"90000","reduceOnly":false,"closePosition":false,"workingType":"CONTRACT_PRICE","updateTime":1}]"#,
    );
    assert_eq!(
        h.engine.store().order("8886774").unwrap().kind,
        safe_cex_engine::model::OrderType::StopLoss
    );
    assert_eq!(account_cover(&mut h).1, None);
    amend_must_stay_refused(&mut h);
}
#[test]
fn unidentified_row_grants_no_cover() {
    let mut h = orders_read_out();
    answer_orders(&mut h, r#"[{"broken":true}]"#);
    assert_eq!(account_cover(&mut h).1, None);
}

/// The CAPABILITIES record's `order_read_covers` bit in a variant's snapshot.
fn advertises_covers(variant: safe_cex_venue_binance::Variant, portfolio_margin: bool) -> bool {
    let mut h = Harness::started(Options {
        variant,
        portfolio_margin,
        ..credentials()
    });
    let frame = h.engine.snapshot_frame(h.now).to_vec();
    let (_, payload) = safe_cex_engine::wire::sections(&frame)
        .find(|(kind, _)| *kind == section::CAPABILITIES)
        .unwrap();
    let flags = u32::from_le_bytes(payload[..4].try_into().unwrap());
    flags & 1 << safe_cex_engine::Capabilities::ORDER_READ_COVERS_BIT != 0
}

/// Every frame-fed alias (USD-M, PM, COIN-M, spot) advertises read covers;
/// options, whose orders read commits no cover, does not.
#[test]
fn binance_advertises_read_covers() {
    use safe_cex_venue_binance::Variant;
    for (variant, pm) in [
        (Variant::UsdM, false),
        (Variant::UsdM, true),
        (Variant::CoinM, false),
        (Variant::Spot, false),
    ] {
        assert!(advertises_covers(variant, pm), "{variant:?} pm {pm}");
    }
    assert!(!advertises_covers(Variant::Options, false));
}

/// A loaded USD-M source, its startup orders read answered and nothing out.
fn loaded() -> Harness {
    let mut h = orders_read_out();
    while h.has_request("/fapi/v1/openOrders") || h.has_request("/fapi/v1/openAlgoOrders") {
        for path in ["/fapi/v1/openOrders", "/fapi/v1/openAlgoOrders"] {
            if h.has_request(path) {
                h.respond(path, 200, "[]");
            }
        }
    }
    h.requests.clear();
    h
}

fn refresh(h: &mut Harness, command: u32, symbols: Option<Vec<String>>) {
    h.event(Event::Command(safe_cex_engine::Command::Venue {
        command,
        action: safe_cex_engine::VenueAction::RefreshOrders { symbols },
    }));
}

fn orders_reads(h: &Harness) -> Vec<safe_cex_engine::RequestId> {
    h.requests
        .iter()
        .filter(|r| {
            r.url.contains("/fapi/v1/openOrders") || r.url.contains("/fapi/v1/openAlgoOrders")
        })
        .map(|r| r.id)
        .collect()
}

/// Answer the reads `ids` with empty lists.
fn answer_reads(h: &mut Harness, ids: &[safe_cex_engine::RequestId]) {
    for id in ids {
        h.requests.retain(|r| r.id != *id);
        h.reply(*id, 200, "", "[]");
    }
}

/// The ask reads both legs; their answers publish an account cover at the
/// ask's send time, not the landing time.
#[test]
fn a_refresh_ask_reads_orders_and_publishes_its_send_time() {
    for symbols in [None, Some(vec!["BTCUSDT".to_owned()])] {
        let mut h = loaded();
        h.advance(5_000.0);
        let asked = h.now;
        refresh(&mut h, 60, symbols.clone());
        let reads = orders_reads(&h);
        assert_eq!(reads.len(), 2, "{symbols:?}: {:?}", h.paths());
        h.advance(1_000.0);
        answer_reads(&mut h, &reads);
        assert_eq!(account_cover(&mut h).1, Some(asked), "{symbols:?}");
    }
}

/// H7/H3: asks while the read is out start nothing; exactly one follow-up
/// batch starts after it lands, and nothing after that one.
#[test]
fn asks_while_a_read_is_out_get_one_follow_up() {
    let mut h = loaded();
    h.advance(5_000.0);
    refresh(&mut h, 60, Some(vec!["BTCUSDT".into()]));
    let first = orders_reads(&h);
    assert_eq!(first.len(), 2, "{:?}", h.paths());
    for command in 61..161 {
        refresh(&mut h, command, None);
    }
    assert_eq!(orders_reads(&h), first, "H7: nothing more while it is out");
    h.advance(100.0);
    answer_reads(&mut h, &first);
    let follow = orders_reads(&h);
    assert_eq!(follow.len(), 2, "one follow-up batch: {:?}", h.paths());
    h.advance(100.0);
    let sent = h.now - 100.0;
    answer_reads(&mut h, &follow);
    assert_eq!(account_cover(&mut h).1, Some(sent));
    h.advance(1_000.0);
    assert!(orders_reads(&h).is_empty(), "{:?}", h.paths());
}

/// A loaded portfolio margin, COIN-M or spot source with nothing out, and its
/// open-orders legs.
fn loaded_variant(variant: &str) -> (Harness, &'static [&'static str]) {
    use safe_cex_venue_binance::Variant;
    let (options, legs): (Options, &[&str]) = match variant {
        "pm" => (
            Options {
                portfolio_margin: true,
                ..credentials()
            },
            &[
                "/papi/v1/um/openOrders",
                "/papi/v1/um/conditional/openOrders",
            ],
        ),
        "coinm" => (
            Options {
                variant: Variant::CoinM,
                ..credentials()
            },
            &["/dapi/v1/openOrders"],
        ),
        _ => (
            Options {
                variant: Variant::Spot,
                ..credentials()
            },
            &["/api/v3/openOrders"],
        ),
    };
    let mut h = Harness::started(options);
    let answers: &[(&str, &str)] = match variant {
        "pm" => &[
            ("/fapi/v1/exchangeInfo", support::EXCHANGE_INFO),
            (
                "/papi/v1/um/leverageBracket",
                r#"[{"symbol":"BTCUSDT","brackets":[{"bracket":1,"initialLeverage":125}]}]"#,
            ),
            ("/fapi/v1/ticker/24hr", support::DAILY),
            ("/fapi/v1/ticker/bookTicker", support::BOOK_TICKERS),
            ("/fapi/v1/premiumIndex", support::PREMIUM),
            ("/papi/v1/listenKey", r#"{"listenKey":"pmkey"}"#),
            (
                "/papi/v1/um/positionSide/dual",
                r#"{"dualSidePosition":false}"#,
            ),
            ("/papi/v1/um/positionRisk", "[]"),
            ("/papi/v1/balance", "[]"),
        ],
        "coinm" => &[
            (
                "/dapi/v1/exchangeInfo",
                r#"{"symbols":[{"symbol":"BTCUSD_PERP","contractStatus":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"1000"}]}]}"#,
            ),
            (
                "/dapi/v2/leverageBracket",
                r#"[{"symbol":"BTCUSD_PERP","brackets":[{"initialLeverage":125}]}]"#,
            ),
            (
                "/dapi/v1/ticker/24hr",
                r#"[{"symbol":"BTCUSD_PERP","lastPrice":"10000","priceChangePercent":"1","volume":"100","baseVolume":"1"}]"#,
            ),
            (
                "/dapi/v1/ticker/bookTicker",
                r#"[{"symbol":"BTCUSD_PERP","bidPrice":"9999","askPrice":"10001"}]"#,
            ),
            (
                "/dapi/v1/premiumIndex",
                r#"[{"symbol":"BTCUSD_PERP","markPrice":"10000","indexPrice":"10000","lastFundingRate":"0.0001","nextFundingTime":100000}]"#,
            ),
            ("/dapi/v1/listenKey", r#"{"listenKey":"coin-key"}"#),
            (
                "/dapi/v1/positionSide/dual",
                r#"{"dualSidePosition":false}"#,
            ),
            ("/dapi/v1/positionRisk", "[]"),
            ("/dapi/v1/account", r#"{"assets":[]}"#),
        ],
        _ => &[
            (
                "/api/v3/exchangeInfo",
                r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}]}]}"#,
            ),
            (
                "/api/v3/ticker/24hr",
                r#"[{"symbol":"BTCUSDT","lastPrice":"10000","bidPrice":"9999","askPrice":"10001","priceChangePercent":"0","volume":"1","quoteVolume":"10000"}]"#,
            ),
        ],
    };
    for (path, body) in answers {
        h.respond(path, 200, body);
    }
    let sockets: Vec<_> = h.sockets.iter().map(|(socket, _)| *socket).collect();
    for socket in sockets {
        h.event(Event::WsOpened { socket });
    }
    if variant == "spot" {
        h.respond("/api/v3/account", 200, r#"{"balances":[]}"#);
    }
    while legs.iter().any(|leg| h.has_request(leg)) {
        for leg in legs {
            if h.has_request(leg) {
                h.respond(leg, 200, "[]");
            }
        }
    }
    h.requests.clear();
    (h, legs)
}

/// The ask on each other frame-fed alias reads every orders leg it reads at
/// startup, and their answers publish an account cover at the send time.
#[test]
fn every_alias_answers_a_refresh_ask_with_a_cover() {
    for variant in ["pm", "coinm", "spot"] {
        let (mut h, legs) = loaded_variant(variant);
        h.advance(5_000.0);
        h.requests.clear();
        let asked = h.now;
        refresh(&mut h, 60, Some(vec!["BTCUSDT".into()]));
        let reads: Vec<_> = h
            .requests
            .iter()
            .filter(|r| legs.iter().any(|leg| r.url.contains(leg)))
            .map(|r| r.id)
            .collect();
        assert_eq!(reads.len(), legs.len(), "{variant}: {:?}", h.paths());
        refresh(&mut h, 61, None);
        h.advance(100.0);
        answer_reads(&mut h, &reads);
        assert_eq!(account_cover(&mut h).1, Some(asked), "{variant}");
        let follow = h
            .requests
            .iter()
            .filter(|r| legs.iter().any(|leg| r.url.contains(leg)))
            .count();
        assert_eq!(follow, legs.len(), "{variant}: one follow-up batch");
    }
}
