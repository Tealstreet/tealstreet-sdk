#[path = "support/private_store.rs"]
mod observed_store;
#[path = "support/private_unicode.rs"]
mod unicode_boundary;
use observed_store::ObservedStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_venue_mexc::{utils::normalize_market, ws_private::MexcPrivateHandlers};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn set_market(store: &dyn Store, size: f64) {
    let market = normalize_market(
        &serde_json::Value::Object(
            (json!({"symbol":"BTC_USDT","contractSize":size,"maxLeverage":20})
                .as_object()
                .unwrap())
            .clone(),
        )
        .into(),
    )
    .unwrap();
    Store::update(
        store,
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}
fn fixture(kind: &str, id: &str, side: u32, time: Value) -> Value {
    let mut row = json!({"symbol":"BTC_USDT","createTime":time,"probe":{"marker":7}});
    let extra = match kind {
        "order" => json!({"orderId":id,"side":1,"state":1,"vol":2,"dealVol":0,"price":10}),
        "position" => json!({"positionType":side,"holdVol":2,"holdAvgPrice":20}),
        "plan" => json!({"id":id,"side":1,"state":1,"vol":2,"triggerPrice":21}),
        "stop" => {
            json!({"stopPlanOrderId":id,"positionType":side,"state":1,"vol":2,"stopLossPrice":5})
        }
        _ => unreachable!(),
    };
    row.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    row
}

#[test]
fn private_handlers_reread_live_markets_and_name_owned_info_snapshot_cost() {
    let (mut reads_total, mut clock_total) = (0, 0);
    for kind in ["order", "position", "plan", "stop"] {
        for scenario in ["live-market", "info-alias"] {
            let live = scenario == "live-market";
            let store = Rc::new(ObservedStore::new());
            set_market(store.as_ref(), 0.5);
            let clock_calls = Rc::new(Cell::new(0));
            let calls = clock_calls.clone();
            let producer = store.clone();
            let clock = Rc::new(move |v: f64| {
                let n = calls.get() + 1;
                calls.set(n);
                if n == 1 {
                    set_market(producer.as_ref(), 3.0);
                }
                v
            });
            let seen = Rc::new(RefCell::new(Vec::<String>::new()));
            let shields = Rc::new(RefCell::new(Vec::<String>::new()));
            let effects = seen.clone();
            let mark = Rc::new(move |id: String| effects.borrow_mut().push(id));
            let effects = shields.clone();
            let shield = Rc::new(move |id: &str| {
                effects.borrow_mut().push(id.into());
                false
            });
            let handlers = MexcPrivateHandlers::new(
                clock,
                None,
                Rc::new(|_, _| -> Result<(), ExchangeError> { panic!("no rejected row") }),
                mark,
                shield,
                Rc::new(|_| -> Result<(), ExchangeError> { panic!("no fill row") }),
            );
            let mut frame = if live {
                json!({"data":[fixture(kind,"live-a",1,json!("2026-01-02T03:04:05")),fixture(kind,"live-b",2,json!("2026-01-02T03:04:06"))]})
            } else {
                json!({"data":[fixture(kind,"alias",1,json!(1000))]})
            };
            match kind {
                "order" => handlers.handle_order(store.as_ref(), &frame),
                "position" => handlers.handle_position(store.as_ref(), &frame),
                "plan" => handlers.handle_plan_order(store.as_ref(), &frame),
                "stop" => handlers.handle_stop_order(store.as_ref(), &frame),
                _ => unreachable!(),
            }
            .unwrap();
            assert_eq!(
                store.market_reads.get(),
                if live { 2 } else { 1 },
                "private native reference independent market getter calls"
            );
            assert_eq!(
                clock_calls.get(),
                if live { 2 } else { 0 },
                "private native real local-date clock calls"
            );
            let want: Vec<f64> = if live { vec![1., 6.] } else { vec![1.] };
            if kind == "position" {
                let positions = store.positions();
                assert_eq!(
                    positions.len(),
                    if live { 2 } else { 1 },
                    "private native reference independent position count"
                );
                for (p, n) in positions.iter().zip(&want) {
                    assert_eq!(
                        p.contracts.to_bits(),
                        n.to_bits(),
                        "private native each position uses newly returned market"
                    );
                }
                assert_eq!(
                    positions[0]
                        .info
                        .as_ref()
                        .unwrap()
                        .get("probe")
                        .unwrap()
                        .get("marker")
                        .unwrap()
                        .as_f64(),
                    Some(7.0),
                    "private native initial position info marker"
                );
            } else {
                let orders = store.orders();
                assert_eq!(
                    orders.len(),
                    if live { 2 } else { 1 },
                    "private native reference independent order count"
                );
                for (o, n) in orders.iter().zip(&want) {
                    assert_eq!(
                        o.amount.to_bits(),
                        n.to_bits(),
                        "private native each order uses newly returned market"
                    );
                }
                assert_eq!(
                    orders[0]
                        .info
                        .as_ref()
                        .unwrap()
                        .get("probe")
                        .unwrap()
                        .get("marker")
                        .unwrap()
                        .as_f64(),
                    Some(7.0),
                    "private native initial order info marker"
                );
            }
            if live {
                assert_eq!(
                    store.markets()[0].contract_size.unwrap().to_bits(),
                    3f64.to_bits(),
                    "private native real producer replaced market"
                );
            } else {
                frame["data"][0]["probe"]["marker"] = json!(42);
                assert_eq!(
                    frame["data"][0]["probe"]["marker"].as_u64(),
                    Some(42),
                    "private native caller changed nested producer"
                );
                let marker = if kind == "position" {
                    store.positions()[0]
                        .info
                        .as_ref()
                        .unwrap()
                        .get("probe")
                        .unwrap()
                        .get("marker")
                        .unwrap()
                        .clone()
                } else {
                    store.orders()[0]
                        .info
                        .as_ref()
                        .unwrap()
                        .get("probe")
                        .unwrap()
                        .get("marker")
                        .unwrap()
                        .clone()
                };
                assert_eq!(
                    marker.as_f64(),
                    Some(7.0),
                    "private native owned info snapshot keeps marker7"
                );
            }
            let expected = if matches!(kind, "plan" | "stop") {
                if live {
                    vec!["live-a".to_owned(), "live-b".to_owned()]
                } else {
                    vec!["alias".to_owned()]
                }
            } else {
                vec![]
            };
            assert_eq!(
                *seen.borrow(),
                expected,
                "private native reference independent seen calls"
            );
            assert_eq!(
                *shields.borrow(),
                expected,
                "private native reference independent shield calls"
            );
            reads_total += store.market_reads.get();
            clock_total += clock_calls.get();
        }
    }
    assert_eq!(
        reads_total, 12,
        "private native independent reference getter floor"
    );
    assert_eq!(
        clock_total, 8,
        "private native independent local-date clock floor"
    );
    unicode_boundary::native_unicode_boundaries();
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private_reference_oracle.mjs"
        ))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual reference Source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.len(),
        8,
        "private Source independent reference root floor"
    );
    for (i, kind) in ["order", "position", "plan", "stop"]
        .into_iter()
        .enumerate()
    {
        for (j, scenario) in ["live-market", "info-alias"].into_iter().enumerate() {
            let row = &rows[i * 2 + j];
            let live = scenario == "live-market";
            assert_eq!(row["kind"], kind);
            assert_eq!(row["scenario"], scenario);
            assert_eq!(row["reads"], if live { 2 } else { 1 });
            assert_eq!(row["clockCalls"], if live { 2 } else { 0 });
            assert_eq!(
                row["marker"],
                if live { 7 } else { 42 },
                "private Source live nested alias differs from native owned snapshot"
            );
        }
    }
    unicode_boundary::assert_source_unicode_floor();
}
