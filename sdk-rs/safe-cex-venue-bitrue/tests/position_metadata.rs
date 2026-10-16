use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::{js_from_str, js_value_as_f64};
use safe_cex_types::{MarginType, PositionMode, PositionSide};
use safe_cex_venue_bitrue::{
    account_helpers::AccountPositionSnapshot,
    position_metadata::{PositionMetadata, PositionMetadataHandle, fetch_position_metadata},
    wire::OpaqueValue,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn run<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..3 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
    panic!("BITRUE-METADATA unexpected pending fetch");
}
fn side(value: &Value) -> PositionSide {
    match value.as_str().unwrap() {
        "long" => PositionSide::Long,
        "short" => PositionSide::Short,
        _ => panic!("side"),
    }
}
fn metadata(value: &Value) -> Vec<(String, PositionMetadataHandle)> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["symbol"].as_str().unwrap().to_owned(),
                Rc::new(RefCell::new(PositionMetadata {
                    symbol: v["symbol"].as_str().unwrap().into(),
                    side: side(&v["side"]),
                    leverage: js_value_as_f64(&v["leverage"]).unwrap(),
                    position_mode: match v["positionMode"].as_str().unwrap() {
                        "hedged" => PositionMode::Hedged,
                        "oneway" => PositionMode::OneWay,
                        _ => panic!("mode"),
                    },
                    margin_type: match v["marginType"].as_str().unwrap() {
                        "cross" => MarginType::Cross,
                        "isolated" => MarginType::Isolated,
                        _ => panic!("margin"),
                    },
                })),
            )
        })
        .collect()
}
fn native(value: &PositionMetadata) -> Value {
    json!({"symbol":value.symbol,
        "side":match value.side {PositionSide::Long=>"long",PositionSide::Short=>"short"},
        "leverage":format!("{:016x}",if value.leverage.is_nan(){0x7ff8000000000000}else{value.leverage.to_bits()}),
        "positionMode":match value.position_mode {PositionMode::OneWay=>"oneway",PositionMode::Hedged=>"hedged"},
        "marginType":match value.margin_type {MarginType::Cross=>"cross",MarginType::Isolated=>"isolated"}})
}
fn compare(
    actual: &[PositionMetadataHandle],
    expected: &Value,
    label: &str,
    comparisons: &mut usize,
) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "BITRUE-METADATA native collection length {label}"
    );
    for index in 0..expected.len() {
        assert_eq!(
            native(&actual[index].borrow()),
            expected[index],
            "BITRUE-METADATA native fields {label} index {index}"
        );
        *comparisons += 1;
    }
}
#[test]
fn position_metadata_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-metadata-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live metadata oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    println!("capture {}", capture["header"]);
    for key in ["utf8Raw", "depthRaw"] {
        assert!(
            js_from_str::<OpaqueValue>(capture[key].as_str().unwrap()).is_err(),
            "BITRUE-METADATA CHOSEN shared ingress refusal {key}"
        );
    }
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 100, "BITRUE-METADATA corpus count");
    let ledger: Vec<[usize; 2]> =
        serde_json::from_str(include_str!("position_metadata_counts.json")).unwrap();
    assert_eq!(ledger.len(), 100, "BITRUE-METADATA count ledger");
    let mut comparisons = 0;
    for (index, row) in rows.iter().enumerate() {
        let expected_output = row["result"]["ok"].as_array();
        let expected_cache = row["cache"].as_array().unwrap();
        assert_eq!(
            [expected_output.map_or(0, Vec::len), expected_cache.len()],
            ledger[index],
            "BITRUE-METADATA source per-case counts {index}"
        );
        let input = &row["input"];
        let cache = RefCell::new(metadata(&input["initial"]));
        let disposed = Cell::new(input["disposed"].as_bool().unwrap());
        let calls = Cell::new(0);
        let events = RefCell::new(Vec::<&str>::new());
        let symbols: Vec<String> = serde_json::from_value(input["symbols"].clone()).unwrap();
        let current = input["currentRaw"]
            .as_str()
            .map(|raw| js_from_str::<OpaqueValue>(raw).unwrap().value);
        let snapshots = current.as_ref().map(|value| {
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|p| AccountPositionSnapshot {
                    symbol: p["symbol"].as_str().unwrap(),
                    side: side(&p["side"]),
                    leverage_level: &p["leverageLevel"],
                    position_type: &p["positionType"],
                })
                .collect::<Vec<_>>()
        });
        let result = run(fetch_position_metadata(
            &cache,
            &symbols,
            snapshots.as_deref(),
            || {
                Ok(async {
                    calls.set(calls.get() + 1);
                    events.borrow_mut().push("fetch");
                    let mut pending = true;
                    poll_fn(|cx| {
                        if pending {
                            pending = false;
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        } else {
                            Poll::Ready(())
                        }
                    })
                    .await;
                    events.borrow_mut().push("resume");
                    if input["disposeAfterAwait"].as_bool().unwrap() {
                        disposed.set(true);
                    }
                    if input.get("midCache").is_some() {
                        *cache.borrow_mut() = metadata(&input["midCache"]);
                    }
                    if input["fetchError"].as_bool().unwrap() {
                        return Err(ExchangeError::new("fetch rejected"));
                    }
                    Ok(input["accountRaw"]
                        .as_str()
                        .map(|raw| js_from_str::<OpaqueValue>(raw).unwrap().value))
                })
            },
            || disposed.get(),
        ));
        match result {
            Ok(values) => {
                assert!(
                    expected_output.is_some(),
                    "BITRUE-METADATA expected rejection {index}"
                );
                compare(
                    &values,
                    &row["result"]["ok"],
                    &format!("output {index}"),
                    &mut comparisons,
                );
            }
            Err(error) => assert_eq!(
                json!({"name":error.name,"message":error.message}),
                row["result"]["error"],
                "BITRUE-METADATA rejection {index}"
            ),
        }
        compare(
            &cache
                .borrow()
                .iter()
                .map(|(_, entry)| entry.clone())
                .collect::<Vec<_>>(),
            &row["cache"],
            &format!("cache {index}"),
            &mut comparisons,
        );
        assert_eq!(
            calls.get(),
            row["calls"].as_u64().unwrap(),
            "BITRUE-METADATA fetch count {index}"
        );
        assert_eq!(
            json!(*events.borrow()),
            row["events"],
            "BITRUE-METADATA fetch events {index}"
        );
        assert_eq!(
            disposed.get(),
            row["disposed"].as_bool().unwrap(),
            "BITRUE-METADATA lifecycle {index}"
        );
    }
    assert_eq!(
        comparisons, COMPARISON_COUNT,
        "BITRUE-METADATA executed record comparisons"
    );
    // Source returns the same mutable cache object for duplicate requests.
    let cache = RefCell::new(metadata(&rows[0]["input"]["initial"]));
    let symbols = vec!["BTC".to_owned(), "BTC".to_owned()];
    let duplicated = run(fetch_position_metadata(
        &cache,
        &symbols,
        None,
        || Ok(async { panic!("identity cache hit must not fetch") }),
        || false,
    ))
    .unwrap();
    assert_eq!(duplicated.len(), 2, "BITRUE-METADATA identity output count");
    let duplicate_identity = Rc::ptr_eq(&duplicated[0], &duplicated[1]);
    let cache_identity = Rc::ptr_eq(&duplicated[0], &cache.borrow()[0].1);
    assert!(
        duplicate_identity && cache_identity,
        "BITRUE-METADATA shared record identity"
    );
    duplicated[0].borrow_mut().symbol = "MUTATED".into();
    let key_fetches = Cell::new(0);
    let by_key = run(fetch_position_metadata(
        &cache,
        &symbols,
        None,
        || {
            Ok(async {
                key_fetches.set(key_fetches.get() + 1);
                Ok(None)
            })
        },
        || false,
    ))
    .unwrap();
    assert_eq!(
        key_fetches.get(),
        0,
        "BITRUE-METADATA stable key fetch count"
    );
    assert_eq!(by_key.len(), 2, "BITRUE-METADATA stable key output count");
    let key_identity =
        Rc::ptr_eq(&by_key[0], &duplicated[0]) && Rc::ptr_eq(&by_key[1], &duplicated[0]);
    assert!(key_identity, "BITRUE-METADATA stable key identity");
    assert_eq!(
        json!({"keyFetches":key_fetches.get(),"keyIdentity":key_identity,"keySymbol":by_key[0].borrow().symbol}),
        json!({"keyFetches":capture["identity"]["keyFetches"],"keyIdentity":capture["identity"]["keyIdentity"],"keySymbol":capture["identity"]["keySymbol"]}),
        "BITRUE-METADATA stable key source parity"
    );
    duplicated[0].borrow_mut().leverage = 31.0;
    let mutated = [
        duplicated[1].borrow().leverage,
        cache.borrow()[0].1.borrow().leverage,
    ];
    let leverage = json!(41);
    let position_type = json!(1);
    let snapshots = [AccountPositionSnapshot {
        symbol: "BTC",
        side: PositionSide::Short,
        leverage_level: &leverage,
        position_type: &position_type,
    }];
    let replaced = run(fetch_position_metadata(
        &cache,
        &symbols[..1],
        Some(&snapshots),
        || Ok(async { panic!("supplied identity snapshot must not fetch") }),
        || false,
    ))
    .unwrap();
    assert_eq!(
        replaced.len(),
        1,
        "BITRUE-METADATA replacement output count"
    );
    let detached = !Rc::ptr_eq(&duplicated[0], &replaced[0]);
    let retained = [
        duplicated[0].borrow().leverage,
        duplicated[1].borrow().leverage,
        replaced[0].borrow().leverage,
    ];
    // Compare numeric bits independently of JSON integer/float representation.
    let expected = &capture["identity"];
    assert_eq!(
        duplicate_identity,
        expected["duplicateIdentity"].as_bool().unwrap()
    );
    assert_eq!(cache_identity, expected["cacheIdentity"].as_bool().unwrap());
    assert_eq!(
        detached,
        expected["detached"].as_bool().unwrap(),
        "BITRUE-METADATA replacement identity"
    );
    for (key, actual) in [
        ("mutated", mutated.as_slice()),
        ("retained", retained.as_slice()),
    ] {
        assert_eq!(
            expected[key].as_array().unwrap().len(),
            actual.len(),
            "BITRUE-METADATA identity source count"
        );
        for (index, number) in actual.iter().enumerate() {
            assert_eq!(
                number.to_bits(),
                js_value_as_f64(&expected[key][index]).unwrap().to_bits(),
                "BITRUE-METADATA shared mutation {key} {index}"
            );
        }
    }
}
const COMPARISON_COUNT: usize = 154;
