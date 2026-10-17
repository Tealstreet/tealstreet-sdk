use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::{JsNumberInput, js_value_as_f64};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{
    Order, OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType,
};
use safe_cex_venue_kraken::orders;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    process::Command,
    task::{Context, Poll, Waker},
};

fn flag(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or(false)
}
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            f64::NAN.to_bits()
        } else {
            value.to_bits()
        }
    )
}
fn assert_json(native: &Value, expected: &Value) {
    match native {
        Value::Number(_) => assert_eq!(
            bits(js_value_as_f64(native).unwrap()),
            expected["number"],
            "ORDERS native retained number"
        ),
        Value::Array(rows) => {
            assert_eq!(rows.len(), expected.as_array().unwrap().len());
            for (i, row) in rows.iter().enumerate() {
                assert_json(row, &expected[i]);
            }
        }
        Value::Object(values) => {
            assert_eq!(values.len(), expected.as_object().unwrap().len());
            for (key, value) in values {
                assert_json(value, &expected[key]);
            }
        }
        _ => assert_eq!(native, expected, "ORDERS native retained primitive"),
    }
}
fn trigger(value: Option<OrderStopTriggerType>, expected: Option<&Value>) {
    let expected = expected.filter(|value| !value.is_object());
    let actual = value.map(|value| match value {
        OrderStopTriggerType::MarkPrice => "mark_price",
        OrderStopTriggerType::IndexPrice => "index_price",
        OrderStopTriggerType::LastPrice => "last_price",
    });
    assert_eq!(
        actual,
        expected.and_then(Value::as_str),
        "ORDERS native trigger"
    );
}
fn assert_order(native: &Order, expected: &Value) {
    assert_eq!(native.id, expected["id"], "ORDERS native id");
    assert_eq!(native.symbol, expected["symbol"], "ORDERS native symbol");
    assert_eq!(native.status, OrderStatus::Open, "ORDERS native status");
    assert_eq!(expected["status"], "open");
    assert_eq!(
        match native
            .r#type
            .value()
            .expect("present source enum in fixture")
        {
            OrderType::Limit => "limit",
            OrderType::Market => "market",
            OrderType::Conditional => "conditional",
            OrderType::StopLoss => "stop_market",
            OrderType::TakeProfit => "take_profit_market",
            OrderType::TrailingStopLoss => "trailing_stop_market",
        },
        expected["type"],
        "ORDERS native type"
    );
    assert_eq!(
        match native.side.value().expect("present source enum in fixture") {
            OrderSide::Buy => "buy",
            OrderSide::Sell => "sell",
        },
        expected["side"],
        "ORDERS native side"
    );
    for (key, value) in [
        ("price", native.price.to_number()),
        ("amount", native.amount),
        ("filled", native.filled),
        ("remaining", native.remaining),
        ("lastUpdate", native.last_update),
    ] {
        assert_eq!(
            bits(value),
            expected[key]["number"],
            "ORDERS native numeric {key}"
        );
    }
    assert_eq!(
        native.reduce_only,
        expected["reduceOnly"].as_bool().unwrap(),
        "ORDERS native reduceOnly"
    );
    assert_eq!(
        match native.time_in_force.unwrap() {
            OrderTimeInForce::GoodTillCancel => "GoodTillCancel",
            OrderTimeInForce::PostOnly => "PostOnly",
            OrderTimeInForce::ImmediateOrCancel => "ImmediateOrCancel",
            OrderTimeInForce::FillOrKill => "FillOrKill",
        },
        expected["timeInForce"],
        "ORDERS native timeInForce"
    );
    trigger(
        (native.sl_trigger_type).into_value(),
        expected.get("slTriggerType"),
    );
    trigger(
        (native.tp_trigger_type).into_value(),
        expected.get("tpTriggerType"),
    );
    assert!(
        native.parent_id.is_none()
            && native.close.is_none()
            && native.trigger_type.is_none()
            && native.take_profit.is_none()
            && native.stop_loss.is_none()
            && native.extra.is_none(),
        "ORDERS absent optional fields"
    );
    assert_json(native.info.as_ref().unwrap(), &expected["info"]);
}
fn fallback(tag: &str, n: f64) -> Order {
    Order {
        client_order_id: (None).into(),
        id: tag.into(),
        parent_id: (None).into(),
        status: safe_cex_types::ModelField::Value(OrderStatus::Open),
        symbol: format!("STORE-{tag}"),
        r#type: safe_cex_types::ModelField::Value(OrderType::Limit),
        side: safe_cex_types::ModelField::Value(OrderSide::Buy),
        price: (n + 1.25).into(),
        amount: n + 2.5,
        filled: n + 3.75,
        remaining: n + 4.125,
        reduce_only: true.into(),
        close: (None).into(),
        info: (Some(json!({"tag":tag}))).into(),
        time_in_force: (Some(OrderTimeInForce::PostOnly)).into(),
        last_update: n + 5.5,
        trigger_type: (None).into(),
        tp_trigger_type: (None).into(),
        sl_trigger_type: (None).into(),
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (None).into(),
    }
}
fn error(name: &str, message: &str, pointer: &Cell<usize>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = name.into();
    error.metadata.code = Some(safe_cex_exchange::js_runtime::value::Js::str(name));
    pointer.set((&*error.metadata as *const _) as usize);
    error
}
#[test]
fn futures_orders_live_source() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/orders-oracle.cjs"
        ))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(oracle["header"]["hashes"].as_object().unwrap().len(), 1);
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 67, "ORDERS independent cases");
    let mut records = 0;
    let mut clocks = 0;
    let mut errors = 0;
    let mut rejections = 0;
    let mut aliases = 0;
    let mut present_undefined = 0;
    for case in cases {
        let config = &case["config"];
        let store = DefaultStore::new();
        Store::update(
            &store,
            StoreUpdate {
                orders: Some(vec![fallback("OLD", 101.0)]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let trace = RefCell::new(Vec::new());
        let released = Cell::new(false);
        let clock = Cell::new(0);
        let read_pointer = Cell::new(0);
        let clock_pointer = Cell::new(0);
        let trace_pointer = Cell::new(0);
        let read = |endpoint| {
            trace
                .borrow_mut()
                .push(json!({"kind":"read","endpoint":endpoint}));
            if flag(config, "syncFail") {
                return Err(error("ReadError", "orders read failure", &read_pointer));
            }
            let mut result = Some(if flag(config, "readFail") {
                Err(error("ReadError", "orders read failure", &read_pointer))
            } else {
                Ok(config["raw"]
                    .as_str()
                    .map(|raw| serde_json::from_str::<JsNumberInput>(raw).unwrap()))
            });
            let released = &released;
            Ok(poll_fn(move |_| {
                if released.get() {
                    Poll::Ready(result.take().unwrap())
                } else {
                    Poll::Pending
                }
            }))
        };
        let now = || {
            clock.set(clock.get() + 1);
            if flag(config, "clockFail") {
                Err(error("ClockError", "orders clock failure", &clock_pointer))
            } else {
                Ok(config
                    .get("now")
                    .and_then(Value::as_f64)
                    .unwrap_or(1770900000123.75))
            }
        };
        let diagnostic = |value: &ExchangeError| {
            let pointer = (&*value.metadata as *const _) as usize;
            trace.borrow_mut().push(json!({"kind":"trace","name":value.name,"message":value.message,"readIdentity":pointer==read_pointer.get(),"clockIdentity":pointer==clock_pointer.get()}));
            if flag(config, "replaceTrace") {
                Store::update(
                    &store,
                    StoreUpdate {
                        orders: Some(vec![fallback("LATE", 211.0)]),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap()
            }
            if flag(config, "traceFail") {
                Err(error("TraceError", "orders trace failure", &trace_pointer))
            } else {
                Ok(())
            }
        };
        let mut future = Box::pin(orders::fetch_orders(&store, read, now, |ms| ms, diagnostic));
        let mut context = Context::from_waker(Waker::noop());
        let first = future.as_mut().poll(&mut context);
        assert_eq!(
            trace.borrow().len(),
            case["prefixTrace"].as_u64().unwrap() as usize,
            "ORDERS native callback prefix"
        );
        if flag(config, "replaceAwait") {
            Store::update(
                &store,
                StoreUpdate {
                    orders: Some(vec![fallback("LATE", 211.0)]),
                    ..Default::default()
                },
                false,
            )
            .unwrap()
        }
        released.set(true);
        let result = match first {
            Poll::Ready(result) => result,
            Poll::Pending => match future.as_mut().poll(&mut context) {
                Poll::Ready(result) => result,
                Poll::Pending => panic!("unsettled orders"),
            },
        };
        assert_eq!(
            *trace.borrow(),
            case["trace"].as_array().unwrap().clone(),
            "ORDERS native callbacks"
        );
        assert_eq!(
            clock.get(),
            case["clockCalls"].as_u64().unwrap() as usize,
            "ORDERS native clock calls"
        );
        clocks += clock.get();
        errors += trace.borrow().len() - 1;
        match result {
            Err(value) => {
                rejections += 1;
                assert_eq!(value.name, case["failure"]["name"]);
                assert_eq!(value.message, case["failure"]["message"]);
                assert_eq!(
                    (&*value.metadata as *const _) as usize,
                    trace_pointer.get(),
                    "ORDERS trace error identity"
                );
                assert_eq!(case["failure"]["traceIdentity"], true);
            }
            Ok(mut values) => {
                assert!(case["failure"].is_null());
                let expected = case["result"].as_array().unwrap();
                assert_eq!(values.len(), expected.len(), "ORDERS native length");
                records += values.len();
                for (index, (native, expected)) in values.iter().zip(expected).enumerate() {
                    assert_order(native, expected);
                    if native.r#type == OrderType::StopLoss && native.sl_trigger_type.is_none() {
                        assert_eq!(
                            case["own"][index]["sl"], true,
                            "ORDERS source owns undefined trigger"
                        );
                        present_undefined += 1;
                    }
                }
                if flag(case, "alias") {
                    aliases += 1;
                    assert_ne!(
                        values.as_ptr(),
                        store.orders().as_ptr(),
                        "ORDERS CHOSEN owned fallback"
                    );
                    assert_eq!(
                        case["storeAfterAlias"][0]["id"], "ALIASED",
                        "ORDERS source alias witness"
                    );
                    let before = store.orders()[0].id.clone();
                    values[0].id = "NATIVE-ONLY".into();
                    assert_eq!(
                        store.orders()[0].id,
                        before,
                        "ORDERS CHOSEN fallback mutation isolation"
                    );
                }
            }
        }
        for (native, expected) in store
            .orders()
            .iter()
            .zip(case["storeBefore"].as_array().unwrap())
        {
            assert_order(native, expected)
        }
    }
    assert_eq!(records, 60, "ORDERS independent records");
    assert_eq!(clocks, 14, "ORDERS independent clock calls");
    assert_eq!(errors, 10, "ORDERS independent diagnostics");
    assert_eq!(rejections, 1, "ORDERS independent rejections");
    assert_eq!(aliases, 9, "ORDERS independent alias pairs");
    assert_eq!(
        present_undefined, 2,
        "ORDERS independent undefined-trigger pairs"
    );
}
