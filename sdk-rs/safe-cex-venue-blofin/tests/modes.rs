#[path = "support/retained_assert.rs"]
mod retained_assert;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value};
use safe_cex_store::{Store, StoreMutationError, StoreUpdate};
use safe_cex_types::{
    FetchedLeverage, MarginType, OrderSide, OrderType, PositionMode, StoreSubscriberError,
};
use safe_cex_venue_blofin::{modes::*, utility::UtilityInput};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    future::Future,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static CORPUS: OnceLock<Value> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/modes_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual_typescript_modes: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["sideCases"].as_array().unwrap().len(),
            48,
            "independent_side_floor"
        );
        assert_eq!(
            value["applyCases"].as_array().unwrap().len(),
            6,
            "independent_apply_floor"
        );
        assert_eq!(
            value["workerCases"].as_array().unwrap().len(),
            12,
            "independent_worker_floor"
        );
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            other => other,
        };
        assert_eq!(value["header"]["architecture"], arch);
        eprintln!("modes oracle header: {}", value["header"]);
        value
    })
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("ready fixture"),
    }
}
fn mode(value: &Value) -> PositionMode {
    js_from_value(value.clone()).unwrap()
}
fn fixture_store(input: &Value) -> DefaultStore {
    let store = DefaultStore::new();
    let update = StoreUpdate {
        positions: input
            .get("positions")
            .map(|value| js_from_value(value.clone()).unwrap()),
        fetched_leverage: input.get("fetchedLeverage").map(|value| {
            value
                .as_object()
                .unwrap()
                .iter()
                .map(|(symbol, row)| {
                    (
                        symbol.clone(),
                        FetchedLeverage {
                            buy_leverage: row["buyLeverage"].as_f64().unwrap(),
                            sell_leverage: row["sellLeverage"].as_f64().unwrap(),
                            margin_type: row
                                .get("marginType")
                                .map(|value| js_from_value(value.clone()).unwrap()),
                            position_mode: row.get("positionMode").map(mode),
                            is_fetching: row["isFetching"].as_bool(),
                        },
                    )
                })
                .collect()
        }),
        ..Default::default()
    };
    Store::update(&store, update, false).unwrap();
    store
}
#[test]
fn native_position_side_mode_precedence_flip_and_fetch_error() {
    for case in corpus()["sideCases"].as_array().unwrap() {
        let opts = &case["opts"];
        let options = PositionSideOptions {
            symbol: opts["symbol"].as_str().unwrap(),
            side: (js_from_value::<OrderSide>(opts["side"].clone()).unwrap()).into(),
            order_type: (js_from_value::<OrderType>(opts["type"].clone()).unwrap()).into(),
            reduce_only: opts["reduceOnly"].as_bool().unwrap(),
        };
        let calls = Cell::new(0);
        let expected_mode = (!case["mode"].is_null()).then(|| mode(&case["mode"]));
        let actual = ready(get_position_side(&options, |symbol| {
            calls.set(calls.get() + 1);
            assert_eq!(symbol, "BTCUSDT", "native_fetch_symbol");
            std::future::ready(Ok::<_, &'static str>(expected_mode))
        }))
        .unwrap();
        assert_eq!(
            actual,
            case["value"].as_str().unwrap(),
            "native_position_side {case}"
        );
        assert_eq!(calls.get(), 1, "native_mode_fetch_once");
    }
    let opts = PositionSideOptions {
        symbol: "ETHUSDT",
        side: safe_cex_types::ModelField::Value(OrderSide::Buy),
        order_type: safe_cex_types::ModelField::Value(OrderType::Limit),
        reduce_only: false,
    };
    let error = ready(get_position_side(&opts, |symbol| {
        assert_eq!(symbol, "ETHUSDT");
        std::future::ready(Err::<Option<PositionMode>, _>("mode fetch probe"))
    }));
    assert_eq!(error, Err("mode fetch probe"), "native_fetch_error");
}
#[test]
fn native_confirmed_mode_cache_first_rekeys_first_id_and_preserves_fields() {
    for case in corpus()["applyCases"].as_array().unwrap() {
        let store = fixture_store(&case["input"]);
        let state = Rc::new(AccountModes::default());
        let expected_mode = mode(&case["mode"]);
        let count = Rc::new(Cell::new(0));
        let observer_state = state.clone();
        let observer_count = count.clone();
        let _unsubscribe = Store::subscribe(
            &store,
            Box::new(move |_, from_worker| {
                assert_eq!(
                    observer_state.position_mode.get(),
                    Some(expected_mode),
                    "native_cache_precedes_store_callback"
                );
                assert!(!from_worker);
                observer_count.set(observer_count.get() + 1);
                Ok(())
            }),
        );
        apply_confirmed_position_mode(&state, &store, expected_mode).unwrap();
        assert_eq!(
            state.position_mode.get(),
            Some(expected_mode),
            "native_confirmed_cache"
        );
        assert_eq!(
            count.get(),
            case["updates"].as_u64().unwrap(),
            "native_single_or_no_store_update"
        );
        let positions = Store::positions(&store);
        let rows = case["positions"].as_array().unwrap();
        let expected_len = if case["input"].get("positions").is_none() {
            0
        } else if expected_mode == PositionMode::OneWay {
            2
        } else {
            3
        };
        assert_eq!(
            positions.len(),
            expected_len,
            "native_independent_position_count"
        );
        assert_eq!(rows.len(), expected_len);
        for (p, row) in positions.iter().zip(rows) {
            assert_eq!(p.id, row["id"].as_str().unwrap(), "native_position_id");
            assert_eq!(p.symbol, row["symbol"].as_str().unwrap());
            assert_eq!(
                p.side,
                js_from_value::<safe_cex_types::PositionSide>(row["side"].clone()).unwrap()
            );
            assert_eq!(p.position_mode, Some(expected_mode));
            assert_eq!(p.margin_type, Some(MarginType::Cross));
            assert_eq!(
                p.entry_price,
                row["entryPrice"].as_f64().unwrap(),
                "native_entry_price"
            );
            assert_eq!(p.notional, row["notional"].as_f64().unwrap());
            assert_eq!(p.leverage, row["leverage"].as_f64().unwrap());
            assert_eq!(p.unrealized_pnl, row["unrealizedPnl"].as_f64().unwrap());
            assert_eq!(p.realized_pnl, Some(row["realizedPnl"].as_f64().unwrap()));
            assert_eq!(p.contracts, row["contracts"].as_f64().unwrap());
            assert_eq!(
                p.liquidation_price,
                row["liquidationPrice"].as_f64().unwrap()
            );
            assert_eq!(p.margin_size, Some(row["marginSize"].as_f64().unwrap()));
            assert_eq!(p.last_update, Some(row["lastUpdate"].as_f64().unwrap()));
            retained_assert::assert_value(
                p.info.as_ref().unwrap().get("tag").unwrap(),
                &row["info"]["tag"],
            );
            assert_eq!(
                p.extra.as_ref().unwrap().contracts_native,
                Some(row["extra"]["contractsNative"].as_f64().unwrap())
            );
        }
        if expected_len > 0 {
            assert_eq!(
                positions[0].entry_price, 102.0,
                "native_first_collision_wins"
            );
        }
        drop(positions);
        let leverage = Store::fetched_leverage(&store);
        let expected_leverage_count = if case["input"].get("fetchedLeverage").is_some() {
            2
        } else {
            0
        };
        assert_eq!(leverage.len(), expected_leverage_count);
        for (symbol, native) in leverage.iter() {
            let row = &case["leverage"][symbol];
            assert_eq!(
                native.position_mode,
                Some(expected_mode),
                "native_leverage_mode"
            );
            assert_eq!(native.buy_leverage, row["buyLeverage"].as_f64().unwrap());
            assert_eq!(native.sell_leverage, row["sellLeverage"].as_f64().unwrap());
            assert_eq!(
                native.margin_type,
                Some(js_from_value(row["marginType"].clone()).unwrap())
            );
            assert_eq!(native.is_fetching, row["isFetching"].as_bool());
        }
    }
    let case = &corpus()["applyCases"][2];
    let store = fixture_store(&case["input"]);
    let state = AccountModes::default();
    let _unsubscribe = Store::subscribe(
        &store,
        Box::new(|_, _| {
            Err(StoreSubscriberError::with_response(
                "subscriber probe",
                json!({"native":17}),
            ))
        }),
    );
    let error = apply_confirmed_position_mode(&state, &store, PositionMode::OneWay).unwrap_err();
    let StoreMutationError::Notification(error) = error else {
        panic!("native notification error origin")
    };
    assert_eq!(error.message(), "subscriber probe");
    assert_eq!(error.cause().response(), Some(&json!({"native":17})));
    assert_eq!(state.position_mode.get(), Some(PositionMode::OneWay));
    assert_eq!(
        Store::positions(&store).len(),
        2,
        "native_mutation_precedes_notification_error"
    );
}
#[test]
fn native_worker_confirmed_result_fallback_margin_and_unknown_method() {
    for case in corpus()["workerCases"].as_array().unwrap() {
        let state = AccountModes::default();
        state.position_mode.set(Some(PositionMode::Hedged));
        state.margin_type.set(Some(MarginType::Cross));
        let result: UtilityInput = js_from_value(case["result"].clone()).unwrap();
        let arg: UtilityInput = js_from_value(case["arg"].clone()).unwrap();
        after_method_called_in_worker(
            &state,
            &DefaultStore::new(),
            Some(&result),
            case["method"].as_str().unwrap(),
            Some(&arg),
        )
        .unwrap();
        assert_eq!(
            state.position_mode.get(),
            Some(mode(&case["mode"])),
            "native_worker_confirmed_mode {case}"
        );
        assert_eq!(
            state.margin_type.get(),
            Some(js_from_value(case["margin"].clone()).unwrap()),
            "native_worker_margin {case}"
        );
    }
    let state = AccountModes::default();
    let bad: UtilityInput = js_from_str(r#"{"positionMode":"bogus"}"#).unwrap();
    let error = after_method_called_in_worker(
        &state,
        &DefaultStore::new(),
        Some(&bad),
        "changePositionMode",
        None,
    )
    .unwrap_err();
    assert_eq!(
        error,
        StoreMutationError::InvalidOperation(
            "Blofin worker result has an unsupported positionMode"
        )
    );
    assert_eq!(
        state.position_mode.get(),
        None,
        "native_unsupported_mode_refuses_before_cache_write"
    );
}
