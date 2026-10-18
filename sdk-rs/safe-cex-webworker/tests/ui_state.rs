#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use runtime::Runtime;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
};
use safe_cex_types::ExchangeName;
use safe_cex_webworker::ui_state::handle_active_symbols_change;
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
fn venue() -> Venue {
    let runtime = Rc::new(Runtime::default());
    runtime.advance(0);
    Venue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime,
            Default::default(),
        ),
        capabilities: Default::default(),
    }
}
#[test]
fn active_symbol_changes_match_live_filter_union_and_event_transitions() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ui_state_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = data["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 108, "ACTIVE-CARDINALITY source total");
    let mut comparisons = 0;
    for (i, row) in rows.iter().enumerate() {
        let exchange = venue();
        exchange
            .base
            .set_active_ticker_symbols(
                &serde_json::from_value::<Vec<String>>(row["initial"].clone()).unwrap(),
            )
            .unwrap();
        let events = Rc::new(RefCell::new(Vec::<Value>::new()));
        let captured = events.clone();
        exchange.base.on(
            "activeTickerSymbols:changed",
            Rc::new(move |_, symbols| {
                captured
                    .borrow_mut()
                    .push(json!({"event":"activeTickerSymbols:changed","symbols":safe_cex_exchange::js_runtime::value::to_json(symbols).unwrap()}));
                Ok(())
            }),
        );
        let steps = row["steps"].as_array().unwrap();
        let expected = row["expected"].as_array().unwrap();
        assert_eq!(steps.len(), 4, "ACTIVE-CARDINALITY steps per case");
        assert_eq!(expected.len(), 4, "ACTIVE-CARDINALITY expected per case");
        assert_eq!(
            steps.len(),
            expected.len(),
            "ACTIVE-CARDINALITY paired lengths"
        );
        for j in 0..4 {
            let step = &steps[j];
            handle_active_symbols_change(&exchange, step.get("data")).unwrap();
            assert_eq!(
                json!({"symbols":exchange.base.get_active_ticker_symbols(),"events":*events.borrow()}),
                expected[j],
                "ACTIVE-SYMBOLS {i}/{j}"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 432, "ACTIVE-CARDINALITY native comparisons");
    let exchange = venue();
    assert_eq!(data["witness"]["inherited"], json!(["INHERITED"]));
    handle_active_symbols_change(&exchange, Some(&json!({}))).unwrap();
    assert!(
        exchange.base.get_active_ticker_symbols().is_empty(),
        "CHOSEN own JSON properties omit inherited activeSymbols"
    );
    assert_eq!(data["witness"]["array"], json!(["NAMED"]));
    handle_active_symbols_change(&exchange, Some(&json!([]))).unwrap();
    assert!(
        exchange.base.get_active_ticker_symbols().is_empty(),
        "CHOSEN JSON arrays omit named properties"
    );
    assert_eq!(
        data["witness"]["surrogateOutput"].as_str().unwrap(),
        r#"["\ud800"]"#
    );
    assert!(
        serde_json::from_str::<Value>(data["witness"]["surrogateInput"].as_str().unwrap()).is_err(),
        "shared UTF-8 boundary refuses a valid JSON lone surrogate"
    );
}

fn bits(number: f64) -> String {
    if number.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", number.to_bits())
    }
}
fn from_bits(value: &Value) -> f64 {
    let text = value.as_str().unwrap();
    if text == "NaN" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(text, 16).unwrap())
    }
}
#[test]
fn leverage_updates_match_source_batch_selection_and_native_numbers() {
    use safe_cex_store::StoreUpdate;
    use safe_cex_types::{FetchedLeverage, Position};
    use safe_cex_webworker::ui_state::update_positions_with_leverage;
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ui_leverage_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2058, "LEVERAGE-CARDINALITY source total");
    for (index, row) in rows.iter().enumerate() {
        let exchange = venue();
        let positions: Vec<Position> = [("other", "OTHER", row["side"].as_str().unwrap()),
            ("first", "BTC", row["side"].as_str().unwrap()),
            ("second", "BTC", if row["side"] == "long" { "short" } else { "long" })]
            .into_iter().map(|(id, symbol, side)| {
                let mut position: Position = serde_json::from_value(json!({"id":id,"symbol":symbol,"side":side,"entryPrice":1,"notional":1,"leverage":1,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0})).unwrap();
                position.position_mode = row.get("mode").map(|v| serde_json::from_value(v.clone()).unwrap()).into();
                position.leverage = from_bits(&row["old"]);
                position.unrealized_pnl = f64::NAN;
                position
            }).collect();
        exchange
            .base
            .store
            .update(
                StoreUpdate {
                    positions: Some(positions.clone()),
                    ..Default::default()
                },
                true,
            )
            .unwrap();
        let before = exchange.base.store.update_ids().positions;
        let leverage = FetchedLeverage {
            buy_leverage: from_bits(&row["buy"]),
            sell_leverage: from_bits(&row["sell"]),
            margin_type: None,
            position_mode: None,
            is_fetching: None,
        };
        let result = update_positions_with_leverage(&exchange, "BTC", &leverage);
        assert!(result.is_ok(), "LEVERAGE-NATIVE-PATCH {index}: {result:?}");
        let calls = row["calls"].as_array().unwrap();
        assert_eq!(
            exchange.base.store.update_ids().positions - before,
            calls.len() as f64,
            "LEVERAGE-BATCH {index}"
        );
        let actual_positions = exchange.base.store.positions();
        assert_eq!(
            actual_positions.len(),
            positions.len(),
            "LEVERAGE-CARDINALITY retained positions"
        );
        for (old, actual) in positions.iter().zip(actual_positions.iter()) {
            assert_eq!(actual.id, old.id, "LEVERAGE-IDENTITY retained position id");
            let mut expected = bits(old.leverage);
            for call in calls {
                for update in call.as_array().unwrap() {
                    if update[0]["id"] == old.id {
                        expected = update[1]["leverage"].as_str().unwrap().to_owned();
                    }
                }
            }
            assert_eq!(
                bits(actual.leverage),
                expected,
                "LEVERAGE-NUMBER {index} {}",
                old.id
            );
            assert!(actual.unrealized_pnl.is_nan(), "LEVERAGE-UNRELATED {index}");
        }
    }
    assert_eq!(corpus["emptyCalls"], json!([]));
    let exchange = venue();
    let before = exchange.base.store.update_ids().positions;
    update_positions_with_leverage(
        &exchange,
        "BTC",
        &FetchedLeverage {
            buy_leverage: 1.0,
            sell_leverage: 2.0,
            margin_type: None,
            position_mode: None,
            is_fetching: None,
        },
    )
    .unwrap();
    assert_eq!(
        exchange.base.store.update_ids().positions,
        before,
        "LEVERAGE-EMPTY"
    );
}
