//! Behavioral contracts use the documented native transaction fields.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome, TimerId};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::*;

fn order(value: Value) -> PlaceOrderOpts {
    serde_json::from_value(value).unwrap()
}
fn limit() -> PlaceOrderOpts {
    order(json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90}))
}
fn ready_unattributed() -> (Engine<Lighter>, safe_cex_engine::SocketId) {
    let mut opts = options();
    opts.testnet = true;
    ready_with(opts)
}
fn place(e: &mut Engine<Lighter>, orders: Vec<PlaceOrderOpts>) {
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders { command: 7, orders }),
    );
}
fn http(e: &mut Engine<Lighter>, path: &str) -> HttpRequest {
    http_at(e, NOW, path)
}
fn http_at(e: &mut Engine<Lighter>, now: f64, path: &str) -> HttpRequest {
    let actions = safe_cex_engine::test_support::prepared_actions(e, now);
    let debug = format!("{actions:?}");
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing request {path}: {debug}"))
}
fn decode(text: &str) -> String {
    let mut out = Vec::new();
    let mut bytes = text.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let pair = [bytes.next().unwrap(), bytes.next().unwrap()];
            out.push(u8::from_str_radix(std::str::from_utf8(&pair).unwrap(), 16).unwrap());
        } else {
            out.push(if b == b'+' { b' ' } else { b });
        }
    }
    String::from_utf8(out).unwrap()
}
fn param(r: &HttpRequest, key: &str) -> String {
    r.body
        .as_ref()
        .unwrap()
        .split('&')
        .find_map(|p| {
            let (k, v) = p.split_once('=')?;
            (k == key).then(|| decode(v))
        })
        .unwrap()
}
fn wire(r: &HttpRequest) -> Value {
    serde_json::from_str(&param(r, "tx_info")).unwrap()
}
fn begin(e: &mut Engine<Lighter>, orders: Vec<PlaceOrderOpts>) -> HttpRequest {
    place(e, orders);
    let nonce = http(e, "nextNonce");
    reply(e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    http(e, "sendTx")
}
fn ack(e: &mut Engine<Lighter>, r: &HttpRequest) {
    ack_at(e, NOW, r);
}
fn ack_at(e: &mut Engine<Lighter>, now: f64, r: &HttpRequest) {
    reply_at(
        e,
        now,
        r,
        200,
        r#"{"code":200,"tx_hash":"transaction-hash"}"#,
    );
}
fn timer(e: &mut Engine<Lighter>) -> TimerId {
    safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms < NOW + 500.0 => Some(timer),
            _ => None,
        })
        .expect("submission pace")
}
fn failure(e: &mut Engine<Lighter>) -> String {
    safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => {
                if error.outcome == Some(Outcome::Unknown) {
                    assert!(error.message.contains(ENGINE_UNKNOWN_TEXT), "{error:?}");
                }
                Some(error.message)
            }
            _ => None,
        })
        .expect("command rejection")
}

#[test]
fn caller_id_on_later_batch_item_rejects_before_any_read_or_write() {
    let (mut e, _) = ready_unattributed();
    let mut bad = limit();
    bad.client_order_id = Some("caller".into());
    place(&mut e, vec![limit(), bad]);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
}

#[test]
fn native_stop_limit_edit_never_converts_to_a_market_stop() {
    for kind in [3, 5] {
        let (mut e, socket) = ready_unattributed();
        message(
            &mut e,
            socket,
            NOW + 1.0,
            &json!({
                "channel":"account_all_orders", "type":"update/account_all_orders", "orders":[{
                    "order_index":"42", "market_id":1, "status":"open", "type":kind,
                    "is_ask":true, "reduce_only":true, "initial_base_amount":"2",
                    "filled_base_amount":"0", "remaining_base_amount":"2",
                    "trigger_price":"80", "price":"79", "time_in_force":1
                }]
            })
            .to_string(),
        );
        assert_eq!(
            e.store()
                .order("42")
                .unwrap()
                .extra
                .as_ref()
                .unwrap()
                .conditional_order_price,
            Some(79.0)
        );
        e.handle(
            NOW + 1.0,
            Event::Command(Command::AmendOrder {
                command: 8,
                id: "42".into(),
                price: Some(81.0),
                amount: None,
            }),
        );
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.0);
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))), "{out:?}");
        assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {error, ..}) if error.outcome == Some(Outcome::NotSent))), "{out:?}");
        assert!(e.store().order("42").is_some());
    }
}

fn rejects_bybit_control_without_batch_effects(bad: PlaceOrderOpts) {
    let (mut e, _) = ready_unattributed();
    let before = e.snapshot_frame(NOW).to_vec();
    e.take_frame(NOW);
    place(&mut e, vec![limit(), bad]);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    assert!(
        actions.iter().all(|action| matches!(
            action,
            Action::Notice(Notice::CommandFailed { command: 7, error })
                if error.message.contains("Bybit BBO and TickSize controls")
        )),
        "unexpected batch effects: {actions:?}"
    );
    assert_eq!(actions.len(), 1, "batch must reject exactly once");
    assert_eq!(e.snapshot_frame(NOW), before);
    assert!(e.take_frame(NOW).is_none(), "rejection dirtied the store");
}

#[test]
fn bybit_bbo_on_later_batch_item_rejects_before_any_read_write_or_mutation() {
    let mut bad = limit();
    bad.bybit_bbo = Some(BybitBbo {
        side_type: "Queue".into(),
        level: 1.0,
    });
    rejects_bybit_control_without_batch_effects(bad);
}

#[test]
fn bybit_ticks_on_later_batch_item_reject_before_any_read_write_or_mutation() {
    for ticks in [0.0, 1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut bad = limit();
        bad.bybit_slippage_tolerance_ticks = Some(ticks);
        rejects_bybit_control_without_batch_effects(bad);
    }
}

#[test]
fn placement_scales_market_metadata_and_returns_native_index() {
    let (mut e, _) = ready_unattributed();
    let r = begin(&mut e, vec![limit()]);
    let tx = wire(&r);
    assert_eq!(param(&r, "tx_type"), "14");
    assert_eq!(tx["BaseAmount"], 200);
    assert_eq!(tx["Price"], 9000);
    assert_eq!(tx["Nonce"], 10);
    assert_eq!(tx["TimeInForce"], 1);
    assert!(tx.get("L2TxAttributes").is_none());
    assert_eq!(tx["Sig"].as_str().unwrap().len(), 108);
    ack(&mut e, &r);
    let id = tx["ClientOrderIndex"].as_u64().unwrap().to_string();
    assert!(e.store().order(&id).is_some());
    assert!(e.store().order(&id).unwrap().client_order_id.is_none());
    assert!(safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone { result:ref ids,.. }) if ids==&format!("[\"{id}\"]"))));
}

#[test]
fn market_uses_side_bbo_before_mark_and_conditional_is_ioc_with_slippage_override() {
    let (mut e, _) = ready_unattributed();
    e.store_mut().update_ticker("ETH", |t| {
        t.ask = 110.0;
        t.mark = 100.0;
    });
    let r = begin(
        &mut e,
        vec![order(
            json!({"symbol":"ETH","type":"market","side":"buy","amount":0.001}),
        )],
    );
    assert_eq!(wire(&r)["Price"], 11220);
    assert_eq!(wire(&r)["TimeInForce"], 0);
    assert_eq!(wire(&r)["OrderExpiry"], 0);
    ack(&mut e, &r);
    safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .for_each(drop);
    place(
        &mut e,
        vec![order(
            json!({"symbol":"ETH","type":"stop_market","side":"sell","amount":0.2,"triggerPrice":95,"timeInForce":"PostOnly","maxSlippagePct":2}),
        )],
    );
    let r = http(&mut e, "sendTx");
    let tx = wire(&r);
    assert_eq!(tx["Type"], 2);
    assert_eq!(tx["Price"], 9310);
    assert_eq!(tx["TriggerPrice"], 9500);
    assert_eq!(tx["TimeInForce"], 0);
    assert!(tx["OrderExpiry"].as_u64().unwrap() > NOW as u64);
}

#[test]
fn plain_same_symbol_limits_use_batch_with_distinct_ids_and_sequential_nonces() {
    let (mut e, _) = ready_unattributed();
    let r = begin(&mut e, vec![limit(), limit()]);
    assert!(r.url.contains("sendTxBatch"));
    assert_eq!(param(&r, "tx_types"), "[14,14]");
    let infos: Vec<String> = serde_json::from_str(&param(&r, "tx_infos")).unwrap();
    let a: Value = serde_json::from_str(&infos[0]).unwrap();
    let b: Value = serde_json::from_str(&infos[1]).unwrap();
    assert_ne!(a["ClientOrderIndex"], b["ClientOrderIndex"]);
    assert_eq!(a["Nonce"], 10);
    assert_eq!(b["Nonce"], 11);
    reply(&mut e, &r, 200, r#"{"code":200,"tx_hash":["one"]}"#);
    assert!(failure(&mut e).contains(ENGINE_UNKNOWN_TEXT));
}

#[test]
fn generated_client_order_indices_are_receipted_before_the_signed_batch() {
    let (mut e, _) = ready_unattributed();
    place(&mut e, vec![limit(), limit()]);
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    let (progress_at, receipts) = actions
        .iter()
        .enumerate()
        .find_map(|(index, action)| match action {
            Action::Notice(Notice::CommandProgress {
                command: 7,
                receipts,
            }) => Some((index, receipts)),
            _ => None,
        })
        .expect("native client indices before submission");
    let (write_at, request) = actions
        .iter()
        .enumerate()
        .find_map(|(index, action)| match action {
            Action::Http(request) if request.url.contains("sendTxBatch") => Some((index, request)),
            _ => None,
        })
        .unwrap();
    assert!(progress_at < write_at);
    let infos: Vec<String> = serde_json::from_str(&param(request, "tx_infos")).unwrap();
    assert_eq!(receipts.len(), infos.len());
    for (receipt, info) in receipts.iter().zip(infos) {
        let wire: Value = serde_json::from_str(&info).unwrap();
        assert_eq!(receipt.symbol, "ETH");
        assert_eq!(receipt.kind, OrderIdentityKind::ClientOrderIndex);
        assert_eq!(
            receipt.identity,
            wire["ClientOrderIndex"].as_u64().unwrap().to_string()
        );
    }
    assert_ne!(receipts[0].identity, receipts[1].identity);
}

#[test]
fn bracket_waits_for_each_ack_and_gives_siblings_their_own_indexes() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2663-2683; base.ts:1456-1481.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:1885/serial-mixed-placement-and-market-children
    let (mut e, _) = ready_unattributed();
    let mut opts = limit();
    opts.kind = OrderType::Market;
    opts.price = None;
    opts.stop_loss = Some(95.0);
    opts.take_profit = Some(110.0);
    let first = begin(&mut e, vec![opts]);
    let entry = wire(&first);
    assert_eq!(entry["Type"], 1);
    ack(&mut e, &first);
    let t = timer(&mut e);
    e.handle(NOW + 100.0, Event::TimerFired { timer: t });
    let stop = http_at(&mut e, NOW + 100.0, "sendTx");
    let sl = wire(&stop);
    assert_eq!(sl["Type"], 2);
    assert_eq!(sl["IsAsk"], 1);
    assert_eq!(sl["ReduceOnly"], 1);
    assert_ne!(entry["ClientOrderIndex"], sl["ClientOrderIndex"]);
    ack_at(&mut e, NOW + 100.0, &stop);
    let t = timer(&mut e);
    e.handle(NOW + 200.0, Event::TimerFired { timer: t });
    let take = http_at(&mut e, NOW + 200.0, "sendTx");
    let tp = wire(&take);
    assert_eq!(tp["Type"], 4);
    assert_ne!(sl["ClientOrderIndex"], tp["ClientOrderIndex"]);
    ack_at(&mut e, NOW + 200.0, &take);
    assert!(safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone { result:ref ids,.. }) if serde_json::from_str::<Vec<String>>(ids).unwrap().len()==3)));
}

#[test]
fn lost_write_response_never_replays_and_next_command_refreshes_nonce() {
    let (mut e, _) = ready_unattributed();
    let r = begin(&mut e, vec![limit()]);
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("sendTx")))
    );
    assert!(actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. })
        if error.outcome == Some(Outcome::Unknown))
    ));
    // The Unknown fences its symbol until the reconcile's orders read lands.
    for read in actions.iter().filter_map(|a| match a {
        Action::Http(r) if r.url.contains("accountActiveOrders") => Some(r.clone()),
        _ => None,
    }) {
        reply(&mut e, &read, 200, OPEN);
    }
    place(&mut e, vec![limit()]);
    assert!(http(&mut e, "nextNonce").url.contains("api_key_index=1"));
}

#[test]
fn known_invalid_nonce_rejection_allows_only_one_fresh_nonce_retry() {
    let (mut e, _) = ready_unattributed();
    let r = begin(&mut e, vec![limit()]);
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":21104,"message":"invalid nonce"}"#,
    );
    let t = timer(&mut e);
    e.handle(NOW + 150.0, Event::TimerFired { timer: t });
    let nonce = http_at(&mut e, NOW + 150.0, "nextNonce");
    reply_at(
        &mut e,
        NOW + 150.0,
        &nonce,
        200,
        r#"{"code":200,"nonce":20}"#,
    );
    let retry = http_at(&mut e, NOW + 150.0, "sendTx");
    assert_eq!(wire(&retry)["Nonce"], 20);
    assert_eq!(
        wire(&retry)["ClientOrderIndex"],
        wire(&r)["ClientOrderIndex"]
    );
    reply(
        &mut e,
        &retry,
        200,
        r#"{"code":21104,"message":"invalid nonce"}"#,
    );
    assert!(failure(&mut e).contains("invalid nonce"));
}

#[test]
fn rejected_cancel_preserves_order_and_amend_does_not_place_replacement() {
    let (mut e, _) = ready_unattributed();
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: Some(92.0),
            amount: None,
        }),
    );
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let cancel = http(&mut e, "sendTx");
    assert_eq!(param(&cancel, "tx_type"), "15");
    reply(
        &mut e,
        &cancel,
        200,
        r#"{"code":21121,"message":"cancel rejected"}"#,
    );
    assert!(e.store().order("42").is_some());
    assert_eq!(failure(&mut e), "cancel rejected");
}

#[test]
fn successful_amend_uses_fresh_generated_index() {
    let (mut e, _) = ready_unattributed();
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: Some(92.0),
            amount: None,
        }),
    );
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let cancel = http(&mut e, "sendTx");
    ack(&mut e, &cancel);
    let t = timer(&mut e);
    e.handle(NOW + 100.0, Event::TimerFired { timer: t });
    let replacement = http_at(&mut e, NOW + 100.0, "sendTx");
    let tx = wire(&replacement);
    assert_ne!(tx["ClientOrderIndex"], 42);
    assert_eq!(tx["Price"], 9200);
    ack_at(&mut e, NOW + 100.0, &replacement);
    assert!(e.store().order("42").is_none());
}

#[test]
fn a_limit_whose_replacement_is_refused_says_the_order_is_gone() {
    let (mut e, _) = ready_unattributed();
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: Some(92.0),
            amount: None,
        }),
    );
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let cancel = http(&mut e, "sendTx");
    ack(&mut e, &cancel);
    let t = timer(&mut e);
    e.handle(NOW + 100.0, Event::TimerFired { timer: t });
    let replacement = http_at(&mut e, NOW + 100.0, "sendTx");
    reply_at(
        &mut e,
        NOW + 100.0,
        &replacement,
        200,
        r#"{"code":21702,"message":"replacement refused"}"#,
    );
    let error = failure(&mut e);
    assert!(error.contains("order is gone"), "{error}");
}

#[test]
fn a_stop_without_native_provenance_is_refused_before_cancel() {
    let (mut e, _) = ready_unattributed();
    e.store_mut().update_order("42", |o| {
        o.kind = OrderType::StopLoss;
        o.side = OrderSide::Sell;
        o.reduce_only = true;
    });
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: Some(92.0),
            amount: None,
        }),
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed {error,..}) if error.outcome == Some(Outcome::NotSent) && error.message.contains("original unavailable"))));
    assert!(e.store().order("42").is_some());
}

#[test]
fn fill_overtaking_cancel_ack_aborts_amendment() {
    let (mut e, _) = ready_unattributed();
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: Some(92.0),
            amount: None,
        }),
    );
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let cancel = http(&mut e, "sendTx");
    e.store_mut().update_order("42", |o| {
        o.filled = 1.0;
        o.remaining = 1.0;
    });
    ack(&mut e, &cancel);
    assert!(failure(&mut e).contains("filled during amendment"));
}

#[test]
fn serialized_settings_compose_latest_accepted_leverage_and_margin_mode() {
    let (mut e, _) = ready_unattributed();
    e.handle(
        NOW,
        Event::Command(Command::SetLeverage {
            command: 8,
            symbol: "ETH".into(),
            leverage: 12.0,
            side: None,
        }),
    );
    let nonce = http(&mut e, "nextNonce");
    e.handle(
        NOW,
        Event::Command(Command::SetMarginType {
            command: 9,
            symbol: "ETH".into(),
            isolated: true,
        }),
    );
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let leverage = http(&mut e, "sendTx");
    assert_eq!(wire(&leverage)["InitialMarginFraction"], 1000);
    ack(&mut e, &leverage);
    let margin = http(&mut e, "sendTx");
    assert_eq!(wire(&margin)["InitialMarginFraction"], 1000);
    assert_eq!(wire(&margin)["MarginMode"], 1);
    ack(&mut e, &margin);
    assert_eq!(
        e.store().setting("ETH").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
}

#[test]
fn uncertain_settings_write_fences_queued_composition_until_fresh_snapshot() {
    let (mut e, _) = ready_unattributed();
    e.handle(
        NOW,
        Event::Command(Command::SetLeverage {
            command: 8,
            symbol: "ETH".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let write = http(&mut e, "sendTx");
    e.handle(
        NOW,
        Event::Command(Command::SetMarginType {
            command: 9,
            symbol: "ETH".into(),
            isolated: true,
        }),
    );
    e.handle(
        NOW,
        Event::HttpFailed {
            request: write.id,
            error: "lost",
        },
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    let nonce = actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("nextNonce") => Some(r),
            _ => None,
        })
        .unwrap();
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":20}"#);
    assert!(failure(&mut e).contains("awaiting reconciliation"));
}

#[test]
fn known_standard_tier_rejects_attribution_before_nonce_or_write() {
    let (mut e, _) = ready();
    place(&mut e, vec![limit()]);
    let tier = http(&mut e, "accountLimits");
    assert!(
        tier.headers
            .iter()
            .any(|(name, _)| name == &"authorization")
    );
    reply(
        &mut e,
        &tier,
        200,
        r#"{"code":200,"user_tier_name":"standard"}"#,
    );
    assert!(failure(&mut e).contains("Plus or Premium"));
}

#[test]
fn approval_requires_unexpired_matching_integrator_and_sufficient_fee_caps() {
    for (expiry, cap) in [(NOW as u64 - 1, 300), (NOW as u64 + 100_000, 249)] {
        let (mut e, _) = ready();
        place(&mut e, vec![limit()]);
        let tier = http(&mut e, "accountLimits");
        reply(
            &mut e,
            &tier,
            200,
            r#"{"code":200,"user_tier_name":"premium"}"#,
        );
        let approval = http(&mut e, "/account?");
        reply(
            &mut e,
            &approval,
            200,
            &format!(
                r#"{{"code":200,"accounts":[{{"approved_integrators":[{{"account_index":158385,"approval_expiry":{expiry},"max_perps_taker_fee":{cap},"max_perps_maker_fee":0}}]}}]}}"#
            ),
        );
        assert!(failure(&mut e).contains("Approve Builder Fee"));
    }
}

#[test]
fn approved_attribution_is_present_on_signed_order() {
    let (mut e, _) = ready();
    place(&mut e, vec![limit()]);
    let tier = http(&mut e, "accountLimits");
    reply(
        &mut e,
        &tier,
        200,
        r#"{"code":200,"user_tier_name":"plus"}"#,
    );
    let approval = http(&mut e, "/account?");
    reply(
        &mut e,
        &approval,
        200,
        &format!(
            r#"{{"code":200,"accounts":[{{"approved_integrators":[{{"account_index":"158385","approval_expiry":{},"max_perps_taker_fee":300,"max_perps_maker_fee":0}}]}}]}}"#,
            NOW as u64 + 100_000
        ),
    );
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let r = http(&mut e, "sendTx");
    assert_eq!(
        wire(&r)["L2TxAttributes"],
        json!({"1":158385,"2":250,"3":0})
    );
}

#[test]
fn account_snapshot_started_during_settings_write_cannot_undo_acknowledgement() {
    for (command, leverage, margin) in [
        (
            Command::SetLeverage {
                command: 8,
                symbol: "ETH".into(),
                leverage: 5.0,
                side: None,
            },
            5.0,
            MarginType::Cross,
        ),
        (
            Command::SetMarginType {
                command: 8,
                symbol: "ETH".into(),
                isolated: true,
            },
            10.0,
            MarginType::Isolated,
        ),
    ] {
        // The private socket's first open is the resync this race needs.
        let mut opts = options();
        opts.testnet = true;
        let (mut e, socket) = ready_unopened_meta(opts, META);
        e.handle(NOW, Event::Command(command));
        let nonce = http(&mut e, "nextNonce");
        reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
        let write = http(&mut e, "sendTx");
        e.handle(NOW + 1.0, Event::WsOpened { socket });
        let held = requests_at(&mut e, NOW + 1.0)
            .into_iter()
            .find(|r| r.url.contains("/account?"))
            .unwrap();
        ack_at(&mut e, NOW + 1.0, &write);
        assert_eq!(e.store().positions()[0].leverage, leverage);
        assert_eq!(e.store().positions()[0].margin_type, Some(margin));
        let snapshot = ACCOUNT.replace(r#""position":"2""#, r#""position":"3""#);
        reply_at(&mut e, NOW + 2.0, &held, 200, &snapshot);
        assert_eq!(e.store().setting("ETH").unwrap().leverage, leverage);
        assert_eq!(e.store().setting("ETH").unwrap().margin_type, Some(margin));
        let position = &e.store().positions()[0];
        assert_eq!(position.leverage, leverage);
        assert_eq!(position.margin_type, Some(margin));
        assert_eq!(position.contracts, 3.0);
    }
}

#[test]
fn execution_after_cancel_ack_aborts_replacement_during_notification_suppression() {
    let (mut e, socket) = ready_unattributed();
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: Some(92.0),
            amount: None,
        }),
    );
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let cancel = http(&mut e, "sendTx");
    ack(&mut e, &cancel);
    let pace = timer(&mut e);
    message(
        &mut e,
        socket,
        NOW + 20.0,
        &format!(
            r#"{{"type":"update/account_all","channel":"account_all:718895","trades":{{"1":[{{"trade_id":1,"market_id":1,"price":"90","size":"0.2","timestamp":{},"bid_account_id":718895,"ask_account_id":2,"bid_id":"42"}}]}}}}"#,
            NOW as u64 + 20
        ),
    );
    e.handle(NOW + 100.0, Event::TimerFired { timer: pace });
    assert!(failure(&mut e).contains("filled during amendment"));
}

#[test]
fn an_omitted_flat_market_does_not_clear_uncertain_settings_barrier() {
    let (mut e, _) = ready_unattributed();
    e.handle(
        NOW,
        Event::Command(Command::SetLeverage {
            command: 8,
            symbol: "BTC".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let write = http(&mut e, "sendTx");
    e.handle(
        NOW,
        Event::HttpFailed {
            request: write.id,
            error: "lost response",
        },
    );
    let read = http(&mut e, "/account?");
    reply(&mut e, &read, 200, ACCOUNT);
    safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .for_each(drop);
    e.handle(
        NOW,
        Event::Command(Command::SetMarginType {
            command: 9,
            symbol: "BTC".into(),
            isolated: true,
        }),
    );
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":20}"#);
    assert!(failure(&mut e).contains("awaiting reconciliation"));
}

#[test]
fn signed_perp_market_indexes_above_legacy_sentinel_are_supported() {
    let (mut e, _) = ready_unattributed();
    let mut markets = e.store().markets().to_vec();
    markets.iter_mut().find(|m| m.symbol == "ETH").unwrap().id = "4096".into();
    e.store_mut().set_markets(markets);
    let request = begin(&mut e, vec![limit()]);
    assert_eq!(wire(&request)["MarketIndex"], 4096);
}

#[test]
fn closing_limit_edit_preserves_native_limit_type() {
    for (side, old_price, new_price) in [
        (OrderSide::Sell, 110.0, 95.0),
        (OrderSide::Buy, 90.0, 105.0),
        (OrderSide::Sell, 95.0, 110.0),
        (OrderSide::Buy, 105.0, 90.0),
        (OrderSide::Buy, 90.0, 100.0),
    ] {
        let (mut e, socket) = ready_unattributed();
        e.store_mut().update_order("42", |o| {
            o.side = side;
            o.price = old_price;
            o.reduce_only = true;
        });
        e.store_mut().update_positions(|p| {
            p.side = if side == OrderSide::Buy {
                PositionSide::Short
            } else {
                PositionSide::Long
            };
        });
        let source = json!({"order_index":"42","client_order_index":"123","market_id":1,"status":"open","type":0,"time_in_force":1,
            "is_ask":side==OrderSide::Sell,"reduce_only":true,"initial_base_amount":"2","filled_base_amount":"0","remaining_base_amount":"2",
            "price":old_price.to_string(),"trigger_price":"0","order_expiry":NOW as u64+1_000_000,"trigger_status":"na",
            "parent_order_index":0,"parent_order_id":"0","to_trigger_order_id_0":"0","to_trigger_order_id_1":"0","to_cancel_order_id_0":"0","order_flags":0});
        message(
            &mut e,
            socket,
            NOW,
            &json!({"channel":"account_all_orders","orders":[source.clone()]}).to_string(),
        );
        assert_eq!(e.store().order("42").unwrap().kind, OrderType::Limit);
        e.handle(
            NOW,
            Event::Command(Command::AmendOrder {
                command: 8,
                id: "42".into(),
                price: Some(new_price),
                amount: None,
            }),
        );
        let nonce = http(&mut e, "nextNonce");
        reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
        let cancel = http(&mut e, "sendTx");
        ack(&mut e, &cancel);
        let t = timer(&mut e);
        e.handle(NOW + 100.0, Event::TimerFired { timer: t });
        let proof = http_at(&mut e, NOW + 100.0, "accountOrders");
        let mut canceled = source;
        canceled["status"] = json!("canceled");
        reply_at(
            &mut e,
            NOW + 100.0,
            &proof,
            200,
            &json!({"code":200,"orders":[canceled]}).to_string(),
        );
        let replacement = http_at(&mut e, NOW + 100.0, "sendTx");
        let tx = wire(&replacement);
        assert_eq!(tx["Type"], 0, "{side:?}: {old_price} -> {new_price}");
        assert_eq!(tx["TriggerPrice"], 0);
    }
}

fn completion(e: &mut Engine<Lighter>) -> (String, Option<Outcome>) {
    safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => {
                if error.outcome == Some(Outcome::Unknown) {
                    assert!(error.message.contains(ENGINE_UNKNOWN_TEXT), "{error:?}");
                }
                Some((error.message, error.outcome))
            }
            _ => None,
        })
        .expect("command completion")
}

#[test]
fn only_uncertain_submissions_report_an_unknown_outcome() {
    for (status, body, unknown) in [
        (503, r#"{"code":503,"message":"service unavailable"}"#, true),
        (200, "not json", true),
        (200, r#"{"code":200,"tx_hash":[]}"#, true),
        (400, r#"{"message":"missing code"}"#, true),
        (400, r#"{"code":21132,"message":"margin mode"}"#, false),
        (400, r#"{"code":21600,"message":"invalid order"}"#, false),
    ] {
        let (mut e, _) = ready_unattributed();
        let r = begin(&mut e, vec![limit()]);
        reply(&mut e, &r, status, body);
        let (message, outcome) = completion(&mut e);
        assert_eq!(
            outcome == Some(Outcome::Unknown),
            unknown,
            "{status} {body}: {message}"
        );
    }
}

#[test]
fn trailing_stop_is_rejected_instead_of_placing_a_market_order() {
    let (mut e, _) = ready_unattributed();
    place(
        &mut e,
        vec![order(
            json!({"symbol":"ETH","type":"trailing_stop_market","side":"sell","amount":0.2,"price":95,"reduceOnly":true}),
        )],
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "{actions:?}"
    );
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 7, error })
            if error.message.contains("trailing")
    )));
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::ErrorKind;
    let failures = |e: &mut Engine<Lighter>| {
        safe_cex_engine::test_support::prepared_actions(e, NOW)
            .into_iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    // A numeric Lighter code on a sub-500 answer is an explicit refusal.
    let (mut e, _) = ready_unattributed();
    let r = begin(&mut e, vec![limit()]);
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":21120,"message":"invalid order"}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.message, "invalid order");
    assert_eq!(
        (error.kind, error.outcome, error.venue_code.as_deref()),
        (
            Some(ErrorKind::Rejected),
            Some(Outcome::Rejected),
            Some("21120")
        )
    );
    // The sequencer may still accept individual transactions of a refused batch.
    let (mut e, _) = ready_unattributed();
    let r = begin(&mut e, vec![limit(), limit()]);
    assert!(r.url.contains("sendTxBatch"));
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":21120,"message":"invalid order"}"#,
    );
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    // A server error does not prove anything.
    let (mut e, _) = ready_unattributed();
    let r = begin(&mut e, vec![limit()]);
    reply(&mut e, &r, 503, r#"{"code":21500,"message":"busy"}"#);
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    // Preflight refusals were never sent.
    let (mut e, _) = ready_unattributed();
    let mut bad = limit();
    bad.client_order_id = Some("caller".into());
    place(&mut e, vec![bad]);
    let [error] = failures(&mut e).try_into().unwrap();
    assert_eq!(error.outcome, Some(Outcome::NotSent));
}

fn reduce_buy() -> PlaceOrderOpts {
    order(
        json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90,"reduceOnly":true}),
    )
}
#[test]
fn reduce_only_waits_for_slow_account_completion_instead_of_a_fixed_delay() {
    let (mut e, _) = ready_unattributed();
    place(&mut e, vec![reduce_buy()]);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    let account = out
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/account?") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    let timer = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > NOW && *at_ms <= NOW + 10000.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .unwrap();
    assert!(
        timer.1 > NOW + 2000.0,
        "position-read deadline must allow slow reads"
    );
    reply_at(
        &mut e,
        NOW + 3000.0,
        &account,
        200,
        &ACCOUNT.replace("\"sign\":1", "\"sign\":-1"),
    );
    let nonce = http_at(&mut e, NOW + 3000.0, "nextNonce");
    reply_at(
        &mut e,
        NOW + 3000.0,
        &nonce,
        200,
        r#"{"code":200,"nonce":10}"#,
    );
    let submit = http_at(&mut e, NOW + 3000.0, "sendTx");
    assert_eq!(wire(&submit)["ReduceOnly"], 1);
    ack_at(&mut e, NOW + 3000.0, &submit);
    assert!(
        safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
            .into_iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
    );
    e.handle(NOW + 11000.0, Event::TimerFired { timer: timer.0 });
    assert!(
        !safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
            .into_iter()
            .any(|a| matches!(
                a,
                Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
            ))
    );
}
#[test]
fn explicit_invalid_margin_rejection_refreshes_account_without_resubmitting() {
    let (mut e, _) = ready_unattributed();
    let submit = begin(&mut e, vec![limit()]);
    reply(
        &mut e,
        &submit,
        400,
        r#"{"code":21613,"message":"invalid margin mode"}"#,
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    assert!(
        out.iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("/account?")))
    );
    assert!(!out.iter().any(
        |a| matches!(a,Action::Http(r) if r.url.contains("sendTx")||r.url.contains("nextNonce"))
    ));
    assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed {command:7,error}) if error.venue_code.as_deref()==Some("21613")&&error.outcome==Some(Outcome::Rejected))));
}
#[test]
fn released_http_throttles_are_unknown_and_never_replay() {
    use safe_cex_engine::ErrorKind;
    for status in [405, 429] {
        let (mut e, _) = ready_unattributed();
        let submit = begin(&mut e, vec![limit()]);
        reply(
            &mut e,
            &submit,
            status,
            r#"{"code":23000,"message":"native throttle"}"#,
        );
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
            .into_iter()
            .collect::<Vec<_>>();
        let errors: Vec<_> = out
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1, "{out:?}");
        let error = errors[0];
        if status == 429 {
            assert_eq!(error.kind, Some(ErrorKind::RateLimited));
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert!(error.message.contains(ENGINE_UNKNOWN_TEXT));
            assert_eq!(error.venue_code, None);
        } else {
            assert_eq!(error.kind, Some(ErrorKind::Internal));
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert!(error.message.contains(ENGINE_UNKNOWN_TEXT));
            assert!(
                error
                    .venue_message
                    .as_deref()
                    .is_some_and(|text| text.contains("native throttle"))
            );
        }
        assert!(!out.iter().any(
            |a| matches!(a,Action::Http(r) if r.url.contains("sendTx")||r.url.contains("nextNonce"))
        ));
    }
}

fn position_wait(e: &mut Engine<Lighter>) -> (HttpRequest, TimerId) {
    place(e, vec![reduce_buy()]);
    let out = safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    let account = out
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/account?") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    let timer = out
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 10000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    assert!(!out.iter().any(
        |a| matches!(a,Action::Http(r) if r.url.contains("nextNonce")||r.url.contains("sendTx"))
    ));
    (account, timer)
}
fn assert_position_read_refused(e: &mut Engine<Lighter>) {
    let out = safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    let failures=out.iter().filter(|a|matches!(a,Action::Notice(Notice::CommandFailed {command:7,error}) if error.outcome==Some(Outcome::NotSent))).count();
    assert_eq!(failures, 1, "{out:?}");
    assert!(!out.iter().any(
        |a| matches!(a,Action::Http(r) if r.url.contains("nextNonce")||r.url.contains("sendTx"))
    ));
}
#[test]
fn incomplete_empty_rejected_and_failed_position_reads_never_authorize_placement() {
    for (status, body) in [
        (200, r#"{"code":200,"accounts":[]}"#),
        (
            200,
            r#"{"code":200,"accounts":[{"index":718895,"total_asset_value":"1000","available_balance":"900","positions":[]}] }"#,
        ),
        (503, r#"{"code":503,"message":"busy"}"#),
        (200, "not json"),
        (
            200,
            r#"{"code":200,"accounts":[{"index":999,"total_asset_value":"1000","available_balance":"900","positions":[]}] }"#,
        ),
    ] {
        let (mut e, _) = ready_unattributed();
        let (account, timer) = position_wait(&mut e);
        reply_at(&mut e, NOW + 500.0, &account, status, body);
        assert_position_read_refused(&mut e);
        e.handle(NOW + 11000.0, Event::TimerFired { timer });
        assert!(
            !safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
                .into_iter()
                .any(|a| matches!(
                    a,
                    Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
                ))
        );
    }
    let (mut e, _) = ready_unattributed();
    let (account, _) = position_wait(&mut e);
    e.handle(
        NOW + 500.0,
        Event::HttpFailed {
            request: account.id,
            error: "scripted failure",
        },
    );
    assert_position_read_refused(&mut e);
}
#[test]
fn settings_only_or_malformed_target_cannot_confirm_retained_exposure() {
    for body in [
        r#"{"code":200,"accounts":[{"index":718895,"total_asset_value":"1000","available_balance":"900","positions":[{"market_id":1,"margin_mode":0,"initial_margin_fraction":1000}]}]}"#,
        r#"{"code":200,"accounts":[{"index":718895,"total_asset_value":"1000","available_balance":"900","positions":[{"market_id":1,"position":"bad","sign":-1}]}]}"#,
    ] {
        let (mut e, socket) = ready_unattributed();
        let (account, _) = position_wait(&mut e);
        message(
            &mut e,
            socket,
            NOW + 100.0,
            r#"{"type":"update/account_all","channel":"account_all:718895","positions":{"1":{"market_id":1,"position":"2","sign":-1,"avg_entry_price":"100"}}}"#,
        );
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
            .into_iter()
            .collect::<Vec<_>>();
        assert!(!out.iter().any(
            |a| matches!(a,Action::Http(r) if r.url.contains("nextNonce")||r.url.contains("sendTx"))
        ));
        assert_eq!(e.store().positions()[0].side, PositionSide::Short);
        reply_at(&mut e, NOW + 500.0, &account, 200, body);
        assert_position_read_refused(&mut e);
        assert_eq!(e.store().positions()[0].side, PositionSide::Short);
    }
}
#[test]
fn timed_out_position_snapshot_cannot_release_a_queued_order() {
    let (mut e, _) = ready_unattributed();
    let (account, timer) = position_wait(&mut e);
    e.handle(
        NOW + 100.0,
        Event::Command(Command::PlaceOrders {
            command: 8,
            orders: vec![reduce_buy()],
        }),
    );
    assert!(
        !safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
            .into_iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
    e.handle(NOW + 10000.0, Event::TimerFired { timer });
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    for command in [7, 8] {
        assert_eq!(out.iter().filter(|a|matches!(a,Action::Notice(Notice::CommandFailed {command:id,error}) if *id==command&&error.outcome==Some(Outcome::NotSent))).count(),1,"{out:?}");
    }
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    reply_at(
        &mut e,
        NOW + 11000.0,
        &account,
        200,
        &ACCOUNT.replace("\"sign\":1", "\"sign\":-1"),
    );
    assert!(!safe_cex_engine::test_support::prepared_actions(&mut e, NOW).into_iter().any(
        |a| matches!(a,Action::Http(r) if r.url.contains("nextNonce")||r.url.contains("sendTx"))
    ));
}
#[test]
fn rate_limit_without_rejection_proof_keeps_unknown_outcome() {
    use safe_cex_engine::ErrorKind;
    for (status, body) in [
        (405, "not json"),
        (429, r#"{"code":23000}"#),
        (429, r#"{"code":200,"message":"native no hash"}"#),
        (405, r#"{"message":"native missing code"}"#),
    ] {
        let (mut e, _) = ready_unattributed();
        let submit = begin(&mut e, vec![limit()]);
        reply(&mut e, &submit, status, body);
        let [error] = typed_failures(&mut e).try_into().unwrap();
        assert_eq!(
            error.kind,
            Some(if status == 429 {
                ErrorKind::RateLimited
            } else {
                ErrorKind::Internal
            })
        );
        assert_eq!(error.outcome, Some(Outcome::Unknown));
    }
    let (mut e, _) = ready_unattributed();
    let submit = begin(&mut e, vec![limit(), limit()]);
    reply(
        &mut e,
        &submit,
        405,
        r#"{"code":23000,"message":"native throttle"}"#,
    );
    let [error] = typed_failures(&mut e).try_into().unwrap();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
}

fn typed_failures(e: &mut Engine<Lighter>) -> Vec<safe_cex_engine::VenueError> {
    safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect()
}

#[test]
fn position_read_failure_after_an_accepted_leg_does_not_claim_whole_command_not_sent() {
    let (mut e, _) = ready_unattributed();
    let submit = begin(&mut e, vec![limit(), reduce_buy()]);
    ack(&mut e, &submit);
    let account = http(&mut e, "/account?");
    e.handle(
        NOW + 200.0,
        Event::HttpFailed {
            request: account.id,
            error: "scripted failure",
        },
    );
    let [error] = typed_failures(&mut e).try_into().unwrap();
    assert_ne!(error.outcome, Some(Outcome::NotSent));
    assert!(error.message.contains("previously placed order indexes"));
}
#[test]
fn rate_limits_do_not_enter_nonce_retry_and_failed_prerequisites_are_not_sent() {
    use safe_cex_engine::ErrorKind;
    let (mut e, _) = ready_unattributed();
    let submit = begin(&mut e, vec![limit()]);
    reply(
        &mut e,
        &submit,
        405,
        r#"{"code":21104,"message":"invalid nonce"}"#,
    );
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::SetTimer {at_ms,..} if *at_ms < NOW+500.0))
    );
    assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed {error,..}) if error.kind==Some(ErrorKind::Rejected)&&error.outcome==Some(Outcome::Rejected))), "{out:?}");
    for status in [405, 429] {
        let (mut e, _) = ready_unattributed();
        place(&mut e, vec![limit()]);
        let nonce = http(&mut e, "nextNonce");
        reply(
            &mut e,
            &nonce,
            status,
            r#"{"code":23000,"message":"native throttle"}"#,
        );
        let [error] = typed_failures(&mut e).try_into().unwrap();
        assert_eq!(error.kind, Some(ErrorKind::RateLimited));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert_eq!(error.venue_message.as_deref(), Some("native throttle"));
    }
}

#[test]
fn bare_rate_code_fences_dependent_settings_until_reconciliation() {
    let (mut e, _) = ready_unattributed();
    e.handle(
        NOW,
        Event::Command(Command::SetLeverage {
            command: 8,
            symbol: "ETH".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let write = http(&mut e, "sendTx");
    e.handle(
        NOW,
        Event::Command(Command::SetMarginType {
            command: 9,
            symbol: "ETH".into(),
            isolated: true,
        }),
    );
    reply(&mut e, &write, 405, r#"{"code":23000}"#);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed {command:8,error}) if error.outcome==Some(Outcome::Unknown))));
    let nonce = out
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("nextNonce") => Some(r),
            _ => None,
        })
        .unwrap();
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":20}"#);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect::<Vec<_>>();
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("sendTx")))
    );
    assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed {command:9,error}) if error.message.contains("awaiting reconciliation"))));
}

#[test]
fn robinhood_attribution_uses_47296_or_the_explicit_override_on_both_networks() {
    for testnet in [false, true] {
        for override_index in [None, Some(999)] {
            let expected = override_index.unwrap_or(47_296);
            let mut options = options();
            options.robinhood = true;
            options.testnet = testnet;
            options.integrator_account_index = override_index;
            let (mut engine, _) = ready_with(options);
            place(&mut engine, vec![limit()]);
            let tier = http(&mut engine, "accountLimits");
            reply(
                &mut engine,
                &tier,
                200,
                r#"{"code":200,"user_tier_name":"plus"}"#,
            );
            let approval = http(&mut engine, "/account?");
            reply(
                &mut engine,
                &approval,
                200,
                &format!(
                    r#"{{"code":200,"accounts":[{{"approved_integrators":[{{"account_index":{expected},"approval_expiry":{},"max_perps_taker_fee":300,"max_perps_maker_fee":0}}]}}]}}"#,
                    NOW as u64 + 100_000
                ),
            );
            let nonce = http(&mut engine, "nextNonce");
            reply(&mut engine, &nonce, 200, r#"{"code":200,"nonce":10}"#);
            let submission = http(&mut engine, "sendTx");
            assert_eq!(
                wire(&submission)["L2TxAttributes"],
                json!({"1":expected,"2":250,"3":0})
            );
        }
    }
}

#[test]
fn documented_codes_in_a_200_refuse_a_placement_and_leave_no_fence() {
    for code in [21120, 21301, 21508, 21601, 21702, 21739] {
        let (mut e, _) = ready_unattributed();
        let r = begin(&mut e, vec![limit()]);
        let body = format!(r#"{{"code":{code},"message":"native"}}"#);
        reply(&mut e, &r, 200, &body);
        let (_, outcome) = completion(&mut e);
        assert_eq!(outcome, Some(Outcome::Rejected), "{code}");
        place(&mut e, vec![limit()]);
        http(&mut e, "nextNonce");
    }
}

#[test]
fn unproven_codes_in_a_200_stay_unknown_and_clear_on_the_next_read() {
    // 21728 is documented, but a duplicate says the first send may be live.
    for code in [29500, 21728, 99999, 23000, 23100] {
        let (mut e, _) = ready_unattributed();
        let r = begin(&mut e, vec![limit()]);
        let body = format!(r#"{{"code":{code},"message":"native"}}"#);
        reply(&mut e, &r, 200, &body);
        let settled = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        assert!(
            settled.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { error, .. })
                if error.outcome == Some(Outcome::Unknown))),
            "{code}: {settled:?}"
        );
        let reads: Vec<_> = settled
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) if r.url.contains("accountActiveOrders") => Some(r.clone()),
                _ => None,
            })
            .collect();
        assert!(!reads.is_empty(), "{code}: the read starts with the answer");
        place(&mut e, vec![limit()]);
        let fenced = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        assert!(
            fenced.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { error, .. })
                if error.message.contains("uncertain"))),
            "{code}: {fenced:?}"
        );
        for read in &reads {
            reply(&mut e, read, 200, OPEN);
        }
        place(&mut e, vec![limit()]);
        http(&mut e, "nextNonce");
    }
}

#[test]
fn a_refused_batch_in_a_200_keeps_its_unknown_handling() {
    let (mut e, _) = ready_unattributed();
    let r = begin(&mut e, vec![limit(), limit()]);
    assert!(r.url.contains("sendTxBatch"));
    reply(
        &mut e,
        &r,
        200,
        r#"{"code":21301,"message":"not enough collateral"}"#,
    );
    let (_, outcome) = completion(&mut e);
    assert_eq!(outcome, Some(Outcome::Unknown));
}

#[test]
fn a_maker_only_key_refusal_says_what_the_key_allows() {
    // The user's sendTx text; the numeric code was not captured, so both shapes are covered.
    for body in [
        r#"{"code":21800,"message":"maker-only api key can only send 0ms delay transactions"}"#,
        r#"{"message":"maker-only api key can only send 0ms delay transactions"}"#,
    ] {
        let (mut e, _) = ready_unattributed();
        e.handle(
            NOW,
            Event::Command(Command::SetLeverage {
                command: 8,
                symbol: "ETH".into(),
                leverage: 5.0,
                side: None,
            }),
        );
        let nonce = http(&mut e, "nextNonce");
        reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
        let write = http(&mut e, "sendTx");
        reply(&mut e, &write, 400, body);
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        let error = out
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 8, error }) => Some(error),
                _ => None,
            })
            .expect("maker-only failure");
        let diagnostic = "This Lighter API key is maker-only, which allows only post-only orders and cancels; change leverage or place market and stop orders from a key that is not maker-only: maker-only api key can only send 0ms delay transactions";
        if error.outcome == Some(Outcome::Unknown) {
            assert!(error.message.contains(ENGINE_UNKNOWN_TEXT));
            assert_eq!(
                error.venue_message.as_deref(),
                Some("maker-only api key can only send 0ms delay transactions")
            );
        } else {
            assert_eq!(error.message, diagnostic, "{body}");
        }
    }
}

#[test]
fn parity_grouped_children_use_parent_fill_without_child_minimums() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2514-2562.
    let metadata = META.replace("\"min_quote_amount\":\"1\"", "\"min_quote_amount\":\"100\"");
    let mut opts = options();
    opts.testnet = true;
    let (mut e, _) = ready_with_meta(opts, &metadata);
    let request = begin(
        &mut e,
        vec![order(json!({
            "symbol":"ETH", "type":"limit", "side":"buy", "amount":1,
            "price":100, "stopLoss":90, "takeProfit":110
        }))],
    );
    assert_eq!(param(&request, "tx_type"), "28");
    let payload = wire(&request);
    assert_eq!(payload["Orders"][0]["BaseAmount"], 1000);
    for child in [&payload["Orders"][1], &payload["Orders"][2]] {
        assert_eq!(child["BaseAmount"], 0);
        assert_eq!(child["ReduceOnly"], 1);
        assert_eq!(child["TimeInForce"], 0);
    }
}

fn parity_engine_with_limits(min: f64, max: f64, min_quote: f64) -> Engine<Lighter> {
    parity_ready_with_limits(min, max, min_quote).0
}

fn parity_ready_with_limits(
    min: f64,
    max: f64,
    min_quote: f64,
) -> (Engine<Lighter>, safe_cex_engine::SocketId) {
    let mut metadata: Value = serde_json::from_str(META).unwrap();
    let row = &mut metadata["order_book_details"][0];
    row["supported_size_decimals"] = json!(0);
    row["min_base_amount"] = json!(min.to_string());
    row["min_quote_amount"] = json!(min_quote.to_string());
    let mut options = options();
    options.testnet = true;
    let (mut e, socket) = ready_with_meta(options, &metadata.to_string());
    let mut markets = e.store().markets().to_vec();
    markets
        .iter_mut()
        .find(|m| m.symbol == "ETH")
        .unwrap()
        .limits
        .amount
        .max = max;
    e.store_mut().set_markets(markets);
    (e, socket)
}

fn parity_order(kind: OrderType, amount: f64) -> PlaceOrderOpts {
    let mut opts = limit();
    opts.kind = kind;
    opts.amount = amount;
    opts.price = Some(100.);
    opts
}

fn parity_refuses(mut e: Engine<Lighter>, orders: Vec<PlaceOrderOpts>) {
    place(&mut e, orders);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .count(),
        0,
        "invalid whole plan released transport"
    );
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(Outcome::NotSent))), "missing NotSent refusal");
}

fn parity_serial_sizes(mut e: Engine<Lighter>, orders: Vec<PlaceOrderOpts>) -> Vec<u64> {
    let mut request = begin(&mut e, orders);
    let mut sizes = Vec::new();
    let mut ids = std::collections::HashSet::new();
    let mut now = NOW;
    for _ in 0..12 {
        let tx = wire(&request);
        sizes.push(tx["BaseAmount"].as_u64().unwrap());
        assert!(
            ids.insert(tx["ClientOrderIndex"].as_u64().unwrap()),
            "duplicate lot identity"
        );
        ack_at(&mut e, now, &request);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        if actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
        {
            assert_eq!(sizes.len(), ids.len());
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.contains("sendTx")))
            );
            return sizes;
        }
        let (timer, at) = actions
            .into_iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if at_ms <= NOW + 1000. => Some((timer, at_ms)),
                _ => None,
            })
            .expect("next prepared lot pacing");
        now = at;
        e.handle(now, Event::TimerFired { timer });
        request = http_at(&mut e, now, "sendTx");
    }
    panic!("prepared plan did not finish");
}

#[test]
fn parity_market_orders_keep_the_legacy_minimum_exemption() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2433-2442.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:2109/max-qty-splitting-market-minima
    for (min, quote) in [(3.0, 1.0), (1.0, 400.0)] {
        assert_eq!(
            parity_serial_sizes(
                parity_engine_with_limits(min, 5.0, quote),
                vec![parity_order(OrderType::Market, 2.0)]
            ),
            [2]
        );
    }
}

#[test]
fn parity_mixed_placements_keep_the_market_minimum_exemption() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2433-2442,2711-2716.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:1885/serial-mixed-placement-and-market-children
    assert_eq!(
        parity_serial_sizes(
            parity_engine_with_limits(3.0, 5.0, 1.0),
            vec![
                parity_order(OrderType::Limit, 3.0),
                parity_order(OrderType::Market, 2.0),
            ]
        ),
        [3, 2]
    );
}

#[test]
fn parity_limit_lots_keep_legacy_remainder_allocation_and_tail_merge() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2483-2507.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:2109/max-qty-splitting-allocation-and-tail
    // KNOWN-ISSUE: legacy TS over-allocates (sum 9 of requested 8); kept for exact TS parity per Sam's decision.
    let sizes = parity_serial_sizes(
        parity_engine_with_limits(1.0, 5.0, 1.0),
        vec![parity_order(OrderType::Limit, 8.0)],
    );
    assert_eq!(sizes, [3, 3, 3]);
    assert_eq!(sizes.iter().sum::<u64>(), 9);

    // KNOWN-ISSUE: legacy TS over-allocates (sum 18 of requested 17); kept for exact TS parity per Sam's decision.
    let sizes = parity_serial_sizes(
        parity_engine_with_limits(3.0, 5.0, 1.0),
        vec![parity_order(OrderType::Limit, 17.0)],
    );
    assert_eq!(sizes, [4, 4, 4, 6]);
    assert_eq!(sizes.iter().sum::<u64>(), 18);
}

#[test]
fn parity_all_non_market_lots_validate_minimums_before_the_first_request() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2491-2507.
    parity_refuses(
        parity_engine_with_limits(4.0, 5.0, 1.0),
        vec![parity_order(OrderType::Limit, 8.0)],
    );
}

#[test]
fn parity_same_symbol_limit_batch_validates_the_last_row_before_transport() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2752-2799.
    parity_refuses(
        parity_engine_with_limits(3.0, 5.0, 1.0),
        vec![
            parity_order(OrderType::Limit, 3.0),
            parity_order(OrderType::Limit, 2.0),
        ],
    );
}

fn after_failure_next(e: &mut Engine<Lighter>, request: &HttpRequest) -> HttpRequest {
    ack(e, request);
    let timer = timer(e);
    e.handle(NOW + 100., Event::TimerFired { timer });
    http_at(e, NOW + 100., "sendTx")
}

fn after_failure_stops(e: &mut Engine<Lighter>, request: &HttpRequest) {
    reply_at(
        e,
        NOW + 100.,
        request,
        400,
        r#"{"code":21101,"message":"fixture refusal"}"#,
    );
    let mut actions = safe_cex_engine::test_support::prepared_actions(e, NOW + 100.);
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms <= NOW + 1000. => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    for (timer, at) in timers {
        e.handle(at, Event::TimerFired { timer });
        actions.extend(safe_cex_engine::test_support::prepared_actions(e, at));
    }
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("sendTx"))),
        "{actions:?}"
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. }))),
        "{actions:?}"
    );
}

#[test]
fn after_failure_market_lot_refusal_stops_later_exempt_lots() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2627-2655.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:2109/max-qty-splitting-market-minima
    // TS lighter.exchange.ts:2433-2442,2627-2655 exempts minima but throws on a lot failure.
    let mut e = parity_engine_with_limits(4., 5., 1000.);
    let first = begin(&mut e, vec![parity_order(OrderType::Market, 8.)]);
    assert_eq!(wire(&first)["BaseAmount"], 3);
    let second = after_failure_next(&mut e, &first);
    assert_eq!(wire(&second)["BaseAmount"], 3);
    after_failure_stops(&mut e, &second);
}

#[test]
fn after_failure_allocation_refusal_stops_the_legacy_oversized_tail() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2627-2655.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:2109/max-qty-splitting-allocation-and-tail
    // TS lighter.exchange.ts:2483-2499,2627-2655: [4,4,4,6], abort before the tail.
    let mut e = parity_engine_with_limits(3., 5., 1.);
    let first = begin(&mut e, vec![parity_order(OrderType::Limit, 17.)]);
    assert_eq!(wire(&first)["BaseAmount"], 4);
    let second = after_failure_next(&mut e, &first);
    assert_eq!(wire(&second)["BaseAmount"], 4);
    after_failure_stops(&mut e, &second);
}

#[test]
fn after_failure_valid_non_market_lots_stop_after_a_venue_refusal() {
    // TS lighter.exchange.ts:2491-2507,2646-2655: valid preflight does not swallow a refusal.
    let mut e = parity_engine_with_limits(1., 5., 1.);
    let first = begin(&mut e, vec![parity_order(OrderType::Limit, 8.)]);
    let second = after_failure_next(&mut e, &first);
    after_failure_stops(&mut e, &second);
}

#[test]
fn after_failure_serial_entry_refusal_stops_the_next_entry() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2711-2716.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:1885/serial-mixed-placement-and-market-children
    // TS lighter.exchange.ts:2711-2716 awaits each order without catching failures.
    let mut e = parity_engine_with_limits(1., 5., 1.);
    let first = begin(
        &mut e,
        vec![
            parity_order(OrderType::Limit, 1.),
            parity_order(OrderType::Market, 1.),
        ],
    );
    after_failure_stops(&mut e, &first);
}

#[test]
fn after_failure_market_child_refusal_skips_its_sibling_and_next_entry() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts:2673-2683.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:1885/serial-mixed-placement-and-market-children
    // TS lighter.exchange.ts:2673-2683 throws from the sibling loop; 2711-2716 stops entries.
    let mut e = parity_engine_with_limits(1., 5., 1.);
    let mut parent = parity_order(OrderType::Market, 1.);
    parent.stop_loss = Some(90.);
    parent.take_profit = Some(110.);
    let first = begin(&mut e, vec![parent, parity_order(OrderType::Limit, 1.)]);
    let sl = after_failure_next(&mut e, &first);
    assert_eq!(wire(&sl)["Type"], 2);
    after_failure_stops(&mut e, &sl);
}

#[test]
fn after_failure_native_limit_batch_is_not_retried_after_refusal() {
    // TS lighter.exchange.ts:2805-2823: one native batch, then rethrow (no per-row retry).
    let mut e = parity_engine_with_limits(1., 5., 1.);
    let batch = begin(&mut e, vec![parity_order(OrderType::Limit, 1.); 2]);
    assert!(batch.url.contains("sendTxBatch"));
    let rows: Vec<String> = serde_json::from_str(&param(&batch, "tx_infos")).unwrap();
    assert_eq!(rows.len(), 2);
    after_failure_stops(&mut e, &batch);
}

#[test]
fn after_failure_group_refusal_stops_the_next_entry_without_fallback() {
    // TS lighter.exchange.ts:2605-2621 rethrows grouped refusal; 2711-2716 stops entries.
    let mut e = parity_engine_with_limits(1., 5., 1.);
    let mut parent = parity_order(OrderType::Limit, 1.);
    parent.stop_loss = Some(90.);
    parent.take_profit = Some(110.);
    let group = begin(&mut e, vec![parent, parity_order(OrderType::Market, 1.)]);
    assert_eq!(param(&group, "tx_type"), "28");
    after_failure_stops(&mut e, &group);
}

#[test]
fn pin_grouped_parent_minimum_trigger_and_split_refuse_before_any_io() {
    // Legacy TS: lighter.exchange.ts:2491-2507,2514-2525 validates before grouped submit.
    for (amount, trigger) in [(1., 90.), (3., 0.), (6., 90.)] {
        let mut parent = parity_order(OrderType::Limit, amount);
        parent.stop_loss = Some(trigger);
        parity_refuses(parity_engine_with_limits(3., 5., 1.), vec![parent]);
    }
}

fn unknown_exception_empty_hash(mut e: Engine<Lighter>, orders: Vec<PlaceOrderOpts>) {
    let first = begin(&mut e, orders);
    let accepted = wire(&first)["ClientOrderIndex"].to_string();
    let second = after_failure_next(&mut e, &first);
    let payload = wire(&second);
    let unknown_ids: Vec<String> = match payload["Orders"].as_array() {
        Some(rows) => rows
            .iter()
            .map(|row| row["ClientOrderIndex"].to_string())
            .collect(),
        None => vec![payload["ClientOrderIndex"].to_string()],
    };
    reply_at(
        &mut e,
        NOW + 100.,
        &second,
        200,
        r#"{"code":200,"tx_hash":""}"#,
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 100.);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("sendTx"))),
        "{actions:?}"
    );
    let error = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .expect("Unknown terminal");
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(
        error.legacy_text().contains(&accepted),
        "accepted prefix missing: {error:?}"
    );
    assert!(error.message.contains(ENGINE_UNKNOWN_TEXT), "{error:?}");
    assert_eq!(
        error.partial.as_ref().unwrap().order_ids.as_slice(),
        std::slice::from_ref(&accepted)
    );
    for identity in &unknown_ids {
        assert!(
            error.message.contains(identity),
            "unknown identity missing: {error:?}"
        );
    }
    let event = actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => {
                Some(event)
            }
            _ => None,
        })
        .expect("Unknown journal telemetry");
    assert!(event.message.contains(ENGINE_UNKNOWN_TEXT));
    assert!(
        event
            .fields
            .iter()
            .any(|(key, value)| *key == "outcome" && value.as_str() == Some("unknown"))
    );
    let report = event
        .fields
        .iter()
        .find_map(|(key, value)| (*key == "journal").then(|| value.as_str()).flatten())
        .expect("journal snapshot");
    let decoded: Value = serde_json::from_str(report).unwrap();
    // Pin the legacy Value encoding: sorted keys, compact arrays, and string indexes.
    assert_eq!(
        report,
        format!(
            r#"{{"notSent":{},"placed":{},"unknown":{}}}"#,
            decoded["notSent"],
            json!([accepted]),
            json!(unknown_ids)
        )
    );
    assert_eq!(
        error.message,
        format!(
            "{ENGINE_UNKNOWN_TEXT}; unknown order indexes: {}; previously placed order indexes: {}",
            json!(unknown_ids),
            json!([accepted])
        )
    );
    let report = decoded;
    assert_eq!(report["placed"], json!([accepted]));
    assert_eq!(report["unknown"], json!(unknown_ids));
    assert!(!report["notSent"].as_array().unwrap().is_empty());
}

#[test]
fn unknown_exception_market_lots_stop_after_an_empty_hash() {
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:2109/max-qty-splitting-market-minima
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at lighter.exchange.ts:2627-2660; api-client.ts:620-623.
    unknown_exception_empty_hash(
        parity_engine_with_limits(4., 5., 1000.),
        vec![parity_order(OrderType::Market, 8.)],
    );
}

#[test]
fn unknown_exception_allocation_and_tail_stop_after_an_empty_hash() {
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:2109/max-qty-splitting-allocation-and-tail
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at lighter.exchange.ts:2627-2660; api-client.ts:620-623.
    unknown_exception_empty_hash(
        parity_engine_with_limits(3., 5., 1.),
        vec![parity_order(OrderType::Limit, 17.)],
    );
}

#[test]
fn unknown_exception_valid_non_market_lots_stop_after_an_empty_hash() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at lighter.exchange.ts:2627-2660; api-client.ts:620-623.
    unknown_exception_empty_hash(
        parity_engine_with_limits(1., 5., 1.),
        vec![parity_order(OrderType::Limit, 8.)],
    );
}

#[test]
fn unknown_exception_serial_placements_stop_after_an_empty_hash() {
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: lighter | trading.rs:1885/serial-mixed-placement-and-market-children
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at lighter.exchange.ts:2673-2683,2711-2716; api-client.ts:620-623.
    let mut bracket = parity_order(OrderType::Market, 1.);
    bracket.stop_loss = Some(90.);
    bracket.take_profit = Some(110.);
    unknown_exception_empty_hash(
        parity_engine_with_limits(1., 5., 1.),
        vec![bracket, parity_order(OrderType::Limit, 1.)],
    );
}

#[test]
fn unknown_exception_grouped_protection_stops_after_an_empty_hash() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at lighter.exchange.ts:2605-2608,2711-2716; api-client.ts:620-623.
    let mut bracket = parity_order(OrderType::Limit, 1.);
    bracket.stop_loss = Some(90.);
    bracket.take_profit = Some(110.);
    unknown_exception_empty_hash(
        parity_engine_with_limits(1., 5., 1.),
        vec![
            parity_order(OrderType::Limit, 1.),
            bracket,
            parity_order(OrderType::Market, 1.),
        ],
    );
}

#[test]
fn unknown_exception_prior_plain_prefix_stays_placed() {
    unknown_exception_empty_hash(
        parity_engine_with_limits(1., 5., 1.),
        vec![parity_order(OrderType::Market, 1.); 3],
    );
}

#[test]
fn unknown_exception_prior_unconfirmed_group_is_not_reported_placed() {
    unknown_exception_prior_group(0);
}

#[test]
fn unknown_exception_prior_group_keeps_only_observed_legs_placed() {
    for observed in [1, 3] {
        unknown_exception_prior_group(observed);
    }
}

fn unknown_exception_prior_group(observed: usize) {
    let (mut e, socket) = parity_ready_with_limits(1., 5., 1.);
    let mut bracket = parity_order(OrderType::Limit, 1.);
    bracket.stop_loss = Some(90.);
    bracket.take_profit = Some(110.);
    let first = begin(
        &mut e,
        vec![
            bracket,
            parity_order(OrderType::Market, 1.),
            parity_order(OrderType::Market, 1.),
        ],
    );
    assert_eq!(param(&first, "tx_type"), "28");
    let group_ids: Vec<String> = wire(&first)["Orders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["ClientOrderIndex"].to_string())
        .collect();
    assert_eq!(group_ids.len(), 3);
    let second = after_failure_next(&mut e, &first);
    let mut unknown = group_ids[observed..].to_vec();
    unknown.push(wire(&second)["ClientOrderIndex"].to_string());
    if observed > 0 {
        let rows: Vec<_> = group_ids[..observed]
            .iter()
            .enumerate()
            .map(|(i, id)| {
                json!({"order_index":(900_001 + i).to_string(), "client_order_index":id,
                    "market_id":1, "status":"open", "initial_base_amount":"1", "price":"100"})
            })
            .collect();
        message(
            &mut e,
            socket,
            NOW + 100.,
            &json!({"channel":"account_all_orders:718895", "orders":{"1":rows}}).to_string(),
        );
    }
    reply_at(
        &mut e,
        NOW + 100.,
        &second,
        200,
        r#"{"code":200,"tx_hash":""}"#,
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 100.);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("sendTx")))
    );
    let error = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .expect("terminal Unknown");
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(error.message.contains(ENGINE_UNKNOWN_TEXT));
    let journal = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown" => event
                .fields
                .iter()
                .find_map(|(key, value)| (*key == "journal").then(|| value.as_str()).flatten()),
            _ => None,
        })
        .expect("journal");
    let journal: Value = serde_json::from_str(journal).unwrap();
    assert_eq!(journal["placed"], json!(group_ids[..observed]), "{error:?}");
    assert_eq!(journal["unknown"], json!(unknown), "{error:?}");
    assert_eq!(journal["notSent"].as_array().unwrap().len(), 1);
    assert!(
        error
            .message
            .contains(&format!("unknown order indexes: {}", journal["unknown"]))
    );
    if observed == 0 {
        assert!(!error.message.contains("previously placed"), "{error:?}");
    } else {
        assert!(error.message.contains(&format!(
            "previously placed order indexes: {}",
            journal["placed"]
        )));
    }
}
