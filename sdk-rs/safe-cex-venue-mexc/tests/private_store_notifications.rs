#[path = "support/private_store.rs"]
mod observed_store;
use observed_store::{ObservedStore, WriteCall};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{OrderStatus, PositionMode, PositionSide, StoreSubscriberError};
use safe_cex_venue_mexc::{utils::normalize_market, ws_private::MexcPrivateHandlers};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    fmt,
    rc::Rc,
};

#[derive(Debug)]
struct HandlerOrigin(u32);
impl fmt::Display for HandlerOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("private subscriber origin")
    }
}
impl Error for HandlerOrigin {}
const ROOTS: [(&str, usize, usize, usize); 12] = [
    ("order", 1, 1, 0),
    ("order", 2, 2, 1),
    ("order", 3, 3, 2),
    ("order", 0, 3, 3),
    ("position", 1, 1, 0),
    ("position", 0, 1, 1),
    ("plan", 1, 1, 0),
    ("plan", 0, 1, 1),
    ("stop", 1, 1, 0),
    ("stop", 0, 1, 1),
    ("direct-plan", 1, 1, 0),
    ("direct-stop", 1, 1, 0),
];
fn fixture(kind: &str) -> Value {
    let rows = match kind {
        "order" => json!([
            {"orderId":"order-closed","symbol":"BTC_USDT","side":1,"state":3,"vol":2,"dealVol":2,"price":10,"createTime":1000},
            {"orderId":"order-canceled","symbol":"BTC_USDT","side":3,"state":4,"vol":4,"dealVol":0,"errorCode":510,"price":20,"createTime":1000}
        ]),
        "position" => {
            json!([{"positionId":"position-7","symbol":"BTC_USDT","positionType":1,"holdVol":6,"holdAvgPrice":20,"leverage":4,"createTime":1000}])
        }
        "plan" => json!([
            {"id":"plan-open","symbol":"BTC_USDT","side":1,"state":1,"vol":4,"triggerPrice":21,"createTime":1000},
            {"id":"plan-terminal","symbol":"BTC_USDT","side":1,"state":2,"vol":2,"triggerPrice":22,"createTime":1000}
        ]),
        "stop" => json!([
            {"stopPlanOrderId":"stop-open","symbol":"BTC_USDT","positionType":1,"state":1,"vol":4,"stopLossPrice":5,"createTime":1000},
            {"stopPlanOrderId":"stop-terminal","symbol":"BTC_USDT","positionType":1,"state":2,"vol":2,"stopLossPrice":6,"createTime":1000}
        ]),
        _ => panic!("fixed native kind"),
    };
    json!({"data":rows})
}
#[test]
fn private_handlers_commit_before_original_subscriber_failure_and_abort_later_effects() {
    let mut first_total = 0;
    let mut later_total = 0;
    for (method, fail_at, expected_first, expected_later) in ROOTS {
        let kind = method.strip_prefix("direct-").unwrap_or(method);
        let store = ObservedStore::new();
        let market = normalize_market(
            json!({"symbol":"BTC_USDT","contractSize":0.5,"maxLeverage":20})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(vec![market]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let first = Rc::new(Cell::new(0));
        let later = Rc::new(Cell::new(0));
        let trace = Rc::new(RefCell::new(Vec::<String>::new()));
        let seen = Rc::new(RefCell::new(Vec::<String>::new()));
        let shields = Rc::new(RefCell::new(Vec::<String>::new()));
        let rejected = Rc::new(RefCell::new(Vec::<(String, f64)>::new()));
        let origin = Rc::new(HandlerOrigin(313));
        let calls = first.clone();
        let effects = trace.clone();
        let cause = origin.clone();
        let _first = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                let next = calls.get() + 1;
                calls.set(next);
                effects.borrow_mut().push("store".into());
                if next == fail_at {
                    Err(StoreSubscriberError::new(cause.to_string()).with_cause(cause.clone()))
                } else {
                    Ok(())
                }
            }),
        );
        let calls = later.clone();
        let _later = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                calls.set(calls.get() + 1);
                Ok(())
            }),
        );
        let calls = rejected.clone();
        let effects = trace.clone();
        let reject = Rc::new(move |id: String, code: f64| {
            effects.borrow_mut().push(format!("reject:{id}"));
            calls.borrow_mut().push((id, code));
            Ok(())
        });
        let calls = seen.clone();
        let effects = trace.clone();
        let mark = Rc::new(move |id: String| {
            effects.borrow_mut().push(format!("seen:{id}"));
            calls.borrow_mut().push(id);
        });
        let calls = shields.clone();
        let effects = trace.clone();
        let open = format!("{kind}-open");
        let shield = Rc::new(move |id: &str| {
            effects.borrow_mut().push(format!("shield:{id}"));
            calls.borrow_mut().push(id.into());
            id == open
        });
        // Source handlePosition omits the mode argument: configuring OneWay must
        // not replace normalizePosition's source default Hedged.
        let handlers = MexcPrivateHandlers::new(
            Rc::new(|v| v),
            Some(PositionMode::OneWay),
            reject,
            mark,
            shield,
            Rc::new(|_| -> Result<(), ExchangeError> {
                panic!("no fill in fixed private Store roots")
            }),
        );
        let frame = fixture(kind);
        let result = match method {
            "order" => handlers.handle_order(&store, &frame),
            "position" => handlers.handle_position(&store, &frame),
            "plan" => handlers.handle_plan_order(&store, &frame),
            "stop" => handlers.handle_stop_order(&store, &frame),
            "direct-plan" => handlers.apply_stop_like_frame(&store, &frame, true),
            "direct-stop" => handlers.apply_stop_like_frame(&store, &frame, false),
            _ => unreachable!(),
        };
        assert_eq!(
            first.get(),
            expected_first,
            "private native independent first subscriber calls {method}/{fail_at}"
        );
        assert_eq!(
            later.get(),
            expected_later,
            "private native independent later subscriber calls {method}/{fail_at}"
        );
        assert_eq!(
            store.market_reads.get(),
            if kind == "position" { 1 } else { 2 },
            "private native per-row market getter calls"
        );
        let expected_writes = if kind == "order" {
            let writes = [
                WriteCall::Orders(vec!["order-closed".into(), "order-canceled".into()]),
                WriteCall::Status("order-closed".into(), OrderStatus::Closed),
                WriteCall::Status("order-canceled".into(), OrderStatus::Canceled),
            ];
            writes[..expected_first].to_vec()
        } else if kind == "position" {
            vec![WriteCall::Positions(vec!["BTCUSDT:long".into()])]
        } else {
            vec![WriteCall::Orders(vec![format!("{kind}-terminal")])]
        };
        assert_eq!(
            *store.writes.borrow(),
            expected_writes,
            "private native exact Store write sequence {method}/{fail_at}"
        );
        first_total += first.get();
        later_total += later.get();
        if fail_at > 0 {
            let error = result.expect_err("private native propagates Store subscriber failure");
            assert_eq!(
                error.message, "private subscriber origin",
                "private native final message"
            );
            assert_eq!(
                error.store_subscriber_ids,
                [0],
                "private native original subscriber id"
            );
            let cause = error
                .source()
                .unwrap()
                .downcast_ref::<HandlerOrigin>()
                .expect("private native source is the original subscriber cause");
            assert_eq!(cause.0, 313, "private native opaque cause code");
            assert!(
                std::ptr::eq(cause, origin.as_ref()),
                "private native original opaque cause identity"
            );
            assert!(
                error.code.is_none(),
                "private native opaque code remains under retained cause, not projected top-level"
            );
        } else {
            result.unwrap();
        }
        if kind == "order" {
            let orders = store.orders();
            assert_eq!(orders.len(), 2, "private native committed order count");
            assert_eq!(orders[0].id, "order-closed");
            assert_eq!(orders[1].id, "order-canceled");
            assert_eq!(orders[0].status, OrderStatus::Closed);
            assert_eq!(orders[1].status, OrderStatus::Canceled);
            assert_eq!(orders[0].amount.to_bits(), 1f64.to_bits());
            assert_eq!(orders[1].amount.to_bits(), 2f64.to_bits());
            let hooks = rejected.borrow();
            assert_eq!(
                hooks.len(),
                usize::from(fail_at == 0),
                "private native rejection cutoff"
            );
            if fail_at == 0 {
                assert_eq!(hooks[0].0, "order-canceled");
                assert_eq!(hooks[0].1.to_bits(), 510f64.to_bits());
            }
        } else if kind == "position" {
            let positions = store.positions();
            assert_eq!(
                positions.len(),
                1,
                "private native committed position count"
            );
            assert_eq!(
                positions[0].position_mode,
                Some(PositionMode::Hedged),
                "private native default hedged position mode"
            );
            assert_eq!(positions[0].side, PositionSide::Long);
            assert_eq!(positions[0].id, "BTCUSDT:long");
            assert_eq!(positions[0].contracts.to_bits(), 3f64.to_bits());
            assert_eq!(positions[0].notional.to_bits(), 60f64.to_bits());
        } else {
            assert_eq!(
                *seen.borrow(),
                [format!("{kind}-open"), format!("{kind}-terminal")],
                "private native seen order"
            );
            assert_eq!(
                *shields.borrow(),
                [format!("{kind}-open")],
                "private native shield calls"
            );
            assert_eq!(
                *trace.borrow(),
                [
                    format!("seen:{kind}-open"),
                    format!("seen:{kind}-terminal"),
                    format!("shield:{kind}-open"),
                    "store".into()
                ],
                "private native full stop effect order"
            );
            let orders = store.orders();
            assert_eq!(orders.len(), 1);
            assert_eq!(orders[0].id, format!("{kind}-terminal"));
            assert_eq!(orders[0].status, OrderStatus::Canceled);
            assert_eq!(orders[0].amount.to_bits(), 1f64.to_bits());
            let info = orders[0]
                .info
                .as_ref()
                .expect("private native stop/plan info");
            let (wanted, unwanted) = if kind == "plan" {
                ("__mexcPlanOrder", "__mexcStopOrder")
            } else {
                ("__mexcStopOrder", "__mexcPlanOrder")
            };
            assert_eq!(
                info.get(wanted).and_then(|value| value.as_bool()),
                Some(true),
                "private native correct plan/stop info classification"
            );
            assert!(
                info.get(unwanted).is_none(),
                "private native opposite plan/stop marker absent"
            );
        }
    }
    assert_eq!(
        first_total, 17,
        "private native independent first subscriber floor"
    );
    assert_eq!(
        later_total, 9,
        "private native independent later subscriber floor"
    );
    // Run/inspect maintained-source trace only after all native assertions.
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private_store_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "actual private Store source: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(rows.len(), 12, "private Source independent root floor");
    for (index, (method, fail_at, first, later)) in ROOTS.into_iter().enumerate() {
        let row = &rows[index];
        assert_eq!(row["method"], method);
        assert_eq!(row["failAt"], fail_at);
        assert_eq!(
            row["marketReads"],
            if method == "position" { 1 } else { 2 },
            "private Source independent market getter calls"
        );
        assert_eq!(row["first"], first);
        assert_eq!(row["later"], later);
        if fail_at > 0 {
            assert_eq!(row["error"]["code"], 313);
            assert_eq!(row["error"]["message"], "private subscriber origin");
        } else {
            assert!(row["error"].is_null());
        }
    }
}
