use super::*;
use safe_cex_engine::model::{Limits, MinMax, Precision};
use serde_json::json;

fn market() -> Market {
    Market {
        id: "EUR/USD".into(),
        symbol: "EUR/USD".into(),
        pretty_symbol: None,
        base: "EUR".into(),
        quote: "USD".into(),
        active: true,
        precision: Precision {
            price: 0.0001,
            amount: 1000.0,
        },
        limits: Limits {
            amount: MinMax {
                min: 1000.0,
                max: 10_000_000.0,
            },
            leverage: MinMax {
                min: 1.0,
                max: 30.0,
            },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}
fn opts(value: serde_json::Value) -> PlaceOrderOpts {
    serde_json::from_value(value).unwrap()
}

#[test]
fn official_single_limit_schema_uses_native_units_and_close_effect() {
    // DX REST section 6.2 / 9.1 single order example, with the SDK's GTC policy.
    let prepared = PreparedOrder::prepare(&opts(json!({"symbol":"EUR/USD","type":"limit","side":"buy","amount":300_000,"price":0.05,"reduceOnly":true})), &market()).unwrap();
    let body: serde_json::Value =
        serde_json::from_str(&prepared.body("tealstreet-test").unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"orderCode":"tealstreet-test","type":"LIMIT","instrument":"EUR/USD","quantity":300_000.0,"side":"BUY","limitPrice":0.05,"positionEffect":"CLOSE","tif":"GTC"})
    );
}

#[test]
fn market_omits_both_price_fields_even_when_caller_includes_price() {
    let prepared = PreparedOrder::prepare(&opts(json!({"symbol":"EUR/USD","type":"market","side":"sell","amount":300_000,"price":1.2,"timeInForce":"FillOrKill"})), &market()).unwrap();
    let body: serde_json::Value =
        serde_json::from_str(&prepared.body("tealstreet-test").unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"orderCode":"tealstreet-test","type":"MARKET","instrument":"EUR/USD","quantity":300_000.0,"side":"SELL","positionEffect":"OPEN","tif":"GTC"})
    );
}

#[test]
fn stop_loss_and_take_profit_use_stop_price_without_limit_price() {
    for (kind, field) in [
        ("stop_market", "stopLoss"),
        ("take_profit_market", "takeProfit"),
    ] {
        let mut input =
            json!({"symbol":"EUR/USD","type":kind,"side":"sell","amount":300_000,"price":1.5});
        input[field] = json!(1.1);
        let prepared = PreparedOrder::prepare(&opts(input), &market()).unwrap();
        let body: serde_json::Value =
            serde_json::from_str(&prepared.body("tealstreet-test").unwrap()).unwrap();
        assert_eq!(body["type"], "STOP");
        assert_eq!(body["stopPrice"], 1.1);
        assert!(body.get("limitPrice").is_none());
        assert!(body.get("positionCode").is_none());
    }
}

#[test]
fn explicit_trigger_price_has_ts_precedence() {
    let prepared = PreparedOrder::prepare(&opts(json!({"symbol":"EUR/USD","type":"stop_market","side":"sell","amount":300_000,"triggerPrice":1.25,"stopLoss":1.1})), &market()).unwrap();
    assert_eq!(prepared.stop_price, Some(1.25));
}

#[test]
fn numeric_rounding_matches_ts_half_up_step_adjustment() {
    let prepared = PreparedOrder::prepare(&opts(json!({"symbol":"EUR/USD","type":"limit","side":"buy","amount":300_500,"price":1.23456})), &market()).unwrap();
    assert_eq!(prepared.quantity, 301_000.0);
    assert_eq!(prepared.limit_price, Some(1.2346));
}

#[test]
fn caller_id_batch_preflight_wins_over_missing_market_or_invalid_earlier_order() {
    let store = Store::default();
    let orders = vec![
        opts(json!({"symbol":"missing","type":"limit","side":"buy","amount":0})),
        opts(
            json!({"symbol":"EUR/USD","type":"market","side":"buy","amount":1,"clientOrderId":""}),
        ),
    ];
    assert!(
        prepare_orders(&store, &orders)
            .unwrap_err()
            .contains("clientOrderId")
    );
}

#[test]
fn invalid_price_and_unsupported_types_fail_during_preflight() {
    for input in [
        json!({"symbol":"EUR/USD","type":"limit","side":"buy","amount":1000}),
        json!({"symbol":"EUR/USD","type":"stop_market","side":"buy","amount":1000}),
        json!({"symbol":"EUR/USD","type":"trailing_stop_market","side":"buy","amount":1000,"triggerPrice":1}),
        json!({"symbol":"EUR/USD","type":"conditional","side":"buy","amount":1000,"price":1}),
        json!({"symbol":"EUR/USD","type":"limit","side":"buy","amount":1,"price":1}),
        json!({"symbol":"EUR/USD","type":"limit","side":"buy","amount":1000,"price":0}),
    ] {
        assert!(PreparedOrder::prepare(&opts(input), &market()).is_err());
    }
}

#[test]
fn foreign_venue_controls_fail_even_when_their_numeric_value_is_zero() {
    for field in ["bybitSlippageToleranceTicks", "bybitBbo", "stpMode"] {
        let mut input = json!({"symbol":"EUR/USD","type":"market","side":"buy","amount":1000});
        input[field] = match field {
            "bybitBbo" => json!({"sideType":"Queue","level":1}),
            "stpMode" => json!("EXPIRE_TAKER"),
            _ => json!(0),
        };
        assert!(PreparedOrder::prepare(&opts(input), &market()).is_err());
    }
}

#[test]
fn numeric_ack_ids_are_mapped_to_generated_native_cancel_codes() {
    let mut state = TradingState::default();
    state.acknowledged.insert(
        "46073148".into(),
        Acknowledged {
            code: "tealstreet-generated".into(),
            symbol: "EUR/USD".into(),
            created_at: 0.0,
        },
    );
    let steps = state
        .cancel_steps(
            &Store::default(),
            vec!["46073148".into(), "816099:5".into(), "46073148".into()],
        )
        .unwrap();
    assert_eq!(steps.len(), 2);
    assert!(matches!(&steps[0], Step::Cancel { code, .. } if code == "tealstreet-generated"));
    assert!(matches!(&steps[1], Step::Cancel { code, .. } if code == "816099:5"));
}

#[test]
fn cancellation_locks_cover_numeric_ack_and_generated_client_code_aliases() {
    let mut state = TradingState::default();
    state.acknowledged.insert(
        "46073148".into(),
        Acknowledged {
            code: "tealstreet-generated".into(),
            symbol: "EUR/USD".into(),
            created_at: 0.0,
        },
    );
    state.jobs.insert(
        1,
        Job {
            cancel_targets: vec!["46073148".into()],
            ..Job::default()
        },
    );
    assert!(
        state
            .cancel_steps(&Store::default(), vec!["tealstreet-generated".into()])
            .is_err()
    );
    assert!(
        state
            .cancel_steps(&Store::default(), vec!["46073148".into()])
            .is_err()
    );
    assert!(
        state
            .cancel_steps(&Store::default(), vec!["different-code".into()])
            .is_ok()
    );
}

#[test]
fn native_ack_requires_nonzero_id_and_accepts_numeric_strings() {
    for body in [
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
        r#"{"orderId":"46073148"}"#,
    ] {
        assert_eq!(
            serde_json::from_str::<PlaceAck>(body)
                .unwrap()
                .order_id
                .text()
                .unwrap(),
            "46073148"
        );
    }
    for body in [
        r#"{"orderId":0}"#,
        r#"{"orderId":""}"#,
        r#"{"orderId":"0"}"#,
        r#"{"orderId":"000"}"#,
        r#"{"orderId":"abc"}"#,
        r#"{"orderId":"1.0"}"#,
        r#"{"orderId":"-1"}"#,
        r#"{"orderId":" 1"}"#,
        r#"{"orderId":"1e3"}"#,
    ] {
        assert!(
            serde_json::from_str::<PlaceAck>(body)
                .unwrap()
                .order_id
                .text()
                .is_err()
        );
    }
    assert!(serde_json::from_str::<PlaceAck>("{}").is_err());
}

#[test]
fn fresh_reconciliation_retires_absent_ack_after_grace_but_retains_open_aliases() {
    use safe_cex_engine::model::{Order, TimeInForce};
    let mut state = TradingState::default();
    state.acknowledged.insert(
        "46073148".into(),
        Acknowledged {
            code: "tealstreet-generated".into(),
            symbol: "EUR/USD".into(),
            created_at: 100.0,
        },
    );
    let mut store = Store::default();
    state.reconcile(&store, 60_099.0);
    assert!(state.acknowledged.contains_key("46073148"));
    let order = Order {
        id: "816099:5".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "EUR/USD".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 1.1,
        amount: 1.0,
        filled: 0.0,
        remaining: 1.0,
        reduce_only: false,
        close: None,
        client_order_id: Some("tealstreet-generated".into()),
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 1_000.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    };
    store.set_orders(vec![order]);
    state.jobs.insert(
        1,
        Job {
            cancel_targets: vec!["816099:5".into()],
            ..Job::default()
        },
    );
    assert!(state.cancel_steps(&store, vec!["46073148".into()]).is_err());
    assert!(
        state
            .cancel_steps(&store, vec!["tealstreet-generated".into()])
            .is_err()
    );
    state.jobs.clear();
    state.reconcile(&store, 3_600_000.0);
    assert!(state.acknowledged.contains_key("46073148"));
    store.set_orders(vec![]);
    state.reconcile(&store, 3_600_001.0);
    assert!(!state.acknowledged.contains_key("46073148"));
}
