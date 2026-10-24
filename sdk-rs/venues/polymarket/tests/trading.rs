use safe_cex_engine::Engine;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, Response, SocketId};
use safe_cex_venue_polymarket::{Options, Polymarket, PolymarketOptions};
use serde_json::{Value, json};

// Documented CLOB fixtures; no credentials or live write traffic.
const KEY: &str = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const ADDRESS: &str = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";
const MARKETS: &str = r#"[{"conditionId":"condition","slug":"election","clobTokenIds":"[\"1\",\"2\"]","outcomes":"[\"Yes\",\"No\"]","active":true,"closed":false,"orderPriceMinTickSize":0.01,"orderMinSize":5}]"#;
struct H {
    e: Engine<Polymarket>,
    now: f64,
    http: Vec<HttpRequest>,
    done: Vec<(u32, Result<String, String>)>,
    failures: Vec<safe_cex_engine::VenueError>,
    private: Option<SocketId>,
}
impl H {
    fn new() -> Self {
        Self::configured(0, None)
    }
    fn configured(signer_type: u8, funder: Option<&str>) -> Self {
        let mut h = Self {
            e: safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options {
                key: Some(KEY.into()),
                polymarket: Some(PolymarketOptions {
                    api_key: Some("api-test".into()),
                    api_secret: Some("c2VjcmV0".into()),
                    api_passphrase: Some("pass-test".into()),
                    signer_type,
                    funder_address: funder.map(str::to_owned),
                    market_slugs: None,
                }),
                ..Default::default()
            })),
            now: 1700000000123.0,
            http: vec![],
            done: vec![],
            failures: vec![],
            private: None,
        };
        h.e.start(h.now);
        h.collect();
        h.response("/markets/keyset", 200, MARKETS);
        h.response("/midpoints", 200, r#"{"1":"0.5","2":"0.5"}"#);
        h.response("/balance-allowance", 200, r#"{"balance":"1000000000"}"#);
        h.response(
            "/data/orders",
            200,
            &format!(
                r#"{{"data":[{}],"next_cursor":"LTE="}}"#,
                detail("o1", "LIVE", 2.0)
            ),
        );
        h.response("/positions", 200, "[]");
        h
    }
    fn collect(&mut self) {
        for a in safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now) {
            match a {
                Action::Http(r) => self.http.push(r),
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.done.push((command, Ok(result)))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.done.push((command, Err(error.legacy_text())));
                    self.failures.push(error);
                }
                Action::WsConnect {
                    socket,
                    channel: Some(safe_cex_engine::SocketChannel::Private),
                    ..
                } => self.private = Some(socket),
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.e.handle(self.now, event);
        self.collect();
    }
    fn command(&mut self, c: Command) {
        self.event(Event::Command(c));
    }
    fn take(&mut self, path: &str) -> HttpRequest {
        let i = self
            .http
            .iter()
            .position(|r| r.url.ends_with(path) || r.url.contains(&format!("{path}?")))
            .unwrap_or_else(|| panic!("missing{path}: {:?}", self.http));
        self.http.remove(i)
    }
    fn respond(&mut self, r: HttpRequest, status: u16, body: &str) {
        self.event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn response(&mut self, path: &str, status: u16, body: &str) {
        let wrapped = (path.contains("/markets") && body.trim_start().starts_with('['))
            .then(|| format!(r#"{{"markets":{body}}}"#));
        let body = wrapped.as_deref().unwrap_or(body);
        let r = self.take(path);
        self.respond(r, status, body);
    }
    fn ws(&mut self, text: &str) {
        let socket = self.private.unwrap();
        self.event(Event::WsOpened { socket });
        self.event(Event::WsMessage { socket, text });
    }
    fn writes(&self) -> usize {
        self.http
            .iter()
            .filter(|r| matches!(r.method, Method::Post | Method::Delete))
            .count()
    }
}
fn order() -> PlaceOrderOpts {
    serde_json::from_value(
        json!({"symbol":"election-Yes","type":"limit","side":"buy","amount":10,"price":0.5}),
    )
    .unwrap()
}
fn detail(id: &str, status: &str, filled: f64) -> String {
    let trades = if filled > 0.0 {
        vec!["prior-fill"]
    } else {
        vec![]
    };
    detail_with_trades(id, status, filled, &trades)
}
fn detail_with_trades(id: &str, status: &str, filled: f64, trades: &[&str]) -> String {
    json!({"id":id,"asset_id":"1","side":"BUY","price":"0.5","original_size":"10","size_matched":filled.to_string(),"associate_trades":trades,"status":status,"order_type":"GTC","created_at":"1700000000"}).to_string()
}
fn one_share_execution(h: &mut H, maker: bool) {
    h.ws(if maker {
        r#"{"event_type":"trade","id":"new-execution","taker_order_id":"foreign","asset_id":"1","side":"SELL","size":"1","price":"0.5","status":"MATCHED","trader_side":"MAKER","maker_orders":[{"order_id":"o1","owner":"api-test","asset_id":"1","side":"BUY","matched_amount":"1","price":"0.5"}]}"#
    } else {
        r#"{"event_type":"trade","id":"new-execution","taker_order_id":"o1","asset_id":"1","side":"BUY","size":"1","price":"0.5","status":"MATCHED","owner":"api-test","trader_side":"TAKER"}"#
    });
}
fn replace(h: &mut H, command: u32) {
    h.command(Command::AmendOrder {
        command,
        id: "o1".into(),
        price: Some(0.6),
        amount: None,
    });
    h.response("/order", 200, r#"{"canceled":["o1"],"not_canceled":{}}"#);
}
fn ack(id: &str) -> String {
    json!({"success":true,"orderID":id,"status":"live","errorMsg":""}).to_string()
}

#[test]
fn placement_matches_official_json_v2_and_independent_typed_data_and_hmac() {
    let mut h = H::new();
    let mut opts = order();
    opts.price = Some(0.56789);
    opts.amount = 5.129;
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![opts.clone(), opts],
    });
    assert_eq!(h.writes(), 1);
    let first = h.take("/order");
    assert_eq!(first.method, Method::Post);
    let raw = first.body.as_ref().unwrap();
    let body: Value = serde_json::from_str(raw).unwrap();
    let o = &body["order"];
    assert_eq!(body["deferExec"], false);
    assert_eq!(body["postOnly"], false);
    assert_eq!(body["owner"], "api-test");
    assert_eq!(body["orderType"], "GTC");
    assert_eq!(body.as_object().unwrap().len(), 5);
    assert_eq!(o.as_object().unwrap().len(), 13);
    let salt = o["salt"].as_u64().unwrap();
    assert!((1..=9_007_199_254_740_991).contains(&salt));
    assert_eq!(o["timestamp"], "1700000000123");
    assert_eq!(o["maker"], ADDRESS);
    assert_eq!(o["signer"], ADDRESS);
    assert_eq!(o["makerAmount"], "2924100");
    assert_eq!(o["takerAmount"], "5130000");
    assert_eq!(o["expiration"], "0");
    assert_eq!(o["side"], "BUY");
    assert_eq!(o["signatureType"], 0);
    assert!(o.get("taker").is_none());
    // Generic dynamic EIP-712 hashing is independent from the venue's direct ABI encoder.
    let typed = json!({"types":{"EIP712Domain":[{"name":"name","type":"string"},{"name":"version","type":"string"},{"name":"chainId","type":"uint256"},{"name":"verifyingContract","type":"address"}],"Order":[{"name":"salt","type":"uint256"},{"name":"maker","type":"address"},{"name":"signer","type":"address"},{"name":"tokenId","type":"uint256"},{"name":"makerAmount","type":"uint256"},{"name":"takerAmount","type":"uint256"},{"name":"side","type":"uint8"},{"name":"signatureType","type":"uint8"},{"name":"timestamp","type":"uint256"},{"name":"metadata","type":"bytes32"},{"name":"builder","type":"bytes32"}]},"primaryType":"Order","domain":{"name":"Polymarket CTF Exchange","version":"2","chainId":137,"verifyingContract":"0xE111180000d2663C0091e4f400237545B87B996B"},"message":{"salt":salt.to_string(),"maker":o["maker"],"signer":o["signer"],"tokenId":o["tokenId"],"makerAmount":o["makerAmount"],"takerAmount":o["takerAmount"],"side":0,"signatureType":0,"timestamp":o["timestamp"],"metadata":o["metadata"],"builder":o["builder"]}});
    let digest = serde_json::from_value::<alloy_dyn_abi::TypedData>(typed)
        .unwrap()
        .eip712_signing_hash()
        .unwrap();
    assert_eq!(
        safe_cex_crypto::ethereum::sign_digest(digest.as_slice(), KEY).unwrap(),
        o["signature"]
    );
    let signature = safe_cex_crypto::hmac_sha256(
        b"secret",
        format!("1700000000POST/order{raw}").as_bytes(),
        safe_cex_crypto::Encoding::Base64,
    )
    .replace('+', "-")
    .replace('/', "_");
    assert!(first.headers.contains(&("POLY_SIGNATURE", signature)));
    h.respond(first, 200, &ack("p1"));
    assert_eq!(h.writes(), 1);
    assert!(h.done.is_empty());
    let second = h.take("/order");
    let second_body: Value = serde_json::from_str(second.body.as_ref().unwrap()).unwrap();
    assert_ne!(o["salt"], second_body["order"]["salt"]);
    h.respond(second, 200, &ack("p2"));
    assert_eq!(h.done.last().unwrap(), &(1, Ok(r#"["p1","p2"]"#.into())));
}

#[test]
fn whole_batch_rejects_unsupported_shapes_before_any_write() {
    let mutations = [
        json!({"clientOrderId":""}),
        json!({"type":"market"}),
        json!({"type":"stop_market"}),
        json!({"type":"conditional"}),
        json!({"type":"take_profit_market"}),
        json!({"type":"trailing_stop_market"}),
        json!({"triggerPrice":0.4}),
        json!({"maxSlippagePct":0.5}),
        json!({"stopLoss":0.3}),
        json!({"takeProfit":0.8}),
        json!({"reduceOnly":true}),
        json!({"close":true}),
        json!({"triggerType":"mark_price"}),
        json!({"tpTriggerType":"mark_price"}),
        json!({"slTriggerType":"last_price"}),
        json!({"timeInForce":"PostOnly"}),
        json!({"timeInForce":"FillOrKill"}),
        json!({"timeInForce":"ImmediateOrCancel"}),
        json!({"bybitBbo":{"sideType":"Queue","level":1}}),
        json!({"bybitSlippageToleranceTicks":2}),
        json!({"price":0}),
        json!({"price":1}),
        json!({"amount":1}),
        json!({"amount":1000001}),
        json!({"symbol":"unknown"}),
    ];
    for mutation in mutations {
        let mut h = H::new();
        let mut value = serde_json::to_value(order()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .extend(mutation.as_object().unwrap().clone());
        let invalid: PlaceOrderOpts = serde_json::from_value(value).unwrap();
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(), invalid],
        });
        assert_eq!(h.writes(), 0, "{mutation}");
        assert!(h.done.last().unwrap().1.is_err(), "{mutation}");
    }
}

#[test]
fn ambiguous_post_failure_never_retries_and_rejected_ack_never_creates_order() {
    let mut h = H::new();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![order(), order()],
    });
    let r = h.take("/order");
    h.event(Event::HttpFailed {
        request: r.id,
        error: "lost response",
    });
    assert_eq!(h.writes(), 0);
    assert!(h.done.last().unwrap().1.is_err());
    h.command(Command::PlaceOrders {
        command: 2,
        orders: vec![order()],
    });
    h.response(
        "/order",
        200,
        r#"{"success":false,"orderID":"rejected","errorMsg":"no balance","status":"live"}"#,
    );
    assert!(h.e.store().order("rejected").is_none());
    assert!(h.done.last().unwrap().1.is_err());
    assert_eq!(h.writes(), 0);
}

#[test]
fn earlier_private_partial_fill_and_terminal_events_survive_late_placement_ack() {
    let mut h = H::new();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![order()],
    });
    h.ws(&format!(
        r#"{{"event_type":"order","type":"UPDATE",{}}}"#,
        &detail("p1", "LIVE", 3.0)[1..detail("p1", "LIVE", 3.0).len() - 1]
    ));
    h.response("/order", 200, &ack("p1"));
    assert_eq!(h.e.store().order("p1").unwrap().filled, 3.0);
    h.command(Command::PlaceOrders {
        command: 2,
        orders: vec![order()],
    });
    h.ws(r#"{"event_type":"order","type":"CANCELLATION","id":"p2"}"#);
    h.response("/order", 200, &ack("p2"));
    assert!(h.e.store().order("p2").is_none());
}

#[test]
fn cancel_partial_failure_is_reported_and_confirmed_ids_are_removed() {
    let mut h = H::new();
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["o1".into(), "other".into()],
    });
    let r = h.take("/orders");
    assert_eq!(r.method, Method::Delete);
    assert_eq!(r.body.as_deref(), Some(r#"["o1","other"]"#));
    h.respond(
        r,
        200,
        r#"{"canceled":["o1"],"not_canceled":{"other":"not found"}}"#,
    );
    assert!(h.done.last().unwrap().1.is_err());
    assert!(h.e.store().order("o1").is_none());
    assert_eq!(h.writes(), 0);
}

#[test]
fn cancellation_by_symbol_uses_only_token_scope_and_empty_id_list_has_no_write() {
    let mut h = H::new();
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec![],
    });
    assert_eq!(h.writes(), 0);
    h.command(Command::CancelSymbolOrders {
        command: 2,
        symbol: "election-Yes".into(),
    });
    let r = h.take("/cancel-market-orders");
    assert_eq!(r.body.as_deref(), Some(r#"{"asset_id":"1"}"#));
    assert_eq!(r.method, Method::Delete);
    h.respond(r, 200, r#"{"canceled":["o1"],"not_canceled":{}}"#);
    assert!(h.done.last().unwrap().1.is_ok());
    assert!(h.e.store().order("o1").is_none());
}

#[test]
fn amend_waits_for_terminal_final_fill_proof_and_replaces_target_total_minus_fills() {
    let mut h = H::new();
    replace(&mut h, 1);
    let r = h.take("/data/order/o1");
    assert_eq!(r.method, Method::Get);
    assert_eq!(h.writes(), 0);
    h.respond(r, 200, &detail("o1", "LIVE", 3.0));
    assert_eq!(h.writes(), 0);
    h.response("/data/order/o1", 200, &detail("o1", "CANCELED", 4.0));
    let r = h.take("/order");
    let b: Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
    assert_eq!(b["order"]["takerAmount"], "6000000");
    assert_eq!(b["order"]["makerAmount"], "3600000");
    h.respond(r, 200, &ack("replacement"));
    assert_eq!(
        h.done.last().unwrap(),
        &(1, Ok(r#"["replacement"]"#.into()))
    );
}

#[test]
fn fully_filled_and_incomplete_final_proofs_never_create_replacement() {
    for proof in [
        detail("o1", "MATCHED", 10.0),
        detail("wrong-id", "CANCELED", 2.0),
        r#"{"id":"o1","status":"CANCELED"}"#.into(),
        detail("o1", "CANCELED", 7.0),
    ] {
        let mut h = H::new();
        replace(&mut h, 1);
        h.response("/data/order/o1", 200, &proof);
        assert_eq!(h.writes(), 0, "{proof}");
        assert!(!h.done.is_empty(), "{proof}");
        if proof.contains("MATCHED") {
            assert_eq!(h.done.last().unwrap().1, Ok("[]".into()));
        } else {
            assert!(h.done.last().unwrap().1.is_err());
        }
    }
    let mut h = H::new();
    replace(&mut h, 1);
    for _ in 0..3 {
        h.response("/data/order/o1", 200, &detail("o1", "LIVE", 2.0));
    }
    assert!(h.done.last().unwrap().1.is_err());
    assert!(h.http.is_empty());
}

#[test]
fn amend_preflight_and_conflicting_mutations_fail_before_cancel() {
    let mut h = H::new();
    h.command(Command::AmendOrder {
        command: 1,
        id: "o1".into(),
        price: Some(1.0),
        amount: None,
    });
    assert_eq!(h.writes(), 0);
    assert!(h.done.last().unwrap().1.is_err());
    h.command(Command::AmendOrder {
        command: 2,
        id: "o1".into(),
        price: Some(0.6),
        amount: None,
    });
    assert_eq!(h.writes(), 1);
    h.command(Command::CancelOrders {
        command: 3,
        ids: vec!["o1".into()],
    });
    assert_eq!(h.writes(), 1);
    assert!(h.done.last().unwrap().1.is_err());
    h.response(
        "/order",
        200,
        r#"{"canceled":[],"not_canceled":{"o1":"still live"}}"#,
    );
    assert_eq!(h.writes(), 0);
    assert!(h.done.last().unwrap().1.is_err());
    assert!(h.e.store().order("o1").is_some());
}

#[test]
fn account_funder_and_all_signature_types_are_preserved_on_the_wire() {
    for kind in 0..=3 {
        let funder = (kind > 0).then_some("0x1111111111111111111111111111111111111111");
        let mut h = H::configured(kind, funder);
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        });
        let r = h.take("/order");
        let b: Value = serde_json::from_str(r.body.as_ref().unwrap()).unwrap();
        assert_eq!(b["order"]["signatureType"], kind);
        assert_eq!(b["order"]["maker"], funder.unwrap_or(ADDRESS));
        assert_eq!(
            b["order"]["signer"],
            if kind == 3 { funder.unwrap() } else { ADDRESS }
        );
        let signature = b["order"]["signature"].as_str().unwrap();
        assert_eq!(signature.len(), if kind == 3 { 636 } else { 132 });
        assert!(r.headers.contains(&("POLY_ADDRESS", ADDRESS.into())));
    }
}

#[test]
fn late_terminal_user_notifications_dominate_older_rest_final_fill_proof() {
    for (kind, filled) in [("CANCELLATION", 5.0), ("UPDATE", 10.0)] {
        let mut h = H::new();
        replace(&mut h, 1);
        let row = detail(
            "o1",
            if kind == "UPDATE" {
                "MATCHED"
            } else {
                "CANCELED"
            },
            filled,
        );
        h.ws(&format!(
            r#"{{"event_type":"order","type":"{kind}",{}}}"#,
            &row[1..row.len() - 1]
        ));
        h.response("/data/order/o1", 200, &detail("o1", "CANCELED", 3.0));
        assert_eq!(h.writes(), 0);
        h.response("/data/order/o1", 200, &detail("o1", "CANCELED", filled));
        if filled == 10.0 {
            assert_eq!(h.writes(), 0);
            assert_eq!(h.done.last().unwrap().1, Ok("[]".into()));
        } else {
            let request = h.take("/order");
            let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
            assert_eq!(body["order"]["takerAmount"], "5000000");
            assert_eq!(body["order"]["makerAmount"], "3000000");
        }
        assert!(h.e.store().order("o1").is_none());
    }
}

#[test]
fn ambiguous_cancel_failure_is_not_retried_and_unlocks_the_old_order() {
    let mut h = H::new();
    h.command(Command::AmendOrder {
        command: 1,
        id: "o1".into(),
        price: Some(0.6),
        amount: None,
    });
    let request = h.take("/order");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "lost cancellation response",
    });
    assert_eq!(h.writes(), 0);
    assert!(h.done.last().unwrap().1.is_err());
    h.command(Command::CancelOrders {
        command: 2,
        ids: vec!["o1".into()],
    });
    assert_eq!(h.writes(), 1);
    h.response("/orders", 503, r#"{"error":"unavailable"}"#);
    assert!(h.done.last().unwrap().1.is_err());
    assert_eq!(h.writes(), 0);
    assert!(h.e.store().order("o1").is_some());
}

#[test]
fn inactive_market_and_nonfinite_batch_member_fail_before_write() {
    let mut h = H::new();
    h.e.store_mut()
        .update_markets(|market| market.active = false);
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![order()],
    });
    assert_eq!(h.writes(), 0);
    assert!(h.done.last().unwrap().1.is_err());
    for (amount, price) in [(f64::NAN, 0.5), (10.0, f64::INFINITY)] {
        let mut h = H::new();
        let mut bad = order();
        bad.amount = amount;
        bad.price = Some(price);
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(), bad],
        });
        assert_eq!(h.writes(), 0);
        assert!(h.done.last().unwrap().1.is_err());
    }
}

#[test]
fn financial_price_tick_and_share_rounding_precede_official_builder_rounding() {
    // Price aligns to the native tick and shares to the 0.01 precision.
    // Decimal midpoint amounts use the engine's common financial half-up rule.
    for (raw_size, expected_shares, expected_collateral) in [
        (5.999, "6000000", "3180000"),
        (5.005, "5010000", "2655300"),
        (5.125, "5130000", "2718900"),
        (5.129, "5130000", "2718900"),
    ] {
        let mut h = H::new();
        let mut metadata = h.e.store().prediction_markets().unwrap().to_vec();
        metadata[0].tick_size = 0.005;
        h.e.store_mut().set_prediction_markets(metadata);
        let mut input = order();
        input.amount = raw_size;
        input.price = Some(0.532);
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![input],
        });
        let request = h.take("/order");
        let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(
            body["order"]["takerAmount"], expected_shares,
            "size={raw_size}"
        );
        assert_eq!(
            body["order"]["makerAmount"], expected_collateral,
            "size={raw_size}"
        );
        h.respond(request, 200, &ack("rounded"));
        assert_eq!(h.e.store().order("rounded").unwrap().price, 0.53);
    }
}

#[test]
fn partial_batch_errors_return_every_acknowledged_id_for_network_and_venue_failures() {
    for network in [false, true] {
        let mut h = H::new();
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(), order()],
        });
        h.response("/order", 200, &ack("first-accepted"));
        let request = h.take("/order");
        if network {
            h.event(Event::HttpFailed {
                request: request.id,
                error: "lost second response",
            });
        } else {
            h.respond(
                request,
                200,
                r#"{"success":false,"orderID":"","errorMsg":"insufficient funds","status":""}"#,
            );
        }
        let error = h.done.last().unwrap().1.as_ref().unwrap_err();
        let result: Value = serde_json::from_str(error).unwrap();
        assert_eq!(result["partialOrderIds"], json!(["first-accepted"]));
        assert_eq!(result["partialOrderCount"], 1);
        assert_eq!(result["totalOrderCount"], 2);
        assert_eq!(result["failedOrderIndex"], 1);
        assert!(result["message"].as_str().unwrap().contains(if network {
            "lost second"
        } else {
            "insufficient"
        }));
        assert!(h.e.store().order("first-accepted").is_some());
        assert_eq!(h.writes(), 0);
    }
}

#[test]
fn matched_ack_does_not_fence_partially_filled_gtc_before_or_after_user_update() {
    for update_first in [false, true] {
        let mut h = H::new();
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        });
        let row = detail("partial", "LIVE", 3.0);
        let update = format!(
            r#"{{"event_type":"order","type":"UPDATE",{}}}"#,
            &row[1..row.len() - 1]
        );
        if update_first {
            h.ws(&update);
        }
        h.response(
            "/order",
            200,
            r#"{"success":true,"orderID":"partial","errorMsg":"","status":"matched"}"#,
        );
        assert!(
            h.http
                .iter()
                .any(|request| request.url.contains("/data/orders"))
        );
        if !update_first {
            h.ws(&update);
        }
        let order = h.e.store().order("partial").unwrap();
        assert_eq!(order.status, OrderStatus::Open);
        assert_eq!(order.filled, 3.0);
        assert_eq!(order.remaining, 7.0);
        // The matched ACK's older in-flight snapshot cannot erase a newer user update.
        h.response("/data/orders", 200, r#"{"data":[],"next_cursor":"LTE="}"#);
        let order = h.e.store().order("partial").unwrap();
        assert_eq!(order.remaining, 7.0);
    }
}

#[test]
fn own_trade_execution_requires_coherent_terminal_rest_proof_before_any_replacement() {
    for catch_up in [false, true] {
        let mut h = H::new();
        replace(&mut h, 1);
        // Three new shares plus the two-share bootstrap baseline require a final five.
        h.ws(r#"{"event_type":"trade","id":"late-execution","taker_order_id":"o1","asset_id":"1","side":"BUY","size":"3","price":"0.5","status":"MATCHED","owner":"api-test","trade_owner":"api-test","trader_side":"TAKER"}"#);
        h.response("/data/order/o1", 200, &detail("o1", "CANCELED", 2.0));
        assert_eq!(h.writes(), 0);
        if catch_up {
            h.response(
                "/data/order/o1",
                200,
                &detail_with_trades("o1", "CANCELED", 5.0, &["prior-fill", "late-execution"]),
            );
            let request = h.take("/order");
            let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
            assert_eq!(body["order"]["takerAmount"], "5000000");
        } else {
            for _ in 0..2 {
                h.response("/data/order/o1", 200, &detail("o1", "CANCELED", 2.0));
            }
            assert_eq!(h.writes(), 0);
            assert!(h.done.last().unwrap().1.is_err());
            assert!(
                !h.http
                    .iter()
                    .any(|request| request.url.ends_with("/data/order/o1"))
            );
        }
    }
}

#[test]
fn disjoint_execution_advances_bootstrap_fills_and_requires_terminal_catchup() {
    for maker in [false, true] {
        let mut h = H::new();
        assert_eq!(h.e.store().order("o1").unwrap().filled, 2.0);
        one_share_execution(&mut h, maker);
        let live = h.e.store().order("o1").unwrap();
        assert_eq!(live.filled, 3.0, "maker={maker}");
        assert_eq!(live.remaining, 7.0, "maker={maker}");

        replace(&mut h, 1);
        h.response("/data/order/o1", 200, &detail("o1", "CANCELED", 2.0));
        assert_eq!(h.writes(), 0, "maker={maker}: stale terminal quantity");
        h.response(
            "/data/order/o1",
            200,
            &detail_with_trades("o1", "CANCELED", 3.0, &["prior-fill", "new-execution"]),
        );
        let request = h.take("/order");
        let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["order"]["takerAmount"], "7000000", "maker={maker}");
        assert_eq!(body["order"]["makerAmount"], "4200000", "maker={maker}");
        h.respond(request, 200, &ack("replacement"));
        assert_eq!(
            h.done.last().unwrap(),
            &(1, Ok(r#"["replacement"]"#.into()))
        );
    }
}

#[test]
fn delayed_execution_does_not_erase_an_earlier_disjoint_cumulative_observation() {
    for maker in [false, true] {
        let mut h = H::new();
        let row = detail_with_trades("o1", "LIVE", 2.0, &["prior-fill", "new-execution"]);
        h.ws(&format!(
            r#"{{"event_type":"order","type":"UPDATE",{}}}"#,
            &row[1..row.len() - 1]
        ));
        one_share_execution(&mut h, maker);
        assert_eq!(
            h.e.store().order("o1").unwrap().filled,
            3.0,
            "maker={maker}"
        );
        replace(&mut h, 1);
        h.response(
            "/data/order/o1",
            200,
            &detail_with_trades("o1", "CANCELED", 2.0, &["prior-fill", "new-execution"]),
        );
        assert_eq!(
            h.writes(),
            0,
            "maker={maker}: all IDs with a stale quantity"
        );
        h.response(
            "/data/order/o1",
            200,
            &detail_with_trades("o1", "CANCELED", 3.0, &["prior-fill", "new-execution"]),
        );
        let request = h.take("/order");
        let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["order"]["takerAmount"], "7000000", "maker={maker}");
    }
}

#[test]
fn fractional_disjoint_execution_accepts_an_exact_six_decimal_terminal_total() {
    let mut h = H::new();
    let row = detail("o1", "LIVE", 2.1);
    h.ws(&format!(
        r#"{{"event_type":"order","type":"UPDATE",{}}}"#,
        &row[1..row.len() - 1]
    ));
    h.ws(r#"{"event_type":"trade","id":"fractional-execution","taker_order_id":"o1","asset_id":"1","side":"BUY","size":"0.2","price":"0.5","status":"MATCHED","owner":"api-test","trader_side":"TAKER"}"#);
    assert_eq!(h.e.store().order("o1").unwrap().filled, 2.3);
    replace(&mut h, 1);
    h.response(
        "/data/order/o1",
        200,
        &detail_with_trades(
            "o1",
            "CANCELED",
            2.3,
            &["prior-fill", "fractional-execution"],
        ),
    );
    let request = h.take("/order");
    let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
    assert_eq!(body["order"]["takerAmount"], "7700000");
}

#[test]
fn terminal_numeric_catchup_requires_the_actual_execution_ids() {
    for maker in [false, true] {
        let mut h = H::new();
        one_share_execution(&mut h, maker);
        replace(&mut h, 1);
        for _ in 0..3 {
            h.response(
                "/data/order/o1",
                200,
                &detail_with_trades(
                    "o1",
                    "CANCELED",
                    3.0,
                    &["prior-fill", "different-execution"],
                ),
            );
            assert_eq!(h.writes(), 0, "maker={maker}: unrelated trade ID");
        }
        assert!(h.done.last().unwrap().1.is_err());
        assert!(h.http.iter().all(|r| !r.url.ends_with("/data/order/o1")));
    }
}

#[test]
fn missing_or_null_terminal_associations_do_not_cover_known_executions() {
    for maker in [false, true] {
        for null in [false, true] {
            let mut h = H::new();
            one_share_execution(&mut h, maker);
            replace(&mut h, 1);
            let mut proof: Value = serde_json::from_str(&detail("o1", "CANCELED", 3.0)).unwrap();
            if null {
                proof["associate_trades"] = Value::Null;
            } else {
                proof.as_object_mut().unwrap().remove("associate_trades");
            }
            for _ in 0..3 {
                h.response("/data/order/o1", 200, &proof.to_string());
                assert_eq!(h.writes(), 0, "maker={maker} null={null}");
            }
            assert!(h.done.last().unwrap().1.is_err());
        }
    }
}

#[test]
fn complete_terminal_coverage_accepts_additional_prebootstrap_trade_ids() {
    for maker in [false, true] {
        let mut h = H::new();
        one_share_execution(&mut h, maker);
        replace(&mut h, 1);
        h.response(
            "/data/order/o1",
            200,
            &detail_with_trades(
                "o1",
                "CANCELED",
                4.0,
                &["prior-fill", "new-execution", "previously-unseen"],
            ),
        );
        let request = h.take("/order");
        let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["order"]["takerAmount"], "6000000", "maker={maker}");
    }
}

#[test]
fn cumulative_coverage_and_replayed_execution_do_not_add_the_same_fill_twice() {
    for maker in [false, true] {
        let mut h = H::new();
        one_share_execution(&mut h, maker);
        let row = detail_with_trades("o1", "LIVE", 3.0, &["prior-fill", "new-execution"]);
        h.ws(&format!(
            r#"{{"event_type":"order","type":"UPDATE",{}}}"#,
            &row[1..row.len() - 1]
        ));
        one_share_execution(&mut h, maker);
        assert_eq!(
            h.e.store().order("o1").unwrap().filled,
            3.0,
            "maker={maker}"
        );
        replace(&mut h, 1);
        h.response(
            "/data/order/o1",
            200,
            &detail_with_trades("o1", "CANCELED", 3.0, &["prior-fill", "new-execution"]),
        );
        let request = h.take("/order");
        let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["order"]["takerAmount"], "7000000", "maker={maker}");
    }
}

#[test]
fn final_missing_or_null_matched_size_never_proves_zero_fills() {
    for null in [false, true] {
        let mut h = H::new();
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        });
        h.response("/order", 200, &ack("fresh-zero"));
        h.command(Command::AmendOrder {
            command: 2,
            id: "fresh-zero".into(),
            price: Some(0.6),
            amount: None,
        });
        h.response(
            "/order",
            200,
            r#"{"canceled":["fresh-zero"],"not_canceled":{}}"#,
        );
        let mut proof: Value =
            serde_json::from_str(&detail("fresh-zero", "CANCELED", 0.0)).unwrap();
        if null {
            proof["size_matched"] = Value::Null;
        } else {
            proof.as_object_mut().unwrap().remove("size_matched");
        }
        h.response("/data/order/fresh-zero", 200, &proof.to_string());
        assert_eq!(h.writes(), 0, "null={null}: no final matched size");
        assert_eq!(h.done.last().unwrap().0, 2);
        assert!(h.done.last().unwrap().1.is_err());
    }
}

#[test]
fn fractional_final_fills_floor_replacement_shares_without_exceeding_target() {
    for (filled, remaining) in [(4.994, "5000000"), (4.88, "5120000")] {
        let mut h = H::new();
        replace(&mut h, 1);
        h.response("/data/order/o1", 200, &detail("o1", "CANCELED", filled));
        let request = h.take("/order");
        let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["order"]["takerAmount"], remaining);
        let units = body["order"]["takerAmount"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!(units as f64 / 1e6 + filled <= 10.0);
    }
}

#[test]
fn global_cancel_uses_native_endpoint_even_without_cached_orders_and_accepts_unknown_ids() {
    for cached in [false, true] {
        let mut h = H::new();
        if !cached {
            h.e.store_mut().set_orders(vec![]);
        }
        h.command(Command::CancelSymbolOrders {
            command: 1,
            symbol: String::new(),
        });
        let request = h.take("/cancel-all");
        assert_eq!(request.method, Method::Delete);
        assert!(request.body.is_none());
        let expected = safe_cex_crypto::hmac_sha256(
            b"secret",
            b"1700000000DELETE/cancel-all",
            safe_cex_crypto::Encoding::Base64,
        )
        .replace('+', "-")
        .replace('/', "_");
        assert!(request.headers.contains(&("POLY_SIGNATURE", expected)));
        h.respond(
            request,
            200,
            r#"{"canceled":["o1","uncached-order"],"not_canceled":{}}"#,
        );
        assert_eq!(h.done.last().unwrap().1, Ok("[]".into()));
        assert!(h.e.store().orders().is_empty());
        let row = detail("uncached-order", "LIVE", 0.0);
        h.ws(&format!(
            r#"{{"event_type":"order","type":"UPDATE",{}}}"#,
            &row[1..row.len() - 1]
        ));
        assert!(h.e.store().order("uncached-order").is_none());
    }
}

#[test]
fn global_cancel_partial_and_missing_proofs_fail_without_assuming_cached_order_canceled() {
    for ack in [
        r#"{"canceled":["uncached-order"],"not_canceled":{"o1":"cannot cancel"}}"#,
        r#"{"canceled":[],"not_canceled":{}}"#,
    ] {
        let mut h = H::new();
        h.command(Command::CancelSymbolOrders {
            command: 1,
            symbol: String::new(),
        });
        h.response("/cancel-all", 200, ack);
        assert!(h.done.last().unwrap().1.is_err());
        assert!(h.e.store().order("o1").is_some());
        assert_eq!(h.writes(), 0);
    }
}

#[test]
fn global_cancel_excludes_all_concurrent_writes_and_releases_lock_after_failure() {
    let mut h = H::new();
    h.command(Command::CancelSymbolOrders {
        command: 1,
        symbol: String::new(),
    });
    let request = h.take("/cancel-all");
    for command in [
        Command::PlaceOrders {
            command: 2,
            orders: vec![order()],
        },
        Command::CancelOrders {
            command: 3,
            ids: vec!["uncached-order".into()],
        },
        Command::AmendOrder {
            command: 4,
            id: "o1".into(),
            price: Some(0.6),
            amount: None,
        },
        Command::CancelSymbolOrders {
            command: 5,
            symbol: "election-No".into(),
        },
        Command::CancelSymbolOrders {
            command: 6,
            symbol: String::new(),
        },
    ] {
        h.command(command);
        assert!(h.done.last().unwrap().1.is_err());
        assert_eq!(h.writes(), 0);
    }
    h.event(Event::HttpFailed {
        request: request.id,
        error: "lost global cancellation response",
    });
    assert!(h.done.last().unwrap().1.is_err());
    assert_eq!(h.writes(), 0);
    h.command(Command::PlaceOrders {
        command: 7,
        orders: vec![order()],
    });
    assert_eq!(h.writes(), 1);
    h.response("/order", 200, &ack("new-order"));
    h.command(Command::CancelSymbolOrders {
        command: 8,
        symbol: String::new(),
    });
    assert_eq!(h.writes(), 1);
}

#[test]
fn global_cancel_cannot_begin_while_an_order_write_is_pending() {
    let mut h = H::new();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![order()],
    });
    h.command(Command::CancelSymbolOrders {
        command: 2,
        symbol: String::new(),
    });
    assert!(h.done.last().unwrap().1.is_err());
    assert_eq!(h.writes(), 1);
    assert!(
        h.http
            .iter()
            .all(|request| !request.url.ends_with("/cancel-all"))
    );
}

#[test]
fn token_cancel_empty_ack_cannot_confirm_a_cached_live_order() {
    let mut h = H::new();
    h.command(Command::CancelSymbolOrders {
        command: 1,
        symbol: "election-Yes".into(),
    });
    let request = h.take("/cancel-market-orders");
    assert_eq!(request.method, Method::Delete);
    assert_eq!(request.body.as_deref(), Some(r#"{"asset_id":"1"}"#));
    h.respond(request, 200, r#"{"canceled":[],"not_canceled":{}}"#);
    assert!(h.done.last().unwrap().1.is_err());
    assert!(h.e.store().order("o1").is_some());
    assert_eq!(h.writes(), 0);
}

#[test]
fn token_cancel_subset_ack_reports_failure_and_only_removes_proven_ids() {
    let mut h = H::new();
    let mut second = h.e.store().order("o1").unwrap().clone();
    second.id = "second-live-order".into();
    h.e.store_mut().upsert_order(second);
    h.command(Command::CancelSymbolOrders {
        command: 1,
        symbol: "election-Yes".into(),
    });
    h.response(
        "/cancel-market-orders",
        200,
        r#"{"canceled":["o1"],"not_canceled":{}}"#,
    );
    assert!(h.done.last().unwrap().1.is_err());
    assert!(h.e.store().order("o1").is_none());
    assert!(h.e.store().order("second-live-order").is_some());
    assert_eq!(h.writes(), 0);
    // Failed partial cancellation releases its mutation scope for an explicit retry.
    h.command(Command::CancelSymbolOrders {
        command: 2,
        symbol: "election-Yes".into(),
    });
    h.response(
        "/cancel-market-orders",
        200,
        r#"{"canceled":["second-live-order"],"not_canceled":{}}"#,
    );
    assert_eq!(h.done.last().unwrap().1, Ok("[]".into()));
    assert!(h.e.store().orders().is_empty());
}

#[test]
fn self_trade_prevention_rejects_whole_batch_before_writing() {
    let mut h = H::new();
    let mut unsupported = order();
    unsupported.stp_mode = Some(StpMode::ExpireBoth);
    h.command(Command::PlaceOrders {
        command: 90,
        orders: vec![order(), unsupported],
    });
    assert_eq!(h.writes(), 0);
    assert!(matches!(&h.done[..], [(90, Err(error))] if error.contains("plain GTC")));
}
#[test]
fn cancellation_with_symbol_uses_native_id_without_store_lookup() {
    let mut h = H::new();
    h.command(Command::CancelOrdersWithSymbols {
        command: 91,
        orders: vec![CancelOrderRef {
            id: "unseen".into(),
            symbol: "unknown".into(),
        }],
    });
    let request = h.take("/orders");
    assert_eq!(request.method, Method::Delete);
    assert_eq!(request.body.as_deref(), Some(r#"["unseen"]"#));
    h.respond(request, 200, r#"{"canceled":["unseen"],"not_canceled":{}}"#);
    assert!(matches!(&h.done[..], [(91, Ok(_))]));
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let rejected = (Some(ErrorKind::Rejected), Some(Outcome::Rejected));
    for (status, body, classified, text) in [
        (503, "unavailable", (None, Some(Outcome::Unknown)), None),
        (408, "timeout", (None, Some(Outcome::Unknown)), None),
        (
            401,
            "{}",
            (Some(ErrorKind::AuthInvalid), Some(Outcome::Rejected)),
            None,
        ),
        (
            403,
            "{}",
            (Some(ErrorKind::AuthPermission), Some(Outcome::Rejected)),
            None,
        ),
        (
            429,
            "{}",
            (Some(ErrorKind::RateLimited), Some(Outcome::Rejected)),
            None,
        ),
        (400, r#"{"error":"invalid order"}"#, rejected, None),
        (
            200,
            r#"{"success":false,"orderID":"","errorMsg":"not enough balance","status":""}"#,
            (None, None),
            Some("Polymarket placement rejected: not enough balance"),
        ),
        (200, "not json", (None, Some(Outcome::Unknown)), None),
    ] {
        let mut h = H::new();
        h.command(Command::PlaceOrders {
            command: 9,
            orders: vec![order()],
        });
        let r = h.take("/order");
        h.respond(r, status, body);
        let [error] = h.failures.as_slice() else {
            panic!("{status} {body}: {:?}", h.failures);
        };
        assert_eq!(
            (error.kind, error.outcome),
            classified,
            "{status} {body}: {}",
            error.message
        );
        let text = text.map_or_else(|| format!("Polymarket HTTP {status}: {body}"), Into::into);
        if status != 200 || text.starts_with("Polymarket placement") {
            assert_eq!(h.done, [(9, Err(text))], "{status} {body}");
        }
    }
    let mut h = H::new();
    h.command(Command::PlaceOrders {
        command: 9,
        orders: vec![order()],
    });
    let r = h.take("/order");
    h.event(Event::HttpFailed {
        request: r.id,
        error: "connection reset",
    });
    assert_eq!(
        (h.failures[0].kind, h.failures[0].outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
}

#[test]
fn failures_after_an_effective_step_are_never_rejected_or_not_sent() {
    use safe_cex_engine::Outcome;
    // A second placement refused after the first was accepted.
    let mut h = H::new();
    h.command(Command::PlaceOrders {
        command: 9,
        orders: vec![order(), order()],
    });
    h.response("/order", 200, &ack("first"));
    let r = h.take("/order");
    h.respond(r, 400, r#"{"error":"invalid order"}"#);
    let [error] = h.failures.as_slice() else {
        panic!("{:?}", h.failures);
    };
    assert_eq!(error.outcome, None);
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["first"]);

    // The replacement cannot be prepared after the cancel was confirmed:
    // nothing was sent, but the cancel took effect.
    let mut h = H::new();
    replace(&mut h, 2);
    h.e.store_mut()
        .update_markets(|market| market.active = false);
    h.response("/data/order/o1", 200, &detail("o1", "CANCELED", 4.88));
    assert_eq!(h.writes(), 0);
    let [error] = h.failures.as_slice() else {
        panic!("{:?}", h.failures);
    };
    assert_eq!(error.message, "Polymarket market is inactive");
    assert_eq!(error.outcome, None);
    assert_ne!(error.outcome, Some(Outcome::Unknown));
}

#[test]
#[ignore = "original RED: P0 write adoption waits for signer/import and Plan seams"]
fn p0_write_bare_4xx_is_not_execution_refusal() {
    let mut h = H::new();
    h.command(Command::PlaceOrders {
        command: 731,
        orders: vec![order()],
    });
    let request = h.take("/order");
    h.respond(request, 400, "{}");
    let [error] = h.failures.as_slice() else {
        panic!("one terminal failure: {:?}", h.failures);
    };
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(h.done.len(), 1);
    assert!(matches!(&h.done[0], (731, Err(_))));
    assert!(
        !h.http
            .iter()
            .any(|request| request.url.ends_with("/order") && request.method == Method::Post)
    );
}

#[test]
#[ignore = "original RED: P0 write adoption waits for signer/import and Plan seams"]
fn p0_write_unmatched_error_message_is_unknown_internal() {
    let mut h = H::new();
    h.command(Command::PlaceOrders {
        command: 732,
        orders: vec![order()],
    });
    let request = h.take("/order");
    h.respond(
        request,
        200,
        r#"{"success":false,"orderID":"","errorMsg":"not enough balance","status":""}"#,
    );
    let [error] = h.failures.as_slice() else {
        panic!("one terminal failure: {:?}", h.failures);
    };
    assert_eq!(
        error.message,
        "Polymarket placement rejected: not enough balance"
    );
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Internal));
    assert_eq!(h.done.len(), 1);
    assert!(matches!(&h.done[0], (732, Err(_))));
    assert!(
        !h.http
            .iter()
            .any(|request| request.url.ends_with("/order") && request.method == Method::Post)
    );
}
