//! Product examples and native base-size units from api-docs.grvt.io market_data_api/trading_api.
use alloy_dyn_abi::TypedData;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, Response, SocketChannel, SocketId,
    Venue,
};
use safe_cex_venue_grvt::{Grvt, Options};
use serde_json::{Value, json};
use std::collections::VecDeque;
const NOW: f64 = 1_700_000_000_000.;
const KEY: &str = "0x0101010101010101010101010101010101010101010101010101010101010101";
const PRODUCTS: [(&str, &str, &str); 7] = [
    ("BTC_USDT_Perp", "PERPETUAL", "0x030501"),
    ("BTC_USDT_Fut_20Oct23", "FUTURE", "0x030502"),
    ("BTC_USDT_Call_20Oct23_2800", "CALL", "0x030503"),
    ("BTC_USDT_Put_20Oct23_2800", "PUT", "0x030504"),
    ("BTC_USDT_Fut_20Nov23", "FUTURE", "0x030505"),
    ("BTC_USDT_Call_20Oct23_2900", "CALL", "0x030506"),
    ("BTC_USDT_Put_20Nov23_2800", "PUT", "0x030507"),
];
fn catalog() -> String {
    json!({"r":PRODUCTS.iter().map(|(id, kind, hash)| json!({
        "i":id,"ih":hash,"b":"BTC","q":"USDT","k":kind,"ts":"0.1","ms":"0.001","mp":"1000","bd":3,"qd":6
    })).collect::<Vec<_>>()}).to_string()
}
fn native_order(instrument: &str, client: &str) -> Value {
    json!({"oi":format!("0x{client}"),"sa":"777","im":false,"ti":"GOOD_TILL_TIME","po":false,"ro":false,
        "l":[{"i":instrument,"s":"1","lp":"100","ib":true}],
        "m":{"co":client,"ct":"1700000000000000000"},"s1":{"s":"OPEN","ts":["0"],"ut":"1700000000000000000"}})
}
fn native_fill(instrument: &str, id: &str) -> Value {
    json!({"et":"1700000000000000000","sa":"777","i":instrument,"ti":id,"co":"101","oi":"0x101","ib":true,"it":false,"s":"0.2","p":"100","f":"0.01","rp":"3","fc":"USDT"})
}
struct Harness {
    e: Engine<Grvt>,
    public: SocketId,
}
impl Harness {
    fn new() -> Self {
        let mut e = Engine::new(Grvt::new(Options {
            key: Some("fixture-key".into()),
            secret: Some(KEY.into()),
            trading_account_id: Some("777".into()),
            ..Options::default()
        }));
        e.start(NOW);
        let mut pending: VecDeque<_> = e.drain_actions().collect();
        let mut public = None;
        while let Some(a) = pending.pop_front() {
            match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => public = Some(socket),
                Action::Http(r) => {
                    let (body, headers) = if r.url.ends_with("all_instruments") {
                        (catalog(), "")
                    } else if r.url.ends_with("login") {
                        (
                            r#"{"status":"success"}"#.into(),
                            "X-Ts-Grvt-Session: session\nX-Grvt-Account-Id: account",
                        )
                    } else if r.url.ends_with("aggregated_account_summary") {
                        (
                            r#"{"r":{"te":"1000","ts2":"1000","tu":"0","sas":[{"si":"777"}]}}"#
                                .into(),
                            "",
                        )
                    } else if r.url.ends_with("get_authorized_builders") {
                        (r#"{"results":[{"builder_account_id":"0xee8f3559f298fffa79b8253437969d15b5dccff3","max_futures_fee_rate":"0.03"}]}"#.into(), "")
                    } else if r.url.ends_with("positions") {
                        (json!({"r":PRODUCTS.iter().map(|(id,_,_)|json!({"i":id,"s":"2","ep":"90","n":"200","up":"20","l":"10","lp":"10"})).collect::<Vec<_>>()} ).to_string(), "")
                    } else if r.url.ends_with("open_orders") {
                        (json!({"r":PRODUCTS.iter().enumerate().map(|(index,(id,_,_))|native_order(id,&(101+index).to_string())).collect::<Vec<_>>()} ).to_string(), "")
                    } else {
                        (r#"{"r":[]}"#.into(), "")
                    };
                    e.handle(
                        NOW + 1.,
                        Event::HttpResponse {
                            request: r.id,
                            response: Response {
                                status: 200,
                                headers,
                                body: &body,
                            },
                        },
                    );
                    pending.extend(e.drain_actions());
                }
                _ => {}
            }
        }
        let public = public.expect("public socket");
        e.handle(NOW + 2., Event::WsOpened { socket: public });
        e.drain_actions();
        Self { e, public }
    }
    fn command(&mut self, command: Command) -> Vec<Action> {
        self.e.handle(NOW + 3., Event::Command(command));
        self.e.drain_actions().collect()
    }
    fn reply(&mut self, request: HttpRequest, body: &str) -> Vec<Action> {
        self.e.handle(
            NOW + 4.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.e.drain_actions().collect()
    }
    fn query(&mut self, query: Query) -> Vec<Action> {
        self.command(Command::Query { command: 8, query })
    }
}
fn take(actions: Vec<Action>, path: &str) -> HttpRequest {
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with(path) => Some(r),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {path}"))
}
fn done(actions: &[Action]) -> &Result<String, String> {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(result),
            _ => None,
        })
        .expect("command completion")
}
fn opts(symbol: &str) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: symbol.into(),
        implied_volatility: None,
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.2,
        price: Some(100.),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: Some(false),
        time_in_force: Some(TimeInForce::GoodTillCancel),
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: Some("555".into()),
        close: None,
        stp_mode: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
    }
}
#[test]
fn catalogs_public_data_and_private_state_retain_all_documented_orderbook_kinds() {
    let mut h = Harness::new();
    assert_eq!(h.e.store().markets().len(), 7);
    for (id, kind, _) in PRODUCTS {
        let symbol = id.to_ascii_uppercase();
        let m = h.e.store().market(&symbol).unwrap();
        assert_eq!(m.id, id);
        assert_eq!(m.is_perpetual, Some(kind == "PERPETUAL"));
        assert_eq!(m.is_expiring, (kind != "PERPETUAL").then_some(true));
        let p =
            h.e.store()
                .positions()
                .iter()
                .find(|p| p.symbol == symbol)
                .expect("native position");
        assert_eq!((p.contracts, p.entry_price, p.notional), (2., 90., 200.));
        assert!(h.e.store().orders().iter().any(|o| o.symbol == symbol));
        h.command(Command::SubscribeOrderBook {
            symbol: symbol.clone(),
        });
        let ticker=json!({"s":"v1.ticker.d","s1":id,"sn":"0","f":{"i":id,"lp":"100","op":"80","bv":"2","sv":"3"}}).to_string();
        h.e.handle(
            NOW + 5.,
            Event::WsMessage {
                socket: h.public,
                text: &ticker,
            },
        );
        assert_eq!(h.e.store().ticker(&symbol).unwrap().last, 100.);
        let book=json!({"s":"v1.book.s","s1":id,"sn":"0","f":{"i":id,"b":[{"p":"99","s":"2"}],"a":[{"p":"101","s":"1"}]}}).to_string();
        h.e.handle(
            NOW + 6.,
            Event::WsMessage {
                socket: h.public,
                text: &book,
            },
        );
        assert_eq!(h.e.venue().book(&symbol).unwrap().bids(), [(99., 2.)]);
        h.e.drain_actions();
    }
}
#[test]
fn every_product_signs_its_native_hash_base_units_and_exact_id() {
    for (id, _, hash) in PRODUCTS {
        let mut h = Harness::new();
        let actions = h.command(Command::PlaceOrders {
            command: 7,
            orders: vec![opts(&id.to_ascii_uppercase())],
        });
        let r = take(actions, "create_order");
        let b: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        let o = &b["o"];
        let s = &o["s"];
        assert_eq!(o["l"][0]["i"], id);
        assert_eq!(o["l"][0]["s"], "0.2");
        assert_eq!(o["m"]["co"], "555");
        let typed = json!({
            "domain":{"name":"GRVT Exchange","version":"0","chainId":325},"primaryType":"OrderWithBuilderFee",
            "types":{
                "EIP712Domain":[{"name":"name","type":"string"},{"name":"version","type":"string"},{"name":"chainId","type":"uint256"}],
                "OrderWithBuilderFee":[{"name":"subAccountID","type":"uint64"},{"name":"isMarket","type":"bool"},{"name":"timeInForce","type":"uint8"},{"name":"postOnly","type":"bool"},{"name":"reduceOnly","type":"bool"},{"name":"legs","type":"OrderLeg[]"},{"name":"builder","type":"address"},{"name":"builderFee","type":"uint32"},{"name":"nonce","type":"uint32"},{"name":"expiration","type":"int64"}],
                "OrderLeg":[{"name":"assetID","type":"uint256"},{"name":"contractSize","type":"uint64"},{"name":"limitPrice","type":"uint64"},{"name":"isBuyingContract","type":"bool"}]},
            "message":{"subAccountID":"777","isMarket":false,"timeInForce":1,"postOnly":false,"reduceOnly":false,
                "legs":[{"assetID":hash,"contractSize":"200","limitPrice":"100000000000","isBuyingContract":true}],
                "builder":o["b"],"builderFee":300,"nonce":s["n"],"expiration":s["e"]}});
        let typed: TypedData = serde_json::from_value(typed).unwrap();
        let signer: PrivateKeySigner = KEY.parse().unwrap();
        let expected = signer
            .sign_dynamic_typed_data_sync(&typed)
            .unwrap()
            .to_string();
        let actual = format!(
            "0x{}{}{:02x}",
            &s["r"].as_str().unwrap()[2..],
            &s["s1"].as_str().unwrap()[2..],
            s["v"].as_u64().unwrap()
        );
        assert_eq!(actual, expected, "{id}");
        let actions = h.reply(r, r#"{"r":{"oi":"0x555","m":{"co":"555"}}}"#);
        assert!(done(&actions).is_ok());
    }
}
#[test]
fn scoped_fill_history_uses_kind_and_filters_sibling_instruments() {
    for (id, kind, _) in PRODUCTS {
        let mut h = Harness::new();
        let a = h.query(Query::Fills {
            symbol: Some(id.to_ascii_uppercase()),
            order_id: None,
            from_ms: None,
            to_ms: Some(NOW),
            limit: None,
        });
        let r = take(a, "fill_history");
        let b: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(b["k"], json!([kind]));
        let rows = PRODUCTS
            .iter()
            .enumerate()
            .map(|(n, (i, _, _))| native_fill(i, &n.to_string()))
            .collect::<Vec<_>>();
        let a = h.reply(r, &json!({"r":rows,"n":""}).to_string());
        let rows: Value = serde_json::from_str(done(&a).as_ref().unwrap()).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["symbol"], id.to_ascii_uppercase());
        assert_eq!(rows[0]["qty"], 0.2);
    }
}
#[test]
fn dated_symbol_cancellation_reads_native_orders_and_never_cancels_siblings() {
    for (id, kind, _) in &PRODUCTS[1..] {
        let mut h = Harness::new();
        let a = h.command(Command::CancelSymbolOrders {
            command: 7,
            symbol: id.to_ascii_uppercase(),
        });
        let r = take(a, "open_orders");
        let b: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(b["k"], json!([kind]));
        // An authoritative native order absent from the local store must also cancel.
        // Sibling expiries/strikes share kind/base/quote and must survive.
        let sibling = PRODUCTS
            .iter()
            .find(|(i, k, _)| i != id && k == kind)
            .unwrap()
            .0;
        let a = h.reply(
            r,
            &json!({"r":[native_order(sibling,"999"),native_order(id,"888")]}).to_string(),
        );
        assert!(
            !a.iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("cancel_all_orders"))),
            "{id}"
        );
        let r = take(a, "cancel_order");
        let b: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        assert!(b.get("co").is_none(), "native identity takes precedence");
        assert_eq!(b["oi"], "0x888");
        let a = h.reply(r, r#"{"r":{"a":true}}"#);
        assert!(done(&a).is_ok());
        assert!(
            !a.iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("cancel_order")))
        );
        assert_eq!(h.e.store().order("101").unwrap().status, OrderStatus::Open);
    }
}
#[test]
fn malformed_scoped_cancellation_never_writes_a_partial_list() {
    let mut h = Harness::new();
    let id = PRODUCTS[2].0;
    let a = h.command(Command::CancelSymbolOrders {
        command: 7,
        symbol: id.to_ascii_uppercase(),
    });
    let r = take(a, "open_orders");
    let a = h.reply(
        r,
        &json!({"r":[native_order(id,"888"),{"broken":true}]}).to_string(),
    );
    assert!(done(&a).is_err());
    assert!(!a.iter().any(|a|matches!(a,Action::Http(r) if r.url.ends_with("cancel_order")||r.url.ends_with("cancel_all_orders"))));
}
#[test]
fn nonperpetual_income_omits_funding_but_still_reads_commissions_and_pnl() {
    let mut h = Harness::new();
    let symbol = Some(PRODUCTS[2].0.to_ascii_uppercase());
    let a = h.query(Query::Income {
        symbol: symbol.clone(),
        kind: Some("FUNDING_FEE".into()),
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    assert_eq!(done(&a).as_ref().unwrap(), "[]");
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    let a = h.query(Query::Income {
        symbol,
        kind: None,
        from_ms: None,
        to_ms: Some(NOW),
        limit: None,
    });
    let r = take(a, "fill_history");
    let a = h.reply(
        r,
        &json!({"r":[native_fill(PRODUCTS[2].0,"1")],"n":""}).to_string(),
    );
    let rows: Value = serde_json::from_str(done(&a).as_ref().unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
}

#[test]
fn unknown_native_product_kind_fails_the_whole_catalog_without_silent_retirement() {
    let mut e = Engine::new(Grvt::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(NOW);
    let request = take(e.drain_actions().collect(), "all_instruments");
    let body = catalog().replace("\"CALL\"", "\"UNRECOGNIZED\"");
    e.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    assert!(!e.store().loaded().markets);
    assert!(e.store().markets().is_empty());
    assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(Notice::Log {message,..}) if message=="Unsupported GRVT instrument kind")));
}

#[test]
fn unknown_option_terms_remain_absent_and_new_foreign_pricing_rejects_before_writes() {
    let mut h = Harness::new();
    let products = h.e.store().derivative_products().expect("typed products");
    assert_eq!(products.len(), 6);
    for p in products {
        assert_eq!(p.underlying, "BTC");
        assert!(
            p.expiry.is_none() && p.strike.is_none() && p.style.is_none() && p.settlement.is_none()
        );
        assert_eq!(
            h.e.store().markets()[p.market as usize].is_perpetual,
            Some(false)
        );
    }
    let mut invalid = opts(&PRODUCTS[2].0.to_ascii_uppercase());
    invalid.implied_volatility = Some(0.5);
    let a = h.command(Command::PlaceOrders {
        command: 9,
        orders: vec![opts(&PRODUCTS[1].0.to_ascii_uppercase()), invalid],
    });
    assert!(
        done(&a)
            .as_ref()
            .unwrap_err()
            .contains("Options are not enabled")
    );
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("create_order")))
    );
}
