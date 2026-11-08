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
    events: Vec<safe_cex_engine::LogEvent>,
    diagnostics: Vec<(u32, String)>,
    timers: Vec<(safe_cex_engine::TimerId, f64)>,
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
            events: vec![],
            diagnostics: vec![],
            timers: vec![],
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
                Action::Notice(Notice::CommandError { command, message }) => {
                    self.diagnostics.push((command, message));
                }
                Action::Notice(Notice::Event(event)) => self.events.push(event),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
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
fn legacy_ts_above_max_sends_one_payload_and_surfaces_native_refusal() {
    // TS packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts:660,680-698.
    // Decimal toFixed followed by one postOrder, with no maximum check.
    // KNOWN-ISSUE: TS sends above-max; venue refuses.
    for (amount, maximum, precision, shares, collateral) in [
        (26.0, 5.0, 1.0, "26000000", "13000000"),
        (11.0, 5.0, 1.0, "11000000", "5500000"),
        (12.0, 5.0, 1.0, "12000000", "6000000"),
        (10.0, 5.0, 1.0, "10000000", "5000000"),
        (7.0, 3.0, 0.5, "7000000", "3500000"),
        (
            1_000_001.0,
            1_000_000.0,
            2.0,
            "1000001000000",
            "500000500000",
        ),
        (5.125, 5.0, 2.0, "5130000", "2565000"),
    ] {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            let mut h = H::new();
            h.e.store_mut().update_markets(|market| {
                market.precision.amount = precision;
                market.limits.amount.min = 0.01;
                market.limits.amount.max = maximum;
            });
            let mut input = order();
            input.amount = amount;
            input.side = side;
            h.command(Command::PlaceOrders {
                command: 81,
                orders: vec![input],
            });
            assert_eq!(
                h.writes(),
                1,
                "amount={amount}, max={maximum}, side={side:?}"
            );
            let request = h.take("/order");
            assert_eq!(request.method, Method::Post);
            let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
            let (maker, taker) = if side == OrderSide::Buy {
                (collateral, shares)
            } else {
                (shares, collateral)
            };
            assert_eq!(body["order"]["makerAmount"], maker);
            assert_eq!(body["order"]["takerAmount"], taker);
            assert_eq!(body["orderType"], "GTC");
            h.respond(request, 200, r#"{"success":false,"orderID":"","errorMsg":"fixture maximum exceeded","status":""}"#);
            assert!(
                h.done
                    .last()
                    .unwrap()
                    .1
                    .as_ref()
                    .unwrap_err()
                    .contains("fixture maximum exceeded")
            );
            assert_eq!(
                h.failures.last().unwrap().outcome,
                Some(safe_cex_engine::Outcome::Rejected),
                "the native false acknowledgement is a definite refusal"
            );
            assert_eq!(h.writes(), 0, "native refusal must not split or resend");
        }
    }
}

#[test]
fn legacy_ts_to_fixed_precision_and_midpoints_precede_builder_rounding() {
    // TS packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts:660.
    // Precision is decimal digits, not a step.
    for (amount, precision, maximum, shares, collateral) in [
        // KNOWN-ISSUE: legacy TS under-allocates (sum 5 of requested 5.005);
        // kept for exact TS parity per Sam's decision.
        (5.005, 2.0, 5.0, "5000000", "2500000"),
        (5.125, 2.0, 10.0, "5130000", "2565000"),
        (5.25, 1.0, 10.0, "5300000", "2650000"),
        // KNOWN-ISSUE: legacy TS under-allocates (sum 5.12 of requested 5.129);
        // kept for exact TS parity per Sam's decision (official builder floors to 2dp).
        (5.129, 3.0, 10.0, "5120000", "2560000"),
        (5.6, 0.001, 10.0, "6000000", "3000000"),
        (6.5, 0.0, 10.0, "7000000", "3500000"),
    ] {
        let mut h = H::new();
        h.e.store_mut().update_markets(|market| {
            market.precision.amount = precision;
            market.limits.amount.max = maximum;
        });
        let mut input = order();
        input.amount = amount;
        h.command(Command::PlaceOrders {
            command: 82,
            orders: vec![input],
        });
        assert_eq!(h.writes(), 1);
        let request = h.take("/order");
        let body: Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(
            body["order"]["takerAmount"], shares,
            "amount={amount}, precision={precision}"
        );
        assert_eq!(
            body["order"]["makerAmount"], collateral,
            "amount={amount}, precision={precision}"
        );
        h.respond(request, 200, &ack("rounded"));
        assert!(h.done.last().unwrap().1.is_ok());
        let sum = shares.parse::<f64>().unwrap() / 1e6;
        assert_eq!(h.e.store().order("rounded").unwrap().amount, sum);
        assert_eq!(h.writes(), 0);
    }
}

#[test]
fn oversized_native_unknown_has_one_report_and_indexed_outcomes_without_resend() {
    use safe_cex_engine::write_plan::ElementResult;
    use safe_cex_engine::{Outcome, UNKNOWN_OUTCOME_TEXT};
    for accepted_prefix in [false, true] {
        for body in [
            "not-json",
            r#"{"orderID":"unproven"}"#,
            r#"{"success":true,"orderID":""}"#,
            r#"{"success":true}"#,
            r#"{"success":true,"orderID":null}"#,
        ] {
            let mut h = H::new();
            h.e.store_mut()
                .update_markets(|market| market.limits.amount.max = 5.0);
            let mut input = order();
            input.amount = 26.0;
            h.command(Command::PlaceOrders {
                command: 91,
                orders: vec![input; 3],
            });
            if accepted_prefix {
                h.response(
                    "/order",
                    200,
                    r#"{"success":true,"orderID":"accepted","status":"live"}"#,
                );
            }
            let request = h.take("/order");
            let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(payload["order"]["takerAmount"], "26000000");
            h.respond(request.clone(), 200, body);
            let error = h.failures.last().unwrap();
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert_eq!(error.message, UNKNOWN_OUTCOME_TEXT);
            assert!(
                error
                    .venue_message
                    .as_deref()
                    .unwrap()
                    .contains("Invalid Polymarket placement acknowledgement")
            );
            let elements = error.elements.as_ref().expect("indexed outcome journal");
            assert_eq!(elements.len(), 3);
            assert_eq!(
                elements
                    .iter()
                    .map(|row| row.element.index)
                    .collect::<Vec<_>>(),
                vec![0, 1, 2]
            );
            let index = usize::from(accepted_prefix);
            if accepted_prefix {
                assert_eq!(
                    elements[0].result,
                    ElementResult::Placed {
                        order_ids: vec!["accepted".into()]
                    }
                );
                assert_eq!(error.partial.as_ref().unwrap().order_ids, ["accepted"]);
            }
            assert!(
                matches!(&elements[index].result, ElementResult::Unknown { error }
                if error.message == UNKNOWN_OUTCOME_TEXT && error.outcome == Some(Outcome::Unknown))
            );
            assert!(
                elements[index + 1..]
                    .iter()
                    .all(|row| row.result == ElementResult::NotSent { error: None })
            );
            assert_eq!(h.diagnostics, vec![(91, UNKNOWN_OUTCOME_TEXT.into())]);
            let events: Vec<_> = h
                .events
                .iter()
                .filter(|event| event.code == "order.outcome_unknown")
                .collect();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].message, UNKNOWN_OUTCOME_TEXT);
            assert!(
                events[0]
                    .fields
                    .iter()
                    .any(|(key, value)| *key == "placedCount" && *value == (index as f64).into())
            );
            assert!(
                events[0]
                    .fields
                    .iter()
                    .any(|(key, value)| *key == "unknownRequest"
                        && value.as_str() == Some("place election-Yes"))
            );
            assert!(
                events[0]
                    .fields
                    .iter()
                    .any(|(key, value)| *key == "command" && *value == 91u32.into())
            );
            assert!(
                events[0]
                    .fields
                    .iter()
                    .any(|(key, value)| *key == "elementIndex" && *value == (index as f64).into())
            );
            assert_eq!(h.writes(), 0);
            let original = error.clone();
            h.respond(
                request,
                200,
                r#"{"success":true,"orderID":"late","status":"live"}"#,
            );
            h.now += 60_000.0;
            for (timer, at) in std::mem::take(&mut h.timers) {
                if at <= h.now {
                    h.event(Event::TimerFired { timer });
                }
            }
            assert_eq!(h.writes(), 0);
            assert_eq!(h.failures, vec![original]);
            assert_eq!(h.diagnostics.len(), 1);
            assert_eq!(
                h.events
                    .iter()
                    .filter(|event| event.code == "order.outcome_unknown")
                    .count(),
                1
            );
        }
    }
}

#[test]
fn oversized_native_refusal_is_rejected_with_an_indexed_journal() {
    use safe_cex_engine::Outcome;
    use safe_cex_engine::write_plan::ElementResult;
    for body in [
        r#"{"success":false,"orderID":"","errorMsg":"native size refusal"}"#,
        r#"{"success":false,"errorMsg":"native size refusal"}"#,
        r#"{"success":false,"orderID":null,"errorMsg":"native size refusal"}"#,
    ] {
        let mut h = H::new();
        h.e.store_mut()
            .update_markets(|market| market.limits.amount.max = 5.0);
        let mut input = order();
        input.amount = 26.0;
        h.command(Command::PlaceOrders {
            command: 92,
            orders: vec![input; 2],
        });
        h.response("/order", 200, body);
        let error = h.failures.last().unwrap();
        assert_eq!(error.outcome, Some(Outcome::Rejected));
        assert_eq!(
            error.message,
            "Polymarket placement rejected: native size refusal"
        );
        let elements = error.elements.as_ref().expect("refusal journal");
        assert_eq!(elements.len(), 2);
        assert!(
            matches!(&elements[0].result, ElementResult::NotSent { error: Some(error) }
        if error.outcome == Some(Outcome::Rejected) && error.message.contains("native size refusal"))
        );
        assert_eq!(elements[1].result, ElementResult::NotSent { error: None });
        assert_eq!(
            h.events
                .iter()
                .filter(|event| event.code == "order.outcome_unknown")
                .count(),
            0
        );
        assert_eq!(h.writes(), 0);
    }
}

#[test]
fn dispatched_ambiguous_writes_are_unknown_without_resend_or_replacement() {
    // No native rate-code table, protective orders, or remote settings exist
    // for this venue. Exercise every supported dispatched mutation instead.
    for (status, body) in [
        (429, "{}"),
        (429, r#"{"error":"rate limit"}"#),
        (429, r#"{"error":"invalid signature: refused"}"#),
        (408, r#"{"error":"invalid order: refused"}"#),
        (500, r#"{"error":"invalid signature: refused"}"#),
        (502, "bad gateway"),
        (503, r#"{"error":"invalid order: refused"}"#),
        (504, "gateway timeout"),
    ] {
        for operation in ["create", "cancel", "amend-cancel", "replacement"] {
            let mut h = H::new();
            match operation {
                "create" => h.command(Command::PlaceOrders {
                    command: 91,
                    orders: vec![order()],
                }),
                "cancel" => h.command(Command::CancelOrders {
                    command: 91,
                    ids: vec!["o1".into()],
                }),
                "amend-cancel" => h.command(Command::AmendOrder {
                    command: 91,
                    id: "o1".into(),
                    price: Some(0.6),
                    amount: None,
                }),
                "replacement" => {
                    replace(&mut h, 91);
                    h.response("/data/order/o1", 200, &detail("o1", "CANCELED", 4.));
                }
                _ => unreachable!(),
            }
            let writes: Vec<_> = h
                .http
                .iter()
                .filter(|r| r.method != Method::Get)
                .cloned()
                .collect();
            assert_eq!(writes.len(), 1, "{operation}: {:?}", h.http);
            let request = h.take(&writes[0].url);
            h.respond(request, status, body);
            assert_eq!(h.failures.len(), 1, "{status} {body}: {:?}", h.done);
            let error = &h.failures[0];
            assert_eq!(
                error.outcome,
                Some(safe_cex_engine::Outcome::Unknown),
                "{operation} {status} {body}: {error:?}"
            );
            use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
            let index = usize::from(operation == "replacement");
            let elements = error.elements.as_ref().expect("mutation journal");
            assert!(
                matches!(&elements[index].result, ElementResult::Unknown { error }
                if error.message.contains(safe_cex_engine::UNKNOWN_OUTCOME_TEXT))
            );
            if operation == "replacement" {
                assert!(matches!(&elements[0].result, ElementResult::Applied {
                    effect: AppliedEffect::Cancel, target_order_id: Some(id), ..
                } if id == "o1"));
            } else if operation == "amend-cancel" {
                assert_eq!(elements[1].result, ElementResult::NotSent { error: None });
            }
            assert_eq!(
                h.diagnostics,
                vec![(91, safe_cex_engine::UNKNOWN_OUTCOME_TEXT.into())]
            );
            assert_eq!(
                h.events
                    .iter()
                    .filter(|event| event.code == "order.outcome_unknown")
                    .count(),
                1
            );
            let text = error.legacy_text().to_ascii_lowercase();
            assert!(
                text.contains("unknown") && text.contains("check open orders"),
                "{text}"
            );
            assert!(
                !text.contains("refused") && !text.contains("order is gone"),
                "{text}"
            );
            assert!(
                !h.http.iter().any(|r| r.method != Method::Get),
                "no resend or replacement: {:?}",
                h.http
            );
        }
    }
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
fn a_limit_whose_replacement_is_refused_says_the_order_is_gone() {
    let mut h = H::new();
    replace(&mut h, 1);
    h.response("/data/order/o1", 200, &detail("o1", "CANCELED", 4.0));
    h.response("/order", 400, r#"{"error":"not enough balance"}"#);
    let error = h.done.last().unwrap().1.clone().unwrap_err();
    assert!(error.contains("order is gone"), "{error}");
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
fn price_tick_and_ts_decimal_share_rounding_precede_official_builder_rounding() {
    // TS reference/polymarket/polymarket.exchange.ts:659-660 aligns price to the tick
    // and uses Number(amount.toFixed(precision.amount)) for shares.
    for (raw_size, expected_shares, expected_collateral) in [
        (5.999, "6000000", "3180000"),
        // KNOWN-ISSUE: legacy TS under-allocates (sum 5 of requested 5.005);
        // kept for exact TS parity per Sam's decision.
        (5.005, "5000000", "2650000"),
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
        assert_eq!(h.writes(), 1);
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
        let sum = body["order"]["takerAmount"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            / 1e6;
        assert_eq!(sum, expected_shares.parse::<f64>().unwrap() / 1e6);
        assert_eq!(h.e.store().order("rounded").unwrap().amount, sum);
        assert_eq!(h.writes(), 0);
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
            "outcome is unknown. Check Open Orders"
        } else {
            "insufficient"
        }));
        if network {
            assert_eq!(
                h.failures[0].venue_message.as_deref(),
                Some("lost second response")
            );
        }
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
fn write_failures_preserve_definitive_text_and_separate_ambiguous_diagnostics() {
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
            (Some(ErrorKind::RateLimited), Some(Outcome::Unknown)),
            None,
        ),
        (400, r#"{"error":"invalid order"}"#, rejected, None),
        (
            200,
            r#"{"success":false,"orderID":"","errorMsg":"not enough balance","status":""}"#,
            (None, Some(Outcome::Rejected)),
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
        if matches!(status, 408 | 429 | 500..) {
            assert_eq!(
                h.done,
                [(9, Err("The outcome is unknown. Check Open Orders".into()))]
            );
            assert_eq!(error.venue_message.as_deref(), Some(text.as_str()));
        } else if status != 200 || text.starts_with("Polymarket placement") {
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
    assert_eq!(
        h.failures[0].message,
        "The outcome is unknown. Check Open Orders"
    );
    assert_eq!(
        h.failures[0].venue_message.as_deref(),
        Some("connection reset")
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
fn aggregation_normalizes_transport_unknown_warning() {
    let mut h = H::new();
    h.command(Command::PlaceOrders {
        command: 91,
        orders: vec![order(), order(), order()],
    });
    h.response("/order", 200, &ack("accepted"));
    let request = h.take("/order");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "fixture connection reset",
    });
    let [error] = h.failures.as_slice() else {
        panic!("{:?}", h.failures)
    };
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Network));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["accepted"]);
    assert_eq!(
        error.venue_message.as_deref(),
        Some("fixture connection reset")
    );
    let text = error.legacy_text().to_ascii_lowercase();
    assert!(
        text.contains("unknown") && text.contains("check open orders"),
        "{error:?}"
    );
    assert!(!text.contains("connection reset"), "{text}");
    assert_eq!(h.writes(), 0, "no resend or third placement");
}

#[test]
fn aggregation_keeps_unknown_warning_and_stops_after_a_refusal() {
    // Unlike Orderly and Paradex, a refusal ends this venue's batch immediately.
    let mut h = H::new();
    h.command(Command::PlaceOrders {
        command: 91,
        orders: vec![order(), order()],
    });
    h.response("/order", 400, r#"{"error":"definitive refusal"}"#);
    assert_eq!(h.failures.len(), 1);
    assert_eq!(
        h.failures[0].outcome,
        Some(safe_cex_engine::Outcome::Rejected)
    );
    assert_eq!(h.writes(), 0, "a later write cannot overwrite this refusal");

    // An accepted prefix also cannot hide the later ambiguous write's warning.
    for (status, body) in [
        (429, r#"{"error":"rate limit"}"#),
        (408, r#"{"error":"refused"}"#),
        (503, r#"{"error":"refused"}"#),
    ] {
        let mut h = H::new();
        h.command(Command::PlaceOrders {
            command: 91,
            orders: vec![order(), order(), order()],
        });
        h.response("/order", 200, &ack("accepted"));
        h.response("/order", status, body);
        assert_eq!(h.failures.len(), 1);
        let error = &h.failures[0];
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["accepted"]);
        let text = error.legacy_text().to_ascii_lowercase();
        assert!(
            text.contains("unknown") && text.contains("check open orders"),
            "{error:?}"
        );
        assert!(!text.contains("refused"), "{text}");
        assert_eq!(h.writes(), 0, "no resend or third placement");
    }
}
