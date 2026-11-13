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
    ready_listing(options, EMPTY)
}

fn ready_listing(options: Options, open_orders: &str) -> Harness {
    ready_listing_with_instruments(options, open_orders, None)
}

fn ready_listing_with_instruments(
    options: Options,
    open_orders: &str,
    instruments_override: Option<String>,
) -> Harness {
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
    h.respond(
        "/v5/market/instruments-info",
        200,
        &instruments_override.unwrap_or(instruments),
    );
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
        h.respond("/v5/order/realtime", 200, open_orders);
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
    if open_orders != EMPTY {
        while h.has_request("/v5/order/realtime") {
            h.respond("/v5/order/realtime", 200, open_orders);
        }
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

#[test]
fn second_review_perp_remainder_keeps_ts_protection() {
    // reference/bybit/bybit.exchange.ts:2723-2731 restores the original request on the remainder.
    // KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
    for (amount, expected, sum) in [
        (26., &["4", "4", "4", "4", "4", "4", "1"][..], 25.),
        (10., &["5", "5"][..], 10.),
    ] {
        let mut h = ready(Variant::Linear);
        let mut markets = h.engine.store().markets().to_vec();
        let market = markets.iter_mut().find(|m| m.symbol == "BTCUSDT").unwrap();
        market.precision.amount = 1.;
        market.limits.amount.max = 5.;
        market.limits.amount.min = 1.;
        h.engine.store_mut().set_markets(markets);
        let mut opts = order("BTCUSDT", amount);
        opts.stop_loss = Some(90.);
        opts.take_profit = Some(110.);
        place(&mut h, 901, vec![opts]);
        resolve_mode(&mut h, "BTCUSDT");
        let request = h.take("/v5/order/create-batch");
        let sent: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        let rows = sent["request"].as_array().unwrap();
        let quantities: Vec<_> = rows
            .iter()
            .map(|row| row["qty"].as_str().unwrap())
            .collect();
        assert_eq!(quantities, expected);
        assert_eq!(
            quantities
                .iter()
                .map(|q| q.parse::<f64>().unwrap())
                .sum::<f64>(),
            sum
        );
        for (index, row) in rows.iter().enumerate() {
            let protected = index == 0 || (amount == 26. && index == rows.len() - 1);
            for (key, value) in [
                ("stopLoss", "90"),
                ("takeProfit", "110"),
                ("slTriggerBy", "MarkPrice"),
                ("tpTriggerBy", "LastPrice"),
            ] {
                assert_eq!(
                    row.get(key).and_then(|v| v.as_str()),
                    protected.then_some(value),
                    "amount={amount}, row={index}, key={key}"
                );
            }
        }
    }
}

#[test]
fn second_review_native_unknown_emits_telemetry_and_journal() {
    for code in [10016, 110007] {
        let mut h = ready(Variant::Spot);
        place(&mut h, 902, vec![order("BTCUSDT", 1.); 3]);
        let prefix = h.take("/v5/order/create");
        h.reply(
            prefix.id,
            200,
            r#"{"retCode":0,"result":{"orderId":"known-prefix"}}"#,
        );
        let uncertain = h.take("/v5/order/create");
        h.reply(
            uncertain.id,
            200,
            &serde_json::json!({"retCode":code,"retMsg":"native result"}).to_string(),
        );
        if code == 10016 {
            support::assert_unknown_pin(&mut h, 902, &["known-prefix"], 1, 3);
            let events: Vec<_> = h
                .notices
                .iter()
                .filter_map(|notice| match notice {
                    safe_cex_engine::Notice::Event(event)
                        if event.code == "order.outcome_unknown" =>
                    {
                        Some(event)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                events.len(),
                1,
                "HTTP-200 native Unknown must emit telemetry"
            );
            assert_eq!(events[0].level, safe_cex_engine::log::Level::Warn);
            assert!(events[0].message.contains(support::ENGINE_UNKNOWN_TEXT));
            assert!(events[0].fields.iter().any(|(key, value)| *key == "command"
                && *value == safe_cex_engine::log::Value::Num(902.)));
            let error = applied_failure(&h, 902);
            let safe_cex_engine::write_plan::ElementResult::Unknown { error } =
                &error.elements.as_ref().unwrap()[1].result
            else {
                panic!("native Unknown journal row")
            };
            assert!(error.message.contains(support::ENGINE_UNKNOWN_TEXT));
        } else {
            assert!(!h.notices.iter().any(|notice| matches!(notice, safe_cex_engine::Notice::Event(event) if event.code == "order.outcome_unknown")));
            assert!(!h.unknown.contains(&902));
        }
    }
}

fn assert_allocation_quantities(
    variant: Variant,
    amount: f64,
    max: f64,
    step: f64,
    expected: &[&str],
    expected_sum: f64,
) {
    let mut h = ready(variant);
    let mut markets = h.engine.store().markets().to_vec();
    let market = markets.iter_mut().find(|m| m.symbol == "BTCUSDT").unwrap();
    market.precision.amount = step;
    market.limits.amount.max = max;
    market.limits.amount.min = step;
    h.engine.store_mut().set_markets(markets);
    place(&mut h, 900, vec![order("BTCUSDT", amount)]);
    if h.has_request("/v5/position/list") {
        resolve_mode(&mut h, "BTCUSDT");
    }
    let mut quantities = Vec::new();
    let mut ids = Vec::new();
    while h.has_request("/v5/order/create") {
        assert!(ids.len() < 30, "unexpected additional allocation writes");
        let batch = h.has_request("/v5/order/create-batch");
        let request = h.take(if batch {
            "/v5/order/create-batch"
        } else {
            "/v5/order/create"
        });
        let sent: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        let rows = if batch {
            sent["request"].as_array().unwrap().clone()
        } else {
            vec![sent]
        };
        let mut results = Vec::new();
        for row in &rows {
            quantities.push(row["qty"].as_str().unwrap().to_owned());
            let id = format!("lot-{}", ids.len());
            results.push(serde_json::json!({"orderId": id}));
            ids.push(id);
        }
        let response = if batch {
            let statuses: Vec<_> = rows
                .iter()
                .map(|_| serde_json::json!({"code":0,"msg":"OK"}))
                .collect();
            serde_json::json!({"retCode":0,"result":{"list":results},"retExtInfo":{"list":statuses}})
        } else {
            serde_json::json!({"retCode":0,"result":results[0]})
        };
        h.reply(request.id, 200, &response.to_string());
        h.advance(25.0);
    }
    assert_eq!(
        quantities, expected,
        "{variant:?}: amount={amount}, max={max}, step={step}"
    );
    let total: f64 = quantities.iter().map(|q| q.parse::<f64>().unwrap()).sum();
    assert_eq!(safe_cex_engine::num::adjust(total, step), expected_sum);
    assert_eq!(
        h.result(900),
        Some(&Ok(serde_json::to_string(&ids).unwrap()))
    );
    assert!(!h.unknown.contains(&900));
    assert!(!h.has_request("/v5/order/create"));
}

#[test]
fn perp_lot_allocation_matches_legacy_ts_audit() {
    // reference/bybit/bybit.exchange.ts:2718-2731, including rounded totals below the requested amount.
    for (amount, max, step, expected, expected_sum) in [
        // KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
        (
            26.0,
            5.0,
            1.0,
            &["4", "4", "4", "4", "4", "4", "1"][..],
            25.,
        ),
        // KNOWN-ISSUE: legacy TS under-allocates (sum 10 of requested 11); kept for exact TS parity per Sam's decision
        (11.0, 5.0, 1.0, &["3", "3", "3", "1"][..], 10.),
        // KNOWN-ISSUE: legacy TS under-allocates (sum 11 of requested 12); kept for exact TS parity per Sam's decision
        (12.0, 5.0, 1.0, &["3", "3", "3", "2"][..], 11.),
        (10.0, 5.0, 1.0, &["5", "5"][..], 10.),
        (7.0, 3.0, 0.5, &["2", "2", "2", "1"][..], 7.),
        (
            1.0005,
            0.3,
            0.001,
            &["0.225", "0.225", "0.225", "0.225", "0.101"][..],
            1.001,
        ),
    ] {
        assert_allocation_quantities(Variant::Linear, amount, max, step, expected, expected_sum);
    }
}

#[test]
fn spot_lot_allocation_retains_incoming_spot_parity() {
    for (amount, max, step, expected, expected_sum) in [
        // KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
        (
            26.0,
            5.0,
            1.0,
            &["4", "4", "4", "4", "4", "4", "1"][..],
            25.,
        ),
        // KNOWN-ISSUE: legacy TS under-allocates (sum 10 of requested 11); kept for exact TS parity per Sam's decision
        (11.0, 5.0, 1.0, &["3", "3", "3", "1"][..], 10.),
        // KNOWN-ISSUE: legacy TS under-allocates (sum 11 of requested 12); kept for exact TS parity per Sam's decision
        (12.0, 5.0, 1.0, &["3", "3", "3", "2"][..], 11.),
        (10.0, 5.0, 1.0, &["5", "5"][..], 10.),
        (7.0, 3.0, 0.5, &["2", "2", "2", "1"][..], 7.),
        (
            1.0005,
            0.3,
            0.001,
            &["0.225", "0.225", "0.225", "0.225", "0.101"][..],
            1.001,
        ),
    ] {
        assert_allocation_quantities(Variant::Spot, amount, max, step, expected, expected_sum);
    }
}

#[test]
fn managed_split_market_close_accepts_server_echoes_without_caller_ids() {
    // Equal TS lots plus remainder under the 500-unit market cap.
    for (amount, quantities) in [
        (900.0, &["250", "250", "400"][..]),
        (1400.0, &["333.333", "333.333", "333.333", "400"][..]),
    ] {
        let mut h = ready(Variant::Demo);
        let mut opts = order("BTCUSDT", amount);
        opts.kind = OrderType::Market;
        opts.reduce_only = Some(true);
        place(&mut h, 130, vec![opts]);
        resolve_mode(&mut h, "BTCUSDT");
        let request = h.take("/v5/order/create-batch");
        let sent: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        let children = sent["request"].as_array().unwrap();
        let count = quantities.len();
        assert_eq!(
            children
                .iter()
                .map(|child| child["qty"].as_str().unwrap())
                .collect::<Vec<_>>(),
            quantities
        );
        // KNOWN-ISSUE: legacy TS under-allocates (sum 1399.999 of requested 1400); kept for exact TS parity per Sam's decision
        let total: f64 = children
            .iter()
            .map(|child| child["qty"].as_str().unwrap().parse::<f64>().unwrap())
            .sum();
        assert_eq!(
            safe_cex_engine::num::adjust(total, 0.001),
            if amount == 1400. { 1399.999 } else { 900. }
        );
        assert!(
            children
                .iter()
                .all(|child| child["reduceOnly"] == true && child.get("orderLinkId").is_none())
        );
        let ids: Vec<String> = (0..count).map(|index| format!("native-{index}")).collect();
        let results: Vec<_> = ids
            .iter()
            .map(|id| serde_json::json!({"orderId":id,"orderLinkId":format!("server-{id}")}))
            .collect();
        let statuses: Vec<_> = ids
            .iter()
            .map(|_| serde_json::json!({"code":0,"msg":"OK"}))
            .collect();
        h.reply(request.id, 200, &serde_json::json!({"retCode":0,"result":{"list":results},"retExtInfo":{"list":statuses}}).to_string());
        assert_eq!(
            h.result(130),
            Some(&Ok(serde_json::to_string(&ids).unwrap()))
        );
        assert!(!h.unknown.contains(&130));
        assert!(!h.has_request("/v5/order/create"));
    }
}

#[test]
fn managed_create_batch_conflicting_echoes_remain_unknown_without_publication_or_resend() {
    for (first_echo, second_echo, requested) in [
        ("caller-b", "caller-b", Some("caller-b")),
        ("caller-b", "", Some("caller-b")),
        ("same", "same", None),
    ] {
        let mut h = ready(Variant::Demo);
        let mut second = order("BTCUSDT", 0.02);
        second.client_order_id = requested.map(str::to_owned);
        place(&mut h, 132, vec![order("BTCUSDT", 0.01), second]);
        resolve_mode(&mut h, "BTCUSDT");
        let request = h.take("/v5/order/create-batch");
        h.reply(request.id, 200, &serde_json::json!({"retCode":0,"result":{"list":[
            {"orderId":"first","orderLinkId":first_echo},{"orderId":"second","orderLinkId":second_echo}
        ]},"retExtInfo":{"list":[{"code":0},{"code":0}]}}).to_string());
        assert!(h.result(132).unwrap().is_err());
        assert!(h.unknown.contains(&132));
        assert!(h.engine.store().order("first").is_none());
        assert!(h.engine.store().order("second").is_none());
        assert!(!h.has_request("/v5/order/create"));
        assert!(!h.has_request("/v5/order/create-batch"));
    }
}

#[test]
fn managed_unproven_create_batch_reports_redacted_child_shape_without_values() {
    let mut h = ready(Variant::Demo);
    place(
        &mut h,
        131,
        vec![order("BTCUSDT", 0.01), order("BTCUSDT", 0.02)],
    );
    resolve_mode(&mut h, "BTCUSDT");
    let request = h.take("/v5/order/create-batch");
    h.reply(request.id, 200, r#"{"retCode":0,"result":{"list":[{"orderId":"PRIVATE_NATIVE","orderLinkId":"PRIVATE_LINK"},{"orderId":"SECOND_NATIVE"}]},"retExtInfo":{"list":[{"code":0,"msg":"PRIVATE_MESSAGE"},{"code":10001,"msg":"PRIVATE_REASON"}]}}"#);
    let error = h
        .notices
        .iter()
        .find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed {
                command: 131,
                error,
            } => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        error.partial.as_ref().unwrap().order_ids,
        ["PRIVATE_NATIVE", "SECOND_NATIVE"]
    );
    let error = &error.message;
    assert!(error.contains("result.list=array/2"), "{error}");
    assert!(error.contains("retExtInfo.list=array/2"), "{error}");
    assert!(error.contains("code=10001"), "{error}");
    for secret in [
        "PRIVATE_NATIVE",
        "PRIVATE_LINK",
        "SECOND_NATIVE",
        "PRIVATE_MESSAGE",
        "PRIVATE_REASON",
    ] {
        assert!(!error.contains(secret), "{error}");
    }
    assert!(h.unknown.contains(&131));
    assert!(!h.has_request("/v5/order/create"));
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
fn second_review_inverse_clamps_to_ts_maximum() {
    // reference/bybit/bybitinverse.exchange.ts:1523-1541 clamps before :1614-1636 allocates.
    // KNOWN-ISSUE: legacy TS under-allocates (sum 10 of requested 25.6); kept for exact TS parity per Sam's decision
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
    let sent: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
    assert_eq!(
        sent["request"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["qty"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["10"]
    );
    let total: f64 = sent["request"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["qty"].as_str().unwrap().parse::<f64>().unwrap())
        .sum();
    assert_eq!(total, 10.);
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

fn spot_split_plan_market(minimum: &str, maximum: &str) -> Harness {
    let instruments = support::INSTRUMENTS
        .replace(
            "\"minOrderQty\":\"0.001\"",
            &format!("\"minOrderQty\":\"{minimum}\""),
        )
        .replace(
            "\"maxOrderQty\":\"1190.000\"",
            &format!("\"maxOrderQty\":\"{maximum}\""),
        );
    let h = ready_listing_with_instruments(options(Variant::Spot), EMPTY, Some(instruments));
    let market = h.engine.store().market("BTCUSDT").expect("fixture market");
    assert_eq!(
        market.limits.amount.min,
        minimum.parse::<f64>().expect("fixture minimum")
    );
    assert_eq!(
        market.limits.amount.max,
        maximum.parse::<f64>().expect("fixture maximum")
    );
    assert_eq!(market.precision.amount, 0.001);
    h
}

fn protected_spot_split_order(amount: f64) -> PlaceOrderOpts {
    let mut opts = order("BTCUSDT", amount);
    opts.take_profit = Some(110.);
    opts.stop_loss = Some(90.);
    opts
}

fn assert_spot_split_plan_quantities(amount: f64, quantities: &[&str]) {
    let mut h = spot_split_plan_market("0.01", "1190");
    place(&mut h, 92, vec![protected_spot_split_order(amount)]);
    let mut total = 0.;
    for (index, quantity) in quantities.iter().enumerate() {
        let request = h.take("/v5/order/create");
        let sent: serde_json::Value = serde_json::from_str(body(&request)).expect("create body");
        assert_eq!(sent["qty"], *quantity);
        let protected = index == 0 || index + 1 == quantities.len();
        for field in ["takeProfit", "stopLoss", "tpOrderType", "slOrderType"] {
            assert_eq!(sent.get(field).is_some(), protected, "{field} lot {index}");
        }
        let qty = quantity.parse::<f64>().expect("fixture quantity");
        assert!((0.01..=1190.).contains(&qty));
        assert_eq!(safe_cex_engine::num::adjust(qty, 0.001), qty);
        total += qty;
        h.reply(
            request.id,
            200,
            &serde_json::json!({
                "retCode": 0, "result": {"orderId": format!("lot-{index}")}
            })
            .to_string(),
        );
    }
    assert_eq!(total, amount);
    assert!(matches!(h.result(92), Some(Ok(_))));
    assert!(h.requests.is_empty());
}

#[test]
fn spot_split_plan_exact_maximum_keeps_both_stops() {
    assert_spot_split_plan_quantities(1190., &["1190"]);
}

#[test]
fn spot_split_plan_exact_minimum_tail_uses_legacy_main_lots() {
    assert_spot_split_plan_quantities(1190.01, &["595", "595", "0.01"]);
}

#[test]
fn spot_ts_parity_does_not_preflight_small_main_lots_or_off_grid_caps() {
    // TS reference/bybit/bybitspot.exchange.ts:828-841 rounds each main lot independently.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit-spot | split-tail
    for (minimum, maximum, amount, quantities, expected_sum) in [
        ("0.75", "1", 1.001, vec!["0.5", "0.5", "0.001"], 1.001),
        (
            "0.01",
            "1190.0005",
            1190.02,
            vec!["595.001", "595.001", "0.019"],
            1190.021,
        ),
        // KNOWN-ISSUE: legacy TS under-allocates (sum 0 of requested 0.002); kept for exact TS parity per Sam's decision
        ("0.0001", "0.0004", 0.002, vec!["0"; 5], 0.0),
    ] {
        let mut h = spot_split_plan_market(minimum, maximum);
        place(&mut h, 196, vec![protected_spot_split_order(amount)]);
        let mut sum = 0.0;
        for (index, quantity) in quantities.iter().enumerate() {
            let request = h.take("/v5/order/create");
            let wire: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
            assert_eq!(wire["qty"], *quantity);
            sum += wire["qty"].as_str().unwrap().parse::<f64>().unwrap();
            h.reply(
                request.id,
                200,
                &serde_json::json!({"retCode":0,"result":{"orderId":format!("wire-{index}")}})
                    .to_string(),
            );
        }
        assert!(matches!(h.result(196), Some(Ok(_))));
        assert!(
            (sum - expected_sum).abs() < 1e-9,
            "sum {sum}, expected {expected_sum}"
        );
        assert!(!h.has_request("/v5/order/create"));
    }
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
        if h.has_request("/v5/order/realtime") {
            // Failed scope proof now seals Unknown and requests the engine's fresh read.
            assert_eq!(outcome(&h, 92), Some(safe_cex_engine::Outcome::Unknown));
            assert!(h.has_request("/v5/order/realtime"));
            assert!(
                h.requests
                    .iter()
                    .all(|r| r.method == safe_cex_engine::Method::Get)
            );
            h.respond(
                "settleCoin=USDT&limit=50",
                200,
                &format!(r#"{{"retCode":0,"result":{{"list":[{row}],"nextPageCursor":""}}}}"#),
            );
            h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
        }
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
    assert!(!h.notices.iter().any(|n| matches!(n, safe_cex_engine::Notice::Event(event) if event.code == "order.outcome_unknown")));
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
fn cancel_all_failed_read_retains_unknown_scope_and_warning() {
    use safe_cex_engine::write_plan::ElementResult;
    use safe_cex_engine::{Notice, Outcome};
    let mut h = cancel_all_omitting_a_stop(998);
    h.respond(
        "settleCoin=USDT&limit=50",
        200,
        r#"{"retCode":0,"result":{}}"#,
    );
    h.advance(0.0);
    let error = applied_failure(&h, 998);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(
        error
            .message
            .contains("The outcome is unknown. Check Open Orders")
    );
    let rows = error.elements.as_ref().expect("cancel-all scope journal");
    assert_eq!(rows.len(), 1);
    assert!(
        matches!(&rows[0].result, ElementResult::Unknown { error } if error.outcome == Some(Outcome::Unknown))
    );
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|p| p.order_ids.is_empty())
    );
    let events: Vec<_> = h
        .notices
        .iter()
        .filter_map(|n| match n {
            Notice::Event(event) if event.code == "order.outcome_unknown" => Some(event),
            _ => None,
        })
        .collect();
    assert_eq!(events.len(), 1);
    assert!(
        events[0]
            .fields
            .iter()
            .any(|(key, value)| *key == "command" && *value == 998u32.into())
    );
    assert_eq!(
        h.engine.store().order("stop-1").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
    h.advance(120_000.);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.method != safe_cex_engine::Method::Get)
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
            if failed == Some(filter) {
                break;
            }
        }
        for (id, filter) in [
            ("ordinary", "Order"),
            ("ordinary__stop_loss", "Order"),
            ("conditional", "StopOrder"),
            ("stop", "tpslOrder"),
        ] {
            assert_eq!(
                h.engine.store().order(id).unwrap().status,
                if failed.is_some_and(|failed| {
                    let filters = ["Order", "StopOrder", "tpslOrder"];
                    filters.iter().position(|item| *item == filter)
                        >= filters.iter().position(|item| *item == failed)
                }) {
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
    assert!(original.contains("\"triggerDirection\":1"));
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

fn assert_native_rate_limit_create_unknown(code: i64) {
    for variant in [Variant::Spot, Variant::Demo] {
        let mut h = ready(variant);
        place(&mut h, 9069, vec![order("BTCUSDT", 0.1)]);
        let path = if variant.is_spot() {
            "/v5/order/create"
        } else {
            resolve_mode(&mut h, "BTCUSDT");
            "/v5/order/create-batch"
        };
        let request = h.take(path);
        h.reply(
            request.id,
            200,
            &format!(r#"{{"retCode":{code},"retMsg":"rate limited","result":{{}}}}"#),
        );
        let error = h
            .notices
            .iter()
            .find_map(|notice| match notice {
                safe_cex_engine::Notice::CommandFailed {
                    command: 9069,
                    error,
                } => Some(error),
                _ => None,
            })
            .expect("one terminal create failure");
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::RateLimited));
        assert_eq!(error.venue_code, Some(code.to_string()));
        h.advance(1500.0);
        assert!(
            !h.has_request("/v5/order/create"),
            "a released rate-limited create cannot fall back or resend"
        );
    }
}

#[test]
fn native_system_frequency_limit_keeps_released_creates_unknown() {
    assert_native_rate_limit_create_unknown(429);
}

#[test]
fn native_legacy_ip_limit_keeps_released_creates_unknown() {
    assert_native_rate_limit_create_unknown(10018);
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
        let mut first = order("BTCUSDT", 0.01);
        if case == 4 {
            first.client_order_id = Some("requested-echo".into());
        }
        place(&mut h, 96, vec![first, order("BTCUSDT", 0.02)]);
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
fn managed_not_modified_amend_succeeds_and_publishes_no_new_value() {
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
        assert_eq!(h.result(106), Some(&Ok("[]".into())));
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
    assert_eq!(
        h.requests
            .iter()
            .filter(|request| request.url.contains("/v5/order/create"))
            .count(),
        1
    );
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    let expected = format!(
        "The outcome is unknown. Check Open Orders. HyroTrader order outcome is unknown (orderLinkId {id}). Reconcile before placing a replacement: lost"
    );
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
        assert_eq!(errors.len(), 1, "command {command}");
        assert_eq!(errors[0].1, expected, "command {command}");
        assert!(errors[0].1.contains(&id));
        let failure = applied_failure(&h, command);
        assert_eq!(failure.message, expected, "command {command}");
        assert!(failure.presents, "command {command}");
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
    assert!(!h.has_request("/v5/order/create"));
    h.advance(60_000.0);
    assert!(!h.has_request("/v5/order/create"));
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

/// An order the store has lost is still cancelled, by the id and symbol given.
#[test]
fn cancel_orders_with_symbols_cancels_an_order_the_store_has_lost() {
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
        assert!(
            h.requests.iter().any(|r| r.url.contains("/v5/order/cancel")
                && r.body
                    .as_deref()
                    .is_some_and(|b| b.contains("any") && b.contains("BTCUSDT"))),
            "{variant:?}: {:?}",
            h.paths()
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

// Legacy TS discards symbol selection and switches linear USDT once.
#[test]
fn legacy_position_mode_switch_uses_linear_usdt_even_with_an_inverse_symbol() {
    let mut h = ready(Variant::Inverse);
    h.command(Command::SetPositionMode {
        command: 1,
        hedged: true,
        symbol: Some("BTCUSD".into()),
    });
    let request = h.take("/v5/position/switch-mode");
    assert_eq!(
        body(&request),
        r#"{"category":"linear","coin":"USDT","mode":3}"#
    );
    h.reply(request.id, 200, r#"{"retCode":0,"result":{}}"#);
    assert_eq!(h.result(1), Some(&Ok("[]".into())));
}

// B4: retCode 0, but symbols with open orders or positions kept the old mode.
#[test]
fn a_partial_position_mode_switch_reports_what_bybit_said() {
    let mut h = ready(Variant::Demo);
    h.command(Command::SetPositionMode {
        command: 1,
        hedged: true,
        symbol: None,
    });
    h.respond(
        "/v5/position/switch-mode",
        200,
        r#"{"retCode":0,"retMsg":"Partial symbols switched successfully, excluding symbols with open orders or positions.","result":{}}"#,
    );
    assert!(
        h.command_errors
            .iter()
            .any(|(command, message)| *command == 1 && message.starts_with("Partial symbols")),
        "{:?}",
        h.command_errors
    );
}

// B7: dropping reduceOnly could open a position the caller asked only to reduce.
#[test]
fn a_reduce_only_conditional_stays_reduce_only() {
    let mut h = ready(Variant::Demo);
    let mut opts = order("BTCUSDT", 1.0);
    opts.kind = OrderType::Conditional;
    opts.trigger_price = Some(70000.0);
    opts.reduce_only = Some(true);
    place(&mut h, 1, vec![opts]);
    resolve_mode(&mut h, "BTCUSDT");
    let create = h.take("/v5/order/create");
    assert!(
        body(&create).contains("\"reduceOnly\":true"),
        "{}",
        body(&create)
    );
}

// B6: tpslMode Full closes the whole position; only an explicit zero asks for it.
#[test]
fn a_position_stop_quantity_rounding_to_zero_is_refused() {
    let mut h = ready(Variant::Demo);
    h.message(
        "/v5/private",
        r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"side":"Buy","size":"3","avgPrice":"50000","markPrice":"51000","positionValue":"153000","leverage":"10","unrealisedPnl":"300","curRealisedPnl":"0","positionIM":"30"}]}"#,
    );
    let mut opts = order("BTCUSDT", 0.000_000_001);
    opts.kind = OrderType::StopLoss;
    opts.side = OrderSide::Sell;
    opts.price = Some(40000.0);
    place(&mut h, 1, vec![opts]);
    assert!(
        !h.has_request("/v5/position/trading-stop"),
        "{:?}",
        h.paths()
    );
    assert!(
        h.result(1)
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("below the BTCUSDT size step"),
        "{:?}",
        h.result(1)
    );
}

// TS position-mode uses a single linear/USDT command.
#[test]
fn after_failure_settings_refusal_does_not_release_a_second_coin() {
    // TS one linear/USDT request: reference/bybit/bybitinverse.exchange.ts:2318-2335.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | position-mode coin loop
    let mut h = ready(Variant::Inverse);
    h.command(Command::SetPositionMode {
        command: 1,
        hedged: true,
        symbol: None,
    });
    let request = h.take("/v5/position/switch-mode");
    assert_eq!(
        body(&request),
        r#"{"category":"linear","coin":"USDT","mode":3}"#
    );
    h.reply(request.id, 200, r#"{"retCode":10001,"retMsg":"refused"}"#);
    h.advance(1_201.0);
    assert!(!h.has_request("/v5/position/switch-mode"));
    assert!(matches!(h.result(1), Some(Err(_))));
}

#[test]
fn legacy_position_mode_success_has_no_per_coin_suffix() {
    // TS one setting write: reference/bybit/bybitinverse.exchange.ts:2318-2335.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | settings Applied adoption
    let mut h = ready(Variant::Inverse);
    h.command(Command::SetPositionMode {
        command: 1,
        hedged: true,
        symbol: None,
    });
    h.respond(
        "/v5/position/switch-mode",
        200,
        r#"{"retCode":0,"result":{}}"#,
    );
    h.advance(1_201.0);
    assert!(!h.has_request("/v5/position/switch-mode"));
    assert_eq!(h.result(1), Some(&Ok("[]".into())));
}

// A cached mode Bybit just refused is re-read before the next order.
#[test]
fn a_refused_position_mode_is_forgotten() {
    let mut h = ready(Variant::Demo);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    resolve_mode(&mut h, "BTCUSDT");
    // The refused batch falls back to its single create, refused the same way.
    while h.has_request("/v5/order/create") {
        h.respond(
            "/v5/order/create",
            200,
            r#"{"retCode":10001,"retMsg":"position idx not match position mode","result":{},"retExtInfo":{}}"#,
        );
    }
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    assert!(!h.has_request("/v5/order/create"), "{:?}", h.paths());
    assert!(h.has_request("/v5/position/list"), "{:?}", h.paths());
}

// TS treats this as done.
#[test]
fn amend_answered_order_not_modified_succeeds() {
    for answer in [
        r#"{"retCode":10001,"retMsg":"order not modified","result":{},"retExtInfo":{}}"#,
        r#"{"retCode":34040,"retMsg":"order not modified","result":{},"retExtInfo":{}}"#,
    ] {
        let mut h = ready(Variant::Demo);
        h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"parent","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}]}"#);
        h.command(Command::AmendOrder {
            command: 2,
            id: "parent".into(),
            price: Some(100.0),
            amount: None,
        });
        resolve_mode(&mut h, "BTCUSDT");
        h.respond("/v5/order/amend", 200, answer);
        assert!(h.result(2).unwrap().is_ok(), "{answer} {:?}", h.result(2));
    }
}

#[test]
fn a_spot_read_shows_rows_that_carry_no_category_of_their_own() {
    let h = ready_listing(
        options(Variant::Spot),
        r#"{"retCode":0,"retMsg":"OK","result":{"category":"spot","nextPageCursor":"","list":[{"symbol":"BTCUSDT","side":"Buy","orderType":"Limit","orderId":"s1","orderStatus":"New","price":"100","qty":"1","leavesQty":"1","cumExecQty":"0","createdTime":"1671999000000"}]}}"#,
    );
    assert!(h.engine.store().order("s1").is_some());
}

fn managed_cancel_pair(reply: &str) -> Harness {
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
        command: 120,
        ids: vec!["one".into(), "two".into()],
    });
    let request = h.take("/v5/order/cancel-batch");
    h.reply(request.id, 200, reply);
    h
}

#[test]
fn managed_cancel_batch_reordered_echoes_resolve_without_a_false_unknown() {
    let h = managed_cancel_pair(
        r#"{"retCode":0,"result":{"list":[{"orderId":"two","orderLinkId":""},{"orderId":"one","orderLinkId":""}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"},{"code":0,"msg":"OK"}]}}"#,
    );
    assert_eq!(h.result(120), Some(&Ok("[]".into())));
    for id in ["one", "two"] {
        assert!(h.engine.store().order(id).is_none());
    }
    assert!(!h.unknown.contains(&120));
    assert!(!h.has_request("/v5/order/cancel-batch"));
}

#[test]
fn managed_cancel_batch_reordered_partial_success_only_retires_the_accepted_order() {
    let h = managed_cancel_pair(
        r#"{"retCode":0,"result":{"list":[{"orderId":"two"},{"orderId":"one"}]},"retExtInfo":{"list":[{"code":10005,"msg":"permission denied"},{"code":0,"msg":"OK"}]}}"#,
    );
    assert!(h.result(120).unwrap().is_err());
    assert!(h.engine.store().order("one").is_none());
    assert_eq!(
        h.engine.store().order("two").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
    assert!(
        !h.unknown.contains(&120),
        "a proven refusal stays a refusal"
    );
    assert!(!h.has_request("/v5/order/cancel-batch"));
}

#[test]
fn applied_batch_cancel_unknown_row_records_only_confirmed_cancellations() {
    let mut h = managed_cancel_pair(
        r#"{"retCode":0,"result":{"list":[{"orderId":"two"},{"orderId":"one"}]},"retExtInfo":{"list":[{"code":10016,"msg":"unknown cancellation outcome"},{"code":0,"msg":"OK"}]}}"#,
    );
    assert!(h.engine.store().order("one").is_none());
    assert_eq!(
        h.engine.store().order("two").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open,
        "an Unknown row must not confirm its target's cancellation"
    );
    h.advance(1_000.0);
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == safe_cex_engine::Method::Get),
        "no cancellation replay or compensating write"
    );
    let errors: Vec<_> = h
        .notices
        .iter()
        .filter_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed {
                command: 120,
                error,
            } => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert!(
        errors[0].partial.as_ref().unwrap().order_ids.is_empty(),
        "cancellation targets are existing orders, never newly created IDs: {:?}",
        errors[0]
    );
    assert_eq!(
        serde_json::to_value(errors[0]).unwrap()["partial"],
        serde_json::json!({ "orderIds": [], "total": 2, "failedIndex": 1 }),
        "the JS boundary still needs the attempted count and Unknown target index"
    );
    let elements = serde_json::to_value(errors[0]).unwrap()["elements"].clone();
    assert_eq!(
        elements[0],
        serde_json::json!({ "index": 0, "status": "applied", "effect": "cancel", "targetOrderId": "one" })
    );
    assert_eq!(elements[1]["index"], 1);
    assert_eq!(elements[1]["status"], "unknown");
    assert_eq!(elements[1]["error"]["outcome"], "unknown");
    assert_eq!(elements[1]["error"]["venueCode"], "10016");
    assert_eq!(elements.as_array().unwrap().len(), 2);
}

fn cancel_unknown_pair() -> Harness {
    managed_cancel_pair(
        r#"{"retCode":0,"result":{"list":[{"orderId":"two"},{"orderId":"one"}]},"retExtInfo":{"list":[{"code":10016,"msg":"unknown cancellation outcome"},{"code":0,"msg":"OK"}]}}"#,
    )
}

#[test]
fn applied_cancel_unknown_has_actionable_guidance_and_native_diagnostics() {
    let h = cancel_unknown_pair();
    let error = spot_parity_error(&h, 120);
    assert_eq!(error.message, "The outcome is unknown. Check Open Orders");
    assert_eq!(error.venue_code.as_deref(), Some("10016"));
    assert_eq!(
        error.venue_message.as_deref(),
        Some("unknown cancellation outcome")
    );
    assert_eq!(
        error.kind,
        Some(safe_cex_engine::ErrorKind::VenueUnavailable)
    );
}

#[test]
fn applied_cancel_unknown_emits_one_bounded_outcome_event() {
    let mut h = cancel_unknown_pair();
    h.advance(1_000.0);
    let events: Vec<_> = h
        .notices
        .iter()
        .filter_map(|notice| match notice {
            safe_cex_engine::Notice::Event(event) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].level, safe_cex_engine::Level::Warn);
    assert_eq!(
        events[0].message,
        "The outcome is unknown. Check Open Orders"
    );
    assert_eq!(
        events[0].fields,
        vec![
            ("command", 120_u32.into()),
            ("outcome", "unknown".into()),
            ("attemptedCount", 2_f64.into()),
            ("appliedCount", 1_f64.into()),
            ("unknownCount", 1_f64.into()),
        ]
    );
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == safe_cex_engine::Method::Get)
    );
}

#[test]
fn applied_cancel_confirmed_and_refused_rows_emit_no_unknown_event() {
    for code in [0, 10005] {
        let h = managed_cancel_pair(&format!(
            r#"{{"retCode":0,"result":{{"list":[{{"orderId":"one"}},{{"orderId":"two"}}]}},"retExtInfo":{{"list":[{{"code":0}},{{"code":{code},"msg":"permission denied"}}]}}}}"#
        ));
        assert!(!h.notices.iter().any(|notice| matches!(notice,
            safe_cex_engine::Notice::Event(event) if event.code == "order.outcome_unknown")));
        if code == 0 {
            assert_eq!(h.result(120), Some(&Ok("[]".into())));
        } else {
            let error = spot_parity_error(&h, 120);
            assert_eq!(
                error.outcome, None,
                "the existing known-partial classification is retained"
            );
            assert_eq!(error.venue_code.as_deref(), Some("10005"));
            assert_eq!(error.message, "Bybit cancel was not confirmed");
        }
    }
}

#[test]
fn applied_cancel_reports_prior_batches_and_an_unsent_tail_after_unknown() {
    let mut h = ready(Variant::Demo);
    let ids: Vec<String> = (0..21).map(|index| format!("target-{index}")).collect();
    let rows: Vec<_> = ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "symbol":"BTCUSDT", "orderId":id, "orderStatus":"New", "orderType":"Limit",
                "side":"Buy", "price":"100", "qty":"1", "leavesQty":"1"
            })
        })
        .collect();
    h.message(
        "/v5/private",
        &serde_json::json!({"topic":"order", "data":rows}).to_string(),
    );
    h.command(Command::CancelOrders {
        command: 123,
        ids: ids.clone(),
    });
    for batch in 0..2 {
        let request = h.take("/v5/order/cancel-batch");
        let wire: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        let expected: Vec<_> = ids[batch * 10..batch * 10 + 10]
            .iter()
            .map(|id| serde_json::json!({"symbol":"BTCUSDT", "orderId":id}))
            .collect();
        assert_eq!(wire["request"], serde_json::json!(expected));
        let targets: Vec<_> = ids[batch * 10..batch * 10 + 10]
            .iter()
            .rev()
            .map(|id| serde_json::json!({"orderId":id}))
            .collect();
        let statuses: Vec<_> = ids[batch * 10..batch * 10 + 10]
            .iter()
            .rev()
            .map(|id| serde_json::json!({"code":if id == "target-11" {10016} else {0}}))
            .collect();
        h.reply(request.id, 200, &serde_json::json!({"retCode":0,"result":{"list":targets},"retExtInfo":{"list":statuses}}).to_string());
        h.advance(1_000.0);
    }
    let error = spot_parity_error(&h, 123);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.partial.as_ref().unwrap().total, Some(21));
    assert_eq!(error.partial.as_ref().unwrap().failed_index, Some(11));
    let elements = serde_json::to_value(error).unwrap()["elements"].clone();
    assert_eq!(elements.as_array().unwrap().len(), 21);
    for (index, id) in ids.iter().enumerate() {
        assert_eq!(elements[index]["index"], index);
        if index == 11 {
            assert_eq!(elements[index]["status"], "unknown");
        } else if index == 20 {
            assert_eq!(
                elements[index],
                serde_json::json!({"index":20,"status":"notSent"})
            );
        } else {
            assert_eq!(
                elements[index],
                serde_json::json!({"index":index,"status":"applied","effect":"cancel","targetOrderId":id})
            );
        }
        assert_eq!(
            h.engine.store().order(id).is_some(),
            index == 11 || index == 20
        );
    }
    assert!(error.partial.as_ref().unwrap().order_ids.is_empty());
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == safe_cex_engine::Method::Get)
    );
    let events: Vec<_> = h
        .notices
        .iter()
        .filter_map(|notice| match notice {
            safe_cex_engine::Notice::Event(event) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(events.len(), 1);
    assert!(
        events[0]
            .fields
            .contains(&("attemptedCount", 20_f64.into()))
    );
    assert!(events[0].fields.contains(&("appliedCount", 19_f64.into())));
    assert!(events[0].fields.contains(&("unknownCount", 1_f64.into())));
}

fn cancel_journal_started(ids: Vec<String>) -> Harness {
    cancel_journal_started_with(Variant::Demo, ids)
}

fn cancel_journal_started_with(variant: Variant, ids: Vec<String>) -> Harness {
    let mut h = ready(variant);
    let rows: Vec<_> = ids
        .iter()
        .map(|id| {
            serde_json::json!({
                "symbol":"BTCUSDT", "orderId":id, "orderStatus":"New", "orderType":"Limit",
                "side":"Buy", "price":"100", "qty":"1", "leavesQty":"1"
            })
        })
        .collect();
    h.message(
        "/v5/private",
        &serde_json::json!({"topic":"order", "data":rows}).to_string(),
    );
    h.command(Command::CancelOrders { command: 124, ids });
    h
}

fn cancel_journal_first_batch(ids: &[String]) -> Harness {
    let mut h = cancel_journal_started(ids.to_vec());
    let request = h.take("/v5/order/cancel-batch");
    let targets: Vec<_> = ids[..10]
        .iter()
        .map(|id| serde_json::json!({"orderId":id}))
        .collect();
    h.reply(
        request.id,
        200,
        &serde_json::json!({
            "retCode":0, "result":{"list":targets},
            "retExtInfo":{"list":vec![serde_json::json!({"code":0}); 10]}
        })
        .to_string(),
    );
    h
}

fn cancel_journal_second_batch(ids: &[String]) -> Harness {
    let mut h = cancel_journal_first_batch(ids);
    h.advance(1_000.0);
    h
}

fn assert_cancel_journal_terminal(h: &mut Harness, ids: &[String], states: &[&str]) {
    h.advance(1_000.0);
    let error = spot_parity_error(h, 124);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    let elements = serde_json::to_value(error).unwrap()["elements"].clone();
    assert_eq!(
        elements
            .as_array()
            .expect("complete cancellation journal")
            .len(),
        ids.len()
    );
    for (index, (id, state)) in ids.iter().zip(states).enumerate() {
        assert_eq!(elements[index]["index"], index);
        assert_eq!(
            elements[index]["status"], *state,
            "caller occurrence {index}"
        );
        if *state == "applied" {
            assert_eq!(elements[index]["effect"], "cancel");
            assert_eq!(elements[index]["targetOrderId"], *id);
        } else if *state == "unknown" {
            assert_eq!(elements[index]["error"]["outcome"], "unknown");
        } else {
            assert_eq!(
                elements[index],
                serde_json::json!({"index":index,"status":"notSent"})
            );
        }
    }
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == safe_cex_engine::Method::Get),
        "no replay or later cancellation"
    );
}

fn cancel_journal_terminal_case(failure: &str) {
    let ids: Vec<_> = (0..21).map(|index| format!("terminal-{index}")).collect();
    let mut h = cancel_journal_second_batch(&ids);
    let request = h.take("/v5/order/cancel-batch");
    match failure {
        "http" => h.reply(request.id, 503, "unavailable"),
        "transport" => h.event(Event::HttpFailed {
            request: request.id,
            error: "transport lost",
        }),
        "timeout" => h.advance(20_001.0),
        "expiry" => {
            // Deliver overdue callbacks after a host-loop pause beyond
            // the whole Plan deadline. The engine must expire the Plan first;
            // walking intermediate due times would only test call timeout.
            h.now += 200_000.0;
            h.advance(0.0);
        }
        "malformed" => h.reply(request.id, 200, r#"{"retCode":0}"#),
        _ => panic!("unknown test failure"),
    }
    if matches!(failure, "expiry" | "timeout") {
        // Whole-Plan expiry owns the final seal: native cleanup contributes
        // passive elements, while the engine retains the physical timeout
        // fact. Call timeout instead uses the venue's presented terminal text.
        let error = spot_parity_error(&h, 124);
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Timeout));
        assert_eq!(
            error
                .message
                .starts_with("The outcome is unknown. Check Open Orders"),
            failure == "timeout",
            "whole-Plan expiry must own the seal; call timeout remains venue-presented"
        );
    }
    let mut states = vec!["applied"; 10];
    states.extend(["unknown"; 10]);
    states.push("notSent");
    assert_cancel_journal_terminal(&mut h, &ids, &states);
    for (index, id) in ids.iter().enumerate() {
        assert_eq!(h.engine.store().order(id).is_some(), index >= 10);
    }
}

#[test]
fn cancel_journal_http_failure_keeps_confirmed_prefix_and_unsent_tail() {
    cancel_journal_terminal_case("http");
}

#[test]
fn cancel_journal_transport_failure_keeps_confirmed_prefix_and_unsent_tail() {
    cancel_journal_terminal_case("transport");
}

#[test]
fn cancel_journal_timeout_keeps_confirmed_prefix_and_unsent_tail() {
    cancel_journal_terminal_case("timeout");
}

#[test]
fn cancel_journal_plan_expiry_keeps_confirmed_prefix_and_unsent_tail() {
    cancel_journal_terminal_case("expiry");
}

#[test]
fn cancel_journal_plan_expiry_between_batches_keeps_confirmed_prefix() {
    let ids: Vec<_> = (0..21).map(|index| format!("paced-{index}")).collect();
    let mut h = cancel_journal_first_batch(&ids);
    // Deliver the pacing/expiry callbacks after a host-loop pause. Advancing
    // through every intermediate timer would release the next batch first.
    h.now += 200_000.0;
    h.advance(0.0);
    let error = spot_parity_error(&h, 124);
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Timeout));
    assert_eq!(
        error.outcome, None,
        "only confirmed effects and never-sent calls"
    );
    let elements = serde_json::to_value(error).unwrap()["elements"].clone();
    assert_eq!(
        elements
            .as_array()
            .expect("paced cancellation journal")
            .len(),
        21
    );
    for (index, id) in ids.iter().enumerate() {
        assert_eq!(elements[index]["index"], index);
        if index < 10 {
            assert_eq!(elements[index]["status"], "applied");
            assert_eq!(elements[index]["effect"], "cancel");
            assert_eq!(elements[index]["targetOrderId"], *id);
        } else {
            assert_eq!(
                elements[index],
                serde_json::json!({"index":index,"status":"notSent"})
            );
        }
    }
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == safe_cex_engine::Method::Get)
    );
}

#[test]
fn cancel_journal_malformed_reply_control_keeps_all_occurrences() {
    cancel_journal_terminal_case("malformed");
}

#[test]
fn cancel_journal_repeated_target_keeps_both_mixed_response_orders() {
    for unknown in [0, 1] {
        let ids = vec!["one".to_owned(), "one".to_owned()];
        let mut h = cancel_journal_started(ids.clone());
        let request = h.take("/v5/order/cancel-batch");
        let wire: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        assert_eq!(wire["request"].as_array().unwrap().len(), 2);
        let rows: Vec<_> = (0..2)
            .map(|index| {
                serde_json::json!({
                    "code":if index == unknown {10016} else {0}, "msg":"native cancel outcome"
                })
            })
            .collect();
        h.reply(
            request.id,
            200,
            &serde_json::json!({"retCode":0,"retExtInfo":{"list":rows}}).to_string(),
        );
        let mut states = ["applied"; 2];
        states[unknown] = "unknown";
        assert_cancel_journal_terminal(&mut h, &ids, &states);
        let events: Vec<_> = h
            .notices
            .iter()
            .filter_map(|notice| match notice {
                safe_cex_engine::Notice::Event(event) if event.code == "order.outcome_unknown" => {
                    Some(event)
                }
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 1);
        assert!(events[0].fields.contains(&("attemptedCount", 2_f64.into())));
        assert!(events[0].fields.contains(&("appliedCount", 1_f64.into())));
        assert!(events[0].fields.contains(&("unknownCount", 1_f64.into())));
    }
}

#[test]
fn cancel_journal_repeated_unsent_tail_does_not_inherit_an_earlier_acceptance() {
    let mut ids: Vec<_> = (0..21).map(|index| format!("repeated-{index}")).collect();
    ids[20] = ids[0].clone();
    let mut h = cancel_journal_second_batch(&ids);
    let request = h.take("/v5/order/cancel-batch");
    h.reply(request.id, 200, r#"{"retCode":0}"#);
    let mut states = vec!["applied"; 10];
    states.extend(["unknown"; 10]);
    states.push("notSent");
    assert_cancel_journal_terminal(&mut h, &ids, &states);
}

#[test]
fn cancel_journal_private_readiness_refusal_keeps_confirmed_prefix() {
    let ids = vec!["confirmed".to_owned(), "unsent".to_owned()];
    let mut h = cancel_journal_started_with(Variant::Hyrotrader, ids.clone());
    let request = h.take("/v5/order/cancel");
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"confirmed"}}"#,
    );
    h.advance(1_000.0);
    let error = spot_parity_error(&h, 124);
    assert_eq!(error.message, "HyroTrader private state is recovering");
    let elements = serde_json::to_value(error).unwrap()["elements"].clone();
    assert_eq!(
        elements,
        serde_json::json!([
            {"index":0,"status":"applied","effect":"cancel","targetOrderId":"confirmed"},
            {"index":1,"status":"notSent"}
        ])
    );
    assert!(
        h.requests
            .iter()
            .all(|r| r.method == safe_cex_engine::Method::Get),
        "private readiness cannot release another cancellation"
    );
}

#[test]
fn cancel_journal_top_level_already_gone_keeps_sent_occurrences_unknown() {
    top_level_already_gone_case(true);
}

#[test]
fn cancel_journal_top_level_already_gone_survives_unproven_http_normalization() {
    top_level_already_gone_case(false);
}

fn top_level_already_gone_case(native_failure: bool) {
    for code in [110001, 170213] {
        let ids: Vec<_> = (0..21).map(|index| format!("gone-{index}")).collect();
        let mut h = cancel_journal_started(ids.clone());
        let request = h.take("/v5/order/cancel-batch");
        h.reply(
            request.id,
            200,
            &serde_json::json!({"retCode":code,"retMsg":"order not exists"}).to_string(),
        );
        h.advance(1_000.0);
        let next = h.take("/v5/order/cancel-batch");
        let wire: serde_json::Value = serde_json::from_str(body(&next)).unwrap();
        assert_eq!(wire["request"][0]["orderId"], ids[10]);
        if native_failure {
            h.reply(
                next.id,
                200,
                &serde_json::json!({
                    "retCode":0, "retExtInfo":{"list":vec![
                        serde_json::json!({"code":10016,"msg":"unknown cancel outcome"});10
                    ]}
                })
                .to_string(),
            );
        } else {
            h.reply(next.id, 503, "unavailable");
        }
        let mut states = vec!["unknown"; 20];
        states.push("notSent");
        assert_cancel_journal_terminal(&mut h, &ids, &states);
        let error = spot_parity_error(&h, 124);
        let elements = serde_json::to_value(error).unwrap()["elements"].clone();
        if native_failure {
            assert_eq!(elements[0]["error"]["venueCode"], code.to_string());
        } else {
            // The existing unproven-HTTP boundary normalizes Unknown diagnostics.
            assert_eq!(elements[0]["error"]["kind"], "venueUnavailable");
        }
    }
}

#[test]
fn managed_cancel_batch_anonymous_already_gone_row_waits_for_reconciliation() {
    let mut h = managed_cancel_pair(
        r#"{"retCode":0,"result":{"list":[{"orderId":"one"},{"orderId":"","orderLinkId":""}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"},{"code":110001,"msg":"order not exists"}]}}"#,
    );
    assert_eq!(h.result(120), Some(&Ok("[]".into())));
    assert!(h.engine.store().order("one").is_none());
    assert!(
        h.engine.store().order("two").is_some(),
        "absence code does not fabricate a cancellation event"
    );
    answer_reconcile(&mut h);
    assert!(h.engine.store().order("two").is_none());
    assert!(!h.unknown.contains(&120));
}

#[test]
fn managed_single_cancel_batch_accepts_a_single_status_with_an_empty_identity() {
    for variant in [Variant::Linear, Variant::Demo] {
        let mut h = ready(variant);
        h.message("/v5/private", r#"{"topic":"order","data":[{"symbol":"BTCUSDT","orderId":"one","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}]}"#);
        h.command(Command::CancelOrders {
            command: 121,
            ids: vec!["one".into()],
        });
        h.respond("/v5/order/cancel-batch", 200, r#"{"retCode":0,"result":{"list":[{"orderId":"","orderLinkId":""}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
        assert_eq!(h.result(121), Some(&Ok("[]".into())));
        assert!(!h.unknown.contains(&121));
    }
}

#[test]
fn managed_cancel_batch_unproven_reply_is_actionable_and_diagnoses_shape_without_values() {
    let h = managed_cancel_pair(
        r#"{"retCode":0,"result":{"list":[{"orderId":"one","orderLinkId":"private-client-needle"},{"orderId":"two"}]},"retExtInfo":{"list":[{"code":0,"msg":"private-status-needle"}]}}"#,
    );
    let error = h.result(120).unwrap().as_ref().unwrap_err();
    assert!(error.contains("check open orders on Bybit before retrying"));
    assert!(error.contains("targets=2; result.list=array/2; retExtInfo.list=array/1"));
    assert!(!error.contains("private-client-needle"));
    assert!(!error.contains("private-status-needle"));
    for id in ["one", "two"] {
        assert!(h.engine.store().order(id).is_some());
    }
    assert!(h.unknown.contains(&120));
    assert!(!h.has_request("/v5/order/cancel-batch"));
}

#[test]
fn managed_cancel_batch_foreign_duplicate_and_ambiguous_echoes_leave_live_orders_unchanged() {
    for list in [
        r#"[{"orderId":"one"},{"orderId":"one"}]"#,
        r#"[{"orderId":"foreign"},{"orderId":"two"}]"#,
        r#"[{},{}]"#,
    ] {
        assert_invalid_managed_cancel_batch_preserves_rows(&format!(
            r#"{{"retCode":0,"result":{{"list":{list}}},"retExtInfo":{{"list":[{{"code":0}},{{"code":0}}]}}}}"#
        ));
    }
}

#[test]
fn managed_single_cancel_batch_already_gone_empty_list_is_satisfied_after_a_fresh_read() {
    for code in [110001, 170213] {
        let mut h = ready(Variant::Demo);
        h.message("/v5/private", r#"{"topic":"order","data":[{"symbol":"BTCUSDT","orderId":"one","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","leavesQty":"1"}]}"#);
        h.command(Command::CancelOrders {
            command: 122,
            ids: vec!["one".into()],
        });
        h.respond("/v5/order/cancel-batch", 200, &format!(r#"{{"retCode":0,"result":{{"list":[]}},"retExtInfo":{{"list":[{{"code":{code},"msg":"order does not exist"}}]}}}}"#));
        assert_eq!(h.result(122), Some(&Ok("[]".into())));
        assert_eq!(
            h.engine.store().order("one").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
        answer_reconcile(&mut h);
        assert!(h.engine.store().order("one").is_none());
        assert!(!h.unknown.contains(&122));
        assert!(!h.has_request("/v5/order/cancel-batch"));
    }
}

/// An Unknown placement on Bybit Inverse fences its symbol until the engine's
/// reconcile reads the INVERSE category's orders and the clean read releases it
/// (one user hit the fence on bybitinverse; the fence must not outlive the read).
#[test]
fn an_unknown_inverse_placement_reconciles_on_the_inverse_category_and_releases() {
    let mut h = ready(Variant::Inverse);
    place(&mut h, 1, vec![order("BTCUSD", 10.0)]);
    resolve_mode(&mut h, "BTCUSD");
    let write = h.take("/v5/order/create-batch");
    assert!(body(&write).contains("\"category\":\"inverse\""));
    h.event(Event::HttpFailed {
        request: write.id,
        error: "connection reset",
    });
    // The reconcile read is the inverse product's own.
    let reads: Vec<_> = h
        .requests
        .iter()
        .filter(|r| r.url.contains("/v5/order/realtime"))
        .cloned()
        .collect();
    assert!(!reads.is_empty(), "the seal's reconcile read");
    assert!(
        reads.iter().all(|r| r.url.contains("category=inverse")),
        "{:?}",
        reads.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    // Fenced until it answers.
    place(&mut h, 2, vec![order("BTCUSD", 10.0)]);
    assert!(
        h.result(2)
            .is_some_and(|r| r.as_ref().is_err_and(|e| e.contains("uncertain"))),
        "{:?}",
        h.result(2)
    );
    assert!(!h.has_request("/v5/order/create-batch"));
    for read in reads {
        h.reply(read.id, 200, EMPTY);
    }
    place(&mut h, 3, vec![order("BTCUSD", 10.0)]);
    while h.has_request("/v5/position/list") {
        resolve_mode(&mut h, "BTCUSD");
    }
    assert!(
        h.has_request("/v5/order/create-batch"),
        "the clean inverse read releases the fence: {:?}",
        h.result(3)
    );
}

// The November 12 default-history change also affects link-filtered lookups.
fn answer_terminal_link(h: &mut Harness, id: &str, status: &str) {
    let filter = format!("orderLinkId={id}");
    for expected in [
        None,
        None,
        Some("Cancelled"),
        Some("Rejected"),
        Some("Deactivated"),
    ] {
        let read = h.take(&filter);
        assert!(read.url.starts_with("https://hyro.example/"));
        match expected {
            None => assert!(!read.url.contains("orderStatus=")),
            Some(value) => assert!(read.url.contains(&format!("orderStatus={value}"))),
        }
        assert!(!h.has_request("/v5/order/create"));
        if expected == Some(status) {
            h.reply(read.id, 200, &lookup_row(id, status));
            return;
        }
        h.reply(read.id, 200, EMPTY);
        assert!(
            !h.has_request("/v5/order/create"),
            "empty earlier axis is no proof"
        );
    }
    panic!("terminal status was not queried");
}

#[test]
fn hyro_background_explicit_terminal_history_preserves_original_identity() {
    for status in ["Cancelled", "Rejected", "Deactivated"] {
        let mut h = ready(Variant::Hyrotrader);
        place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
        let create = h.take("/v5/order/create");
        let id = link(&create);
        h.event(Event::HttpFailed {
            request: create.id,
            error: "lost",
        });
        answer_terminal_link(&mut h, &id, status);
        assert_eq!(outcome(&h, 1), Some(safe_cex_engine::Outcome::Unknown));
        assert!(!h.has_request("/v5/order/create"));
        place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
        if status == "Rejected" {
            assert_ne!(
                link(&h.take("/v5/order/create")),
                id,
                "positive rejection remints"
            );
        } else {
            assert_eq!(h.result(2), Some(&Ok("[\"recovered\"]".into())));
            assert!(!h.has_request("/v5/order/create"));
            assert!(h.engine.store().order("recovered").is_none());
        }
    }
}

#[test]
fn hyro_preflight_explicit_terminal_history_preserves_original_identity() {
    for status in ["Cancelled", "Rejected", "Deactivated"] {
        let mut h = ready(Variant::Hyrotrader);
        place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
        let create = h.take("/v5/order/create");
        let id = link(&create);
        h.event(Event::HttpFailed {
            request: create.id,
            error: "lost",
        });
        h.respond(&format!("orderLinkId={id}"), 500, "unavailable");
        place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
        answer_terminal_link(&mut h, &id, status);
        if status == "Rejected" {
            assert_ne!(
                link(&h.take("/v5/order/create")),
                id,
                "positive rejection remints"
            );
        } else {
            assert_eq!(h.result(2), Some(&Ok("[\"recovered\"]".into())));
            assert!(!h.has_request("/v5/order/create"));
        }
    }
}

#[test]
fn hyro_terminal_history_failure_never_proves_absence() {
    for body in [
        "unavailable",
        r#"{"retCode":0,"result":{"list":[],"nextPageCursor":"more"}}"#,
    ] {
        let mut h = ready(Variant::Hyrotrader);
        place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
        let create = h.take("/v5/order/create");
        let id = link(&create);
        h.event(Event::HttpFailed {
            request: create.id,
            error: "lost",
        });
        h.respond(&format!("orderLinkId={id}"), 500, "unavailable");
        place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
        h.respond(&format!("orderLinkId={id}"), 200, EMPTY);
        h.respond(&format!("orderLinkId={id}"), 200, EMPTY);
        let terminal = h.take("orderStatus=Cancelled");
        h.reply(terminal.id, 200, body);
        assert_eq!(outcome(&h, 2), Some(safe_cex_engine::Outcome::Unknown));
        assert!(!h.has_request("/v5/order/create"));
        assert!(!h.has_request(&format!("orderLinkId={id}")));
    }
}

#[test]
fn hyro_explicit_terminal_reads_share_the_original_background_deadline() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost",
    });
    let started = h.now;
    let filter = format!("orderLinkId={id}");
    for _ in 0..4 {
        let read = h.take(&filter);
        h.advance(9_000.0);
        h.reply(read.id, 200, EMPTY);
    }
    let last = h.take("orderStatus=Deactivated");
    h.advance(started + 40_000.0 - h.now);
    h.reply(last.id, 200, &lookup_row(&id, "Deactivated"));
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    assert!(
        h.has_request(&filter),
        "late DATA did not resolve the expired lookup"
    );
    assert!(!h.has_request("/v5/order/create"));
    assert!(h.result(2).is_none());
}

#[test]
fn review_terminal_history_retention_expiry_is_not_complete_absence() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    assert!(
        h.has_request("/v5/order/create"),
        "initial create: {:?}",
        h.done
    );
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost after reaching venue",
    });
    h.respond(&format!("orderLinkId={id}"), 500, "lookup unavailable");
    // The actual original order was Cancelled. All its status-filtered history
    // axes cease returning it after the documented 24-hour retention window.
    h.advance(24.0 * 60.0 * 60.0 * 1000.0 + 1.0);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    for op in ["auth", "subscribe"] {
        let frame = h
            .sent_on("/v5/private")
            .into_iter()
            .rev()
            .find(|m| m.contains(&format!("\"op\":\"{op}\"")))
            .unwrap();
        let frame: serde_json::Value = serde_json::from_str(&frame).unwrap();
        h.message(
            "/v5/private",
            &serde_json::json!({"op":op,"success":true,"req_id":frame["req_id"]}).to_string(),
        );
    }
    for (path, body) in [
        ("/v5/order/realtime", EMPTY),
        ("/v5/position/list", EMPTY),
        ("/v5/account/wallet-balance", WALLET),
    ] {
        while h.has_request(path) {
            h.respond(path, 200, body);
        }
    }
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    assert!(
        h.has_request(&format!("orderLinkId={id}")),
        "retry genuinely enters reconciliation"
    );
    while h.has_request(&format!("orderLinkId={id}")) {
        h.respond(&format!("orderLinkId={id}"), 200, EMPTY);
    }
    assert!(
        !h.has_request("/v5/order/create"),
        "expired terminal coverage cannot authorize a second physical create"
    );
    assert_eq!(outcome(&h, 2), Some(safe_cex_engine::Outcome::Unknown));
    let error = h.result(2).unwrap().as_ref().unwrap_err();
    assert!(
        error.contains("24-hour") && error.contains("broker"),
        "{error}"
    );
}

#[test]
fn review_extra_status_reads_cannot_create_after_the_original_caller_deadline() {
    for wait in [120_000, 40_000] {
        let mut h = ready(Variant::Hyrotrader);
        place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
        let create = h.take("/v5/order/create");
        let id = link(&create);
        h.event(Event::HttpFailed {
            request: create.id,
            error: "lost",
        });
        h.respond(&format!("orderLinkId={id}"), 500, "unavailable");
        let began = h.now;
        h.engine
            .handle_command(
                h.now,
                Command::PlaceOrders {
                    command: 2,
                    orders: vec![order("BTCUSDT", 1.0)],
                },
                Some(wait),
            )
            .unwrap();
        h.collect();
        for _ in 0..5 {
            let filter = format!("orderLinkId={id}");
            let read = h.has_request(&filter).then(|| h.take(&filter));
            for _ in 0..2 {
                h.advance(9_500.);
                h.message("/v5/private", r#"{"op":"pong"}"#);
                for (path, body) in [
                    ("/v5/order/realtime", EMPTY),
                    ("/v5/position/list", EMPTY),
                    ("/v5/account/wallet-balance", WALLET),
                ] {
                    loop {
                        let next = h
                            .requests
                            .iter()
                            .find(|r| r.url.contains(path) && !r.url.contains("orderLinkId="))
                            .map(|r| r.id);
                        let Some(id) = next else {
                            break;
                        };
                        h.requests.retain(|r| r.id != id);
                        h.reply(id, 200, body);
                    }
                }
            }
            if let Some(read) = read {
                h.reply(read.id, 200, EMPTY);
            }
        }
        assert_eq!(h.now - began, 95_000.);
        if wait == 40_000 {
            assert_eq!(outcome(&h, 2), Some(safe_cex_engine::Outcome::Unknown));
            assert!(
                h.notices.iter().any(|notice| matches!(notice,
                    safe_cex_engine::Notice::CommandFailed { command: 2, error }
                    if error.kind == Some(safe_cex_engine::ErrorKind::Timeout)
                )),
                "expiry keeps its typed Timeout cause"
            );
            assert!(
                h.result(2).unwrap().is_err(),
                "expired caller is visibly settled"
            );
            assert_eq!(h.done.iter().filter(|(id, _)| *id == 2).count(), 1);
        }
        let late = h
            .requests
            .iter()
            .find(|r| r.url.contains("/v5/order/create"));
        assert_eq!(
            late.is_some(),
            wait == 120_000,
            "caller wait={wait},elapsed95s; create link={:?}, result={:?}",
            late.map(link),
            h.result(2)
        );
    }
}

#[test]
fn review_every_incomplete_axis_fails_closed_for_both_lookup_owners() {
    for preflight in [false, true] {
        for axis in 0..5 {
            for body in [
                r#"{"retCode":0,"result":{"list":[],"nextPageCursor":"more"}}"#,
                r#"{"retCode":0,"result":{"list":null}}"#,
                r#"{"retCode":0,"result":{}}"#,
                r#"{"retCode":10001,"result":{"list":[]}}"#,
            ] {
                let mut h = ready(Variant::Hyrotrader);
                place(&mut h, 1, vec![order("BTCUSDT", 1.)]);
                let create = h.take("/v5/order/create");
                let id = link(&create);
                h.event(Event::HttpFailed {
                    request: create.id,
                    error: "lost",
                });
                if preflight {
                    h.respond(&format!("orderLinkId={id}"), 500, "unavailable");
                    place(&mut h, 2, vec![order("BTCUSDT", 1.)]);
                }
                for _ in 0..axis {
                    h.respond(&format!("orderLinkId={id}"), 200, EMPTY);
                    assert!(!h.has_request("/v5/order/create"));
                }
                h.respond(&format!("orderLinkId={id}"), 200, body);
                assert!(
                    !h.has_request("/v5/order/create"),
                    "preflight={preflight} axis={axis} body={body}"
                );
                assert!(
                    !h.has_request(&format!("orderLinkId={id}")),
                    "incomplete axis stops the walk"
                );
                assert_eq!(
                    outcome(&h, if preflight { 2 } else { 1 }),
                    Some(safe_cex_engine::Outcome::Unknown)
                );
            }
        }
    }
}

fn retained_hyro_link_after(age_ms: f64) -> (Harness, String) {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 1, vec![order("BTCUSDT", 1.0)]);
    let create = h.take("/v5/order/create");
    let id = link(&create);
    h.event(Event::HttpFailed {
        request: create.id,
        error: "lost after dispatch",
    });
    h.respond(&format!("orderLinkId={id}"), 500, "unavailable");
    h.advance(age_ms);
    let socket = h.socket("/v5/private").unwrap();
    h.event(Event::WsOpened { socket });
    for op in ["auth", "subscribe"] {
        let frame = h
            .sent_on("/v5/private")
            .into_iter()
            .rev()
            .find(|m| m.contains(&format!("\"op\":\"{op}\"")))
            .unwrap();
        let frame: serde_json::Value = serde_json::from_str(&frame).unwrap();
        h.message(
            "/v5/private",
            &serde_json::json!({"op":op,"success":true,"req_id":frame["req_id"]}).to_string(),
        );
    }
    for (path, body) in [
        ("/v5/order/realtime", EMPTY),
        ("/v5/position/list", EMPTY),
        ("/v5/account/wallet-balance", WALLET),
    ] {
        while h.has_request(path) {
            h.respond(path, 200, body);
        }
    }
    (h, id)
}

#[test]
fn hyro_absence_coverage_expires_at_equality_and_when_the_last_axis_crosses_it() {
    let day = 86_400_000.0;
    for (age, last_delay, may_create) in [
        (day - 1.0, 0.0, true),
        (day, 0.0, false),
        (day - 1.0, 1.0, false),
    ] {
        let (mut h, id) = retained_hyro_link_after(age);
        place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
        for axis in 0..5 {
            let read = h.take(&format!("orderLinkId={id}"));
            if axis == 4 {
                h.advance(last_delay);
            }
            h.reply(read.id, 200, EMPTY);
        }
        assert_eq!(
            h.has_request("/v5/order/create"),
            may_create,
            "age={age} delay={last_delay}"
        );
        if !may_create {
            assert_eq!(outcome(&h, 2), Some(safe_cex_engine::Outcome::Unknown));
            assert!(
                h.result(2)
                    .unwrap()
                    .as_ref()
                    .unwrap_err()
                    .contains("24-hour")
            );
        }
    }
}

#[test]
fn hyro_positive_terminal_evidence_still_resolves_expired_absence_coverage() {
    for status in ["Cancelled", "Rejected", "Deactivated"] {
        let (mut h, id) = retained_hyro_link_after(86_400_001.0);
        place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
        answer_terminal_link(&mut h, &id, status);
        if status == "Rejected" {
            assert_ne!(
                link(&h.take("/v5/order/create")),
                id,
                "positive rejection retains remint"
            );
        } else {
            assert_eq!(h.result(2), Some(&Ok("[\"recovered\"]".into())));
            assert!(!h.has_request("/v5/order/create"));
        }
    }
}

#[test]
fn hyro_a_second_send_cannot_renew_the_earliest_send_retention_window() {
    let (mut h, id) = retained_hyro_link_after(86_399_999.0);
    place(&mut h, 2, vec![order("BTCUSDT", 1.0)]);
    answer_link_absent(&mut h, &id);
    let second = h.take("/v5/order/create");
    assert_eq!(link(&second), id);
    h.event(Event::HttpFailed {
        request: second.id,
        error: "second acknowledgement lost",
    });
    h.respond(&format!("orderLinkId={id}"), 500, "unavailable");
    h.advance(2.0);
    place(&mut h, 3, vec![order("BTCUSDT", 1.0)]);
    answer_link_absent(&mut h, &id);
    assert!(!h.has_request("/v5/order/create"));
    assert_eq!(outcome(&h, 3), Some(safe_cex_engine::Outcome::Unknown));
    assert!(
        h.result(3)
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("24-hour"),
        "coverage is measured from the first send, not the second"
    );
}

#[test]
fn market_quantity_cap_splits_each_reduce_only_child_but_keeps_the_limit_cap() {
    for (kind, reduce_only, expected) in [
        (
            OrderType::Market,
            false,
            vec![333.333, 333.333, 333.333, 100.0],
        ),
        (
            OrderType::Market,
            true,
            vec![333.333, 333.333, 333.333, 100.0],
        ),
        (OrderType::Limit, false, vec![1100.0]),
    ] {
        let mut h = ready(Variant::Demo);
        let mut opts = order("BTCUSDT", 1100.0);
        opts.kind = kind;
        opts.reduce_only = Some(reduce_only);
        place(&mut h, 1, vec![opts]);
        resolve_mode(&mut h, "BTCUSDT");
        let request = h.take("/v5/order/create-batch");
        let payload: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        let children = payload["request"].as_array().unwrap();
        let quantities: Vec<f64> = children
            .iter()
            .map(|child| child["qty"].as_str().unwrap().parse().unwrap())
            .collect();
        assert_eq!(quantities, expected, "{kind:?}, reduceOnly={reduce_only}");
        assert!(
            children
                .iter()
                .all(|child| child["reduceOnly"] == reduce_only)
        );
        assert_eq!(
            h.engine
                .store()
                .market("BTCUSDT")
                .unwrap()
                .limits
                .amount
                .max,
            500.0
        );
    }
}

#[test]
fn a_zero_market_only_cap_still_publishes_the_limit_maximum() {
    let instrument = support::INSTRUMENTS.replace("500.000", "0");
    assert!(instrument.contains("\"maxMktOrderQty\":\"0\""));
    let h = ready_listing_with_instruments(options(Variant::Demo), EMPTY, Some(instrument));
    assert_eq!(
        h.engine
            .store()
            .market("BTCUSDT")
            .unwrap()
            .limits
            .amount
            .max,
        1190.0
    );
}

#[test]
fn market_quantity_cap_never_exceeds_limit_cap_and_missing_field_keeps_limit_cap() {
    for (instrument, expected) in [
        (support::INSTRUMENTS.replace("500.000", "2000.000"), 1190.0),
        (support::INSTRUMENTS.replace("500.000", "0"), 1190.0),
        (support::INSTRUMENTS.replace("500.000", "-1"), 1190.0),
        (
            support::INSTRUMENTS.replace(",\"maxMktOrderQty\":\"500.000\"", ""),
            1190.0,
        ),
    ] {
        let mut h = ready_listing_with_instruments(options(Variant::Demo), EMPTY, Some(instrument));
        assert_eq!(
            h.engine
                .store()
                .market("BTCUSDT")
                .unwrap()
                .limits
                .amount
                .max,
            expected
        );
        let mut opts = order("BTCUSDT", 1500.0);
        opts.kind = OrderType::Market;
        opts.reduce_only = Some(true);
        place(&mut h, 1, vec![opts]);
        resolve_mode(&mut h, "BTCUSDT");
        let request = h.take("/v5/order/create-batch");
        let payload: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        assert_eq!(
            payload["request"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["qty"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["595", "595", "310"]
        );
    }
}

#[test]
fn market_quantity_cap_refuses_caller_identity_that_would_span_children() {
    let mut h = ready(Variant::Demo);
    let mut opts = order("BTCUSDT", 1100.0);
    opts.kind = OrderType::Market;
    opts.client_order_id = Some("cap-control".into());
    place(&mut h, 1, vec![opts]);
    assert!(!h.has_request("/v5/order/create"));
    assert!(
        h.done
            .iter()
            .any(|(command, outcome)| *command == 1 && outcome.is_err())
    );
}

#[test]
fn spot_attached_take_profit_amend_omits_derivative_trigger_reference() {
    let mut h = ready(Variant::Spot);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"spot","symbol":"BTCUSDT","orderId":"spot-parent","side":"Buy","orderType":"Limit","orderStatus":"New","qty":"1","price":"100","leavesQty":"1","takeProfit":"110","tpTriggerBy":"UNKNOWN"}]}"#);
    assert!(h.engine.store().order("spot-parent__take_profit").is_some());
    h.command(Command::AmendOrder {
        command: 980,
        id: "spot-parent__take_profit".into(),
        price: Some(115.0),
        amount: None,
    });
    let request = h.take("/v5/order/amend");
    let body: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["category"], "spot");
    assert_eq!(body["orderId"], "spot-parent");
    assert_eq!(body["takeProfit"], "115");
    assert!(
        body.get("tpTriggerBy").is_none(),
        "spot has no derivative trigger reference: {body}"
    );
    assert!(!h.has_request("/v5/order/cancel"));
}

#[test]
fn spot_attached_stop_loss_amend_omits_derivative_trigger_reference() {
    let mut h = ready(Variant::Spot);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"spot","symbol":"BTCUSDT","orderId":"spot-parent","side":"Buy","orderType":"Limit","orderStatus":"New","qty":"1","price":"100","leavesQty":"1","stopLoss":"90","slTriggerBy":"IndexPrice"}]}"#);
    assert!(h.engine.store().order("spot-parent__stop_loss").is_some());
    h.command(Command::AmendOrder {
        command: 981,
        id: "spot-parent__stop_loss".into(),
        price: Some(85.0),
        amount: None,
    });
    let request = h.take("/v5/order/amend");
    let body: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["category"], "spot");
    assert_eq!(body["orderId"], "spot-parent");
    assert_eq!(body["stopLoss"], "85");
    assert!(
        body.get("slTriggerBy").is_none(),
        "spot omits derivative reference: {body}"
    );
    assert!(!h.has_request("/v5/order/cancel"));
}

fn stop_row(trigger: &str, qty: &str) -> String {
    format!(
        r#"{{"topic":"order","data":[{{"category":"linear","symbol":"BTCUSDT","orderId":"stop","orderStatus":"Untriggered","orderType":"Market","stopOrderType":"Stop","triggerPrice":"{trigger}","triggerBy":"MarkPrice","side":"Sell","price":"0","qty":"{qty}","leavesQty":"{qty}","reduceOnly":true,"closeOnTrigger":true}}]}}"#
    )
}

fn amend_stop(h: &mut Harness, command: u32, price: f64, amount: Option<f64>) -> String {
    h.command(Command::AmendOrder {
        command,
        id: "stop".into(),
        price: Some(price),
        amount,
    });
    if h.has_request("/v5/position/list") {
        resolve_mode(h, "BTCUSDT");
    }
    let request = h.take("/v5/order/amend");
    let body = request.body.clone().unwrap_or_default();
    h.reply(
        request.id,
        200,
        r#"{"retCode":10001,"retMsg":"order not modified","result":{},"retExtInfo":{}}"#,
    );
    body
}

/// A stop moved to the trigger it already has: Bybit answers "order not modified"
/// (prod, release 7dd55432) and the edit is done, not an error.
#[test]
fn a_trigger_amend_equal_to_the_stored_stop_is_done_when_not_modified() {
    let mut h = ready(Variant::Demo);
    h.message("/v5/private", &stop_row("60000", "1"));
    let body = amend_stop(&mut h, 3, 60000.0, None);
    assert!(body.contains(r#""triggerPrice":"60000""#), "{body}");
    assert!(h.result(3).unwrap().is_ok(), "{:?}", h.result(3));
    let mut h = ready(Variant::Demo);
    h.message("/v5/private", &stop_row("60000", "1"));
    amend_stop(&mut h, 4, 60000.0, Some(1.0));
    assert!(h.result(4).unwrap().is_ok(), "{:?}", h.result(4));
}

/// Anything the stored stop does not already hold stays Bybit's refusal.
#[test]
fn a_trigger_amend_that_differs_from_the_stored_stop_stays_refused() {
    for (price, amount) in [(60100.0, None), (60000.0, Some(2.0))] {
        let mut h = ready(Variant::Demo);
        h.message("/v5/private", &stop_row("60000", "1"));
        amend_stop(&mut h, 5, price, amount);
        let result = h.result(5).unwrap();
        assert!(
            result
                .as_ref()
                .is_err_and(|e| e.contains("order not modified")),
            "{price} {amount:?}: {result:?}"
        );
    }
}

fn pending_trigger_amend(h: &mut Harness, price: f64) -> safe_cex_engine::HttpRequest {
    h.command(Command::AmendOrder {
        command: 87,
        id: "stop".into(),
        price: Some(price),
        amount: None,
    });
    if h.has_request("/v5/position/list") {
        resolve_mode(h, "BTCUSDT");
    }
    h.take("/v5/order/amend")
}

/// The no-op needs the whole answer to agree: another order's echo, an item row, or a
/// server error keeps it the refusal it is.
#[test]
fn a_trigger_no_op_needs_a_clean_whole_answer() {
    for (status, reply) in [
        (
            200,
            r#"{"retCode":10001,"retMsg":"order not modified","result":{"orderId":"foreign"},"retExtInfo":{}}"#,
        ),
        (
            200,
            r#"{"retCode":10001,"retMsg":"order not modified","result":{},"retExtInfo":{"list":[{"code":10001,"msg":"invalid trigger"}]}}"#,
        ),
        (
            500,
            r#"{"retCode":10001,"retMsg":"order not modified","result":{},"retExtInfo":{}}"#,
        ),
    ] {
        let mut h = ready(Variant::Demo);
        h.message("/v5/private", &stop_row("60000", "1"));
        let request = pending_trigger_amend(&mut h, 60000.0);
        h.reply(request.id, status, reply);
        assert!(
            h.result(87).unwrap().is_err(),
            "{reply}: {:?}",
            h.result(87)
        );
    }
}

/// Only a trigger-bearing row stores its trigger: once the same id is an open limit
/// (it triggered), its execution price is never trigger proof.
#[test]
fn a_trigger_no_op_never_reads_an_active_limits_price_as_its_trigger() {
    let mut h = ready(Variant::Demo);
    h.message("/v5/private", &stop_row("59000", "1"));
    let request = pending_trigger_amend(&mut h, 60000.0);
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"stop","orderStatus":"New","orderType":"Limit","stopOrderType":"UNKNOWN","triggerPrice":"0","side":"Sell","price":"60000","qty":"1","leavesQty":"1","reduceOnly":true,"closeOnTrigger":false}]}"#);
    assert_eq!(
        h.engine.store().order("stop").unwrap().kind,
        OrderType::Limit
    );
    h.reply(
        request.id,
        200,
        r#"{"retCode":10001,"retMsg":"order not modified","result":{},"retExtInfo":{}}"#,
    );
    assert!(h.result(87).unwrap().is_err(), "{:?}", h.result(87));
}

/// The comparison reads the store when the answer lands: a stop moved meanwhile keeps
/// the refusal.
#[test]
fn a_trigger_no_op_compares_the_store_as_it_stands_when_answered() {
    let mut h = ready(Variant::Demo);
    h.message("/v5/private", &stop_row("60000", "1"));
    let request = pending_trigger_amend(&mut h, 60000.0);
    h.message("/v5/private", &stop_row("59000", "1"));
    h.reply(
        request.id,
        200,
        r#"{"retCode":10001,"retMsg":"order not modified","result":{},"retExtInfo":{}}"#,
    );
    assert!(h.result(87).unwrap().is_err(), "{:?}", h.result(87));
}

fn not_modified_with(result: &str, ext: &str) -> String {
    format!(
        r#"{{"retCode":10001,"retMsg":"order not modified","result":{result},"retExtInfo":{ext}}}"#
    )
}

/// Only a provably clean answer is a no-op: unreadable identities, duplicated native
/// ids or item lists are never read as clean absence.
#[test]
fn a_trigger_no_op_rejects_unreadable_or_duplicated_answers() {
    for (result, ext) in [
        (r#"{"orderId":"foreign","orderLinkId":123}"#, "{}"),
        (r#"{"orderId":"foreign","orderId":"stop"}"#, "{}"),
        (r#"{"orderId":123}"#, "{}"),
        (
            "{}",
            r#"{"list":[{"code":10001,"msg":"invalid trigger"}],"list":[]}"#,
        ),
    ] {
        let mut h = ready(Variant::Demo);
        h.message("/v5/private", &stop_row("60000", "1"));
        let request = pending_trigger_amend(&mut h, 60000.0);
        h.reply(request.id, 200, &not_modified_with(result, ext));
        assert!(
            h.result(87).unwrap().is_err(),
            "{result} {ext}: {:?}",
            h.result(87)
        );
    }
}

/// A clean empty answer, or one echoing only its own order, is still done.
#[test]
fn a_trigger_no_op_accepts_a_clean_empty_or_own_echo() {
    for (result, ext) in [
        ("{}", "{}"),
        (r#"{"orderId":"stop"}"#, "{}"),
        ("{}", r#"{"list":[]}"#),
    ] {
        let mut h = ready(Variant::Demo);
        h.message("/v5/private", &stop_row("60000", "1"));
        let request = pending_trigger_amend(&mut h, 60000.0);
        h.reply(request.id, 200, &not_modified_with(result, ext));
        assert!(
            h.result(87).unwrap().is_ok(),
            "{result} {ext}: {:?}",
            h.result(87)
        );
    }
}

#[test]
fn fast_713_second_clock_write_retry_is_re_signed_and_never_replayed_twice() {
    let mut h = ready(Variant::Spot);
    let initial_stamp = h.engine.signing_clock(h.now).0.floor();
    place(&mut h, 601, vec![order("BTCUSDT", 0.1)]);
    let first = h.take("/v5/order/create");
    let server = initial_stamp - 713_451.;
    let refusal = format!(
        r#"{{"retCode":10002,"retMsg":"recv_window server_timestamp[{server}]","result":{{}}}}"#
    );
    h.reply(first.id, 200, &refusal);
    let second = h.take("/v5/order/create");
    assert_eq!(
        Harness::header(&first, "X-BAPI-TIMESTAMP"),
        Some(initial_stamp.to_string())
    );
    assert_eq!(
        Harness::header(&second, "X-BAPI-TIMESTAMP"),
        Some(server.to_string())
    );
    assert_ne!(
        Harness::header(&first, "X-BAPI-SIGN"),
        Harness::header(&second, "X-BAPI-SIGN")
    );
    let payload = format!(
        "{}{}{}{}",
        Harness::header(&second, "X-BAPI-TIMESTAMP").unwrap(),
        Harness::header(&second, "X-BAPI-API-KEY").unwrap(),
        Harness::header(&second, "X-BAPI-RECV-WINDOW").unwrap(),
        second.body.as_deref().unwrap()
    );
    assert_eq!(
        Harness::header(&second, "X-BAPI-SIGN"),
        Some(safe_cex_crypto::hmac_sha256(
            b"secret",
            payload.as_bytes(),
            safe_cex_crypto::Encoding::Hex
        ))
    );
    assert_eq!(
        second.body, first.body,
        "same client-order identity and economics"
    );
    h.reply(second.id, 200, &refusal);
    assert!(
        !h.has_request("/v5/order/create"),
        "a second refusal grants no third placement"
    );
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 601).count(), 1);
}

#[test]
fn ambiguous_clock_like_write_failures_never_calibrate_or_replay() {
    for (status, body) in [
        (
            504,
            r#"{"retCode":10002,"retMsg":"recv_window server_timestamp[1793989307022]","result":{}}"#,
        ),
        (
            200,
            r#"{"retCode":10016,"retMsg":"recv_window server_timestamp[1793989307022]","result":{}}"#,
        ),
        (
            200,
            r#"{"retMsg":"recv_window server_timestamp[1793989307022]","result":{}}"#,
        ),
    ] {
        let mut h = ready(Variant::Spot);
        place(&mut h, 602, vec![order("BTCUSDT", 0.1)]);
        let first = h.take("/v5/order/create");
        let clock = h.engine.signing_clock(h.now);
        h.reply(first.id, status, body);
        assert_eq!(h.engine.signing_clock(h.now), clock);
        assert!(
            !h.has_request("/v5/order/create"),
            "ambiguous answer cannot grant a resend"
        );
        assert!(h.unknown.contains(&602));
        assert!(!h.notices.iter().any(
            |n| matches!(n, safe_cex_engine::Notice::Event(e) if e.code == "bybit.clock.corrected")
        ));
    }
}

/// A dispatched placement can have landed despite an HTTP refusal and a
/// timestamp/signature diagnostic. Only reads may resolve that uncertainty.
#[test]
fn ambiguous_http_clock_refusals_never_resend_spot_creates() {
    const SPOT_EMPTY: &str =
        r#"{"retCode":0,"result":{"category":"spot","list":[],"nextPageCursor":""}}"#;
    for status in [429, 408, 500, 503, 504] {
        for code in [10002, 10004] {
            // Complete the startup census so the unknown write owes a new read.
            let mut h = ready_listing(options(Variant::Spot), SPOT_EMPTY);
            place(&mut h, 603, vec![order("BTCUSDT", 0.1)]);
            let first = h.take("/v5/order/create");
            let clock = h.engine.signing_clock(h.now);
            let refusal = format!(
                r#"{{"retCode":{code},"retMsg":"recv_window server_timestamp[1793989307022]","result":{{}}}}"#
            );
            h.reply(first.id, status, &refusal);
            assert!(
                h.requests
                    .iter()
                    .all(|r| r.method == safe_cex_engine::Method::Get),
                "HTTP {status}, native {code}: no second physical write: {:?}",
                h.requests
            );
            assert!(h.unknown.contains(&603), "HTTP {status}, native {code}");
            assert_eq!(h.done.iter().filter(|(id, _)| *id == 603).count(), 1);
            assert_eq!(h.engine.signing_clock(h.now), clock);
            assert!(h.has_request("/v5/order/realtime"), "reconcile by reading");
            while h.has_request("/v5/order/realtime") {
                h.respond("/v5/order/realtime", 200, EMPTY);
            }
            assert!(
                h.requests
                    .iter()
                    .all(|r| r.method == safe_cex_engine::Method::Get)
            );
            assert_eq!(h.done.iter().filter(|(id, _)| *id == 603).count(), 1);
        }
    }
}

#[test]
fn engine_ambiguity_429_clock_code_cannot_replay() {
    for (status, body) in [
        (
            429,
            r#"{"retCode":10002,"retMsg":"recv_window server_timestamp[1793989307022]","result":{}}"#,
        ),
        (
            200,
            r#"{"retCode":10016,"retMsg":"recv_window server_timestamp[1793989307022]","result":{}}"#,
        ),
        (
            200,
            r#"{"retMsg":"recv_window server_timestamp[1793989307022]","result":{}}"#,
        ),
    ] {
        let mut h = ready(Variant::Spot);
        place(&mut h, 602, vec![order("BTCUSDT", 0.1)]);
        let first = h.take("/v5/order/create");
        h.reply(first.id, status, body);
        assert!(
            !h.has_request("/v5/order/create"),
            "ambiguous answer cannot grant a resend"
        );
        assert!(h.unknown.contains(&602));
        assert!(!h.notices.iter().any(
            |n| matches!(n, safe_cex_engine::Notice::Event(e) if e.code == "bybit.clock.corrected")
        ));
    }
}

#[test]
fn after_failure_hyro_native_refusal_stops_and_reports_each_serial_row() {
    assert_serial_plan_failure(Some(200));
}

#[test]
fn after_failure_hyro_lost_reply_stops_and_reports_unknown_serial_row() {
    assert_serial_plan_failure(None);
}

#[test]
fn after_failure_hyro_server_error_keeps_prefix_pending_and_unsent_rows() {
    assert_serial_plan_failure(Some(503));
}

fn assert_serial_plan_failure(status: Option<u16>) {
    // TS awaits serial creates: reference/bybit/hyrotrader.exchange.ts:352-356; hyrotrader.order-recovery.ts:92-106.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | Spot/Hyro serial runtime failure
    let variant = Variant::Hyrotrader;
    let mut h = ready(variant);
    let orders = (0..3)
        .map(|index| {
            let mut opts = order("BTCUSDT", 1.0);
            if variant == Variant::Spot {
                opts.client_order_id = Some(format!("journal-{index}"));
            }
            opts
        })
        .collect();
    place(&mut h, 146, orders);
    assert_eq!(h.requests.len(), 1, "{variant:?}: {:?}", h.done);
    let first = h.take("/v5/order/create");
    let first_link = link(&first);
    h.reply(first.id, 200, &serde_json::json!({"retCode":0,"result":{"orderId":"placed-first","orderLinkId":first_link}}).to_string());
    assert_eq!(h.requests.len(), 1, "{variant:?}: {:?}", h.done);
    let second = h.take("/v5/order/create");
    let second_link = link(&second);
    if let Some(status) = status {
        h.reply(
            second.id,
            status,
            r#"{"retCode":110007,"retMsg":"insufficient balance"}"#,
        );
    } else {
        h.event(Event::HttpFailed {
            request: second.id,
            error: "reply lost",
        });
    }
    let detail = h
        .notices
        .iter()
        .find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed {
                command: 146,
                error,
            } => Some(error),
            _ => None,
        })
        .expect("terminal failure");
    let elements = detail.elements.as_ref().expect("indexed write outcomes");
    assert_eq!(elements.len(), 3);
    for (index, row) in elements.iter().enumerate() {
        assert_eq!(row.element.index, index);
        if variant == Variant::Spot {
            assert_eq!(
                row.element.client_order_id,
                Some(format!("journal-{index}"))
            );
        } else {
            assert!(
                row.element
                    .client_order_id
                    .as_ref()
                    .is_some_and(|id| !id.is_empty())
            );
        }
    }
    assert_eq!(
        elements[0].element.client_order_id.as_ref(),
        Some(&first_link)
    );
    assert_eq!(
        elements[1].element.client_order_id.as_ref(),
        Some(&second_link)
    );
    use safe_cex_engine::write_plan::ElementResult;
    assert!(
        matches!(&elements[0].result, ElementResult::Placed { order_ids } if order_ids == &["placed-first"])
    );
    if status != Some(200) {
        assert!(
            matches!(&elements[1].result, ElementResult::Unknown { error } if error.outcome == Some(safe_cex_engine::Outcome::Unknown))
        );
    } else {
        assert!(
            matches!(&elements[1].result, ElementResult::NotSent { error: Some(error) } if error.outcome == Some(safe_cex_engine::Outcome::Rejected))
        );
    }
    assert_eq!(elements[2].result, ElementResult::NotSent { error: None });
    assert!(!matches!(
        detail.outcome,
        Some(safe_cex_engine::Outcome::NotSent | safe_cex_engine::Outcome::Rejected)
    ));
    assert_eq!(
        detail.partial.as_ref().expect("accepted prefix").order_ids,
        ["placed-first"]
    );
    assert!(
        !h.requests
            .iter()
            .any(|request| request.method == safe_cex_engine::Method::Post),
        "failed row released another write"
    );
    h.advance(1_201.0);
    assert!(
        !h.requests
            .iter()
            .any(|request| request.method == safe_cex_engine::Method::Post),
        "resend, suffix or rollback write"
    );
}

#[test]
fn legacy_serial_bbo_without_explicit_price_is_still_sent_once() {
    let mut h = ready(Variant::Inverse);
    let mut opts = order("BTCUSD", 1.0);
    opts.price = None;
    opts.reduce_only = Some(true);
    opts.client_order_id = Some("bbo-control".into());
    opts.bybit_bbo = Some(BybitBbo {
        side_type: "Queue".into(),
        level: 3.0,
    });
    place(&mut h, 143, vec![opts]);
    assert_eq!(h.requests.len(), 1);
    resolve_mode(&mut h, "BTCUSD");
    assert_eq!(h.requests.len(), 1);
    let request = h.take("/v5/order/create");
    let sent: serde_json::Value = serde_json::from_str(body(&request)).expect("create body");
    assert_eq!(sent["bboSideType"], "Queue");
    assert_eq!(sent["bboLevel"], "3");
    assert_eq!(sent["reduceOnly"], true);
    assert_eq!(sent["orderLinkId"], "bbo-control");
    assert!(sent.get("price").is_none());
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"bbo-accepted","orderLinkId":"bbo-control"}}"#,
    );
    assert_eq!(h.result(143), Some(&Ok("[\"bbo-accepted\"]".into())));
    assert!(h.requests.is_empty());
}

fn applied_failure(h: &Harness, command: u32) -> &safe_cex_engine::VenueError {
    h.notices
        .iter()
        .find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed {
                command: own,
                error,
            } if *own == command => Some(error),
            _ => None,
        })
        .expect("indexed terminal failure")
}

fn seed_applied_cancel_orders(h: &mut Harness) {
    let category = if h
        .engine
        .store()
        .market("BTCUSDT")
        .is_some_and(|market| market.is_spot == Some(true))
    {
        "spot"
    } else {
        "linear"
    };
    let frame = r#"{"topic":"order","data":[{"category":"spot","symbol":"BTCUSDT","orderId":"parent","orderStatus":"New","orderType":"Limit","side":"Buy","price":"100","qty":"1","cumExecQty":"0","leavesQty":"1"}]}"#.replace("\"category\":\"spot\"", &format!("\"category\":\"{category}\""));
    h.message("/v5/private", &frame);
    let base = h
        .engine
        .store()
        .order("parent")
        .expect("native seed")
        .clone();
    let mut orders = vec![base.clone()];
    for index in 0..3 {
        let mut order = base.clone();
        order.id = format!("cancel-{index}");
        orders.push(order);
    }
    let mut attached = base;
    attached.id = "parent__stop_loss".into();
    attached.parent_id = Some("parent".into());
    attached.kind = OrderType::StopLoss;
    attached.price = 90.0;
    orders.push(attached);
    h.engine.store_mut().set_orders(orders);
}

#[test]
fn after_failure_cancel_prefix_is_reported_with_targets_and_stops_the_suffix() {
    // TS awaits each cancel: reference/bybit/hyrotrader.exchange.ts:419-445.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | ordinary cancellation and cancel-all compensating re-place
    let mut h = ready(Variant::Spot);
    seed_applied_cancel_orders(&mut h);
    h.command(Command::CancelOrders {
        command: 151,
        ids: (0..3).map(|i| format!("cancel-{i}")).collect(),
    });
    let first = h.take("/v5/order/cancel");
    h.reply(
        first.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"cancel-0"}}"#,
    );
    let second = h.take("/v5/order/cancel");
    h.reply(
        second.id,
        200,
        r#"{"retCode":110007,"retMsg":"native refusal"}"#,
    );
    let elements = applied_failure(&h, 151)
        .elements
        .as_ref()
        .expect("cancel outcomes");
    use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
    assert_eq!(elements.len(), 3);
    assert!(
        matches!(&elements[0].result, ElementResult::Applied { effect: AppliedEffect::Cancel, target_order_id: Some(id), position_key: None } if id == "cancel-0")
    );
    assert!(
        matches!(&elements[1].result, ElementResult::NotSent { error: Some(error) } if error.outcome == Some(safe_cex_engine::Outcome::Rejected))
    );
    assert_eq!(elements[2].result, ElementResult::NotSent { error: None });
    h.advance(1_201.0);
    assert!(
        !h.requests
            .iter()
            .any(|request| request.method == safe_cex_engine::Method::Post)
    );
}

#[test]
fn applied_amend_prefix_names_existing_parent_without_placing_an_order() {
    let mut h = ready(Variant::Spot);
    seed_applied_cancel_orders(&mut h);
    h.command(Command::CancelOrders {
        command: 152,
        ids: vec![
            "parent__stop_loss".into(),
            "cancel-0".into(),
            "cancel-1".into(),
        ],
    });
    let first = h.take("/v5/order/amend");
    h.reply(
        first.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"parent"}}"#,
    );
    let second = h.take("/v5/order/cancel");
    h.reply(
        second.id,
        200,
        r#"{"retCode":110007,"retMsg":"native refusal"}"#,
    );
    let elements = applied_failure(&h, 152)
        .elements
        .as_ref()
        .expect("amend outcomes");
    use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
    assert!(
        matches!(&elements[0].result, ElementResult::Applied { effect: AppliedEffect::Amend, target_order_id: Some(id), .. } if id == "parent")
    );
    assert!(
        !elements
            .iter()
            .any(|row| matches!(row.result, ElementResult::Placed { .. }))
    );
    assert_eq!(elements[2].result, ElementResult::NotSent { error: None });
    assert!(
        !h.requests
            .iter()
            .any(|request| request.method == safe_cex_engine::Method::Post)
    );
}

#[test]
fn applied_trading_stop_prefix_survives_a_rejected_create_without_an_order_id() {
    let mut h = ready(Variant::Hyrotrader);
    h.message("/v5/private", r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"side":"Buy","size":"1","avgPrice":"100","markPrice":"100","positionValue":"100","leverage":"10","unrealisedPnl":"0","curRealisedPnl":"0","positionIM":"0"}]}"#);
    let mut stop = order("BTCUSDT", 1.0);
    stop.kind = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.price = Some(90.0);
    place(
        &mut h,
        154,
        vec![stop, order("BTCUSDT", 1.0), order("ETHUSDT", 1.0)],
    );
    let first = h.take("/v5/position/trading-stop");
    h.reply(first.id, 200, r#"{"retCode":0,"result":{}}"#);
    let second = h.take("/v5/order/create");
    h.reply(
        second.id,
        200,
        r#"{"retCode":110007,"retMsg":"native refusal"}"#,
    );
    let error = applied_failure(&h, 154);
    let elements = error.elements.as_ref().expect("trading-stop outcomes");
    use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
    assert_eq!(elements.len(), 3);
    assert!(
        matches!(&elements[0].result, ElementResult::Applied { effect: AppliedEffect::TradingStop, target_order_id: None, position_key: Some(key) } if key.contains("BTCUSDT"))
    );
    assert_eq!(elements[2].result, ElementResult::NotSent { error: None });
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
    assert!(
        !h.requests
            .iter()
            .any(|request| request.method == safe_cex_engine::Method::Post)
    );
}

#[test]
fn applied_cancel_batch_keeps_native_target_rows_and_stops_later_chunks() {
    for code in [10001, 10016] {
        let mut h = ready(Variant::Demo);
        seed_applied_cancel_orders(&mut h);
        let base = h
            .engine
            .store()
            .order("parent")
            .expect("native cancel seed")
            .clone();
        let orders = (0..12)
            .map(|index| {
                let mut order = base.clone();
                order.id = format!("batch-cancel-{index}");
                order
            })
            .collect();
        h.engine.store_mut().set_orders(orders);
        h.command(Command::CancelOrders {
            command: 155,
            ids: (0..12)
                .map(|index| format!("batch-cancel-{index}"))
                .collect(),
        });
        let request = h.take("/v5/order/cancel-batch");
        let wire: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        assert_eq!(wire["request"].as_array().unwrap().len(), 10);
        let rows: Vec<_> = (0..10)
            .map(|index| serde_json::json!({"orderId":format!("batch-cancel-{index}")}))
            .collect();
        let statuses: Vec<_> = (0..10).map(|index| serde_json::json!({"code":if index == 1 {code} else {0},"msg":"native cancel row"})).collect();
        h.reply(
            request.id,
            200,
            &serde_json::json!({"retCode":0,"result":{"list":rows},"retExtInfo":{"list":statuses}})
                .to_string(),
        );
        assert!(
            applied_failure(&h, 155)
                .partial
                .as_ref()
                .is_none_or(|partial| partial.order_ids.is_empty()),
            "cancel targets are Applied effects, never created-order IDs"
        );
        let partial = applied_failure(&h, 155)
            .partial
            .as_ref()
            .expect("whole caller cancellation counts");
        assert_eq!(partial.total, Some(12));
        assert_eq!(partial.failed_index, Some(1));
        assert_eq!(
            applied_failure(&h, 155).venue_code.as_deref(),
            Some(code.to_string().as_str())
        );
        let elements = applied_failure(&h, 155)
            .elements
            .as_ref()
            .expect("native cancel row outcomes");
        assert_eq!(elements.len(), 12);
        use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
        for (index, row) in elements.iter().enumerate() {
            match index {
                1 if code == 10001 => assert!(
                    matches!(&row.result, ElementResult::NotSent { error: Some(error) } if error.outcome == Some(safe_cex_engine::Outcome::Rejected))
                ),
                1 => assert!(
                    matches!(&row.result, ElementResult::Unknown { error } if error.outcome == Some(safe_cex_engine::Outcome::Unknown))
                ),
                10.. => assert_eq!(row.result, ElementResult::NotSent { error: None }),
                _ => assert!(
                    matches!(&row.result, ElementResult::Applied { effect: AppliedEffect::Cancel, target_order_id: Some(id), .. } if id == &format!("batch-cancel-{index}"))
                ),
            }
        }
        assert!(
            !elements
                .iter()
                .any(|row| matches!(row.result, ElementResult::Placed { .. }))
        );
        h.advance(1_201.0);
        assert!(
            !h.requests
                .iter()
                .any(|request| request.method == safe_cex_engine::Method::Post)
        );
    }
}

#[test]
fn legacy_non_option_price_and_notional_do_not_gain_a_whole_plan_gate() {
    // TS per-order formatting: reference/bybit/bybitspot.exchange.ts:765-795,881-888; hyrotrader.exchange.ts:342-365.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | create wire quantity/price/minimum-notional adoption gate
    for variant in [Variant::Spot, Variant::Hyrotrader] {
        for (price, amount) in [(None, 1.0), (Some(0.0), 1.0), (Some(100.0), 0.01)] {
            let mut h = ready(variant);
            let mut last = order("BTCUSDT", amount);
            last.price = price;
            let mut first = order("BTCUSDT", 1.0);
            if variant == Variant::Spot {
                first.client_order_id = Some("parity-prefix".into());
                last.client_order_id = Some("parity-last".into());
            }
            place(&mut h, 156, vec![first, last]);
            let first = h.take("/v5/order/create");
            let first_link = link(&first);
            h.reply(first.id, 200, &serde_json::json!({"retCode":0,"result":{"orderId":"legacy-prefix","orderLinkId":first_link}}).to_string());
            let second = h.take("/v5/order/create");
            let wire: serde_json::Value = serde_json::from_str(body(&second)).unwrap();
            assert_eq!(wire["qty"], amount.to_string());
            if price.is_none() || price == Some(0.0) {
                assert!(wire.get("price").is_none());
            }
            h.reply(
                second.id,
                200,
                r#"{"retCode":110007,"retMsg":"native rejection"}"#,
            );
            let error = applied_failure(&h, 156);
            assert_eq!(error.partial.as_ref().unwrap().order_ids, ["legacy-prefix"]);
            assert!(matches!(
                error.elements.as_ref().unwrap()[0].result,
                safe_cex_engine::write_plan::ElementResult::Placed { .. }
            ));
        }
    }
}

#[test]
fn after_failure_spot_finishes_the_current_orders_lots_then_stops() {
    // TS completes payloads, then stops outer orders: reference/bybit/bybitspot.exchange.ts:844-870,881-888.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | Spot/Hyro serial runtime failure
    for (failed, code) in [(0, 110007), (1, 110007)] {
        let mut h = spot_split_plan_market("0.001", "1");
        let first = order("BTCUSDT", 3.0);
        let mut later = order("BTCUSDT", 1.0);
        later.client_order_id = Some("untouched-next-order".into());
        place(&mut h, 160, vec![first, later]);
        for index in 0..3 {
            let request = h.take("/v5/order/create");
            assert!(!body(&request).contains("untouched-next-order"));
            if index == failed {
                h.reply(
                    request.id,
                    200,
                    &serde_json::json!({"retCode":code,"retMsg":"lot refused"}).to_string(),
                );
            } else {
                h.reply(
                    request.id,
                    200,
                    &serde_json::json!({"retCode":0,"result":{"orderId":format!("lot-{index}")}})
                        .to_string(),
                );
            }
        }
        h.advance(1_201.0);
        assert!(!h.has_request("/v5/order/create"));
        assert!(matches!(h.result(160), Some(Err(_))));
        let elements = applied_failure(&h, 160).elements.as_ref().unwrap();
        assert_eq!(elements.len(), 4);
        for (index, row) in elements.iter().enumerate() {
            use safe_cex_engine::write_plan::ElementResult;
            match index {
                3 => assert_eq!(row.result, ElementResult::NotSent { error: None }),
                _ if index == failed && code == 10016 => {
                    assert!(matches!(row.result, ElementResult::Unknown { .. }))
                }
                _ if index == failed => assert!(matches!(
                    row.result,
                    ElementResult::NotSent { error: Some(_) }
                )),
                _ => assert!(matches!(row.result, ElementResult::Placed { .. })),
            }
        }
    }
}

#[test]
fn after_failure_spot_cancel_all_does_not_release_another_family() {
    // TS stops its failed cancel action: reference/bybit/bybitspot.exchange.ts:1179-1216.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | ordinary cancellation and cancel-all compensating re-place
    let mut h = ready(Variant::Spot);
    seed_applied_cancel_orders(&mut h);
    h.command(Command::CancelSymbolOrders {
        command: 161,
        symbol: "BTCUSDT".into(),
    });
    let request = h.take("/v5/order/cancel-all");
    h.reply(
        request.id,
        200,
        r#"{"retCode":110007,"retMsg":"cancel refused"}"#,
    );
    h.advance(1_201.0);
    assert!(!h.has_request("/v5/order/cancel-all"));
    assert!(matches!(h.result(161), Some(Err(_))));
}

#[test]
fn after_failure_native_batch_row_refusal_keeps_the_later_chunk() {
    // TS continues definite row refusals: reference/bybit/bybit.exchange.ts:3077-3106.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | regular Linear/Inverse create-batch runtime continuation and fallback
    let mut h = ready(Variant::Demo);
    place(
        &mut h,
        162,
        (0..12).map(|_| order("BTCUSDT", 1.0)).collect(),
    );
    resolve_mode(&mut h, "BTCUSDT");
    let first = h.take("/v5/order/create-batch");
    let rows: Vec<_> = (0..10).map(|i| serde_json::json!({"orderId":if i == 1 {String::new()} else {format!("batch-{i}")}})).collect();
    let statuses: Vec<_> = (0..10)
        .map(|i| serde_json::json!({"code":if i == 1 {110007} else {0},"msg":"native row"}))
        .collect();
    h.reply(
        first.id,
        200,
        &serde_json::json!({"retCode":0,"result":{"list":rows},"retExtInfo":{"list":statuses}})
            .to_string(),
    );
    h.advance(1_201.0);
    let next = h.take("/v5/order/create-batch");
    let wire: serde_json::Value = serde_json::from_str(body(&next)).unwrap();
    assert_eq!(wire["request"].as_array().unwrap().len(), 2);
    h.reply(next.id, 200, r#"{"retCode":0,"result":{"list":[{"orderId":"last-0"},{"orderId":"last-1"}]},"retExtInfo":{"list":[{"code":0},{"code":0}]}}"#);
    assert!(!h.has_request("/v5/order/create"));
    assert!(!h.has_request("/v5/order/create-batch"));
    let elements = applied_failure(&h, 162).elements.as_ref().unwrap();
    use safe_cex_engine::write_plan::ElementResult;
    assert_eq!(elements.len(), 12);
    assert!(
        matches!(&elements[10].result, ElementResult::Placed { order_ids } if order_ids == &["last-0"])
    );
    assert!(
        matches!(&elements[11].result, ElementResult::Placed { order_ids } if order_ids == &["last-1"])
    );
    assert!(matches!(
        elements[1].result,
        ElementResult::NotSent { error: Some(_) }
    ));
}

#[test]
fn pinfix_10001_batch_fallback_continues_past_a_refused_single() {
    // Preserve master's 10001 trigger; TS catches each fallback refusal: reference/bybit/bybit.exchange.ts:3015-3042.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | regular Linear/Inverse create-batch runtime continuation and fallback
    let code = 10001;
    let mut h = ready(Variant::Demo);
    place(
        &mut h,
        163,
        vec![order("BTCUSDT", 1.0), order("BTCUSDT", 1.0)],
    );
    resolve_mode(&mut h, "BTCUSDT");
    let batch = h.take("/v5/order/create-batch");
    h.reply(
        batch.id,
        200,
        &serde_json::json!({"retCode":code,"retMsg":"batch refused"}).to_string(),
    );
    let first = h.take("/v5/order/create");
    h.reply(
        first.id,
        200,
        r#"{"retCode":110007,"retMsg":"first refused"}"#,
    );
    let second = h.take("/v5/order/create");
    h.reply(
        second.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"fallback-last"}}"#,
    );
    assert!(!h.has_request("/v5/order/create"));
    assert!(matches!(h.result(163), Some(Err(_))));
    let elements = applied_failure(&h, 163).elements.as_ref().unwrap();
    assert_eq!(elements.len(), 2);
    use safe_cex_engine::write_plan::ElementResult;
    assert!(matches!(
        elements[0].result,
        ElementResult::NotSent { error: Some(_) }
    ));
    assert!(
        matches!(&elements[1].result, ElementResult::Placed { order_ids } if order_ids == &["fallback-last"])
    );
}

#[test]
fn after_failure_special_tpsl_keeps_the_regular_order_batch() {
    // TS catches special refusals: reference/bybit/bybit.exchange.ts:2969-2985.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | regular Linear/Inverse create-batch runtime continuation and fallback
    let mut h = ready(Variant::Demo);
    h.message("/v5/private", r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"side":"Buy","size":"1","avgPrice":"100","markPrice":"100","positionValue":"100","leverage":"10","unrealisedPnl":"0","curRealisedPnl":"0","positionIM":"0"}]}"#);
    let mut stop = order("BTCUSDT", 0.0);
    stop.kind = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.price = Some(90.0);
    place(&mut h, 164, vec![stop, order("BTCUSDT", 1.0)]);
    let request = h.take("/v5/position/trading-stop");
    h.reply(
        request.id,
        200,
        r#"{"retCode":110007,"retMsg":"stop refused"}"#,
    );
    let batch = h.take("/v5/order/create-batch");
    h.reply(batch.id, 200, r#"{"retCode":0,"result":{"list":[{"orderId":"regular-last"}]},"retExtInfo":{"list":[{"code":0}]}}"#);
    assert!(!h.has_request("/v5/position/trading-stop"));
    assert!(matches!(h.result(164), Some(Err(_))));
    let elements = applied_failure(&h, 164).elements.as_ref().unwrap();
    assert_eq!(elements.len(), 2);
    use safe_cex_engine::write_plan::ElementResult;
    assert!(matches!(
        elements[0].result,
        ElementResult::NotSent { error: Some(_) }
    ));
    assert!(
        matches!(&elements[1].result, ElementResult::Placed { order_ids } if order_ids == &["regular-last"])
    );
}

#[test]
fn parity_exception_unknown_spot_halts_lots_and_later_caller_orders() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at reference/bybit/bybitspot.exchange.ts:844-856.
    for split in [false, true] {
        for status in [Some(200), Some(503), None] {
            let mut h = if split {
                spot_split_plan_market("0.001", "1")
            } else {
                ready(Variant::Spot)
            };
            let orders = if split {
                vec![order("BTCUSDT", 3.0), order("BTCUSDT", 1.0)]
            } else {
                vec![order("BTCUSDT", 1.0); 3]
            };
            place(&mut h, 175, orders);
            let first = h.take("/v5/order/create");
            h.reply(
                first.id,
                200,
                r#"{"retCode":0,"result":{"orderId":"known-prefix"}}"#,
            );
            let second = h.take("/v5/order/create");
            if let Some(status) = status {
                h.reply(
                    second.id,
                    status,
                    r#"{"retCode":10016,"retMsg":"uncertain"}"#,
                );
            } else {
                h.event(Event::HttpFailed {
                    request: second.id,
                    error: "reply lost",
                });
            }
            support::assert_unknown_pin(
                &mut h,
                175,
                &["known-prefix"],
                1,
                if split { 4 } else { 3 },
            );
        }
    }
}

#[test]
fn parity_exception_unknown_batch_halts_chunks_and_single_fallback() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at reference/bybit/bybit.exchange.ts:3015-3042,3077-3106.
    for row_unknown in [false, true] {
        let mut h = ready(Variant::Demo);
        place(
            &mut h,
            176,
            (0..12).map(|_| order("BTCUSDT", 1.0)).collect(),
        );
        resolve_mode(&mut h, "BTCUSDT");
        let batch = h.take("/v5/order/create-batch");
        if row_unknown {
            let rows: Vec<_> = (0..10).map(|i| serde_json::json!({"orderId":if i==1 {String::new()} else {format!("known-{i}")}})).collect();
            let statuses: Vec<_> = (0..10)
                .map(|i| serde_json::json!({"code":if i==1 {10016} else {0},"msg":"uncertain"}))
                .collect();
            h.reply(batch.id, 200, &serde_json::json!({"retCode":0,"result":{"list":rows},"retExtInfo":{"list":statuses}}).to_string());
            let ids: Vec<_> = (0..10)
                .filter(|i| *i != 1)
                .map(|i| format!("known-{i}"))
                .collect();
            let refs: Vec<_> = ids.iter().map(String::as_str).collect();
            support::assert_unknown_pin(&mut h, 176, &refs, 1, 12);
        } else {
            h.reply(batch.id, 200, r#"{"retCode":10016,"retMsg":"uncertain"}"#);
            support::assert_unknown_pin(&mut h, 176, &[], 0, 12);
        }
    }
}

#[test]
fn parity_exception_unknown_fallback_halts_remaining_singles() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at reference/bybit/bybit.exchange.ts:3026-3042.
    for status in [Some(200), Some(503), None] {
        let mut h = ready(Variant::Demo);
        place(&mut h, 177, vec![order("BTCUSDT", 1.0); 4]);
        resolve_mode(&mut h, "BTCUSDT");
        let batch = h.take("/v5/order/create-batch");
        h.reply(batch.id, 200, r#"{"retCode":10001,"retMsg":"use singles"}"#);
        let refused = h.take("/v5/order/create");
        h.reply(
            refused.id,
            200,
            r#"{"retCode":110007,"retMsg":"refused single"}"#,
        );
        let first = h.take("/v5/order/create");
        h.reply(
            first.id,
            200,
            r#"{"retCode":0,"result":{"orderId":"known-prefix"}}"#,
        );
        let second = h.take("/v5/order/create");
        if let Some(status) = status {
            h.reply(
                second.id,
                status,
                r#"{"retCode":10016,"retMsg":"uncertain"}"#,
            );
        } else {
            h.event(Event::HttpFailed {
                request: second.id,
                error: "reply lost",
            });
        }
        // Engine-owned HTTP seals retain their own canonical text after a refusal.
        let text = if status == Some(503) {
            support::ENGINE_UNKNOWN_AFTER_REFUSAL_TEXT
        } else {
            support::ENGINE_UNKNOWN_TEXT
        };
        support::assert_unknown_pin_with_text(&mut h, 177, &["known-prefix"], 2, 4, text);
    }
}

#[test]
fn parity_exception_unknown_special_halts_regular_batch() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at reference/bybit/bybit.exchange.ts:2969-2985.
    for lost in [false, true] {
        let mut h = ready(Variant::Demo);
        h.message("/v5/private", r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"side":"Buy","size":"1","avgPrice":"100","markPrice":"100","positionValue":"100","leverage":"10","unrealisedPnl":"0","curRealisedPnl":"0","positionIM":"0"}]}"#);
        let mut stop = order("BTCUSDT", 0.0);
        stop.kind = OrderType::StopLoss;
        stop.side = OrderSide::Sell;
        stop.price = Some(90.0);
        place(&mut h, 178, vec![stop, order("BTCUSDT", 1.0)]);
        let request = h.take("/v5/position/trading-stop");
        if lost {
            h.event(Event::HttpFailed {
                request: request.id,
                error: "reply lost",
            });
        } else {
            h.reply(request.id, 200, r#"{"retCode":10016,"retMsg":"uncertain"}"#);
        }
        support::assert_unknown_pin(&mut h, 178, &[], 0, 2);
    }
}

#[test]
fn pin_caller_identity_and_execution_checks_precede_the_valid_prefix() {
    // TS whole-command checks: reference/bybit/bybit.exchange.ts:2940-2946; bybitspot.exchange.ts:881-885.
    for variant in [Variant::Demo, Variant::Spot, Variant::Hyrotrader] {
        for fault in 0..2 {
            let mut h = ready(variant);
            if variant == Variant::Demo {
                place(&mut h, 183, vec![order("BTCUSDT", 1.0)]);
                resolve_mode(&mut h, "BTCUSDT");
                let setup = h.take("/v5/order/create-batch");
                h.reply(setup.id, 200, r#"{"retCode":0,"result":{"list":[{"orderId":"setup-mode"}]},"retExtInfo":{"list":[{"code":0}]}}"#);
            }
            let mut invalid = order("BTCUSDT", 1.0);
            if fault == 0 {
                invalid.client_order_id = Some("not valid!".into());
            } else {
                invalid.bybit_bbo = Some(BybitBbo {
                    side_type: "Counterparty".into(),
                    level: 6.0,
                });
            }
            place(&mut h, 181, vec![order("BTCUSDT", 1.0), invalid]);
            assert!(
                matches!(h.result(181), Some(Err(_))),
                "variant={variant:?} fault={fault} done={:?} paths={:?}",
                h.done,
                h.paths()
            );
            assert!(
                h.requests.is_empty(),
                "whole-command preflight: {variant:?}"
            );
        }
    }
}

#[test]
fn pin_linear_and_inverse_price_rows_are_sent_in_the_native_batch() {
    // TS formats per chunk: reference/bybit/bybit.exchange.ts:3015-3021,3119-3185; bybitinverse.exchange.ts:1933-2034.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | trading.rs:669 category-batch-and-serial-chunking / plain Limit missing-or-zero price
    for variant in [Variant::Demo, Variant::Inverse] {
        for price in [None, Some(0.0)] {
            let mut h = ready(variant);
            let symbol = if variant == Variant::Inverse {
                "BTCUSD"
            } else {
                "BTCUSDT"
            };
            let mut invalid = order(symbol, 1.0);
            invalid.price = price;
            place(&mut h, 182, vec![order(symbol, 1.0), invalid]);
            resolve_mode(&mut h, symbol);
            let request = h.take("/v5/order/create-batch");
            let wire: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
            assert_eq!(wire["request"].as_array().unwrap().len(), 2);
            assert_eq!(wire["request"][0]["qty"], "1");
            assert_eq!(wire["request"][1]["qty"], "1");
            assert!(wire["request"][1].get("price").is_none());
            h.reply(request.id, 200, r#"{"retCode":0,"result":{"list":[{"orderId":"known-prefix"},{}]},"retExtInfo":{"list":[{"code":0},{"code":110007,"msg":"missing price"}]}}"#);
            assert!(matches!(h.result(182), Some(Err(_))));
            assert!(
                !h.requests
                    .iter()
                    .any(|r| r.method == safe_cex_engine::Method::Post)
            );
        }
    }
}

#[test]
fn parity_exception_lost_native_batch_halts_chunks_without_fallback() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at reference/bybit/bybit.exchange.ts:3015-3042.
    let mut h = ready(Variant::Demo);
    place(&mut h, 184, vec![order("BTCUSDT", 1.0); 12]);
    resolve_mode(&mut h, "BTCUSDT");
    let batch = h.take("/v5/order/create-batch");
    h.event(Event::HttpFailed {
        request: batch.id,
        error: "reply lost",
    });
    support::assert_unknown_pin(&mut h, 184, &[], 0, 12);
}

#[test]
fn pinfix_nontrigger_batch_refusal_never_falls_back_to_singles() {
    // TS fallback context: reference/bybit/bybit.exchange.ts:3015-3042.
    // The lead's preservation rule retains master's stricter 10001-only trigger.
    for code in [110007, 10003] {
        let mut h = ready(Variant::Demo);
        place(&mut h, 185, vec![order("BTCUSDT", 1.0); 2]);
        resolve_mode(&mut h, "BTCUSDT");
        let batch = h.take("/v5/order/create-batch");
        h.reply(
            batch.id,
            200,
            &serde_json::json!({"retCode":code,"retMsg":"definite batch refusal"}).to_string(),
        );
        h.advance(1_201.0);
        assert!(
            !h.has_request("/v5/order/create"),
            "nontrigger {code} released fallback"
        );
        assert!(!h.has_request("/v5/order/create-batch"));
        let error = applied_failure(&h, 185);
        assert_eq!(error.venue_code.as_deref(), Some(code.to_string().as_str()));
        let elements = error.elements.as_ref().unwrap();
        assert_eq!(elements.len(), 2);
        assert!(elements.iter().all(|row| matches!(&row.result,
            safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(error) }
                if error.venue_code.as_deref() == Some(code.to_string().as_str()))));
    }
}

#[test]
fn spot_split_limit_entries_follow_legacy_protection_attachment() {
    for entries in [1, 2] {
        let mut h = ready(Variant::Spot);
        let orders = (0..entries)
            .map(|_| {
                let mut opts = order("BTCUSDT", 2500.);
                opts.take_profit = Some(110.);
                opts
            })
            .collect();
        place(&mut h, 3, orders);
        let mut ids = Vec::new();
        for index in 0..entries * 4 {
            let request = h.take("/v5/order/create");
            let sent: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
            assert_eq!(sent["category"], "spot");
            assert_eq!(sent["orderType"], "Limit");
            let protected = index % 4 == 0 || index % 4 == 3;
            assert_eq!(sent.get("takeProfit").is_some(), protected);
            assert_eq!(sent.get("tpOrderType").is_some(), protected);
            assert_eq!(sent["qty"], if index % 4 == 3 { "120" } else { "793.333" });
            let id = format!("spot-{index}");
            h.reply(
                request.id,
                200,
                &serde_json::json!({
                    "retCode": 0, "result": {"orderId": id}
                })
                .to_string(),
            );
            ids.push(id);
        }
        assert_eq!(h.result(3), Some(&Ok(serde_json::to_string(&ids).unwrap())));
        assert!(!h.unknown.contains(&3));
        assert!(!h.has_request("/v5/order/create"));
    }
}

fn spot_parity_error(h: &Harness, command: u32) -> &safe_cex_engine::VenueError {
    h.notices
        .iter()
        .find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed { command: id, error } if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .expect("Spot command journal/telemetry failure")
}

#[test]
fn spot_ts_parity_sends_the_below_minimum_tail_after_valid_lots() {
    // TS reference/bybit/bybitspot.exchange.ts:828-846 sends [595,595,0.001].
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit-spot | split-tail
    let mut h = spot_split_plan_market("0.01", "1190");
    place(&mut h, 191, vec![protected_spot_split_order(1190.001)]);
    for (i, qty) in ["595", "595", "0.001"].iter().enumerate() {
        let request = h.take("/v5/order/create");
        let wire: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        assert_eq!(wire["qty"], *qty);
        if i == 2 {
            h.reply(
                request.id,
                200,
                r#"{"retCode":110007,"retMsg":"tail refused"}"#,
            );
        } else {
            h.reply(
                request.id,
                200,
                &serde_json::json!({"retCode":0,"result":{"orderId":format!("placed-{i}")}})
                    .to_string(),
            );
        }
    }
    let error = spot_parity_error(&h, 191);
    assert_eq!(
        error.partial.as_ref().unwrap().order_ids,
        ["placed-0", "placed-1"]
    );
    h.advance(1_201.0);
    assert!(!h.has_request("/v5/order/create"));
}

#[test]
fn spot_ts_parity_batch_keeps_prefix_before_a_missing_market() {
    // TS reference/bybit/bybitspot.exchange.ts:755-758,881-887 formats each order in series.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit-spot | multi-order-batch
    let mut h = ready(Variant::Spot);
    place(
        &mut h,
        192,
        vec![
            order("BTCUSDT", 1.0),
            order("MISSINGUSDT", 1.0),
            order("BTCUSDT", 1.0),
        ],
    );
    let first = h.take("/v5/order/create");
    h.reply(
        first.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"placed-prefix"}}"#,
    );
    let error = spot_parity_error(&h, 192);
    assert!(error.message.contains("Market MISSINGUSDT not found"));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["placed-prefix"]);
    assert!(!h.has_request("/v5/order/create"));
}

#[test]
fn spot_ts_parity_protects_only_first_main_lot_and_remainder() {
    // TS reference/bybit/bybitspot.exchange.ts:833-841 omits brackets after main lot zero.
    // KNOWN-ISSUE: legacy TS under-allocates (sum 2499.999 of requested 2500); kept for exact TS parity per Sam's decision
    let mut h = ready(Variant::Spot);
    place(&mut h, 193, vec![protected_spot_split_order(2500.0)]);
    let mut sum = 0.0;
    for (i, qty) in ["793.333", "793.333", "793.333", "120"].iter().enumerate() {
        let request = h.take("/v5/order/create");
        let wire: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        assert_eq!(wire["qty"], *qty);
        sum += wire["qty"].as_str().unwrap().parse::<f64>().unwrap();
        for key in ["stopLoss", "takeProfit", "slOrderType", "tpOrderType"] {
            assert_eq!(wire.get(key).is_some(), i == 0 || i == 3, "{key} lot {i}");
        }
        h.reply(
            request.id,
            200,
            &serde_json::json!({"retCode":0,"result":{"orderId":format!("placed-{i}")}})
                .to_string(),
        );
    }
    assert!(matches!(h.result(193), Some(Ok(_))));
    assert!((sum - 2499.999).abs() < 1e-9, "sum {sum}, requested 2500");
    assert!(!h.has_request("/v5/order/create"));
}

#[test]
fn spot_ts_parity_definite_lot_refusal_finishes_order_then_stops_batch() {
    // TS reference/bybit/bybitspot.exchange.ts:844-870,881-887 finishes lots then throws.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit-spot | multi-order-batch
    let mut h = spot_split_plan_market("0.01", "1190");
    place(
        &mut h,
        194,
        vec![protected_spot_split_order(1190.01), order("BTCUSDT", 1.0)],
    );
    for i in 0..3 {
        let request = h.take("/v5/order/create");
        let wire: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        assert_eq!(wire["qty"], ["595", "595", "0.01"][i]);
        if i == 0 {
            h.reply(
                request.id,
                200,
                r#"{"retCode":110007,"retMsg":"first lot refused"}"#,
            );
        } else {
            h.reply(
                request.id,
                200,
                &serde_json::json!({"retCode":0,"result":{"orderId":format!("placed-{i}")}})
                    .to_string(),
            );
        }
    }
    h.advance(1_201.0);
    assert!(
        !h.has_request("/v5/order/create"),
        "next caller order must stop"
    );
    assert_eq!(
        spot_parity_error(&h, 194)
            .partial
            .as_ref()
            .unwrap()
            .order_ids,
        ["placed-1", "placed-2"]
    );
}

#[test]
fn spot_ts_parity_unknown_stops_lots_and_batch_with_canonical_evidence() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at reference/bybit/bybitspot.exchange.ts:844-856.
    const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";
    for status in [Some(200), Some(503), None] {
        let mut h = spot_split_plan_market("0.01", "1190");
        place(
            &mut h,
            195,
            vec![protected_spot_split_order(1190.01), order("BTCUSDT", 1.0)],
        );
        let first = h.take("/v5/order/create");
        h.reply(
            first.id,
            200,
            r#"{"retCode":0,"result":{"orderId":"placed-prefix"}}"#,
        );
        let uncertain = h.take("/v5/order/create");
        if let Some(status) = status {
            h.reply(
                uncertain.id,
                status,
                r#"{"retCode":10016,"retMsg":"unconfirmed"}"#,
            );
        } else {
            h.event(Event::HttpFailed {
                request: uncertain.id,
                error: "reply lost",
            });
        }
        h.advance(1_201.0);
        assert!(
            !h.has_request("/v5/order/create"),
            "no tail, next caller or retry"
        );
        let error = spot_parity_error(&h, 195);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert!(error.message.contains(ENGINE_UNKNOWN_TEXT), "{error:?}");
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["placed-prefix"]);
        assert!(h.unknown.contains(&195));
    }
}

#[test]
fn spot_ts_parity_identity_and_execution_controls_remain_atomic() {
    // TS reference/bybit/bybitspot.exchange.ts:881-885 checks IDs/execution before mapSeries.
    for identity in [true, false] {
        let mut h = ready(Variant::Spot);
        let mut invalid = order("BTCUSDT", 1.0);
        if identity {
            invalid.client_order_id = Some("invalid id!".into());
        } else {
            invalid.bybit_bbo = Some(BybitBbo {
                side_type: "Queue".into(),
                level: 1.0,
            });
        }
        place(&mut h, 197, vec![order("BTCUSDT", 1.0), invalid]);
        assert!(matches!(h.result(197), Some(Err(_))));
        assert!(
            h.requests.is_empty(),
            "atomic TS checks must precede the valid prefix"
        );
    }
}

#[test]
fn hyro_v1_already_gone_chase_cancel_is_satisfied_without_resending() {
    let mut h = ready(Variant::Hyrotrader);
    place(&mut h, 991, vec![order("BTCUSDT", 0.1)]);
    created(&mut h, "chase-gone");
    h.command(Command::CancelOrders {
        command: 992,
        ids: vec!["chase-gone".into()],
    });
    let request = h.take("/v5/order/cancel");
    assert_eq!(
        Harness::header(&request, "Content-Type").as_deref(),
        Some("application/json")
    );
    h.reply(
        request.id,
        200,
        r#"{"retCode":110001,"retMsg":"already filled"}"#,
    );
    assert!(h.result(992).expect("cancel completes").is_ok());
    assert!(!h.unknown.contains(&992));
    assert!(!h.has_request("/v5/order/cancel"));
    answer_reconcile(&mut h);
    assert!(!h.has_request("/v5/order/cancel"));
}

#[test]
fn hyro_v1_conditional_trigger_direction_is_numeric_and_writes_are_strict_json() {
    for (side, expected) in [(OrderSide::Buy, 1), (OrderSide::Sell, 2)] {
        let mut h = ready(Variant::Hyrotrader);
        let mut opts = order("BTCUSDT", 0.1);
        opts.kind = OrderType::Conditional;
        opts.side = side;
        opts.trigger_price = Some(if expected == 1 { 70000. } else { 60000. });
        place(&mut h, 990, vec![opts]);
        let request = h.take("/v5/order/create");
        assert_eq!(
            Harness::header(&request, "Content-Type").as_deref(),
            Some("application/json")
        );
        let payload: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
        assert_eq!(payload["triggerDirection"].as_i64(), Some(expected));
    }
}

#[test]
fn hyro_v1_unsupported_modes_are_typed_and_send_nothing() {
    use safe_cex_engine::{Action, ErrorKind, Notice, Outcome};
    for command in [
        Command::SetMarginType {
            command: 993,
            symbol: "BTCUSDT".into(),
            isolated: true,
        },
        Command::SetPositionMode {
            command: 993,
            hedged: true,
            symbol: None,
        },
    ] {
        let mut h = ready(Variant::Hyrotrader);
        h.engine.handle_command(h.now, command, None).unwrap();
        let actions = safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now);
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed {
                    command: 993,
                    error,
                }) => Some(error),
                _ => None,
            })
            .expect("typed refusal");
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert!(error.message.contains("only"));
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn inverse_create_keeps_legacy_integer_clamp_before_allocation() {
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
    // reference/bybit/bybitinverse.exchange.ts rounds and clamps before allocation.
    // KNOWN-ISSUE: legacy TS under-allocates (sum 10 of requested 25.6); kept for exact TS parity per Sam's decision
    let wire: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
    let quantities: Vec<_> = wire["request"]
        .as_array()
        .unwrap()
        .iter()
        .map(|order| order["qty"].as_str().unwrap())
        .collect();
    assert_eq!(quantities, ["10"]);
    let total: f64 = quantities
        .iter()
        .map(|qty| qty.parse::<f64>().unwrap())
        .sum();
    assert_eq!(total, 10.0);
}

#[test]
fn inverse_position_mode_legacy_success_has_no_second_coin() {
    let mut h = ready(Variant::Inverse);
    let mut second = h.engine.store().markets()[0].clone();
    second.base = "ZZZ".into();
    second.symbol = "ZZZUSD".into();
    second.id = "ZZZUSD".into();
    let mut markets = h.engine.store().markets().to_vec();
    markets.push(second);
    h.engine.store_mut().set_markets(markets);
    h.command(Command::SetPositionMode {
        command: 1,
        hedged: true,
        symbol: None,
    });
    let request = h.take("/v5/position/switch-mode");
    assert_eq!(
        body(&request),
        r#"{"category":"linear","coin":"USDT","mode":3}"#
    );
    h.reply(request.id, 200, r#"{"retCode":0,"result":{}}"#);
    assert!(
        !h.has_request("/v5/position/switch-mode"),
        "{:?}",
        h.paths()
    );
    h.advance(250.0);
    assert!(
        !h.has_request("/v5/position/switch-mode"),
        "{:?}",
        h.paths()
    );
    h.advance(60_000.0);
    assert!(!h.has_request("/v5/position/switch-mode"));
    assert_eq!(h.result(1), Some(&Ok("[]".into())));
}

#[test]
fn inverse_position_mode_legacy_discards_a_named_symbol() {
    let mut h = ready(Variant::Inverse);
    h.command(Command::SetPositionMode {
        command: 1,
        hedged: true,
        symbol: Some("BTCUSD".into()),
    });
    let request = h.take("/v5/position/switch-mode");
    assert_eq!(
        body(&request),
        r#"{"category":"linear","coin":"USDT","mode":3}"#
    );
    h.reply(request.id, 200, r#"{"retCode":0,"result":{}}"#);
    assert_eq!(h.result(1), Some(&Ok("[]".into())));
}

#[test]
fn inverse_position_mode_legacy_refusal_has_no_second_coin() {
    let mut h = ready(Variant::Inverse);
    let mut second = h.engine.store().markets()[0].clone();
    second.base = "ZZZ".into();
    second.symbol = "ZZZUSD".into();
    second.id = "ZZZUSD".into();
    h.engine.store_mut().update_markets(|_| {});
    let mut markets = h.engine.store().markets().to_vec();
    markets.push(second);
    h.engine.store_mut().set_markets(markets);
    h.command(Command::SetPositionMode {
        command: 1,
        hedged: true,
        symbol: None,
    });
    let request = h.take("/v5/position/switch-mode");
    assert_eq!(
        body(&request),
        r#"{"category":"linear","coin":"USDT","mode":3}"#
    );
    h.reply(request.id, 200, r#"{"retCode":10001,"retMsg":"refused"}"#);
    h.advance(250.0);
    assert!(
        !h.has_request("/v5/position/switch-mode"),
        "{:?}",
        h.paths()
    );
    h.advance(60_000.0);
    assert!(!h.has_request("/v5/position/switch-mode"));
    assert!(
        h.result(1)
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("refused")
    );
}
