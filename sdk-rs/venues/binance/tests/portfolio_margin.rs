//! Portfolio margin's USD-M side against Binance's documented payloads.
use support::Options;
mod support;

use safe_cex_engine::model::{MarginType, OrderSide, OrderStatus, OrderType, PlaceOrderOpts};
use safe_cex_engine::{Command, Event, Method};

use support::Harness;

const POSITION_RISK: &str = r#"[{"entryPrice":"95000.0","leverage":"20","markPrice":"96000.0","maxNotionalValue":"6000000.0","positionAmt":"0.010","notional":"960.0","symbol":"BTCUSDT","unRealizedProfit":"10.0","liquidationPrice":"80000.5","positionSide":"BOTH","updateTime":1720736417660}]"#;
const BALANCE: &str = r#"[{"asset":"USDT","totalWalletBalance":"1000.0","crossMarginAsset":"0","crossMarginBorrowed":"0","crossMarginFree":"900.0","crossMarginInterest":"0","crossMarginLocked":"0","umWalletBalance":"1000.0","umUnrealizedPNL":"10.0","cmWalletBalance":"0","cmUnrealizedPNL":"0","updateTime":0,"negativeBalance":"0"},{"asset":"BNB","totalWalletBalance":"5","crossMarginFree":"5","umUnrealizedPNL":"0"}]"#;
const OPEN_ORDERS: &str = r#"[{"avgPrice":"0.00000","clientOrderId":"x-s3vFFRQpresting","cumQuote":"0","executedQty":"0","orderId":1917641,"origQty":"0.40","origType":"LIMIT","price":"90000","reduceOnly":false,"side":"BUY","positionSide":"BOTH","status":"NEW","symbol":"BTCUSDT","time":1579276756075,"timeInForce":"GTC","type":"LIMIT","updateTime":1579276756075,"selfTradePreventionMode":"NONE","goodTillDate":0}]"#;
const OPEN_CONDITIONAL: &str = r#"[{"newClientStrategyId":"abc","strategyId":123445,"strategyStatus":"NEW","strategyType":"TAKE_PROFIT_MARKET","origQty":"0.010","price":"0","reduceOnly":true,"side":"SELL","positionSide":"BOTH","stopPrice":"99000","symbol":"BTCUSDT","bookTime":1566818724710,"updateTime":1566818724722,"timeInForce":"GTC","workingType":"MARK_PRICE","priceProtect":false}]"#;

fn ready() -> Harness {
    ready_with(OPEN_ORDERS)
}

fn ready_with(open_orders: &str) -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        portfolio_margin: true,
        ..Options::default()
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
    h.respond("/papi/v1/um/positionRisk", 200, POSITION_RISK);
    h.respond("/papi/v1/balance", 200, BALANCE);
    h.respond("/papi/v1/um/openOrders", 200, open_orders);
    h.respond("/papi/v1/um/conditional/openOrders", 200, OPEN_CONDITIONAL);
    for path in ["/public/ws", "/market/ws", "/pm/ws"] {
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
    }
    h
}

fn param(url: &str, name: &str) -> Option<String> {
    url.split_once('?')?
        .1
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_owned())
}

fn limit(amount: f64, price: f64) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount,
        price: Some(price),
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
    }
}

#[test]
fn startup_reads_the_account_from_papi() {
    let h = ready();
    assert!(
        h.url_of(h.socket("/pm/ws").unwrap())
            .ends_with("/pm/ws/pmkey")
    );
    let store = h.engine.store();
    assert!(store.loaded().balance && store.loaded().orders && store.loaded().positions);
    let position = &store.positions()[0];
    assert_eq!((position.contracts, position.leverage), (0.01, 20.0));
    assert_eq!(position.margin_type, Some(MarginType::Cross));
    assert_eq!(position.margin_size, Some(48.0));
    assert_eq!(store.setting("BTCUSDT").unwrap().leverage, 20.0);
    let balance = store.balance();
    assert_eq!(
        (balance.total, balance.free, balance.used, balance.upnl),
        (1000.0, 900.0, 100.0, 10.0)
    );
    let orders = store.orders();
    assert_eq!(
        orders[0].id, "x-s3vFFRQpresting",
        "orders are keyed by client id"
    );
    assert_eq!(orders[1].id, "abc");
    assert_eq!(orders[1].kind, OrderType::TakeProfit);
    assert_eq!(orders[1].price, 99000.0);
    assert!(
        orders[1].trigger_type.is_some(),
        "the trigger survives (TS dropped it)"
    );
}

#[test]
fn stream_events_track_orders_by_client_id() {
    let mut h = ready();
    h.message(
        "/pm/ws",
        r#"{"fs":"UM","e":"ORDER_TRADE_UPDATE","T":1,"E":1,"o":{"s":"BTCUSDT","c":"mine","S":"BUY","o":"LIMIT","f":"GTC","q":"0.002","p":"91000","ap":"0","sp":"0","x":"NEW","X":"NEW","i":8886774,"l":"0","z":"0","L":"0","T":1,"t":0,"b":"0","a":"0","m":false,"R":false,"ps":"BOTH","rp":"0"}}"#,
    );
    assert_eq!(h.engine.store().order("mine").unwrap().price, 91000.0);
    h.message(
        "/pm/ws",
        r#"{"e":"CONDITIONAL_ORDER_TRADE_UPDATE","T":1,"E":1,"fs":"UM","so":{"s":"BTCUSDT","c":"abc","si":123445,"S":"SELL","st":"TAKE_PROFIT_MARKET","f":"GTC","q":"0.010","p":"0","sp":"99000","os":"CANCELED","T":1,"ut":1,"R":true,"wt":"MARK_PRICE","ps":"BOTH","cp":false,"AP":"0","cr":"0","i":0,"V":"NONE","gtd":0}}"#,
    );
    assert_eq!(
        h.engine.store().order("abc").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn orders_go_one_at_a_time_and_report_their_client_ids() {
    let mut h = ready();
    h.event(Event::Command(Command::PlaceOrders {
        command: 1,
        orders: vec![limit(0.002, 90000.0), limit(0.003, 89000.0)],
    }));
    let first = h.take("/papi/v1/um/order");
    assert_eq!(first.method, Method::Post);
    assert!(
        first
            .url
            .starts_with("https://papi.binance.com/papi/v1/um/order?")
    );
    let id = param(&first.url, "newClientOrderId").unwrap();
    assert!(
        !h.has_request("/papi/v1/um/order"),
        "no batch endpoint: one at a time"
    );
    h.reply(
        first.id,
        200,
        "",
        r#"{"clientOrderId":"x","orderId":1,"status":"NEW"}"#,
    );
    let second = h.take("/papi/v1/um/order");
    let second_id = param(&second.url, "newClientOrderId").unwrap();
    h.reply(second.id, 200, "", r#"{"orderId":2,"status":"NEW"}"#);
    assert_eq!(h.result(1), Some(&Ok(format!(r#"["{id}","{second_id}"]"#))));
}

#[test]
fn a_full_position_stop_is_a_conditional_order_for_the_position_size() {
    let mut h = ready();
    let mut stop = limit(0.0, 85000.0);
    stop.kind = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.close = Some(true);
    h.event(Event::Command(Command::PlaceOrders {
        command: 2,
        orders: vec![stop],
    }));
    let request = h.take("/papi/v1/um/conditional/order");
    assert_eq!(
        param(&request.url, "strategyType").as_deref(),
        Some("STOP_MARKET")
    );
    assert_eq!(param(&request.url, "stopPrice").as_deref(), Some("85000"));
    assert_eq!(param(&request.url, "quantity").as_deref(), Some("0.01"));
    assert_eq!(param(&request.url, "reduceOnly").as_deref(), Some("true"));
    assert_eq!(param(&request.url, "closePosition"), None);
    assert_eq!(param(&request.url, "algoType"), None);
    let id = param(&request.url, "newClientStrategyId").expect("cancellable by its strategy id");
    h.reply(
        request.id,
        200,
        "",
        r#"{"strategyId":1,"strategyStatus":"NEW"}"#,
    );
    assert_eq!(h.result(2), Some(&Ok(format!(r#"["{id}"]"#))));
}

#[test]
fn close_flag_with_explicit_amount_preserves_pm_stop_size() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        for hedged in [false, true] {
            for has_position in [false, true] {
                let mut h = ready();
                h.engine.store_mut().set_orders(vec![]);
                h.engine.store_mut().set_hedged(hedged);
                if !has_position {
                    h.engine.store_mut().set_positions(vec![]);
                }
                let mut stop = limit(0.004, 85000.0);
                stop.kind = kind;
                stop.side = OrderSide::Sell;
                stop.close = Some(true);
                stop.reduce_only = Some(true);
                h.event(Event::Command(Command::PlaceOrders {
                    command: 5,
                    orders: vec![stop],
                }));
                let request = h.take("/papi/v1/um/conditional/order");
                assert_eq!(request.method, Method::Post);
                assert_eq!(
                    param(&request.url, "quantity").as_deref(),
                    Some("0.004"),
                    "{kind:?}, hedged={hedged}, position={has_position}"
                );
                assert_eq!(param(&request.url, "closePosition"), None);
                assert_eq!(
                    param(&request.url, "positionSide").as_deref(),
                    Some(if hedged { "LONG" } else { "BOTH" })
                );
                assert_eq!(
                    param(&request.url, "reduceOnly").as_deref(),
                    (!hedged).then_some("true")
                );
            }
        }
    }
}

#[test]
fn explicit_pm_stop_size_does_not_cancel_an_existing_full_stop() {
    let mut h = ready();
    let mut existing = h.engine.store().orders()[1].clone();
    existing.kind = OrderType::StopLoss;
    existing.amount = 0.0;
    existing.remaining = 0.0;
    existing.close = Some(true);
    h.engine.store_mut().set_orders(vec![existing.clone()]);
    let mut stop = limit(0.004, 85000.0);
    stop.kind = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.close = Some(true);
    h.event(Event::Command(Command::PlaceOrders {
        command: 6,
        orders: vec![stop],
    }));
    assert_eq!(h.take("/papi/v1/um/conditional/order").method, Method::Post);
    assert_eq!(h.engine.store().order(&existing.id), Some(&existing));
}

#[test]
fn cancels_by_client_id_and_cancel_all_covers_conditional_orders() {
    let mut h = ready();
    h.event(Event::Command(Command::CancelOrders {
        command: 3,
        ids: vec!["x-s3vFFRQpresting".into(), "abc".into()],
    }));
    let regular = h.take("/papi/v1/um/order");
    assert_eq!(regular.method, Method::Delete);
    assert_eq!(
        param(&regular.url, "origClientOrderId").as_deref(),
        Some("x-s3vFFRQpresting")
    );
    h.reply(regular.id, 200, "", r#"{"symbol":"BTCUSDT","orderId":1917641,"clientOrderId":"x-s3vFFRQpresting","origQty":"0.40","executedQty":"0","status":"CANCELED"}"#);
    let conditional = h.take("/papi/v1/um/conditional/order");
    assert_eq!(
        param(&conditional.url, "newClientStrategyId").as_deref(),
        Some("abc")
    );
    h.reply(conditional.id, 200, "", r#"{"symbol":"BTCUSDT","strategyId":123445,"newClientStrategyId":"abc","strategyStatus":"CANCELED"}"#);
    assert_eq!(
        h.engine.store().order("abc").unwrap().status,
        OrderStatus::Canceled
    );

    h.event(Event::Command(Command::CancelSymbolOrders {
        command: 4,
        symbol: "BTCUSDT".into(),
    }));
    assert!(h.has_request("/papi/v1/um/allOpenOrders"));
    assert!(
        h.has_request("/papi/v1/um/conditional/allOpenOrders"),
        "TS skipped this one"
    );
}

#[test]
fn margin_type_is_refused_on_a_cross_only_account() {
    let mut h = ready();
    h.event(Event::Command(Command::SetMarginType {
        command: 5,
        symbol: "BTCUSDT".into(),
        isolated: true,
    }));
    assert!(h.requests.is_empty());
    assert!(matches!(h.result(5), Some(Err(message)) if message.contains("cross margin only")));
}

#[test]
fn an_invalid_open_order_row_is_rejected_by_identity_while_peers_commit() {
    let overfilled = OPEN_ORDERS.replace(r#""executedQty":"0""#, r#""executedQty":"0.50""#);
    let h = ready_with(&overfilled);
    assert!(
        h.logs
            .iter()
            .any(|log| log == "Binance open order x-s3vFFRQpresting has invalid economics"),
        "{:?}",
        h.logs
    );
    assert!(h.engine.store().order("x-s3vFFRQpresting").is_none());
    assert!(h.engine.store().order("abc").is_some());
}

#[test]
fn pm_strategy_receipt_uses_final_renamed_identity_before_write() {
    let mut h = ready();
    let mut stop = limit(0.0, 85000.0);
    stop.kind = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.close = Some(true);
    h.event(Event::Command(Command::PlaceOrders {
        command: 92,
        orders: vec![stop],
    }));
    let request = h.take("/papi/v1/um/conditional/order");
    let id = param(&request.url, "newClientStrategyId").unwrap();
    assert_eq!(
        h.progress,
        vec![(
            92,
            vec![safe_cex_engine::OrderIdentityReceipt {
                symbol: "BTCUSDT".into(),
                kind: safe_cex_engine::OrderIdentityKind::ClientOrderId,
                identity: id
            }]
        )]
    );
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
fn unproven_pm_ids_require_the_native_client_identity_echo() {
    let mut observations = Vec::new();
    for algo in [false, true] {
        for echo in [0, 1, 2] {
            let mut h = ready();
            let mut input = limit(1., 100.);
            input.client_order_id = Some("caller_1".into());
            if algo {
                input.kind = OrderType::StopLoss;
                input.client_order_id = None;
            }
            let before = h.engine.store().orders().to_vec();
            h.event(Event::Command(Command::PlaceOrders {
                command: 722,
                orders: vec![input],
            }));
            let request = h.take(if algo {
                "/papi/v1/um/conditional/order"
            } else {
                "/papi/v1/um/order"
            });
            let expected = param(
                &request.url,
                if algo {
                    "newClientStrategyId"
                } else {
                    "newClientOrderId"
                },
            )
            .unwrap();
            let (native, field) = if algo {
                ("strategyId", "newClientStrategyId")
            } else {
                ("orderId", "clientOrderId")
            };
            let body = match echo {
                0 => format!(r#"{{"{native}":12}}"#),
                1 => format!(r#"{{"{native}":12,"{field}":"other"}}"#),
                _ => format!(r#"{{"{native}":12,"{field}":"{expected}"}}"#),
            };
            h.reply(request.id, 503, "", &body);
            let failure = &h.failures.iter().find(|(id, _)| *id == 722).unwrap().1;
            assert_eq!(failure.outcome, Some(safe_cex_engine::Outcome::Unknown));
            let ids = failure
                .partial
                .as_ref()
                .map(|p| p.order_ids.clone())
                .unwrap_or_default();
            assert_eq!(h.engine.store().orders(), before);
            observations.push((
                algo,
                echo,
                ids,
                if echo == 2 { vec![expected] } else { vec![] },
            ));
        }
    }
    for (algo, echo, ids, expected) in observations {
        assert_eq!(ids, expected, "algo={algo} echo={echo}");
    }
}
