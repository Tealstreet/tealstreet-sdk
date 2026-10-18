use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_value_ref};
use safe_cex_store::StoreUpdate;
use safe_cex_types::{Balance, ModelField, Order, Position};
use safe_cex_venue_breakoutprop::portfolio::*;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, sync::LazyLock};
const SOURCE_CENSUS: &str = include_str!("support/portfolio-census.json");
include!("support/cardinality.rs");
static ORACLE: LazyLock<Value> = LazyLock::new(|| {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/portfolio_cardinality_oracle.cjs"
        ))
        .env("TZ", "UTC")
        .output()
        .expect("execute actual venue TypeScript");
    assert!(
        out.status.success(),
        "portfolio oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_source_census(&corpus, SOURCE_CENSUS);
    corpus
});
fn number(value: f64) -> Value {
    json!({"number":format!("{:016x}",if value.is_nan() {0x7ff8000000000000} else {value.to_bits()})})
}
#[path = "support/retained_info_values.rs"]
mod retained_info_values;
use retained_info_values::observe;

fn optional(value: &mut Value, name: &str, field: Option<Value>) {
    if let Some(field) = field {
        value[name] = field;
    }
}
fn model_field<T>(
    value: &mut Value,
    name: &str,
    field: &ModelField<T>,
    observe: impl FnOnce(&T) -> Value,
) {
    match field {
        ModelField::Absent => {}
        ModelField::Undefined => value[name] = json!({"$undefined":true}),
        ModelField::Value(field) => value[name] = observe(field),
    }
}
fn position(p: &Position) -> Value {
    let mut v = json!({"id":p.id,"symbol":p.symbol,"entryPrice":number(p.entry_price),"notional":number(p.notional),"leverage":number(p.leverage),"unrealizedPnl":number(p.unrealized_pnl),"contracts":number(p.contracts),"liquidationPrice":number(p.liquidation_price)});
    model_field(&mut v, "side", &p.side, |x| json!(x));
    optional(&mut v, "marginType", p.margin_type.map(|x| json!(x)));
    optional(&mut v, "positionMode", p.position_mode.map(|x| json!(x)));
    optional(&mut v, "marginSize", p.margin_size.map(number));
    optional(&mut v, "lastUpdate", p.last_update.map(number));
    optional(&mut v, "info", p.info.as_ref().map(observe));
    v
}
fn balance(b: &Balance) -> Value {
    let mut v = json!({"used":number(b.used),"free":number(b.free),"total":number(b.total),"upnl":number(b.upnl)});
    optional(&mut v, "info", b.info.as_ref().map(observe));
    v
}
fn order(o: &Order) -> Value {
    let mut v = json!({"id":o.id,"symbol":o.symbol,"amount":number(o.amount),"filled":number(o.filled),"remaining":number(o.remaining),"reduceOnly":o.reduce_only,"lastUpdate":number(o.last_update)});
    model_field(&mut v, "status", &o.status, |x| json!(x));
    model_field(&mut v, "type", &o.r#type, |x| json!(x));
    model_field(&mut v, "side", &o.side, |x| json!(x));
    model_field(&mut v, "price", &o.price, |x| number(*x));
    optional(&mut v, "info", o.info.as_ref().map(observe));
    optional(&mut v, "timeInForce", o.time_in_force.map(|x| json!(x)));
    v
}
fn result<T>(
    value: Result<T, ExchangeError>,
    observe: impl FnOnce(T) -> Value,
    errors: &RefCell<Vec<String>>,
) -> Value {
    match value {
        Ok(v) => json!({"ok":observe(v),"errors":errors.borrow().clone()}),
        Err(e) => json!({"error":e.message,"errors":errors.borrow().clone()}),
    }
}
#[test]
fn actual_portfolio_formatters() {
    let mut comparisons = 0;
    eprintln!("portfolio oracle runtime: {}", ORACLE["runtime"]);
    assert!(ORACLE["runtime"]["node"].as_str().unwrap().starts_with('v'));
    assert!(!ORACLE["runtime"]["arch"].as_str().unwrap().is_empty());
    for case in ORACLE["cases"].as_array().unwrap() {
        let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let s: Value = serde_json::from_str(case["storeRaw"].as_str().unwrap()).unwrap();
        let store = DefaultStore::new();
        let mut tickers: Vec<safe_cex_types::Ticker> = js_from_value_ref(&s["tickers"]).unwrap();
        if s["nanMark"] == true {
            tickers[0].mark = f64::NAN;
        }
        safe_cex_store::Store::update(
            &store,
            StoreUpdate {
                balance: Some(js_from_value_ref(&s["balance"]).unwrap()),
                markets: Some(js_from_value_ref(&s["markets"]).unwrap()),
                tickers: Some(tickers),
                orders: Some(js_from_value_ref(&s["orders"]).unwrap()),
                ..StoreUpdate::default()
            },
            false,
        )
        .expect("fixture Store update");
        let symbols: Option<Vec<String>> = serde_json::from_value(case["symbols"].clone()).unwrap();
        let errors = RefCell::new(Vec::new());
        let log = |s: &str| {
            errors.borrow_mut().push(s.to_owned());
            Ok(())
        };
        let mut actual = serde_json::Map::new();
        actual.insert(
            "formatBalance".into(),
            result(
                format_balance(&input, &store, &|ms| ms, &log),
                |v| balance(&v),
                &errors,
            ),
        );
        errors.borrow_mut().clear();
        actual.insert(
            "formatOpenPositions".into(),
            result(
                format_open_positions(&input, &store, &|ms| ms, &log),
                |v| {
                    v.map(|p| Value::Array(p.iter().map(position).collect()))
                        .unwrap_or(json!({"$undefined":true}))
                },
                &errors,
            ),
        );
        actual.insert(
            "formatPositions".into(),
            result(
                format_positions(&input, &store, symbols.as_deref(), &|ms| ms, &log),
                |p| Value::Array(p.iter().map(position).collect()),
                &errors,
            ),
        );
        actual.insert(
            "formatOrders".into(),
            result(
                format_orders(&input, &store, &|ms| ms, &log),
                |v| {
                    v.map(|o| Value::Array(o.iter().map(order).collect()))
                        .unwrap_or(json!({"$undefined":true}))
                },
                &errors,
            ),
        );
        let name = case["name"].as_str().unwrap();
        for (method, value) in &actual {
            if value.get("error").is_none() {
                assert_native_cardinality(
                    value,
                    &case["expected"][method],
                    &format!("portfolio_{method}"),
                );
            }
        }
        for (method, value) in actual {
            comparisons += 1;
            assert_native_cardinality(
                &value,
                &case["expected"][&method],
                &format!("portfolio_{method}"),
            );
            assert_eq!(
                value, case["expected"][&method],
                "portfolio_{method}: {name}"
            );
        }
    }
    assert_eq!(comparisons, 112, "COMPARISON-COUNT portfolio");
}

#[test]
fn owned_info_boundary() {
    let case = &ORACLE["cases"][0];
    let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
    let store = DefaultStore::new();
    for alias in ORACLE["aliases"].as_array().unwrap() {
        let method = alias["method"].as_str().unwrap();
        let mut info = match method {
            "formatBalance" => format_balance(&input, &store, &|ms| ms, &|_| Ok(()))
                .unwrap()
                .info
                .unwrap(),
            "formatOpenPositions" => format_open_positions(&input, &store, &|ms| ms, &|_| Ok(()))
                .unwrap()
                .unwrap()
                .into_iter()
                .next()
                .unwrap()
                .info
                .unwrap(),
            "formatPositions" => {
                format_positions(&input, &store, Some(&["BTCUSD".into()]), &|ms| ms, &|_| {
                    Ok(())
                })
                .unwrap()
                .into_iter()
                .next()
                .unwrap()
                .info
                .unwrap()
            }
            "formatOrders" => format_orders(&input, &store, &|ms| ms, &|_| Ok(()))
                .unwrap()
                .unwrap()
                .into_iter()
                .next()
                .unwrap()
                .info
                .unwrap(),
            _ => panic!("method inventory"),
        };
        if method == "formatBalance" {
            *info.get_mut("portfolios").unwrap().as_array_mut().unwrap()[0]
                .get_mut("positions")
                .unwrap()
                .as_array_mut()
                .unwrap()[0]
                .get_mut("openPrice")
                .unwrap() = json!(999).into();
        } else if method == "formatOrders" {
            *info.get_mut("orderCode").unwrap() = json!("changed-through-result").into();
        } else {
            *info.get_mut("openPrice").unwrap() = json!(999).into();
        }
        let decoded: safe_cex_venue_breakoutprop::types::PortfolioResponse =
            input.clone().decode().unwrap();
        if method == "formatOrders" {
            assert_eq!(
                alias["changed"], "changed-through-result",
                "source_live_info_{method}"
            );
            assert_eq!(
                decoded.portfolios[0].orders[0].order_code, "id-A",
                "owned_info_{method}"
            );
        } else {
            assert_eq!(alias["changed"], 999, "source_live_info_{method}");
            assert_eq!(
                decoded.portfolios[0].positions[0].open_price, 37.0,
                "owned_info_{method}"
            );
        }
    }
}
#[test]
fn portfolio_utf8_boundary() {
    for case in ORACLE["surrogates"].as_array().unwrap() {
        assert_eq!(
            case["codeUnit"], 55296,
            "source_lone_surrogate_{}",
            case["method"]
        );
        let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let retained: safe_cex_types::retained_json::RetainedJsonInput =
            input.clone().decode().unwrap();
        assert_eq!(
            retained.get("portfolios").unwrap().as_array().unwrap()[0]
                .get("positions")
                .unwrap()
                .as_array()
                .unwrap()[0]
                .get("symbol")
                .unwrap()
                .as_text(),
            Some([0xd800].as_slice())
        );
        let store = DefaultStore::new();
        let report = |_: &str| -> Result<(), ExchangeError> {
            panic!("retained symbol is not a source mapping error")
        };
        match case["method"].as_str().unwrap() {
            "formatBalance" => {
                let balance = format_balance(&input, &store, &|ms| ms, &report).unwrap();
                let info = balance.info.as_ref().unwrap();
                assert_eq!(
                    info.get("portfolios").unwrap().as_array().unwrap()[0]
                        .get("positions")
                        .unwrap()
                        .as_array()
                        .unwrap()[0]
                        .get("symbol")
                        .unwrap()
                        .as_text(),
                    Some([0xd800].as_slice())
                );
                assert!(
                    balance.used > 0.0,
                    "native position reduction precedes closed model projection"
                );
            }
            "formatOpenPositions" => {
                let rows = format_open_positions(&input, &store, &|ms| ms, &report)
                    .unwrap()
                    .unwrap();
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].symbol, "\u{fffd}");
                assert_eq!(rows[1].symbol, "ETHUSD");
                assert_eq!(
                    rows[0]
                        .info
                        .as_ref()
                        .unwrap()
                        .get("symbol")
                        .unwrap()
                        .as_text(),
                    Some([0xd800].as_slice())
                );
            }
            "formatOrders" => {
                let rows = format_orders(&input, &store, &|ms| ms, &report)
                    .unwrap()
                    .unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].id, "id-A");
                assert_eq!(rows[0].symbol, "\u{fffd}");
                assert_eq!(
                    rows[0]
                        .info
                        .as_ref()
                        .unwrap()
                        .get("instrument")
                        .unwrap()
                        .as_text(),
                    Some([0xd800].as_slice())
                );
            }
            "formatPositions" => {
                assert!(
                    serde_json::from_str::<Vec<String>>(r#"["\ud800"]"#).is_err(),
                    "source requested symbol cannot enter typed filter"
                );
                assert!(
                    format_positions(&input, &store, None, &|ms| ms, &report)
                        .unwrap()
                        .is_empty(),
                    "default empty Store symbol filter"
                );
            }
            method => panic!("unknown surrogate formatter {method}"),
        }
    }
}

#[test]
fn fallback_snapshot_boundary() {
    for expected in ORACLE["fallbackAliases"].as_array().unwrap() {
        let method = expected["method"].as_str().unwrap();
        let name = if method == "formatBalance" {
            "empty-portfolios"
        } else {
            "empty-leg"
        };
        let case = ORACLE["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap();
        let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let raw: Value = serde_json::from_str(case["storeRaw"].as_str().unwrap()).unwrap();
        let store = DefaultStore::new();
        safe_cex_store::Store::update(
            &store,
            StoreUpdate {
                balance: Some(js_from_value_ref(&raw["balance"]).unwrap()),
                orders: Some(js_from_value_ref(&raw["orders"]).unwrap()),
                ..StoreUpdate::default()
            },
            false,
        )
        .expect("fixture Store update");
        if method == "formatBalance" {
            let mut returned = format_balance(&input, &store, &|ms| ms, &|_| Ok(())).unwrap();
            returned.free = 123.0;
            assert_eq!(returned.free, 123.0);
            assert_eq!(expected["observed"], 123, "source_balance_fallback_alias");
            assert_eq!(
                safe_cex_store::Store::balance(&store).unwrap().free,
                893.0,
                "owned_balance_fallback"
            );
        } else {
            let mut returned = format_orders(&input, &store, &|ms| ms, &|_| Ok(()))
                .unwrap()
                .unwrap();
            returned[0].id = "changed-cache".into();
            assert_eq!(returned[0].id, "changed-cache");
            assert_eq!(
                expected["observed"], "changed-cache",
                "source_orders_fallback_alias"
            );
            assert_eq!(
                safe_cex_store::Store::orders(&store).unwrap()[0].id,
                "cached-order",
                "owned_orders_fallback"
            );
        }
    }
}

#[test]
fn inherited_closed_output_boundary() {
    assert_eq!(ORACLE["inherited"].as_array().unwrap().len(), 12);
    for case in ORACLE["inherited"].as_array().unwrap() {
        let key = case["key"].as_str().unwrap();
        let field = case["field"].as_str().unwrap();
        assert_eq!(
            case["beforeSerialization"]["type"],
            if key == "__proto__" {
                "object"
            } else {
                "function"
            },
            "source_inherited_typeof_{field}_{key}"
        );
        assert_eq!(
            case["beforeSerialization"]["truthy"], true,
            "source_inherited_truthy_{field}_{key}"
        );
        let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let store = DefaultStore::new();
        if field == "position-side" {
            assert_eq!(
                case["methodsSucceed"],
                json!(["formatBalance", "formatOpenPositions", "formatPositions"])
            );
            let expected = format!("Unrepresentable position side: {key}");
            assert_eq!(
                format_open_positions(&input, &store, &|ms| ms, &|_| Ok(()))
                    .unwrap_err()
                    .message,
                expected,
                "inherited_formatOpenPositions_{key}"
            );
            assert!(
                format_balance(&input, &store, &|ms| ms, &|_| Ok(())).is_ok(),
                "inherited side is unread by balance"
            );
            assert!(
                format_positions(&input, &store, Some(&[]), &|ms| ms, &|_| Ok(()))
                    .unwrap()
                    .is_empty(),
                "unselected inherited side never crosses the public Position boundary"
            );
            assert_eq!(
                format_positions(&input, &store, Some(&["BTCUSD".into()]), &|ms| ms, &|_| Ok(
                    ()
                ))
                .unwrap_err()
                .message,
                expected,
                "inherited selected position remains an explicit output refusal"
            );
        } else {
            assert_eq!(case["methodsSucceed"], json!(["formatOrders"]));
            assert_eq!(
                format_orders(&input, &store, &|ms| ms, &|_| Ok(()))
                    .unwrap_err()
                    .message,
                format!("Unrepresentable order {field}: {key}"),
                "inherited_formatOrders_{field}_{key}"
            );
        }
    }
}

include!("support/logger.rs");
#[test]
fn formatter_logger_failure_preserves_metadata() {
    assert_eq!(ORACLE["loggerFailures"].as_array().unwrap().len(), 2);
    for case in ORACLE["loggerFailures"].as_array().unwrap() {
        let method = case["method"].as_str().unwrap();
        assert_eq!(case["calls"], 1, "LOGGER source calls {method}");
        assert_eq!(case["sameError"], true, "LOGGER source identity {method}");
        let original = logger_error_fixture();
        assert_eq!(
            logger_error_observe(&original),
            case["error"],
            "LOGGER source metadata {method}"
        );
        let calls = std::cell::Cell::new(0);
        let report = |_: &str| {
            calls.set(calls.get() + 1);
            Err(original.clone())
        };
        let input = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let store = DefaultStore::new();
        let result = match method {
            "formatBalance" => format_balance(&input, &store, &|ms| ms, &report).map(|_| ()),
            "formatOrders" => format_orders(&input, &store, &|ms| ms, &report).map(|_| ()),
            _ => unreachable!(),
        };
        assert_eq!(result, Err(original), "LOGGER-PROPAGATION {method}");
        assert_eq!(calls.get(), 1, "LOGGER native calls {method}");
    }
}

#[test]
fn source_only_fields_and_unread_collections_match_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/portfolio_unused_fields_oracle.cjs"
        ))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 82);
    for case in cases.as_array().unwrap() {
        let input: JsNumberInput = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let store = DefaultStore::new();
        let state: Value = serde_json::from_str(case["storeRaw"].as_str().unwrap()).unwrap();
        let patch = StoreUpdate {
            balance: Some(js_from_value_ref(&state["balance"]).unwrap()),
            positions: Some(js_from_value_ref(&state["positions"]).unwrap()),
            orders: Some(js_from_value_ref(&state["orders"]).unwrap()),
            markets: Some(js_from_value_ref(&state["markets"]).unwrap()),
            tickers: Some(js_from_value_ref(&state["tickers"]).unwrap()),
            ..Default::default()
        };
        safe_cex_store::Store::update(&store, patch, false).unwrap();
        let errors = RefCell::new(Vec::new());
        let log = |message: &str| {
            errors.borrow_mut().push(message.to_owned());
            Ok(())
        };
        let method = case["method"].as_str().unwrap();
        let actual = match method {
            "formatBalance" => result(
                format_balance(&input, &store, &|ms| ms, &log),
                |b| balance(&b),
                &errors,
            ),
            "formatOpenPositions" => result(
                format_open_positions(&input, &store, &|ms| ms, &log),
                |rows| {
                    rows.map(|rows| Value::Array(rows.iter().map(position).collect()))
                        .unwrap_or(json!({"$undefined":true}))
                },
                &errors,
            ),
            "formatPositions" => result(
                format_positions(&input, &store, None, &|ms| ms, &log),
                |rows| Value::Array(rows.iter().map(position).collect()),
                &errors,
            ),
            "formatOrders" => result(
                format_orders(&input, &store, &|ms| ms, &log),
                |rows| {
                    rows.map(|rows| Value::Array(rows.iter().map(order).collect()))
                        .unwrap_or(json!({"$undefined":true}))
                },
                &errors,
            ),
            _ => panic!("source method inventory"),
        };
        assert_eq!(
            actual, case["expected"],
            "{method} malformedUnread={}",
            case["malformedUnread"]
        );
    }
}
