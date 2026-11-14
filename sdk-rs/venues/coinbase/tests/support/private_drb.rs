use super::*;
use serde_json::json;

// Synthetic USDC linear-perpetual examples following Coinbase's migration
// guide and adv-starbase-openapi.json (2026-11-07); never live account data.
fn account_row() -> Value {
    json!({"currency":"USDC","balance":1000.,"equity":1020.,
        "available_funds":800.,"initial_margin":220.,"maintenance_margin":100.,
        "futures_session_upl":20.,"margin_model":"cross_sm"})
}
fn position_row() -> Value {
    json!({"instrument_name":"BTC_USDC-PERPETUAL","kind":"future",
        "user_id":42,"direction":"sell","size":-6000.,"size_currency":-0.1,
        "average_price":61000.,"mark_price":60000.,"index_price":60001.,
        "floating_profit_loss":100.,"realized_profit_loss":5.,
        "initial_margin":600.,"maintenance_margin":300.,"open_orders_margin":0.,
        "leverage":10,"estimated_liquidation_price":66000.})
}
fn order_row() -> Value {
    json!({"order_id":"USDC-123","instrument_name":"BTC_USDC-PERPETUAL",
        "direction":"buy","order_state":"open","order_type":"limit",
        "time_in_force":"good_til_cancelled","price":60000.,
        "amount":0.1,"filled_amount":0.025,"label":"client-label",
        "post_only":true,"reduce_only":false,
        "creation_timestamp":1700000000000u64,"last_update_timestamp":1700000001000u64})
}
#[test]
fn summary_maps_equity_and_available_margin_in_usdc() {
    let b = summary(account_row()).unwrap();
    assert_eq!((b.total, b.free, b.used, b.upnl), (1020., 800., 220., 20.));
    let mut bad = account_row();
    bad["currency"] = json!("BTC");
    assert!(summary(bad).is_err());
}
#[test]
fn short_position_uses_base_size_and_mark_notional() {
    let p = position(position_row()).unwrap();
    assert_eq!(p.symbol, "BTC_USDC-PERPETUAL");
    assert_eq!(p.side, PositionSide::Short);
    assert_eq!(
        (p.contracts, p.notional, p.entry_price, p.unrealized_pnl),
        (0.1, 6000., 61000., 100.)
    );
    assert_eq!((p.leverage, p.liquidation_price), (10., 66000.));
}
#[test]
fn orders_use_linear_base_units_and_preserve_linked_trigger_identity() {
    let o = order(order_row()).unwrap();
    assert_eq!(o.symbol, "BTC_USDC-PERPETUAL");
    assert!((o.remaining - 0.075).abs() < 1e-12);
    assert_eq!((o.amount, o.filled), (0.1, 0.025));
    assert_eq!(o.time_in_force, Some(TimeInForce::PostOnly));
    let mut child = order_row();
    child["order_type"] = json!("stop_limit");
    child["order_state"] = json!("untriggered");
    child["trigger_price"] = json!(55000.);
    child["trigger"] = json!("mark_price");
    child["primary_order_id"] = json!("USDC-parent");
    child["reduce_only"] = json!(true);
    let o = order(child).unwrap();
    assert_eq!(o.parent_id.as_deref(), Some("USDC-parent"));
    assert_eq!(o.kind, OrderType::StopLoss);
    assert_eq!(o.price, 55000.);
    assert_eq!(o.extra.unwrap().conditional_order_price, Some(60000.));
    assert_eq!(o.trigger_type, Some(StopTriggerType::MarkPrice));
}
#[test]
fn malformed_economics_are_rejected_before_snapshot_publication() {
    for (key, value) in [
        ("amount", json!(-1)),
        ("filled_amount", json!(1)),
        ("direction", json!("BUY")),
        ("order_state", json!("surprise")),
        ("price", json!("60000")),
    ] {
        let mut row = order_row();
        row[key] = value;
        assert!(order(row).is_err(), "{key}");
    }
    let mut row = position_row();
    row.as_object_mut().unwrap().remove("size_currency");
    assert!(position(row).is_err());
}

use crate::{Coinbase, Options};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, TimerId};
struct Rig {
    e: Engine<Coinbase>,
    now: f64,
    http: std::collections::VecDeque<HttpRequest>,
    timers: Vec<(f64, TimerId)>,
    actions: Vec<Action>,
}
impl Rig {
    fn new(spot: bool) -> Self {
        let mut e = safe_cex_engine::test_support::seeded_engine(
            Coinbase::new(Options {
                key: "organizations/test/apiKeys/test".into(),
                secret: STANDARD.encode([7; 32]),
                spot_only: spot,
                ..Default::default()
            })
            .unwrap(),
        );
        e.start(1700000000000.);
        Self {
            e,
            now: 1700000000000.,
            http: Default::default(),
            timers: vec![],
            actions: vec![],
        }
    }
    fn collect(&mut self) {
        for a in safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now) {
            match a {
                Action::Http(r) if r.url.contains("api.international.coinbase.com") => {
                    self.reply(r, json!([]))
                }
                Action::Http(r) => self.http.push_back(r),
                Action::SetTimer { at_ms, timer } => self.timers.push((at_ms, timer)),
                Action::ClearTimer { timer } => self.timers.retain(|(_, t)| *t != timer),
                a => self.actions.push(a),
            }
        }
    }
    fn request(&mut self) -> HttpRequest {
        for _ in 0..100 {
            self.collect();
            if let Some(r) = self.http.pop_front() {
                return r;
            }
            self.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (at, timer) = self.timers.remove(0);
            self.now = self.now.max(at);
            self.e.handle(self.now, Event::TimerFired { timer });
        }
        panic!("no request")
    }
    fn reply(&mut self, r: HttpRequest, body: Value) {
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &body.to_string(),
                },
            },
        );
    }
    #[allow(dead_code)]
    fn rpc(&mut self, r: HttpRequest, result: Value) {
        let body: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        self.reply(r, json!({"jsonrpc":"2.0","id":body["id"],"result":result}));
    }
    fn select_intx(&mut self) {
        let r = self.request();
        assert!(r.url.ends_with("/portfolios"));
        self.reply(
            r,
            json!({"portfolios":[{"uuid":"portfolio-one","type":"INTX"}]}),
        );
        let r = self.request();
        assert!(r.url.ends_with("/portfolios/portfolio-one"));
        self.reply(r, json!({"breakdown":{}}));
    }
}
#[test]
fn intx_startup_exchanges_cdp_key_on_drb_before_private_reads() {
    let mut r = Rig::new(false);
    r.select_intx();
    let request = r.request();
    assert_eq!(request.url, format!("{}/public/auth", crate::drb::HTTP_URL));
    let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["params"]["grant_type"], "coinbase_cdp");
    assert!(!request.url.contains("token"));
}

fn trade_row(id: &str, seq: u64) -> Value {
    json!({"trade_id":id,"trade_seq":seq,"order_id":"USDC-123",
        "instrument_name":"BTC_USDC-PERPETUAL","timestamp":1700000001000u64,
        "direction":"buy","price":60000.,"amount":0.025,"fee":0.1,
        "fee_currency":"USDC","liquidity":"M"})
}
impl Rig {
    fn ready() -> Self {
        Self::ready_with(Self::new(false))
    }
    fn ready_with(mut r: Self) -> Self {
        r.select_intx();
        let auth = r.request();
        r.rpc(auth,json!({"access_token":"fixture-http-token","expires_in":900,"scope":"trade:read","token_type":"bearer"}));
        let instruments = r.request();
        assert!(
            instruments.url.ends_with("/public/get_instruments"),
            "{}",
            instruments.url
        );
        r.rpc(
            instruments,
            json!([{
                "instrument_name":"BTC_USDC-PERPETUAL", "kind":"future",
                "instrument_type":"linear", "settlement_period":"perpetual",
                "settlement_currency":"USDC", "base_currency":"BTC", "quote_currency":"USDC",
                "is_active":true, "tick_size":0.01, "contract_size":0.001,
                "min_trade_amount":0.001, "max_leverage":50.
            }]),
        );
        let summaries = r.request();
        assert!(
            summaries
                .url
                .ends_with("/public/get_book_summary_by_currency")
        );
        r.rpc(
            summaries,
            json!([{
                "instrument_name":"BTC_USDC-PERPETUAL", "funding_8h":0.0001,
                "last":60000., "mark_price":60000., "estimated_delivery_price":60001.,
                "volume":2., "volume_notional":120000., "open_interest":10.
            }]),
        );
        for _ in 0..3 {
            let request = r.request();
            assert!(
                request.url.starts_with(crate::drb::HTTP_URL),
                "{}",
                request.url
            );
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(k, v)| *k == "Authorization" && v == "Bearer fixture-http-token")
            );
            let result = if request.url.ends_with("/get_account_summary") {
                account_row()
            } else if request.url.ends_with("/get_positions") {
                json!([position_row()])
            } else if request.url.ends_with("/get_open_orders_by_currency")
                || request.url.ends_with("/get_open_orders")
            {
                json!([order_row()])
            } else {
                panic!("{}", request.url)
            };
            r.rpc(request, result);
        }
        r.collect();
        r
    }
    fn private_socket(&self) -> safe_cex_engine::SocketId {
        self.actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. } if url == crate::drb::WS_URL => Some(*socket),
                _ => None,
            })
            .expect("drb private socket")
    }
    fn ws_reply(&mut self, socket: safe_cex_engine::SocketId, id: Value, result: Value) {
        self.e.handle(
            self.now,
            Event::WsMessage {
                socket,
                text: &json!({"jsonrpc":"2.0","id":id,"result":result}).to_string(),
            },
        );
        self.collect();
    }
    fn subscribe(&mut self) -> safe_cex_engine::SocketId {
        let socket = self.private_socket();
        self.e.handle(self.now, Event::WsOpened { socket });
        self.collect();
        let auth = self
            .actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::WsSend { socket: s, text } if *s == socket => {
                    Some(serde_json::from_str::<Value>(text).unwrap())
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(auth["method"], "public/auth");
        assert_eq!(auth["params"]["grant_type"], "coinbase_cdp");
        self.ws_reply(socket,auth["id"].clone(),json!({"access_token":"fixture-ws-token","expires_in":3600,"scope":"trade:read","token_type":"bearer"}));
        let messages = self
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::WsSend { socket: s, text } if *s == socket => {
                    Some(serde_json::from_str::<Value>(text).unwrap())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for msg in messages {
            if msg["method"] == "private/subscribe" {
                self.ws_reply(socket, msg["id"].clone(), msg["params"]["channels"].clone());
            } else if msg["method"] == "public/set_heartbeat" {
                self.ws_reply(socket, msg["id"].clone(), json!("ok"));
            }
        }
        socket
    }
    fn notification(&mut self, socket: safe_cex_engine::SocketId, channel: &str, data: Value) {
        self.e.handle(self.now,Event::WsMessage{socket,text:&json!({"jsonrpc":"2.0","method":"subscription","params":{"channel":channel,"data":data}}).to_string()});
        self.collect();
    }
}

#[path = "drb_linkage.rs"]
mod linkage_tests;
#[path = "drb_provider.rs"]
mod provider_tests;
#[test]
fn startup_publishes_private_state_and_never_calls_retired_intx_reads() {
    let r = Rig::ready();
    assert!(
        r.e.store().loaded().balance
            && r.e.store().loaded().orders
            && r.e.store().loaded().positions
    );
    assert_eq!(r.e.store().balance().total, 1020.);
    assert_eq!(r.e.store().positions()[0].contracts, 0.1);
    assert_eq!(r.e.store().orders()[0].amount, 0.1);
}
#[test]
fn websocket_changes_are_atomic_and_fill_ids_deduplicate() {
    let mut r = Rig::ready();
    let socket = r.subscribe();
    let mut p = position_row();
    p["size_currency"] = json!(-0.2);
    let mut o = order_row();
    o["filled_amount"] = json!(0.05);
    o["last_update_timestamp"] = json!(1700000002000u64);
    let data = json!({"orders":[o],"positions":[p],"trades":[trade_row("USDC-fill",10)]});
    r.notification(socket, "user.changes.future.USDC.100ms", data.clone());
    r.notification(socket, "user.changes.future.USDC.100ms", data);
    assert_eq!(r.e.store().positions()[0].contracts, 0.2);
    assert_eq!(r.e.store().orders()[0].filled, 0.05);
    let frame = r.e.take_market_frame(r.now).expect("execution frame");
    let (_, payload) = safe_cex_engine::wire::sections(frame)
        .find(|(k, _)| *k == safe_cex_engine::wire::section::FILLS)
        .expect("fills section");
    assert_eq!(u32::from_le_bytes(payload[..4].try_into().unwrap()), 1);
    let mut b = account_row();
    b["equity"] = json!(1030.);
    r.notification(socket, "user.portfolio.USDC", b);
    assert_eq!(r.e.store().balance().total, 1030.);
}
#[test]
fn fills_query_retains_bounds_symbol_order_identity_and_seconds_output() {
    let mut r = Rig::ready();
    r.e.handle(
        r.now,
        Event::Command(safe_cex_engine::Command::Query {
            command: 77,
            query: safe_cex_engine::Query::Fills {
                symbol: Some("BTC_USDC-PERPETUAL".into()),
                order_id: Some("USDC-123".into()),
                from_ms: Some(1700000000000.),
                to_ms: Some(1700000003000.),
                limit: Some(10),
            },
        }),
    );
    let req = r.request();
    assert!(
        req.url.ends_with("/private/get_user_trades_by_order"),
        "{}",
        req.url
    );
    r.rpc(req, json!([trade_row("USDC-fill", 10)]));
    let archived = r.request();
    r.rpc(archived, json!([]));
    r.collect();
    let result = r
        .actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandDone {
                command: 77,
                result,
            }) => Some(serde_json::from_str::<Value>(result).unwrap()),
            _ => None,
        })
        .expect("history command completes");
    assert_eq!(result[0]["timestamp"], 1700000001.);
    assert_eq!(result[0]["qty"], 0.025);
    assert_eq!(result[0]["orderId"], "USDC-123");
    assert_eq!(result[0]["symbol"], "BTC_USDC-PERPETUAL");
}

impl Rig {
    fn poll_requests(&mut self) -> Vec<HttpRequest> {
        for _ in 0..30 {
            self.collect();
            if self.http.len() >= 3 {
                return self.http.drain(..).collect();
            }
            self.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (at, timer) = self.timers.remove(0);
            self.now = self.now.max(at);
            self.e.handle(self.now, Event::TimerFired { timer });
        }
        panic!("account poll requests missing")
    }
    fn command_result(&self, id: u32) -> Result<Value, String> {
        self.actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(safe_cex_engine::Notice::CommandDone { command, result })
                    if *command == id =>
                {
                    Some(Ok(serde_json::from_str(result).unwrap()))
                }
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command, error })
                    if *command == id =>
                {
                    Some(Err(error.message.clone()))
                }
                _ => None,
            })
            .expect("command terminal")
    }
    fn query(&mut self, q: safe_cex_engine::Query) {
        self.e.handle(
            self.now,
            Event::Command(safe_cex_engine::Command::Query {
                command: 77,
                query: q,
            }),
        );
        self.collect();
    }
}
#[test]
fn stale_rest_snapshots_cannot_overwrite_newer_stream_state() {
    let mut r = Rig::ready();
    let socket = r.subscribe();
    let requests = r.poll_requests();
    let mut p = position_row();
    p["size_currency"] = json!(-0.2);
    let mut o = order_row();
    o["filled_amount"] = json!(0.1);
    o["order_state"] = json!("filled");
    o["last_update_timestamp"] = json!(1700000004000u64);
    r.notification(socket, CHANNELS[0], json!({"positions":[p],"orders":[o]}));
    let mut b = account_row();
    b["equity"] = json!(999.);
    r.notification(socket, CHANNELS[1], b);
    for req in requests {
        let value = if req.url.ends_with("get_positions") {
            json!([])
        } else if req.url.ends_with("get_open_orders") {
            json!([order_row()])
        } else {
            account_row()
        };
        r.rpc(req, value);
    }
    assert_eq!(r.e.store().positions()[0].contracts, 0.2);
    assert_eq!(
        r.e.store().order("USDC-123").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(r.e.store().balance().total, 999.);
}
#[test]
fn accepted_write_invalidates_older_order_snapshot() {
    let mut r = Rig::ready();
    let requests = r.poll_requests();
    r.e.store_mut().invalidate_order_snapshots();
    for req in requests {
        if req.url.ends_with("get_open_orders") {
            r.rpc(req, json!([]));
        }
    }
    assert!(
        r.e.store().order("USDC-123").is_some(),
        "a snapshot captured before write admission cannot remove the order"
    );
}
#[test]
fn websocket_position_without_liquidation_estimate_retains_rest_estimate() {
    let mut r = Rig::ready();
    let socket = r.subscribe();
    let mut p = position_row();
    p.as_object_mut()
        .unwrap()
        .remove("estimated_liquidation_price");
    p.as_object_mut().unwrap().remove("leverage");
    r.notification(socket, CHANNELS[0], json!({"positions":[p]}));
    assert_eq!(r.e.store().positions()[0].liquidation_price, 66000.);
    assert_eq!(r.e.store().positions()[0].leverage, 10.);
}
#[test]
fn malformed_stream_event_commits_no_component_and_closes_socket() {
    let mut r = Rig::ready();
    let socket = r.subscribe();
    let mut o = order_row();
    o["amount"] = json!(-1);
    let mut p = position_row();
    p["size_currency"] = json!(-0.2);
    r.notification(socket, CHANNELS[0], json!({"positions":[p],"orders":[o]}));
    assert_eq!(r.e.store().positions()[0].contracts, 0.1);
    assert!(
        r.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket:s} if *s==socket))
    );
}
#[test]
fn fills_pages_follow_native_sequence_and_deduplicate_archive_overlap() {
    let mut r = Rig::ready();
    r.query(safe_cex_engine::Query::Fills {
        symbol: Some("BTC_USDC-PERPETUAL".into()),
        order_id: None,
        from_ms: Some(1700000000000.),
        to_ms: Some(1700000003000.),
        limit: Some(10),
    });
    let req = r.request();
    assert!(req.url.ends_with("get_user_trades_by_instrument"));
    let body: Value = serde_json::from_str(req.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["params"]["start_timestamp"], 1700000000000.);
    r.rpc(
        req,
        json!({"trades":[trade_row("USDC-10",10)],"has_more":true}),
    );
    let req = r.request();
    let body: Value = serde_json::from_str(req.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["params"]["end_seq"], 10);
    r.rpc(
        req,
        json!({"trades":[trade_row("USDC-10",10),trade_row("USDC-9",9)],"has_more":false}),
    );
    let req = r.request();
    let body: Value = serde_json::from_str(req.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["params"]["historical"], true);
    assert!(body["params"].get("end_seq").is_none());
    r.rpc(
        req,
        json!({"trades":[trade_row("USDC-9",9)],"has_more":false}),
    );
    r.collect();
    assert_eq!(r.command_result(77).unwrap().as_array().unwrap().len(), 2);
}
#[test]
fn history_order_continuation_keeps_labels_out_of_identity_and_never_publishes_account_orders() {
    let mut r = Rig::ready();
    r.query(safe_cex_engine::Query::OrderHistory {
        symbol: None,
        from_ms: None,
        to_ms: None,
        limit: Some(10),
    });
    let req = r.request();
    assert!(req.url.ends_with("get_order_history_by_currency"));
    let mut o = order_row();
    o["order_state"] = json!("cancelled");
    r.rpc(req, json!({"orders":[o.clone()],"continuation":"next/+"}));
    let req = r.request();
    let body: Value = serde_json::from_str(req.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["params"]["continuation"], "next/+");
    r.rpc(req, json!({"orders":[],"continuation":null}));
    let req = r.request();
    r.rpc(req, json!([]));
    r.collect();
    let rows = r.command_result(77).unwrap();
    assert_eq!(rows[0]["id"], "USDC-123");
    assert_eq!(rows[0]["status"], "canceled");
    assert_eq!(
        r.e.store().order("USDC-123").unwrap().status,
        OrderStatus::Open
    );
}
#[test]
fn malformed_history_identity_fails_command_without_returning_partial_page() {
    let mut r = Rig::ready();
    r.query(safe_cex_engine::Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: Some(10),
    });
    let req = r.request();
    let mut bad = trade_row("USDC-10", 10);
    bad["fee_currency"] = json!("BTC");
    r.rpc(
        req,
        json!({"trades":[trade_row("USDC-9",9),bad],"has_more":false}),
    );
    r.collect();
    assert!(r.command_result(77).is_err());
}

#[test]
fn auth_refusal_never_claims_gateway_capability_or_sends_private_queries() {
    let mut r = Rig::new(false);
    r.select_intx();
    let req = r.request();
    let body: Value = serde_json::from_str(req.body.as_deref().unwrap()).unwrap();
    r.reply(
        req,
        json!({"jsonrpc":"2.0","id":body["id"],"error":{"code":10000,"message":"fixture refusal"}}),
    );
    r.collect();
    assert!(!r.e.venue().drb_authenticated());
    assert!(r.http.is_empty());
    assert!(!r.e.store().loaded().balance);
}
#[test]
fn successful_exchange_is_required_for_gateway_capability() {
    let mut r = Rig::new(false);
    r.select_intx();
    assert!(!r.e.venue().drb_authenticated());
    let req = r.request();
    r.rpc(req,json!({"access_token":"fixture","expires_in":900,"scope":"trade:read","token_type":"bearer"}));
    assert!(r.e.venue().drb_authenticated());
}
#[test]
fn wrong_rpc_id_cannot_complete_authentication() {
    let mut r = Rig::new(false);
    r.select_intx();
    let req = r.request();
    r.reply(req,json!({"jsonrpc":"2.0","id":99999,"result":{"access_token":"fixture","expires_in":900,"scope":"trade:read","token_type":"bearer"}}));
    r.collect();
    assert!(!r.e.venue().drb_authenticated());
    assert!(r.http.is_empty());
}
#[test]
fn spot_only_portfolio_keeps_advanced_trade_startup_and_never_authenticates_drb() {
    let mut r = Rig::new(true);
    let req = r.request();
    r.reply(
        req,
        json!({"portfolios":[{"uuid":"spot-one","type":"DEFAULT"}]}),
    );
    let req = r.request();
    r.reply(req, json!({"breakdown":{}}));
    let req = r.request();
    assert!(
        req.url.contains("api.coinbase.com") && !req.url.contains("PERPETUAL"),
        "{}",
        req.url
    );
    assert!(!r.e.venue().drb_authenticated());
}
#[test]
fn invalid_snapshot_keeps_all_previous_rows_and_empty_snapshot_clears_them() {
    let mut r = Rig::ready();
    let requests = r.poll_requests();
    for req in requests {
        if req.url.ends_with("get_positions") {
            r.rpc(req, json!([{"instrument_name":"BTC_USDC-PERPETUAL"}]));
        } else if req.url.ends_with("get_open_orders") {
            let mut bad = order_row();
            bad["filled_amount"] = json!(5);
            r.rpc(req, json!([bad]));
        } else {
            r.rpc(req, account_row());
        }
    }
    assert_eq!(r.e.store().positions()[0].contracts, 0.1);
    assert!(r.e.store().order("USDC-123").is_some());
    let requests = r.poll_requests();
    for req in requests {
        if req.url.ends_with("get_account_summary") {
            r.rpc(req, account_row());
        } else {
            r.rpc(req, json!([]));
        }
    }
    assert!(r.e.store().orders().is_empty());
    assert!(r.e.store().positions().iter().all(|p| p.contracts == 0.));
}
#[test]
fn heartbeat_test_request_is_answered_without_any_account_mutation() {
    let mut r = Rig::ready();
    let socket = r.subscribe();
    let before = r.e.store().positions().to_vec();
    r.e.handle(
        r.now,
        Event::WsMessage {
            socket,
            text: r#"{"jsonrpc":"2.0","method":"heartbeat","params":{"type":"test_request"}}"#,
        },
    );
    r.collect();
    assert!(r.actions.iter().any(|a| match a {
        Action::WsSend { socket: s, text } => *s == socket && text.contains("public/test"),
        _ => false,
    }));
    assert_eq!(r.e.store().positions(), before);
}
#[test]
fn reconnect_authenticates_a_fresh_socket_and_ignores_retired_notifications() {
    let mut r = Rig::ready();
    let socket = r.subscribe();
    r.e.handle(r.now, Event::WsClosed { socket, code: 1006 });
    r.collect();
    let mut b = account_row();
    b["equity"] = json!(9999.);
    r.notification(socket, CHANNELS[1], b);
    assert_eq!(r.e.store().balance().total, 1020.);
    for _ in 0..30 {
        r.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (at, timer) = r.timers.remove(0);
        r.now = r.now.max(at);
        r.e.handle(r.now, Event::TimerFired { timer });
        r.collect();
        if r.actions.iter().any(|a|matches!(a,Action::WsConnect{socket:s,url,..} if *s!=socket&&url==crate::drb::WS_URL)){break;}
    }
    let new = r.e.venue().private_drb.socket.expect("fresh socket");
    assert_ne!(new, socket);
    r.e.handle(r.now, Event::WsOpened { socket: new });
    r.collect();
    assert!(r.actions.iter().any(
        |a| matches!(a,Action::WsSend{socket:s,text} if *s==new&&text.contains("public/auth"))
    ));
}
#[test]
fn isolated_positions_keep_distinct_subaccount_identity_and_margin_mode() {
    let mut a = position_row();
    a["isolated"] = json!(true);
    a["user_id"] = json!(1234);
    let p = position(a).unwrap();
    assert_eq!(p.id, "BTC_USDC-PERPETUAL:1234");
    assert_eq!(p.margin_type, Some(MarginType::Isolated));
}
#[test]
fn otoco_child_and_market_trigger_prices_remain_visible_as_independent_orders() {
    let mut r = Rig::ready();
    let socket = r.subscribe();
    let mut child = order_row();
    child["order_id"] = json!("USDC-stop");
    child["primary_order_id"] = json!("USDC-123");
    child["order_type"] = json!("stop_market");
    child["order_state"] = json!("untriggered");
    child["price"] = json!("market_price");
    child["trigger_price"] = json!(55000.);
    child["trigger"] = json!("index_price");
    r.notification(socket, CHANNELS[0], json!({"orders":[child]}));
    let o = r.e.store().order("USDC-stop").unwrap();
    assert_eq!(o.parent_id.as_deref(), Some("USDC-123"));
    assert_eq!(o.price, 55000.);
    assert_eq!(o.trigger_type, Some(StopTriggerType::IndexPrice));
    assert_eq!(r.e.store().orders().len(), 2);
}

#[test]
fn shared_http_session_keeps_its_engine_identity_across_fresh_cdp_refresh() {
    let mut r = Rig::ready();
    let owner =
        r.e.venue()
            .drb_http_session()
            .expect("authenticated HTTP session")
            .owner();
    let revision = r.e.venue().private_drb.bearer_revision;
    let timer = r.e.venue().private_drb.timer.unwrap();
    r.now += 875000.;
    r.e.handle(r.now, Event::TimerFired { timer });
    r.collect();
    assert!(
        r.e.venue().drb_http_session().is_none(),
        "expired token cannot bind a trading request"
    );
    let req = r.request();
    assert!(req.url.ends_with("public/auth"));
    r.rpc(req,json!({"access_token":"refreshed-fixture","expires_in":900,"scope":"trade:read","token_type":"bearer"}));
    assert_eq!(r.e.venue().drb_http_session().unwrap().owner(), owner);
    assert!(r.e.venue().private_drb.bearer_revision > revision);
}

impl Rig {
    fn rpc_error(&mut self, request: HttpRequest, code: i64, message: &str) {
        let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        self.reply(
            request,
            json!({"jsonrpc":"2.0","id":body["id"],"error":{"code":code,"message":message}}),
        );
        self.collect();
    }
    fn command_error(&self, id: u32) -> &VenueError {
        self.actions
            .iter()
            .find_map(|action| match action {
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command, error })
                    if *command == id =>
                {
                    Some(error)
                }
                _ => None,
            })
            .expect("typed command failure")
    }
}
#[test]
fn native_auth_refusal_reports_typed_startup_failure_and_never_retries() {
    let mut r = Rig::new(false);
    r.select_intx();
    let request = r.request();
    r.rpc_error(request, 10000, "authorization_required");
    let error = r
        .actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::StartupFailed { error, .. }) => Some(error),
            _ => None,
        })
        .expect("native auth refusal must retain the engine startup refusal");
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.venue_code.as_deref(), Some("10000"));
    assert_eq!(
        error.venue_message.as_deref(),
        Some("authorization_required")
    );
    for _ in 0..20 {
        r.collect();
        if r.timers.is_empty() {
            break;
        }
        r.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (at, timer) = r.timers.remove(0);
        r.now = r.now.max(at);
        r.e.handle(r.now, Event::TimerFired { timer });
    }
    r.collect();
    assert!(
        r.http.is_empty(),
        "a refused key must not retry authentication"
    );
    assert_eq!(
        r.actions
            .iter()
            .filter(|a| matches!(
                a,
                Action::Notice(safe_cex_engine::Notice::StartupFailed { .. })
            ))
            .count(),
        1
    );
}
#[test]
fn native_history_rate_limit_preserves_kind_code_and_native_message() {
    let mut r = Rig::ready();
    r.query(safe_cex_engine::Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: Some(10),
    });
    let request = r.request();
    r.rpc_error(request, 10028, "too_many_requests");
    let error = r.command_error(77);
    assert_eq!(error.kind, Some(ErrorKind::RateLimited));
    assert_eq!(error.venue_code.as_deref(), Some("10028"));
    assert_eq!(error.venue_message.as_deref(), Some("too_many_requests"));
    assert_eq!(
        error.outcome, None,
        "a Read classification grants no write retry authority"
    );
}
#[test]
fn history_transport_failure_preserves_network_classification() {
    let mut r = Rig::ready();
    r.query(safe_cex_engine::Query::Fills {
        symbol: None,
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: Some(10),
    });
    let request = r.request();
    r.e.handle(
        r.now,
        Event::HttpFailed {
            request: request.id,
            error: "offline fixture connection lost",
        },
    );
    r.collect();
    let error = r.command_error(77);
    assert_eq!(error.kind, Some(ErrorKind::Network));
    assert_eq!(error.venue_code, None);
    assert_eq!(error.outcome, None);
}
#[test]
fn documented_original_order_types_preserve_market_intent_and_conditional_types() {
    // OpenAPI original_order_type is optional and closed to market/market_limit.
    // Cross every documented native order type with both original variants.
    for native in [
        "limit",
        "market",
        "market_limit",
        "stop_limit",
        "stop_market",
        "take_limit",
        "take_market",
        "trailing_stop",
    ] {
        for original in [None, Some("market"), Some("market_limit")] {
            let mut row = order_row();
            row["order_type"] = json!(native);
            if let Some(original) = original {
                row["original_order_type"] = json!(original);
            }
            let conditional = matches!(
                native,
                "stop_limit" | "stop_market" | "take_limit" | "take_market" | "trailing_stop"
            );
            if conditional {
                row["trigger_price"] = json!(55000.);
                row["trigger"] = json!("mark_price");
            }
            let expected = match native {
                "limit" if original.is_none() => OrderType::Limit,
                "limit" | "market" | "market_limit" => OrderType::Market,
                "stop_limit" | "stop_market" => OrderType::StopLoss,
                "take_limit" | "take_market" => OrderType::TakeProfit,
                "trailing_stop" => OrderType::TrailingStopLoss,
                _ => unreachable!(),
            };
            let mapped = order(row).unwrap();
            assert_eq!(mapped.kind, expected, "{native} / {original:?}");
            assert_eq!(mapped.trigger_type.is_some(), conditional);
        }
    }
    for original in [
        json!("limit"),
        json!("stop_market"),
        json!("unknown"),
        json!(42),
        Value::Null,
    ] {
        let mut row = order_row();
        row["original_order_type"] = original;
        assert!(
            order(row).is_err(),
            "unknown original types cannot fall back to Limit"
        );
    }
}
#[test]
fn live_internal_limits_display_original_market_type() {
    let mut r = Rig::ready();
    let socket = r.subscribe();
    let mut row = order_row();
    row["original_order_type"] = json!("market");
    row["last_update_timestamp"] = json!(1700000002000u64);
    r.notification(socket, CHANNELS[0], json!({"orders":[row]}));
    assert_eq!(
        r.e.store().order("USDC-123").unwrap().kind,
        OrderType::Market
    );
}
#[test]
fn historical_internal_limits_display_original_market_type() {
    let mut r = Rig::ready();
    r.query(safe_cex_engine::Query::OrderHistory {
        symbol: None,
        from_ms: None,
        to_ms: None,
        limit: Some(1),
    });
    let request = r.request();
    assert!(request.url.ends_with("get_order_history_by_currency"));
    let mut row = order_row();
    row["original_order_type"] = json!("market");
    row["order_state"] = json!("filled");
    row["filled_amount"] = row["amount"].clone();
    r.rpc(request, json!({"orders":[row],"continuation":null}));
    r.collect();
    assert_eq!(r.command_result(77).unwrap()[0]["type"], "market");
}

#[test]
fn authenticated_gateway_suppresses_legacy_notice_before_account_ready() {
    use safe_cex_engine::test_support::ProbeVenue;
    let mut venue = Coinbase::new(Options::default()).unwrap();
    venue.private_drb.active = true;
    let probe = ProbeVenue::new(venue, |venue: &mut Coinbase, cx: &mut Cx<'_, Pending>| {
        assert!(!venue.fail_moved_intx_startup(cx, true));
        assert!(
            !venue.intx_startup_failed,
            "suppression must precede the terminal latch"
        );
    });
    let mut e = safe_cex_engine::test_support::seeded_engine(probe);
    e.start(1000.);
    assert!(
        !safe_cex_engine::test_support::prepared_actions(&mut e, 1000.)
            .iter()
            .any(|a| matches!(
                a,
                Action::Notice(safe_cex_engine::Notice::StartupFailed { .. })
            ))
    );
}

#[test]
fn private_state_uses_the_same_symbols_as_the_public_catalogue() {
    let r = Rig::ready();
    let store = r.e.store();
    assert!(store.loaded().markets && store.loaded().tickers);
    for order in store.orders() {
        assert!(store.market(&order.symbol).is_some(), "{}", order.symbol);
    }
    for position in store.positions() {
        assert!(
            store.market(&position.symbol).is_some(),
            "{}",
            position.symbol
        );
    }
    assert_eq!(store.orders().len(), 1);
    assert_eq!(store.positions().len(), 1);
}

#[test]
fn authenticated_gateway_catalogue_failure_never_reports_legacy_migration() {
    let mut r = Rig::new(false);
    r.select_intx();
    let auth = r.request();
    r.rpc(auth, json!({"access_token":"fixture-http-token","expires_in":900,"scope":"trade:read","token_type":"bearer"}));
    let instruments = r.request();
    assert!(instruments.url.ends_with("/public/get_instruments"));
    r.e.handle(
        r.now,
        Event::HttpResponse {
            request: instruments.id,
            response: Response {
                status: 502,
                headers: "",
                body: "Bad Gateway",
            },
        },
    );
    r.collect();
    assert!(r.e.venue().drb_authenticated());
    assert!(!r.e.venue().intx_startup_failed);
    assert!(!r.actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::StartupFailed { .. })
    )));
    let retry = r.request();
    assert!(retry.url.ends_with("/public/get_instruments"));
}
