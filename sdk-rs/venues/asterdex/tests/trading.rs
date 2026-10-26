//! Native request contract from the official Aster V3 New Order, Cancel Order,
//! Change Initial Leverage and STP examples. Synthetic credentials only.
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
mod support;
use safe_cex_venue_asterdex::AsterDex;
use serde_json::json;
use support::Options;

struct Harness {
    engine: Engine<AsterDex>,
    requests: Vec<HttpRequest>,
    done: Vec<(u32, Result<String, String>)>,
    failures: Vec<(u32, safe_cex_engine::VenueError)>,
}
impl Harness {
    fn new() -> Self {
        Self::options(Options {
            key: Some("synthetic-key".into()),
            secret: Some("synthetic-secret".into()),
            ..Options::default()
        })
    }
    fn options(options: Options) -> Self {
        let mut engine = support::engine(options);
        engine.store_mut().set_markets(vec![Market {
            id: "BTCUSDT".into(),
            symbol: "BTCUSDT".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                price: 0.1,
                amount: 0.001,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.001,
                    max: 2.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 125.0,
                },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
        Self {
            engine,
            requests: Vec::new(),
            done: Vec::new(),
            failures: Vec::new(),
        }
    }
    fn collect(&mut self) {
        for action in support::drain(&mut self.engine, 1700000000000.0).into_iter() {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.done.push((command, Ok(result)))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.done.push((command, Err(error.legacy_text())));
                    self.failures.push((command, error));
                }
                _ => {}
            }
        }
        if let Some(index) = self.requests.iter().position(|r| r.url.ends_with("/time")) {
            let request = self.requests.remove(index);
            self.engine.handle(
                1700000000000.0,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: r#"{"serverTime":1700000000000}"#,
                    },
                },
            );
            self.collect();
        }
    }
    fn command(&mut self, command: Command) {
        self.engine.handle(1700000000000.0, Event::Command(command));
        self.collect();
    }
    fn take(&mut self, method: Method) -> HttpRequest {
        let index = self
            .requests
            .iter()
            .position(|r| r.method == method && r.url.contains("/order?"))
            .unwrap_or_else(|| panic!("missingorder: {:?}", self.requests));
        self.requests.remove(index)
    }
    fn reply(&mut self, request: HttpRequest, body: &str) {
        self.engine.handle(
            1700000000000.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
    fn place(&mut self, order: serde_json::Value) {
        self.command(Command::PlaceOrders {
            command: 1,
            orders: vec![serde_json::from_value(order).unwrap()],
        });
    }
    fn result(&self, id: u32) -> &Result<String, String> {
        &self
            .done
            .iter()
            .find(|(command, _)| *command == id)
            .unwrap()
            .1
    }
}
fn limit() -> serde_json::Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1.2344,"price":9300.04})
}
fn order(id: &str, kind: OrderType) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind,
        side: OrderSide::Sell,
        price: 9300.0,
        amount: 1.0,
        filled: 0.0,
        remaining: 1.0,
        reduce_only: true,
        close: None,
        client_order_id: None,
        time_in_force: Some(TimeInForce::PostOnly),
        last_update: 0.0,
        trigger_type: Some(StopTriggerType::LastPrice),
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

#[test]
fn documented_regular_payload_and_nested_or_large_ack_ids() {
    let mut h = Harness::new();
    h.place(limit());
    let request = h.take(Method::Post);
    assert!(request.url.contains("/fapi/v1/order?symbol=BTCUSDT&side=BUY&type=LIMIT&quantity=1.234&timeInForce=GTC&price=9300"),"{}",request.url);
    assert!(!request.url.contains("positionSide"));
    assert!(!request.url.contains("builder="));
    assert!(request.body.is_none());
    h.reply(request, r#"{"data":{"orderId":"9007199254740993"}}"#);
    assert_eq!(h.result(1).as_ref().unwrap(), "[\"9007199254740993\"]");
}
#[test]
fn bracket_splits_regular_first_then_sequential_conditional_children() {
    let mut h = Harness::new();
    let mut opts = limit();
    opts["amount"] = json!(5.001);
    opts["stopLoss"] = json!(9000.09);
    opts["takeProfit"] = json!(9500.09);
    h.place(opts);
    for (index, amount) in ["2", "2", "1.001"].into_iter().enumerate() {
        let request = h.take(Method::Post);
        assert!(request.url.contains(&format!("quantity={amount}&")));
        assert!(request.url.contains("type=LIMIT"));
        h.reply(request, &format!(r#"{{"orderId":{}}}"#, index + 1));
    }
    let sl = h.take(Method::Post);
    assert!(sl.url.contains("side=SELL&type=STOP_MARKET&quantity=5.001&reduceOnly=true&stopPrice=9000.1&workingType=MARK_PRICE"),"{}",sl.url);
    h.reply(sl, r#"{"orderId":4}"#);
    let tp = h.take(Method::Post);
    assert!(tp.url.contains("type=TAKE_PROFIT_MARKET&quantity=5.001&reduceOnly=true&stopPrice=9500.1&workingType=CONTRACT_PRICE"),"{}",tp.url);
    h.reply(tp, r#"{"orderId":5}"#);
    assert_eq!(
        h.result(1).as_ref().unwrap(),
        "[\"1\",\"2\",\"3\",\"4\",\"5\"]"
    );
}
#[test]
fn full_close_conditional_has_no_quantity_reduce_only_or_tif() {
    let mut h = Harness::new();
    h.engine.store_mut().set_hedged(true);
    h.place(json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"triggerPrice":9300.04,"reduceOnly":true}));
    let request = h.take(Method::Post);
    assert!(
        request
            .url
            .contains("positionSide=LONG&closePosition=true&stopPrice=9300&workingType=MARK_PRICE")
    );
    for forbidden in ["quantity=", "reduceOnly=", "timeInForce="] {
        assert!(!request.url.contains(forbidden));
    }
}
#[test]
fn caller_id_anywhere_rejects_entire_batch_before_write() {
    let mut h = Harness::new();
    let mut second = limit();
    second["clientOrderId"] = json!("caller");
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![
            serde_json::from_value(limit()).unwrap(),
            serde_json::from_value(second).unwrap(),
        ],
    });
    assert!(h.requests.is_empty());
    assert!(h.result(1).is_err());
}
#[test]
fn bybit_execution_controls_reject_entire_batch_before_clock_or_order_write() {
    for version in [
        safe_cex_venue_asterdex::sign::Version::V1,
        safe_cex_venue_asterdex::sign::Version::V3,
    ] {
        for (field, value) in [
            ("bybitBbo", json!({"sideType":"Queue","level":1})),
            ("bybitSlippageToleranceTicks", json!(1)),
            ("bybitSlippageToleranceTicks", json!(0)),
        ] {
            // V3 setup accepts only a hex wallet key.
            let secret = match version {
                safe_cex_venue_asterdex::sign::Version::V1 => "synthetic-secret".into(),
                safe_cex_venue_asterdex::sign::Version::V3 => "01".repeat(32),
            };
            let mut h = Harness::options(Options {
                api_version: Some(version),
                key: Some("synthetic-key".into()),
                secret: Some(secret),
                ..Options::default()
            });
            let mut second = limit();
            second[field] = value;
            h.command(Command::PlaceOrders {
                command: 1,
                orders: vec![
                    serde_json::from_value(limit()).unwrap(),
                    serde_json::from_value(second).unwrap(),
                ],
            });
            assert!(h.requests.is_empty(), "{version:?} {field}");
            assert!(
                h.result(1)
                    .as_ref()
                    .unwrap_err()
                    .contains("not supported by AsterDEX"),
                "{version:?} {field}"
            );
        }
    }
}

#[test]
fn trailing_and_v1_stp_fail_before_write() {
    for value in [
        json!({"symbol":"BTCUSDT","type":"trailing_stop_market","side":"sell","amount":1,"price":9000}),
        json!({"symbol":"BTCUSDT","type":"market","side":"buy","amount":1,"stpMode":"EXPIRE_TAKER"}),
    ] {
        let mut h = Harness::new();
        h.place(value);
        assert!(h.requests.is_empty());
        assert!(h.result(1).is_err());
    }
    let mut h = Harness::new();
    h.command(Command::FetchStpMode { command: 2 });
    assert!(h.requests.is_empty());
    assert!(h.result(2).is_err());
}
#[test]
fn individual_batch_failure_continues_and_returns_successful_ids() {
    let mut h = Harness::new();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![
            serde_json::from_value(limit()).unwrap(),
            serde_json::from_value(limit()).unwrap(),
        ],
    });
    let first = h.take(Method::Post);
    h.reply(first, r#"{"code":-2010,"msg":"insufficient margin"}"#);
    let second = h.take(Method::Post);
    h.reply(second, r#"{"orderId":22542179}"#);
    assert_eq!(h.result(1).as_ref().unwrap(), "[\"22542179\"]");
}
#[test]
fn cancel_unknown_is_idempotent_and_does_not_replace_filled_status() {
    let mut h = Harness::new();
    h.engine
        .store_mut()
        .set_orders(vec![order("283194212", OrderType::Limit)]);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["283194212".into()],
    });
    let cancel = h.take(Method::Delete);
    assert!(cancel.url.contains("symbol=BTCUSDT&orderId=283194212"));
    h.engine
        .store_mut()
        .set_order_status("283194212", OrderStatus::Closed);
    h.reply(cancel, r#"{"code":-2011,"msg":"Unknown order sent."}"#);
    assert!(h.result(1).is_ok());
    assert_eq!(
        h.engine.store().order("283194212").unwrap().status,
        OrderStatus::Closed
    );
}
#[test]
fn amend_uses_cancel_then_post_preserving_trigger_basis_and_tif() {
    let mut h = Harness::new();
    h.engine
        .store_mut()
        .set_orders(vec![order("283194212", OrderType::StopLoss)]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "283194212".into(),
        price: Some(9200.04),
        amount: Some(0.3),
    });
    let cancel = h.take(Method::Delete);
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    h.reply(cancel, r#"{"orderId":283194212,"status":"CANCELED"}"#);
    let place = h.take(Method::Post);
    assert!(place.url.contains(
        "type=STOP_MARKET&quantity=0.3&reduceOnly=true&stopPrice=9200&workingType=CONTRACT_PRICE"
    ));
    assert!(!place.url.contains("timeInForce"));
    h.reply(place, r#"{"orderId":283194213}"#);
    assert!(h.result(1).is_ok());
}
#[test]
fn invalid_amend_does_not_cancel_live_order_and_failed_cancel_never_replaces() {
    let mut h = Harness::new();
    h.engine
        .store_mut()
        .set_orders(vec![order("10", OrderType::Limit)]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "10".into(),
        price: Some(-1.0),
        amount: None,
    });
    assert!(h.requests.is_empty());
    assert!(h.result(1).is_err());
    h.command(Command::AmendOrder {
        command: 2,
        id: "10".into(),
        price: Some(9200.0),
        amount: None,
    });
    let cancel = h.take(Method::Delete);
    h.reply(cancel, r#"{"code":-2015,"msg":"Invalid API-key"}"#);
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    assert!(h.result(2).is_err());
}
#[test]
fn native_settings_requests_round_and_clamp_leverage() {
    let mut h = Harness::new();
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage: 125.6,
        side: None,
    });
    let request = h.requests.remove(0);
    assert!(
        request
            .url
            .contains("/leverage?symbol=BTCUSDT&leverage=125&")
    );
    h.reply(
        request,
        r#"{"leverage":125,"maxNotionalValue":"1000000","symbol":"BTCUSDT"}"#,
    );
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 125.0);
    h.command(Command::SetMarginType {
        command: 2,
        symbol: "BTCUSDT".into(),
        isolated: true,
    });
    let request = h.requests.remove(0);
    assert!(request.url.contains("marginType=ISOLATED"));
    h.reply(request, r#"{"code":200,"msg":"success"}"#);
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
    h.command(Command::SetPositionMode {
        command: 3,
        hedged: true,
        symbol: None,
    });
    let request = h.requests.remove(0);
    assert!(request.url.contains("dualSidePosition=true"));
    h.reply(request, r#"{"code":200,"msg":"success"}"#);
    assert!(h.engine.store().hedged());
}

fn v3_options() -> Options {
    Options {
        key: Some("0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf".into()),
        secret: Some("0000000000000000000000000000000000000000000000000000000000000001".into()),
        application_id: Some("0x1111111111111111111111111111111111111111".into()),
        builder_fee_rate_maker: Some("0.0002000".into()),
        builder_fee_rate_taker: Some("0.0003".into()),
        ..Options::default()
    }
}

#[test]
fn v3_builder_maker_taker_brackets_and_per_order_stp_are_exact_decimals() {
    let mut h = Harness::options(v3_options());
    let mut opts = limit();
    opts["timeInForce"] = json!("PostOnly");
    opts["stpMode"] = json!("EXPIRE_BOTH");
    opts["stopLoss"] = json!(9000);
    h.place(opts);
    let entry = h.take(Method::Post);
    assert!(entry.url.contains("/fapi/v3/order?symbol=BTCUSDT&side=BUY&type=LIMIT&stpMode=EXPIRE_BOTH&builder=0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca&feeRate=0.0002&quantity=1.234&timeInForce=GTX&price=9300"), "{}", entry.url);
    h.reply(entry, r#"{"orderId":1}"#);
    let bracket = h.take(Method::Post);
    assert!(bracket.url.contains("type=STOP_MARKET&builder=0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca&feeRate=0.0003&quantity=1.234&reduceOnly=true"), "{}", bracket.url);
    assert!(!bracket.url.contains("stpMode="));
}

#[test]
fn builder_zero_rate_omits_attribution_and_testnet_placeholder_disables_it() {
    let mut options = v3_options();
    options.builder_fee_rate_maker = Some("0".into());
    let mut h = Harness::options(options);
    let mut opts = limit();
    opts["timeInForce"] = json!("PostOnly");
    h.place(opts);
    let request = h.take(Method::Post);
    assert!(!request.url.contains("builder="));
    assert!(!request.url.contains("feeRate="));
    let mut options = v3_options();
    options.testnet = true;
    let mut h = Harness::options(options);
    h.place(limit());
    assert!(!h.take(Method::Post).url.contains("builder="));
}

#[test]
fn invalid_v3_builder_config_fails_before_any_order() {
    for rate in [
        "0.001000000000000001",
        "1e-4",
        "-0.1",
        "0.0000000000000000001",
    ] {
        let mut options = v3_options();
        options.builder_fee_rate_taker = Some(rate.into());
        let mut h = Harness::options(options);
        h.place(limit());
        assert!(h.requests.is_empty(), "{rate}");
        assert!(h.result(1).is_err());
    }
}

#[test]
fn documented_stp_account_read_and_write_use_v3_endpoint() {
    let mut h = Harness::options(v3_options());
    h.command(Command::FetchStpMode { command: 1 });
    let read = h.requests.remove(0);
    assert_eq!(read.method, Method::Get);
    assert!(read.url.contains("/fapi/v3/stpMode?"));
    h.reply(read, r#"{"stpMode":"EXPIRE_TAKER"}"#);
    assert_eq!(h.result(1).as_ref().unwrap(), "\"EXPIRE_TAKER\"");
    h.command(Command::SetStpMode {
        command: 2,
        mode: StpMode::ExpireBoth,
    });
    let write = h.requests.remove(0);
    assert_eq!(write.method, Method::Post);
    assert!(write.url.contains("/fapi/v3/stpMode?stpMode=EXPIRE_BOTH&"));
    h.reply(write, r#"{"code":200,"msg":"success"}"#);
    assert!(h.result(2).is_ok());
}

#[test]
fn malformed_ack_is_not_success_and_ambiguous_write_is_never_retried() {
    let mut h = Harness::new();
    h.place(limit());
    let request = h.take(Method::Post);
    h.reply(request, r#"{"code":200,"msg":"success"}"#);
    assert!(h.result(1).is_err());
    assert!(
        !h.requests
            .iter()
            .any(|request| request.method == Method::Post)
    );
    let mut h = Harness::new();
    h.place(limit());
    let request = h.take(Method::Post);
    h.engine.handle(
        1700000000000.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 503,
                headers: "",
                body: r#"{"msg":"Unknown error, please check your request or try again later."}"#,
            },
        },
    );
    h.collect();
    assert!(h.result(1).is_err());
    assert!(
        !h.requests
            .iter()
            .any(|request| request.method == Method::Post)
    );
}

#[test]
fn per_item_validation_failure_preserves_other_valid_batch_orders() {
    let mut h = Harness::new();
    let mut invalid = limit();
    invalid["symbol"] = json!("UNKNOWN");
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![
            serde_json::from_value(invalid).unwrap(),
            serde_json::from_value(limit()).unwrap(),
        ],
    });
    let valid = h.take(Method::Post);
    h.reply(valid, r#"{"orderId":22542179}"#);
    assert_eq!(h.result(1).as_ref().unwrap(), "[\"22542179\"]");
}

#[test]
fn documented_algo_capacity_rejects_new_stop_but_allows_cancel_replace_at_limit() {
    let mut h = Harness::new();
    h.engine.start(1700000000000.0);
    h.collect();
    let index = h
        .requests
        .iter()
        .position(|r| r.url.ends_with("/exchangeInfo"))
        .unwrap();
    let info = h.requests.remove(index);
    h.reply(info,r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"2"},{"filterType":"MAX_NUM_ALGO_ORDERS","limit":1}]}]}"#);
    // Private startup begins at markets; settle the position-mode read that gates amends.
    let index = h
        .requests
        .iter()
        .position(|r| r.url.contains("/positionSide/dual"))
        .unwrap();
    let mode = h.requests.remove(index);
    h.reply(mode, r#"{"dualSidePosition":false}"#);
    h.engine
        .store_mut()
        .set_orders(vec![order("283194212", OrderType::StopLoss)]);
    h.place(json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":1,"triggerPrice":9200}));
    assert!(
        !h.requests
            .iter()
            .any(|r| r.method == Method::Post && r.url.contains("/order?"))
    );
    assert!(h.result(1).is_err());
    h.command(Command::AmendOrder {
        command: 2,
        id: "283194212".into(),
        price: Some(9200.0),
        amount: None,
    });
    let cancel = h.take(Method::Delete);
    h.reply(cancel, r#"{"orderId":283194212,"status":"CANCELED"}"#);
    let replacement = h.take(Method::Post);
    assert!(replacement.url.contains("stopPrice=9200"));
    h.reply(replacement, r#"{"orderId":283194213}"#);
    assert!(h.result(2).is_ok());
}

#[test]
fn missing_credentials_fail_once_before_split_requests_or_clock() {
    let mut h = Harness::options(Options::default());
    let mut opts = limit();
    opts["amount"] = json!(100);
    h.place(opts);
    assert!(h.requests.is_empty());
    assert!(h.result(1).is_err());
    assert_eq!(h.done.len(), 1);
}

#[test]
fn amend_never_replaces_an_order_that_fills_during_cancel() {
    for status in [
        OrderStatus::Closed,
        OrderStatus::Open,
        OrderStatus::Canceled,
    ] {
        let mut h = Harness::new();
        h.engine
            .store_mut()
            .set_orders(vec![order("10", OrderType::Limit)]);
        h.command(Command::AmendOrder {
            command: 1,
            id: "10".into(),
            price: Some(9200.0),
            amount: None,
        });
        let cancel = h.take(Method::Delete);
        h.engine.store_mut().update_order("10", |order| {
            order.status = status;
            order.filled = 0.5;
            order.remaining = 0.5;
        });
        h.reply(cancel, r#"{"orderId":10,"status":"CANCELED"}"#);
        assert!(!h.requests.iter().any(|r| r.method == Method::Post));
        assert!(h.result(1).is_err());
    }
    let mut h = Harness::new();
    let mut closed = order("10", OrderType::Limit);
    closed.status = OrderStatus::Closed;
    h.engine.store_mut().set_orders(vec![closed]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "10".into(),
        price: Some(9200.0),
        amount: None,
    });
    assert!(h.requests.is_empty());
    assert!(h.result(1).is_err());
}

#[test]
fn cancel_rejected_with_other_2011_message_cannot_trigger_amend_replacement() {
    let mut h = Harness::new();
    h.engine
        .store_mut()
        .set_orders(vec![order("10", OrderType::Limit)]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "10".into(),
        price: Some(9200.0),
        amount: None,
    });
    let cancel = h.take(Method::Delete);
    h.reply(cancel, r#"{"code":-2011,"msg":"CANCEL_REJECTED"}"#);
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    assert!(h.result(1).is_err());
    assert_eq!(
        h.engine.store().order("10").unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn native_cancel_ack_cumulative_fill_prevents_replacement_before_ws_delivery() {
    for status in ["CANCELED", "FILLED"] {
        let mut h = Harness::new();
        h.engine
            .store_mut()
            .set_orders(vec![order("10", OrderType::Limit)]);
        h.command(Command::AmendOrder {
            command: 1,
            id: "10".into(),
            price: Some(9200.0),
            amount: None,
        });
        let cancel = h.take(Method::Delete);
        h.reply(
            cancel,
            &format!(r#"{{"orderId":10,"status":"{status}","executedQty":"0.5"}}"#),
        );
        assert!(!h.requests.iter().any(|r| r.method == Method::Post));
        assert!(h.result(1).is_err());
        assert_eq!(h.engine.store().order("10").unwrap().filled, 0.5);
    }
}

#[test]
fn settings_ack_invalidates_older_position_risk_settings_snapshot() {
    for margin in [false, true] {
        let mut h = Harness::new();
        h.engine.start(1700000000000.0);
        h.collect();
        for (path, body) in [
            (
                "/exchangeInfo",
                r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"2"}]}]}"#,
            ),
            ("/leverageBracket", "[]"),
            (
                "/ticker/24hr",
                r#"[{"symbol":"BTCUSDT","lastPrice":"9300","priceChangePercent":"1","volume":"2","quoteVolume":"18600"}]"#,
            ),
            (
                "/openInterest",
                r#"[{"symbol":"BTCUSDT","openInterest":"2"}]"#,
            ),
        ] {
            let index = h
                .requests
                .iter()
                .position(|r| r.url.split('?').next().unwrap().ends_with(path))
                .unwrap_or_else(|| panic!("missing {path}: {:?}", h.requests));
            let request = h.requests.remove(index);
            h.reply(request, body);
        }
        let index = h
            .requests
            .iter()
            .position(|r| r.url.contains("/positionRisk?"))
            .unwrap();
        let stale = h.requests.remove(index);
        let (command, endpoint) = if margin {
            (
                Command::SetMarginType {
                    command: 1,
                    symbol: "BTCUSDT".into(),
                    isolated: true,
                },
                "/marginType?",
            )
        } else {
            (
                Command::SetLeverage {
                    command: 1,
                    symbol: "BTCUSDT".into(),
                    leverage: 8.0,
                    side: None,
                },
                "/leverage?",
            )
        };
        h.command(command);
        let index = h
            .requests
            .iter()
            .position(|r| r.url.contains(endpoint))
            .unwrap();
        let write = h.requests.remove(index);
        h.reply(write, r#"{"code":200,"msg":"success"}"#);
        h.reply(stale,r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0","entryPrice":"0","unRealizedProfit":"0","notional":"0","leverage":"3","liquidationPrice":"0","marginType":"cross"}]"#);
        let setting = h.engine.store().setting("BTCUSDT").unwrap();
        if margin {
            assert_eq!(setting.margin_type, Some(MarginType::Isolated));
        } else {
            assert_eq!(setting.leverage, 8.0);
        }
    }
}

#[test]
fn cancel_with_caller_symbol_works_before_reconciliation_and_for_external_orders() {
    let mut h = Harness::new();
    h.place(limit());
    let placed = h.take(Method::Post);
    h.reply(placed, r#"{"orderId":22542179}"#);
    assert!(h.result(1).is_ok());
    // Deliberately leave the openOrders refresh and the private NEW undelivered.
    assert!(h.engine.store().order("22542179").is_none());
    for (command, id) in [(2, "22542179"), (3, "22542180")] {
        h.command(Command::CancelOrdersWithSymbols {
            command,
            orders: vec![CancelOrderRef {
                id: id.into(),
                symbol: "BTCUSDT".into(),
            }],
        });
        let cancel = h.take(Method::Delete);
        assert!(
            cancel
                .url
                .contains(&format!("symbol=BTCUSDT&orderId={id}&"))
        );
        h.reply(cancel, r#"{"status":"CANCELED","executedQty":"0"}"#);
        assert!(h.result(command).is_ok());
        assert!(h.engine.store().order(id).is_none());
    }
}

#[test]
fn id_only_cancellation_cannot_claim_success_without_routing_context() {
    let mut h = Harness::new();
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["unseen".into()],
    });
    assert!(h.requests.is_empty());
    assert!(
        h.result(1)
            .as_ref()
            .unwrap_err()
            .contains("requires its symbol")
    );
}

#[test]
fn symbol_cancel_includes_acknowledged_order_before_rest_or_private_new() {
    let mut h = Harness::new();
    h.place(limit());
    let placed = h.take(Method::Post);
    h.reply(placed, r#"{"orderId":22542179}"#);
    assert!(h.engine.store().order("22542179").is_none());
    h.command(Command::CancelSymbolOrders {
        command: 2,
        symbol: "BTCUSDT".into(),
    });
    let cancel = h.take(Method::Delete);
    assert!(cancel.url.contains("symbol=BTCUSDT&orderId=22542179&"));
    h.reply(cancel, r#"{"orderId":22542179,"status":"CANCELED"}"#);
    assert!(h.result(2).is_ok());
    h.command(Command::CancelSymbolOrders {
        command: 3,
        symbol: "BTCUSDT".into(),
    });
    assert!(!h.requests.iter().any(|r| r.method == Method::Delete));
    assert!(h.result(3).is_ok());
}

#[test]
fn symbol_cancel_retires_ack_provenance_when_store_has_seen_the_order() {
    let mut h = Harness::new();
    h.place(limit());
    let placed = h.take(Method::Post);
    h.reply(placed, r#"{"orderId":22542179}"#);
    h.engine
        .store_mut()
        .set_orders(vec![order("22542179", OrderType::Limit)]);
    h.command(Command::CancelSymbolOrders {
        command: 2,
        symbol: "BTCUSDT".into(),
    });
    let cancel = h.take(Method::Delete);
    h.reply(cancel, r#"{"orderId":22542179,"status":"CANCELED"}"#);
    assert!(!h.requests.iter().any(|r| r.method == Method::Delete));
    assert!(h.result(2).is_ok());
}

#[test]
fn unknown_order_is_ambiguous_for_amend_and_never_authorizes_replacement() {
    let mut h = Harness::new();
    h.engine
        .store_mut()
        .set_orders(vec![order("10", OrderType::Limit)]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "10".into(),
        price: Some(9200.0),
        amount: None,
    });
    let cancel = h.take(Method::Delete);
    // Venue may already have filled the order while the private fill is delayed.
    h.reply(cancel, r#"{"code":-2011,"msg":"Unknown order sent."}"#);
    assert!(h.result(1).is_err());
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    // Do not manufacture a cancellation from the ambiguous rejection.
    assert_eq!(
        h.engine.store().order("10").unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn leverage_query_returns_loaded_account_leverage_instead_of_market_maximum() {
    let mut h = Harness::new();
    h.engine
        .store_mut()
        .update_setting("BTCUSDT", |s| s.leverage = 10.0);
    assert_eq!(
        h.engine
            .store()
            .market("BTCUSDT")
            .unwrap()
            .limits
            .leverage
            .max,
        125.0
    );
    h.command(Command::FetchLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
    });
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(h.result(1).as_ref().unwrap()).unwrap(),
        json!({"buyLeverage":10,"sellLeverage":10})
    );
    assert!(h.requests.is_empty());
}

#[test]
fn id_only_cancel_uses_ack_routing_before_store_sees_the_placement() {
    let mut h = Harness::new();
    h.place(limit());
    let placed = h.take(Method::Post);
    h.reply(placed, r#"{"orderId":22542179}"#);
    assert!(h.engine.store().order("22542179").is_none());
    h.command(Command::CancelOrders {
        command: 2,
        ids: vec!["22542179".into()],
    });
    let cancel = h.take(Method::Delete);
    assert!(cancel.url.contains("symbol=BTCUSDT&orderId=22542179&"));
    h.reply(cancel, r#"{"orderId":22542179,"status":"CANCELED"}"#);
    assert!(h.result(2).is_ok());
}

#[test]
fn ack_routing_capacity_rejects_oversized_and_concurrent_jobs_before_new_writes() {
    let mut h = Harness::new();
    let value: PlaceOrderOpts = serde_json::from_value(limit()).unwrap();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![value.clone(); 10_001],
    });
    assert!(
        h.result(1)
            .as_ref()
            .unwrap_err()
            .contains("routing capacity")
    );
    assert!(h.requests.is_empty());
    // Reserve room for every sequential split before the first write. Another
    // command must not consume that capacity while its first request is pending.
    h.command(Command::PlaceOrders {
        command: 2,
        orders: vec![value.clone(); 10_000],
    });
    let first = h.take(Method::Post);
    h.command(Command::PlaceOrders {
        command: 3,
        orders: vec![value],
    });
    assert!(
        h.result(3)
            .as_ref()
            .unwrap_err()
            .contains("routing capacity")
    );
    assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    h.reply(first, r#"{"orderId":22542179}"#);
    // The existing job retains its reservation and can keep making progress.
    let _second = h.take(Method::Post);
    h.command(Command::CancelOrders {
        command: 4,
        ids: vec!["22542179".into()],
    });
    let cancel = h.take(Method::Delete);
    assert!(cancel.url.contains("orderId=22542179&"));
    h.reply(cancel, r#"{"orderId":22542179,"status":"CANCELED"}"#);
    h.command(Command::PlaceOrders {
        command: 5,
        orders: vec![serde_json::from_value(limit()).unwrap()],
    });
    let _new = h.take(Method::Post);
}

#[test]
fn unknown_id_does_not_prevent_other_cancellations_in_the_batch() {
    let mut h = Harness::new();
    h.engine
        .store_mut()
        .set_orders(vec![order("known", OrderType::Limit)]);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["unknown".into(), "known".into()],
    });
    let cancel = h.take(Method::Delete);
    assert!(cancel.url.contains("orderId=known&"));
    h.reply(cancel, r#"{"status":"CANCELED"}"#);
    assert!(h.result(1).is_ok());
    assert_eq!(
        h.engine.store().order("known").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn trading_failures_carry_the_aster_code_kind_and_outcome() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (status, body, kind, outcome) in [
        (
            400,
            r#"{"code":-2019,"msg":"Margin is insufficient."}"#,
            Some(ErrorKind::Rejected),
            Outcome::Rejected,
        ),
        (
            400,
            r#"{"code":-1111,"msg":"Precision is over the maximum defined for this asset."}"#,
            Some(ErrorKind::InvalidInput),
            Outcome::Rejected,
        ),
        (
            503,
            r#"{"code":-2019,"msg":"Margin is insufficient."}"#,
            // The shared classifier ranks an unproven gateway status above
            // the native code.
            Some(ErrorKind::VenueUnavailable),
            Outcome::Unknown,
        ),
        (
            503,
            r#"{"msg":"Unknown error, please check your request or try again later."}"#,
            Some(ErrorKind::VenueUnavailable),
            Outcome::Unknown,
        ),
        (
            200,
            r#"{"code":200,"msg":"success"}"#,
            // An acknowledgement without an order ID is an unproven reply.
            Some(ErrorKind::Internal),
            Outcome::Unknown,
        ),
    ] {
        let mut h = Harness::new();
        h.place(limit());
        let request = h.take(Method::Post);
        h.engine.handle(
            1700000000000.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        h.collect();
        let [(1, error)] = h.failures.as_slice() else {
            panic!("{body}: {:?}", h.failures);
        };
        assert_eq!(
            (error.kind, error.outcome),
            (kind, Some(outcome)),
            "{status} {body}"
        );
        assert_eq!(
            h.result(1),
            &Err(error.message.clone()),
            "text is unchanged"
        );
    }
}

#[test]
fn capability_profile_v1_does_not_advertise_stp() {
    let h = Harness::new();
    assert!(!h.engine.capabilities().stp_controls);
}

#[test]
fn capability_profile_v3_keeps_stp_advertised() {
    let h = Harness::options(v3_options());
    assert!(h.engine.capabilities().stp_controls);
}
