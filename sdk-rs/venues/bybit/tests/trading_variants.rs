//! Variant order semantics from the V5 create/cancel specifications and the
//! documented Hyro simulator identity/reconciliation contract.
mod support;
use safe_cex_engine::model::{BybitBbo, OrderSide, OrderType, PlaceOrderOpts};
use safe_cex_engine::{Command, Event, HttpRequest, Query};
use safe_cex_venue_bybit::{Endpoints, Options, Variant};
use support::Harness;

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
    let mut h = Harness::started(options(variant));
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
    } else if variant == Variant::Demo {
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
    });
    assert!(h.result(3).unwrap().is_err());
    h.command(Command::SetLeverage {
        command: 4,
        symbol: "BTCUSDT".into(),
        leverage: 3.0,
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
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/order/history", 200, &lookup_row(&id, "Filled"));
    assert_eq!(h.result(1), Some(&Ok("[\"recovered\"]".into())));
    assert!(h.engine.store().order("recovered").is_none());
    assert!(h.requests.is_empty());
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
    assert!(h.requests.is_empty());
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
    h.respond(
        "/v5/order/realtime",
        500,
        r#"{"retCode":10016,"retMsg":"unavailable"}"#,
    );
    assert!(h.result(1).unwrap().as_ref().unwrap_err().contains(&id));
    assert!(h.requests.is_empty());
    place(&mut h, 2, orders);
    assert!(!h.has_request("/v5/order/create"));
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/order/history", 200, EMPTY);
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
        assert!(h.requests.is_empty());
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
    assert!(h.result(1).is_none());
    assert!(h.engine.store().order("accepted").is_none());
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/order/history", 200, EMPTY);
    assert_eq!(h.result(1), Some(&Ok("[\"accepted\"]".into())));
    assert!(h.engine.store().order("accepted").is_none());
    assert!(h.requests.is_empty());
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
    place(&mut h, 2, vec![opts]);
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/order/history", 200, EMPTY);
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
    h.message("/v5/private", &format!(r#"{{"topic":"order","data":[{{"category":"linear","symbol":"BTCUSDT","orderId":"recovered","orderLinkId":"{id}","orderStatus":"Filled","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"1","leavesQty":"0"}}]}}"#));
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    h.respond("/v5/order/realtime", 200, &lookup_row(&id, "New"));
    assert_eq!(
        h.engine.store().order("recovered").unwrap().status,
        safe_cex_engine::model::OrderStatus::Closed
    );
    assert_eq!(h.result(1), Some(&Ok("[\"recovered\"]".into())));
}

#[test]
fn hyro_two_complete_absences_reuse_original_id_but_never_loop_writes() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/order/history", 200, EMPTY);
    let retry = h.take("/v5/order/create");
    assert_eq!(link(&retry), id);
    h.event(Event::HttpFailed {
        request: retry.id,
        error: "lost again",
    });
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/order/history", 200, EMPTY);
    assert!(h.result(1).unwrap().as_ref().unwrap_err().contains(&id));
    assert!(h.requests.is_empty());
}

#[test]
fn hyro_cyclic_lookup_is_incomplete_and_keeps_unknown_outcome() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    let page = r#"{"retCode":0,"result":{"list":[],"nextPageCursor":"same"}}"#;
    h.respond("/v5/order/realtime", 200, page);
    h.respond("/v5/order/realtime", 200, page);
    assert!(h.result(1).unwrap().is_err());
    assert!(h.requests.is_empty());
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
fn demo_cancel_all_retires_every_captured_row_including_old_terminal_rows() {
    let mut h = ready(Variant::Demo);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"old-filled","orderStatus":"Filled","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"1","leavesQty":"0"},{"category":"linear","symbol":"BTCUSDT","orderId":"resting","orderStatus":"New","orderType":"Limit","side":"Buy","price":"99","qty":"1","cumExecQty":"0","leavesQty":"1"}]}"#);
    assert_eq!(h.engine.store().orders().len(), 2);
    h.command(Command::CancelSymbolOrders {
        command: 1,
        symbol: "BTCUSDT".into(),
    });
    h.respond("/v5/order/cancel-all", 200, r#"{"retCode":0,"result":{}}"#);
    assert!(h.engine.store().orders().is_empty());
    assert!(h.requests.is_empty());
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
            assert!(h.requests.is_empty());
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
                r#"{"retCode":0,"result":{"list":[],"success":"1"}}"#
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
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/order/history", 200, EMPTY);
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
