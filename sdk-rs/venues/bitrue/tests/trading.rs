//! Scripted orders from Bitrue's documented v2 futures payloads.
use safe_cex_engine::model::*;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_bitrue::{Bitrue, Options};
const CONTRACT: &str = r#"[{"symbol":"E-BTC-USDT","pricePrecision":1,"multiplier":0.001,"minOrderVolume":1,"maxMarketVolume":10,"maxLimitVolume":10,"minOrderMoney":5,"status":1,"side":1,"type":"E","multiplierCoin":"BTC"}]"#;
struct Harness {
    e: Engine<Bitrue>,
    requests: Vec<HttpRequest>,
    notices: Vec<Notice>,
}
impl Harness {
    fn new() -> Self {
        let mut e = Engine::new(Bitrue::new(Options {
            key: Some("replay-key".into()),
            secret: Some("replay-secret".into()),
            ..Options::default()
        }));
        e.start(0.0);
        let mut h = Self {
            e,
            requests: vec![],
            notices: vec![],
        };
        h.collect();
        let request = h.take(Method::Get, "/fapi/v1/contracts");
        h.reply(request, CONTRACT);
        h.e.store_mut().update_setting("E-BTC-USDT", |setting| {
            *setting = MarketSetting {
                leverage: 20.0,
                margin_type: Some(MarginType::Cross),
                ..MarketSetting::default()
            }
        });
        h.e.store_mut().set_tickers(vec![Ticker {
            id: "E-BTC-USDT".into(),
            symbol: "E-BTC-USDT".into(),
            last: 2000.0,
            mark: 2000.0,
            ..Ticker::default()
        }]);
        h
    }
    fn collect(&mut self) {
        for action in self.e.drain_actions() {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::Notice(n) => self.notices.push(n),
                _ => {}
            }
        }
    }
    fn take(&mut self, method: Method, path: &str) -> HttpRequest {
        let index = self
            .requests
            .iter()
            .position(|r| r.method == method && r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {method:?} {path}, requests {:?}", self.requests));
        self.requests.remove(index)
    }
    fn reply(&mut self, request: HttpRequest, body: &str) {
        self.e.handle(
            100.0,
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
    fn command(&mut self, command: Command) {
        self.e.handle(100.0, Event::Command(command));
        self.collect();
    }
    fn place(&mut self, raw: &str) {
        let orders = serde_json::from_str(raw).unwrap();
        self.command(Command::PlaceOrders { command: 7, orders });
    }
    fn failed(&self) -> bool {
        self.notices.iter().any(|n| {
            matches!(
                n,
                Notice::CommandDone {
                    command: 7,
                    result: Err(_)
                }
            )
        })
    }
    fn result(&self) -> Option<&str> {
        self.notices.iter().find_map(|n| match n {
            Notice::CommandDone {
                command: 7,
                result: Ok(v),
            } => Some(v.as_str()),
            _ => None,
        })
    }
    fn writes(&self) -> usize {
        self.requests
            .iter()
            .filter(|r| {
                r.method == Method::Post
                    && (r.url.ends_with("/fapi/v2/order") || r.url.ends_with("/fapi/v2/cancel"))
            })
            .count()
    }
    fn order(&mut self, filled: f64) {
        self.e.store_mut().upsert_order(Order {
            id: "2334151155877155281".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "E-BTC-USDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 2000.0,
            amount: 0.01,
            filled,
            remaining: 0.01 - filled,
            reduce_only: false,
            close: None,
            client_order_id: Some("callerOld".into()),
            time_in_force: Some(TimeInForce::GoodTillCancel),
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra {
                remaining_native: Some(0.01 - filled),
                filled_in_placement_units: Some(filled),
                ..OrderExtra::default()
            }),
        });
    }
    fn amend(&mut self) {
        self.command(Command::AmendOrder {
            command: 7,
            id: "2334151155877155281".into(),
            price: Some(1990.0),
            amount: None,
        });
    }
}
fn body(request: &HttpRequest) -> serde_json::Value {
    serde_json::from_str(request.body.as_ref().unwrap()).unwrap()
}
fn detail(status: &str, filled: f64, extra: &str) -> String {
    format!(
        r#"{{"code":"0","data":[{{"orderId":2334151155877155281,"origQty":0.01,"executedQty":{filled},"type":"LIMIT","status":"{status}","side":"BUY","action":"OPEN"{extra}}}]}}"#
    )
}
#[test]
fn normal_amount_stays_base_and_caller_id_is_verbatim() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000.04,"clientOrderId":"space / punctuation allowed"}]"#);
    let request = h.take(Method::Post, "/fapi/v2/order");
    let row = body(&request);
    assert_eq!(row["volume"], "0.003");
    assert_eq!(row["price"], 2000.0);
    assert_eq!(row["clientOrderId"], "space / punctuation allowed");
    assert_eq!(row["leverage"], 20.0);
    assert_eq!(row["positionType"], 1);
    assert_eq!(row["open"], "OPEN");
    h.reply(
        request,
        r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
    );
    assert_eq!(h.result(), Some(r#"["2334151155877155281"]"#));
}
#[test]
fn market_open_is_quote_with_step_buffer_and_minimum_but_reduce_is_base() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"market","side":"buy","amount":0.001}]"#);
    let request = h.take(Method::Post, "/fapi/v2/order");
    let row = body(&request);
    assert_eq!(row["amount"], 5.0);
    assert!(row.get("volume").is_none());
    h.reply(request, r#"{"code":"0","data":{"orderId":1}}"#);
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"market","side":"sell","amount":0.001,"reduceOnly":true}]"#);
    let row = body(&h.take(Method::Post, "/fapi/v2/order"));
    assert_eq!(row["volume"], "0.001");
    assert_eq!(row["open"], "CLOSE");
    assert!(row.get("amount").is_none());
}
#[test]
fn attached_protection_is_atomic_with_independent_child_ids() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"stopLoss":1800,"takeProfit":2200,"slTriggerType":"mark_price","tpTriggerType":"last_price"}]"#);
    assert_eq!(h.writes(), 1);
    let row = body(&h.take(Method::Post, "/fapi/v2/order"));
    assert!(row.get("conditionOrder").is_none());
    assert_eq!(row["triggerOrderCreateParams"][0]["triggerType"], 1);
    assert_eq!(row["triggerOrderCreateParams"][0]["triggerPriceType"], 2);
    assert_eq!(row["triggerOrderCreateParams"][1]["triggerPriceType"], 1);
    assert_ne!(
        row["triggerOrderCreateParams"][0]["clientOrderId"],
        row["triggerOrderCreateParams"][1]["clientOrderId"]
    );
}
#[test]
fn invalid_final_batch_member_prevents_every_write() {
    for tail in [
        r#"{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"clientOrderId":"bad\nID"}"#,
        r#"{"symbol":"E-BTC-USDT","type":"conditional","side":"buy","amount":0.003,"triggerPrice":2000}"#,
        r#"{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"timeInForce":"ImmediateOrCancel"}"#,
        r#"{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.02,"price":2000,"clientOrderId":"cannotSplit"}"#,
        r#"{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"clientOrderId":"cannotAttach","stopLoss":1800}"#,
    ] {
        let mut h = Harness::new();
        h.place(&format!(r#"[{{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}},{tail}]"#));
        assert!(h.failed(), "accepted {tail}");
        assert_eq!(h.writes(), 0);
    }
}
#[test]
fn splits_conserve_step_units_and_each_size_stays_below_maximum() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.025,"price":2000}]"#);
    let mut units = 0;
    let mut ids = std::collections::HashSet::new();
    for id in 1..=3 {
        let request = h.take(Method::Post, "/fapi/v2/order");
        let row = body(&request);
        let amount = row["volume"].as_str().unwrap().parse::<f64>().unwrap();
        assert!(amount <= 0.01);
        units += (amount / 0.001).round() as i32;
        assert!(ids.insert(row["clientOrderId"].as_str().unwrap().to_owned()));
        h.reply(
            request,
            &format!(r#"{{"code":"0","data":{{"orderId":{id}}}}}"#),
        );
    }
    assert_eq!(units, 25);
    assert_eq!(h.result(), Some(r#"["1","2","3"]"#));
}
#[test]
fn protective_orders_require_a_position_and_opposite_side() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"stop_market","side":"sell","amount":0.001,"price":1800}]"#);
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
    h.notices.clear();
    h.e.store_mut().set_positions(vec![Position {
        id: "E-BTC-USDT".into(),
        symbol: "E-BTC-USDT".into(),
        side: PositionSide::Long,
        entry_price: 2000.0,
        notional: 20.0,
        leverage: 20.0,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: 0.0,
        realized_pnl: None,
        contracts: 0.01,
        liquidation_price: 0.0,
        margin_size: None,
        last_update: None,
    }]);
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"stop_market","side":"sell","amount":0.001,"price":1800}]"#);
    let request = h.take(Method::Post, "/fapi/v2/order");
    let row = body(&request);
    assert_eq!(row["conditionOrder"], true);
    assert_eq!(row["open"], "CLOSE");
    assert_eq!(row["triggerType"], 1);
    assert_eq!(row["triggerPriceType"], 2);
    h.reply(request, r#"{"code":"0","data":{"orderId":123}}"#);
    assert_eq!(h.result(), Some("[]"));
}
#[test]
fn lost_placement_response_never_retries_a_write() {
    let mut h = Harness::new();
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}]"#);
    let request = h.take(Method::Post, "/fapi/v2/order");
    h.e.handle(
        110.0,
        Event::HttpFailed {
            request: request.id,
            error: "response lost after acceptance",
        },
    );
    h.collect();
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
}
#[test]
fn amend_reads_then_cancels_then_verifies_before_replacing_remaining_with_new_ids() {
    let mut h = Harness::new();
    h.order(0.004);
    h.amend();
    assert_eq!(h.writes(), 0);
    let request = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(request, &detail("PARTIALLY_FILLED", 0.004, r#", "childOrders":[{"orderId":"child","origQty":0.01,"executedQty":0,"status":"INIT","type":"MARKET","triggerType":1,"triggerPrice":1800,"triggerPriceType":2}]"#));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    assert_eq!(body(&cancel)["conditionOrder"], false);
    h.reply(
        cancel,
        r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
    );
    assert_eq!(h.writes(), 0);
    let request = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(request, &detail("CANCELLED", 0.004, ""));
    let request = h.take(Method::Post, "/fapi/v2/order");
    let row = body(&request);
    assert_eq!(row["volume"], "0.006");
    assert_eq!(row["price"], 1990.0);
    assert_ne!(row["clientOrderId"], "callerOld");
    assert_eq!(row["triggerOrderCreateParams"][0]["triggerPrice"], 1800.0);
    assert_eq!(row["triggerOrderCreateParams"][0]["triggerPriceType"], 2);
}
#[test]
fn amend_final_fill_or_missing_protection_provenance_never_places() {
    for final_reply in [
        detail("CANCELLED", 0.005, ""),
        detail("FILLED", 0.01, ""),
        detail("INIT", 0.004, ""),
    ] {
        let mut h = Harness::new();
        h.order(0.004);
        h.amend();
        let request = h.take(Method::Get, "/fapi/v2/order?");
        h.reply(request, &detail("PARTIALLY_FILLED", 0.004, ""));
        let cancel = h.take(Method::Post, "/fapi/v2/cancel");
        h.reply(
            cancel,
            r#"{"code":"0","data":{"orderId":2334151155877155281}}"#,
        );
        let request = h.take(Method::Get, "/fapi/v2/order?");
        h.reply(request, &final_reply);
        assert!(h.failed());
        assert_eq!(h.writes(), 0);
    }
    let mut h = Harness::new();
    h.order(0.004);
    h.amend();
    let request = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(request,&detail("PARTIALLY_FILLED",0.004,r#", "childOrders":[{"orderId":"child","status":"INIT","type":"MARKET","triggerType":1,"triggerPrice":1800}]"#));
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
}
#[test]
fn unrelated_cancel_identity_cannot_start_replacement() {
    let mut h = Harness::new();
    h.order(0.004);
    h.amend();
    let request = h.take(Method::Get, "/fapi/v2/order?");
    h.reply(request, &detail("INIT", 0.004, ""));
    let cancel = h.take(Method::Post, "/fapi/v2/cancel");
    h.reply(cancel, r#"{"code":"0","data":{"orderId":999}}"#);
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
}
#[test]
fn incomplete_authoritative_children_cannot_drop_known_protection() {
    for children in ["", r#", "childOrders":[]"#] {
        let mut h = Harness::new();
        h.order(0.0);
        let mut order = h.e.store().order("2334151155877155281").unwrap().clone();
        order.stop_loss = Some(1800.0);
        h.e.store_mut().upsert_order(order);
        h.amend();
        let request = h.take(Method::Get, "/fapi/v2/order?");
        h.reply(request, &detail("INIT", 0.0, children));
        assert!(h.failed());
        assert_eq!(h.writes(), 0);
    }
}
#[test]
fn settings_commit_only_after_success_and_unsupported_modes_never_write() {
    let mut h = Harness::new();
    h.command(Command::SetLeverage {
        command: 7,
        symbol: "E-BTC-USDT".into(),
        leverage: 30.0,
    });
    let request = h.take(Method::Post, "/fapi/v2/level_edit");
    h.reply(request, r#"{"code":"-1","msg":"denied"}"#);
    assert!(h.failed());
    assert_eq!(h.e.store().setting("E-BTC-USDT").unwrap().leverage, 20.0);
    h.notices.clear();
    h.command(Command::SetMarginType {
        command: 7,
        symbol: "E-BTC-USDT".into(),
        isolated: true,
    });
    assert!(h.failed());
    h.notices.clear();
    h.command(Command::SetPositionMode {
        command: 7,
        hedged: true,
    });
    assert!(h.failed());
}
#[test]
fn unknown_metadata_and_invalid_caller_null_fail_before_writes() {
    let mut h = Harness::new();
    h.e.store_mut()
        .update_setting("E-BTC-USDT", |s| s.margin_type = None);
    h.place(r#"[{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000}]"#);
    assert!(h.failed());
    assert_eq!(h.writes(), 0);
    assert!(serde_json::from_str::<PlaceOrderOpts>(r#"{"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.003,"price":2000,"clientOrderId":null}"#).is_err());
}
