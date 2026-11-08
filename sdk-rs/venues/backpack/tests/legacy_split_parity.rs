mod support;

use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, VenueError};
use serde_json::{Value, json};
use support::managed::*;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn place(engine: &mut Driver, orders: Vec<Value>) -> Result {
    engine.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 901,
            orders: serde_json::from_value(Value::Array(orders))?,
        }),
    );
    Ok(())
}

fn limit(price: f64) -> Value {
    json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":0.4,"price":price})
}

fn packet(engine: &mut Driver, transport: &mut Vec<HttpRequest>) -> Result<HttpRequest> {
    let packets = requests(engine);
    assert_eq!(
        packets.len(),
        1,
        "legacy TS dispatches the next independent write"
    );
    transport.extend(packets.iter().cloned());
    packets
        .into_iter()
        .next()
        .ok_or_else(|| "missing write".into())
}

fn sequence(transport: &[HttpRequest], expected: &[(Method, &str)]) {
    assert_eq!(
        transport.len(),
        expected.len(),
        "exact legacy TS write count"
    );
    for (request, (method, path)) in transport.iter().zip(expected) {
        assert_eq!(&request.method, method);
        assert!(request.url.ends_with(path), "{}", request.url);
    }
}

fn pair_ack(request: &HttpRequest) -> Result<String> {
    let rows: Vec<Value> = serde_json::from_str(request.body.as_deref().ok_or("missing pair")?)?;
    Ok(serde_json::to_string(
        &rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                json!({
                    "operation":"Ok","id":format!("pair-{i}"),"clientId":row["clientId"],
                    "symbol":row["symbol"],"side":row["side"],"orderType":row["orderType"]
                })
            })
            .collect::<Vec<_>>(),
    )?)
}

fn failure(out: &[Action]) -> Result<&VenueError> {
    out.iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 901,
                error,
            }) => Some(error),
            _ => None,
        })
        .ok_or_else(|| "missing reporting-only terminal".into())
}

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:991-1025,1072-1079,1207-1229.
fn unknown_stop(path: &str) -> Result {
    for status in [Some(400), Some(200), Some(503), None] {
        if path == "pair" && status != Some(200) {
            continue;
        }
        let rows = ["123", "456", "789"].map(|id| ORDER.replace("\"123\"", &format!("\"{id}\"")));
        let (mut engine, _) = ready_orders(&format!("[{}]", rows.join(",")));
        engine
            .store_mut()
            .update_markets(|market| market.limits.amount.max = 1000.);
        let mut transport = vec![];
        let bracket = json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130});
        let (unknown, count, accepted) = match path {
            "cancel" => {
                engine.handle(
                    2000.,
                    Event::Command(Command::CancelOrders {
                        command: 901,
                        ids: vec!["123".into(), "456".into(), "789".into()],
                    }),
                );
                (1, 3, vec![])
            }
            "pair" => {
                place(
                    &mut engine,
                    [140., 141., 142., 143.].into_iter().map(limit).collect(),
                )?;
                (1, 4, vec!["prefix".to_owned()])
            }
            "tp" | "sl" | "entry" => {
                place(&mut engine, vec![bracket, limit(142.)])?;
                let index = if path == "entry" {
                    0
                } else if path == "tp" {
                    1
                } else {
                    2
                };
                (
                    index,
                    4,
                    (0..index).map(|i| format!("prefix-{i}")).collect(),
                )
            }
            _ => {
                let orders = (1..=3)
                    .map(|id| {
                        let mut row = limit(if path == "cap" && id == 2 {
                            1000.01
                        } else {
                            140.
                        });
                        row["clientOrderId"] = json!(id.to_string());
                        row
                    })
                    .collect();
                place(&mut engine, orders)?;
                (1, 3, vec!["prefix-0".to_owned()])
            }
        };
        if path != "pair" {
            for i in 0..unknown {
                let request = packet(&mut engine, &mut transport)?;
                reply(&mut engine, &request, 200, &json!({"id":if path=="cancel" {"123".to_owned()} else {format!("prefix-{i}")}}).to_string());
            }
        }
        let request = packet(&mut engine, &mut transport)?;
        if path == "pair" {
            let mut ack: Vec<Value> = serde_json::from_str(&pair_ack(&request)?)?;
            ack[0]["id"] = json!("prefix");
            ack[1]["operation"] = json!("Error");
            ack[1]["id"] = json!("");
            reply(&mut engine, &request, 200, &serde_json::to_string(&ack)?);
        } else if let Some(status) = status {
            reply(
                &mut engine,
                &request,
                status,
                r#"{"code":"UNCLASSIFIED","message":"uncertain"}"#,
            );
        } else {
            engine.handle(
                2000.,
                Event::HttpFailed {
                    request: request.id,
                    error: "lost reply",
                },
            );
        }
        let out = actions(&mut engine);
        assert!(
            !out.iter().any(|action| matches!(action, Action::Http(_))),
            "Unknown must halt {path}"
        );
        let error = failure(&out)?;
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert!(
            error.message.contains(ENGINE_UNKNOWN_TEXT),
            "{}",
            error.message
        );
        let elements = error.elements.as_ref().ok_or("missing journal")?;
        assert_eq!(elements.len(), count);
        assert!(
            matches!(&elements[unknown].result,ElementResult::Unknown { error } if error.message.contains(ENGINE_UNKNOWN_TEXT))
        );
        assert!(
            elements[unknown + 1..]
                .iter()
                .all(|element| matches!(element.result, ElementResult::NotSent { error: None }))
        );
        let ids = error
            .partial
            .as_ref()
            .map_or(&[][..], |partial| partial.order_ids.as_slice());
        assert_eq!(ids, accepted);
        if path == "cancel" {
            assert!(matches!(
                elements[0].result,
                ElementResult::Applied {
                    effect: AppliedEffect::Cancel,
                    ..
                }
            ));
        }
        assert!(out.iter().any(|action| matches!(action,Action::Notice(Notice::Event(event)) if event.code=="backpack.write.unknown" && event.message.contains(ENGINE_UNKNOWN_TEXT))));
        reply(&mut engine, &request, 200, r#"{"id":"late"}"#);
        assert!(!actions(&mut engine).iter().any(|action| matches!(
            action,
            Action::Http(_)
                | Action::Notice(
                    Notice::CommandDone { command: 901, .. }
                        | Notice::CommandFailed { command: 901, .. }
                )
        )));
        assert_eq!(
            transport.len(),
            if path == "pair" { 1 } else { unknown + 1 }
        );
    }
    Ok(())
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1223-1229.
#[test]
fn over_cap_unknown_halts_later_row() -> Result {
    unknown_stop("cap")
}

struct RefuseTakeProfit(safe_cex_engine::signing::Provider);

impl RefuseTakeProfit {
    fn refuses(request: &safe_cex_engine::signing::SignRequest) -> bool {
        use safe_cex_engine::signing::{Operation, PayloadPart};
        request.operations.iter().any(|operation| match operation {
            Operation::Sign { payload, .. } => payload.iter().any(|part| matches!(part,
                PayloadPart::Bytes { bytes } if String::from_utf8_lossy(bytes).contains("triggerPrice=150.00")
            )),
            _ => false,
        })
    }
}

impl safe_cex_engine::signing::Signer for RefuseTakeProfit {
    fn immediate(&self) -> bool {
        self.0.immediate()
    }
    fn can_sign(
        &self,
        key: &safe_cex_engine::signing::KeyRef,
        algorithm: safe_cex_engine::signing::Algorithm,
    ) -> safe_cex_engine::signing::Capability {
        self.0.can_sign(key, algorithm)
    }
    fn sign(
        &self,
        request: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        if Self::refuses(&request) {
            Box::pin(async { Err(safe_cex_engine::signing::SignError::Refused) })
        } else {
            self.0.sign(request)
        }
    }
    fn sign_ready(
        &self,
        request: &safe_cex_engine::signing::SignRequest,
    ) -> Option<
        std::result::Result<
            safe_cex_engine::signing::SignResult,
            safe_cex_engine::signing::SignError,
        >,
    > {
        if Self::refuses(request) {
            Some(Err(safe_cex_engine::signing::SignError::Refused))
        } else {
            self.0.sign_ready(request)
        }
    }
}

// Proven pre-release refusal is safe to continue, matching TS's child catch.
#[test]
fn definite_not_sent_child_refusal_still_allows_sl() -> Result {
    let (snapshot, _) = ready();
    let signer = snapshot.signer().ok_or("missing fixture signer")?;
    let identity = snapshot
        .identities()
        .primary
        .clone()
        .ok_or("missing fixture identity")?;
    let mut engine = identity_only_orders(
        "[]",
        identity,
        std::sync::Arc::new(RefuseTakeProfit(signer)),
        None,
    );
    let mut transport = vec![];
    place(
        &mut engine,
        vec![
            json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":1,"price":140,"takeProfit":150,"stopLoss":130}),
        ],
    )?;
    let entry = packet(&mut engine, &mut transport)?;
    reply(&mut engine, &entry, 200, r#"{"id":"entry"}"#);
    let sl = packet(&mut engine, &mut transport)?;
    let row: Value = serde_json::from_str(sl.body.as_deref().ok_or("missing SL")?)?;
    assert_eq!(row["triggerPrice"], "130.00");
    reply(&mut engine, &sl, 200, r#"{"id":"sl"}"#);
    let out = actions(&mut engine);
    let error = failure(&out)?;
    assert_ne!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    let elements = error.elements.as_ref().ok_or("missing outcomes")?;
    assert_eq!(elements.len(), 3);
    assert!(
        matches!(&elements[1].result, ElementResult::NotSent { error:Some(error) } if error.outcome==Some(safe_cex_engine::Outcome::NotSent))
    );
    assert_eq!(
        error
            .partial
            .as_ref()
            .ok_or("missing accepted IDs")?
            .order_ids,
        ["entry", "sl"]
    );
    assert_eq!(transport.len(), 2);
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:728-779,1207-1229.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | sdk-rs/venues/backpack/src/trading.rs:522 pair-batching-and-serial-rows
#[test]
fn legacy_rounds_without_rechecking_wire_quantity_bounds() -> Result {
    for (min, raw, wire) in [(0.1, 0.95, "1.0"), (0.14, 0.14, "0.1")] {
        let (mut engine, _) = ready();
        let mut transport = vec![];
        engine.store_mut().update_markets(|market| {
            market.precision.amount = 0.1;
            market.limits.amount.min = min;
            market.limits.amount.max = 0.95;
        });
        let mut last = limit(142.);
        last["amount"] = json!(raw);
        place(&mut engine, vec![limit(140.), limit(141.), last])?;
        let first = packet(&mut engine, &mut transport)?;
        assert!(first.url.ends_with("/orders"));
        reply(&mut engine, &first, 200, &pair_ack(&first)?);
        let last = packet(&mut engine, &mut transport)?;
        let row: Value = serde_json::from_str(last.body.as_deref().ok_or("missing last row")?)?;
        assert_eq!(row["quantity"], wire);
        sequence(
            &transport,
            &[
                (Method::Post, "/api/v1/orders"),
                (Method::Post, "/api/v1/order"),
            ],
        );
        reply(&mut engine, &last, 200, r#"{"id":"last"}"#);
        assert!(actions(&mut engine).iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { command: 901, .. })
        )));
    }
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:906-923,1121-1135.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | native maxPrice gate (placement, trigger, attached price and amendment)
#[test]
fn legacy_does_not_gate_price_caps_before_placement_or_cancel() -> Result {
    let markets = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01","maxQuantity":"1000","filters":{"price":{"maxPrice":"1000"}}}]"#;
    let (mut engine, _, _) = ready_catalogue(markets, "[]");
    let mut transport = vec![];
    place(&mut engine, vec![limit(140.), limit(141.), limit(1000.01)])?;
    let first = packet(&mut engine, &mut transport)?;
    reply(&mut engine, &first, 200, &pair_ack(&first)?);
    let last = packet(&mut engine, &mut transport)?;
    let row: Value = serde_json::from_str(last.body.as_deref().ok_or("missing over-cap row")?)?;
    assert_eq!(row["price"], "1000.01");
    sequence(
        &transport,
        &[
            (Method::Post, "/api/v1/orders"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    let (mut engine, _, _) = ready_catalogue(markets, &format!("[{ORDER}]"));
    let mut transport = vec![];
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 901,
            id: "123".into(),
            price: Some(1000.01),
            amount: None,
        }),
    );
    let cancel = packet(&mut engine, &mut transport)?;
    assert_eq!(cancel.method, Method::Delete);
    reply(
        &mut engine,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let replacement = packet(&mut engine, &mut transport)?;
    let row: Value =
        serde_json::from_str(replacement.body.as_deref().ok_or("missing replacement")?)?;
    assert_eq!(row["price"], "1000.01");
    sequence(
        &transport,
        &[
            (Method::Delete, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:906-923,991-1025,1031-1044.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | native maxPrice gate (placement, trigger, attached price and amendment)
#[test]
fn legacy_trigger_and_attached_prices_are_sent_past_catalogue_cap() -> Result {
    let markets = r#"[{"symbol":"SOL_USDC_PERP","marketType":"PERP","quantityIncrement":"0.01","priceIncrement":"0.01","maxQuantity":"1000","filters":{"price":{"maxPrice":"1000"}}}]"#;
    for order in [
        json!({"symbol":SYMBOL,"type":"stop_market","side":"sell","amount":1,"price":1000.01}),
        json!({"symbol":SYMBOL,"type":"market","side":"buy","amount":1,"takeProfit":1000.01}),
    ] {
        let (mut engine, _, _) = ready_catalogue(markets, "[]");
        let mut transport = vec![];
        place(&mut engine, vec![order])?;
        let request = packet(&mut engine, &mut transport)?;
        let row: Value =
            serde_json::from_str(request.body.as_deref().ok_or("missing trigger body")?)?;
        assert_eq!(
            row[if row.get("triggerPrice").is_some() {
                "triggerPrice"
            } else {
                "takeProfitTriggerPrice"
            }],
            "1000.01"
        );
        reply(&mut engine, &request, 200, r#"{"id":"accepted"}"#);
        assert!(actions(&mut engine).iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { command: 901, .. })
        )));
        assert!(requests(&mut engine).is_empty());
        sequence(&transport, &[(Method::Post, "/api/v1/order")]);
    }
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1223-1229.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | runtime row/pair failure suffix
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1223-1229.
#[test]
fn legacy_serial_failure_halts_unknown_with_complete_reporting() -> Result {
    unknown_stop("serial")
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:991-1025.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | runtime TP/SL child failure suffix
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:991-1025.
#[test]
fn legacy_tp_failure_halts_before_sl() -> Result {
    unknown_stop("tp")
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1010-1028,1223-1229.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | runtime TP/SL child failure suffix
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1010-1028.
#[test]
fn legacy_sl_failure_halts_before_later_entry() -> Result {
    unknown_stop("sl")
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1072-1079.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | CancelOrders accepted-ID-less effect and failed-row continuation
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1072-1079.
#[test]
fn legacy_cancel_failure_halts_without_inventing_ids() -> Result {
    unknown_stop("cancel")
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1104-1135.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | amendment changed-fill/fresh-reference gates
#[test]
fn legacy_amend_preserves_requested_total_after_cancel_fills() -> Result {
    for (amount, expected) in [(None, "5.00"), (Some(4.), "4.00"), (Some(1.), "1.00")] {
        let old = ORDER.replace("\"executedQuantity\":\"0\"", "\"executedQuantity\":\"2\"");
        let (mut engine, _) = ready_orders(&format!("[{old}]"));
        let mut transport = vec![];
        engine.handle(
            2000.,
            Event::Command(Command::AmendOrder {
                command: 901,
                id: "123".into(),
                price: Some(142.),
                amount,
            }),
        );
        let cancel = packet(&mut engine, &mut transport)?;
        assert_eq!(cancel.method, Method::Delete);
        reply(
            &mut engine,
            &cancel,
            200,
            r#"{"id":"123","status":"Cancelled","executedQuantity":"3"}"#,
        );
        let replacement = packet(&mut engine, &mut transport)?;
        let row: Value =
            serde_json::from_str(replacement.body.as_deref().ok_or("missing replacement")?)?;
        assert_eq!(row["quantity"], expected);
        sequence(
            &transport,
            &[
                (Method::Delete, "/api/v1/order"),
                (Method::Post, "/api/v1/order"),
            ],
        );
    }
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1104-1135.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | amendment changed-fill/fresh-reference gates
#[test]
fn legacy_stop_amend_does_not_add_a_position_or_reference_gate() -> Result {
    let stop=ORDER.replace("\"orderType\":\"Limit\"","\"orderType\":\"Market\",\"triggerPrice\":\"130\",\"triggerQuantity\":\"5\",\"reduceOnly\":true")
        .replace("\"side\":\"Bid\"","\"side\":\"Ask\"");
    let (mut engine, _) = ready_orders(&format!("[{stop}]"));
    let mut transport = vec![];
    engine.store_mut().set_positions(vec![]);
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 901,
            id: "123".into(),
            price: Some(125.),
            amount: None,
        }),
    );
    let cancel = packet(&mut engine, &mut transport)?;
    reply(
        &mut engine,
        &cancel,
        200,
        r#"{"id":"123","status":"Cancelled","executedQuantity":"0"}"#,
    );
    let replacement = packet(&mut engine, &mut transport)?;
    let row: Value = serde_json::from_str(replacement.body.as_deref().ok_or("missing stop")?)?;
    assert_eq!(row["triggerPrice"], "125.00");
    assert_eq!(row["triggerQuantity"], "5.00");
    sequence(
        &transport,
        &[
            (Method::Delete, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:980-1028.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | sdk-rs/venues/backpack/src/trading.rs:451 separate-TP-SL-children
#[test]
fn legacy_bracket_sends_each_rounded_quantity_without_a_wire_bound_gate() -> Result {
    let (mut engine, _) = ready();
    let mut transport = vec![];
    engine.store_mut().update_markets(|market| {
        market.precision.amount = 0.1;
        market.limits.amount.max = 0.95;
    });
    place(
        &mut engine,
        vec![
            json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":0.95,"price":140,"takeProfit":150,"stopLoss":130}),
        ],
    )?;
    for (index, id) in ["entry", "tp", "sl"].into_iter().enumerate() {
        let request = packet(&mut engine, &mut transport)?;
        let row: Value =
            serde_json::from_str(request.body.as_deref().ok_or("missing bracket body")?)?;
        assert_eq!(
            row[if index == 0 {
                "quantity"
            } else {
                "triggerQuantity"
            }],
            "1.0"
        );
        reply(&mut engine, &request, 200, &json!({"id":id}).to_string());
    }
    assert!(actions(&mut engine).iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 901, .. })
    )));
    sequence(
        &transport,
        &[
            (Method::Post, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
            (Method::Post, "/api/v1/order"),
        ],
    );
    assert!(requests(&mut engine).is_empty());
    Ok(())
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:984,1047-1049,1223-1229.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | runtime TP/SL child failure suffix
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1047-1049,1223-1229.
#[test]
fn legacy_failed_entry_halts_children_and_other_orders() -> Result {
    unknown_stop("entry")
}

// Legacy TS: packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1213-1219.
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: backpack | runtime row/pair failure suffix
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts:1213-1219.
#[test]
fn legacy_mixed_pair_failure_halts_the_next_pair() -> Result {
    unknown_stop("pair")
}
