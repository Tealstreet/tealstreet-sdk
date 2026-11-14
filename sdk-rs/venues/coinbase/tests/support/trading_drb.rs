//! Offline trading oracles: Coinbase's migration guide and OpenAPI 2.1.1.
use crate::{Coinbase, Options};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Outcome, Response, VenueAction, model::*,
    test_support::prepared_now,
};
use serde_json::{Value, json};

const SYMBOL: &str = "BTC_USDC-PERPETUAL";
fn fixtures() -> Value {
    serde_json::from_str(include_str!("../fixtures/drb-trading.json")).unwrap()
}
fn opts() -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":SYMBOL,"type":"limit","side":"buy",
        "amount":0.01,"price":65000.5,"clientOrderId":"fixture-entry"}))
    .unwrap()
}
fn engine() -> Engine<Coinbase> {
    let mut venue = Coinbase::new(Options {
        key: "organizations/test/apiKeys/trading".into(),
        secret: STANDARD.encode([7; 32]),
        ..Default::default()
    })
    .unwrap();
    venue.account.kind = Some(crate::ProductKind::Intx);
    let mut e = safe_cex_engine::test_support::seeded_engine(venue);
    e.store_mut().set_markets(vec![Market {
        id: "BTC_USDC-PERPETUAL".into(),
        symbol: SYMBOL.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDC".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.,
            },
            leverage: MinMax { min: 1., max: 50. },
            price: None,
        },
        contract_size: Some(1.),
        is_expiring: Some(false),
        is_inverse: Some(false),
        is_spot: Some(false),
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e.store_mut().set_orders(vec![]);
    e
}
fn packet(r: &HttpRequest) -> Value {
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}
fn requests(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.starts_with("https://drb.coinbase.com/api/v2/") => {
                let p = packet(r);
                let owned = matches!(
                    p["method"].as_str(),
                    Some(
                        "public/auth"
                            | "private/buy"
                            | "private/sell"
                            | "private/edit"
                            | "private/cancel"
                            | "private/cancel_all"
                            | "private/cancel_all_by_instrument"
                            | "private/set_leverage"
                            | "private/get_leverage"
                            | "private/get_order_state"
                            | "private/get_order_state_by_label"
                            | "private/get_open_orders_by_instrument"
                            | "private/get_open_orders"
                    )
                );
                (owned && p["params"]["include_isolated"] != true).then(|| r.clone())
            }
            _ => None,
        })
        .collect()
}
fn reply(e: &mut Engine<Coinbase>, r: &HttpRequest, result: Value) -> Vec<Action> {
    let body = json!({"jsonrpc":"2.0","id":packet(r)["id"],"result":result}).to_string();
    e.handle(
        safe_cex_engine::test_support::dispatch_now(e).max(1001.),
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    prepared_now(e)
}
fn command(e: &mut Engine<Coinbase>, c: Command) -> Vec<Action> {
    e.handle(
        safe_cex_engine::test_support::dispatch_now(e).max(1000.),
        Event::Command(c),
    );
    let a = prepared_now(e);
    let r = requests(&a);
    if r.first()
        .is_some_and(|r| packet(r)["method"] == "public/auth")
    {
        assert_eq!(r.len(), 1);
        assert_eq!(packet(&r[0])["params"]["grant_type"], "coinbase_cdp");
        reply(e, &r[0], fixtures()["auth"].clone())
    } else {
        a
    }
}
fn place(e: &mut Engine<Coinbase>, o: PlaceOrderOpts) -> HttpRequest {
    let a = command(
        e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![o],
        },
    );
    let r = requests(&a);
    assert_eq!(r.len(), 1, "expected a gateway write: {a:?}");
    assert!(
        r[0].url
            .starts_with("https://drb.coinbase.com/api/v2/private/")
    );
    assert!(
        r[0].headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("authorization")
                && v == "Bearer synthetic-trading-token")
    );
    r[0].clone()
}
fn done(a: &[Action]) -> bool {
    a.iter()
        .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
}
#[path = "trading_regressions.rs"]
mod regressions;
#[test]
fn limit_and_market_controls_are_numeric_and_explicit() {
    for tif in [
        TimeInForce::GoodTillCancel,
        TimeInForce::PostOnly,
        TimeInForce::ImmediateOrCancel,
        TimeInForce::FillOrKill,
    ] {
        let mut e = engine();
        let mut o = opts();
        o.time_in_force = Some(tif);
        o.reduce_only = Some(true);
        let p = packet(&place(&mut e, o));
        assert_eq!(p["method"], "private/buy");
        assert_eq!(p["params"]["instrument_name"], "BTC_USDC-PERPETUAL");
        assert_eq!(p["params"]["amount"], 0.01);
        assert_eq!(p["params"]["reduce_only"], true);
        assert_eq!(p["params"]["post_only"], tif == TimeInForce::PostOnly);
        assert_eq!(
            p["params"]["time_in_force"],
            match tif {
                TimeInForce::ImmediateOrCancel => "immediate_or_cancel",
                TimeInForce::FillOrKill => "fill_or_kill",
                _ => "good_til_cancelled",
            }
        );
    }
    let mut e = engine();
    let mut o = opts();
    o.kind = OrderType::Market;
    o.side = OrderSide::Sell;
    let p = packet(&place(&mut e, o));
    assert_eq!(p["method"], "private/sell");
    assert_eq!(p["params"]["type"], "market");
    assert!(p["params"].get("price").is_none());
}
#[test]
fn partial_fill_ack_preserves_executed_and_remaining_size() {
    let mut e = engine();
    let r = place(&mut e, opts());
    assert!(done(&reply(&mut e, &r, fixtures()["partial"].clone())));
    let o = e.store().order("USDC-94166").unwrap();
    assert_eq!(o.filled, 0.004);
    assert!((o.remaining - 0.006).abs() < 1e-12);
    assert_eq!(o.status, OrderStatus::Open);
}
#[test]
fn bracket_exits_are_linked_reduce_only_and_follow_partial_fills() {
    let mut e = engine();
    let mut o = opts();
    o.stop_loss = Some(60000.);
    o.take_profit = Some(70000.);
    o.sl_trigger_type = Some(StopTriggerType::MarkPrice);
    let p = packet(&place(&mut e, o));
    let p = &p["params"];
    assert_eq!(p["linked_order_type"], "one_triggers_one_cancels_other");
    assert_eq!(p["trigger_fill_condition"], "incremental");
    let legs = p["otoco_config"].as_array().unwrap();
    assert_eq!(legs.len(), 2);
    assert!(
        legs.iter()
            .all(|l| l["direction"] == "sell" && l["reduce_only"] == true && l["amount"] == 0.01)
    );
    assert!(legs.iter().any(|l| l["type"] == "stop_market"
        && l["trigger"] == "mark_price"
        && l["trigger_price"] == 60000.));
    assert!(
        legs.iter()
            .any(|l| l["type"] == "take_market" && l["trigger_price"] == 70000.)
    );
}
#[test]
fn standalone_protection_uses_native_trigger_orders() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        let mut e = engine();
        let mut o = opts();
        o.kind = kind;
        o.side = OrderSide::Sell;
        o.price = Some(60000.);
        o.reduce_only = Some(true);
        let p = packet(&place(&mut e, o));
        assert_eq!(
            p["params"]["type"],
            if kind == OrderType::StopLoss {
                "stop_market"
            } else {
                "take_market"
            }
        );
        assert_eq!(p["params"]["trigger_price"], 60000.);
        assert!(p["params"].get("price").is_none());
    }
}
#[test]
fn cancel_id_and_native_edit_preserve_partial_fills() {
    let mut e = engine();
    let r = place(&mut e, opts());
    reply(&mut e, &r, fixtures()["partial"].clone());
    let a = command(
        &mut e,
        Command::AmendOrder {
            command: 2,
            id: "USDC-94166".into(),
            price: Some(66000.),
            amount: Some(0.02),
        },
    );
    let r = requests(&a).pop().unwrap();
    let p = packet(&r);
    assert_eq!(p["method"], "private/edit");
    assert_eq!(p["params"]["amount"], 0.02);
    assert_eq!(p["params"]["price"], 66000.);
    let mut changed = fixtures()["partial"].clone();
    changed["order"]["amount"] = json!(0.02);
    changed["order"]["price"] = json!(66000.);
    assert!(done(&reply(&mut e, &r, changed)));
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 3,
            ids: vec!["USDC-94166".into()],
        },
    );
    let r = requests(&a).pop().unwrap();
    assert_eq!(packet(&r)["method"], "private/cancel");
    let mut canceled = fixtures()["partial"]["order"].clone();
    canceled["order_state"] = json!("cancelled");
    assert!(done(&reply(&mut e, &r, canceled)));
    assert_eq!(e.store().order("USDC-94166").unwrap().filled, 0.004);
}
#[test]
fn cancel_all_by_instrument_and_account_have_real_mutation_authority() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::CancelSymbolOrders {
            command: 1,
            symbol: SYMBOL.into(),
        },
    );
    let r = requests(&a).pop().unwrap();
    assert_eq!(packet(&r)["method"], "private/cancel_all_by_instrument");
    assert!(done(&reply(&mut e, &r, json!(0))));
    e.handle(
        1002.,
        Event::Command(Command::Venue {
            command: 2,
            action: VenueAction::CancelAccountOrders,
        }),
    );
    let a = prepared_now(&mut e);
    let r = requests(&a).pop().unwrap();
    assert_eq!(packet(&r)["method"], "private/cancel_all");
    assert!(done(&reply(&mut e, &r, json!(2))));
}
#[test]
fn leverage_uses_gateway_and_unsupported_modes_are_refused_before_io() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 1,
            symbol: SYMBOL.into(),
            leverage: 5.,
            side: None,
        },
    );
    let r = requests(&a).pop().unwrap();
    assert_eq!(packet(&r)["method"], "private/set_leverage");
    assert_eq!(packet(&r)["params"]["leverage"], 5.);
    for c in [
        Command::SetMarginType {
            command: 2,
            symbol: SYMBOL.into(),
            isolated: true,
        },
        Command::SetPositionMode {
            command: 3,
            symbol: Some(SYMBOL.into()),
            hedged: true,
        },
    ] {
        let a = command(&mut e, c);
        assert!(requests(&a).is_empty());
        assert!(a.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.outcome == Some(Outcome::NotSent))));
    }
}
#[test]
fn native_reject_is_definitive_but_unlisted_codes_and_loss_are_unknown() {
    for name in ["insufficient_margin", "unknown"] {
        let mut e = engine();
        let r = place(&mut e, opts());
        let body =
            json!({"jsonrpc":"2.0","id":packet(&r)["id"],"error":fixtures()[name]}).to_string();
        e.handle(
            1001.,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
            },
        );
        let a = prepared_now(&mut e);
        assert!(a.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.outcome == Some(if name == "unknown" { Outcome::Unknown } else { Outcome::Rejected }))));
        assert!(requests(&a).iter().all(|r| !matches!(
            packet(r)["method"].as_str(),
            Some("private/buy" | "private/sell")
        )));
    }
}
#[test]
fn unknown_outcome_reads_back_by_label_without_replaying_write() {
    let mut e = engine();
    let r = place(&mut e, opts());
    e.handle(
        1001.,
        Event::HttpFailed {
            request: r.id,
            error: "connection reset",
        },
    );
    let a = prepared_now(&mut e);
    let reads = requests(&a);
    assert!(a.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.outcome == Some(Outcome::Unknown))));
    let r = reads
        .iter()
        .find(|r| packet(r)["method"] == "private/get_order_state_by_label")
        .expect("uncertain placement needs a primary-account label read");
    assert_eq!(packet(r)["params"]["label"], "fixture-entry");
    assert!(reads.iter().all(|r| packet(r)["method"] != "private/buy"));
    reply(&mut e, r, json!([fixtures()["partial"]["order"]]));
    assert_eq!(e.store().order("USDC-94166").unwrap().filled, 0.004);
}

#[test]
fn dispatched_http_failures_are_unknown_and_never_put_the_write_back() {
    for status in [408, 429, 500, 502, 503, 504] {
        let mut e = engine();
        let r = place(&mut e, opts());
        // Even a native refusal body cannot establish a transport-level refusal.
        let body =
            json!({"jsonrpc":"2.0","id":packet(&r)["id"],"error":fixtures()["reject"]}).to_string();
        e.handle(
            1001.,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "Retry-After: 1",
                    body: &body,
                },
            },
        );
        let actions = prepared_now(&mut e);
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.outcome == Some(Outcome::Unknown))), "status {status}: {actions:?}");
        assert!(!done(&actions));
        assert!(e.store().orders().is_empty());
        assert!(
            requests(&actions)
                .iter()
                .all(|r| packet(r)["method"] != "private/buy")
        );
        let mut fresh = opts();
        fresh.client_order_id = Some(format!("fresh-{status}"));
        let actions = command(
            &mut e,
            Command::PlaceOrders {
                command: 2,
                orders: vec![fresh],
            },
        );
        assert!(
            requests(&actions)
                .iter()
                .all(|r| packet(r)["method"] != "private/buy"),
            "an uncertain dispatched write must fence new placements"
        );
    }
}
#[test]
fn reused_label_and_unsupported_controls_are_refused_without_io() {
    let mut e = engine();
    let r = place(&mut e, opts());
    reply(&mut e, &r, fixtures()["partial"].clone());
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 2,
            orders: vec![opts()],
        },
    );
    assert!(requests(&a).is_empty());
    assert!(!done(&a));
    let mut e = engine();
    let mut o = opts();
    o.max_slippage_pct = Some(1.);
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![o],
        },
    );
    assert!(requests(&a).is_empty());
    assert!(!done(&a));
}

#[test]
fn timeout_seals_once_as_unknown_and_only_reads_back() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts()],
        },
    );
    let timers: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms <= 32_000. => Some(*timer),
            _ => None,
        })
        .collect();
    assert!(!timers.is_empty());
    let mut all = vec![];
    for timer in timers {
        e.handle(32_000., Event::TimerFired { timer });
        all.extend(prepared_now(&mut e));
    }
    assert_eq!(all.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, error }) if error.outcome == Some(Outcome::Unknown))).count(), 1);
    assert!(
        requests(&all)
            .iter()
            .all(|r| packet(r)["method"] != "private/buy")
    );
    assert!(
        requests(&all)
            .iter()
            .any(|r| packet(r)["method"] == "private/get_order_state_by_label")
    );
}

#[test]
fn mismatched_ack_controls_and_quantities_never_publish_success() {
    for (field, value) in [
        ("amount", json!(5.)),
        ("direction", json!("sell")),
        ("reduce_only", json!(true)),
        ("order_type", json!("market")),
        ("filled_amount", json!(100.)),
        ("label", json!("other")),
        ("instrument_name", json!("ETH_USDC-PERPETUAL")),
    ] {
        let mut e = engine();
        let r = place(&mut e, opts());
        let mut result = fixtures()["partial"].clone();
        result["order"][field] = value;
        let a = reply(&mut e, &r, result);
        assert!(!done(&a), "unproven {field} ACK cannot succeed");
        assert!(e.store().orders().is_empty());
    }
}

#[test]
fn stale_recovery_read_cannot_reopen_a_terminal_order_or_reduce_fills() {
    let mut e = engine();
    let r = place(&mut e, opts());
    e.handle(
        1001.,
        Event::HttpFailed {
            request: r.id,
            error: "lost",
        },
    );
    let a = prepared_now(&mut e);
    let read = requests(&a).pop().unwrap();
    let mut authoritative = fixtures()["partial"]["order"].clone();
    authoritative["filled_amount"] = json!(0.008);
    authoritative["order_state"] = json!("cancelled");
    authoritative["last_update_timestamp"] = json!(2000.);
    // Obtain a native record through the normal ACK codec in a separate engine.
    let mut other = engine();
    let r = place(&mut other, opts());
    reply(&mut other, &r, json!({"order":authoritative,"trades":[]}));
    e.store_mut()
        .upsert_order(other.store().order("USDC-94166").unwrap().clone());
    reply(&mut e, &read, json!([fixtures()["partial"]["order"]]));
    let order = e.store().order("USDC-94166").unwrap();
    assert_eq!(order.filled, 0.008);
    assert_eq!(order.status, OrderStatus::Canceled);
}

#[test]
fn auth_is_shared_by_overlapping_commands_and_stale_waiters_do_not_write() {
    let mut e = engine();
    e.handle(
        1000.,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts()],
        }),
    );
    let a = prepared_now(&mut e);
    let auth = requests(&a).pop().unwrap();
    let mut o = opts();
    o.client_order_id = Some("second".into());
    e.handle(
        1001.,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![o],
        }),
    );
    assert!(requests(&prepared_now(&mut e)).is_empty());
    let a = reply(&mut e, &auth, fixtures()["auth"].clone());
    assert_eq!(requests(&a).len(), 2);
    assert!(
        requests(&a)
            .iter()
            .all(|r| packet(r)["method"] == "private/buy")
    );

    let mut e = engine();
    e.handle(
        1000.,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts()],
        }),
    );
    let a = prepared_now(&mut e);
    let auth = requests(&a).pop().unwrap();
    let body =
        json!({"jsonrpc":"2.0","id":packet(&auth)["id"],"result":fixtures()["auth"]}).to_string();
    e.handle(
        32_000.,
        Event::HttpResponse {
            request: auth.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    let a = prepared_now(&mut e);
    assert!(requests(&a).is_empty());
    assert!(!done(&a));
}

#[test]
fn leverage_ack_updates_settings_and_query_uses_read_authority() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::SetLeverage {
            command: 1,
            symbol: SYMBOL.into(),
            leverage: 5.,
            side: None,
        },
    );
    let r = requests(&a).pop().unwrap();
    assert!(done(&reply(&mut e, &r, json!(5.))));
    assert_eq!(e.store().setting(SYMBOL).unwrap().leverage, 5.);
    let a = command(
        &mut e,
        Command::FetchLeverage {
            command: 2,
            symbol: SYMBOL.into(),
        },
    );
    let r = requests(&a).pop().unwrap();
    assert_eq!(packet(&r)["method"], "private/get_leverage");
    let a = reply(
        &mut e,
        &r,
        json!({"instrument_name":"BTC_USDC-PERPETUAL","leverage":5.,"max_leverage":50.}),
    );
    assert!(done(&a));
}

#[test]
fn empty_or_duplicate_label_recovery_does_not_prove_the_unknown_order_absent() {
    for rows in [
        json!([]),
        json!([
            fixtures()["partial"]["order"],
            fixtures()["partial"]["order"]
        ]),
    ] {
        let mut e = engine();
        let r = place(&mut e, opts());
        e.handle(
            1001.,
            Event::HttpFailed {
                request: r.id,
                error: "lost",
            },
        );
        let a = prepared_now(&mut e);
        let read = requests(&a).pop().unwrap();
        assert!(!done(&reply(&mut e, &read, rows)));
        assert!(e.store().orders().is_empty());
        let a = command(
            &mut e,
            Command::PlaceOrders {
                command: 2,
                orders: vec![opts()],
            },
        );
        assert!(requests(&a).is_empty());
        assert!(!done(&a));
    }
}

#[test]
fn a_new_client_id_cannot_repeat_an_uncertain_placement_before_readback() {
    let mut e = engine();
    let r = place(&mut e, opts());
    e.handle(
        1001.,
        Event::HttpFailed {
            request: r.id,
            error: "lost",
        },
    );
    let a = prepared_now(&mut e);
    let read = requests(&a).pop().unwrap();
    let mut o = opts();
    o.client_order_id = Some("new-id".into());
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 2,
            orders: vec![o.clone()],
        },
    );
    assert!(
        requests(&a).is_empty(),
        "an uncertain entry fences new IDs on the same instrument"
    );
    reply(&mut e, &read, json!([fixtures()["partial"]["order"]]));
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 3,
            orders: vec![o],
        },
    );
    assert_eq!(requests(&a).len(), 1);
}

#[test]
fn unknown_leverage_and_bulk_cancels_reconcile_with_read_methods() {
    for c in [
        Command::SetLeverage {
            command: 1,
            symbol: SYMBOL.into(),
            leverage: 5.,
            side: None,
        },
        Command::CancelSymbolOrders {
            command: 1,
            symbol: SYMBOL.into(),
        },
    ] {
        let mut e = engine();
        let a = command(&mut e, c);
        let r = requests(&a).pop().unwrap();
        e.handle(
            1001.,
            Event::HttpFailed {
                request: r.id,
                error: "lost",
            },
        );
        let reads = requests(&prepared_now(&mut e));
        assert!(reads.iter().any(|r| matches!(
            packet(r)["method"].as_str(),
            Some("private/get_leverage" | "private/get_open_orders_by_instrument")
        )));
        assert!(reads.iter().all(|r| !matches!(
            packet(r)["method"].as_str(),
            Some("private/set_leverage" | "private/cancel_all_by_instrument")
        )));
    }
}

#[test]
fn batch_keeps_accepted_ids_when_a_later_leg_is_refused() {
    let mut e = engine();
    let mut o = opts();
    o.client_order_id = Some("second-entry".into());
    let a = command(
        &mut e,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts(), o],
        },
    );
    let first = requests(&a).pop().unwrap();
    let a = reply(&mut e, &first, fixtures()["partial"].clone());
    let second = requests(&a).pop().unwrap();
    let body=json!({"jsonrpc":"2.0","id":packet(&second)["id"],"error":fixtures()["insufficient_margin"]}).to_string();
    e.handle(
        1002.,
        Event::HttpResponse {
            request: second.id,
            response: Response {
                status: 200,
                headers: "",
                body: &body,
            },
        },
    );
    let a = prepared_now(&mut e);
    assert!(!done(&a));
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed {error,..}) if error.partial.as_ref().is_some_and(|p|p.order_ids==["USDC-94166"]))));
    assert!(e.store().order("USDC-94166").is_some());
}

#[test]
fn absent_label_is_read_again_without_retrying_the_placement() {
    let mut e = engine();
    let r = place(&mut e, opts());
    e.handle(
        1001.,
        Event::HttpFailed {
            request: r.id,
            error: "lost",
        },
    );
    let a = prepared_now(&mut e);
    let r = requests(&a).pop().unwrap();
    let a = reply(&mut e, &r, json!([]));
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms < 10_000. => Some(*timer),
            _ => None,
        })
        .expect("absent label needs a later read");
    e.handle(6001., Event::TimerFired { timer });
    let r = requests(&prepared_now(&mut e));
    assert_eq!(r.len(), 1);
    assert_eq!(packet(&r[0])["method"], "private/get_order_state_by_label");
}

#[test]
fn shared_auth_wait_keeps_the_original_command_deadline() {
    let mut e = engine();
    e.handle(
        1000.,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![opts()],
        }),
    );
    let a = prepared_now(&mut e);
    let timers: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms >= 30_000. && *at_ms <= 31_000. => {
                Some(*timer)
            }
            _ => None,
        })
        .collect();
    let mut all = vec![];
    for timer in timers {
        e.handle(31_000., Event::TimerFired { timer });
        all.extend(prepared_now(&mut e));
    }
    assert!(all.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed {command:1,error}) if error.outcome==Some(Outcome::NotSent))));
    assert!(
        requests(&all)
            .iter()
            .all(|r| packet(r)["method"] != "private/buy")
    );
}

#[test]
fn cancel_by_native_id_does_not_require_a_cached_order() {
    let mut e = engine();
    let a = command(
        &mut e,
        Command::CancelOrders {
            command: 1,
            ids: vec!["USDC-94166".into()],
        },
    );
    let r = requests(&a)
        .pop()
        .expect("native cancel by ID must not need an order snapshot");
    assert_eq!(packet(&r)["method"], "private/cancel");
    let mut row = fixtures()["partial"]["order"].clone();
    row["order_state"] = json!("cancelled");
    assert!(done(&reply(&mut e, &r, row)));
}

#[test]
fn an_unchanged_read_does_not_resolve_an_uncertain_edit_or_cancel() {
    for cancel in [false, true] {
        let mut e = engine();
        let r = place(&mut e, opts());
        reply(&mut e, &r, fixtures()["partial"].clone());
        let c = if cancel {
            Command::CancelOrders {
                command: 2,
                ids: vec!["USDC-94166".into()],
            }
        } else {
            Command::AmendOrder {
                command: 2,
                id: "USDC-94166".into(),
                price: Some(66000.),
                amount: Some(0.02),
            }
        };
        let a = command(&mut e, c);
        let r = requests(&a).pop().unwrap();
        e.handle(
            1002.,
            Event::HttpFailed {
                request: r.id,
                error: "lost",
            },
        );
        let a = prepared_now(&mut e);
        let read = requests(&a).pop().unwrap();
        reply(&mut e, &read, fixtures()["partial"]["order"].clone());
        let mut o = opts();
        o.client_order_id = Some("after-unknown".into());
        let a = command(
            &mut e,
            Command::PlaceOrders {
                command: 3,
                orders: vec![o],
            },
        );
        assert!(
            requests(&a).is_empty(),
            "the read still shows the old state after unknown cancel={cancel}"
        );
    }
}

#[test]
fn every_trading_method_takes_its_authority_from_the_s0_classifier() {
    use super::Method;
    use safe_cex_engine::{classify::Intent, request_context::Operation};
    let methods = [
        Method::Buy,
        Method::Sell,
        Method::Edit,
        Method::Cancel,
        Method::CancelInstrument,
        Method::CancelAll,
        Method::SetLeverage,
        Method::GetLeverage,
    ];
    for method in methods {
        // Exhaustive: a new trading method must be listed and classified here.
        let read = match method {
            Method::GetLeverage => true,
            Method::Buy
            | Method::Sell
            | Method::Edit
            | Method::Cancel
            | Method::CancelInstrument
            | Method::CancelAll
            | Method::SetLeverage => false,
        };
        let intent = crate::drb::method_intent(method.name())
            .unwrap_or_else(|e| panic!("{} is unclassified: {e}", method.name()));
        assert_eq!(intent == Intent::Read, read, "{}", method.name());
        let expected = if read {
            Operation::Read
        } else {
            Operation::Mutation
        };
        assert_eq!(method.operation(), expected, "{}", method.name());
    }
}

/// Every dispatched drb write (place, OTOCO, amend, cancel, bulk cancel and
/// leverage) that meets 429/408/5xx or a gateway rate limit resolves Unknown.
/// The write is neither resent nor put back; only read-back may follow.
#[test]
fn every_dispatched_drb_write_rejected_by_transport_status_is_unknown_without_resend() {
    let sell = || {
        let mut o = opts();
        o.side = OrderSide::Sell;
        o
    };
    let otoco = || {
        let mut o = opts();
        o.stop_loss = Some(60000.);
        o.take_profit = Some(70000.);
        o
    };
    let place_cmd = |o: PlaceOrderOpts| Command::PlaceOrders {
        command: 2,
        orders: vec![o],
    };
    type Case = (&'static str, bool, Box<dyn Fn() -> Command>);
    let cases: Vec<Case> = vec![
        ("private/buy", false, Box::new(move || place_cmd(opts()))),
        ("private/sell", false, Box::new(move || place_cmd(sell()))),
        ("private/buy", false, Box::new(move || place_cmd(otoco()))),
        (
            "private/edit",
            true,
            Box::new(|| Command::AmendOrder {
                command: 2,
                id: "USDC-94166".into(),
                price: Some(66000.),
                amount: Some(0.02),
            }),
        ),
        (
            "private/cancel",
            true,
            Box::new(|| Command::CancelOrders {
                command: 2,
                ids: vec!["USDC-94166".into()],
            }),
        ),
        (
            "private/cancel_all_by_instrument",
            true,
            Box::new(|| Command::CancelSymbolOrders {
                command: 2,
                symbol: SYMBOL.into(),
            }),
        ),
        (
            "private/cancel_all",
            true,
            Box::new(|| Command::Venue {
                command: 2,
                action: VenueAction::CancelAccountOrders,
            }),
        ),
        (
            "private/set_leverage",
            true,
            Box::new(|| Command::SetLeverage {
                command: 2,
                symbol: SYMBOL.into(),
                leverage: 5.,
                side: None,
            }),
        ),
    ];
    // HTTP 200 with drb 10028 is the gateway's own rate limit on a released write.
    for status in [408, 429, 500, 502, 503, 504, 200] {
        for (method, resting, make) in &cases {
            let case = format!("{method} status {status}");
            let mut e = engine();
            if *resting {
                let first = place(&mut e, opts());
                assert!(done(&reply(&mut e, &first, fixtures()["partial"].clone())));
            }
            let before = e.store().orders().to_vec();
            let a = command(&mut e, make());
            let writes: Vec<_> = requests(&a)
                .into_iter()
                .filter(|r| packet(r)["method"] == *method)
                .collect();
            assert_eq!(writes.len(), 1, "{case}: {a:?}");
            let r = &writes[0];
            let error = if status == 200 {
                json!({"code":10028,"message":"too_many_requests"})
            } else {
                // A native refusal body cannot override the transport status.
                fixtures()["insufficient_margin"].clone()
            };
            let body = json!({"jsonrpc":"2.0","id":packet(r)["id"],"error":error}).to_string();
            e.handle(
                1001.,
                Event::HttpResponse {
                    request: r.id,
                    response: Response {
                        status,
                        headers: "Retry-After: 1",
                        body: &body,
                    },
                },
            );
            let after = prepared_now(&mut e);
            assert!(
                after.iter().any(
                    |a| matches!(a, Action::Notice(Notice::CommandFailed { command: 2, error })
                    if error.outcome == Some(Outcome::Unknown))
                ),
                "{case}: {after:?}"
            );
            assert!(!done(&after), "{case}");
            assert!(
                requests(&after)
                    .iter()
                    .all(|r| packet(r)["method"] != *method),
                "{case}: the write was resent"
            );
            assert_eq!(
                e.store().orders(),
                &before[..],
                "{case}: state was put back"
            );
            let mut fresh = opts();
            fresh.client_order_id = Some("after-unknown".into());
            let a = command(
                &mut e,
                Command::PlaceOrders {
                    command: 3,
                    orders: vec![fresh],
                },
            );
            assert!(
                requests(&a).iter().all(|r| !matches!(
                    packet(r)["method"].as_str(),
                    Some("private/buy" | "private/sell")
                )),
                "{case}: the uncertainty was dropped before read-back"
            );
        }
    }
}
