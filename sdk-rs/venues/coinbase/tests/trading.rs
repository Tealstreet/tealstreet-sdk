use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_coinbase::{Coinbase, Options};
use serde::Deserialize;

const SPOT: &str = r#"{"products":[{"product_id":"BTC-USD","product_type":"SPOT","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.001","price_increment":"0.01","base_min_size":"0.001","base_max_size":"100","price":"100","volume_24h":"2","price_percentage_change_24h":"1"}]}"#;
const INTX: &str = r#"{"products":[{"product_id":"BTC-PERP-INTX","product_type":"FUTURE","base_currency_id":"BTC","quote_currency_id":"USDC","status":"TRADING","base_increment":"0.001","price_increment":"0.01","base_min_size":"0.001","base_max_size":"100","price":"100","volume_24h":"2","price_percentage_change_24h":"1","future_product_details":{"contract_expiry_type":"PERPETUAL","contract_root_unit":"BTC","contract_size":"1"}}]}"#;
const CFM: &str = r#"{"products":[{"product_id":"BTC-CDE","product_type":"FUTURE","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"1","price_increment":"0.01","base_min_size":"1","base_max_size":"100","price":"100","volume_24h":"2","price_percentage_change_24h":"1","future_product_details":{"contract_expiry_type":"EXPIRING","contract_root_unit":"BTC","contract_size":"0.01"}}]}"#;
const CFM_BALANCE: &str = r#"{"balance_summary":{"total_usd_balance":{"value":"10"},"available_margin":{"value":"8"},"unrealized_pnl":{"value":"1.5"}}}"#;
const BREAKDOWN: &str = r#"{"breakdown":{"spot_positions":[{"asset":"BTC","total_balance_crypto":1.5,"available_to_trade_crypto":1,"total_balance_fiat":150}]}}"#;
const INTX_BALANCE: &str =
    r#"{"summary":{"total_balance":{"value":"1000"},"buying_power":{"value":"800"}}}"#;
const INTX_POSITIONS: &str = r#"{"positions":[{"symbol":"BTC-PERP-INTX","position_side":"POSITION_SIDE_LONG","net_size":"2","entry_vwap":{"value":"100"},"mark_price":{"value":"110"},"liquidation_price":{"value":"50"},"leverage":"10"}]}"#;
const CFM_POSITIONS: &str = r#"{"positions":[{"product_id":"BTC-CDE","side":"SHORT","number_of_contracts":"2","avg_entry_price":"100","current_price":"90","unrealized_pnl":"0.2"}]}"#;

struct Session {
    details: std::collections::HashMap<String, String>,
    results: Vec<(u32, Result<String, String>)>,
    e: Engine<Coinbase>,
    timers: Vec<(f64, TimerId)>,
    now: f64,
    private: Option<SocketId>,
    urls: Vec<String>,
    kind: &'static str,
    open_orders: String,
    requests: Vec<HttpRequest>,
    read_portfolio: &'static str,
    failing_positions: bool,
}
impl Session {
    fn new(kind: &'static str, spot: bool) -> Self {
        Self::with_options(
            kind,
            Options {
                key: "organizations/test/apiKeys/test".into(),
                secret: STANDARD.encode([7; 32]),
                spot_only: spot,
                ..Default::default()
            },
        )
    }
    fn with_options(kind: &'static str, options: Options) -> Self {
        let mut e = Engine::new(Coinbase::new(options).unwrap());
        e.start(1000.0);
        Self {
            details: std::collections::HashMap::new(),
            e,
            timers: vec![],
            now: 1000.0,
            private: None,
            urls: vec![],
            results: vec![],
            kind,
            open_orders: String::new(),
            requests: vec![],
            read_portfolio: "portfolio-one",
            failing_positions: false,
        }
    }
    fn reply(&mut self, r: HttpRequest, status: u16, body: &str) {
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        )
    }
    fn collect(&mut self) -> Vec<HttpRequest> {
        let mut requests = vec![];
        for a in self.e.drain_actions().collect::<Vec<_>>() {
            match a {
                Action::Notice(safe_cex_engine::Notice::CommandDone { command, result }) => {
                    self.results.push((command, result))
                }
                Action::Http(r) => {
                    self.urls.push(r.url.clone());
                    self.requests.push(r.clone());
                    requests.push(r)
                }
                Action::SetTimer { timer, at_ms } => self.timers.push((at_ms, timer)),
                Action::ClearTimer { timer } => self.timers.retain(|(_, t)| *t != timer),
                Action::WsConnect { socket, url, .. } if url.contains("ws-user") => {
                    self.private = Some(socket)
                }
                _ => {}
            }
        }
        requests
    }
    fn fire(&mut self) {
        self.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (at, t) = self.timers.remove(0);
        self.now = self.now.max(at);
        self.e.handle(self.now, Event::TimerFired { timer: t });
    }
    fn answer(&mut self, r: HttpRequest) {
        let url = &r.url;
        if self.failing_positions && url.contains("/positions") {
            return self.reply(r, 503, "unavailable");
        }
        let body = if url.ends_with("/key_permissions") {
            let portfolio = if jwt_subject(&r).ends_with("/read") {
                self.read_portfolio
            } else {
                "portfolio-one"
            };
            format!(r#"{{"can_view":true,"can_trade":true,"portfolio_uuid":"{portfolio}"}}"#)
        } else if url.ends_with("/portfolios") {
            format!(
                r#"{{"portfolios":[{{"uuid":"portfolio-one","type":"{}"}}]}}"#,
                self.kind
            )
        } else if url.contains("/cfm/balance_summary") {
            CFM_BALANCE.into()
        } else if url.contains("/portfolios/portfolio-one") {
            BREAKDOWN.into()
        } else if url.contains("/intx/portfolio/") {
            INTX_BALANCE.into()
        } else if url.contains("/intx/positions/") {
            INTX_POSITIONS.into()
        } else if url.contains("/cfm/positions") {
            CFM_POSITIONS.into()
        } else if let Some(id) = url
            .split("/orders/historical/")
            .nth(1)
            .filter(|id| !id.starts_with("batch"))
        {
            self.details.get(id).cloned().unwrap_or_else(|| format!(
                r#"{{"order":{{"order_id":"{id}","product_id":"BTC-USD","order_configuration":{{"limit_limit_gtc":{{"base_size":"1","limit_price":"100"}}}}}}}}"#
            ))
        } else if url.contains("/orders/historical/batch") {
            let rows = if url.contains("order_status=OPEN") {
                self.open_orders.as_str()
            } else {
                ""
            };
            format!(r#"{{"orders":[{rows}],"has_next":false}}"#)
        } else if url.contains("/products") {
            if url.contains("contract_expiry_type=PERPETUAL") {
                INTX.into()
            } else if url.contains("product_type=FUTURE") {
                CFM.into()
            } else {
                SPOT.into()
            }
        } else if url.contains("international") {
            "[]".into()
        } else {
            panic!("unhandled {}", url)
        };
        self.reply(r, 200, &body)
    }
    fn ready(mut self) -> Self {
        for _ in 0..250 {
            let requests = self.collect();
            if !requests.is_empty() {
                for r in requests {
                    self.answer(r)
                }
                continue;
            }
            let l = self.e.store().loaded();
            if l.markets && l.balance && l.orders && l.positions {
                let socket = self.private.unwrap();
                self.e.handle(self.now, Event::WsOpened { socket });
                return self;
            }
            self.fire()
        }
        panic!("private startup did not finish")
    }
    fn message(&mut self, seq: u64, kind: &str, orders: &str) {
        let text = format!(
            r#"{{"channel":"user","sequence_num":{seq},"events":[{{"type":"{kind}","orders":{orders}}}]}}"#
        );
        self.e.handle(
            self.now + 1.0,
            Event::WsMessage {
                socket: self.private.unwrap(),
                text: &text,
            },
        );
    }
}

fn opts(symbol: &str, kind: OrderType, side: OrderSide, amount: f64) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: symbol.into(),
        kind,
        side,
        amount,
        price: Some(100.0),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        close: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
        stp_mode: None,
    }
}
impl Session {
    fn command(&mut self, command: Command) {
        self.e.handle(self.now, Event::Command(command));
    }
    fn write(&mut self) -> HttpRequest {
        for _ in 0..100 {
            let mut writes = vec![];
            for r in self.collect() {
                if r.method == safe_cex_engine::Method::Post
                    || self.kind == "INTX"
                        && r.url.contains("/orders/historical/")
                        && !r.url.contains("/batch")
                {
                    writes.push(r);
                } else {
                    self.answer(r);
                }
            }
            if !writes.is_empty() {
                assert_eq!(writes.len(), 1, "writes must serialize");
                return writes.remove(0);
            }
            self.fire();
        }
        panic!("no trading request");
    }
    fn done(&mut self, id: u32) -> Result<String, String> {
        for _ in 0..100 {
            let reads = self.collect();
            for r in reads {
                assert_eq!(r.method, safe_cex_engine::Method::Get);
                self.answer(r);
            }
            if let Some(at) = self.results.iter().position(|(command, _)| *command == id) {
                return self.results.remove(at).1;
            }
            self.fire();
        }
        panic!("no command result");
    }
    fn seed(&mut self, id: &str, product: &str, size: &str, price: &str) {
        self.details.insert(id.into(), format!(r#"{{"order":{{"order_id":"{id}","product_id":"{product}","order_configuration":{{"limit_limit_gtc":{{"base_size":"{size}","limit_price":"{price}"}}}}}}}}"#));
        let orders = format!(
            r#"[{{"order_id":"{id}","client_order_id":"original-client","product_id":"{product}","status":"OPEN","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"{size}","limit_price":"{price}","stop_price":"0","avg_price":"0"}}]"#
        );
        self.message(0, "snapshot", &orders);
        self.reconcile_order(id);
    }
    fn reconcile_order(&mut self, id: &str) {
        for _ in 0..100 {
            if self.e.store().order(id).is_some() {
                return;
            }
            for r in self.collect() {
                self.answer(r);
            }
            if self.e.store().order(id).is_none() {
                self.fire();
            }
        }
        panic!("seed order did not reconcile");
    }
}

#[derive(Deserialize)]
struct CreateWire {
    client_order_id: String,
    product_id: String,
    side: String,
    order_configuration: ConfigWire,
}
#[derive(Deserialize)]
struct ConfigWire {
    market_market_ioc: Option<SizeWire>,
    limit_limit_gtc: Option<LimitWire>,
    stop_limit_stop_limit_gtc: Option<StopWire>,
}
#[derive(Deserialize)]
struct SizeWire {
    base_size: String,
}
#[derive(Deserialize)]
struct LimitWire {
    base_size: String,
    limit_price: String,
    post_only: bool,
}
#[derive(Deserialize)]
struct StopWire {
    base_size: String,
    limit_price: String,
    stop_price: String,
    stop_direction: String,
}
fn create_wire(request: &HttpRequest) -> CreateWire {
    serde_json::from_str(request.body.as_ref().unwrap()).unwrap()
}

#[test]
fn native_create_payloads_preserve_caller_ids_market_ioc_and_postonly_limit() {
    let mut s = Session::new("DEFAULT", true).ready();
    let mut first = opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.2344);
    first.time_in_force = Some(TimeInForce::PostOnly);
    first.client_order_id = Some("caller-verbatim_42".into());
    first.price = Some(101.234);
    let second = opts("BTC-USD", OrderType::Market, OrderSide::Sell, 0.5);
    s.command(Command::PlaceOrders {
        command: 1,
        orders: vec![first, second],
    });
    let r = s.write();
    assert!(r.url.ends_with("/orders"));
    let wire = create_wire(&r);
    assert_eq!(
        (
            wire.client_order_id.as_str(),
            wire.product_id.as_str(),
            wire.side.as_str()
        ),
        ("caller-verbatim_42", "BTC-USD", "BUY")
    );
    let limit = wire.order_configuration.limit_limit_gtc.unwrap();
    assert_eq!(
        (
            limit.base_size.as_str(),
            limit.limit_price.as_str(),
            limit.post_only
        ),
        ("1.234", "101.23", true)
    );
    s.reply(r,200,r#"{"success":true,"success_response":{"order_id":"limit-id","product_id":"BTC-USD","client_order_id":"caller-verbatim_42","side":"BUY"}}"#);
    let r = s.write();
    let wire = create_wire(&r);
    assert!(wire.client_order_id.starts_with("tealstreet-"));
    assert_eq!(
        wire.order_configuration
            .market_market_ioc
            .unwrap()
            .base_size,
        "0.5"
    );
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"market-id"}}"#,
    );
    assert_eq!(s.done(1).unwrap(), r#"["limit-id","market-id"]"#);
}

#[test]
fn cfm_placement_amount_is_native_contracts_and_store_amount_is_base() {
    let mut s = Session::new("CFM", false).ready();
    s.command(Command::PlaceOrders {
        command: 2,
        orders: vec![opts("BTC-CDE", OrderType::Limit, OrderSide::Buy, 2.0)],
    });
    let r = s.write();
    assert_eq!(
        create_wire(&r)
            .order_configuration
            .limit_limit_gtc
            .unwrap()
            .base_size,
        "2"
    );
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"cfm-created"}}"#,
    );
    assert!(s.done(2).is_ok());
    let order = s.e.store().order("cfm-created").unwrap();
    assert_eq!((order.amount, order.remaining), (0.02, 0.02));
    assert_eq!(order.extra.as_ref().unwrap().remaining_native, Some(2.0));
}

#[test]
fn stop_limit_uses_side_direction_and_one_percent_limit_offset() {
    let mut s = Session::new("INTX", false).ready();
    s.command(Command::PlaceOrders {
        command: 3,
        orders: vec![opts(
            "BTC-PERP-INTX",
            OrderType::StopLoss,
            OrderSide::Sell,
            0.5,
        )],
    });
    let r = s.write();
    let stop = create_wire(&r)
        .order_configuration
        .stop_limit_stop_limit_gtc
        .unwrap();
    assert_eq!(
        (
            stop.base_size.as_str(),
            stop.stop_price.as_str(),
            stop.limit_price.as_str(),
            stop.stop_direction.as_str()
        ),
        ("0.5", "100", "99", "STOP_DIRECTION_STOP_DOWN")
    );
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"stop"}}"#,
    );
    assert!(s.done(3).is_ok());
}

#[test]
fn entire_batch_is_preflighted_before_any_write() {
    let mut s = Session::new("DEFAULT", true).ready();
    let good = opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0);
    let mut cases = vec![];
    let mut bad = good.clone();
    bad.client_order_id = Some("has space".into());
    cases.push(bad);
    let mut bad = good.clone();
    bad.client_order_id = Some("x".repeat(129));
    cases.push(bad);
    let mut bad = good.clone();
    bad.client_order_id = Some("é".repeat(129));
    cases.push(bad);
    let mut bad = good.clone();
    bad.take_profit = Some(200.0);
    cases.push(bad);
    let mut bad = good.clone();
    bad.kind = OrderType::TakeProfit;
    cases.push(bad);
    let mut bad = good.clone();
    bad.time_in_force = Some(TimeInForce::ImmediateOrCancel);
    cases.push(bad);
    let mut bad = good.clone();
    bad.bybit_bbo = Some(BybitBbo {
        side_type: "Counterparty".into(),
        level: 1.0,
    });
    cases.push(bad);
    let mut bad = good.clone();
    bad.bybit_slippage_tolerance_ticks = Some(2.0);
    cases.push(bad);
    let mut bad = good.clone();
    bad.stp_mode = Some(StpMode::ExpireBoth);
    cases.push(bad);
    let mut bad = good.clone();
    bad.trigger_type = Some(StopTriggerType::MarkPrice);
    cases.push(bad);
    let mut bad = good.clone();
    bad.amount = f64::NAN;
    cases.push(bad);
    for (index, bad) in cases.into_iter().enumerate() {
        let id = 100 + index as u32;
        s.command(Command::PlaceOrders {
            command: id,
            orders: vec![good.clone(), bad],
        });
        assert!(s.done(id).is_err());
    }
    let mut same = good.clone();
    same.client_order_id = Some("same".into());
    s.command(Command::PlaceOrders {
        command: 120,
        orders: vec![same.clone(), same],
    });
    assert!(s.done(120).unwrap_err().contains("duplicate"));
}

#[test]
fn caller_ids_allow_exactly_one_hundred_twenty_eight_unicode_characters() {
    let mut s = Session::new("DEFAULT", true).ready();
    let caller = "é".repeat(128);
    let mut order = opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0);
    order.client_order_id = Some(caller.clone());
    s.command(Command::PlaceOrders {
        command: 121,
        orders: vec![order],
    });
    let request = s.write();
    assert_eq!(create_wire(&request).client_order_id, caller);
    s.reply(
        request,
        200,
        r#"{"success":true,"success_response":{"order_id":"unicode-limit"}}"#,
    );
    assert!(s.done(121).is_ok());
}

#[test]
fn missing_create_id_is_ambiguous_and_never_retried_or_followed_by_second_order() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.command(Command::PlaceOrders {
        command: 4,
        orders: vec![
            opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0),
            opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 2.0),
        ],
    });
    let r = s.write();
    s.reply(r, 200, r#"{"success":true}"#);
    assert!(s.done(4).unwrap_err().contains("order ID"));
}

#[test]
fn transport_failure_never_resends_a_write() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.command(Command::PlaceOrders {
        command: 5,
        orders: vec![opts("BTC-USD", OrderType::Market, OrderSide::Buy, 1.0)],
    });
    let r = s.write();
    s.e.handle(
        s.now,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    assert!(s.done(5).is_err());
}

#[test]
fn cancel_checks_every_native_result_and_only_successes_become_canceling() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.seed("one", "BTC-USD", "1", "100");
    s.message(1,"update",r#"[{"order_id":"two","product_id":"BTC-USD","status":"OPEN","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"2","limit_price":"100","stop_price":"0"}]"#);
    s.reconcile_order("two");
    s.command(Command::CancelOrders {
        command: 6,
        ids: vec!["one".into(), "two".into()],
    });
    let r = s.write();
    s.reply(r,200,r#"{"results":[{"order_id":"one","success":true},{"order_id":"two","success":false,"failure_reason":"ORDER_NOT_FOUND"}]}"#);
    assert!(s.done(6).unwrap_err().contains("two"));
    assert_eq!(
        s.e.store().order("one").unwrap().status,
        OrderStatus::Canceling
    );
    assert_eq!(s.e.store().order("two").unwrap().status, OrderStatus::Open);
}

#[test]
fn malformed_cancel_ids_never_terminalize_or_replace() {
    let mut s = Session::new("INTX", false).ready();
    s.seed("old", "BTC-PERP-INTX", "1", "100");
    s.command(Command::AmendOrder {
        command: 7,
        id: "old".into(),
        price: Some(101.0),
        amount: None,
    });
    let r = s.write();
    s.reply(
        r,
        200,
        r#"{"results":[{"order_id":"wrong","success":true}]}"#,
    );
    assert!(s.done(7).is_err());
    assert_eq!(s.e.store().order("old").unwrap().status, OrderStatus::Open);
}

#[test]
fn intx_amend_waits_for_zero_fill_terminal_confirmation_and_mints_new_client_id() {
    let mut s = Session::new("INTX", false).ready();
    s.seed("old", "BTC-PERP-INTX", "1", "100");
    s.command(Command::AmendOrder {
        command: 8,
        id: "old".into(),
        price: Some(101.0),
        amount: None,
    });
    let r = s.write();
    assert!(r.url.ends_with("/batch_cancel"));
    s.reply(r, 200, r#"{"results":[{"order_id":"old","success":true}]}"#);
    let r = s.write();
    assert_eq!(r.method, safe_cex_engine::Method::Get);
    assert!(r.url.ends_with("/historical/old"));
    assert_eq!(
        s.e.store().order("old").unwrap().status,
        OrderStatus::Canceling
    );
    s.reply(r,200,r#"{"order":{"order_id":"old","product_id":"BTC-PERP-INTX","status":"CANCELLED","filled_size":"0"}}"#);
    let r = s.write();
    let wire = create_wire(&r);
    assert_ne!(wire.client_order_id, "original-client");
    assert_eq!(
        wire.order_configuration
            .limit_limit_gtc
            .unwrap()
            .limit_price,
        "101"
    );
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"new"}}"#,
    );
    assert_eq!(s.done(8).unwrap(), r#"["new"]"#);
    assert_eq!(
        s.e.store().order("old").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn acknowledged_but_still_open_cancellation_never_places_replacement() {
    let mut s = Session::new("INTX", false).ready();
    s.seed("old", "BTC-PERP-INTX", "1", "100");
    s.command(Command::AmendOrder {
        command: 9,
        id: "old".into(),
        price: Some(101.0),
        amount: None,
    });
    let r = s.write();
    s.reply(r, 200, r#"{"results":[{"order_id":"old","success":true}]}"#);
    let r = s.write();
    s.reply(r,200,r#"{"order":{"order_id":"old","product_id":"BTC-PERP-INTX","status":"CANCEL_QUEUED","filled_size":"0"}}"#);
    assert!(s.done(9).is_err());
}

#[test]
fn native_spot_edit_preserves_id_and_requires_documented_empty_errors() {
    for (response, success) in [
        (r#"{"success":true,"errors":[]}"#, true),
        (r#"{"success":true}"#, false),
        (
            r#"{"success":true,"errors":[{"edit_failure_reason":"ORDER_NOT_EDITABLE"}]}"#,
            false,
        ),
    ] {
        let mut s = Session::new("DEFAULT", true).ready();
        s.seed("spot", "BTC-USD", "1.2500", "100.00");
        s.command(Command::AmendOrder {
            command: 10,
            id: "spot".into(),
            price: Some(101.004),
            amount: None,
        });
        let r = s.write();
        assert!(r.url.ends_with("/edit"));
        #[derive(Deserialize)]
        struct EditWire {
            order_id: String,
            price: String,
            size: String,
        }
        let e: EditWire = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(
            (e.order_id.as_str(), e.price.as_str(), e.size.as_str()),
            ("spot", "101", "1.25")
        );
        s.reply(r, 200, response);
        assert_eq!(s.done(10).is_ok(), success);
    }
}

#[test]
fn native_edit_rejects_float_aliasing_size_provenance_and_cfm() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.seed("spot", "BTC-USD", "1.00000000000000000001", "100");
    s.command(Command::AmendOrder {
        command: 11,
        id: "spot".into(),
        price: Some(101.0),
        amount: None,
    });
    assert!(s.done(11).is_err());
    let mut s = Session::new("CFM", false).ready();
    s.seed("cfm", "BTC-CDE", "1", "100");
    s.command(Command::AmendOrder {
        command: 12,
        id: "cfm".into(),
        price: Some(101.0),
        amount: None,
    });
    assert!(s.done(12).is_err());
}

#[test]
fn queued_native_edit_rechecks_proof_after_private_fill() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.seed("spot", "BTC-USD", "1", "100");
    s.command(Command::PlaceOrders {
        command: 13,
        orders: vec![opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0)],
    });
    let first = s.write();
    s.command(Command::AmendOrder {
        command: 14,
        id: "spot".into(),
        price: Some(101.0),
        amount: None,
    });
    s.message(1,"update",r#"[{"order_id":"spot","product_id":"BTC-USD","status":"OPEN","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0.5","leaves_quantity":"0.5","limit_price":"100","avg_price":"100","stop_price":"0"}]"#);
    s.reply(
        first,
        200,
        r#"{"success":true,"success_response":{"order_id":"first"}}"#,
    );
    assert!(s.done(13).is_ok());
    assert!(s.done(14).is_err());
}

#[test]
fn reduce_only_batch_reserves_observed_opposite_position_before_any_write() {
    let mut s = Session::new("INTX", false).ready();
    let mut closing = opts("BTC-PERP-INTX", OrderType::Limit, OrderSide::Sell, 1.5);
    closing.reduce_only = Some(true);
    s.command(Command::PlaceOrders {
        command: 15,
        orders: vec![closing.clone(), closing],
    });
    assert!(s.done(15).is_err());
    let mut close = opts("BTC-PERP-INTX", OrderType::Limit, OrderSide::Sell, 1.0);
    close.reduce_only = Some(true);
    s.command(Command::PlaceOrders {
        command: 16,
        orders: vec![close],
    });
    let r = s.write();
    assert!(!r.body.as_ref().unwrap().contains("reduce_only"));
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"reducing"}}"#,
    );
    assert!(s.done(16).is_ok());
    assert!(s.e.store().order("reducing").unwrap().reduce_only);
}

#[test]
fn queued_reduce_only_revalidates_position_before_dispatch() {
    let mut s = Session::new("INTX", false).ready();
    s.command(Command::PlaceOrders {
        command: 17,
        orders: vec![opts("BTC-PERP-INTX", OrderType::Limit, OrderSide::Buy, 1.0)],
    });
    let first = s.write();
    let mut close = opts("BTC-PERP-INTX", OrderType::StopLoss, OrderSide::Sell, 1.0);
    close.reduce_only = Some(true);
    s.command(Command::PlaceOrders {
        command: 18,
        orders: vec![close],
    });
    s.e.store_mut().set_positions(vec![]);
    s.reply(
        first,
        200,
        r#"{"success":true,"success_response":{"order_id":"first"}}"#,
    );
    assert!(s.done(17).is_ok());
    assert!(s.done(18).is_err());
}

#[test]
fn cancel_chunks_are_serial_and_never_exceed_one_hundred_native_ids() {
    let mut s = Session::new("DEFAULT", true).ready();
    let ids: Vec<_> = (0..201).map(|n| format!("order-{n}")).collect();
    s.command(Command::CancelOrders {
        command: 19,
        ids: ids.clone(),
    });
    #[derive(Deserialize)]
    struct CancelWire {
        order_ids: Vec<String>,
    }
    for (index, expected) in ids.chunks(100).enumerate() {
        let r = s.write();
        let wire: CancelWire = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(wire.order_ids, expected);
        assert_eq!(wire.order_ids.len(), if index < 2 { 100 } else { 1 });
        let rows = expected
            .iter()
            .map(|id| format!(r#"{{"order_id":"{id}","success":true}}"#))
            .collect::<Vec<_>>()
            .join(",");
        s.reply(r, 200, &format!(r#"{{"results":[{rows}]}}"#));
    }
    assert_eq!(s.done(19).unwrap(), "null");
}

#[test]
fn submitting_another_job_cannot_dispatch_the_active_jobs_next_step_early() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.command(Command::PlaceOrders {
        command: 20,
        orders: vec![
            opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0),
            opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 2.0),
        ],
    });
    let first = s.write();
    s.command(Command::PlaceOrders {
        command: 21,
        orders: vec![opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 3.0)],
    });
    for r in s.collect() {
        assert_eq!(r.method, safe_cex_engine::Method::Get);
        s.answer(r);
    }
    s.reply(
        first,
        200,
        r#"{"success":true,"success_response":{"order_id":"first"}}"#,
    );
    let second = s.write();
    assert_eq!(
        create_wire(&second)
            .order_configuration
            .limit_limit_gtc
            .unwrap()
            .base_size,
        "2"
    );
    s.reply(
        second,
        200,
        r#"{"success":true,"success_response":{"order_id":"second"}}"#,
    );
    let third = s.write();
    assert_eq!(
        create_wire(&third)
            .order_configuration
            .limit_limit_gtc
            .unwrap()
            .base_size,
        "3"
    );
    s.reply(
        third,
        200,
        r#"{"success":true,"success_response":{"order_id":"third"}}"#,
    );
    assert!(s.done(20).is_ok());
    assert!(s.done(21).is_ok());
}

#[test]
fn early_terminal_stream_event_is_never_reopened_by_the_placement_response() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.command(Command::PlaceOrders {
        command: 22,
        orders: vec![opts("BTC-USD", OrderType::Market, OrderSide::Buy, 1.0)],
    });
    let r = s.write();
    s.message(0,"update",r#"[{"order_id":"filled-before-ack","product_id":"BTC-USD","status":"FILLED","order_side":"BUY","order_type":"Market","cumulative_quantity":"1","leaves_quantity":"0","avg_price":"100"}]"#);
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"filled-before-ack"}}"#,
    );
    assert!(s.done(22).is_ok());
    assert_eq!(
        s.e.store().order("filled-before-ack").unwrap().status,
        OrderStatus::Closed
    );
}

#[test]
fn explicit_close_uses_observed_derivative_capacity_and_rejects_caller_id_shape() {
    let mut s = Session::new("INTX", false).ready();
    let mut close = opts("BTC-PERP-INTX", OrderType::Market, OrderSide::Sell, 1.0);
    close.close = Some(true);
    let mut with_id = close.clone();
    with_id.client_order_id = Some("caller-close".into());
    s.command(Command::PlaceOrders {
        command: 23,
        orders: vec![with_id],
    });
    assert!(s.done(23).is_err());
    s.command(Command::PlaceOrders {
        command: 24,
        orders: vec![close],
    });
    let r = s.write();
    assert!(!r.body.as_ref().unwrap().contains("reduce_only"));
    assert_eq!(
        create_wire(&r)
            .order_configuration
            .market_market_ioc
            .unwrap()
            .base_size,
        "1"
    );
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"closed-position"}}"#,
    );
    assert!(s.done(24).is_ok());
    assert!(s.e.store().order("closed-position").unwrap().reduce_only);
}
impl Session {
    fn next_position_read(&mut self) -> HttpRequest {
        for _ in 0..200 {
            let requests = self.collect();
            if requests.is_empty() {
                self.fire();
                continue;
            }
            for r in requests {
                if r.url.contains("/intx/positions/") {
                    return r;
                }
                assert_eq!(r.method, safe_cex_engine::Method::Get);
                self.answer(r)
            }
        }
        panic!("missing position read")
    }
    fn closing(&mut self, id: u32, amount: f64) {
        let mut order = opts("BTC-PERP-INTX", OrderType::Market, OrderSide::Sell, amount);
        order.reduce_only = Some(true);
        self.command(Command::PlaceOrders {
            command: id,
            orders: vec![order],
        });
    }
    fn filled_sell(&mut self, seq: u64, id: &str, size: &str) {
        let row = format!(
            r#"[{{"order_id":"{id}","product_id":"BTC-PERP-INTX","status":"FILLED","order_side":"SELL","order_type":"Market","cumulative_quantity":"{size}","leaves_quantity":"0","avg_price":"100"}}]"#
        );
        self.message(seq, "update", &row);
    }
}
#[test]
fn terminal_reduce_fill_remains_reserved_until_causal_position_decrease() {
    let mut s = Session::new("INTX", false).ready();
    s.message(0, "snapshot", "[]");
    s.closing(200, 2.0);
    let r = s.write();
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"reduced"}}"#,
    );
    assert!(s.done(200).is_ok());
    let held = s.next_position_read();
    s.filled_sell(1, "reduced", "2");
    s.closing(201, 2.0);
    assert!(s.done(201).is_err());
    // A response enqueued before the fill cannot certify its execution, even if flat.
    s.reply(held, 200, r#"{"positions":[]}"#);
    let stale = s.next_position_read();
    s.reply(stale, 200, INTX_POSITIONS);
    s.closing(202, 2.0);
    assert!(s.done(202).is_err());
    let fresh = s.next_position_read();
    s.reply(fresh, 200, r#"{"positions":[]}"#);
    s.closing(203, 2.0);
    assert!(s.done(203).is_err());
    // Once the old leg is proven flat, a new observed leg has fresh capacity.
    let next = s.next_position_read();
    s.reply(next, 200, INTX_POSITIONS);
    s.closing(204, 2.0);
    let r = s.write();
    assert_eq!(
        create_wire(&r)
            .order_configuration
            .market_market_ioc
            .unwrap()
            .base_size,
        "2"
    );
}
#[test]
fn two_filled_closing_orders_need_aggregate_position_decrease() {
    let mut s = Session::new("INTX", false).ready();
    s.message(0, "snapshot", "[]");
    s.closing(210, 1.0);
    let first = s.write();
    s.reply(
        first,
        200,
        r#"{"success":true,"success_response":{"order_id":"reduce-one"}}"#,
    );
    assert!(s.done(210).is_ok());
    s.closing(211, 1.0);
    let second = s.write();
    s.reply(
        second,
        200,
        r#"{"success":true,"success_response":{"order_id":"reduce-two"}}"#,
    );
    assert!(s.done(211).is_ok());
    s.filled_sell(1, "reduce-one", "1");
    s.filled_sell(2, "reduce-two", "1");
    let fresh = s.next_position_read();
    s.reply(
        fresh,
        200,
        &INTX_POSITIONS.replace("\"net_size\":\"2\"", "\"net_size\":\"1\""),
    );
    s.closing(212, 1.0);
    assert!(s.done(212).is_err());
}
#[test]
fn cancel_symbol_resolves_ids_after_earlier_queued_placements() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.command(Command::PlaceOrders {
        command: 220,
        orders: vec![opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0)],
    });
    let first = s.write();
    s.command(Command::PlaceOrders {
        command: 221,
        orders: vec![opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 2.0)],
    });
    s.command(Command::CancelSymbolOrders {
        command: 222,
        symbol: "BTC-USD".into(),
    });
    s.reply(
        first,
        200,
        r#"{"success":true,"success_response":{"order_id":"first"}}"#,
    );
    let second = s.write();
    s.reply(
        second,
        200,
        r#"{"success":true,"success_response":{"order_id":"second"}}"#,
    );
    let cancel = s.write();
    assert!(cancel.url.ends_with("batch_cancel"));
    #[derive(Deserialize)]
    struct Cancel {
        order_ids: Vec<String>,
    }
    let body: Cancel = serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
    assert_eq!(body.order_ids, vec!["first", "second"]);
    s.reply(
        cancel,
        200,
        r#"{"results":[{"order_id":"first","success":true},{"order_id":"second","success":true}]}"#,
    );
    assert!(s.done(222).is_ok());
}
#[test]
fn cancel_symbol_does_not_claim_success_for_unknown_placement_identity() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.command(Command::PlaceOrders {
        command: 230,
        orders: vec![opts("BTC-USD", OrderType::Market, OrderSide::Buy, 1.0)],
    });
    let first = s.write();
    s.command(Command::CancelSymbolOrders {
        command: 231,
        symbol: "BTC-USD".into(),
    });
    s.e.handle(
        s.now,
        Event::HttpFailed {
            request: first.id,
            error: "response lost",
        },
    );
    assert!(s.done(230).is_err());
    assert!(s.done(231).unwrap_err().contains("unresolved"));
}
#[test]
fn external_partial_close_reserves_execution_until_position_repair() {
    let mut s = Session::new("INTX", false).ready();
    s.message(0,"snapshot",r#"[{"order_id":"external-close","product_id":"BTC-PERP-INTX","status":"OPEN","order_side":"SELL","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"1","limit_price":"100","avg_price":"0"}]"#);
    s.message(1,"update",r#"[{"order_id":"external-close","product_id":"BTC-PERP-INTX","status":"OPEN","order_side":"SELL","order_type":"Limit","cumulative_quantity":"0.5","leaves_quantity":"0.5","limit_price":"100","avg_price":"100"}]"#);
    s.closing(240, 1.5);
    assert!(s.done(240).is_err());
    s.closing(241, 1.0);
    let r = s.write();
    assert_eq!(
        create_wire(&r)
            .order_configuration
            .market_market_ioc
            .unwrap()
            .base_size,
        "1"
    );
}
#[test]
fn delayed_create_ack_cannot_resurrect_pruned_terminal_order() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.command(Command::PlaceOrders {
        command: 250,
        orders: vec![opts("BTC-USD", OrderType::Market, OrderSide::Buy, 1.0)],
    });
    let r = s.write();
    s.message(0, "snapshot", "[]");
    s.message(1,"update",r#"[{"order_id":"pruned-filled","product_id":"BTC-USD","status":"FILLED","order_side":"BUY","order_type":"Market","cumulative_quantity":"1","leaves_quantity":"0","avg_price":"100"}]"#);
    s.message(2, "snapshot", "[]");
    assert!(s.e.store().order("pruned-filled").is_none());
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"pruned-filled"}}"#,
    );
    assert!(s.done(250).is_ok());
    assert!(s.e.store().order("pruned-filled").is_none());
}
#[test]
fn early_fill_without_client_id_is_coalesced_with_later_ack_reservation() {
    let mut s = Session::new("INTX", false).ready();
    s.message(0, "snapshot", "[]");
    s.closing(260, 1.0);
    let r = s.write();
    s.filled_sell(1, "early-reduction", "1");
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"early-reduction"}}"#,
    );
    assert!(s.done(260).is_ok());
    for _ in 0..2 {
        let r = s.next_position_read();
        s.reply(
            r,
            200,
            &INTX_POSITIONS.replace("\"net_size\":\"2\"", "\"net_size\":\"1\""),
        );
    }
    s.closing(261, 1.0);
    let r = s.write();
    assert_eq!(
        create_wire(&r)
            .order_configuration
            .market_market_ioc
            .unwrap()
            .base_size,
        "1"
    );
}

#[test]
fn lost_spot_create_response_recovers_identity_from_zero_fill_user_order() {
    for event in ["update", "snapshot"] {
        let mut s = Session::new("DEFAULT", true).ready();
        s.message(0, "snapshot", "[]");
        let mut order = opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0);
        order.client_order_id = Some("recover-client".into());
        s.command(Command::PlaceOrders {
            command: 270,
            orders: vec![order],
        });
        let r = s.write();
        s.e.handle(
            s.now,
            Event::HttpFailed {
                request: r.id,
                error: "response lost",
            },
        );
        assert!(s.done(270).is_err());
        s.message(1, event, r#"[{"order_id":"recovered","client_order_id":"recover-client","product_id":"BTC-USD","status":"OPEN","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"1","limit_price":"100","avg_price":"0"}]"#);
        assert!(s.e.store().order("recovered").is_some());
        s.command(Command::CancelSymbolOrders {
            command: 271,
            symbol: "BTC-USD".into(),
        });
        let cancel = s.write();
        assert!(cancel.url.ends_with("/batch_cancel"));
        assert_eq!(
            cancel.body.as_deref(),
            Some(r#"{"order_ids":["recovered"]}"#)
        );
    }
}

#[test]
fn recovered_zero_fill_cfm_identity_keeps_closing_exposure_reserved() {
    let mut s = Session::new("DEFAULT", false).ready();
    s.message(0, "snapshot", "[]");
    let mut order = opts("BTC-CDE", OrderType::Limit, OrderSide::Buy, 1.0);
    order.client_order_id = Some("recover-cfm".into());
    order.reduce_only = Some(true);
    s.command(Command::PlaceOrders {
        command: 280,
        orders: vec![order],
    });
    let r = s.write();
    s.e.handle(
        s.now,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    assert!(s.done(280).is_err());
    s.message(1,"update",r#"[{"order_id":"recovered-cfm","client_order_id":"recover-cfm","product_id":"BTC-CDE","status":"OPEN","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"1","limit_price":"100","avg_price":"0"}]"#);
    let mut next = opts("BTC-CDE", OrderType::Market, OrderSide::Buy, 2.0);
    next.reduce_only = Some(true);
    s.command(Command::PlaceOrders {
        command: 281,
        orders: vec![next],
    });
    assert!(s.done(281).is_err());
    s.command(Command::CancelSymbolOrders {
        command: 282,
        symbol: "BTC-CDE".into(),
    });
    let cancel = s.write();
    assert_eq!(
        cancel.body.as_deref(),
        Some(r#"{"order_ids":["recovered-cfm"]}"#)
    );
}

#[test]
fn acknowledged_spot_order_pruned_by_snapshot_has_no_stale_cancel_reservation() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.message(0, "snapshot", "[]");
    s.command(Command::PlaceOrders {
        command: 290,
        orders: vec![opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0)],
    });
    let r = s.write();
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"spot-ack"}}"#,
    );
    assert!(s.done(290).is_ok());
    s.message(1, "snapshot", "[]");
    assert!(s.e.store().order("spot-ack").is_none());
    s.command(Command::CancelSymbolOrders {
        command: 291,
        symbol: "BTC-USD".into(),
    });
    assert_eq!(s.done(291).unwrap(), "null");
}

#[test]
fn quote_sized_zero_fill_spot_user_order_never_gains_native_edit_proof() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.message(0, "snapshot", "[]");
    s.details.insert("quote-order".into(), r#"{"order":{"order_id":"quote-order","product_id":"BTC-USD","order_configuration":{"limit_limit_gtc":{"quote_size":"200","limit_price":"100"}}}}"#.into());
    s.message(1,"update",r#"[{"order_id":"quote-order","product_id":"BTC-USD","status":"OPEN","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"200","limit_price":"100","avg_price":"0"}]"#);
    s.reconcile_order("quote-order");
    assert_eq!(s.e.store().order("quote-order").unwrap().amount, 2.0);
    s.command(Command::AmendOrder {
        command: 300,
        id: "quote-order".into(),
        price: Some(101.0),
        amount: None,
    });
    assert!(s.done(300).unwrap_err().contains("provenance"));
}

#[test]
fn recovered_client_identity_coalesces_earlier_external_fill_without_create_ack() {
    let mut s = Session::new("INTX", false).ready();
    s.message(0, "snapshot", "[]");
    s.closing(310, 1.0);
    let r = s.write();
    let client = create_wire(&r).client_order_id;
    s.e.handle(
        s.now,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    assert!(s.done(310).is_err());
    s.filled_sell(1, "recovered-fill", "1");
    s.message(2, "update", &format!(r#"[{{"order_id":"recovered-fill","client_order_id":"{client}","product_id":"BTC-PERP-INTX","status":"FILLED","order_side":"SELL","order_type":"Market","cumulative_quantity":"1","leaves_quantity":"0","avg_price":"100"}}]"#));
    for _ in 0..2 {
        let r = s.next_position_read();
        s.reply(
            r,
            200,
            &INTX_POSITIONS.replace("\"net_size\":\"2\"", "\"net_size\":\"1\""),
        );
    }
    s.closing(311, 1.0);
    let r = s.write();
    assert_eq!(
        create_wire(&r)
            .order_configuration
            .market_market_ioc
            .unwrap()
            .base_size,
        "1"
    );
}

#[test]
fn lost_spot_create_response_recovers_zero_fill_terminal_identity() {
    for status in ["CANCELLED", "EXPIRED", "FAILED"] {
        let mut s = Session::new("DEFAULT", true).ready();
        s.message(0, "snapshot", "[]");
        let mut order = opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0);
        order.client_order_id = Some("terminal-client".into());
        s.command(Command::PlaceOrders {
            command: 320,
            orders: vec![order],
        });
        let r = s.write();
        s.e.handle(
            s.now,
            Event::HttpFailed {
                request: r.id,
                error: "response lost",
            },
        );
        assert!(s.done(320).is_err());
        s.message(1, "update", &format!(r#"[{{"order_id":"terminal-zero","client_order_id":"terminal-client","product_id":"BTC-USD","status":"{status}","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"0","limit_price":"100","avg_price":"0"}}]"#));
        let recovered = s.e.store().order("terminal-zero").unwrap();
        assert_eq!(recovered.status, OrderStatus::Canceled);
        assert_eq!(recovered.amount, 1.0);
        s.command(Command::CancelSymbolOrders {
            command: 321,
            symbol: "BTC-USD".into(),
        });
        assert_eq!(s.done(321).unwrap(), "null");
    }
}

#[test]
fn zero_fill_terminal_before_create_ack_cannot_be_resurrected_after_pruning() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.message(0, "snapshot", "[]");
    s.command(Command::PlaceOrders {
        command: 330,
        orders: vec![opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0)],
    });
    let r = s.write();
    let client = create_wire(&r).client_order_id;
    s.message(1, "update", &format!(r#"[{{"order_id":"early-cancel","client_order_id":"{client}","product_id":"BTC-USD","status":"CANCELLED","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"0","limit_price":"100","avg_price":"0"}}]"#));
    assert_eq!(
        s.e.store().order("early-cancel").unwrap().status,
        OrderStatus::Canceled
    );
    s.message(2, "snapshot", "[]");
    assert!(s.e.store().order("early-cancel").is_none());
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"early-cancel"}}"#,
    );
    assert!(s.done(330).is_ok());
    assert!(s.e.store().order("early-cancel").is_none());
}

#[test]
fn zero_fill_terminal_without_client_id_resolves_from_create_ack() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.message(0, "snapshot", "[]");
    s.command(Command::PlaceOrders {
        command: 350,
        orders: vec![opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0)],
    });
    let r = s.write();
    s.message(1, "update", r#"[{"order_id":"anonymous-cancel","product_id":"BTC-USD","status":"CANCELLED","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"0","limit_price":"100","avg_price":"0"}]"#);
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"anonymous-cancel"}}"#,
    );
    assert!(s.done(350).is_ok());
    assert_eq!(
        s.e.store().order("anonymous-cancel").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn duplicate_client_ack_preserves_authoritative_quote_sizing_and_edit_provenance() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.message(0, "snapshot", "[]");
    s.details.insert("quote-duplicate".into(), r#"{"order":{"order_id":"quote-duplicate","product_id":"BTC-USD","order_configuration":{"limit_limit_gtc":{"quote_size":"200","limit_price":"100"}}}}"#.into());
    let row = r#"[{"order_id":"quote-duplicate","client_order_id":"duplicate-client","product_id":"BTC-USD","status":"OPEN","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"200","limit_price":"100","avg_price":"0"}]"#;
    s.message(1, "update", row);
    s.reconcile_order("quote-duplicate");
    let mut order = opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0);
    order.client_order_id = Some("duplicate-client".into());
    s.command(Command::PlaceOrders {
        command: 360,
        orders: vec![order],
    });
    let r = s.write();
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"quote-duplicate"}}"#,
    );
    assert!(s.done(360).is_ok());
    s.message(2, "update", row);
    assert_eq!(s.e.store().order("quote-duplicate").unwrap().amount, 2.0);
    s.command(Command::AmendOrder {
        command: 361,
        id: "quote-duplicate".into(),
        price: Some(101.0),
        amount: None,
    });
    assert!(s.done(361).unwrap_err().contains("provenance"));
}

#[test]
fn lost_create_partially_filled_cancel_recovers_and_releases_only_after_position_repair() {
    let mut s = Session::new("INTX", false).ready();
    s.message(0, "snapshot", "[]");
    s.closing(340, 2.0);
    let r = s.write();
    let client = create_wire(&r).client_order_id;
    s.e.handle(
        s.now,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    assert!(s.done(340).is_err());
    s.message(1, "update", &format!(r#"[{{"order_id":"partial-cancel","client_order_id":"{client}","product_id":"BTC-PERP-INTX","status":"CANCELLED","order_side":"SELL","order_type":"Market","cumulative_quantity":"1","leaves_quantity":"0","avg_price":"100"}}]"#));
    s.closing(341, 2.0);
    assert!(s.done(341).is_err());
    s.command(Command::CancelSymbolOrders {
        command: 342,
        symbol: "BTC-PERP-INTX".into(),
    });
    assert_eq!(s.done(342).unwrap(), "null");
    for _ in 0..2 {
        let r = s.next_position_read();
        s.reply(
            r,
            200,
            &INTX_POSITIONS.replace("\"net_size\":\"2\"", "\"net_size\":\"1\""),
        );
    }
    s.closing(343, 1.0);
    assert_eq!(
        create_wire(&s.write())
            .order_configuration
            .market_market_ioc
            .unwrap()
            .base_size,
        "1"
    );
}

impl Session {
    fn place(&mut self, command: u32, order: PlaceOrderOpts) -> HttpRequest {
        self.command(Command::PlaceOrders {
            command,
            orders: vec![order],
        });
        self.write()
    }
    fn cancel_symbol(&mut self, command: u32) -> Result<String, String> {
        self.command(Command::CancelSymbolOrders {
            command,
            symbol: "BTC-USD".into(),
        });
        self.done(command)
    }
    fn advance(&mut self, ms: f64) {
        let until = self.now + ms;
        while self.now < until {
            for r in self.collect() {
                assert_eq!(r.method, safe_cex_engine::Method::Get);
                self.answer(r);
            }
            self.fire();
        }
        for r in self.collect() {
            self.answer(r);
        }
    }
}

#[test]
fn cli_market_order_with_gtc_is_sent_as_ioc() {
    let mut s = Session::new("DEFAULT", true).ready();
    let mut market = opts("BTC-USD", OrderType::Market, OrderSide::Buy, 1.0);
    market.time_in_force = Some(TimeInForce::GoodTillCancel);
    let r = s.place(300, market);
    assert!(
        create_wire(&r)
            .order_configuration
            .market_market_ioc
            .is_some()
    );
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"market"}}"#,
    );
    assert!(s.done(300).is_ok());
    let order = s.e.store().order("market").unwrap();
    assert_eq!(order.time_in_force, Some(TimeInForce::ImmediateOrCancel));
}

#[test]
fn definite_http_rejection_releases_reservation_and_keeps_coinbase_message() {
    let mut s = Session::new("DEFAULT", true).ready();
    let r = s.place(301, opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0));
    s.reply(
        r,
        400,
        r#"{"error":"INVALID_ARGUMENT","error_details":"","message":"limit price too far from market"}"#,
    );
    let error = s.done(301).unwrap_err();
    assert_eq!(
        error,
        "Coinbase HTTP 400: limit price too far from market (INVALID_ARGUMENT)"
    );
    assert!(s.cancel_symbol(302).is_ok());
}

#[test]
fn ambiguous_placement_blocks_cancel_symbol_until_bounded_reconciliation() {
    let mut s = Session::new("DEFAULT", true).ready();
    let r = s.place(303, opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0));
    s.reply(r, 503, r#"{"error":"UNAVAILABLE","message":"try again"}"#);
    let error = s.done(303).unwrap_err();
    assert!(error.contains("requires reconciliation"), "{error}");
    assert!(error.contains("try again"), "{error}");
    assert!(s.cancel_symbol(304).unwrap_err().contains("unresolved"));
    s.advance(31_000.0);
    assert!(s.cancel_symbol(305).is_ok());
}

#[test]
fn lost_placement_response_is_released_after_bounded_reconciliation() {
    let mut s = Session::new("DEFAULT", true).ready();
    let r = s.place(306, opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0));
    s.e.handle(
        s.now,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    assert!(s.done(306).is_err());
    s.advance(10_000.0);
    assert!(s.cancel_symbol(307).unwrap_err().contains("unresolved"));
    s.advance(21_000.0);
    assert!(s.cancel_symbol(308).is_ok());
}

#[test]
fn rejected_placement_keeps_coinbase_message_and_reason() {
    let mut s = Session::new("DEFAULT", true).ready();
    let r = s.place(309, opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0));
    s.reply(
        r,
        200,
        r#"{"success":false,"failure_reason":"UNKNOWN_FAILURE_REASON","order_id":"","error_response":{"error":"INSUFFICIENT_FUND","message":"Insufficient balance in source account","error_details":"","preview_failure_reason":"PREVIEW_INSUFFICIENT_FUND","new_order_failure_reason":"UNKNOWN_FAILURE_REASON"}}"#,
    );
    assert_eq!(
        s.done(309).unwrap_err(),
        "Coinbase placement rejected: Insufficient balance in source account (INSUFFICIENT_FUND)"
    );
    assert!(s.cancel_symbol(310).is_ok());
}

#[test]
fn protective_stop_and_take_profit_reserve_capacity_independently() {
    let mut s = Session::new("INTX", false).ready();
    let mut stop = opts("BTC-PERP-INTX", OrderType::StopLoss, OrderSide::Sell, 2.0);
    stop.reduce_only = Some(true);
    stop.trigger_price = Some(90.0);
    stop.price = Some(90.0);
    let r = s.place(311, stop.clone());
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"sl"}}"#,
    );
    assert!(s.done(311).is_ok());
    let mut tp = opts("BTC-PERP-INTX", OrderType::Limit, OrderSide::Sell, 2.0);
    tp.reduce_only = Some(true);
    tp.price = Some(120.0);
    let r = s.place(312, tp.clone());
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"tp"}}"#,
    );
    assert!(s.done(312).is_ok());
    tp.amount = 0.5;
    s.command(Command::PlaceOrders {
        command: 313,
        orders: vec![tp],
    });
    assert!(s.done(313).unwrap_err().contains("exceed"));
    stop.amount = 0.5;
    s.command(Command::PlaceOrders {
        command: 314,
        orders: vec![stop],
    });
    assert!(s.done(314).unwrap_err().contains("exceed"));
}

#[test]
fn spot_reduce_only_sell_is_a_plain_sell() {
    let mut s = Session::new("DEFAULT", true).ready();
    let mut sell = opts("BTC-USD", OrderType::Limit, OrderSide::Sell, 1.0);
    sell.reduce_only = Some(true);
    let r = s.place(315, sell);
    assert_eq!(create_wire(&r).side, "SELL");
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"spot-sell"}}"#,
    );
    assert!(s.done(315).is_ok());
    assert!(!s.e.store().order("spot-sell").unwrap().reduce_only);
}

const BRACKET_REST: &str = r#"{"order_id":"bracket-1","client_order_id":"site","product_id":"BTC-PERP-INTX","side":"SELL","status":"OPEN","order_type":"BRACKET","filled_size":"0","average_filled_price":"0","order_configuration":{"trigger_bracket_gtc":{"base_size":"1","limit_price":"120","stop_trigger_price":"90"}}}"#;

#[test]
fn external_bracket_order_neither_blocks_trading_nor_escapes_cancel_symbol() {
    let mut s = Session::new("INTX", false);
    s.open_orders = BRACKET_REST.into();
    let mut s = s.ready();
    assert!(s.e.store().orders().is_empty());
    let site = r#"{"order_id":"bracket-2","client_order_id":"","product_id":"BTC-PERP-INTX","status":"OPEN","order_side":"SELL","order_type":"Bracket","cumulative_quantity":"0","leaves_quantity":"1","limit_price":"120","stop_price":"90","avg_price":"0"}"#;
    let ours = r#"{"order_id":"ours","client_order_id":"mine","product_id":"BTC-PERP-INTX","status":"OPEN","order_side":"BUY","order_type":"Limit","cumulative_quantity":"0","leaves_quantity":"1","limit_price":"100","stop_price":"0","avg_price":"0"}"#;
    s.message(0, "snapshot", &format!("[{site},{ours}]"));
    assert!(s.e.store().order("ours").is_some());
    let r = s.place(
        316,
        opts("BTC-PERP-INTX", OrderType::Limit, OrderSide::Buy, 1.0),
    );
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"placed"}}"#,
    );
    assert!(s.done(316).is_ok());
    s.command(Command::CancelSymbolOrders {
        command: 317,
        symbol: "BTC-PERP-INTX".into(),
    });
    let cancel = s.write();
    #[derive(Deserialize)]
    struct Cancel {
        order_ids: Vec<String>,
    }
    let body: Cancel = serde_json::from_str(cancel.body.as_deref().unwrap()).unwrap();
    assert_eq!(body.order_ids, vec!["bracket-2", "ours", "placed"]);
}

#[test]
fn rest_snapshot_with_external_bracket_still_loads_orders() {
    let mut s = Session::new("INTX", false);
    s.open_orders = format!(
        r#"{BRACKET_REST},{{"order_id":"ours","client_order_id":"mine","product_id":"BTC-PERP-INTX","side":"BUY","status":"OPEN","order_type":"LIMIT","filled_size":"0","average_filled_price":"0","order_configuration":{{"limit_limit_gtc":{{"base_size":"1","limit_price":"100"}}}}}}"#
    );
    let mut s = s.ready();
    assert!(s.e.store().order("ours").is_some());
    s.command(Command::CancelSymbolOrders {
        command: 318,
        symbol: "BTC-PERP-INTX".into(),
    });
    let cancel = s.write();
    assert!(cancel.body.as_deref().unwrap().contains("bracket-1"));
}

fn jwt_subject(request: &HttpRequest) -> String {
    let (_, auth) = request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .unwrap();
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(
            auth.strip_prefix("Bearer ")
                .unwrap()
                .split('.')
                .nth(1)
                .unwrap(),
        )
        .unwrap();
    #[derive(Deserialize)]
    struct Claims {
        sub: String,
    }
    serde_json::from_slice::<Claims>(&claims).unwrap().sub
}

#[test]
fn saved_read_only_key_signs_reads_while_writes_keep_the_trading_key() {
    let read: safe_cex_engine::ReadCredentials = serde_json::from_str(&format!(
        r#"{{"key":"organizations/test/apiKeys/read","secret":"{}"}}"#,
        STANDARD.encode([9; 32])
    ))
    .unwrap();
    let mut s = Session::with_options(
        "DEFAULT",
        Options {
            key: "organizations/test/apiKeys/test".into(),
            secret: STANDARD.encode([7; 32]),
            spot_only: true,
            read_credentials: Some(read),
            ..Default::default()
        },
    )
    .ready();
    let r = s.place(319, opts("BTC-USD", OrderType::Limit, OrderSide::Buy, 1.0));
    assert_eq!(jwt_subject(&r), "organizations/test/apiKeys/test");
    s.reply(
        r,
        200,
        r#"{"success":true,"success_response":{"order_id":"signed"}}"#,
    );
    assert!(s.done(319).is_ok());
    let reads = s
        .requests
        .iter()
        .filter(|r| {
            r.method == safe_cex_engine::Method::Get
                && r.url.contains("/brokerage/")
                && !r.url.contains("/market/")
        })
        .collect::<Vec<_>>();
    assert!(reads.len() > 3);
    for read in reads {
        if read.url.ends_with("/key_permissions") && jwt_subject(read).ends_with("/test") {
            assert_eq!(read.route, Some(safe_cex_engine::RequestRoute::Primary));
            continue;
        }
        assert_eq!(
            jwt_subject(read),
            "organizations/test/apiKeys/read",
            "{}",
            read.url
        );
        assert_eq!(read.route, None);
    }
}

fn read_key_session(read_portfolio: &'static str) -> Session {
    let read: safe_cex_engine::ReadCredentials = serde_json::from_str(&format!(
        r#"{{"key":"organizations/test/apiKeys/read","secret":"{}"}}"#,
        STANDARD.encode([9; 32])
    ))
    .unwrap();
    let mut s = Session::with_options(
        "DEFAULT",
        Options {
            key: "organizations/test/apiKeys/test".into(),
            secret: STANDARD.encode([7; 32]),
            spot_only: true,
            read_credentials: Some(read),
            ..Default::default()
        },
    );
    s.read_portfolio = read_portfolio;
    s.ready()
}

#[test]
fn read_only_key_from_another_portfolio_is_refused_and_reads_use_the_trading_key() {
    let s = read_key_session("other-portfolio");
    let reads = s
        .requests
        .iter()
        .filter(|r| r.url.contains("/brokerage/") && !r.url.contains("/market/"))
        .skip(2)
        .collect::<Vec<_>>();
    assert!(reads.len() > 3);
    for read in reads {
        assert_eq!(
            jwt_subject(read),
            "organizations/test/apiKeys/test",
            "{}",
            read.url
        );
        assert_eq!(read.route, Some(safe_cex_engine::RequestRoute::Primary));
    }
    assert_eq!(s.e.account_id(), Some("portfolio-one"));
}

#[test]
fn unresolved_derivative_placement_waits_for_a_later_position_read() {
    let mut s = Session::new("INTX", false).ready();
    let r = s.place(
        320,
        opts("BTC-PERP-INTX", OrderType::Market, OrderSide::Buy, 1.0),
    );
    s.e.handle(
        s.now,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    assert!(s.done(320).is_err());
    s.failing_positions = true;
    s.advance(31_000.0);
    s.command(Command::CancelSymbolOrders {
        command: 321,
        symbol: "BTC-PERP-INTX".into(),
    });
    assert!(s.done(321).unwrap_err().contains("unresolved"));
    s.failing_positions = false;
    s.advance(6_000.0);
    s.command(Command::CancelSymbolOrders {
        command: 322,
        symbol: "BTC-PERP-INTX".into(),
    });
    assert!(s.done(322).is_ok());
}
