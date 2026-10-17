#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::{
    js_from_str, js_from_value, js_from_value_ref, js_value_as_f64,
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::js_coercion::JsCoercionError;
use safe_cex_types::{MarginType, Market, Position, PositionMode, PositionSide};
use safe_cex_venue_blofin::positions::*;
use serde_json::{Value, json};
use std::{process::Command, rc::Rc};
fn bits(value: f64) -> Value {
    json!({"number":if value.is_nan(){"nan".into()}else{format!("{:016x}",value.to_bits())}})
}
fn info(value: Value) -> Value {
    match value {
        Value::Number(_) => bits(js_value_as_f64(&value).unwrap()),
        Value::Array(v) => Value::Array(v.into_iter().map(info).collect()),
        Value::Object(v) => Value::Object(v.into_iter().map(|(k, v)| (k, info(v))).collect()),
        other => other,
    }
}
fn native(p: Position) -> Value {
    assert!(
        p.realized_pnl.is_none() && p.margin_size.is_none() && p.last_update.is_none(),
        "positions_absent_fields"
    );
    let extra = p.extra.expect("positions_extra");
    assert!(
        extra.unrealized_pnl_native.is_none(),
        "positions_extra_absent"
    );
    json!({"id":p.id,"symbol":p.symbol,"side":match p.side.value().expect("present source enum in fixture"){PositionSide::Long=>"long",PositionSide::Short=>"short"},"entryPrice":bits(p.entry_price),"notional":bits(p.notional),"leverage":bits(p.leverage),"unrealizedPnl":bits(p.unrealized_pnl),"contracts":bits(p.contracts),"liquidationPrice":bits(p.liquidation_price),"marginType":match p.margin_type.unwrap(){MarginType::Isolated=>"isolated",MarginType::Cross=>"cross"},"positionMode":match p.position_mode.unwrap(){PositionMode::OneWay=>"oneway",PositionMode::Hedged=>"hedged"},"extra":{"contractsNative":bits(extra.contracts_native.unwrap())},"info":info(p.info.expect("positions_info"))})
}
#[test]
fn live_positions_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/positions_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual_typescript_positions: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pin = cardinality::Census::new("positions", &corpus);
    let header = &corpus["header"];
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(header["architecture"], arch, "capture_architecture");
    assert!(
        header["nodeMajor"].as_u64().is_some_and(|n| n > 0),
        "capture_node_major"
    );
    eprintln!(
        "positions capture header: {header}; cases={}",
        corpus["cases"].as_array().unwrap().len()
    );
    let mut refused = 0;
    for i in 0..pin.count("/cases") {
        let case = &corpus["cases"][i];
        for path in 0..3 {
            let raw = case["raw"].as_str().unwrap();
            let input: PositionsInput = match path {
                0 => js_from_str(raw),
                1 => js_from_value(serde_json::from_str(raw).unwrap()),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()),
            }
            .expect("positions_ingress_retained");
            let initial = &case["initial"];
            let markets: Vec<Market> = js_from_value(initial["markets"].clone()).unwrap();
            let positions: Vec<Position> = js_from_value(initial["positions"].clone()).unwrap();
            let fetched = initial["fetchedLeverage"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| {
                    assert!(
                        value.as_object().unwrap().keys().all(|k| [
                            "buyLeverage",
                            "sellLeverage",
                            "isFetching"
                        ]
                        .contains(&k.as_str())),
                        "positions_fixture_fetched_shape"
                    );
                    pin.hit("fetched-fixture-entries");
                    (
                        key.clone(),
                        safe_cex_types::FetchedLeverage {
                            buy_leverage: js_from_value(value["buyLeverage"].clone()).unwrap(),
                            sell_leverage: js_from_value(value["sellLeverage"].clone()).unwrap(),
                            is_fetching: value.get("isFetching").map(|v| v.as_bool().unwrap()),
                            margin_type: None,
                            position_mode: None,
                        },
                    )
                })
                .collect();
            let store = DefaultStore::new();
            Store::update(
                &store,
                StoreUpdate {
                    markets: Some(markets),
                    positions: Some(positions),
                    fetched_leverage: Some(fetched),
                    ..Default::default()
                },
                false,
            )
            .expect("positions_fixture_update");
            let mut logs = Vec::new();
            let sentinel = Rc::new("host:position-warning".to_owned());
            let mut result = map_positions(&input, &store, |message| {
                logs.push(message.to_owned());
                if case["hostMutates"] == true {
                    let mut markets: Vec<Market> =
                        js_from_value(initial["markets"].clone()).unwrap();
                    markets[0].contract_size = Some(0.5);
                    Store::update(
                        &store,
                        StoreUpdate {
                            markets: Some(markets),
                            ..Default::default()
                        },
                        false,
                    )
                    .expect("positions_reentrant_update");
                }
                if case["hostThrows"] == true {
                    Err(sentinel.clone())
                } else {
                    Ok(())
                }
            });
            assert_eq!(
                json!(logs),
                case["logs"],
                "positions_warning_trace case={i}"
            );
            if case["hostThrows"] == true && !logs.is_empty() {
                assert!(
                    matches!(&result,Err(PositionMapError::Emitter(e)) if Rc::ptr_eq(e,&sentinel)),
                    "positions_emitter_identity"
                );
            }
            if case["invalid"] == true {
                let err = result.expect_err("positions_typed_boundary");
                assert!(
                    matches!(
                        err,
                        PositionMapError::Coercion(JsCoercionError {
                            name: "RepresentationError",
                            ..
                        })
                    ),
                    "positions_typed_boundary"
                );
                assert!(
                    case["result"]["ok"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|p| !p["side"].is_string() || p["leverage"].get("number").is_none()),
                    "source_positions_invalid_native"
                );
                refused += 1;
                pin.compared(i, path);
                continue;
            }
            if let Ok(rows) = &mut result {
                pin.native(i, "/result/ok", rows.len());
                assert_eq!(
                    rows.len(),
                    case["infoAliases"].as_u64().unwrap() as usize,
                    "positions_alias_census"
                );
                if let Some(first) = rows.first_mut() {
                    let before = serde_json::to_value(&input).unwrap();
                    let row = first.info.as_mut().unwrap().as_object_mut().unwrap();
                    row.insert("__ownership_probe".into(), json!(true));
                    assert_eq!(
                        serde_json::to_value(&input).unwrap(),
                        before,
                        "positions_owned_info_boundary"
                    );
                    row.remove("__ownership_probe");
                }
            }
            let actual = match result {
                Ok(rows) => json!({"ok":rows.into_iter().map(native).collect::<Vec<_>>()}),
                Err(PositionMapError::Coercion(e)) => {
                    json!({"error":e.name,"message":e.message,"identity":false})
                }
                Err(PositionMapError::Emitter(e)) => {
                    assert!(Rc::ptr_eq(&e, &sentinel), "positions_emitter_identity");
                    json!({"error":"Error","message":e.as_str(),"identity":true})
                }
            };
            assert_eq!(
                actual, case["result"],
                "positions_native_fields case={i} path={path} raw={raw}"
            );
            pin.compared(i, path);
        }
        pin.hit("cases");
    }
    pin.finish("cases", pin.count("/cases"));
    pin.finish_comparisons(3);
    assert_eq!(refused, 216, "positions_refusal_census");
    pin.finish("fetched-fixture-entries", 540);
    for index in 0..pin.count("/boundaries") {
        let raw = &corpus["boundaries"][index];
        assert!(
            js_from_str::<PositionsInput>(raw.as_str().unwrap()).is_err(),
            "positions_ingress_boundary"
        );
        pin.hit("boundaries-raw-1");
    }
    pin.finish("boundaries-raw-1", pin.count("/boundaries"));
}
