//! Official Spot v3 examples; see fixtures/spot/README.md for adaptations.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Outcome, Query, Response, SocketId,
};
use safe_cex_venue_bingx::{Options, spot::BingxSpot};
use serde_json::{Value, json};
#[path = "spot/parity.rs"]
mod parity;

const NOW: f64 = 1_702_720_824_000.;
const MARKETS: &str = include_str!("fixtures/spot/markets.json");
const TICKERS: &str = include_str!("fixtures/spot/tickers.json");
const BALANCES: &str = include_str!("fixtures/spot/balances.json");
const ORDERS: &str = include_str!("fixtures/spot/orders.json");
const FILLS: &str = include_str!("fixtures/spot/fills.json");
const CANDLES: &str = include_str!("fixtures/spot/candles.json");
fn engine(private: bool) -> Engine<BingxSpot> {
    let (signer, ids) = LocalSigner::from_setup(LocalSetup {
        primary: private.then(|| LocalIdentity {
            key: Secret::new("spot-fixture-key".into()),
            secret: Secret::new("spot-fixture-secret".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let mut e = safe_cex_engine::test_support::seeded_engine(BingxSpot::new(Options {
        credentials: ids.primary,
        read_credentials: None,
        public_data_only: !private,
    }));
    e.set_signer(Some(std::sync::Arc::new(signer))).unwrap();
    e
}
fn collect(e: &mut Engine<BingxSpot>) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, NOW)
}
fn reply(e: &mut Engine<BingxSpot>, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        NOW,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn request(actions: &[Action], suffix: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.split('?').next().unwrap().ends_with(suffix) => {
                Some(r.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing request {suffix}"))
}
fn seed(private: bool) -> (Engine<BingxSpot>, Vec<(SocketId, String)>) {
    seed_with_fixtures(private, MARKETS, TICKERS)
}
fn seed_with_fixtures(
    private: bool,
    markets: &str,
    tickers: &str,
) -> (Engine<BingxSpot>, Vec<(SocketId, String)>) {
    let mut e = engine(private);
    e.start(NOW);
    let first = collect(&mut e);
    let r = request(&first, "/common/symbols");
    reply(&mut e, &r, 200, markets);
    let mut sockets = vec![];
    for _ in 0..8 {
        let actions = collect(&mut e);
        let mut count = 0;
        for a in actions {
            match a {
                Action::WsConnect { socket, url, .. } => sockets.push((socket, url)),
                Action::Http(r) => {
                    count += 1;
                    let p = r.url.split('?').next().unwrap();
                    let body = if p.ends_with("/quote/ticker") {
                        tickers
                    } else if p.ends_with("/account/balance") {
                        BALANCES
                    } else if p.ends_with("/trade/openOrders") {
                        r#"{"code":0,"data":{"orders":[]}}"#
                    } else if p.ends_with("/userDataStream") {
                        r#"{"listenKey":"fixture-session"}"#
                    } else {
                        panic!("unexpected startup route {p}")
                    };
                    reply(&mut e, &r, 200, body);
                }
                _ => {}
            }
        }
        if count == 0 {
            break;
        }
    }
    (e, sockets)
}
fn place(e: &mut Engine<BingxSpot>, kind: &str, side: &str) -> Vec<Action> {
    let orders = serde_json::from_value(json!([{"symbol":"BTCUSDT","type":kind,"side":side,"amount":0.002,"price":41000,"clientOrderId":"spot_1"}])).unwrap();
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 41,
            orders,
        }),
    );
    collect(e)
}
fn done(actions: &[Action], id: u32) -> Value {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                Some(serde_json::from_str(result).unwrap())
            }
            _ => None,
        })
        .expect("command completed")
}
#[test]
fn startup_maps_spot_markets_tickers_balances_and_authoritative_empty_positions() {
    let (e, _) = seed(true);
    let m = &e.store().markets()[0];
    assert_eq!(m.is_spot, Some(true));
    assert_eq!(m.id, "BTC-USDT");
    assert_eq!(m.precision.amount, 0.00001);
    let t = &e.store().tickers()[0];
    assert_eq!(
        (t.last, t.bid, t.ask, t.percentage),
        (42215.15, 42214.65, 42215.55, 2.91)
    );
    assert!(e.store().loaded().positions);
    assert!(e.store().positions().is_empty());
    assert!(e.store().loaded().balance);
    assert!((e.store().balance().total - 567017.3795652849).abs() < 0.000001);
}
#[test]
fn spot_hosts_and_explicit_private_subscriptions_are_separate_from_swap() {
    let (mut e, sockets) = seed(true);
    assert!(
        sockets
            .iter()
            .any(|(_, u)| u == "wss://open-api-ws.bingx.com/market")
    );
    let s = sockets
        .iter()
        .find(|(_, u)| u.contains("listenKey="))
        .expect("private spot socket")
        .0;
    e.handle(NOW, Event::WsOpened { socket: s });
    let sent: Vec<_> = collect(&mut e)
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(sent.iter().any(|t| t.contains("spot.executionReport")));
}
#[test]
fn limit_placement_and_cancel_preserve_native_ids_and_client_ids() {
    let (mut e, _) = seed(true);
    let actions = place(&mut e, "limit", "sell");
    let r = request(&actions, "/spot/v1/trade/order");
    assert_eq!(r.method, Method::Post);
    let b: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_eq!(b["quantity"], 0.002);
    assert_eq!(b["newClientOrderId"], "spot_1");
    assert!(b.get("positionSide").is_none());
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":0,"data":{"orderId":9007199254740993,"symbol":"BTC-USDT","side":"SELL","type":"LIMIT","status":"PENDING"}}"#,
    );
    assert_eq!(done(&collect(&mut e), 41), json!(["9007199254740993"]));
    e.handle(
        NOW,
        Event::Command(Command::CancelOrdersWithSymbols {
            command: 42,
            orders: serde_json::from_value(json!([{"id":"9007199254740993","symbol":"BTCUSDT"}]))
                .unwrap(),
        }),
    );
    let r = request(&collect(&mut e), "/spot/v1/trade/cancel");
    let b: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_eq!(b["orderId"], "9007199254740993");
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":0,"data":{"orderId":9007199254740993,"symbol":"BTC-USDT","status":"CANCELED"}}"#,
    );
    assert_eq!(done(&collect(&mut e), 42), json!([]));
}
#[test]
fn market_buy_is_quote_sized_while_sell_is_base_sized() {
    for (side, field, amount) in [("buy", "quoteOrderQty", 82.), ("sell", "quantity", 0.002)] {
        let (mut e, _) = seed(true);
        let a = place(&mut e, "market", side);
        let r = request(&a, "/spot/v1/trade/order");
        let b: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(b[field], amount);
        assert!(b.get("price").is_none());
    }
}
#[test]
fn released_ambiguous_writes_are_unknown_and_never_retried() {
    for status in [408, 429, 500, 502, 503] {
        let (mut e, _) = seed(true);
        let a = place(&mut e, "limit", "buy");
        let r = request(&a, "/spot/v1/trade/order");
        reply(&mut e, &r, status, r#"{"code":100400,"msg":"bad request"}"#);
        let a = collect(&mut e);
        assert!(a.iter().any(|a| matches!(a,Action::Notice(Notice::CommandFailed{command:41,error}) if error.outcome==Some(Outcome::Unknown))));
        assert!(!a.iter().any(|a| matches!(a,Action::Http(r) if r.method==Method::Post && r.url.ends_with("/trade/order"))));
    }
}
#[test]
fn spot_open_orders_and_fills_use_their_own_envelopes() {
    let (mut e, _) = seed(true);
    e.handle(
        NOW,
        Event::Command(Command::Venue {
            command: 44,
            action: safe_cex_engine::VenueAction::RefreshOrders { symbols: None },
        }),
    );
    let r = request(&collect(&mut e), "/trade/openOrders");
    reply(&mut e, &r, 200, &ORDERS.replace("BNB-USDC", "BTC-USDT"));
    collect(&mut e);
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(
        e.store().orders()[0].side,
        safe_cex_engine::model::OrderSide::Sell
    );
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 45,
            query: Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: Some(10),
            },
        }),
    );
    let r = request(&collect(&mut e), "/trade/myTrades");
    reply(&mut e, &r, 200, FILLS);
    let rows = done(&collect(&mut e), 45);
    assert_eq!(rows[0]["id"], "36767057");
    assert_eq!(rows[0]["qty"], 0.1430254);
}
#[test]
fn ohlcv_and_full_depth_snapshot_are_spot_specific() {
    let (mut e, sockets) = seed(false);
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 46,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2),
            from_ms: None,
            to_ms: None,
        }),
    );
    let r = request(&collect(&mut e), "/spot/v2/quote/klines");
    reply(&mut e, &r, 200, CANDLES);
    let c = done(&collect(&mut e), 46);
    assert_eq!(c[0]["timestamp"], 1702720560.);
    e.handle(
        NOW,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    collect(&mut e);
    let s = sockets.iter().find(|(_, u)| !u.contains('?')).unwrap().0;
    e.handle(NOW, Event::WsOpened { socket: s });
    collect(&mut e);
    e.handle(NOW,Event::WsMessage{socket:s,text:r#"{"code":0,"dataType":"BTC-USDT@depth50","data":{"bids":[["42214.65","0.8962"]],"asks":[["42215.55","2.20125"]],"T":1702720823624}}"#});
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(42214.65, 0.8962)]);
    e.handle(NOW,Event::WsMessage{socket:s,text:r#"{"code":0,"dataType":"BTC-USDT@depth50","data":{"bids":[],"asks":[],"T":1702720823625}}"#});
    assert!(e.book("BTCUSDT").unwrap().bids().is_empty());
}

#[test]
fn native_spot_signing_matches_an_independent_hmac_golden() {
    let (mut e, _) = seed(true);
    let r = request(&place(&mut e, "limit", "sell"), "/spot/v1/trade/order");
    let b: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_eq!(b["timestamp"], NOW);
    assert_eq!(
        b["signature"],
        "702048d7a52c83bd2d60d2ead90eb204dcbd6b1ab658bae77d1108e94760d2fd"
    );
    assert!(
        r.headers
            .contains(&("X-BX-APIKEY", "spot-fixture-key".into()))
    );
}
#[test]
fn private_execution_reports_publish_each_fill_once_and_keep_zero_fill_orders() {
    let (mut e, sockets) = seed(true);
    let s = sockets.iter().find(|(_, u)| u.contains('?')).unwrap().0;
    e.handle(NOW, Event::WsOpened { socket: s });
    collect(&mut e);
    e.take_market_frame(NOW);
    let report = |qty: f64, last: f64, status: &str, trade: u32| {
        json!({"code":0,"dataType":"spot.executionReport","data":{"s":"BTC-USDT","i":9007199254740993u64,"S":"BUY","o":"LIMIT","X":status,"q":"0.002","p":"42215.15","z":qty,"l":last,"L":"42215.15","t":trade,"T":NOW,"n":"-0.000001"}}).to_string()
    };
    let zero = report(0., 0., "NEW", 0);
    e.handle(
        NOW,
        Event::WsMessage {
            socket: s,
            text: &zero,
        },
    );
    assert_eq!(e.store().orders()[0].filled, 0.);
    for (qty, status, trade) in [(0.001, "PARTIALLY_FILLED", 77), (0.002, "FILLED", 78)] {
        let text = report(qty, 0.001, status, trade);
        e.handle(
            NOW,
            Event::WsMessage {
                socket: s,
                text: &text,
            },
        );
        let frame = e.take_market_frame(NOW).unwrap();
        assert!(
            safe_cex_engine::wire::sections(frame)
                .any(|(kind, _)| kind == safe_cex_engine::wire::section::FILLS)
        );
        e.handle(
            NOW,
            Event::WsMessage {
                socket: s,
                text: &text,
            },
        );
        if let Some(frame) = e.take_market_frame(NOW) {
            assert!(
                !safe_cex_engine::wire::sections(frame)
                    .any(|(kind, _)| kind == safe_cex_engine::wire::section::FILLS)
            );
        }
    }
    e.handle(
        NOW,
        Event::WsMessage {
            socket: s,
            text: &zero,
        },
    );
    assert_eq!(
        e.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Closed
    );
}
#[test]
fn malformed_and_full_open_order_snapshots_do_not_clear_existing_orders() {
    for body in [
        ORDERS.replace("BNB-USDC", "BTC-USDT"),
        r#"{"code":0,"data":{}}"#.into(),
        r#"{"code":100500,"msg":"busy","data":{"orders":[]}}"#.into(),
    ] {
        let (mut e, _) = seed(true);
        e.store_mut().upsert_order(
            safe_cex_engine::model::CancelOrderRef {
                id: "123".into(),
                symbol: "BTCUSDT".into(),
            }
            .limit_stand_in(),
        );
        e.handle(
            NOW,
            Event::Command(Command::Venue {
                command: 47,
                action: safe_cex_engine::VenueAction::RefreshOrders { symbols: None },
            }),
        );
        let r = request(&collect(&mut e), "/trade/openOrders");
        reply(&mut e, &r, 200, &body);
        collect(&mut e);
        assert_eq!(e.store().orders().len(), 1);
    }
}

#[test]
fn caller_id_lookup_uses_spot_primary_query_and_preserves_the_native_id() {
    let (mut e, _) = seed(true);
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 51,
            query: Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "spot_1".into(),
            },
        }),
    );
    let r = request(&collect(&mut e), "/spot/v1/trade/query");
    assert!(r.url.contains("clientOrderID=spot_1"));
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":0,"data":{"symbol":"BTC-USDT","orderId":9007199254740993,"clientOrderID":"spot_1","price":"41000","origQty":"0.002","executedQty":"0","status":"PENDING","type":"LIMIT","side":"BUY","time":1702720824000}}"#,
    );
    let value = done(&collect(&mut e), 51);
    assert_eq!(value["found"], true);
    assert_eq!(value["order"]["id"], "9007199254740993");
}
#[test]
fn zero_fill_reports_without_a_trade_id_still_establish_an_open_order() {
    let (mut e, sockets) = seed(true);
    let socket = sockets.iter().find(|(_, u)| u.contains('?')).unwrap().0;
    e.handle(NOW, Event::WsOpened { socket });
    collect(&mut e);
    e.handle(NOW,Event::WsMessage{socket,text:r#"{"code":0,"dataType":"spot.executionReport","data":{"s":"BTC-USDT","i":9007199254740993,"S":"BUY","o":"LIMIT","X":"NEW","q":"0.002","p":"41000","z":"0","l":"0","L":"0","T":1702720824000}}"#});
    assert!(e.store().order("9007199254740993").is_some());
}

#[test]
fn spot_followup_execution_reports_preserve_documented_client_id() {
    for field in ["c", "C"] {
        let (mut e, sockets) = seed(true);
        let socket = sockets.iter().find(|(_, url)| url.contains('?')).unwrap().0;
        e.handle(NOW, Event::WsOpened { socket });
        collect(&mut e);
        for (filled, last, status, trade) in [
            ("0", "0", "NEW", 0),
            ("0.001", "0.001", "PARTIALLY_FILLED", 77),
        ] {
            let mut report = json!({"code":0,"dataType":"spot.executionReport","data":{
                "s":"BTC-USDT","i":9007199254740993u64,"S":"BUY","o":"LIMIT",
                "X":status,"q":"0.002","p":"41000","z":filled,"l":last,
                "L":"41000","t":trade,"T":NOW
            }});
            report["data"][field] = json!("caller_spot_1");
            e.handle(
                NOW,
                Event::WsMessage {
                    socket,
                    text: &report.to_string(),
                },
            );
            let order = e.store().order("9007199254740993").unwrap();
            assert_eq!(order.filled, filled.parse::<f64>().unwrap());
            assert_eq!(
                order.client_order_id.as_deref(),
                Some("caller_spot_1"),
                "field {field}"
            );
        }
    }
}
#[test]
fn public_ticker_and_candle_frames_follow_the_spot_topics() {
    let (mut e, sockets) = seed(false);
    let socket = sockets.iter().find(|(_, u)| !u.contains('?')).unwrap().0;
    e.handle(NOW, Event::WsOpened { socket });
    collect(&mut e);
    e.handle(
        NOW,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    assert!(
        collect(&mut e)
            .iter()
            .any(|a| matches!(a,Action::WsSend{text,..} if text.contains("BTC-USDT@kline_1min")))
    );
    e.handle(NOW,Event::WsMessage{socket,text:r#"{"code":0,"dataType":"BTC-USDT@ticker","data":{"s":"BTC-USDT","p":"-262.0","P":"-0.21","c":"121621.7","v":"17030.6463","q":"210028.10","A":"121621.7","B":"121621.5"}}"#});
    assert_eq!(e.store().tickers()[0].last, 121621.7);
    e.take_frame(NOW);
    e.handle(NOW,Event::WsMessage{socket,text:r#"{"code":0,"dataType":"BTC-USDT@kline_1min","data":{"s":"BTC-USDT","K":{"t":1760066280000,"o":"121137.946","h":"121137.946","l":"121137.93299999999","c":"121137.93299999999","v":"0.025847"}}}"#});
    let frame = e.take_market_frame(NOW).unwrap();
    assert!(
        safe_cex_engine::wire::sections(frame)
            .any(|(kind, _)| kind == safe_cex_engine::wire::section::CANDLES)
    );
}

#[test]
fn late_rest_seed_cannot_overwrite_a_proven_live_spot_ticker() {
    let mut e = engine(false);
    e.start(NOW);
    let r = request(&collect(&mut e), "/common/symbols");
    reply(&mut e, &r, 200, MARKETS);
    let actions = collect(&mut e);
    let rest = request(&actions, "/quote/ticker");
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    e.handle(NOW, Event::WsOpened { socket });
    collect(&mut e);
    e.handle(NOW,Event::WsMessage{socket,text:r#"{"code":0,"dataType":"BTC-USDT@ticker","data":{"s":"BTC-USDT","P":"-0.21","c":"121621.7","v":"17030.6463","q":"210028.10","A":"121621.7","B":"121621.5"}}"#});
    reply(&mut e, &rest, 200, TICKERS);
    collect(&mut e);
    assert_eq!(e.store().tickers()[0].last, 121621.7);
}
#[test]
fn overlapping_balance_reads_cannot_roll_back_the_newer_snapshot() {
    let mut e = engine(true);
    e.start(NOW);
    let r = request(&collect(&mut e), "/common/symbols");
    reply(&mut e, &r, 200, MARKETS);
    let a = collect(&mut e);
    let old = request(&a, "/account/balance");
    let key = request(&a, "/userDataStream");
    reply(&mut e, &key, 200, r#"{"listenKey":"fixture-session"}"#);
    let a = collect(&mut e);
    let socket = a
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.contains('?') => Some(*socket),
            _ => None,
        })
        .unwrap();
    e.handle(NOW, Event::WsOpened { socket });
    let new = request(&collect(&mut e), "/account/balance");
    reply(
        &mut e,
        &new,
        200,
        r#"{"code":0,"data":{"balances":[{"asset":"USDT","free":"100","locked":"0"}]}}"#,
    );
    collect(&mut e);
    reply(&mut e, &old, 200, BALANCES);
    collect(&mut e);
    assert_eq!(e.store().balance().total, 100.);
}

#[test]
fn refusal_with_a_native_order_claim_is_unknown() {
    let (mut e, _) = seed(true);
    let r = request(&place(&mut e, "limit", "buy"), "/trade/order");
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":100001,"msg":"signature refused","data":{"orderId":9007199254740993,"symbol":"BTC-USDT","status":"PENDING","type":"LIMIT","side":"BUY"}}"#,
    );
    assert!(collect(&mut e).iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:41,error})if error.outcome==Some(Outcome::Unknown))));
}
#[test]
fn rest_depth_sorts_the_documented_descending_asks() {
    let (mut e, _) = seed(false);
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: 55,
            symbol: "BTCUSDT".into(),
            limit: Some(5),
        }),
    );
    let r = request(&collect(&mut e), "/spot/v1/market/depth");
    reply(&mut e, &r, 200, include_str!("fixtures/spot/depth.json"));
    let value = done(&collect(&mut e), 55);
    assert_eq!(value["asks"][0][0], 42183.17);
}
#[test]
fn symbol_cancel_covers_venue_orders_missing_from_the_store() {
    let (mut e, _) = seed(true);
    assert!(e.store().orders().is_empty());
    e.handle(
        NOW,
        Event::Command(Command::CancelSymbolOrders {
            command: 56,
            symbol: "BTCUSDT".into(),
        }),
    );
    let r = request(&collect(&mut e), "/spot/v1/trade/cancelOpenOrders");
    let b: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_eq!(b["symbol"], "BTC-USDT");
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":0,"data":{"orders":[{"symbol":"BTC-USDT","orderId":9007199254740993,"status":"CANCELED"}]}}"#,
    );
    assert_eq!(done(&collect(&mut e), 56), json!([]));
}

#[test]
fn balance_key_refusal_is_typed_authentication_evidence() {
    let mut e = engine(true);
    e.start(NOW);
    let r = request(&collect(&mut e), "/common/symbols");
    reply(&mut e, &r, 200, MARKETS);
    let r = request(&collect(&mut e), "/account/balance");
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":100004,"msg":"permission denied"}"#,
    );
    assert!(collect(&mut e).iter().any(|a| matches!(
        a,
        Action::Notice(Notice::AccountAuth {
            scope: safe_cex_engine::AccountReadScope::Balance,
            outcome: safe_cex_engine::AuthOutcome::Rejected,
            ..
        })
    )));
}
#[test]
fn capped_history_is_explicitly_incomplete() {
    let (mut e, _) = seed(true);
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 59,
            query: Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(1704961920000.),
                to_ms: Some(1704961930000.),
                limit: Some(2),
            },
        }),
    );
    let r = request(&collect(&mut e), "/trade/myTrades");
    let mut body: Value = serde_json::from_str(FILLS).unwrap();
    let mut second = body["data"]["fills"][0].clone();
    second["id"] = json!(36767058);
    body["data"]["fills"].as_array_mut().unwrap().push(second);
    reply(&mut e, &r, 200, &body.to_string());
    let a = collect(&mut e);
    assert_eq!(done(&a, 59).as_array().unwrap().len(), 2);
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::Event(e))if e.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
}
#[test]
fn unsupported_spot_capability_refuses_the_batch_before_io() {
    let (mut e, _) = seed(true);
    let orders=serde_json::from_value(json!([
        {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":41000,"clientOrderId":"spot_1"},
        {"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.002,"price":41000,"clientOrderId":"spot_2","reduceOnly":true}
    ])).unwrap();
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 60,
            orders,
        }),
    );
    let a = collect(&mut e);
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.method==Method::Post))
    );
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:60,error})if error.outcome==Some(Outcome::NotSent))));
}

fn batch_order(amount: f64, price: f64, client: &str) -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy",
        "amount":amount,"price":price,"clientOrderId":client})
}

fn submit_batch(e: &mut Engine<BingxSpot>, orders: Vec<Value>) -> Vec<Action> {
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 70,
            orders: serde_json::from_value(json!(orders)).expect("batch fixture"),
        }),
    );
    collect(e)
}

#[test]
fn spot_batch_valid_native_boundaries_place_every_row_exactly_once() {
    let (mut e, _) = seed(true);
    let expected = [(0.00019, 41000.), (18.26637, 1.), (0.002, 2500.)];
    let orders = expected
        .iter()
        .enumerate()
        .map(|(i, (amount, price))| batch_order(*amount, *price, &format!("valid_{i}")))
        .collect();
    let mut actions = submit_batch(&mut e, orders);
    for (i, (amount, price)) in expected.iter().enumerate() {
        let requests: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) if r.method == Method::Post => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(
            requests.len(),
            1,
            "exactly one row may be dispatched at a time"
        );
        let r = requests[0];
        assert_eq!(r.method, Method::Post);
        assert!(r.url.ends_with("/spot/v1/trade/order"));
        let body: Value =
            serde_json::from_str(r.body.as_deref().expect("order body")).expect("order JSON");
        assert_eq!(body["quantity"], *amount);
        assert_eq!(body["price"], *price);
        assert_eq!(body["newClientOrderId"], format!("valid_{i}"));
        let ack = json!({"code":0,"data":{"orderId":format!("900{i}"),
            "symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"NEW"}});
        reply(&mut e, r, 200, &ack.to_string());
        actions = collect(&mut e);
    }
    assert_eq!(done(&actions, 70), json!(["9000", "9001", "9002"]));
    assert!(
        actions
            .iter()
            .all(|a| !matches!(a, Action::Http(r) if r.method == Method::Post))
    );
    assert!(
        collect(&mut e)
            .iter()
            .all(|a| !matches!(a, Action::Http(r) if r.method == Method::Post))
    );
}

#[test]
fn a_keyless_client_id_lookup_finishes_without_private_io() {
    let (mut e, _) = seed(false);
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 61,
            query: Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "spot_1".into(),
            },
        }),
    );
    let a = collect(&mut e);
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:61,error})if error.outcome==Some(Outcome::NotSent))));
}

#[test]
fn history_retains_requested_time_bounds_even_if_the_venue_defaults_its_range() {
    let (mut e, _) = seed(true);
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 62,
            query: Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(1704961900000.),
                to_ms: Some(1704961924999.),
                limit: Some(10),
            },
        }),
    );
    let r = request(&collect(&mut e), "/trade/myTrades");
    reply(&mut e, &r, 200, FILLS);
    let a = collect(&mut e);
    assert_eq!(done(&a, 62), json!([]));
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::Event(e))if e.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
}

#[test]
fn documented_placement_envelope_keeps_its_exact_integer_identity() {
    let (mut e, _) = seed(true);
    let r = request(&place(&mut e, "limit", "sell"), "/trade/order");
    let mut body: Value = serde_json::from_str(include_str!("fixtures/spot/order.json")).unwrap();
    let id = body["data"]["orderId"].to_string();
    body["data"]["symbol"] = json!("BTC-USDT");
    body["data"]["price"] = json!("41000");
    body["data"]["origQty"] = json!("0.002");
    reply(&mut e, &r, 200, &body.to_string());
    assert_eq!(done(&collect(&mut e), 41), json!([id]));
}

#[test]
fn caller_read_errors_are_invalid_input_and_unsupported_axes_remain_unsupported() {
    let (mut e, _) = seed(true);
    for (command, expected) in [
        (
            Command::FetchOrderBook {
                command: 81,
                symbol: "UNKNOWN".into(),
                limit: None,
            },
            safe_cex_engine::ErrorKind::InvalidInput,
        ),
        (
            Command::Query {
                command: 81,
                query: Query::OrderByClientId {
                    symbol: "BTCUSDT".into(),
                    client_order_id: "invalid-id!".into(),
                },
            },
            safe_cex_engine::ErrorKind::InvalidInput,
        ),
        (
            Command::FetchCandles {
                command: 81,
                symbol: "BTCUSDT".into(),
                interval: safe_cex_engine::model::Interval::S1,
                limit: None,
                from_ms: None,
                to_ms: None,
            },
            safe_cex_engine::ErrorKind::Unsupported,
        ),
    ] {
        e.handle(NOW, Event::Command(command));
        let actions = collect(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 81, error }) if error.kind == Some(expected) && error.outcome == Some(Outcome::NotSent))), "{actions:?}");
    }
}

#[test]
fn lost_write_transport_is_unknown_network_and_never_resends() {
    for route in ["order", "cancel", "cancelOpenOrders"] {
        let (mut e, _) = seed(true);
        let actions = match route {
            "order" => place(&mut e, "limit", "buy"),
            "cancel" => {
                e.handle(
                    NOW,
                    Event::Command(Command::CancelOrdersWithSymbols {
                        command: 41,
                        orders: serde_json::from_value(
                            json!([{"id":"9007199254740993","symbol":"BTCUSDT"}]),
                        )
                        .unwrap(),
                    }),
                );
                collect(&mut e)
            }
            _ => {
                e.handle(
                    NOW,
                    Event::Command(Command::CancelSymbolOrders {
                        command: 41,
                        symbol: "BTCUSDT".into(),
                    }),
                );
                collect(&mut e)
            }
        };
        let r = request(&actions, &format!("/trade/{route}"));
        e.handle(
            NOW,
            Event::HttpFailed {
                request: r.id,
                error: "connection lost after send",
            },
        );
        let actions = collect(&mut e);
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 41, error }) if error.kind == Some(safe_cex_engine::ErrorKind::Network) && error.outcome == Some(Outcome::Unknown))), "{route}: {actions:?}");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
        );
        if route == "order" {
            let _reconcile = request(&actions, "/trade/openOrders");
            let second = place(&mut e, "limit", "buy");
            assert!(second.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 41, error }) if error.outcome == Some(Outcome::NotSent) && error.message == "Placement outcome is uncertain; reconcile before placing or amending")), "{second:?}");
            assert!(
                !second
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
            );
        }
        for action in actions {
            if let Action::SetTimer { timer, at_ms } = action {
                e.handle(at_ms, Event::TimerFired { timer });
            }
        }
        assert!(!collect(&mut e).iter().any(|a| matches!(a, Action::Http(r) if r.method == Method::Post && r.url.contains("/trade/"))));
    }
}
