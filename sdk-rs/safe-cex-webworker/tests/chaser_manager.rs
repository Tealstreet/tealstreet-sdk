#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use runtime::Runtime;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Position};
use safe_cex_webworker::{
    chaser::ChaserOrderInstance,
    chaser_manager::{bybit_demo_position_fill, has_reached_target_position},
    task_types::ChaserOrderOptions,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        _ => v.as_f64().unwrap(),
    }
}
fn positions(side: &str, contracts: Option<&Value>) -> Vec<Position> {
    let target = if side == "buy" { "long" } else { "short" };
    let position = |symbol: &str, side: &str, amount: f64| {
        let mut p: Position = serde_json::from_value(json!({"id":"p","symbol":symbol,"side":side,"entryPrice":100,"notional":0,"leverage":1,"unrealizedPnl":0,"contracts":0,"liquidationPrice":0})).unwrap();
        p.contracts = amount;
        p
    };
    let mut values = vec![
        position("OTHER", target, 999.0),
        position(
            "BTC",
            if target == "long" { "short" } else { "long" },
            999.0,
        ),
    ];
    if let Some(contracts) = contracts {
        values.push(position("BTC", target, number(contracts)));
        values.push(position("BTC", target, 999.0));
    }
    for (index, position) in values.iter_mut().enumerate() {
        position.id = format!("p{index}");
    }
    values
}
#[test]
fn position_completion_and_fill_match_live_manager_transitions() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_manager_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 192);
    let mut transitions = 0;
    for (row_index, row) in rows.iter().enumerate() {
        let store = Rc::new(DefaultStore::new());
        Store::update(
            store.as_ref(),
            StoreUpdate {
                positions: Some(positions(row["side"].as_str().unwrap(), row.get("initial"))),
                ..Default::default()
            },
            true,
        )
        .unwrap();
        let runtime = Rc::new(Runtime::default());
        runtime.advance(0);
        let venue = Rc::new(Venue {
            base: BaseExchange::new(
                serde_json::from_value::<ExchangeName>(row["name"].clone()).unwrap(),
                serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
                store.clone(),
                runtime,
                Default::default(),
            ),
            capabilities: Default::default(),
        });
        let mut options:ChaserOrderOptions=serde_json::from_value(json!({"symbol":"BTC","size":0,"side":row["side"],"maxAttempts":1,"delay":100,"reduceOnly":row["reduceOnly"]})).unwrap();
        options.size = number(&row["size"]);
        let options = Rc::new(RefCell::new(options));
        let instance = ChaserOrderInstance::new(venue, options.clone());
        for (step_index, step) in row["steps"].as_array().unwrap().iter().enumerate() {
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    positions: Some(positions(
                        row["side"].as_str().unwrap(),
                        step.get("contracts"),
                    )),
                    ..Default::default()
                },
                true,
            )
            .unwrap();
            options.borrow_mut().size = number(&step["currentSize"]);
            assert_eq!(
                has_reached_target_position(&instance),
                step["reached"].as_bool().unwrap(),
                "TARGET-POSITION row {row_index} step {step_index}"
            );
            let actual = bybit_demo_position_fill(&instance);
            let expected = number(&step["filled"]);
            assert!(
                actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
                "POSITION-FILL row {row_index} step {step_index}: {actual:?} != {expected:?}"
            );
            transitions += 1;
        }
    }
    assert_eq!(transitions, 7488);
}
