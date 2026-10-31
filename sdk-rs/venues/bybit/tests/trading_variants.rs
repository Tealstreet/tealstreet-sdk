//! Variant order semantics from the V5 create/cancel specifications and the
//! documented Hyro simulator identity/reconciliation contract.
mod support;
use safe_cex_engine::model::{BybitBbo, OrderSide, OrderType, PlaceOrderOpts};
use safe_cex_engine::{Command, Event, HttpRequest, Query};
use safe_cex_venue_bybit::{Endpoints, Variant};
use support::Harness;
use support::Options;

const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;
const WALLET: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"accountType":"UNIFIED","totalEquity":"1000","totalWalletBalance":"1000","totalMarginBalance":"1000","totalAvailableBalance":"1000","totalInitialMargin":"0","totalMaintenanceMargin":"0","totalPerpUPL":"0","coin":[{"coin":"USDT","walletBalance":"1000","usdValue":"1000","equity":"1000","totalPositionIM":"0","totalOrderIM":"0","locked":"0"}]}]}}"#;

fn options(variant: Variant) -> Options {
    Options {
        variant,
        key: Some("key".into()),
        secret: Some("secret".into()),
        endpoints: variant.is_hyro().then(|| Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    }
}

fn ready(variant: Variant) -> Harness {
    ready_with(options(variant))
}

fn ready_with(options: Options) -> Harness {
    let variant = options.variant;
    let mut h = Harness::started(options);
    if h.has_request("/v5/account/info") {
        h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    }
    if h.has_request("/v5/account/fee-rate") {
        h.respond("/v5/account/fee-rate", 200, EMPTY);
    }
    if h.has_request("status=PreLaunch") {
        h.respond("status=PreLaunch", 200, EMPTY);
    }
    let instruments = if variant.is_inverse() {
        support::INSTRUMENTS
            .replace("BTCUSDT", "BTCUSD")
            .replace("ETHUSDT", "ETHUSD")
            .replace("LinearPerpetual", "InversePerpetual")
            .replace("\"USDT\"", "\"USD\"")
            .replace("1190.000", "10")
            .replace("0.001", "1")
    } else {
        support::INSTRUMENTS.to_owned()
    };
    h.respond("/v5/market/instruments-info", 200, &instruments);
    let tickers = if variant.is_inverse() {
        support::TICKERS
            .replace("BTCUSDT", "BTCUSD")
            .replace("ETHUSDT", "ETHUSD")
    } else {
        support::TICKERS.to_owned()
    };
    h.respond("/v5/market/tickers", 200, &tickers);
    // Managed public feeds accept no traffic on an unopened socket.
    let public: Vec<_> = h
        .sockets
        .iter()
        .filter(|(_, url)| url.contains("/v5/public"))
        .map(|(socket, _)| *socket)
        .collect();
    for socket in public {
        h.event(Event::WsOpened { socket });
    }
    if variant.is_hyro() {
        let socket = h.socket("/v5/private").unwrap();
        h.event(Event::WsOpened { socket });
        for op in ["auth", "subscribe"] {
            let frame = h
                .sent_on("/v5/private")
                .into_iter()
                .rev()
                .find(|m| m.contains(&format!("\"op\":\"{op}\"")))
                .unwrap();
            #[derive(serde::Deserialize)]
            struct Sent {
                req_id: String,
            }
            let frame: Sent = serde_json::from_str(&frame).unwrap();
            h.message(
                "/v5/private",
                &format!(
                    r#"{{"op":"{op}","success":true,"req_id":"{}"}}"#,
                    frame.req_id
                ),
            );
        }
        h.respond("/v5/order/realtime", 200, EMPTY);
        h.respond("/v5/position/list", 200, EMPTY);
        h.respond("/v5/account/wallet-balance", 200, WALLET);
        assert!(h.engine.store().loaded().orders);
    } else {
        // Managed sockets deliver nothing on an unopened socket; Bybit's own auth ACK then
        // marks the private session live, as in a real session.
        let socket = h.socket("/v5/private").unwrap();
        h.event(Event::WsOpened { socket });
        h.message(
            "/v5/private",
            r#"{"success":true,"ret_msg":"","op":"auth","conn_id":"cejreaspqfh3sjdnldmg-p"}"#,
        );
    }
    if variant == Variant::Demo {
        while h.has_request("/v5/order/realtime") {
            h.respond("/v5/order/realtime", 200, EMPTY);
        }
        while h.has_request("/v5/position/list") {
            h.respond("/v5/position/list", 200, EMPTY);
        }
        h.respond("/v5/account/wallet-balance", 200, WALLET);
    }
    h.requests.clear();
    h
}

fn order(symbol: &str, amount: f64) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: symbol.into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount,
        price: Some(100.0),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        close: None,
        stp_mode: None,
        implied_volatility: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
    }
}
fn place(h: &mut Harness, command: u32, orders: Vec<PlaceOrderOpts>) {
    h.command(Command::PlaceOrders { command, orders });
}
fn body(request: &HttpRequest) -> &str {
    request.body.as_deref().unwrap()
}
/// D2: an absent-order answer is no Cancelled transition; the engine's one
/// reconcile order read decides. Answer it (empty: the rows are gone).
fn answer_reconcile(h: &mut Harness) {
    assert!(h.has_request("/v5/order/realtime"), "one reconcile read");
    while h.has_request("/v5/order/realtime") {
        h.respond("/v5/order/realtime", 200, EMPTY);
    }
}

fn resolve_mode(h: &mut Harness, symbol: &str) {
    h.respond(
        "/v5/position/list",
        200,
        &format!(
            r#"{{"retCode":0,"result":{{"list":[{{"symbol":"{symbol}","positionIdx":0}}]}}}}"#
        ),
    );
}
fn created(h: &mut Harness, id: &str) {
    h.respond(
        "/v5/order/create",
        200,
        &format!(r#"{{"retCode":0,"retMsg":"OK","result":{{"orderId":"{id}"}}}}"#),
    );
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SentCreate {
    order_link_id: String,
}
fn link(request: &HttpRequest) -> String {
    serde_json::from_str::<SentCreate>(body(request))
        .unwrap()
        .order_link_id
}
fn lookup_row(link: &str, status: &str) -> String {
    format!(
        r#"{{"retCode":0,"result":{{"list":[{{"symbol":"BTCUSDT","orderId":"recovered","orderLinkId":"{link}","orderStatus":"{status}","orderType":"Limit","side":"Buy","price":"101","qty":"1","cumExecQty":"0.4","leavesQty":"0.6"}}],"nextPageCursor":""}}}}"#
    )
}

/// H2: a Hyro link read (realtime then history, filtered by orderLinkId)
/// answered as a complete absence on both axes.
fn answer_link_absent(h: &mut Harness, link: &str) {
    let filter = format!("orderLinkId={link}");
    assert!(h.has_request(&filter), "a link-filtered read");
    while h.has_request(&filter) {
        h.respond(&filter, 200, EMPTY);
    }
}
fn outcome(h: &Harness, command: u32) -> Option<safe_cex_engine::Outcome> {
    h.outcomes
        .iter()
        .find(|(id, _)| *id == command)
        .and_then(|(_, outcome)| *outcome)
}

#[test]
fn inverse_create_keeps_native_integer_quantity_and_exact_split_total() {
    let mut h = ready(Variant::Inverse);
    place(&mut h, 1, vec![order("BTCUSD", 25.6)]);
    let mode = h.take("/v5/position/list");
    assert!(mode.url.contains("category=inverse&symbol=BTCUSD"));
    h.reply(
        mode.id,
        200,
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSD","positionIdx":0}]}}"#,
    );
    let request = h.take("/v5/order/create-batch");
    assert!(body(&request).contains("\"category\":\"inverse\""));
    assert_eq!(body(&request).matches("\"qty\":\"10\"").count(), 2);
    assert_eq!(body(&request).matches("\"qty\":\"6\"").count(), 1);
}

#[test]
fn spot_market_buys_use_base_amount_and_single_endpoint() {
    let mut h = ready(Variant::Spot);
    let mut opts = order("BTCUSDT", 0.1);
    opts.kind = OrderType::Market;
    opts.price = None;
    place(&mut h, 2, vec![opts]);
    let request = h.take("/v5/order/create");
    assert!(body(&request).contains("\"category\":\"spot\""));
    assert!(body(&request).contains("\"marketUnit\":\"baseCoin\""));
    for key in [
        "positionIdx",
        "reduceOnly",
        "closeOnTrigger",
        "triggerDirection",
        "triggerBy",
    ] {
        assert!(!body(&request).contains(key));
    }
    assert!(!h.has_request("/v5/position/list"));
    assert!(!h.has_request("/v5/order/create-batch"));
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"spot"}}"#,
    );
    assert_eq!(h.result(2), Some(&Ok("[\"spot\"]".into())));
}

#[test]
fn spot_stops_use_native_filter_and_derivative_controls_fail_before_writes() {
    let mut h = ready(Variant::Spot);
    let mut opts = order("BTCUSDT", 0.1);
    opts.kind = OrderType::StopLoss;
    place(&mut h, 1, vec![opts.clone()]);
    let request = h.take("/v5/order/create");
    assert!(body(&request).contains("\"orderFilter\":\"tpslOrder\""));
    assert!(body(&request).contains("\"triggerPrice\":\"100\""));
    assert!(!h.has_request("trading-stop"));
    opts.kind = OrderType::TrailingStopLoss;
    place(&mut h, 2, vec![opts]);
    assert!(h.result(2).unwrap().is_err());
    h.command(Command::SetPositionMode {
        command: 3,
        hedged: true,
        symbol: None,
    });
    assert!(h.result(3).unwrap().is_err());
    h.command(Command::SetLeverage {
        command: 4,
        symbol: "BTCUSDT".into(),
        leverage: 3.0,
        side: None,
    });
    assert!(h.result(4).unwrap().is_err());
    assert!(h.requests.is_empty());
}

fn assert_missing_stop_trigger_rejects_batch(variant: Variant, kind: OrderType) {
    let mut h = ready(variant);
    let valid = order("BTCUSDT", 0.1);
    let mut invalid = valid.clone();
    invalid.kind = kind;
    invalid.price = None;
    place(&mut h, 1, vec![valid, invalid]);
    assert!(
        h.requests.is_empty(),
        "invalid stop must cause no HTTP: {:?}",
        h.paths()
    );
    assert!(h.result(1).is_some_and(Result::is_err));
}

#[test]
fn spot_stop_loss_without_trigger_rejects_whole_batch_before_io() {
    assert_missing_stop_trigger_rejects_batch(Variant::Spot, OrderType::StopLoss);
}

#[test]
fn spot_take_profit_without_trigger_rejects_whole_batch_before_io() {
    assert_missing_stop_trigger_rejects_batch(Variant::Spot, OrderType::TakeProfit);
}

#[test]
fn flat_linear_stop_loss_without_trigger_rejects_whole_batch_before_io() {
    assert_missing_stop_trigger_rejects_batch(Variant::Linear, OrderType::StopLoss);
}

#[test]
fn flat_linear_take_profit_without_trigger_rejects_whole_batch_before_io() {
    assert_missing_stop_trigger_rejects_batch(Variant::Linear, OrderType::TakeProfit);
}

#[test]
fn standalone_stops_require_finite_positive_rounded_triggers() {
    for variant in [Variant::Spot, Variant::Linear] {
        for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
            for explicit in [false, true] {
                for trigger in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0, 0.001] {
                    let mut h = ready(variant);
                    let mut invalid = order("BTCUSDT", 0.1);
                    invalid.kind = kind;
                    invalid.price = (!explicit).then_some(trigger);
                    invalid.trigger_price = explicit.then_some(trigger);
                    place(&mut h, 1, vec![order("BTCUSDT", 0.1), invalid]);
                    assert!(
                        h.requests.is_empty(),
                        "{variant:?} {kind:?} trigger={trigger}"
                    );
                    assert!(h.result(1).is_some_and(Result::is_err));
                }
            }
        }
    }
}

#[test]
fn standalone_stops_accept_explicit_trigger_or_price_fallback() {
    for variant in [Variant::Spot, Variant::Linear] {
        for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
            for explicit in [false, true] {
                let mut h = ready(variant);
                let mut opts = order("BTCUSDT", 0.1);
                opts.kind = kind;
                opts.price = (!explicit).then_some(100.0);
                opts.trigger_price = explicit.then_some(100.0);
                place(&mut h, 1, vec![opts]);
                if !variant.is_spot() {
                    resolve_mode(&mut h, "BTCUSDT");
                }
                let request = h.take("/v5/order/create");
                assert!(body(&request).contains("\"triggerPrice\":\"100\""));
                if variant.is_spot() {
                    assert!(body(&request).contains("\"orderFilter\":\"tpslOrder\""));
                }
            }
        }
    }
}

#[test]
fn explicit_native_execution_controls_use_singles_and_are_validated() {
    let mut h = ready(Variant::Inverse);
    let mut opts = order("BTCUSD", 1.0);
    opts.kind = OrderType::Market;
    opts.price = None;
    opts.max_slippage_pct = Some(0.25);
    place(&mut h, 1, vec![opts]);
    resolve_mode(&mut h, "BTCUSD");
    let request = h.take("/v5/order/create");
    assert!(body(&request).contains("\"slippageToleranceType\":\"Percent\""));
    assert!(body(&request).contains("\"slippageTolerance\":\"0.25\""));
    let mut bbo = order("BTCUSD", 1.0);
    bbo.price = None;
    bbo.bybit_bbo = Some(BybitBbo {
        side_type: "Queue".into(),
        level: 3.0,
    });
    place(&mut h, 2, vec![bbo.clone()]);
    let request = h.take("/v5/order/create");
    assert!(body(&request).contains("\"bboSideType\":\"Queue\""));
    assert!(!body(&request).contains("\"price\""));
    bbo.bybit_bbo.as_mut().unwrap().level = 6.0;
    place(&mut h, 3, vec![bbo]);
    assert!(h.result(3).unwrap().is_err());
    assert!(h.requests.is_empty());
}

#[test]
fn variant_history_queries_keep_category_and_inverse_quote_size() {
    let mut h = ready(Variant::Inverse);
    h.command(Command::Query {
        command: 1,
        query: Query::PublicTrades {
            symbol: "BTCUSD".into(),
            limit: Some(5),
        },
    });
    let request = h.take("/v5/market/recent-trade");
    assert!(request.url.contains("category=inverse"));
    h.reply(request.id, 200, r#"{"retCode":0,"result":{"list":[{"execId":"trade","price":"100","size":"25","side":"Buy","time":"1700000000000"}]}}"#);
    assert!(
        h.result(1)
            .unwrap()
            .as_ref()
            .unwrap()
            .contains("\"quoteQty\":25.0")
    );
    let mut spot = ready(Variant::Spot);
    spot.command(Command::Query {
        command: 2,
        query: Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    });
    assert!(spot.result(2).unwrap().is_err());
    assert!(spot.requests.is_empty());
    spot.command(Command::Query {
        command: 3,
        query: Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(1000),
        },
    });
    assert!(spot.take("recent-trade").url.contains("limit=60"));
}

#[test]
fn demo_private_creates_use_demo_origin() {
    let mut h = ready(Variant::Demo);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    resolve_mode(&mut h, "BTCUSDT");
    assert!(
        h.take("create-batch")
            .url
            .starts_with("https://api-demo.bybit.com/")
    );
    assert!(
        h.socket("wss://stream.bybit.com/v5/public/linear")
            .is_some()
    );
}

#[test]
fn hyro_lost_ack_recovers_terminal_identity_without_recreating() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost acknowledgement",
    });
    // H2 (Q3): Unknown at once; the reconcile link read recovers it for the retry.
    assert_eq!(outcome(&h, 1), Some(safe_cex_engine::Outcome::Unknown));
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/order/history", 200, &lookup_row(&id, "Filled"));
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    assert_eq!(h.result(2), Some(&Ok("[\"recovered\"]".into())));
    assert!(h.engine.store().order("recovered").is_none());
    assert!(!h.has_request("/v5/order/create") && !h.has_request("orderLinkId="));
}

#[test]
fn hyro_partial_recovery_publishes_authoritative_quantity_and_price() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.reply(
        create.id,
        409,
        r#"{"retCode":110072,"retMsg":"duplicate orderLinkId"}"#,
    );
    h.respond(
        "/v5/order/realtime",
        200,
        &lookup_row(&id, "PartiallyFilled"),
    );
    let row = h.engine.store().order("recovered").unwrap();
    assert_eq!((row.price, row.filled, row.remaining), (101.0, 0.4, 0.6));
    // H2 (Q7): only the reconcile snapshot remains beside the link read.
    assert!(!h.has_request("/v5/order/create") && !h.has_request("orderLinkId="));
}

#[test]
fn hyro_unknown_lookup_keeps_original_identity_and_accepted_siblings() {
    let mut h = ready(Variant::Hyrotrader);
    let orders = vec![order("BTCUSDT", 1.0), order("ETHUSDT", 2.0)];
    place(&mut h, 1, orders.clone());
    created(&mut h, "first");
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    // H2: the lost create seals Unknown, keeping its link and the accepted sibling.
    assert_eq!(outcome(&h, 1), Some(safe_cex_engine::Outcome::Unknown));
    assert!(h.result(1).unwrap().as_ref().unwrap_err().contains(&id));
    // Its reconcile read is incomplete, so the identity stays unresolved.
    h.respond(
        &format!("orderLinkId={id}"),
        500,
        r#"{"retCode":10016,"retMsg":"unavailable"}"#,
    );
    assert!(!h.has_request("/v5/order/create"));
    // The caller's retry re-reads first; proven absent, it sends the same link.
    place(&mut h, 2, orders);
    assert!(!h.has_request("/v5/order/create"));
    answer_link_absent(&mut h, &id);
    let retry = h.take("/v5/order/create");
    assert_eq!(link(&retry), id);
    assert!(body(&retry).contains("ETHUSDT"));
    h.reply(
        retry.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"second"}}"#,
    );
    assert_eq!(h.result(2), Some(&Ok("[\"first\",\"second\"]".into())));
}

#[test]
fn hyro_identical_concurrent_submissions_share_one_dispatch() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    assert_eq!(h.requests.len(), 1);
    created(&mut h, "first");
    assert_eq!(h.result(1), h.result(2));
    place(&mut h, 3, vec![order("BTCUSDT", 1.0)]);
    assert!(h.has_request("/v5/order/create"));
}

#[test]
fn hyro_malformed_or_cyclic_lookup_never_authorizes_replacement() {
    for malformed in [
        r#"{"retCode":0,"result":{}}"#,
        r#"{"retCode":0,"result":{"list":[],"nextPageCursor":9}}"#,
    ] {
        let mut h = ready(Variant::Hyrotrader);
        place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
        let create = h.take("/v5/order/create");
        h.event(Event::HttpFailed {
            request: create.id,
            error: "lost",
        });
        h.respond("/v5/order/realtime", 200, malformed);
        assert!(h.result(1).unwrap().is_err());
        // H2 (Q7): only the reconcile snapshot remains beside the link read.
        assert!(!h.has_request("/v5/order/create") && !h.has_request("orderLinkId="));
    }
}

#[test]
fn hyro_caller_ids_and_client_protection_fail_before_dispatch() {
    let mut h = ready(Variant::Hyrotrader);
    let mut opts = order("BTCUSDT", 1.0);
    opts.client_order_id = Some("caller".into());
    place(&mut h, 1, vec![opts.clone()]);
    assert!(h.result(1).unwrap().is_err());
    opts.client_order_id = None;
    opts.kind = OrderType::Market;
    opts.price = None;
    opts.max_slippage_pct = Some(0.5);
    place(&mut h, 2, vec![opts]);
    assert!(h.result(2).unwrap().is_err());
    assert!(h.requests.is_empty());
}

fn finish_hyro_snapshot(h: &mut Harness) {
    h.respond(
        "/v5/order/realtime?category=linear&settleCoin=USDT",
        200,
        EMPTY,
    );
    h.respond(
        "/v5/position/list?category=linear&settleCoin=USDT",
        200,
        EMPTY,
    );
    h.respond("/v5/account/wallet-balance", 200, WALLET);
}

#[test]
fn hyro_ack_crossing_snapshot_is_reconciled_without_optimistic_resurrection() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    h.advance(5000.0);
    finish_hyro_snapshot(&mut h);
    h.reply(
        create.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"accepted"}}"#,
    );
    // H2 (Q6): the ACK proves acceptance; no confirmation lookup and no
    // optimistic row across the moved snapshot.
    assert_eq!(h.result(1), Some(&Ok("[\"accepted\"]".into())));
    assert!(h.engine.store().order("accepted").is_none());
    assert!(!h.paths().iter().any(|path| path.contains("orderLinkId=")));
}

#[test]
fn hyro_lookup_crossing_snapshot_keeps_discovered_identity_for_caller_retry() {
    let mut h = ready(Variant::Hyrotrader);
    let opts = order("BTCUSDT", 1.0);
    place(&mut h, 1, vec![opts.clone()]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    let lookup = h.take("/v5/order/realtime");
    h.advance(5000.0);
    finish_hyro_snapshot(&mut h);
    h.reply(lookup.id, 200, &lookup_row(&id, "New"));
    assert!(h.result(1).unwrap().is_err());
    assert!(h.engine.store().order("recovered").is_none());
    // H2 (Q3): the reconcile read already found it; the retry reads nothing.
    place(&mut h, 2, vec![opts]);
    assert_eq!(h.result(2), Some(&Ok("[\"recovered\"]".into())));
    assert!(h.requests.is_empty());
    assert!(h.engine.store().order("recovered").is_none());
}

#[test]
fn hyro_private_terminal_observation_blocks_stale_recovered_open_row() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    // H2 (Q3): Unknown at once. The terminal observation lands while the link
    // read is out; that read's stale open row must not resurrect the order.
    assert_eq!(outcome(&h, 1), Some(safe_cex_engine::Outcome::Unknown));
    let read = h.take(&format!("orderLinkId={id}"));
    h.message("/v5/private", &format!(r#"{{"topic":"order","data":[{{"category":"linear","symbol":"BTCUSDT","orderId":"recovered","orderLinkId":"{id}","orderStatus":"Filled","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"1","leavesQty":"0"}}]}}"#));
    h.reply(read.id, 200, &lookup_row(&id, "New"));
    assert_eq!(
        h.engine.store().order("recovered").unwrap().status,
        safe_cex_engine::model::OrderStatus::Closed
    );
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    assert_eq!(h.result(2), Some(&Ok("[\"recovered\"]".into())));
    assert!(!h.has_request("/v5/order/create"));
}

#[test]
fn hyro_complete_absence_never_resends_in_the_same_call() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    answer_link_absent(&mut h, &id);
    // R1(a): no automatic same-link resend; the call stays Unknown with its link.
    assert!(!h.has_request("/v5/order/create"));
    assert_eq!(outcome(&h, 1), Some(safe_cex_engine::Outcome::Unknown));
    assert!(h.result(1).unwrap().as_ref().unwrap_err().contains(&id));
}

#[test]
fn hyro_cyclic_lookup_is_incomplete_and_keeps_unknown_outcome() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    // H2 (Q7): a link-filtered page that still names another page is no absence.
    let page = r#"{"retCode":0,"result":{"list":[],"nextPageCursor":"same"}}"#;
    h.respond(&format!("orderLinkId={id}"), 200, page);
    assert!(h.result(1).unwrap().is_err());
    assert_eq!(outcome(&h, 1), Some(safe_cex_engine::Outcome::Unknown));
    assert!(!h.has_request("/v5/order/create") && !h.has_request("orderLinkId="));
}

#[test]
fn hyro_definitive_rejection_permits_new_submission_identity() {
    let mut h = ready(Variant::Hyrotrader);
    let opts = order("BTCUSDT", 1.0);
    place(&mut h, 1, vec![opts.clone()]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.reply(
        create.id,
        200,
        r#"{"retCode":110007,"retMsg":"insufficient balance"}"#,
    );
    assert!(h.result(1).unwrap().is_err());
    place(&mut h, 2, vec![opts]);
    assert_ne!(link(&h.take("/v5/order/create")), id);
}

#[test]
fn spot_trigger_cancel_uses_its_native_order_family() {
    let mut h = ready(Variant::Spot);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"spot","symbol":"BTCUSDT","orderId":"stop","orderStatus":"Untriggered","orderType":"Market","stopOrderType":"tpslOrder","side":"Sell","price":"0","triggerPrice":"99","qty":"0.1","cumExecQty":"0","leavesQty":"0.1"}]}"#);
    assert!(h.engine.store().order("stop").is_some());
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["stop".into()],
    });
    let request = h.take("/v5/order/cancel");
    assert!(body(&request).contains("\"orderFilter\":\"tpslOrder\""));
    assert!(!h.has_request("cancel-batch"));
}

#[test]
fn demo_cancel_all_retires_listed_parent_projections_and_old_terminal_rows() {
    let mut h = ready(Variant::Demo);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"old-filled","orderStatus":"Filled","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"1","leavesQty":"0"},{"category":"linear","symbol":"BTCUSDT","orderId":"resting","orderStatus":"New","orderType":"Limit","side":"Buy","price":"99","qty":"1","cumExecQty":"0","leavesQty":"1"}]}"#);
    assert_eq!(h.engine.store().orders().len(), 2);
    let parent = h.engine.store().order("resting").unwrap().clone();
    for suffix in ["__stop_loss", "__take_profit"] {
        let mut projection = parent.clone();
        projection.id.push_str(suffix);
        projection.parent_id = Some(parent.id.clone());
        h.engine.store_mut().upsert_order(projection);
    }
    h.command(Command::CancelSymbolOrders {
        command: 1,
        symbol: "BTCUSDT".into(),
    });
    h.respond(
        "/v5/order/cancel-all",
        200,
        r#"{"retCode":0,"result":{"list":[{"orderId":"resting","orderLinkId":""}],"success":"1"}}"#,
    );
    assert!(h.engine.store().orders().is_empty());
    assert_eq!(h.result(1), Some(&Ok("[]".into())));
    assert!(h.requests.is_empty());
}

// Futures cancel-all selects at most 500 orders; the per-symbol placement
// limits separately allow 500 active and 10 conditional orders.
// https://bybit-exchange.github.io/docs/v5/order/cancel-all
// https://bybit-exchange.github.io/docs/v5/order/create-order
#[test]
fn demo_capped_cancel_all_preserves_omitted_live_order_and_reports_partial_failure() {
    let mut h = ready(Variant::Demo);
    let survivor = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"survivor","orderStatus":"Untriggered","orderType":"Limit","stopOrderType":"Stop","triggerPrice":"70000","triggerDirection":1,"side":"Buy","price":"70000","qty":"1","cumExecQty":"0","leavesQty":"1"}"#;
    h.message(
        "/v5/private",
        &format!(r#"{{"topic":"order","data":[{survivor}]}}"#),
    );
    let seed = h.engine.store().order("survivor").unwrap().clone();
    for index in 0..500 {
        let mut active = seed.clone();
        active.id = format!("active-{index}");
        active.kind = OrderType::Limit;
        h.engine.store_mut().upsert_order(active);
    }
    assert_eq!(h.engine.store().orders().len(), 501);
    h.command(Command::CancelSymbolOrders {
        command: 91,
        symbol: "BTCUSDT".into(),
    });
    let affected = (0..500)
        .map(|index| format!(r#"{{"orderId":"active-{index}","orderLinkId":""}}"#))
        .collect::<Vec<_>>()
        .join(",");
    h.respond(
        "/v5/order/cancel-all",
        200,
        &format!(r#"{{"retCode":0,"result":{{"list":[{affected}],"success":"1"}}}}"#),
    );
    assert!(
        h.engine.store().order("survivor").is_some(),
        "omitted order must remain visible"
    );
    assert_eq!(h.engine.store().orders().len(), 1);

    // The proof read, issued after the ACK, must commit the omitted row and
    // unrelated arrivals while still filtering a replica-stale listed order.
    assert!(
        h.requests
            .iter()
            .all(|r| r.url.contains("/v5/order/realtime"))
    );
    let unrelated = survivor.replace("survivor", "unrelated-live");
    let stale = survivor.replace("survivor", "active-0");
    h.respond("settleCoin=USDT&limit=50", 200, &format!(r#"{{"retCode":0,"result":{{"list":[{survivor},{unrelated},{stale}],"nextPageCursor":""}}}}"#));
    h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
    h.advance(0.0);
    for id in ["survivor", "unrelated-live"] {
        assert_eq!(
            h.engine.store().order(id).unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
    }
    assert!(h.engine.store().order("active-0").is_none());
    assert!(
        h.result(91)
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("1 live order(s) on BTCUSDT: survivor")
    );
}

#[test]
fn demo_cancel_all_without_affected_identity_cannot_retire_or_tombstone_live_rows() {
    for ack in [
        r#"{"retCode":0,"result":{}}"#,
        r#"{"retCode":0,"result":{"success":"1"}}"#,
        r#"{"retCode":0,"result":{"list":[],"success":"1"}}"#,
        r#"{"retCode":0,"result":{"list":{}}}"#,
        r#"{"retCode":0,"result":{"list":null}}"#,
        r#"{"retCode":0,"result":null}"#,
        r#"{"retCode":0,"result":{"list":[{"orderLinkId":"client"}]}}"#,
        r#"{"retCode":0,"result":{"list":[{"orderId":""}]}}"#,
        r#"{"retCode":0,"result":{"list":[{"orderId":"   "}]}}"#,
        r#"{"retCode":0,"result":{"list":[{"orderId":"live"},{}]}}"#,
        r#"{"retCode":0,"result":{"list":[{"orderId":"live__stop_loss"}]}}"#,
        r#"{"retCode":0,"result":{"list":[{"orderId":"live"}],"success":"0"}}"#,
        r#"{"retCode":110001,"retMsg":"order not exists or too late to cancel"}"#,
    ] {
        let mut h = ready(Variant::Demo);
        let row = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"live","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}"#;
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"order","data":[{row}]}}"#),
        );
        h.command(Command::CancelSymbolOrders {
            command: 92,
            symbol: "BTCUSDT".into(),
        });
        h.respond("/v5/order/cancel-all", 200, ack);
        assert!(h.engine.store().order("live").is_some(), "{ack}");
        // The reconcile read keeps the live row, and the command fails.
        assert!(
            h.requests
                .iter()
                .all(|r| r.url.contains("/v5/order/realtime")),
            "{ack}"
        );
        h.respond(
            "settleCoin=USDT&limit=50",
            200,
            &format!(r#"{{"retCode":0,"result":{{"list":[{row}],"nextPageCursor":""}}}}"#),
        );
        h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
        h.advance(0.0);
        assert_eq!(
            h.engine.store().order("live").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open,
            "{ack}"
        );
        assert!(h.result(92).unwrap().is_err(), "{ack}");
        assert!(!h.has_request("/v5/order/realtime"), "{ack}");
    }
}

#[test]
fn demo_cancel_all_accepts_documented_empty_list_with_no_live_targets() {
    let mut h = ready(Variant::Demo);
    h.command(Command::CancelSymbolOrders {
        command: 93,
        symbol: "BTCUSDT".into(),
    });
    h.respond(
        "/v5/order/cancel-all",
        200,
        r#"{"retCode":0,"result":{"list":[],"success":"1"}}"#,
    );
    assert_eq!(h.result(93), Some(&Ok("[]".into())));
}

#[test]
fn demo_terminal_cleanup_does_not_prove_cancellation_of_omitted_native_ids() {
    let mut h = ready(Variant::Demo);
    let row = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"live","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}"#;
    h.message(
        "/v5/private",
        &format!(r#"{{"topic":"order","data":[{row}]}}"#),
    );
    // Local terminal rows have no native terminal event or cancellation proof.
    let seed = h.engine.store().order("live").unwrap().clone();
    for (id, status) in [
        ("old-filled", safe_cex_engine::model::OrderStatus::Closed),
        (
            "old-canceled",
            safe_cex_engine::model::OrderStatus::Canceled,
        ),
    ] {
        let mut terminal = seed.clone();
        terminal.id = id.into();
        terminal.status = status;
        h.engine.store_mut().upsert_order(terminal);
    }
    h.command(Command::CancelSymbolOrders {
        command: 97,
        symbol: "BTCUSDT".into(),
    });
    // An unrelated terminal arrival was not captured by this command.
    let mut arrival = seed;
    arrival.id = "later-terminal".into();
    arrival.status = safe_cex_engine::model::OrderStatus::Closed;
    h.engine.store_mut().upsert_order(arrival);
    h.respond(
        "/v5/order/cancel-all",
        200,
        r#"{"retCode":0,"result":{"list":[],"success":"1"}}"#,
    );
    for id in ["old-filled", "old-canceled"] {
        assert!(h.engine.store().order(id).is_none());
    }
    assert!(h.engine.store().order("later-terminal").is_some());
    assert!(h.engine.store().order("live").is_some());
    h.message("/v5/private", r#"{"op":"auth","success":true}"#);
    h.message("/v5/private", r#"{"op":"subscribe","success":true}"#);
    let filled = row.replace("live", "old-filled");
    let canceled = row.replace("live", "old-canceled");
    h.respond(
        "settleCoin=USDT&limit=50",
        200,
        &format!(
            r#"{{"retCode":0,"result":{{"list":[{row},{filled},{canceled}],"nextPageCursor":""}}}}"#
        ),
    );
    h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
    for id in ["live", "old-filled", "old-canceled"] {
        assert_eq!(
            h.engine.store().order(id).unwrap().status,
            safe_cex_engine::model::OrderStatus::Open,
            "terminal cleanup must not invent native cancellation proof for {id}"
        );
    }
    // The reconnect outdated that read, so its follow-up is the proof.
    h.respond(
        "settleCoin=USDT&limit=50",
        200,
        &format!(
            r#"{{"retCode":0,"result":{{"list":[{row},{filled},{canceled}],"nextPageCursor":""}}}}"#
        ),
    );
    h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
    h.advance(0.0);
    assert!(h.result(97).unwrap().is_err());
}

#[test]
fn demo_cancel_all_fences_listed_native_ids_absent_from_captured_store() {
    for arrived_during_request in [false, true] {
        let mut h = ready(Variant::Demo);
        let row = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"external","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}"#;
        h.command(Command::CancelSymbolOrders {
            command: 94,
            symbol: "BTCUSDT".into(),
        });
        if arrived_during_request {
            h.message(
                "/v5/private",
                &format!(r#"{{"topic":"order","data":[{row}]}}"#),
            );
        }
        h.respond("/v5/order/cancel-all", 200, r#"{"retCode":0,"result":{"list":[{"orderId":"external","orderLinkId":""},{"orderId":"external","orderLinkId":""}],"success":"1"}}"#);
        assert!(h.engine.store().order("external").is_none());
        assert_eq!(h.result(94), Some(&Ok("[]".into())));
        h.message("/v5/private", r#"{"op":"auth","success":true}"#);
        h.message("/v5/private", r#"{"op":"subscribe","success":true}"#);
        let other = row.replace("external", "unrelated");
        h.respond(
            "settleCoin=USDT&limit=50",
            200,
            &format!(r#"{{"retCode":0,"result":{{"list":[{row},{other}],"nextPageCursor":""}}}}"#),
        );
        h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
        assert!(h.engine.store().order("external").is_none());
        assert_eq!(
            h.engine.store().order("unrelated").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
        assert!(!h.has_request("/v5/order/realtime"));
    }
}

#[test]
fn cancel_all_omitted_targets_that_finished_during_request_are_not_partial_failures() {
    for status in ["Filled", "Cancelled"] {
        let mut h = ready(Variant::Demo);
        let row = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"finished","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}"#;
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"order","data":[{row}]}}"#),
        );
        h.command(Command::CancelSymbolOrders {
            command: 95,
            symbol: "BTCUSDT".into(),
        });
        let terminal = row
            .replace("New", status)
            .replace(r#""leavesQty":"1""#, r#""leavesQty":"0""#);
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"order","data":[{terminal}]}}"#),
        );
        h.respond(
            "/v5/order/cancel-all",
            200,
            r#"{"retCode":0,"result":{"list":[],"success":"1"}}"#,
        );
        assert_eq!(h.result(95), Some(&Ok("[]".into())));
        assert!(h.engine.store().order("finished").is_none());
    }
}

const STOP_ROW: &str = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"stop-1","orderStatus":"Untriggered","orderType":"Market","stopOrderType":"Stop","triggerPrice":"70000","triggerDirection":2,"side":"Sell","price":"0","qty":"1","cumExecQty":"0","leavesQty":"1","reduceOnly":true}"#;

/// A stop the stream still shows, omitted from an empty cancel-all answer: the
/// venue dropped it with the position, or it survived. Only a read can tell.
fn cancel_all_omitting_a_stop(command: u32) -> Harness {
    let mut h = ready(Variant::Demo);
    h.message(
        "/v5/private",
        &format!(r#"{{"topic":"order","data":[{STOP_ROW}]}}"#),
    );
    h.command(Command::CancelSymbolOrders {
        command,
        symbol: "BTCUSDT".into(),
    });
    h.respond(
        "/v5/order/cancel-all",
        200,
        r#"{"retCode":0,"result":{"list":[],"success":"1"}}"#,
    );
    assert_eq!(h.result(command), None, "no verdict before the venue read");
    h
}

#[test]
fn cancel_all_omitting_a_stop_the_venue_already_removed_succeeds_after_the_read() {
    let mut h = cancel_all_omitting_a_stop(96);
    h.respond("settleCoin=USDT&limit=50", 200, EMPTY);
    h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
    h.advance(0.0);
    assert_eq!(h.result(96), Some(&Ok("[]".into())));
    assert!(h.engine.store().order("stop-1").is_none());
}

#[test]
fn cancel_all_omitting_a_stop_the_venue_still_lists_names_it() {
    let mut h = cancel_all_omitting_a_stop(97);
    h.respond(
        "settleCoin=USDT&limit=50",
        200,
        &format!(r#"{{"retCode":0,"result":{{"list":[{STOP_ROW}],"nextPageCursor":""}}}}"#),
    );
    h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
    h.advance(0.0);
    let error = h.result(97).unwrap().as_ref().unwrap_err();
    assert!(
        error.contains("1 live order(s) on BTCUSDT: stop-1"),
        "{error}"
    );
}

#[test]
fn cancel_all_whose_read_never_answers_still_settles() {
    let mut h = cancel_all_omitting_a_stop(99);
    h.advance(120_000.0);
    assert!(h.result(99).is_some_and(Result::is_err));
}

#[test]
fn cancel_all_whose_read_fails_says_the_order_is_unconfirmed() {
    let mut h = cancel_all_omitting_a_stop(98);
    h.respond(
        "settleCoin=USDT&limit=50",
        200,
        r#"{"retCode":0,"result":{}}"#,
    );
    h.advance(0.0);
    let error = h.result(98).unwrap().as_ref().unwrap_err();
    assert!(
        error.contains("could not confirm 1 order(s) on BTCUSDT: stop-1"),
        "{error}"
    );
}

#[test]
fn inverse_cancel_all_without_success_field_uses_only_documented_affected_ids() {
    let mut h = ready(Variant::Inverse);
    h.message("/v5/private", r#"{"topic":"order.inverse","data":[{"category":"inverse","symbol":"BTCUSD","orderId":"listed","orderStatus":"New","orderType":"Limit","side":"Buy","price":"50000","qty":"1000","cumExecQty":"0","leavesQty":"1000"},{"category":"inverse","symbol":"BTCUSD","orderId":"omitted","orderStatus":"New","orderType":"Limit","side":"Buy","price":"50000","qty":"1000","cumExecQty":"0","leavesQty":"1000"}]}"#);
    h.command(Command::CancelSymbolOrders {
        command: 96,
        symbol: "BTCUSD".into(),
    });
    // UTA1.0 inverse responses do not contain success.
    h.respond(
        "/v5/order/cancel-all",
        200,
        r#"{"retCode":0,"result":{"list":[{"orderId":"listed","orderLinkId":""}]}}"#,
    );
    assert_eq!(
        h.engine.store().order("listed").unwrap().status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
    assert_eq!(
        h.engine.store().order("omitted").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
    let omitted = r#"{"category":"inverse","symbol":"BTCUSD","orderId":"omitted","orderStatus":"New","orderType":"Limit","side":"Buy","price":"50000","qty":"1000","cumExecQty":"0","leavesQty":"1000"}"#;
    let pages: Vec<_> = h.requests.iter().map(|r| r.id).collect();
    for page in pages {
        h.reply(
            page,
            200,
            &format!(r#"{{"retCode":0,"result":{{"list":[{omitted}],"nextPageCursor":""}}}}"#),
        );
    }
    h.advance(0.0);
    assert!(h.result(96).unwrap().is_err());
}

#[test]
fn hyro_recovered_rows_validate_raw_numbers_and_exact_symbol_even_when_terminal() {
    for status in ["New", "Filled"] {
        for bad in ["bad-qty", "wrong-symbol", "wrong-category"] {
            let mut h = ready(Variant::Hyrotrader);
            place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
            let create = h.take("/v5/order/create");
            let id = link(&create);
            h.event(Event::HttpFailed {
                request: create.id,
                error: "lost",
            });
            let row = lookup_row(&id, status);
            let row = match bad {
                "bad-qty" => row.replace("\"qty\":\"1\"", "\"qty\":\"oops\""),
                "wrong-symbol" => row.replace("BTCUSDT", "ETHUSDT"),
                _ => row.replace("\"symbol\":", "\"category\":\"inverse\",\"symbol\":"),
            };
            h.respond("/v5/order/realtime", 200, &row);
            assert!(h.result(1).unwrap().is_err(), "{status} {bad}");
            // H2 (Q7): only the reconcile snapshot remains beside the link read.
            assert!(!h.has_request("/v5/order/create") && !h.has_request("orderLinkId="));
            assert!(h.engine.store().order("recovered").is_none());
        }
    }
}

#[test]
fn hyro_trailing_stop_noop_succeeds_and_definitive_rejection_allows_retry() {
    let mut h = ready(Variant::Hyrotrader);
    let mut opts = order("BTCUSDT", 0.0);
    opts.kind = OrderType::TrailingStopLoss;
    opts.price = Some(65000.0);
    place(&mut h, 1, vec![opts.clone()]);
    h.respond(
        "/v5/position/trading-stop",
        200,
        r#"{"retCode":34040,"retMsg":"not modified"}"#,
    );
    assert_eq!(h.result(1), Some(&Ok("[]".into())));
    place(&mut h, 2, vec![opts.clone()]);
    h.respond(
        "/v5/position/trading-stop",
        200,
        r#"{"retCode":110001,"retMsg":"position not exists"}"#,
    );
    assert!(h.result(2).unwrap().is_err());
    place(&mut h, 3, vec![opts]);
    assert!(h.has_request("/v5/position/trading-stop"));
    assert!(
        h.progress.is_empty(),
        "locally allocated links are not transmitted by position-stop writes"
    );
}

#[test]
fn demo_limit_ack_and_amend_publish_without_private_events_but_markets_do_not() {
    let mut h = ready(Variant::Demo);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    resolve_mode(&mut h, "BTCUSDT");
    h.respond("/v5/order/create-batch", 200, r#"{"retCode":0,"result":{"list":[{"orderId":"local"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    assert_eq!(h.engine.store().order("local").unwrap().price, 100.0);
    h.command(Command::AmendOrder {
        command: 2,
        id: "local".into(),
        price: Some(101.0),
        amount: Some(2.0),
    });
    h.respond(
        "/v5/order/amend",
        200,
        r#"{"retCode":0,"result":{"orderId":"local"}}"#,
    );
    let row = h.engine.store().order("local").unwrap();
    assert_eq!((row.price, row.amount, row.remaining), (101.0, 2.0, 2.0));
    let mut opts = order("BTCUSDT", 1.0);
    opts.kind = OrderType::Market;
    opts.price = None;
    place(&mut h, 3, vec![opts]);
    h.respond("/v5/order/create-batch", 200, r#"{"retCode":0,"result":{"list":[{"orderId":"market"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    assert!(h.engine.store().order("market").is_none());
}

#[test]
fn demo_late_amend_ack_cannot_overwrite_newer_private_fill() {
    let mut h = ready(Variant::Demo);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    resolve_mode(&mut h, "BTCUSDT");
    h.respond("/v5/order/create-batch", 200, r#"{"retCode":0,"result":{"list":[{"orderId":"local"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    h.command(Command::AmendOrder {
        command: 2,
        id: "local".into(),
        price: Some(101.0),
        amount: Some(2.0),
    });
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"local","orderStatus":"Filled","orderType":"Limit","side":"Buy","price":"102","qty":"1","cumExecQty":"1","leavesQty":"0"}]}"#);
    h.respond(
        "/v5/order/amend",
        200,
        r#"{"retCode":0,"result":{"orderId":"local"}}"#,
    );
    let row = h.engine.store().order("local").unwrap();
    assert_eq!((row.price, row.amount, row.remaining), (102.0, 1.0, 0.0));
}

#[test]
fn hyro_market_ack_does_not_invent_a_resting_open_order() {
    let mut h = ready(Variant::Hyrotrader);
    let mut opts = order("BTCUSDT", 1.0);
    opts.kind = OrderType::Market;
    opts.price = None;
    place(&mut h, 1, vec![opts]);
    created(&mut h, "market");
    assert_eq!(h.result(1), Some(&Ok("[\"market\"]".into())));
    assert!(h.engine.store().order("market").is_none());
}

#[test]
fn demo_late_create_ack_cannot_reopen_order_removed_by_newer_snapshot() {
    let mut h = ready(Variant::Demo);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    resolve_mode(&mut h, "BTCUSDT");
    let create = h.take("/v5/order/create-batch");
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"late","orderStatus":"Filled","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"1","leavesQty":"0"}]}"#);
    assert!(h.engine.store().order("late").is_some());
    h.now += 1.0;
    h.message("/v5/private", r#"{"op":"auth","success":true}"#);
    h.message("/v5/private", r#"{"op":"subscribe","success":true}"#);
    while h.has_request("/v5/order/realtime") {
        h.respond("/v5/order/realtime", 200, EMPTY);
    }
    assert!(h.engine.store().order("late").is_none());
    h.reply(create.id, 200, r#"{"retCode":0,"result":{"list":[{"orderId":"late"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    assert_eq!(h.result(1), Some(&Ok("[\"late\"]".into())));
    assert!(h.engine.store().order("late").is_none());
}

#[test]
fn demo_create_ack_from_closed_private_socket_cannot_publish_open_order() {
    let mut h = ready(Variant::Demo);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    resolve_mode(&mut h, "BTCUSDT");
    let create = h.take("/v5/order/create-batch");
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    h.reply(create.id, 200, r#"{"retCode":0,"result":{"list":[{"orderId":"late"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    assert_eq!(h.result(1), Some(&Ok("[\"late\"]".into())));
    assert!(h.engine.store().order("late").is_none());
}

#[test]
fn spot_cancel_all_covers_each_family_and_keeps_failed_family_open() {
    for (failed, success_zero) in [
        (None, false),
        (Some("Order"), false),
        (Some("StopOrder"), false),
        (Some("tpslOrder"), false),
        (Some("Order"), true),
        (Some("StopOrder"), true),
        (Some("tpslOrder"), true),
    ] {
        let mut h = ready(Variant::Spot);
        h.message("/v5/private", r#"{"topic":"order.spot","data":[{"category":"spot","symbol":"BTCUSDT","orderId":"ordinary","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1","stopLoss":"90"},{"category":"spot","symbol":"BTCUSDT","orderId":"conditional","orderStatus":"Untriggered","orderType":"Market","stopOrderType":"Stop","side":"Buy","triggerPrice":"101","qty":"1","cumExecQty":"0","leavesQty":"1"},{"category":"spot","symbol":"BTCUSDT","orderId":"stop","orderStatus":"Untriggered","orderType":"Market","stopOrderType":"tpslOrder","side":"Sell","triggerPrice":"90","qty":"1","cumExecQty":"0","leavesQty":"1"}]}"#);
        h.command(Command::CancelSymbolOrders {
            command: 1,
            symbol: "BTCUSDT".into(),
        });
        for filter in ["Order", "StopOrder", "tpslOrder"] {
            let request = h.take("/v5/order/cancel-all");
            assert!(body(&request).contains(&format!("\"orderFilter\":\"{filter}\"")));
            let reply = if failed == Some(filter) {
                if success_zero {
                    r#"{"retCode":0,"result":{"list":[],"success":"0"}}"#
                } else {
                    r#"{"retCode":10006,"retMsg":"Too many visits"}"#
                }
            } else {
                match filter {
                    "Order" => {
                        r#"{"retCode":0,"result":{"list":[{"orderId":"ordinary","orderLinkId":""}],"success":"1"}}"#
                    }
                    "StopOrder" => {
                        r#"{"retCode":0,"result":{"list":[{"orderId":"conditional","orderLinkId":""}],"success":"1"}}"#
                    }
                    _ => {
                        r#"{"retCode":0,"result":{"list":[{"orderId":"stop","orderLinkId":""}],"success":"1"}}"#
                    }
                }
            };
            h.reply(request.id, 200, reply);
        }
        for (id, filter) in [
            ("ordinary", "Order"),
            ("ordinary__stop_loss", "Order"),
            ("conditional", "StopOrder"),
            ("stop", "tpslOrder"),
        ] {
            assert_eq!(
                h.engine.store().order(id).unwrap().status,
                if failed == Some(filter) {
                    safe_cex_engine::model::OrderStatus::Open
                } else {
                    safe_cex_engine::model::OrderStatus::Canceled
                },
                "{id} after {failed:?} failed"
            );
        }
        assert_eq!(h.result(1).unwrap().is_err(), failed.is_some());
    }
}

#[test]
fn hyro_conditional_retry_preserves_identity_and_body_after_ticker_crosses_trigger() {
    let mut h = ready(Variant::Hyrotrader);
    let mut opts = order("BTCUSDT", 1.0);
    opts.kind = OrderType::Conditional;
    opts.trigger_price = Some(70000.0);
    place(&mut h, 1, vec![opts.clone()]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    let original = body(&create).to_owned();
    assert!(original.contains("\"triggerDirection\":\"1\""));
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    h.respond("/v5/order/realtime", 500, r#"{"retCode":10016}"#);
    h.message("/v5/public/linear", r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","lastPrice":"71000"}}"#);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 71000.0);
    place(&mut h, 2, vec![opts]);
    assert!(!h.has_request("/v5/order/create"));
    // H2 (Q7): the reconcile snapshot's order read is also out; answer the link.
    answer_link_absent(&mut h, &id);
    let retry = h.take("/v5/order/create");
    assert_eq!(body(&retry), original);
    h.reply(
        retry.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"same"}}"#,
    );
    assert_eq!(h.result(2), Some(&Ok("[\"same\"]".into())));
}

#[test]
fn hyro_unknown_trailing_stop_cannot_bypass_recovery_after_ticker_changes() {
    let mut h = ready(Variant::Hyrotrader);
    let mut opts = order("BTCUSDT", 0.0);
    opts.kind = OrderType::TrailingStopLoss;
    opts.price = Some(65000.0);
    place(&mut h, 1, vec![opts.clone()]);
    let stop = h.take("/v5/position/trading-stop");
    h.event(Event::HttpFailed {
        request: stop.id,
        error: "lost",
    });
    h.message("/v5/public/linear", r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","lastPrice":"71000"}}"#);
    place(&mut h, 2, vec![opts]);
    assert!(h.result(2).unwrap().is_err());
    assert!(!h.has_request("/v5/position/trading-stop"));
}

#[test]
fn hyro_invalid_nonfinite_option_cannot_alias_pending_intent() {
    let mut h = ready(Variant::Hyrotrader);
    let opts = order("BTCUSDT", 1.0);
    place(&mut h, 1, vec![opts.clone()]);
    let mut invalid = opts;
    invalid.stop_loss = Some(f64::NAN);
    place(&mut h, 2, vec![invalid]);
    assert!(h.result(2).unwrap().is_err());
    created(&mut h, "first");
    assert_eq!(h.result(1), Some(&Ok("[\"first\"]".into())));
    assert!(h.requests.is_empty());
}

#[test]
fn hyro_receipts_cover_shared_write_late_waiter_and_retained_accepted_lots() {
    let mut h = ready(Variant::Hyrotrader);
    let orders = vec![order("BTCUSDT", 1.0), order("ETHUSDT", 1.0)];
    place(&mut h, 91, orders.clone());
    let first = link(h.requests.first().unwrap());
    place(&mut h, 92, orders.clone());
    assert_eq!(h.requests.len(), 1);
    for command in [91, 92] {
        assert_eq!(
            h.progress
                .iter()
                .filter(|(c, _)| *c == command)
                .flat_map(|(_, r)| r)
                .map(|r| r.identity.as_str())
                .collect::<Vec<_>>(),
            [first.as_str()]
        );
    }
    created(&mut h, "first");
    let second = link(h.requests.first().unwrap());
    place(&mut h, 93, orders);
    let late: Vec<_> = h
        .progress
        .iter()
        .filter(|(c, _)| *c == 93)
        .flat_map(|(_, r)| r)
        .collect();
    assert_eq!(
        late.iter().map(|r| r.symbol.as_str()).collect::<Vec<_>>(),
        ["BTCUSDT", "ETHUSDT"]
    );
    assert_eq!(
        late.iter().map(|r| r.identity.as_str()).collect::<Vec<_>>(),
        [first.as_str(), second.as_str()]
    );
    assert_eq!(h.requests.len(), 1);
    assert_eq!(
        h.progress_at_request
            .iter()
            .find(|(id, _)| *id == h.requests[0].id)
            .unwrap()
            .1,
        4
    );
}
#[test]
fn ordinary_bybit_receipts_preserve_absence_and_caller_identity_through_batch_fallback() {
    let mut h = ready(Variant::Linear);
    let mut supplied = order("BTCUSDT", 1.0);
    supplied.client_order_id = Some("Caller91".into());
    place(&mut h, 91, vec![supplied, order("ETHUSDT", 1.0)]);
    while h.has_request("/v5/position/list") {
        h.respond("/v5/position/list", 200, r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0},{"symbol":"ETHUSDT","positionIdx":0}]}}"#);
    }
    let batch = h.take("/v5/order/create-batch");
    assert_eq!(h.progress.len(), 1);
    assert_eq!(h.progress[0].1.len(), 1);
    assert_eq!(h.progress[0].1[0].identity, "Caller91");
    assert_eq!(
        h.progress_at_request
            .iter()
            .find(|(id, _)| *id == batch.id)
            .unwrap()
            .1,
        1
    );
    h.reply(
        batch.id,
        200,
        r#"{"retCode":10001,"retMsg":"batch rejected"}"#,
    );
    let first = h.take("/v5/order/create");
    assert!(body(&first).contains("Caller91"));
    assert_eq!(h.progress[1], h.progress[0]);
    h.reply(
        first.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"first"}}"#,
    );
    let second = h.take("/v5/order/create");
    assert!(!body(&second).contains("orderLinkId"));
    assert_eq!(h.progress.len(), 2);
    h.reply(
        second.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"second"}}"#,
    );
    assert_eq!(h.result(91), Some(&Ok("[\"first\",\"second\"]".into())));
    assert_eq!(
        h.command_errors.len(),
        1,
        "partial success keeps its diagnostic"
    );
    assert_eq!(h.command_errors[0].0, 91);
    assert!(
        h.command_errors[0]
            .1
            .contains("Failed to place batch orders")
    );
    assert_eq!(h.command_notices, [(91, "error"), (91, "done")]);
}

#[test]
fn hyro_rejected_later_link_is_not_replayed_as_a_new_waiters_identity() {
    let mut h = ready(Variant::Hyrotrader);
    let orders = vec![order("BTCUSDT", 1.0), order("ETHUSDT", 1.0)];
    place(&mut h, 91, orders.clone());
    let first = link(h.requests.first().unwrap());
    created(&mut h, "first");
    let second = h.take("/v5/order/create");
    let rejected = link(&second);
    h.reply(
        second.id,
        200,
        r#"{"retCode":110007,"retMsg":"insufficient balance"}"#,
    );
    place(&mut h, 92, orders);
    let replacement = link(h.requests.first().unwrap());
    assert_ne!(replacement, rejected);
    let rows: Vec<_> = h
        .progress
        .iter()
        .filter(|(command, _)| *command == 92)
        .flat_map(|(_, rows)| rows)
        .collect();
    assert_eq!(
        rows.iter().map(|r| r.identity.as_str()).collect::<Vec<_>>(),
        [first.as_str(), replacement.as_str()]
    );
    assert!(
        h.progress
            .iter()
            .filter(|(command, _)| *command == 91)
            .flat_map(|(_, rows)| rows)
            .any(|r| r.identity == rejected)
    );
}

#[test]
fn spot_single_create_receipt_preserves_exact_caller_identity() {
    let mut h = ready(Variant::Spot);
    let mut opts = order("BTCUSDT", 1.0);
    opts.client_order_id = Some("Exact_Caller-92".into());
    place(&mut h, 92, vec![opts]);
    let request = h.take("/v5/order/create");
    assert_eq!(link(&request), "Exact_Caller-92");
    assert_eq!(h.progress.len(), 1);
    assert_eq!(h.progress[0].0, 92);
    assert_eq!(h.progress[0].1.len(), 1);
    assert_eq!(h.progress[0].1[0].identity, "Exact_Caller-92");
    assert_eq!(
        h.progress_at_request
            .iter()
            .find(|(id, _)| *id == request.id)
            .unwrap()
            .1,
        1
    );
}

#[test]
fn shared_hyro_failure_is_correlated_to_each_waiter_before_its_completion() {
    let mut h = ready(Variant::Hyrotrader);
    let orders = vec![order("BTCUSDT", 1.0)];
    place(&mut h, 91, orders.clone());
    place(&mut h, 92, orders);
    let write = h.take("/v5/order/create");
    h.reply(
        write.id,
        200,
        r#"{"retCode":110007,"retMsg":"insufficient balance"}"#,
    );
    // An explicit retCode before any create landed is a refusal.
    assert_eq!(
        h.outcomes,
        [
            (91, Some(safe_cex_engine::Outcome::Rejected)),
            (92, Some(safe_cex_engine::Outcome::Rejected))
        ]
    );
    assert_eq!(h.command_errors.len(), 2);
    assert_eq!(h.command_errors[0].0, 91);
    assert_eq!(h.command_errors[1].0, 92);
    assert_eq!(h.command_errors[0].1, h.command_errors[1].1);
    // H2: one execution's seal reports to every observer, then completes each.
    assert_eq!(
        h.command_notices,
        [(91, "error"), (92, "error"), (91, "done"), (92, "done")]
    );
    assert!(h.result(91).unwrap().is_err());
    assert!(h.result(92).unwrap().is_err());
}

#[test]
fn identical_independent_validation_failures_keep_distinct_command_owners() {
    let mut h = ready(Variant::Spot);
    for command in [91, 92] {
        let mut opts = order("BTCUSDT", 1.0);
        opts.client_order_id = Some("invalid whitespace".into());
        place(&mut h, command, vec![opts]);
    }
    assert!(h.requests.is_empty());
    assert_eq!(h.command_errors.len(), 2);
    assert_eq!(h.command_errors[0].0, 91);
    assert_eq!(h.command_errors[1].0, 92);
    assert_eq!(h.command_errors[0].1, h.command_errors[1].1);
    assert_eq!(
        h.command_notices,
        [(91, "error"), (91, "done"), (92, "error"), (92, "done")]
    );
}

// V5 acknowledgement and realtime shapes:
// https://bybit-exchange.github.io/docs/v5/order/cancel-order
// https://bybit-exchange.github.io/docs/v5/order/open-order
#[test]
fn demo_cancel_ack_fences_in_flight_order_snapshot_at_same_clock() {
    for mode in [
        "accepted",
        "already-gone",
        "cancel-all",
        "partial",
        "rejected",
    ] {
        let mut h = ready(Variant::Demo);
        let row = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"canceled","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}"#;
        let failed = row.replace("canceled", "rejected");
        let rows = if mode == "partial" {
            format!("{row},{failed}")
        } else {
            row.to_owned()
        };
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"order","data":[{rows}]}}"#),
        );
        let command = if mode == "cancel-all" {
            Command::CancelSymbolOrders {
                command: 91,
                symbol: "BTCUSDT".into(),
            }
        } else {
            Command::CancelOrders {
                command: 91,
                ids: if mode == "partial" {
                    vec!["canceled".into(), "rejected".into()]
                } else {
                    vec!["canceled".into()]
                },
            }
        };
        h.command(command);
        let cancel = h.take(if mode == "cancel-all" {
            "/v5/order/cancel-all"
        } else {
            "/v5/order/cancel-batch"
        });
        // Subscription reconciliation starts while cancellation is in flight.
        h.message("/v5/private", r#"{"op":"auth","success":true}"#);
        h.message("/v5/private", r#"{"op":"subscribe","success":true}"#);
        let stale = h.take("settleCoin=USDT&limit=50");
        let reply = match mode {
            "already-gone" => {
                r#"{"retCode":110001,"retMsg":"order not exists or too late to cancel"}"#
            }
            "cancel-all" => {
                r#"{"retCode":0,"result":{"list":[{"orderId":"canceled","orderLinkId":""}],"success":"1"}}"#
            }
            // Refusals by native code: 110001 is the absent-order answer (D2).
            "partial" => {
                r#"{"retCode":0,"result":{},"retExtInfo":{"list":[{"code":0,"msg":"OK"},{"code":110020,"msg":"permission denied"}]}}"#
            }
            "rejected" => {
                r#"{"retCode":0,"result":{},"retExtInfo":{"list":[{"code":110020,"msg":"permission denied"}]}}"#
            }
            _ => r#"{"retCode":0,"result":{},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
        };
        h.reply(cancel.id, 200, reply);
        // D2: an absent-order answer waits for the reconcile read below.
        let retired_by_ack = !matches!(mode, "rejected" | "already-gone");
        assert_eq!(
            h.engine.store().order("canceled").is_none(),
            retired_by_ack,
            "{mode}"
        );
        h.reply(
            stale.id,
            200,
            &format!(r#"{{"retCode":0,"result":{{"list":[{rows}],"nextPageCursor":""}}}}"#),
        );
        h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
        assert_eq!(
            h.engine.store().order("canceled").is_none(),
            retired_by_ack,
            "{mode}: old snapshot reopened canceled order"
        );
        assert_eq!(
            h.has_request("/v5/order/realtime"),
            mode != "rejected",
            "{mode}: accepted cancellation invalidates read; rejection does not"
        );
        if mode == "partial" {
            assert_eq!(
                h.engine.store().order("rejected").unwrap().status,
                safe_cex_engine::model::OrderStatus::Open
            );
            assert!(h.result(91).unwrap().is_err());
        }
        while h.has_request("/v5/order/realtime") {
            h.respond("/v5/order/realtime", 200, EMPTY);
        }
        assert_eq!(
            h.engine.store().order("canceled").is_none(),
            mode != "rejected",
            "{mode}"
        );
    }
}

#[test]
fn hyro_cancel_ack_fences_snapshot_started_after_dispatch() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 90, vec![order("BTCUSDT", 1.0)]);
    created(&mut h, "canceled");
    h.command(Command::CancelOrders {
        command: 91,
        ids: vec!["canceled".into()],
    });
    let cancel = h.take("/v5/order/cancel");
    h.advance(5000.0);
    let stale = h.take("/v5/order/realtime");
    h.reply(
        cancel.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"canceled"}}"#,
    );
    assert!(h.engine.store().order("canceled").is_none());
    h.reply(stale.id, 200, r#"{"retCode":0,"result":{"list":[{"category":"linear","symbol":"BTCUSDT","orderId":"canceled","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}],"nextPageCursor":""}}"#);
    h.respond("/v5/position/list", 200, EMPTY);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    assert!(
        h.engine.store().order("canceled").is_none(),
        "atomic snapshot reopened canceled order"
    );
    assert!(h.has_request("/v5/order/realtime"));
    finish_hyro_snapshot(&mut h);
    assert!(h.engine.store().order("canceled").is_none());
}

// https://bybit-exchange.github.io/docs/v5/order/amend-order
#[test]
fn inverse_amend_ack_keeps_native_and_coin_units_until_stream_confirmation() {
    let mut h = ready(Variant::Inverse);
    h.message("/v5/private", r#"{"topic":"order.inverse","data":[{"category":"inverse","symbol":"BTCUSD","orderId":"inverse-amend","orderStatus":"PartiallyFilled","orderType":"Limit","side":"Buy","price":"50000","qty":"1000","cumExecQty":"250","leavesQty":"750"}]}"#);
    h.command(Command::AmendOrder {
        command: 92,
        id: "inverse-amend".into(),
        price: Some(40000.0),
        amount: Some(1500.0),
    });
    if h.has_request("/v5/position/list") {
        resolve_mode(&mut h, "BTCUSD");
    }
    let request = h.take("/v5/order/amend");
    assert!(body(&request).contains(r#""qty":"1500""#));
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"inverse-amend"}}"#,
    );
    let before = h.engine.store().order("inverse-amend").unwrap();
    assert_eq!(
        (before.amount, before.filled, before.remaining),
        (1000.0, 250.0, 0.015)
    );
    assert_eq!(before.extra.as_ref().unwrap().remaining_native, Some(750.0));
    h.message("/v5/private", r#"{"topic":"order.inverse","data":[{"category":"inverse","symbol":"BTCUSD","orderId":"inverse-amend","orderStatus":"PartiallyFilled","orderType":"Limit","side":"Buy","price":"40000","qty":"1500","cumExecQty":"250","leavesQty":"1250"}]}"#);
    let after = h.engine.store().order("inverse-amend").unwrap();
    assert_eq!(
        (after.amount, after.filled, after.remaining),
        (1500.0, 250.0, 0.03125)
    );
    assert_eq!(after.extra.as_ref().unwrap().remaining_native, Some(1250.0));
}

#[test]
fn demo_terminal_stream_at_snapshot_issue_time_cannot_be_reopened() {
    let mut h = ready(Variant::Demo);
    h.message("/v5/private", r#"{"op":"auth","success":true}"#);
    h.message("/v5/private", r#"{"op":"subscribe","success":true}"#);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"stream-canceled","orderStatus":"Cancelled","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"0"}]}"#);
    h.respond("settleCoin=USDT&limit=50", 200, r#"{"retCode":0,"result":{"list":[{"category":"linear","symbol":"BTCUSDT","orderId":"stream-canceled","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}],"nextPageCursor":""}}"#);
    h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
    assert_eq!(
        h.engine.store().order("stream-canceled").unwrap().status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
    assert!(h.has_request("/v5/order/realtime"));
}

#[test]
fn local_cancel_tombstone_filters_replica_stale_reads_issued_after_ack() {
    for variant in [Variant::Demo, Variant::Hyrotrader] {
        let mut h = ready(variant);
        let row = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"retired","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}"#;
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"order","data":[{row}]}}"#),
        );
        h.command(Command::CancelOrders {
            command: 95,
            ids: vec!["retired".into()],
        });
        if variant.is_hyro() {
            // "Cancel Order", the documented single-cancel acknowledgement (D).
            h.respond(
                "/v5/order/cancel",
                200,
                r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"retired","orderLinkId":""},"retExtInfo":{},"time":1}"#,
            );
        } else {
            h.respond(
                "/v5/order/cancel-batch",
                200,
                r#"{"retCode":0,"result":{},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
            );
        }
        assert!(h.engine.store().order("retired").is_none());
        // The stale replica read starts after cancellation is already acknowledged.
        for (terminal, reconnect) in [(false, false), (true, false), (false, true)] {
            if reconnect {
                let socket = h.socket("/v5/private").unwrap();
                h.event(Event::WsClosed { socket, code: 1006 });
                h.advance(2000.0);
                let fresh = h.socket("/v5/private").unwrap();
                assert_ne!(socket, fresh);
                h.event(Event::WsOpened { socket: fresh });
                for op in ["auth", "subscribe"] {
                    if variant.is_hyro() {
                        #[derive(serde::Deserialize)]
                        struct Sent {
                            req_id: String,
                        }
                        let sent = h
                            .sent_on("/v5/private")
                            .into_iter()
                            .rev()
                            .find(|message| message.contains(&format!("\"op\":\"{op}\"")))
                            .unwrap();
                        let sent: Sent = serde_json::from_str(&sent).unwrap();
                        h.message(
                            "/v5/private",
                            &format!(
                                r#"{{"op":"{op}","req_id":"{}","success":true}}"#,
                                sent.req_id
                            ),
                        );
                    } else {
                        h.message("/v5/private", &format!(r#"{{"op":"{op}","success":true}}"#));
                    }
                }
            } else if variant.is_hyro() {
                h.advance(5000.0);
            } else {
                h.message("/v5/private", r#"{"op":"auth","success":true}"#);
                h.message("/v5/private", r#"{"op":"subscribe","success":true}"#);
            }
            let other = row.replace("retired", "legitimate");
            let stale = if terminal {
                row.replace("New", "Cancelled")
                    .replace(r#""leavesQty":"1""#, r#""leavesQty":"0""#)
            } else {
                row.to_owned()
            };
            h.respond(
                "settleCoin=USDT&limit=50",
                200,
                &format!(
                    r#"{{"retCode":0,"result":{{"list":[{stale},{other}],"nextPageCursor":""}}}}"#
                ),
            );
            if !variant.is_hyro() {
                h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
            }
            if variant.is_hyro() {
                h.respond("/v5/position/list", 200, EMPTY);
                h.respond("/v5/account/wallet-balance", 200, WALLET);
            }
            assert!(
                h.engine
                    .store()
                    .order("retired")
                    .is_none_or(|o| o.status != safe_cex_engine::model::OrderStatus::Open),
                "{variant:?}: post-ack replica resurrected retired native ID"
            );
            assert_eq!(
                h.engine.store().order("legitimate").unwrap().status,
                safe_cex_engine::model::OrderStatus::Open
            );
            assert!(
                !h.has_request("/v5/order/realtime"),
                "{variant:?}: filtered/terminal rows must not cause a retry loop"
            );
        }
    }
}

#[test]
fn transitional_trigger_and_pending_cancel_do_not_tombstone_native_id() {
    for status in ["Triggered", "PendingCancel"] {
        let mut h = ready(Variant::Demo);
        let row = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"transition","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}"#;
        h.message(
            "/v5/private",
            &format!(
                r#"{{"topic":"order","data":[{}]}}"#,
                row.replace("New", status)
            ),
        );
        for body in [row.replace("New", status), row.to_owned()] {
            h.message("/v5/private", r#"{"op":"auth","success":true}"#);
            h.message("/v5/private", r#"{"op":"subscribe","success":true}"#);
            h.respond(
                "settleCoin=USDT&limit=50",
                200,
                &format!(r#"{{"retCode":0,"result":{{"list":[{body}],"nextPageCursor":""}}}}"#),
            );
            h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
        }
        assert_eq!(
            h.engine.store().order("transition").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open,
            "{status} is transitional"
        );
    }
}

#[test]
fn lost_or_server_failed_creates_are_unknown_but_venue_rejections_are_not() {
    for (case, unknown) in [
        ("transport", true),
        ("502", true),
        ("retCode 10016", true),
        ("504 with envelope", true),
        ("missing order id", true),
        ("retCode 170131", false),
    ] {
        let mut h = ready(Variant::Spot);
        place(&mut h, 2, vec![order("BTCUSDT", 0.1)]);
        let request = h.take("/v5/order/create");
        match case {
            "transport" => h.event(Event::HttpFailed {
                request: request.id,
                error: "connection reset",
            }),
            "502" => h.reply(request.id, 502, "<html>Bad Gateway</html>"),
            "retCode 10016" => h.reply(
                request.id,
                200,
                r#"{"retCode":10016,"retMsg":"Internal system error","result":{}}"#,
            ),
            "504 with envelope" => h.reply(
                request.id,
                504,
                r#"{"retCode":10002,"retMsg":"request expired","result":{}}"#,
            ),
            "missing order id" => h.reply(
                request.id,
                200,
                r#"{"retCode":0,"retMsg":"OK","result":{}}"#,
            ),
            _ => h.reply(
                request.id,
                200,
                r#"{"retCode":170131,"retMsg":"Insufficient balance.","result":{}}"#,
            ),
        }
        assert!(
            h.done
                .iter()
                .any(|(command, result)| *command == 2 && result.is_err()),
            "{case}: {:?}",
            h.done
        );
        assert_eq!(h.unknown == [2], unknown, "{case}");
    }
}

fn demo_reduce_only_resting(h: &mut Harness) {
    let mut opts = order("BTCUSDT", 1.0);
    opts.reduce_only = Some(true);
    opts.side = OrderSide::Sell;
    place(h, 1, vec![opts]);
    resolve_mode(h, "BTCUSDT");
    let create = h.take("create-batch");
    h.reply(
        create.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"orderId":"ro-1","orderLinkId":""}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
    );
    assert!(
        h.engine.store().order("ro-1").is_some(),
        "local Demo row published"
    );
}

#[test]
fn demo_reduce_only_limit_filled_without_stream_events_is_retired_by_its_executions() {
    let mut h = ready(Variant::Demo);
    demo_reduce_only_resting(&mut h);
    h.requests.clear();
    h.advance(1_000.0);
    let check = h.take("/v5/execution/list");
    assert!(
        check.url.contains("orderId=ro-1") && check.url.contains("symbol=BTCUSDT"),
        "{}",
        check.url
    );
    h.reply(
        check.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"nextPageCursor":"page2","list":[{"symbol":"BTCUSDT","orderId":"ro-1","execId":"e1","side":"Sell","execPrice":"100","execQty":"0.4","execType":"Trade","execTime":"1"},{"symbol":"BTCUSDT","orderId":"ro-1","execId":"e2","side":"Sell","execPrice":"100","execQty":"0.6","execType":"Trade","execTime":"2"}]}}"#,
    );
    // Bybit returns a cursor even on a short page; the walk follows it.
    assert!(h.engine.store().order("ro-1").is_some());
    let next = h.take("/v5/execution/list");
    assert!(next.url.contains("cursor=page2"), "{}", next.url);
    h.reply(
        next.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"nextPageCursor":"","list":[]}}"#,
    );
    assert!(
        h.engine.store().order("ro-1").is_none(),
        "a fully filled order stayed on the chart"
    );
    // A stale replica cannot reopen it.
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"ro-1","orderStatus":"New","orderType":"Limit","side":"Sell","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1","reduceOnly":true}]}"#);
    assert!(h.engine.store().order("ro-1").is_none());
}

#[test]
fn demo_reduce_only_partial_or_failed_check_never_proves_a_fill() {
    let mut h = ready(Variant::Demo);
    demo_reduce_only_resting(&mut h);
    h.requests.clear();
    h.advance(1_000.0);
    let check = h.take("/v5/execution/list");
    h.reply(
        check.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"nextPageCursor":"","list":[{"symbol":"BTCUSDT","orderId":"ro-1","execId":"e1","side":"Sell","execPrice":"100","execQty":"0.4","execType":"Trade","execTime":"1"}]}}"#,
    );
    assert!(h.engine.store().order("ro-1").is_some());
    h.advance(2_000.0);
    let check = h.take("/v5/execution/list");
    h.reply(check.id, 500, "upstream");
    assert!(h.engine.store().order("ro-1").is_some());
    // After the placement grace an inconclusive check re-reads open orders instead.
    h.requests.clear();
    h.advance(5_250.0);
    let check = h.take("/v5/execution/list");
    h.reply(
        check.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"nextPageCursor":"more","list":[{"symbol":"BTCUSDT","orderId":"ro-1","execId":"e1","side":"Sell","execPrice":"100","execQty":"1","execType":"Trade","execTime":"1"}]}}"#,
    );
    // The next page fails, so the walk is incomplete.
    let next = h.take("/v5/execution/list");
    h.reply(next.id, 500, "upstream");
    assert!(
        h.engine.store().order("ro-1").is_some(),
        "an incomplete page proved a full fill"
    );
    assert!(h.has_request("/v5/order/realtime"));
}

// Private reads now start before the ticker seed. A spot TP/SL with no
// trigger direction was labelled against a zero price and never relabelled.
#[test]
fn spot_tpsl_read_before_the_ticker_seed_is_relabelled_when_it_lands() {
    let mut h = Harness::started(options(Variant::Spot));
    for (path, body) in [
        ("/v5/account/info", support::ACCOUNT_INFO),
        ("/v5/account/fee-rate", EMPTY),
        ("status=PreLaunch", EMPTY),
    ] {
        if h.has_request(path) {
            h.respond(path, 200, body);
        }
    }
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    assert!(
        h.has_request("/v5/order/realtime"),
        "orders read before tickers"
    );
    h.respond(
        "/v5/order/realtime",
        200,
        r#"{"retCode":0,"result":{"list":[{"category":"spot","symbol":"BTCUSDT","orderId":"stop","orderStatus":"Untriggered","orderType":"Market","stopOrderType":"tpslOrder","side":"Sell","price":"0","triggerPrice":"60000","qty":"0.1","cumExecQty":"0","leavesQty":"0.1"}],"nextPageCursor":""}}"#,
    );
    while h.has_request("/v5/order/realtime") {
        h.respond("/v5/order/realtime", 200, EMPTY);
    }
    if let Some(order) = h.engine.store().order("stop") {
        assert_eq!(
            order.kind,
            OrderType::TakeProfit,
            "labelled against no price"
        );
    }
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    let order = h.engine.store().order("stop").expect("spot stop stored");
    assert_eq!(order.kind, OrderType::StopLoss);
}

// The same row streamed in before the seed but committed after it.
#[test]
fn spot_tpsl_streamed_before_the_seed_and_committed_after_it_is_relabelled() {
    let mut h = Harness::started(options(Variant::Spot));
    for (path, body) in [
        ("/v5/account/info", support::ACCOUNT_INFO),
        ("/v5/account/fee-rate", EMPTY),
        ("status=PreLaunch", EMPTY),
    ] {
        if h.has_request(path) {
            h.respond(path, 200, body);
        }
    }
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    let read = h.take("/v5/order/realtime");
    let body = r#"{"retCode":0,"result":{"list":[{"category":"spot","symbol":"BTCUSDT","orderId":"stop","orderStatus":"Untriggered","orderType":"Market","stopOrderType":"tpslOrder","side":"Sell","price":"0","triggerPrice":"60000","qty":"0.1","cumExecQty":"0","leavesQty":"0.1"}],"nextPageCursor":""}}"#;
    h.event(Event::HttpStart {
        request: read.id,
        status: 200,
        headers: "",
    });
    h.event(Event::HttpChunk {
        request: read.id,
        data: body.as_bytes(),
    });
    h.respond("/v5/market/tickers", 200, support::TICKERS);
    h.event(Event::HttpEnd { request: read.id });
    while h.has_request("/v5/order/realtime") {
        h.respond("/v5/order/realtime", 200, EMPTY);
    }
    let order = h.engine.store().order("stop").expect("spot stop stored");
    assert_eq!(order.kind, OrderType::StopLoss);
}

/// Two same-ID sends per leg is a bound on the retained intent, not per
/// caller: a resumed identical call must not send the uncertain create again.
/// Amended (ruling Q4, R1(a)): one send per call, re-read before each resume.
#[test]
fn hyro_resumed_call_cannot_exceed_two_same_id_creates() {
    let mut h = ready(Variant::Hyrotrader);
    let mut sends = 0;
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    sends += 1;
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    answer_link_absent(&mut h, &id);
    assert!(h.result(1).unwrap().is_err());
    for command in [2, 3] {
        place(&mut h, command, vec![order("BTCUSDT", 1.0)]);
        answer_link_absent(&mut h, &id);
        while h.has_request("/v5/order/create") {
            let create = h.take("/v5/order/create");
            sends += 1;
            assert_eq!(link(&create), id);
            h.event(Event::HttpFailed {
                request: create.id,
                error: "lost",
            });
            answer_link_absent(&mut h, &id);
        }
        assert!(h.result(command).unwrap().is_err());
        assert!(h.unknown.contains(&command));
    }
    assert_eq!(sends, 2, "same orderLinkId physically sent more than twice");
}

#[test]
fn ordinary_gateway_already_gone_text_cannot_authorize_cancellation_effects() {
    use safe_cex_engine::Outcome;
    let mut observed = Vec::new();
    for (status, code) in [
        (408, 0),
        (500, 0),
        (408, 110001),
        (500, 110001),
        (200, 110001),
    ] {
        let mut h = ready(Variant::Demo);
        h.message(
            "/v5/private",
            r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"kept","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}]}"#,
        );
        let before = h
            .engine
            .store()
            .order("kept")
            .cloned()
            .expect("native order seed");
        h.command(Command::CancelOrders {
            command: 91,
            ids: vec!["kept".into()],
        });
        let request = h.take("/v5/order/cancel-batch");
        h.reply(
            request.id,
            status,
            &format!(r#"{{"retCode":{code},"retMsg":"order not exists or too late to cancel"}}"#),
        );
        observed.push((
            status,
            code,
            h.engine.store().order("kept") == Some(&before),
            h.outcomes
                .iter()
                .find(|(id, _)| *id == 91)
                .and_then(|(_, outcome)| *outcome),
            h.result(91).expect("one final outcome").is_err(),
        ));
        assert!(
            !h.has_request("/v5/order/cancel"),
            "no automatic cancel replay"
        );
        if status == 200 {
            // D2: only the reconcile read, never the answer, retires the row.
            answer_reconcile(&mut h);
            assert!(h.engine.store().order("kept").is_none());
        }
    }
    assert_eq!(
        observed,
        vec![
            (408, 0, true, Some(Outcome::Unknown), true),
            (500, 0, true, Some(Outcome::Unknown), true),
            (408, 110001, true, Some(Outcome::Unknown), true),
            (500, 110001, true, Some(Outcome::Unknown), true),
            (200, 110001, true, None, false),
        ],
        "gateway status dominates benign text; only a definitive native response may cancel",
    );
}

fn signature_words_without_refusal(body: &str, success: bool) {
    let mut h = ready(Variant::Demo);
    h.command(Command::SetLeverage {
        command: 92,
        symbol: "BTCUSDT".into(),
        leverage: 3.,
        side: None,
    });
    let request = h.take("/v5/position/set-leverage");
    h.reply(request.id, 200, body);
    assert!(
        !h.has_request("/v5/position/set-leverage"),
        "text cannot authorize replay"
    );
    let result = h.result(92).expect("one terminal result");
    assert_eq!(result.is_ok(), success);
    if !success {
        assert!(
            h.outcomes
                .contains(&(92, Some(safe_cex_engine::Outcome::Unknown)))
        );
    }
}

#[test]
fn ordinary_signature_words_without_native_code_cannot_authorize_retry() {
    signature_words_without_refusal("error sign", false);
}

#[test]
fn ordinary_signature_words_in_native_success_cannot_authorize_retry() {
    signature_words_without_refusal(r#"{"retCode":0,"retMsg":"error sign","result":{}}"#, true);
}

#[test]
fn ordinary_signature_words_in_mode_read_success_cannot_authorize_retry() {
    let mut h = ready(Variant::Demo);
    place(&mut h, 93, vec![order("BTCUSDT", 0.01)]);
    let mode = h.take("/v5/position/list");
    h.reply(mode.id, 200, r#"{"retCode":0,"retMsg":"error sign","result":{"list":[{"symbol":"BTCUSDT","positionIdx":0}]}}"#);
    assert!(!h.has_request("/v5/position/list"));
    assert!(h.has_request("/v5/order/create-batch"));
    assert!(h.result(93).is_none());
}

fn assert_invalid_managed_cancel_batch_preserves_rows(reply: &str) {
    let mut h = ready(Variant::Demo);
    let row = r#"{"symbol":"BTCUSDT","orderId":"one","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}"#;
    h.message(
        "/v5/private",
        &format!(
            r#"{{"topic":"order","data":[{},{}]}}"#,
            row,
            row.replace("one", "two")
        ),
    );
    h.command(Command::CancelOrders {
        command: 94,
        ids: vec!["one".into(), "two".into()],
    });
    let cancel = h.take("/v5/order/cancel-batch");
    h.reply(cancel.id, 200, reply);
    for id in ["one", "two"] {
        assert_eq!(
            h.engine.store().order(id).map(|o| o.status),
            Some(safe_cex_engine::model::OrderStatus::Open),
            "{reply}: {id}"
        );
    }
    assert!(h.result(94).unwrap().is_err());
    assert!(h.unknown.contains(&94));
    assert!(!h.has_request("/v5/order/cancel"));
}

#[test]
fn managed_cancel_batch_missing_or_mismatched_status_never_patches_store() {
    for reply in [
        r#"{"retCode":0,"result":{}}"#,
        r#"{"retCode":0,"result":{},"retExtInfo":{"list":[{"code":0}]}}"#,
        r#"{"retCode":0,"result":{},"retExtInfo":{"list":[{},{}]}}"#,
        r#"{"retCode":0,"result":{},"retExtInfo":{"list":[[0,"OK"],[0,"OK"]]}}"#,
    ] {
        assert_invalid_managed_cancel_batch_preserves_rows(reply);
    }
}

#[test]
fn managed_cancel_batch_invalid_message_type_cannot_hide_refused_rows() {
    assert_invalid_managed_cancel_batch_preserves_rows(
        r#"{"retCode":0,"result":{},"retExtInfo":{"list":[{"code":0,"msg":"OK"},{"code":10005,"msg":42}]}}"#,
    );
}

#[test]
fn managed_cancel_batch_null_result_list_cannot_hide_refused_rows() {
    assert_invalid_managed_cancel_batch_preserves_rows(
        r#"{"retCode":0,"result":{"list":null},"retExtInfo":{"list":[{"code":0,"msg":"OK"},{"code":10005,"msg":"permission denied"}]}}"#,
    );
}

#[test]
fn managed_create_batch_count_mismatch_keeps_only_evidence_ids_without_local_rows() {
    let mut h = ready(Variant::Demo);
    place(
        &mut h,
        95,
        vec![order("BTCUSDT", 0.01), order("BTCUSDT", 0.02)],
    );
    resolve_mode(&mut h, "BTCUSDT");
    let create = h.take("/v5/order/create-batch");
    h.reply(create.id, 200, r#"{"retCode":0,"result":{"list":[{"orderId":"native-one"}]},"retExtInfo":{"list":[{"code":0},{"code":0}]}}"#);
    assert!(
        h.engine.store().order("native-one").is_none(),
        "unattributed native ID is evidence only"
    );
    assert!(h.result(95).unwrap().is_err());
    assert!(h.unknown.contains(&95));
    assert!(h.notices.iter().any(|notice| matches!(notice, safe_cex_engine::Notice::CommandFailed { command:95, error } if error.partial.as_ref().is_some_and(|p|p.order_ids == ["native-one"]))));
    assert!(!h.has_request("/v5/order/create"));
}

#[test]
fn managed_create_batch_native_shapes_are_validated_before_any_local_publication() {
    let ids = r#"[{"orderId":"native-one"},{"orderId":"native-two"}]"#;
    let statuses = r#"[{"code":0},{"code":0}]"#;
    let extra_ids = (0..11)
        .map(|i| format!(r#"{{"orderId":"extra-{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let replies = [
        format!(
            r#"{{"retCode":0,"result":{{"list":[["native-one"],["native-two"]]}},"retExtInfo":{{"list":{statuses}}}}}"#
        ),
        format!(r#"{{"retCode":0,"result":{{"list":{ids}}},"retExtInfo":{{"list":[[0],[0]]}}}}"#),
        format!(r#"{{"retCode":0,"result":{{"list":{ids}}},"retExtInfo":{{"list":[{{}},{{}}]}}}}"#),
        format!(
            r#"{{"retCode":0,"result":{{"list":[{{"orderId":"native-one"}},{{"orderId":"native-one"}}]}},"retExtInfo":{{"list":{statuses}}}}}"#
        ),
        format!(
            r#"{{"retCode":0,"result":{{"list":[{{"orderId":"native-one","orderLinkId":"wrong-echo"}},{{"orderId":"native-two"}}]}},"retExtInfo":{{"list":{statuses}}}}}"#
        ),
        format!(
            r#"{{"retCode":0,"result":{{"list":[{extra_ids}]}},"retExtInfo":{{"list":{statuses}}}}}"#
        ),
        format!(
            r#"{{"retCode":0,"result":{{"list":[{{"orderId":"native-one"}},123]}},"retExtInfo":{{"list":{statuses}}}}}"#
        ),
    ];
    for (case, reply) in replies.into_iter().enumerate() {
        let mut h = ready(Variant::Demo);
        place(
            &mut h,
            96,
            vec![order("BTCUSDT", 0.01), order("BTCUSDT", 0.02)],
        );
        resolve_mode(&mut h, "BTCUSDT");
        let create = h.take("/v5/order/create-batch");
        h.reply(create.id, 200, &reply);
        assert!(
            h.engine.store().order("native-one").is_none(),
            "case {case}"
        );
        assert!(
            h.engine.store().order("native-two").is_none(),
            "case {case}"
        );
        assert!(h.result(96).unwrap().is_err(), "case {case}");
        assert!(h.unknown.contains(&96), "case {case}");
        let error = h
            .notices
            .iter()
            .find_map(|notice| match notice {
                safe_cex_engine::Notice::CommandFailed { command: 96, error } => Some(error),
                _ => None,
            })
            .unwrap();
        let actual = error
            .partial
            .as_ref()
            .map(|p| p.order_ids.clone())
            .unwrap_or_default();
        if case == 0 || case == 6 {
            assert!(actual.is_empty(), "no positional or malformed-prefix IDs");
        }
        if case == 5 {
            assert_eq!(
                actual,
                (0..11).map(|i| format!("extra-{i}")).collect::<Vec<_>>(),
                "valid extra rows survive as evidence only"
            );
            assert!(
                !h.engine
                    .store()
                    .orders()
                    .iter()
                    .any(|o| o.id.starts_with("extra-"))
            );
        }
        assert!(!h.has_request("/v5/order/create"));
    }
}

fn assert_contradictory_managed_batch_refusal_does_not_retry(code: i64) {
    let mut h = ready(Variant::Demo);
    place(
        &mut h,
        97,
        vec![order("BTCUSDT", 0.01), order("BTCUSDT", 0.02)],
    );
    resolve_mode(&mut h, "BTCUSDT");
    let create = h.take("/v5/order/create-batch");
    h.reply(create.id,200,&format!(r#"{{"retCode":{code},"retMsg":"error sign or invalid batch","result":{{"list":[{{"orderId":"contradictory-native"}}]}}}}"#));
    assert!(
        !h.has_request("/v5/order/create"),
        "contradictory native IDs forbid fallback/resigning"
    );
    assert!(h.result(97).unwrap().is_err());
    assert!(h.unknown.contains(&97));
    assert!(h.engine.store().order("contradictory-native").is_none());
    assert!(h.notices.iter().any(|n|matches!(n,safe_cex_engine::Notice::CommandFailed{command:97,error} if error.partial.as_ref().is_some_and(|p|p.order_ids==["contradictory-native"]))));
}
#[test]
fn managed_batch_refusal_with_native_ids_cannot_authorize_replacement() {
    assert_contradictory_managed_batch_refusal_does_not_retry(10001);
}
#[test]
fn managed_batch_signature_refusal_with_native_ids_cannot_authorize_retry() {
    assert_contradictory_managed_batch_refusal_does_not_retry(10004);
}

fn assert_batch_refusal_with_item_effect_does_not_retry(code: i64) {
    for item_code in [0, 10016] {
        let mut h = ready(Variant::Demo);
        place(
            &mut h,
            98,
            vec![order("BTCUSDT", 0.01), order("BTCUSDT", 0.02)],
        );
        resolve_mode(&mut h, "BTCUSDT");
        let create = h.take("/v5/order/create-batch");
        h.reply(create.id,200,&format!(r#"{{"retCode":{code},"retMsg":"native batch refusal","result":{{}},"retExtInfo":{{"list":[{{"code":{item_code},"msg":"native per-item fact"}},{{"code":10005,"msg":"refused"}}]}}}}"#));
        assert!(
            !h.has_request("/v5/order/create"),
            "per-item acceptance/uncertainty forbids whole-refusal retry"
        );
        assert!(h.result(98).unwrap().is_err());
        assert!(h.unknown.contains(&98));
        assert!(h.engine.store().orders().is_empty());
    }
}
#[test]
fn managed_batch_refusal_with_item_effect_cannot_authorize_replacement() {
    assert_batch_refusal_with_item_effect_does_not_retry(10001);
}
#[test]
fn managed_batch_signature_refusal_with_item_effect_cannot_authorize_retry() {
    assert_batch_refusal_with_item_effect_does_not_retry(10004);
}

#[test]
fn managed_cancel_benign_native_refusal_keeps_valid_target_echoes() {
    let mut h = ready(Variant::Demo);
    let row = r#"{"symbol":"BTCUSDT","orderId":"one","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}"#;
    h.message(
        "/v5/private",
        &format!(
            r#"{{"topic":"order","data":[{},{}]}}"#,
            row,
            row.replace("one", "two")
        ),
    );
    h.command(Command::CancelOrders {
        command: 99,
        ids: vec!["one".into(), "two".into()],
    });
    let cancel = h.take("/v5/order/cancel-batch");
    h.reply(cancel.id,200,r#"{"retCode":110001,"retMsg":"order not exists or too late to cancel","result":{"list":[{"orderId":"one"},{"orderId":"two"}]}}"#);
    assert!(
        h.engine.store().order("one").is_some(),
        "no local Cancelled transition"
    );
    answer_reconcile(&mut h);
    assert!(h.engine.store().order("one").is_none());
    assert!(h.engine.store().order("two").is_none());
    assert_eq!(h.result(99), Some(&Ok("[]".into())));
    assert!(!h.unknown.contains(&99));
}

#[test]
fn managed_native_id_overflow_denies_local_publication_before_command_seal() {
    let mut h = ready(Variant::Demo);
    place(&mut h, 100, vec![order("BTCUSDT", 0.01)]);
    resolve_mode(&mut h, "BTCUSDT");
    let create = h.take("/v5/order/create-batch");
    let native_id = "n".repeat(1025);
    h.reply(create.id,200,&format!(r#"{{"retCode":0,"result":{{"list":[{{"orderId":"{native_id}"}}]}},"retExtInfo":{{"list":[{{"code":0}}]}}}}"#));
    assert!(
        h.engine.store().order(&native_id).is_none(),
        "native publication needs bounded admitted facts first"
    );
    assert!(h.result(100).unwrap().is_err());
    assert!(h.unknown.contains(&100));
    assert!(!h.engine.store().orders().iter().any(|o| o.id.len() > 1024));
}

#[test]
fn managed_create_batch_proven_subset_survives_an_unresolved_sibling() {
    let mut h = ready(Variant::Demo);
    place(
        &mut h,
        101,
        vec![order("BTCUSDT", 0.01), order("BTCUSDT", 0.02)],
    );
    resolve_mode(&mut h, "BTCUSDT");
    let create = h.take("/v5/order/create-batch");
    h.reply(create.id, 200, r#"{"retCode":0,"result":{"list":[{"orderId":"proven-native"},{}]},"retExtInfo":{"list":[{"code":0},{"code":10016,"msg":"execution unresolved"}]}}"#);
    assert!(h.engine.store().order("proven-native").is_some());
    assert_eq!(h.engine.store().orders().len(), 1);
    assert!(h.result(101).unwrap().is_err());
    assert!(h.unknown.contains(&101));
    assert!(h.notices.iter().any(
        |notice| matches!(notice, safe_cex_engine::Notice::CommandFailed { command:101, error }
        if error.partial.as_ref().is_some_and(|p|p.order_ids == ["proven-native"]))
    ));
    assert!(!h.has_request("/v5/order/create"));
    assert!(!h.has_request("/v5/order/create-batch"));
}

#[test]
fn managed_cancel_all_housekeeping_preserves_live_and_uncaptured_children() {
    let mut h = ready(Variant::Demo);
    let row = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"live","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}"#;
    h.message(
        "/v5/private",
        &format!(r#"{{"topic":"order","data":[{row}]}}"#),
    );
    let seed = h.engine.store().order("live").unwrap().clone();
    let mut terminal = seed.clone();
    terminal.id = "old-parent".into();
    terminal.status = safe_cex_engine::model::OrderStatus::Closed;
    h.engine.store_mut().upsert_order(terminal);
    let mut revived = seed.clone();
    revived.id = "captured-terminal-now-live".into();
    revived.status = safe_cex_engine::model::OrderStatus::Canceled;
    h.engine.store_mut().upsert_order(revived.clone());
    let mut child = seed.clone();
    child.id = "old-parent__stop_loss".into();
    child.kind = OrderType::StopLoss;
    h.engine.store_mut().upsert_order(child.clone());
    h.command(Command::CancelSymbolOrders {
        command: 102,
        symbol: "BTCUSDT".into(),
    });
    revived.status = safe_cex_engine::model::OrderStatus::Open;
    h.engine.store_mut().upsert_order(revived.clone());
    let mut uncaptured = seed;
    uncaptured.id = "old-parent__take_profit".into();
    uncaptured.status = safe_cex_engine::model::OrderStatus::Closed;
    h.engine.store_mut().upsert_order(uncaptured.clone());
    h.respond(
        "/v5/order/cancel-all",
        200,
        r#"{"retCode":0,"result":{"list":[],"success":"1"}}"#,
    );
    assert!(h.engine.store().order("old-parent").is_none());
    assert_eq!(h.engine.store().order(&child.id), Some(&child));
    assert_eq!(h.engine.store().order(&uncaptured.id), Some(&uncaptured));
    assert_eq!(h.engine.store().order(&revived.id), Some(&revived));
    assert!(h.engine.store().order("live").is_some());
    h.respond(
        "settleCoin=USDT&limit=50",
        200,
        &format!(r#"{{"retCode":0,"result":{{"list":[{row}],"nextPageCursor":""}}}}"#),
    );
    h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
    h.advance(0.0);
    assert!(
        h.result(102)
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("on BTCUSDT: live")
    );
}

#[test]
fn managed_cancel_all_housekeeping_requires_validated_native_success() {
    for (status, body) in [
        (503, r#"{"retCode":0,"result":{"list":[],"success":"1"}}"#),
        (200, r#"{"retCode":0,"result":{"list":null,"success":"1"}}"#),
        (
            200,
            r#"{"retCode":10016,"result":{"list":[],"success":"1"}}"#,
        ),
    ] {
        let mut h = ready(Variant::Demo);
        let row = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"terminal","orderStatus":"Filled","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"1","leavesQty":"0"}"#;
        // Native terminal rows may remain in the local snapshot until cleanup.
        let open = row
            .replace("Filled", "New")
            .replace(r#""cumExecQty":"1""#, r#""cumExecQty":"0""#);
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"order","data":[{open}]}}"#),
        );
        let mut terminal = h.engine.store().order("terminal").unwrap().clone();
        terminal.status = safe_cex_engine::model::OrderStatus::Closed;
        h.engine.store_mut().upsert_order(terminal.clone());
        h.command(Command::CancelSymbolOrders {
            command: 103,
            symbol: "BTCUSDT".into(),
        });
        h.respond("/v5/order/cancel-all", status, body);
        assert_eq!(
            h.engine.store().order("terminal"),
            Some(&terminal),
            "{status} {body}"
        );
        assert!(h.result(103).unwrap().is_err());
        assert!(h.unknown.contains(&103));
    }
}

fn managed_cancel_code_reply(body: &str, accepted: bool) {
    let mut h = ready(Variant::Demo);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"kept","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}]}"#);
    let before = h.engine.store().order("kept").unwrap().clone();
    h.command(Command::CancelOrders {
        command: 104,
        ids: vec!["kept".into()],
    });
    h.respond("/v5/order/cancel-batch", 200, body);
    if accepted {
        assert_eq!(
            h.engine.store().order("kept"),
            Some(&before),
            "no local transition"
        );
        answer_reconcile(&mut h);
        assert!(h.engine.store().order("kept").is_none());
        assert_eq!(h.result(104), Some(&Ok("[]".into())));
        assert!(!h.unknown.contains(&104));
    } else {
        assert_eq!(h.engine.store().order("kept"), Some(&before));
        assert!(h.result(104).unwrap().is_err());
        assert!(h.unknown.contains(&104));
    }
    assert!(!h.has_request("/v5/order/cancel"));
    assert!(!h.has_request("/v5/order/cancel-batch"));
}

#[test]
fn managed_whole_cancel_unresolved_code_cannot_borrow_already_gone_text() {
    managed_cancel_code_reply(
        r#"{"retCode":10016,"retMsg":"order not exists or too late to cancel"}"#,
        false,
    );
}

#[test]
fn managed_item_cancel_unresolved_code_cannot_borrow_already_gone_text() {
    managed_cancel_code_reply(
        r#"{"retCode":0,"result":{},"retExtInfo":{"list":[{"code":10016,"msg":"order not exists or too late to cancel"}]}}"#,
        false,
    );
}

#[test]
fn managed_documented_absent_order_codes_do_not_depend_on_diagnostic_text() {
    for code in [110001, 170213] {
        managed_cancel_code_reply(
            &format!(r#"{{"retCode":{code},"retMsg":"native diagnostic"}}"#),
            true,
        );
    }
}

fn managed_trailing_noop_reply(code: i64, accepted: bool) {
    let mut h = ready(Variant::Linear);
    let mut opts = order("BTCUSDT", 0.0);
    opts.kind = OrderType::TrailingStopLoss;
    opts.price = Some(65000.0);
    place(&mut h, 105, vec![opts]);
    resolve_mode(&mut h, "BTCUSDT");
    let message = if accepted {
        "native diagnostic"
    } else {
        "not modified"
    };
    h.respond(
        "/v5/position/trading-stop",
        200,
        &format!(r#"{{"retCode":{code},"retMsg":"{message}"}}"#),
    );
    if accepted {
        assert_eq!(h.result(105), Some(&Ok("[]".into())));
        assert!(!h.unknown.contains(&105));
    } else {
        assert!(h.result(105).unwrap().is_err());
        assert!(h.unknown.contains(&105));
    }
    assert!(!h.has_request("/v5/position/trading-stop"));
    assert!(!h.has_request("/v5/order/create"));
}

#[test]
fn managed_not_modified_text_on_unresolved_code_cannot_seal_success() {
    managed_trailing_noop_reply(10016, false);
}

#[test]
fn managed_documented_tpsl_noop_code_retains_success() {
    managed_trailing_noop_reply(34040, true);
}

#[test]
fn managed_tpsl_noop_code_cannot_authorize_an_unrelated_amend_or_publish_a_new_value() {
    for attached in [false, true] {
        let mut h = ready(Variant::Demo);
        h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"parent","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}]}"#);
        let mut before = h.engine.store().order("parent").unwrap().clone();
        if attached {
            before.id = "parent__stop_loss".into();
            before.parent_id = Some("parent".into());
            before.kind = OrderType::StopLoss;
            before.price = 70.;
            h.engine.store_mut().upsert_order(before.clone());
        }
        h.command(Command::AmendOrder {
            command: 106,
            id: before.id.clone(),
            price: Some(110.),
            amount: None,
        });
        resolve_mode(&mut h, "BTCUSDT");
        h.respond(
            "/v5/order/amend",
            200,
            r#"{"retCode":34040,"retMsg":"not modified"}"#,
        );
        assert_eq!(h.engine.store().order(&before.id), Some(&before));
        if attached {
            assert_eq!(h.result(106), Some(&Ok("[]".into())));
        } else {
            assert!(h.result(106).unwrap().is_err());
        }
        assert!(!h.unknown.contains(&106));
        assert!(!h.has_request("/v5/order/amend"));
    }
}

fn managed_single_create() -> (Harness, HttpRequest) {
    let mut h = ready(Variant::Demo);
    let mut opts = order("BTCUSDT", 0.01);
    opts.client_order_id = Some("SingleCaller".into());
    place(&mut h, 107, vec![opts]);
    resolve_mode(&mut h, "BTCUSDT");
    h.respond(
        "/v5/order/create-batch",
        200,
        r#"{"retCode":10001,"retMsg":"native batch refusal"}"#,
    );
    let create = h.take("/v5/order/create");
    assert!(body(&create).contains("SingleCaller"));
    (h, create)
}

fn assert_managed_single_create_unproven(reply: &str) {
    let (mut h, create) = managed_single_create();
    h.reply(create.id, 200, reply);
    assert!(
        !h.has_request("/v5/order/create"),
        "invalid identity cannot authorize retry"
    );
    assert!(h.engine.store().order("native-one").is_none());
    assert!(h.result(107).unwrap().is_err());
    assert!(h.unknown.contains(&107));
}

#[test]
fn managed_single_create_rejects_positional_identity_before_local_publication() {
    assert_managed_single_create_unproven(r#"{"retCode":0,"result":["native-one"]}"#);
}

#[test]
fn managed_single_create_requires_its_supplied_client_echo() {
    assert_managed_single_create_unproven(
        r#"{"retCode":0,"result":{"orderId":"native-one","orderLinkId":"other-caller"}}"#,
    );
}

#[test]
fn managed_single_create_signature_refusal_with_accepted_identity_cannot_retry() {
    assert_managed_single_create_unproven(
        r#"{"retCode":10004,"retMsg":"error sign","result":{"orderId":"native-one","orderLinkId":"SingleCaller"}}"#,
    );
}

#[test]
fn managed_single_create_valid_identity_and_absent_or_matching_echo_preserve_success() {
    for echo in ["", r#", "orderLinkId":"SingleCaller""#] {
        let (mut h, create) = managed_single_create();
        h.reply(
            create.id,
            200,
            &format!(r#"{{"retCode":0,"result":{{"orderId":"native-one"{echo}}}}}"#),
        );
        assert!(h.engine.store().order("native-one").is_some());
        assert_eq!(h.result(107), Some(&Ok(r#"["native-one"]"#.into())));
        assert!(!h.unknown.contains(&107));
    }
}

#[test]
fn managed_single_amend_requires_exact_object_target_before_local_publication() {
    for result in [r#"["parent"]"#, "{}", r#"{"orderId":"other"}"#] {
        let mut h = ready(Variant::Demo);
        h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"parent","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}]}"#);
        let before = h.engine.store().order("parent").unwrap().clone();
        h.command(Command::AmendOrder {
            command: 108,
            id: "parent".into(),
            price: Some(110.),
            amount: None,
        });
        resolve_mode(&mut h, "BTCUSDT");
        h.respond(
            "/v5/order/amend",
            200,
            &format!(r#"{{"retCode":0,"result":{result}}}"#),
        );
        assert_eq!(h.engine.store().order("parent"), Some(&before), "{result}");
        assert!(h.result(108).unwrap().is_err());
        assert!(h.unknown.contains(&108));
        assert!(!h.has_request("/v5/order/amend"));
    }
}

#[test]
fn managed_single_cancel_requires_exact_native_target_before_store_effects() {
    let mut h = ready(Variant::Spot);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"spot","symbol":"BTCUSDT","orderId":"parent","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}]}"#);
    let before = h.engine.store().order("parent").unwrap().clone();
    h.command(Command::CancelOrders {
        command: 109,
        ids: vec!["parent".into()],
    });
    h.respond(
        "/v5/order/cancel",
        200,
        r#"{"retCode":0,"result":{"orderId":"other"}}"#,
    );
    assert_eq!(h.engine.store().order("parent"), Some(&before));
    assert!(h.result(109).unwrap().is_err());
    assert!(h.unknown.contains(&109));
    assert!(!h.has_request("/v5/order/cancel"));
}

#[test]
fn managed_single_target_only_requests_accept_the_existing_returned_client_identity() {
    for mode in ["amend", "attached", "cancel"] {
        let mut h = ready(if mode == "cancel" {
            Variant::Spot
        } else {
            Variant::Demo
        });
        let category = if mode == "cancel" { "spot" } else { "linear" };
        h.message("/v5/private", &format!(r#"{{"topic":"order","data":[{{"category":"{category}","symbol":"BTCUSDT","orderId":"parent","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}}]}}"#));
        let target = if mode == "attached" {
            let mut child = h.engine.store().order("parent").unwrap().clone();
            child.id = "parent__stop_loss".into();
            child.parent_id = Some("parent".into());
            child.kind = OrderType::StopLoss;
            child.price = 70.;
            h.engine.store_mut().upsert_order(child);
            "parent__stop_loss"
        } else {
            "parent"
        };
        let path = if mode == "cancel" {
            h.command(Command::CancelOrders {
                command: 110,
                ids: vec!["parent".into()],
            });
            "/v5/order/cancel"
        } else {
            h.command(Command::AmendOrder {
                command: 110,
                id: target.into(),
                price: Some(110.),
                amount: None,
            });
            resolve_mode(&mut h, "BTCUSDT");
            "/v5/order/amend"
        };
        let request = h.take(path);
        assert!(!body(&request).contains("orderLinkId"));
        h.reply(
            request.id,
            200,
            r#"{"retCode":0,"result":{"orderId":"parent","orderLinkId":"existing-client"}}"#,
        );
        assert!(h.result(110).unwrap().is_ok(), "{mode}");
        assert!(!h.unknown.contains(&110));
        let order = h.engine.store().order(target).unwrap();
        if mode == "cancel" {
            assert_eq!(order.status, safe_cex_engine::model::OrderStatus::Canceled);
        } else {
            assert_eq!(order.price, 110.);
        }
    }
}

fn managed_margin_reply(body: &str, success: bool, unproven: bool) {
    let mut h = ready(Variant::Demo);
    h.command(Command::SetMarginType {
        command: 111,
        symbol: "BTCUSDT".into(),
        isolated: true,
    });
    let request = h.take("/v5/account/set-margin-mode");
    h.reply(request.id, 200, body);
    assert_eq!(
        h.result(111).expect("one terminal margin result").is_ok(),
        success,
        "{body}"
    );
    assert_eq!(h.unknown.contains(&111), unproven, "{body}");
    assert_eq!(
        h.has_request("/v5/position/list"),
        success,
        "only proved success refreshes positions: {body}"
    );
    assert!(
        !h.has_request("/v5/account/set-margin-mode"),
        "invalid reply must not retry"
    );
}

#[test]
fn managed_margin_rejects_positional_or_malformed_supplied_reasons() {
    for result in [
        "[[]]",
        r#"{"reasons":[["native refusal"]]}"#,
        r#"{"reasons":null}"#,
        r#"{"reasons":{}}"#,
        r#"{"reasons":[{"reasonMsg":1}]}"#,
        r#"{"reasons":[{"reasonCode":1}]}"#,
    ] {
        managed_margin_reply(
            &format!(r#"{{"retCode":0,"result":{result}}}"#),
            false,
            true,
        );
    }
}

#[test]
fn managed_margin_nonempty_reason_cannot_become_success_when_text_is_blank() {
    for reason in [r#"{"reasonCode":"3400000","reasonMsg":""}"#, "{}"] {
        managed_margin_reply(
            &format!(r#"{{"retCode":0,"result":{{"reasons":[{reason}]}}}}"#),
            false,
            true,
        );
    }
}

#[test]
fn managed_margin_preserves_empty_omitted_and_native_rejection_controls() {
    for reply in [
        r#"{"retCode":0,"result":{"reasons":[]}}"#,
        r#"{"retCode":0,"result":{}}"#,
        r#"{"retCode":0}"#,
    ] {
        managed_margin_reply(reply, true, false);
    }
    managed_margin_reply(
        r#"{"retCode":3400045,"retMsg":"Set margin mode failed","result":{"reasons":[{"reasonCode":"3400000","reasonMsg":"Equity too low"}]}}"#,
        false,
        false,
    );
}

#[test]
fn managed_mode_read_rejects_positional_rows_without_seeding_cache_or_advancing_writes() {
    let positional = r#"["BTCUSDT",null,null,null,0]"#;
    for result in [
        format!(r#"[[{positional}]]"#),
        format!(r#"{{"list":[{positional}]}}"#),
        format!(r#"{{"list":[{{"symbol":"BTCUSDT","positionIdx":0}},{positional}]}}"#),
    ] {
        let mut h = ready(Variant::Demo);
        place(&mut h, 112, vec![order("BTCUSDT", 0.01)]);
        h.respond(
            "/v5/position/list",
            200,
            &format!(r#"{{"retCode":0,"result":{result}}}"#),
        );
        assert!(
            !h.has_request("/v5/order/create-batch"),
            "unproven mode cannot advance a write: {result}"
        );
        assert!(h.result(112).expect("mode read fails the command").is_err());
        assert!(
            h.outcomes
                .contains(&(112, Some(safe_cex_engine::Outcome::NotSent)))
        );
        place(&mut h, 113, vec![order("BTCUSDT", 0.01)]);
        assert!(
            h.has_request("/v5/position/list"),
            "invalid prefix must not seed cached mode: {result}"
        );
        resolve_mode(&mut h, "BTCUSDT");
        assert!(h.has_request("/v5/order/create-batch"));
    }
}

fn managed_long_serial_spot_plan(signature_retry: bool) {
    let mut h = ready(Variant::Spot);
    let start = h.now;
    place(&mut h, 114, (0..8).map(|_| order("BTCUSDT", 0.1)).collect());
    for index in 0..8 {
        let mut request = h.take("/v5/order/create");
        assert_eq!(
            request.timeout_ms,
            Some(20_000),
            "the native initial packet retains its physical window, leg {index}"
        );
        assert_eq!(
            h.result(114),
            None,
            "required future slots keep the command open"
        );
        if signature_retry {
            let original_body = request.body.clone();
            h.advance(19_000.);
            h.reply(
                request.id,
                200,
                r#"{"retCode":10004,"retMsg":"error sign"}"#,
            );
            assert_eq!(
                h.result(114),
                None,
                "a definitive signature refusal alone does not retire the plan at leg {index}"
            );
            request = h.take("/v5/order/create");
            assert_eq!(
                request.body, original_body,
                "retry preserves the logical order"
            );
            assert_eq!(
                request.timeout_ms,
                Some(20_000),
                "the native retry gets its fresh physical window, leg {index}"
            );
        }
        h.advance(19_000.);
        assert!(
            !h.has_request("/v5/order/create"),
            "no later logical call may hide expiry of the held packet, leg {index}"
        );
        assert_eq!(
            h.result(114),
            None,
            "valid native physical deadline, leg {index}, retry={signature_retry}"
        );
        h.reply(
            request.id,
            200,
            &format!(r#"{{"retCode":0,"result":{{"orderId":"long-{index}"}}}}"#),
        );
        if index < 7 {
            assert_eq!(
                h.result(114),
                None,
                "no premature completion after an accepted prefix"
            );
        }
    }
    assert_eq!(
        h.now - start,
        if signature_retry { 304_000. } else { 152_000. }
    );
    let expected =
        serde_json::to_string(&(0..8).map(|n| format!("long-{n}")).collect::<Vec<_>>()).unwrap();
    assert_eq!(h.result(114), Some(&Ok(expected)));
    assert!(!h.unknown.contains(&114));
    assert!(!h.has_request("/v5/order/create"));
}

#[test]
fn managed_long_plan_preserves_eight_valid_serial_native_deadlines() {
    managed_long_serial_spot_plan(false);
}

#[test]
fn managed_long_plan_preserves_fresh_physical_signature_retry_windows() {
    managed_long_serial_spot_plan(true);
}

#[test]
fn managed_mode_redos_do_not_start_the_final_write_plan_deadline() {
    let mut h = ready(Variant::Linear);
    let start = h.now;
    place(&mut h, 115, vec![order("BTCUSDT", 0.1)]);
    let path = "/v5/position/list?category=linear&symbol=BTCUSDT";
    for round in 0..7 {
        let old_mode = h.take(path);
        assert_eq!(old_mode.timeout_ms, Some(20_000), "mode redo {round}");
        h.advance(19_000.);
        assert_eq!(
            h.result(115),
            None,
            "bounded read is still live, redo {round}"
        );
        assert!(!h.has_request("/v5/order/create"));
        h.command(Command::SetPositionMode {
            command: 200 + round,
            hedged: round % 2 == 0,
            symbol: Some("BTCUSDT".into()),
        });
        h.respond(
            "/v5/position/switch-mode",
            200,
            r#"{"retCode":0,"result":{}}"#,
        );
        assert_eq!(h.result(200 + round), Some(&Ok("[]".into())));
        h.reply(
            old_mode.id,
            200,
            r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0}]}}"#,
        );
        assert!(
            !h.has_request("/v5/order/create"),
            "old mode version never advances a write"
        );
        assert_eq!(h.result(115), None);
    }
    assert_eq!(h.now - start, 133_000.);
    let current = h.take(path);
    h.reply(
        current.id,
        200,
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0}]}}"#,
    );
    let write = h.take("/v5/order/create-batch");
    assert_eq!(write.timeout_ms, Some(20_000));
    h.reply(
        write.id,
        200,
        r#"{"retCode":0,"result":{"list":[{"orderId":"after-preflight"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
    );
    assert_eq!(h.result(115), Some(&Ok(r#"["after-preflight"]"#.into())));
    assert!(!h.unknown.contains(&115));
}

#[test]
fn managed_preflight_signature_retry_keeps_its_native_physical_deadline() {
    let mut h = ready(Variant::Linear);
    place(&mut h, 116, vec![order("BTCUSDT", 0.1)]);
    let path = "/v5/position/list?category=linear&symbol=BTCUSDT";
    let first = h.take(path);
    assert_eq!(first.timeout_ms, Some(20_000));
    h.advance(19_000.);
    h.reply(first.id, 200, r#"{"retCode":10004,"retMsg":"error sign"}"#);
    let retry = h.take(path);
    assert_eq!(retry.url, first.url);
    assert_eq!(retry.body, first.body);
    assert_eq!(retry.timeout_ms, Some(20_000));
    assert!(!h.has_request("/v5/order/create"));
    h.advance(19_000.);
    assert_eq!(h.result(116), None);
    assert!(!h.has_request(path));
    assert!(!h.has_request("/v5/order/create"));
    h.reply(
        retry.id,
        200,
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0}]}}"#,
    );
    let write = h.take("/v5/order/create-batch");
    assert_eq!(write.timeout_ms, Some(20_000));
    h.reply(
        write.id,
        200,
        r#"{"retCode":0,"result":{"list":[{"orderId":"after-read-retry"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
    );
    assert_eq!(h.result(116), Some(&Ok(r#"["after-read-retry"]"#.into())));
    assert!(!h.unknown.contains(&116));
}

// The legacy pipeline has no engine deadline (the host owns its timeout), so
// these also prove the controls above run on the managed Plan path.
#[test]
fn managed_write_window_expiry_without_a_reply_is_unknown_and_never_resent() {
    let mut h = ready(Variant::Spot);
    place(&mut h, 117, vec![order("BTCUSDT", 0.1)]);
    let create = h.take("/v5/order/create");
    assert_eq!(create.timeout_ms, Some(20_000));
    h.advance(19_999.);
    assert_eq!(h.result(117), None, "the native window is still open");
    h.advance(2.);
    assert!(h.result(117).expect("the window expired").is_err());
    assert!(h.unknown.contains(&117));
    assert!(!h.has_request("/v5/order/create"), "never resent");
    // A late answer after expiry cannot reopen the command.
    h.reply(
        create.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"late"}}"#,
    );
    assert!(h.engine.store().order("late").is_none());
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 117).count(), 1);
}

#[test]
fn managed_mode_read_window_expiry_fails_not_sent_before_any_write() {
    let mut h = ready(Variant::Linear);
    place(&mut h, 118, vec![order("BTCUSDT", 0.1)]);
    let read = h.take("/v5/position/list?category=linear&symbol=BTCUSDT");
    assert_eq!(read.timeout_ms, Some(20_000));
    h.advance(20_001.);
    assert!(h.result(118).expect("the read window expired").is_err());
    assert!(
        h.outcomes
            .contains(&(118, Some(safe_cex_engine::Outcome::NotSent)))
    );
    assert!(!h.has_request("/v5/order/create-batch"));
    h.reply(
        read.id,
        200,
        r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0}]}}"#,
    );
    assert!(
        !h.has_request("/v5/order/create-batch"),
        "a late read plans nothing"
    );
}

/// Bybit's own clock in a 10002 refusal corrects the one native resend.
fn recv_window_refusal(now: f64) -> String {
    let server = now + 600_000.;
    format!(
        r#"{{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param. req_timestamp[{now}],server_timestamp[{server}],recv_window[20000]","result":{{}}}}"#
    )
}

#[test]
fn managed_recv_window_resends_carry_the_venue_clock() {
    // A write leg: the classified grant's resend is re-signed on Bybit's clock.
    let mut h = ready(Variant::Spot);
    place(&mut h, 119, vec![order("BTCUSDT", 0.1)]);
    let create = h.take("/v5/order/create");
    h.reply(create.id, 200, &recv_window_refusal(h.now));
    let resent = h.take("/v5/order/create");
    assert_eq!(resent.body, create.body);
    assert_eq!(
        Harness::header(&resent, "X-BAPI-TIMESTAMP"),
        Some((h.now + 600_000.).floor().to_string()),
        "write resend uses the corrected clock"
    );
    // A mode read: its one native retry is corrected the same way.
    let mut h = ready(Variant::Linear);
    place(&mut h, 120, vec![order("BTCUSDT", 0.1)]);
    let path = "/v5/position/list?category=linear&symbol=BTCUSDT";
    let read = h.take(path);
    h.reply(read.id, 200, &recv_window_refusal(h.now));
    let retry = h.take(path);
    assert_eq!(
        Harness::header(&retry, "X-BAPI-TIMESTAMP"),
        Some((h.now + 600_000.).floor().to_string()),
        "mode-read retry uses the corrected clock"
    );
}

// Hyro H1 original REDs: non-placement Hyro commands run on the ordinary Plan path.

/// H1a: a Hyro cancel is one admitted Plan call with its native window.
#[test]
fn hyro_cancel_runs_on_the_ordinary_plan_with_a_native_window() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 95, vec![order("BTCUSDT", 1.0)]);
    created(&mut h, "plan-cancel");
    h.command(Command::CancelOrders {
        command: 96,
        ids: vec!["plan-cancel".into()],
    });
    let cancel = h.take("/v5/order/cancel");
    assert_eq!(cancel.timeout_ms, Some(20_000), "a managed Plan call");
}

/// H1b: a lost Hyro cancel reconciles with one fresh Hyro snapshot.
#[test]
fn a_lost_hyro_cancel_reconciles_with_a_fresh_snapshot() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 97, vec![order("BTCUSDT", 1.0)]);
    created(&mut h, "lost-cancel");
    h.command(Command::CancelOrders {
        command: 98,
        ids: vec!["lost-cancel".into()],
    });
    let cancel = h.take("/v5/order/cancel");
    h.event(Event::HttpFailed {
        request: cancel.id,
        error: "connection reset",
    });
    assert!(
        h.has_request("/v5/order/realtime"),
        "the uncertain write is reconciled by a fresh order read"
    );
}

/// H1c: host text that resembles a signature refusal never resends a write.
#[test]
fn hyro_cancel_is_never_resent_on_text_alone() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 99, vec![order("BTCUSDT", 1.0)]);
    created(&mut h, "text-cancel");
    h.command(Command::CancelOrders {
        command: 100,
        ids: vec!["text-cancel".into()],
    });
    let cancel = h.take("/v5/order/cancel");
    h.event(Event::HttpFailed {
        request: cancel.id,
        error: "network lost; error sign",
    });
    assert!(!h.has_request("/v5/order/cancel"), "no write resend");
}

/// R2: a duplicate-link refusal on create may mean the order exists, so it is
/// Unknown for Place; on cancel the same code stays a refusal.
#[test]
fn duplicate_link_codes_are_unknown_for_place_only() {
    for code in [10014, 110072] {
        let mut h = ready(Variant::Linear);
        place(&mut h, 120, vec![order("BTCUSDT", 1.0)]);
        while h.has_request("/v5/position/list") {
            h.respond(
                "/v5/position/list",
                200,
                r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0}]}}"#,
            );
        }
        let create = h.take("/v5/order/create");
        h.reply(
            create.id,
            200,
            &format!(r#"{{"retCode":{code},"retMsg":"duplicate"}}"#),
        );
        assert_eq!(
            h.outcomes,
            [(120, Some(safe_cex_engine::Outcome::Unknown))],
            "{code} on create"
        );

        let mut h = ready(Variant::Linear);
        let row = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"dup-cancel","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}"#;
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"order","data":[{row}]}}"#),
        );
        h.command(Command::CancelOrders {
            command: 122,
            ids: vec!["dup-cancel".into()],
        });
        let path = if h.has_request("/v5/order/cancel-batch") {
            "/v5/order/cancel-batch"
        } else {
            "/v5/order/cancel"
        };
        let cancel = h.take(path);
        h.reply(
            cancel.id,
            200,
            &format!(r#"{{"retCode":{code},"retMsg":"duplicate"}}"#),
        );
        assert_eq!(
            h.outcomes,
            [(122, Some(safe_cex_engine::Outcome::Rejected))],
            "{code} on cancel"
        );
    }
}

/// H2: a lost ACK is Unknown at once; one link-filtered reconcile read finds
/// the order, and the caller's retry returns it with no create.
#[test]
fn hyro_lost_ack_is_unknown_and_one_link_read_recovers_for_the_retry() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost acknowledgement",
    });
    assert_eq!(outcome(&h, 1), Some(safe_cex_engine::Outcome::Unknown));
    let filter = format!("orderLinkId={id}");
    let realtime = h.take(&filter);
    assert!(realtime.url.contains("/v5/order/realtime"));
    h.reply(realtime.id, 200, EMPTY);
    let history = h.take(&filter);
    assert!(history.url.contains("/v5/order/history"));
    h.reply(history.id, 200, &lookup_row(&id, "Filled"));
    assert!(!h.has_request(&filter), "one reconcile read per lost ACK");
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    assert!(!h.has_request("/v5/order/create"));
    assert_eq!(h.result(2), Some(&Ok("[\"recovered\"]".into())));
}

/// H2 (Q5), M2: an identical intent arriving while a resume is still
/// re-reading waits for it; nothing is sent in parallel.
#[test]
fn hyro_identical_intent_during_a_resume_read_waits_and_sends_nothing() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    answer_link_absent(&mut h, &id);
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    assert!(
        h.has_request(&format!("orderLinkId={id}")),
        "the resume re-reads"
    );
    place(&mut h, 3, vec![order("BTCUSDT", 1.0)]);
    assert_eq!(outcome(&h, 3), None, "the identical caller waits");
    assert!(!h.has_request("/v5/order/create"));
}

/// H2 (#1939): an ACK that arrives after its create's window sealed Unknown is
/// reconciled into the retained intent; a retry returns it without a resend.
#[test]
fn hyro_late_ack_after_unknown_is_reconciled_without_a_resend() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    h.advance(20_001.0);
    assert_eq!(outcome(&h, 1), Some(safe_cex_engine::Outcome::Unknown));
    h.reply(
        create.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"late"}}"#,
    );
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    while h.has_request("orderLinkId=") {
        h.respond("orderLinkId=", 200, EMPTY);
    }
    assert!(!h.has_request("/v5/order/create"), "never a second create");
    assert_eq!(h.result(2), Some(&Ok("[\"late\"]".into())));
}

fn answer_link_reads(h: &mut Harness, link: &str) {
    let filter = format!("orderLinkId={link}");
    while h.has_request(&filter) {
        h.respond(&filter, 200, EMPTY);
    }
}

/// Review A: a retry's definitive refusal cannot rule out the earlier lost
/// create landing later, so the lot keeps its link (never re-minted) and the
/// call is Unknown; later retries only re-read.
#[test]
fn hyro_refused_retry_after_a_lost_ack_never_mints_a_new_link() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    answer_link_reads(&mut h, &id);
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    answer_link_reads(&mut h, &id);
    let retry = h.take("/v5/order/create");
    assert_eq!(link(&retry), id);
    h.reply(
        retry.id,
        200,
        r#"{"retCode":110007,"retMsg":"insufficient balance"}"#,
    );
    assert_eq!(outcome(&h, 2), Some(safe_cex_engine::Outcome::Unknown));
    answer_link_reads(&mut h, &id);
    place(&mut h, 3, vec![order("BTCUSDT", 1.0)]);
    answer_link_reads(&mut h, &id);
    assert!(
        !h.has_request("/v5/order/create"),
        "no new link is ever sent"
    );
    assert_eq!(outcome(&h, 3), Some(safe_cex_engine::Outcome::Unknown));
}

/// Review 1: an acceptance the private stream observed is never downgraded
/// by a contradictory refusal of the same create.
#[test]
fn hyro_observed_acceptance_outranks_a_later_refusal() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.message("/v5/private", &format!(r#"{{"topic":"order","data":[{{"category":"linear","symbol":"BTCUSDT","orderId":"seen","orderLinkId":"{id}","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}}]}}"#));
    h.reply(
        create.id,
        200,
        r#"{"retCode":110007,"retMsg":"insufficient balance"}"#,
    );
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    assert_eq!(h.result(2), Some(&Ok("[\"seen\"]".into())));
    assert!(!h.has_request("/v5/order/create"));
}

/// Review 3: on a lost ACK every observer gets one error report, then one
/// Unknown terminal naming the link.
#[test]
fn hyro_lost_ack_reports_to_each_observer_before_its_terminal() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 91, vec![order("BTCUSDT", 1.0)]);
    place(&mut h, 92, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    for command in [91, 92] {
        let notices: Vec<_> = h
            .command_notices
            .iter()
            .filter(|(c, _)| *c == command)
            .map(|(_, notice)| *notice)
            .collect();
        assert_eq!(notices, ["error", "done"], "command {command}");
        let errors: Vec<_> = h
            .command_errors
            .iter()
            .filter(|(c, _)| *c == command)
            .collect();
        assert!(errors[0].1.contains(&id));
        assert_eq!(
            outcome(&h, command),
            Some(safe_cex_engine::Outcome::Unknown)
        );
        assert!(
            h.result(command)
                .unwrap()
                .as_ref()
                .unwrap_err()
                .contains(&id)
        );
    }
}

/// A Hyro create refused 10002 is resent once, signed with the clock Bybit
/// named in the refusal.
#[test]
fn hyro_recv_window_refusal_resend_uses_bybit_clock() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    h.reply(create.id, 200, &recv_window_refusal(h.now));
    // The restored one-time resend signs with Bybit's corrected clock.
    let resend = h.take("/v5/order/create");
    assert_eq!(link(&resend), link(&create));
    assert_eq!(
        Harness::header(&resend, "X-BAPI-TIMESTAMP"),
        Some((h.now + 600_000.).floor().to_string())
    );
}

/// Bybit has no symbol-addressed cancel: the command is refused before any
/// I/O instead of finishing without a terminal.
#[test]
fn cancel_orders_with_symbols_is_refused_not_left_open() {
    for variant in [Variant::Linear, Variant::Hyrotrader] {
        let mut h = ready(variant);
        h.requests.clear();
        h.command(Command::CancelOrdersWithSymbols {
            command: 130,
            orders: vec![safe_cex_engine::model::CancelOrderRef {
                id: "any".into(),
                symbol: "BTCUSDT".into(),
            }],
        });
        let failure = h.notices.iter().find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed {
                command: 130,
                error,
            } => Some(error),
            _ => None,
        });
        assert_eq!(
            failure.and_then(|error| error.kind),
            Some(safe_cex_engine::ErrorKind::Unsupported),
            "{variant:?}: {failure:?}"
        );
        assert_eq!(
            h.outcomes,
            [(130, Some(safe_cex_engine::Outcome::NotSent))],
            "{variant:?}"
        );
        assert!(
            !h.paths().iter().any(|p| p.contains("/v5/order/")),
            "{variant:?}"
        );
    }
}

/// Review M1 (#2106): a retry whose remaining lot is refused again cannot
/// report Rejected or NotSent while an earlier call's lot is live on Hyro;
/// a client would read either as nothing placed and send a replacement.
#[test]
fn hyro_retry_refused_beside_a_live_lot_is_never_rejected_or_not_sent() {
    let mut h = ready(Variant::Hyrotrader);
    let orders = vec![order("BTCUSDT", 1.0), order("ETHUSDT", 1.0)];
    place(&mut h, 1, orders.clone());
    created(&mut h, "live");
    let refused = h.take("/v5/order/create");
    let id = link(&refused);
    h.reply(
        refused.id,
        200,
        r#"{"retCode":110007,"retMsg":"insufficient balance"}"#,
    );
    answer_link_reads(&mut h, &id);
    place(&mut h, 2, orders);
    answer_link_reads(&mut h, &id);
    let retry = h.take("/v5/order/create");
    h.reply(
        retry.id,
        200,
        r#"{"retCode":110007,"retMsg":"insufficient balance"}"#,
    );
    answer_link_reads(&mut h, &id);
    let sealed = outcome(&h, 2);
    assert!(sealed.is_some(), "the retry has one terminal");
    assert!(
        !matches!(
            sealed,
            Some(safe_cex_engine::Outcome::Rejected | safe_cex_engine::Outcome::NotSent)
        ),
        "lot 0 is live: {sealed:?}"
    );
}

/// Review M1 (#2106): the reconnecting refusal of a retry's remaining lot is
/// not NotSent while an earlier call's lot is live on Hyro.
#[test]
fn hyro_retry_while_reconnecting_beside_a_live_lot_is_never_not_sent() {
    let mut h = ready(Variant::Hyrotrader);
    let orders = vec![order("BTCUSDT", 1.0), order("ETHUSDT", 1.0)];
    place(&mut h, 1, orders.clone());
    created(&mut h, "live");
    let refused = h.take("/v5/order/create");
    let id = link(&refused);
    h.reply(
        refused.id,
        200,
        r#"{"retCode":110007,"retMsg":"insufficient balance"}"#,
    );
    answer_link_reads(&mut h, &id);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    place(&mut h, 2, orders);
    assert!(!h.has_request("/v5/order/create"), "nothing is sent");
    let sealed = outcome(&h, 2);
    assert!(sealed.is_some(), "the retry has one terminal");
    assert_ne!(
        sealed,
        Some(safe_cex_engine::Outcome::NotSent),
        "lot 0 is live"
    );
}

/// Review M2 (#2106): an identical caller arriving while the founder's link
/// reads run waits for the founder and gets its outcome, as legacy waiters did.
fn reconciling_founder(h: &mut Harness) -> String {
    place(h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost acknowledgement",
    });
    place(h, 2, vec![order("BTCUSDT", 1.0)]);
    assert!(
        h.has_request(&format!("orderLinkId={id}")),
        "the founder re-reads"
    );
    id
}

#[test]
fn hyro_identical_caller_during_reconcile_gets_the_founders_success() {
    let mut h = ready(Variant::Hyrotrader);
    let id = reconciling_founder(&mut h);
    place(&mut h, 3, vec![order("BTCUSDT", 1.0)]);
    assert_eq!(outcome(&h, 3), None, "the waiter is parked, not refused");
    while h.has_request("/v5/order/realtime") {
        h.respond("/v5/order/realtime", 200, EMPTY);
    }
    while h.has_request("/v5/order/history") {
        h.respond("/v5/order/history", 200, &lookup_row(&id, "Filled"));
    }
    assert_eq!(h.result(2), Some(&Ok("[\"recovered\"]".into())));
    assert_eq!(h.result(3), Some(&Ok("[\"recovered\"]".into())));
    assert!(
        !h.has_request("/v5/order/create"),
        "nothing is sent for the waiter"
    );
}

#[test]
fn hyro_waiter_whose_deadline_fires_first_is_unknown_never_not_sent() {
    let mut h = ready(Variant::Hyrotrader);
    reconciling_founder(&mut h);
    place(&mut h, 3, vec![order("BTCUSDT", 1.0)]);
    h.advance(600_000.0);
    assert_eq!(outcome(&h, 3), Some(safe_cex_engine::Outcome::Unknown));
    assert!(
        !h.has_request("/v5/order/create"),
        "nothing is sent for the waiter"
    );
}

#[test]
fn hyro_waiter_takes_its_own_founders_outcome_not_a_later_founders() {
    let mut h = ready(Variant::Hyrotrader);
    let id = reconciling_founder(&mut h);
    place(&mut h, 3, vec![order("BTCUSDT", 1.0)]);
    // The founder's reads fail: it seals Unknown and the intent stays.
    let filter = format!("orderLinkId={id}");
    while h.has_request(&filter) {
        h.respond(&filter, 502, "upstream timeout");
    }
    assert_eq!(outcome(&h, 2), Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(outcome(&h, 3), Some(safe_cex_engine::Outcome::Unknown));
    // A later founder of the same intent proves the order; the waiter is unmoved.
    place(&mut h, 4, vec![order("BTCUSDT", 1.0)]);
    while h.has_request("/v5/order/realtime") {
        h.respond("/v5/order/realtime", 200, EMPTY);
    }
    while h.has_request("/v5/order/history") {
        h.respond("/v5/order/history", 200, &lookup_row(&id, "Filled"));
    }
    assert_eq!(h.result(4), Some(&Ok("[\"recovered\"]".into())));
    assert_eq!(outcome(&h, 3), Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(
        h.done.iter().filter(|(c, _)| *c == 3).count(),
        1,
        "one terminal"
    );
}

/// TS keeps its local REST order lifecycle for Bybit Brazil (`BRA_BTL`) as for
/// Demo, until the proxied private WS is venue-validated; plain live Bybit
/// leaves order rows to the private stream.
#[test]
fn brazil_limit_ack_publishes_locally_but_plain_linear_waits_for_the_stream() {
    for (site, local) in [(Some("BRA_BTL"), true), (None, false)] {
        let mut h = ready_with(Options {
            bybit_site_id: site.map(Into::into),
            ..options(Variant::Linear)
        });
        place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
        resolve_mode(&mut h, "BTCUSDT");
        h.respond("/v5/order/create-batch", 200, r#"{"retCode":0,"result":{"list":[{"orderId":"local"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
        assert_eq!(
            h.engine.store().order("local").is_some(),
            local,
            "site {site:?}"
        );
    }
}
