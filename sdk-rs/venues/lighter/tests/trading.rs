//! Behavioral contracts use the documented native transaction fields.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, TimerId};
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
    let actions = e.drain_actions().collect::<Vec<_>>();
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
    reply(e, r, 200, r#"{"code":200,"tx_hash":"transaction-hash"}"#);
}
fn timer(e: &mut Engine<Lighter>) -> TimerId {
    e.drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms < NOW + 500.0 => Some(timer),
            _ => None,
        })
        .expect("submission pace")
}
fn failure(e: &mut Engine<Lighter>) -> String {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                result: Err(error), ..
            }) => Some(error),
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
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
}

fn rejects_bybit_control_without_batch_effects(bad: PlaceOrderOpts) {
    let (mut e, _) = ready_unattributed();
    let before = e.snapshot_frame(NOW).to_vec();
    e.take_frame(NOW);
    place(&mut e, vec![limit(), bad]);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        actions.iter().all(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { command: 7, result: Err(error) })
                if error.contains("Bybit BBO and TickSize controls")
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
    assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(Notice::CommandDone { result:Ok(ref ids),.. }) if ids==&format!("[\"{id}\"]"))));
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
    e.drain_actions().for_each(drop);
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
    assert!(failure(&mut e).contains("Uncertain"));
}

#[test]
fn bracket_waits_for_each_ack_and_gives_siblings_their_own_indexes() {
    let (mut e, _) = ready_unattributed();
    let mut opts = limit();
    opts.stop_loss = Some(95.0);
    opts.take_profit = Some(110.0);
    let first = begin(&mut e, vec![opts]);
    let entry = wire(&first);
    assert_eq!(entry["Type"], 0);
    ack(&mut e, &first);
    let t = timer(&mut e);
    e.handle(NOW + 100.0, Event::TimerFired { timer: t });
    let stop = http(&mut e, "sendTx");
    let sl = wire(&stop);
    assert_eq!(sl["Type"], 2);
    assert_eq!(sl["IsAsk"], 1);
    assert_eq!(sl["ReduceOnly"], 1);
    assert_ne!(entry["ClientOrderIndex"], sl["ClientOrderIndex"]);
    ack(&mut e, &stop);
    let t = timer(&mut e);
    e.handle(NOW + 200.0, Event::TimerFired { timer: t });
    let take = http(&mut e, "sendTx");
    let tp = wire(&take);
    assert_eq!(tp["Type"], 4);
    assert_ne!(sl["ClientOrderIndex"], tp["ClientOrderIndex"]);
    ack(&mut e, &take);
    assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(Notice::CommandDone { result:Ok(ref ids),.. }) if serde_json::from_str::<Vec<String>>(ids).unwrap().len()==3)));
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
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("sendTx")))
    );
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone { result:Err(error),.. }) if error.contains("Uncertain"))));
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
    let nonce = http(&mut e, "nextNonce");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":20}"#);
    let retry = http(&mut e, "sendTx");
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
    let replacement = http(&mut e, "sendTx");
    let tx = wire(&replacement);
    assert_ne!(tx["ClientOrderIndex"], 42);
    assert_eq!(tx["Price"], 9200);
    ack(&mut e, &replacement);
    assert!(e.store().order("42").is_none());
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
    let t = timer(&mut e);
    e.handle(NOW + 100.0, Event::TimerFired { timer: t });
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
    let actions = e.drain_actions().collect::<Vec<_>>();
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
        let (mut e, socket) = ready_unattributed();
        e.handle(NOW, Event::Command(command));
        let nonce = http(&mut e, "nextNonce");
        reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
        let write = http(&mut e, "sendTx");
        e.handle(NOW + 1.0, Event::WsOpened { socket });
        let held = requests(&mut e)
            .into_iter()
            .find(|r| r.url.contains("/account?"))
            .unwrap();
        ack(&mut e, &write);
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
    e.drain_actions().for_each(drop);
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
fn amend_infers_protection_from_requested_price() {
    for (side, old_price, new_price, conditional) in [
        (OrderSide::Sell, 110.0, 95.0, true),
        (OrderSide::Buy, 90.0, 105.0, true),
        (OrderSide::Sell, 95.0, 110.0, false),
        (OrderSide::Buy, 105.0, 90.0, false),
        (OrderSide::Buy, 90.0, 100.0, true),
    ] {
        let (mut e, _) = ready_unattributed();
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
        let replacement = http(&mut e, "sendTx");
        let tx = wire(&replacement);
        assert_eq!(
            tx["Type"],
            if conditional { 2 } else { 0 },
            "{side:?}: {old_price} -> {new_price}"
        );
        assert_eq!(
            tx["TriggerPrice"],
            if conditional {
                (new_price * 100.0) as u64
            } else {
                0
            }
        );
    }
}
