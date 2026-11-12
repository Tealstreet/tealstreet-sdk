//! Documented order vocabulary: https://phemex-docs.github.io/#common-order-fields
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, Response, SocketId};
use safe_cex_venue_phemex::PhemexPerpetuals;
use serde_json::{Value, json};
use support::{Options, PreparedEngine as Engine};

const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"error":null,"result":[{"symbol":"BTCUSDT","bidRp":"69000","askRp":"69001","lastRp":"69000","markRp":"69000","indexRp":"69000","openRp":"69000","openInterestRv":"250","fundingRateRr":"0.0001","volumeRq":"10"}]}"#;

fn requests(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect()
}

fn writes(actions: &[Action]) -> Vec<HttpRequest> {
    requests(actions)
        .into_iter()
        .filter(|r| r.method != Method::Get)
        .collect()
}

fn raw(e: &mut Engine<PhemexPerpetuals>, r: &HttpRequest, body: &str) -> Vec<Action> {
    e.handle(
        1001.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}

fn respond(e: &mut Engine<PhemexPerpetuals>, r: &HttpRequest, body: Value) -> Vec<Action> {
    raw(e, r, &body.to_string())
}

fn done(actions: &[Action]) -> Result<String, String> {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { result, .. }) => Some(Ok(result.clone())),
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(Err(error.legacy_text())),
            _ => None,
        })
        .expect("command completed")
}

fn command(e: &mut Engine<PhemexPerpetuals>, c: Command) -> Vec<Action> {
    e.handle(1000., Event::Command(c));
    e.drain_actions().collect()
}

fn engine_with_socket(mode: &str) -> (Engine<PhemexPerpetuals>, SocketId) {
    engine_with_socket_policy(mode, safe_cex_engine::PublicFeedPolicy::Direct)
}

fn engine_with_socket_policy(
    mode: &str,
    policy: safe_cex_engine::PublicFeedPolicy,
) -> (Engine<PhemexPerpetuals>, SocketId) {
    let mut e = support::prepared_engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    e.set_public_feed_policy(policy).unwrap();
    e.start(0.);
    let a: Vec<_> = e.drain_actions().collect();
    let mut a = raw(&mut e, &requests(&a)[0], PRODUCTS);
    let ticker = requests(&a)
        .into_iter()
        .find(|r| r.url.contains("24hr/all"))
        .unwrap();
    a.extend(raw(&mut e, &ticker, DAILY));
    a.retain(|action| !matches!(action, Action::Http(r) if r.url.contains("24hr/all")));
    let socket = a
        .iter()
        .filter_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } = a
            {
                Some(*socket)
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    for r in requests(&a) {
        let currency = if r.url.contains("USDC") {
            "USDC"
        } else {
            "USDT"
        };
        let rows = if currency == "USDT" {
            vec![
                json!({"symbol":"BTCUSDT","currency":"USDT","posMode":mode,"posSide":if mode=="OneWay"{"Merged"}else{"Long"},"size":"0","leverageRr":"-10","crossMargin":true}),
            ]
        } else {
            vec![]
        };
        respond(
            &mut e,
            &r,
            json!({"code":0,"data":{"account":{"currency":currency,"accountBalanceRv":"1000","totalUsedBalanceRv":"0"},"positions":rows}}),
        );
    }
    e.drain_actions().for_each(drop);
    (e, socket)
}

fn engine_mode(mode: &str) -> Engine<PhemexPerpetuals> {
    engine_with_socket(mode).0
}

fn engine() -> Engine<PhemexPerpetuals> {
    engine_mode("OneWay")
}

fn opts(id: &str) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.01,
        price: Some(69000.),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: (!id.is_empty()).then(|| id.into()),
        close: None,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    }
}

fn params(r: &HttpRequest) -> Vec<(String, String)> {
    r.url
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap();
            (k.into(), v.replace("%2C", ","))
        })
        .collect()
}

fn p(r: &HttpRequest, key: &str) -> Option<String> {
    params(r)
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

fn ack(r: &HttpRequest, id: &str) -> Value {
    json!({"code":0,"data":{"orderID":id,"clOrdID":p(r,"clOrdID").unwrap(),"symbol":"BTCUSDT","side":p(r,"side").unwrap(),"posSide":p(r,"posSide").unwrap(),"orderType":p(r,"ordType").unwrap(),"orderQtyRq":p(r,"orderQtyRq").unwrap(),"priceRp":p(r,"priceRp").unwrap_or("0".into()),"ordStatus":"New","execStatus":"New","timeInForce":"GoodTillCancel"}})
}

fn order(id: &str) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 69000.,
        amount: 0.1,
        filled: 0.,
        remaining: 0.1,
        reduce_only: false,
        close: None,
        client_order_id: Some("old".into()),
        time_in_force: Some(TimeInForce::PostOnly),
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

fn authenticated(mode: &str) -> (Engine<PhemexPerpetuals>, SocketId) {
    let (mut e, socket) = engine_with_socket(mode);
    e.handle(1000., Event::WsOpened { socket });
    e.handle(
        1000.,
        Event::WsMessage {
            socket,
            text: r#"{"id":999,"result":{"status":"success"}}"#,
        },
    );
    e.drain_actions().for_each(drop);
    (e, socket)
}

fn ws_orders(e: &mut Engine<PhemexPerpetuals>, socket: SocketId, rows: Vec<Value>) {
    let text =
        json!({"type":"incremental","timestamp":2_000_000_000_u64,"orders_p":rows}).to_string();
    e.handle(
        1002.,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    e.drain_actions().for_each(drop);
}

fn live_row(id: &str) -> Value {
    json!({"orderID":id,"symbol":"BTCUSDT","side":"Buy","posSide":"Merged","ordType":"Limit","ordStatus":"New","orderQty":"0.1","cumQty":"0","leavesQty":"0.1","priceRp":"69000"})
}

fn history_row() -> Value {
    json!({"actionTimeNs":1667562110213260743_u64,"transactTimeNs":1667562110221077395_u64,
        "orderId":"history-order","symbol":"BTCUSDT","side":"Sell","ordType":"Stop",
        "ordStatus":"Canceled","priceRp":"0","orderQtyRq":"0.1","cumQtyRq":"0",
        "reduceOnly":true,"stopDirection":"Falling","stopPxRp":"68000","pegPriceType":""})
}

fn history(row: Value, streamed: bool) -> Result<Vec<Value>, String> {
    let mut e = engine();
    let actions = command(
        &mut e,
        Command::Query {
            command: 1,
            query: safe_cex_engine::Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        },
    );
    let request = requests(&actions).remove(0);
    let body = json!([row]).to_string();
    let actions = if streamed {
        e.handle(
            1001.,
            Event::HttpStart {
                request: request.id,
                status: 200,
                headers: "",
            },
        );
        for data in body.as_bytes().chunks(7) {
            e.handle(
                1001.,
                Event::HttpChunk {
                    request: request.id,
                    data,
                },
            );
        }
        e.handle(
            1001.,
            Event::HttpEnd {
                request: request.id,
            },
        );
        e.drain_actions().collect()
    } else {
        raw(&mut e, &request, &body)
    };
    done(&actions).map(|body| serde_json::from_str(&body).unwrap())
}

// https://phemex-docs.github.io/#common-order-fields: RPI remains a restricted TIF.
#[test]
fn rpi_post_only_survives_private_stream_and_rest_census() {
    for native in ["PostOnly", "RPIPostOnly"] {
        for rest in [false, true] {
            let (mut e, socket) = authenticated("OneWay");
            let mut row = live_row("post-only");
            row["timeInForce"] = json!(native);
            if rest {
                let actions = command(
                    &mut e,
                    Command::Venue {
                        command: 1,
                        action: safe_cex_engine::VenueAction::RefreshOrders {
                            symbols: Some(vec!["BTCUSDT".into()]),
                        },
                    },
                );
                let read = requests(&actions)
                    .into_iter()
                    .find(|r| r.url.contains("/g-orders/activeList"))
                    .expect("REST census");
                respond(&mut e, &read, json!({"code":0,"data":{"rows":[row]}}));
            } else {
                ws_orders(&mut e, socket, vec![row]);
            }
            let order = e.store().order("post-only").expect("visible limit order");
            assert_eq!(order.kind, OrderType::Limit);
            assert_eq!(order.status, OrderStatus::Open);
            assert_eq!(order.remaining, 0.1);
            assert_eq!(
                order.time_in_force,
                Some(TimeInForce::PostOnly),
                "{native}, rest={rest}"
            );
        }
    }
}

#[test]
fn rpi_post_only_survives_client_id_lookup_without_mutating_store() {
    for native in ["PostOnly", "RPIPostOnly"] {
        let mut e = engine();
        let actions = command(
            &mut e,
            Command::Query {
                command: 1,
                query: safe_cex_engine::Query::OrderByClientId {
                    symbol: "BTCUSDT".into(),
                    client_order_id: "caller".into(),
                },
            },
        );
        let read = requests(&actions).remove(0);
        let mut row = live_row("post-only");
        row["clOrdID"] = json!("caller");
        row["transactTimeNs"] = json!(2_000_000_000_u64);
        row["timeInForce"] = json!(native);
        let actions = respond(&mut e, &read, json!([row]));
        let result: Value = serde_json::from_str(&done(&actions).unwrap()).unwrap();
        assert_eq!(result["found"], true);
        assert_eq!(result["order"]["id"], "post-only");
        assert_eq!(result["order"]["timeInForce"], "PostOnly", "{native}");
        assert!(e.store().orders().is_empty());
    }
}

#[test]
fn deactivated_stop_history_is_terminal_cancelled() {
    for streamed in [false, true] {
        let mut row = history_row();
        row["ordStatus"] = json!("Deactivated");
        let rows = history(row, streamed).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["status"], "canceled");
        assert_eq!(rows[0]["type"], "stop_market");
        assert_eq!(rows[0]["filled"], 0.);
    }
}

#[test]
fn unspecified_history_peg_preserves_ordinary_and_protective_kinds() {
    for streamed in [false, true] {
        for (native, kind) in [("Limit", "limit"), ("Stop", "stop_market")] {
            let mut row = history_row();
            row["ordType"] = json!(native);
            row["pegPriceType"] = json!("UNSPECIFIED");
            let rows = history(row.clone(), streamed).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["type"], kind);
            if native == "Stop" {
                row["ordStatus"] = json!("Deactivated");
                assert_eq!(history(row, streamed).unwrap()[0]["status"], "canceled");
            }
        }
    }
}

#[test]
fn created_create_ack_returns_identity_without_publishing_open_order() {
    let (mut e, socket) = authenticated("OneWay");
    let actions = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts("caller")],
        },
    );
    let request = writes(&actions).remove(0);
    let mut value = ack(&request, "actual-id");
    value["data"]["ordStatus"] = json!("Created");
    let actions = respond(&mut e, &request, value);
    assert_eq!(done(&actions).unwrap(), "[\"actual-id\"]");
    assert!(
        e.store().orders().is_empty(),
        "Created is only a transient acknowledgement"
    );
    let mut row = live_row("actual-id");
    row["orderQty"] = json!("0.01");
    row["leavesQty"] = json!("0.01");
    ws_orders(&mut e, socket, vec![row]);
    assert_eq!(
        e.store().order("actual-id").unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn created_cancel_ack_waits_for_private_finality_without_tombstone() {
    let (mut e, socket) = authenticated("OneWay");
    e.store_mut().upsert_order(order("old"));
    let actions = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["old".into()],
        },
    );
    let request = writes(&actions).remove(0);
    let actions = respond(
        &mut e,
        &request,
        json!({"code":0,"data":{"orderID":"old","ordStatus":"Created","execStatus":"Created","bizError":0,"cumQtyRq":"0"}}),
    );
    assert_eq!(done(&actions).unwrap(), "[]");
    assert!(writes(&actions).is_empty());
    assert_eq!(
        e.store().order("old").unwrap().status,
        OrderStatus::Canceling
    );
    ws_orders(&mut e, socket, vec![live_row("old")]);
    assert_eq!(e.store().order("old").unwrap().status, OrderStatus::Open);
    let mut row = live_row("old");
    row["ordStatus"] = json!("Deactivated");
    row["transactTimeNs"] = json!(3_000_000_000_u64);
    ws_orders(&mut e, socket, vec![row]);
    assert_eq!(
        e.store().order("old").unwrap().status,
        OrderStatus::Canceled
    );
    ws_orders(&mut e, socket, vec![live_row("old")]);
    assert_eq!(
        e.store().order("old").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn created_amend_ack_never_sends_replacement_without_private_finality() {
    let (mut e, socket) = authenticated("OneWay");
    e.store_mut().upsert_order(order("old"));
    let actions = command(
        &mut e,
        Command::AmendOrder {
            command: 1,
            id: "old".into(),
            price: Some(67900.),
            amount: None,
        },
    );
    let request = writes(&actions).remove(0);
    let actions = respond(
        &mut e,
        &request,
        json!({"code":0,"data":{"orderID":"old","ordStatus":"Created","execStatus":"Created","bizError":0,"cumQtyRq":"0"}}),
    );
    assert!(writes(&actions).is_empty());
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
    assert_eq!(
        e.store().order("old").unwrap().status,
        OrderStatus::Canceling
    );
    ws_orders(&mut e, socket, vec![live_row("old")]);
    assert_eq!(e.store().order("old").unwrap().status, OrderStatus::Open);
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 11_001. => Some(*timer),
            _ => None,
        })
        .expect("replacement wait timer");
    e.handle(11_001., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(writes(&actions).is_empty());
    assert!(
        done(&actions)
            .unwrap_err()
            .contains("replacement was not sent")
    );
}

#[test]
fn init_account_row_is_nonterminal_like_created() {
    for status in ["Created", "Init"] {
        let (mut e, socket) = authenticated("OneWay");
        let mut row = live_row("transient");
        row["ordStatus"] = json!(status);
        ws_orders(&mut e, socket, vec![row]);
        assert_eq!(
            e.store().order("transient").unwrap().status,
            OrderStatus::Open,
            "{status}"
        );
        ws_orders(&mut e, socket, vec![live_row("transient")]);
        assert_eq!(e.store().order("transient").unwrap().remaining, 0.1);
    }
}

#[test]
fn history_unknown_values_and_invalid_protective_fields_still_fail_closed() {
    for streamed in [false, true] {
        for (status, peg) in [("Canceled", ""), ("Deactivated", "UNSPECIFIED")] {
            for (field, value) in [
                ("ordStatus", json!("Unknown")),
                ("pegPriceType", json!("UnknownPeg")),
                ("side", json!("Unknown")),
                ("stopDirection", json!("Unknown")),
                ("stopPxRp", json!("0")),
                ("cumQtyRq", json!("0.2")),
                ("orderId", json!("")),
            ] {
                let mut row = history_row();
                row["ordStatus"] = json!(status);
                row["pegPriceType"] = json!(peg);
                row[field] = value;
                assert!(history(row, streamed).is_err(), "{field}");
            }
        }
        let rows = history(history_row(), streamed).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["type"], "stop_market");
    }
}

#[test]
fn transient_cancel_aliases_keep_array_cardinality_guards() {
    for status in ["Init", "Created"] {
        for duplicate in [false, true] {
            let mut e = engine();
            e.store_mut().upsert_order(order("old"));
            let actions = command(
                &mut e,
                Command::CancelOrders {
                    command: 1,
                    ids: vec!["old".into()],
                },
            );
            let request = writes(&actions).remove(0);
            let row = json!({"orderID":"old","ordStatus":status,"cumQtyRq":"0"});
            let rows = if duplicate {
                vec![row.clone(), row]
            } else {
                vec![]
            };
            let actions = respond(&mut e, &request, json!({"code":0,"data":rows}));
            assert!(done(&actions).is_err());
            assert!(writes(&actions).is_empty());
            assert_eq!(e.store().order("old").unwrap().status, OrderStatus::Open);
        }
    }
}

#[test]
fn transient_create_aliases_keep_identity_and_rejection_guards() {
    for status in ["Init", "Created"] {
        for (field, mismatch) in [
            ("clOrdID", json!("other")),
            ("symbol", json!("ETHUSDT")),
            ("side", json!("Sell")),
            ("orderQtyRq", json!("0.02")),
            ("posSide", json!("Short")),
            ("orderType", json!("Unknown")),
            ("bizError", json!(1)),
        ] {
            let mut e = engine();
            let actions = command(
                &mut e,
                Command::PlaceOrders {
                    command: 1,
                    orders: vec![opts("caller")],
                },
            );
            let request = writes(&actions).remove(0);
            let mut value = ack(&request, "actual-id");
            value["data"]["ordStatus"] = json!(status);
            value["data"][field] = mismatch;
            let actions = respond(&mut e, &request, value);
            assert!(done(&actions).is_err(), "{status}: {field}");
            assert!(e.store().order("actual-id").is_none());
        }
    }
}

#[test]
fn transient_cancel_aliases_keep_identity_rejection_and_fill_guards() {
    for status in ["Init", "Created"] {
        for (field, value) in [
            ("orderID", json!("other")),
            ("bizError", json!(1)),
            ("cumQtyRq", json!("-1")),
            ("execStatus", json!("Rejected")),
            ("ordStatus", json!("Unknown")),
        ] {
            let mut e = engine();
            e.store_mut().upsert_order(order("old"));
            let actions = command(
                &mut e,
                Command::CancelOrders {
                    command: 1,
                    ids: vec!["old".into()],
                },
            );
            let request = writes(&actions).remove(0);
            let mut body =
                json!({"code":0,"data":{"orderID":"old","ordStatus":status,"cumQtyRq":"0"}});
            body["data"][field] = value;
            let actions = respond(&mut e, &request, body);
            assert!(done(&actions).is_err(), "{status}: {field}");
            assert!(writes(&actions).is_empty());
            assert_eq!(e.store().order("old").unwrap().status, OrderStatus::Open);
        }
    }
}

#[test]
fn unknown_account_status_does_not_admit_or_remove_orders() {
    let (mut e, socket) = authenticated("OneWay");
    ws_orders(&mut e, socket, vec![live_row("old")]);
    let mut row = live_row("old");
    row["ordStatus"] = json!("Unknown");
    ws_orders(&mut e, socket, vec![row.clone()]);
    assert_eq!(e.store().order("old").unwrap().status, OrderStatus::Open);
    row["orderID"] = json!("unknown");
    ws_orders(&mut e, socket, vec![row]);
    assert!(e.store().order("unknown").is_none());
}
