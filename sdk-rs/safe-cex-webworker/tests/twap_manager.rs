use safe_cex_webworker::{
    task_types::{TwapOptions, TwapState, TwapStatus},
    twap_manager::{ManagedTwap, TwapManager, twap_manager},
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

const LABELS: [&str; 15] = [
    "empty",
    "add-A",
    "add-completed-B",
    "add-paused-C",
    "replace-A",
    "remove-B-and-missing",
    "alias-and-state-replace",
    "live-ID-change",
    "numeric-order",
    "own-constructor",
    "prototype-write",
    "remove-middle-C",
    "reinsert-C",
    "empty-ID",
    "unicode-ID",
];
const MAP_COUNTS: [usize; 15] = [0, 1, 2, 3, 3, 2, 2, 2, 3, 4, 5, 4, 5, 6, 7];
const OUTPUT_COUNTS: [usize; 15] = [0, 1, 1, 2, 2, 2, 2, 2, 3, 4, 4, 3, 4, 5, 6];
const QUERIES: [&str; 10] = [
    "A",
    "B",
    "C",
    "7",
    "42",
    "constructor",
    "__proto__",
    "missing",
    "",
    "é🪙",
];
fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}
fn numeric(value: Value) -> Value {
    match value {
        // Wire shape compares numeric values; native signed-zero bits are asserted separately.
        Value::Number(n) => {
            let n = n.as_f64().unwrap();
            json!(if n == 0.0 { 0.0 } else { n })
        }
        Value::Array(values) => Value::Array(values.into_iter().map(numeric).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, numeric(value)))
                .collect(),
        ),
        value => value,
    }
}
fn instance(id: &str, size: f64, status: TwapStatus) -> Rc<RefCell<ManagedTwap>> {
    let market = json!({"id":"market-id","symbol":"MARKET","base":"BASE","quote":"QUOTE","active":true,
        "precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}});
    let options: TwapOptions = serde_json::from_value(
        json!({"accountName":"options-account","activeMarket":market,
        "size":size,"duration":17,"ordersCount":3,"side":"buy","chaserDelay":100,"reduceOnly":false,
        "randomnessPercentage":5,"useLimitOrders":false,"useChaserOrders":false}),
    )
    .unwrap();
    let mut state_market = market.clone();
    state_market["id"] = json!("state-market-id");
    let mut state: TwapState = serde_json::from_value(json!({"accountName":"state-account","activeMarket":state_market,
        "size":size+1.0,"ordersSizes":[size+2.0,size+3.0],"side":"sell","status":"idle","ordersCount":4,
        "ordersExecuted":0,"nextOrderAt":size+9.0})).unwrap();
    state.status = status;
    Rc::new(RefCell::new(ManagedTwap {
        id: id.into(),
        options: Rc::new(RefCell::new(options)),
        state: Rc::new(RefCell::new(state)),
    }))
}
fn assert_step(manager: &TwapManager, rows: &[Value], index: usize) {
    let source = &rows[index];
    assert_eq!(
        source["label"], LABELS[index],
        "TWAP-MANAGER source trace label"
    );
    assert_eq!(
        source["mapKeys"].as_array().unwrap().len(),
        MAP_COUNTS[index],
        "TWAP-MANAGER source map count {index}"
    );
    assert_eq!(
        source["entries"].as_array().unwrap().len(),
        OUTPUT_COUNTS[index],
        "TWAP-MANAGER source output count {index}"
    );
    assert_eq!(
        source["queries"].as_array().unwrap().len(),
        10,
        "TWAP-MANAGER source query count"
    );
    let map_keys = manager
        .running_twaps
        .borrow()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        map_keys.len(),
        MAP_COUNTS[index],
        "TWAP-MANAGER native map count {index}"
    );
    let out = manager.serialize_state();
    assert_eq!(
        out.len(),
        OUTPUT_COUNTS[index],
        "TWAP-MANAGER native output count {index}"
    );
    let entries: Vec<_> = out.iter().map(|(key, entry)| {
        let o=entry.options.borrow();let s=entry.state.borrow();
        let status=match s.status {TwapStatus::Idle=>"idle",TwapStatus::Running=>"running",TwapStatus::Paused=>"paused",TwapStatus::Completed=>"completed"};
        assert_eq!(s.orders_sizes.len(), 2, "TWAP-MANAGER native size count {index}/{key}");
        // Read decoded native fields directly, without either task record's serializer.
        json!({"key":key,"account":o.account_name,"market":o.active_market.id,"size":bits(o.size),"duration":bits(o.duration),
            "stateAccount":s.account_name,"stateMarket":s.active_market.id,"stateSize":bits(s.size),"status":status,
            "executed":bits(s.orders_executed),"next":bits(s.next_order_at),"sizes":s.orders_sizes.iter().copied().map(bits).collect::<Vec<_>>()})
    }).collect();
    let queries: Vec<_> = QUERIES
        .iter()
        .map(|id| {
            let found = manager
                .get_by_id(id)
                .map(|record| record.borrow().id.clone());
            json!([id, manager.has_id(id), found])
        })
        .collect();
    assert_eq!(
        json!({"label":LABELS[index],"mapKeys":map_keys,"queries":queries,"entries":entries}),
        *source,
        "TWAP-MANAGER native trace {index}"
    );
}

#[test]
fn manager_membership_order_and_live_fields_match_executed_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/twap_manager_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!source["architecture"].as_str().unwrap().is_empty());
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 15, "TWAP-MANAGER source trace total");
    let manager = TwapManager::default();
    let mut comparisons = 0;
    macro_rules! check {
        ($index:expr) => {
            assert_step(&manager, rows, $index);
            comparisons += 1;
        };
    }
    check!(0);
    let a = instance("A", 11.0, TwapStatus::Idle);
    let b = instance("B", 22.0, TwapStatus::Completed);
    let c = instance("C", 33.0, TwapStatus::Paused);
    assert_eq!(manager.add(a), "A", "TWAP-MANAGER native add return");
    check!(1);
    assert_eq!(manager.add(b), "B");
    check!(2);
    assert_eq!(manager.add(c), "C");
    check!(3);
    let a = instance("A", 44.0, TwapStatus::Running);
    manager.add(a.clone());
    check!(4);
    assert!(
        Rc::ptr_eq(&manager.get_by_id("A").unwrap(), &a),
        "TWAP-MANAGER native get identity"
    );
    manager.remove("B");
    manager.remove("missing");
    check!(5);
    let held = manager.serialize_state();
    a.borrow().options.borrow_mut().size = 99.0;
    a.borrow().state.borrow_mut().orders_executed = 7.0;
    a.borrow().state.borrow_mut().next_order_at = -0.0;
    assert_eq!(
        held["A"].options.borrow().size.to_bits(),
        99.0_f64.to_bits(),
        "TWAP-MANAGER native option alias value"
    );
    assert_eq!(
        held["A"].state.borrow().orders_executed.to_bits(),
        7.0_f64.to_bits(),
        "TWAP-MANAGER native state alias value"
    );
    assert_eq!(
        held["A"].state.borrow().next_order_at.to_bits(),
        (-0.0_f64).to_bits(),
        "TWAP-MANAGER native negative-zero alias"
    );
    let mut next = a.borrow().state.borrow().clone();
    next.orders_executed = 9.0;
    a.borrow_mut().state = Rc::new(RefCell::new(next));
    assert_eq!(
        held["A"].state.borrow().orders_executed,
        7.0,
        "TWAP-MANAGER native old state retained"
    );
    assert_eq!(
        a.borrow().state.borrow().orders_executed,
        9.0,
        "TWAP-MANAGER native replaced state"
    );
    check!(6);
    a.borrow_mut().id = "42".into();
    check!(7);
    manager.add(instance("7", 55.0, TwapStatus::Idle));
    check!(8);
    manager.add(instance("constructor", 66.0, TwapStatus::Running));
    check!(9);
    manager.add(instance("__proto__", 77.0, TwapStatus::Idle));
    check!(10);
    assert!(
        !manager.serialize_state().contains_key("__proto__"),
        "TWAP-MANAGER native prototype omitted"
    );
    manager.remove("C");
    check!(11);
    manager.add(instance("C", 88.0, TwapStatus::Idle));
    check!(12);
    manager.add(instance("", 90.0, TwapStatus::Idle));
    check!(13);
    manager.add(instance("é🪙", 101.0, TwapStatus::Paused));
    check!(14);
    assert_eq!(comparisons, 15, "TWAP-MANAGER native trace comparisons");
    let first = twap_manager();
    let second = twap_manager();
    assert!(
        Rc::ptr_eq(&first, &second),
        "TWAP-MANAGER native singleton identity"
    );
    first.running_twaps.borrow_mut().clear();
    first.add(a);
    assert!(
        second.has_id("42"),
        "TWAP-MANAGER native singleton shared state"
    );
    first.running_twaps.borrow_mut().clear();
    assert_eq!(source["getIdentity"], true);
    assert_eq!(source["singletonIdentity"], true);
    assert_eq!(source["alias"]["optionSize"], bits(99.0));
    assert_eq!(source["alias"]["executed"], bits(7.0));
    assert_eq!(source["alias"]["next"], bits(-0.0));
    assert_eq!(source["alias"]["heldAfterReplace"], bits(7.0));
    assert_eq!(source["alias"]["currentAfterReplace"], bits(9.0));
    assert_eq!(source["prototype"]["own"], false);
    assert_eq!(
        source["prototype"]["inheritedOptionSize"],
        bits(77.0),
        "TWAP-MANAGER source prototype boundary"
    );
    assert_eq!(source["surrogate"]["has"], true);
    assert_eq!(source["surrogate"]["getSame"], true);
    assert_eq!(source["surrogate"]["removed"], true);
    assert_eq!(source["surrogate"]["afterRemoveMissing"], true);
    assert!(
        serde_json::from_str::<String>(source["surrogate"]["idJson"].as_str().unwrap()).is_err(),
        "TWAP-MANAGER shared UTF-8 ID refusal"
    );
    assert!(
        serde_json::from_str::<Value>(source["surrogate"]["serializedJson"].as_str().unwrap())
            .is_err()
    );
    // Session-storage wire shape is checked only after all decoded native observations above.
    assert_eq!(
        numeric(serde_json::to_value(manager.serialize_state()).unwrap()),
        numeric(serde_json::from_str(source["serializedJson"].as_str().unwrap()).unwrap()),
        "TWAP-MANAGER final wire state"
    );
}
