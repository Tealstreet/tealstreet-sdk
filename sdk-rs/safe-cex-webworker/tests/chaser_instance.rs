#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use runtime::Runtime;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::ExchangeError,
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Order, Position};
use safe_cex_webworker::{chaser::*, task_types::*};
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
fn encoded(n: f64) -> Value {
    if n.is_nan() {
        json!("NaN")
    } else if n == f64::INFINITY {
        json!("Infinity")
    } else if n == f64::NEG_INFINITY {
        json!("-Infinity")
    } else if n == 0.0 && n.is_sign_negative() {
        json!("-0")
    } else {
        json!(n)
    }
}
fn numeric(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(numeric).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, numeric(v))).collect()),
        v => v,
    }
}
fn snapshot(i: &ChaserOrderInstance) -> Value {
    let state = i.state();
    let state = state.borrow();
    json!({"id":i.id(),"state":{"attempts":encoded(state.attempts),"isStopped":state.is_stopped,"stopReason":state.stop_reason,"currentOrdersIds":state.current_orders_ids},"remaining":encoded(i.remaining_size()),"filled":encoded(i.filled_size()),"partial":encoded(i.last_partial_fill_at()),"target":i.target_position_contracts().map(encoded),"ids":i.current_order_ids(),"ever":i.ever_tracked_order_ids().borrow().iter().collect::<Vec<_>>(),"stopped":i.is_stopped(),"min":encoded(i.min_tick_distance()),"pending":i.pending_orders_update(),"fresh":i.fresh_order(),"size":encoded(i.options().borrow().size),"identity":true})
}
#[test]
fn chaser_instance_matches_live_source_transitions_prototype_keys_and_disposal() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_instance_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let scenarios: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(scenarios.len(), 648);
    for (index, s) in scenarios.iter().enumerate() {
        let runtime = Rc::new(Runtime::default());
        let store = Rc::new(DefaultStore::new());
        if let Some(contracts) = s.get("contracts") {
            let side = if s["side"] == "buy" { "long" } else { "short" };
            let mut p:Position=serde_json::from_value(json!({"id":"p","symbol":"BTC","side":side,"entryPrice":100,"notional":0,"leverage":1,"unrealizedPnl":0,"contracts":0,"liquidationPrice":0})).unwrap();
            p.contracts = number(contracts);
            let mut duplicate = p.clone();
            duplicate.id = "duplicate".into();
            duplicate.contracts = 999.0;
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    positions: Some(vec![p, duplicate]),
                    ..Default::default()
                },
                true,
            )
            .unwrap();
        }
        let venue = Rc::new(Venue {
            base: BaseExchange::new(
                ExchangeName::Bybit,
                serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
                store,
                runtime.clone(),
                Default::default(),
            ),
            capabilities: Default::default(),
        });
        let options=Rc::new(RefCell::new(serde_json::from_value::<ChaserOrderOptions>(json!({"symbol":"BTC","size":10,"side":s["side"],"maxAttempts":1,"delay":100,"reduceOnly":s["reduceOnly"]})).unwrap()));
        let i = Rc::new(ChaserOrderInstance::new(venue, options.clone()).unwrap());
        let held_state = i.state();
        let held_ever = i.ever_tracked_order_ids();
        let log = Rc::new(RefCell::new(Vec::<String>::new()));
        let mut expected = vec![snapshot(&i)];
        for (ordinal, a) in s["actions"].as_array().unwrap().iter().enumerate() {
            runtime.advance(((ordinal + 1) * 10) as u64);
            let mut result = Value::Null;
            match a["op"].as_str().unwrap() {
                "attempt" => i.increment_attempts(),
                "complete" => i.mark_complete(a.get("reason").and_then(Value::as_str)),
                "claim" => result = json!(i.claim_complete_emission()),
                "ids" => i.set_current_order_ids(serde_json::from_value(a["ids"].clone()).unwrap()),
                "flags" => {
                    i.set_min_tick_distance(number(&a["min"]));
                    i.set_pending_orders_update(a["pending"].as_bool().unwrap());
                    i.set_fresh_order(a["fresh"].as_bool().unwrap());
                }
                "sub" => {
                    let mut order:Order=serde_json::from_value(json!({"id":a["id"],"status":"open","symbol":"BTC","type":"limit","side":"buy","price":100,"amount":10,"filled":0,"remaining":10,"lastUpdate":0,"reduceOnly":false,"extra":{}})).unwrap();
                    order.filled = number(&a["n"]);
                    order.extra.as_mut().unwrap().filled_in_placement_units =
                        a.get("placement").map(number);
                    i.subtract_filled_size(&order);
                }
                "add" => i.add_fill_amount(a["id"].as_str().unwrap(), number(&a["n"])),
                "set" => i.set_filled_amount(a["id"].as_str().unwrap(), number(&a["n"])),
                "recalc" => i.recalculate_remaining_size(),
                "size" => options.borrow_mut().size = number(&a["n"]),
                "register" => {
                    let action = a.clone();
                    let log = log.clone();
                    let weak = Rc::downgrade(&i);
                    i.register_disposer(Box::new(move || {
                        log.borrow_mut()
                            .push(action["label"].as_str().unwrap().into());
                        let i = weak.upgrade().unwrap();
                        if let Some(label) = action.get("register") {
                            let label = label.as_str().unwrap().to_owned();
                            let log = log.clone();
                            i.register_disposer(Box::new(move || {
                                log.borrow_mut().push(label);
                                Ok(())
                            }));
                        }
                        if action["reenter"] == true {
                            i.dispose();
                        }
                        if action["throws"] == true {
                            Err(ExchangeError::new("disposer failed"))
                        } else {
                            Ok(())
                        }
                    }));
                }
                "dispose" => i.dispose(),
                "termination" => {
                    let mut opts = options.borrow_mut();
                    opts.side = serde_json::from_value(a["side"].clone()).unwrap();
                    opts.terminate = Some(ChaserTerminate {
                        kind: serde_json::from_value(a["kind"].clone()).unwrap(),
                        value: number(&a["value"]),
                        anchor_price: number(&a["anchor"]),
                    });
                    drop(opts);
                    result = json!(
                        a["prices"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| i.should_terminate(number(v)))
                            .collect::<Vec<_>>()
                    );
                }
                op => panic!("unknown action {op}"),
            }
            assert!(Rc::ptr_eq(&held_state, &i.state()));
            assert!(Rc::ptr_eq(&options, &i.options()));
            assert!(Rc::ptr_eq(&held_ever, &i.ever_tracked_order_ids()));
            expected.push(json!({"state":snapshot(&i),"result":result,"log":*log.borrow()}));
        }
        assert_eq!(
            numeric(json!(expected)),
            numeric(s["expected"].clone()),
            "CHASER-IDENTITY native state scenario {index}: {}",
            s["actions"]
        );
    }
}

#[test]
fn owned_instance_id_arrays_have_explicit_live_source_witnesses() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_instance_oracle.mjs"
        ))
        .arg("--representation-witness")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let witness: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        witness,
        json!({"seeded":["seed"],"afterCaller":["a","caller"],"held":["a","caller","state"]})
    );
    let runtime = Rc::new(Runtime::default());
    runtime.advance(0);
    let venue = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime,
            Default::default(),
        ),
        capabilities: Default::default(),
    });
    let options = Rc::new(RefCell::new(
        serde_json::from_value(
            json!({"symbol":"BTC","size":1,"side":"buy","maxAttempts":1,"delay":100}),
        )
        .unwrap(),
    ));
    let i = ChaserOrderInstance::new(venue, options).unwrap();
    assert!(i.current_order_ids().is_empty());
    let mut supplied = vec!["a".to_owned()];
    i.set_current_order_ids(supplied.clone());
    supplied.push("caller".into());
    assert_eq!(i.current_order_ids(), vec!["a"]);
    let held = i.current_order_ids();
    i.state()
        .borrow_mut()
        .current_orders_ids
        .push("state".into());
    assert_eq!(held, vec!["a"]);
    assert_eq!(i.current_order_ids(), vec!["a", "state"]);
}

#[test]
fn active_chaser_postprocessor_matches_live_identity_filter_and_current_state() {
    use safe_cex_webworker::postprocessors::ChaserOrderPostprocessor;
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_postprocessor_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 90);
    fn active(rows: &[safe_cex_types::ActiveChaser]) -> Value {
        Value::Array(
            rows.iter()
                .map(|row| {
                    let mut v = json!({
                        "id": row.id, "symbol": row.symbol, "size": encoded(row.size),
                        "maxAttempts": encoded(row.max_attempts), "attempts": encoded(row.attempts),
                        "isStopped": row.is_stopped, "stopReason": row.stop_reason,
                        "side": match row.side {
                            safe_cex_types::OrderSide::Buy => "buy",
                            safe_cex_types::OrderSide::Sell => "sell",
                        },
                    });
                    if let Some(value) = row.reduce_only {
                        v["reduceOnly"] = json!(value);
                    }
                    if let Some(value) = &row.current_order_ids {
                        v["currentOrderIds"] = json!(value);
                    }
                    if let Some(value) = row.origin {
                        use safe_cex_types::store_model::TaskOrigin;
                        v["origin"] = json!(match value {
                            TaskOrigin::Web => "web",
                            TaskOrigin::Cli => "cli",
                            TaskOrigin::Webhook => "webhook",
                            TaskOrigin::Automation => "automation",
                        });
                    }
                    numeric(v)
                })
                .collect(),
        )
    }
    for (index, case) in cases.iter().enumerate() {
        let runtimes = [Rc::new(Runtime::default()), Rc::new(Runtime::default())];
        let venues: Vec<Rc<dyn Exchange>> = (0..2)
            .map(|index| {
                Rc::new(Venue {
                    base: BaseExchange::new(
                        ExchangeName::Bybit,
                        serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
                        Rc::new(DefaultStore::new()),
                        runtimes[index].clone(),
                        Default::default(),
                    ),
                    capabilities: Default::default(),
                }) as Rc<dyn Exchange>
            })
            .collect();
        let specs = [
            (0, "buy", "BTC", 3.0, vec!["first", "first"]),
            (1, "sell", "OTHER", 5.0, vec!["other"]),
            (0, "sell", "STOPPED", 0.0, vec![]),
            (0, "sell", "ETH", -1.0, vec!["last"]),
        ];
        let instances: Vec<_> = specs
            .into_iter()
            .enumerate()
            .map(|(n, (venue, side, symbol, attempts, ids))| {
                let mut options: ChaserOrderOptions = serde_json::from_value(
                    json!({"symbol":symbol,"side":side,"size":0,"maxAttempts":0,"delay":100}),
                )
                .unwrap();
                options.size = number(&case["size"]);
                options.max_attempts = 17.25;
                options.reduce_only = case.get("reduceOnly").map(|v| v.as_bool().unwrap());
                options.origin = case
                    .get("origin")
                    .map(|v| serde_json::from_value(v.clone()).unwrap());
                *runtimes[venue].id.borrow_mut() = Some(format!("id{n}"));
                let i = Rc::new(
                    ChaserOrderInstance::new(venues[venue].clone(), Rc::new(RefCell::new(options)))
                        .unwrap(),
                );
                i.state().borrow_mut().attempts = attempts;
                i.set_current_order_ids(ids.into_iter().map(str::to_owned).collect());
                if n == 2 {
                    i.mark_complete(Some("stopped"));
                }
                i
            })
            .collect();
        let mut store = safe_cex_store::store::DEFAULT_STORE.clone();
        ChaserOrderPostprocessor.process(&mut store, &venues[0], &instances);
        assert_eq!(
            active(&store.active_chasers),
            numeric(case["first"].clone()),
            "CHASER-PROJECTION native first case {index}"
        );
        instances[0]
            .state()
            .borrow_mut()
            .current_orders_ids
            .push("late".into());
        assert_eq!(case["aliased"], true, "live TS alias witness");
        assert!(
            !store.active_chasers[0]
                .current_order_ids
                .as_ref()
                .unwrap()
                .contains(&"late".into()),
            "CHOSEN: owned store snapshot omits TS array alias"
        );
        instances[0].options().borrow_mut().symbol = "UPDATED".into();
        instances[0].state().borrow_mut().attempts = 9.0;
        ChaserOrderPostprocessor.process(&mut store, &venues[0], &instances);
        assert_eq!(
            active(&store.active_chasers),
            numeric(case["second"].clone()),
            "CHASER-PROJECTION native second case {index}"
        );
        ChaserOrderPostprocessor.process(&mut store, &venues[0], &[]);
        assert_eq!(
            active(&store.active_chasers),
            case["cleared"],
            "empty manager clears store"
        );
    }
}

#[test]
fn constructors_snapshot_the_live_shallow_frozen_default() {
    use safe_cex_exchange::js_runtime::{globals::install_builtins, value::Js};
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_instance_oracle.mjs"
        ))
        .arg("--default-snapshot")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let runtime = Rc::new(Runtime::default());
    let venue: Rc<dyn Exchange> = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime,
            Default::default(),
        ),
        capabilities: Default::default(),
    });
    let options = Rc::new(RefCell::new(
        serde_json::from_value(
            json!({"symbol":"BTC","size":1,"side":"buy","maxAttempts":1,"delay":100}),
        )
        .unwrap(),
    ));
    let template = default_chaser_state().unwrap();
    assert!(
        template.same_reference(&default_chaser_state().unwrap()),
        "DEFAULT shared module identity"
    );
    let ids = template.get("currentOrdersIds").unwrap();
    let before = ChaserOrderInstance::new(venue.clone(), options.clone()).unwrap();
    ids.call_method("push", vec![Js::str("seed")]).unwrap();
    let first = ChaserOrderInstance::new(venue.clone(), options.clone()).unwrap();
    ids.call_method("push", vec![Js::str("later")]).unwrap();
    let second = ChaserOrderInstance::new(venue, options).unwrap();
    first
        .state()
        .borrow_mut()
        .current_orders_ids
        .push("local".into());
    let globals = Js::object(vec![]);
    install_builtins::<()>(&globals, std::rc::Weak::new());
    let object = globals.get("Object").unwrap();
    let frozen = object
        .call_method("isFrozen", vec![template.clone()])
        .unwrap()
        .truthy();
    let nested_frozen = object
        .call_method("isFrozen", vec![ids.clone()])
        .unwrap()
        .truthy();
    let replace = template.set("currentOrdersIds", Js::array(vec![])).is_ok();
    let extend = template.set("extra", Js::Bool(true)).is_ok();
    let actual = json!({"before":before.current_order_ids(),"first":first.current_order_ids(),"second":second.current_order_ids(),
        "template": ids.array_items().unwrap().unwrap().into_iter().map(|value| {
            let Js::String(value) = value.unwrap() else { panic!("DEFAULT string IDs") };
            String::from_utf16(&value).unwrap()
        }).collect::<Vec<_>>(),"frozen":frozen,"nestedFrozen":nested_frozen,"replace":replace,"extend":extend});
    assert_eq!(
        ids.get("length").unwrap().to_number().unwrap().to_bits(),
        2.0_f64.to_bits()
    );
    let independent = std::thread::spawn(|| {
        default_chaser_state()
            .unwrap()
            .get("currentOrdersIds")
            .unwrap()
            .get("length")
            .unwrap()
            .to_number()
            .unwrap()
            .to_bits()
    })
    .join()
    .unwrap();
    assert_eq!(
        independent,
        0.0_f64.to_bits(),
        "DEFAULT independent native module realm"
    );
    ids.set("length", Js::Number(0.0)).unwrap();
    assert_eq!(
        actual, expected,
        "DEFAULT actual constructor must clone the live template"
    );
}
