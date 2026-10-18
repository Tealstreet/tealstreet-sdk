#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use runtime::Runtime;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, FetchedLeverage, StoreData};
use safe_cex_webworker::ui_state::handle_active_margin_symbol_change;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

struct Venue {
    base: BaseExchange,
    caps: ExchangeCapabilities,
    spec: Value,
    events: Rc<RefCell<Vec<Value>>>,
    calls: Cell<usize>,
    pending: bool,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.caps
    }
    fn fetch_leverage_for_symbol(
        &self,
        symbol: String,
    ) -> ExchangeFuture<'_, Option<FetchedLeverage>> {
        Box::pin(async move {
            self.calls.set(self.calls.get() + 1);
            self.events
                .borrow_mut()
                .push(json!({"event":"fetch","symbol":symbol}));
            if self.pending {
                std::future::pending::<()>().await;
            }
            if self.spec["concurrent"] == true {
                let mut fetched = self.base.store.fetched_leverage().unwrap().clone();
                fetched.insert("OTHER".into(), leverage(7.0, 8.0));
                self.base.store.update(
                    StoreUpdate {
                        fetched_leverage: Some(fetched),
                        ..Default::default()
                    },
                    true,
                )?;
            }
            if self.spec["concurrentNumeric"] == true {
                // Supply an insertion-ordered host map so cleanup itself must restore JS key order.
                let mut fetched = self.base.store.fetched_leverage().unwrap().clone();
                fetched.insert("42".into(), leverage(21.0, 23.0));
                self.base.store.update(
                    StoreUpdate {
                        fetched_leverage: Some(fetched),
                        ..Default::default()
                    },
                    true,
                )?;
            }
            match self.spec["outcome"].as_str() {
                Some("reject") => Err(ExchangeError::new("fetch failed")),
                Some("none") => Ok(None),
                _ => Ok(Some(leverage(11.0, 13.0))),
            }
        })
    }
}
fn numeric(value: Value) -> Value {
    match value {
        Value::Number(number) => json!(number.as_f64().unwrap()),
        Value::Array(values) => Value::Array(values.into_iter().map(numeric).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, numeric(value)))
                .collect(),
        ),
        other => other,
    }
}
fn leverage(buy: f64, sell: f64) -> FetchedLeverage {
    FetchedLeverage {
        buy_leverage: buy,
        sell_leverage: sell,
        margin_type: None,
        position_mode: None,
        is_fetching: None,
    }
}
fn view(data: &StoreData) -> Value {
    json!({"fetched":data.fetched_leverage,"leverage":data.positions.iter().map(|p|p.leverage).collect::<Vec<_>>()})
}
fn setup(spec: Value, pending: bool) -> (Venue, Rc<Runtime>, Rc<dyn Store>) {
    let runtime = Rc::new(Runtime::default());
    runtime.advance(0);
    let store: Rc<dyn Store> = Rc::new(DefaultStore::new());
    let position = json!({"id":"p","symbol":"BTC","side":"long","entryPrice":100,"contracts":2,"notional":200,"leverage":5,"unrealizedPnl":10,"liquidationPrice":0});
    let mut fetched = indexmap::IndexMap::new();
    if spec["existing"] == true {
        let mut previous = leverage(3.0, 4.0);
        previous.is_fetching = Some(spec["fetching"] == true);
        fetched.insert("BTC".into(), previous);
    }
    if spec["initialOther"] == true {
        fetched.insert("OTHER".into(), leverage(7.0, 8.0));
    }
    store
        .update(
            StoreUpdate {
                positions: Some(if spec["positions"] == false {
                    vec![]
                } else {
                    vec![serde_json::from_value(position).unwrap()]
                }),
                fetched_leverage: Some(fetched),
                ..Default::default()
            },
            true,
        )
        .unwrap();
    let base = BaseExchange::new(
        ExchangeName::Bybit,
        serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
        store.clone(),
        runtime.clone(),
        Default::default(),
    );
    (
        Venue {
            base,
            caps: Default::default(),
            spec,
            events: Rc::default(),
            calls: Cell::new(0),
            pending,
        },
        runtime,
        store,
    )
}
fn expected_specs() -> [Value; 32] {
    [
        json!({"name":"disabled","enabled":false}),
        json!({"name":"missing","symbol":null}),
        json!({"name":"empty","symbol":""}),
        json!({"name":"existing","existing":true}),
        json!({"name":"fetching","existing":true,"fetching":true}),
        json!({"name":"success"}),
        json!({"name":"success-no-positions","positions":false}),
        json!({"name":"none","outcome":"none"}),
        json!({"name":"reject","outcome":"reject"}),
        json!({"name":"initial-throw","fail":[1]}),
        json!({"name":"position-throw","fail":[2]}),
        json!({"name":"none-clear-throw","outcome":"none","fail":[2]}),
        json!({"name":"none-both-clear-throw","outcome":"none","fail":[2,3]}),
        json!({"name":"reject-clear-throw","outcome":"reject","fail":[2]}),
        json!({"name":"position-and-clear-throw","fail":[2,3]}),
        json!({"name":"concurrent-none","outcome":"none","concurrent":true}),
        json!({"name":"scalar","symbol":"é"}),
        json!({"name":"astral","symbol":"🪙"}),
        json!({"name":"whitespace","symbol":"\u{feff}"}),
        json!({"name":"inherited:constructor","symbol":"constructor"}),
        json!({"name":"inherited:__defineGetter__","symbol":"__defineGetter__"}),
        json!({"name":"inherited:__defineSetter__","symbol":"__defineSetter__"}),
        json!({"name":"inherited:hasOwnProperty","symbol":"hasOwnProperty"}),
        json!({"name":"inherited:__lookupGetter__","symbol":"__lookupGetter__"}),
        json!({"name":"inherited:__lookupSetter__","symbol":"__lookupSetter__"}),
        json!({"name":"inherited:isPrototypeOf","symbol":"isPrototypeOf"}),
        json!({"name":"inherited:propertyIsEnumerable","symbol":"propertyIsEnumerable"}),
        json!({"name":"inherited:toString","symbol":"toString"}),
        json!({"name":"inherited:valueOf","symbol":"valueOf"}),
        json!({"name":"inherited:__proto__","symbol":"__proto__"}),
        json!({"name":"inherited:toLocaleString","symbol":"toLocaleString"}),
        json!({"name":"numeric-key","symbol":"42","initialOther":true}),
    ]
}

fn assert_native_state(store: &dyn Store, expected: &Value, index: usize) {
    let expected_positions = expected.get("leverage").unwrap().as_array().unwrap();
    let positions = store.positions().unwrap();
    assert_eq!(
        positions.len(),
        expected_positions.len(),
        "MARGIN native position count {index}"
    );
    for (position, expected) in positions.iter().zip(expected_positions) {
        assert_eq!(
            position.leverage.to_bits(),
            expected.as_f64().unwrap().to_bits(),
            "MARGIN native position leverage {index}"
        );
    }
    let expected_fetched = expected.get("fetched").unwrap().as_object().unwrap();
    let fetched = store.fetched_leverage().unwrap();
    assert_eq!(
        fetched.keys().collect::<Vec<_>>(),
        expected_fetched.keys().collect::<Vec<_>>(),
        "MARGIN native fetched key order {index}"
    );
    assert_eq!(
        fetched.len(),
        expected_fetched.len(),
        "MARGIN native fetched count {index}"
    );
    for (key, expected) in expected_fetched {
        let actual = fetched.get(key).expect("MARGIN native fetched key");
        assert_eq!(
            actual.buy_leverage.to_bits(),
            expected
                .get("buyLeverage")
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            "MARGIN native buy leverage {index}"
        );
        assert_eq!(
            actual.sell_leverage.to_bits(),
            expected
                .get("sellLeverage")
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            "MARGIN native sell leverage {index}"
        );
        assert_eq!(
            actual.is_fetching,
            expected.get("isFetching").map(|v| v.as_bool().unwrap()),
            "MARGIN native fetching {index}"
        );
    }
}

#[test]
fn margin_fetch_effects_match_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ui_margin_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(capture["boundaries"]["numericCalls"], 1);
    assert_eq!(
        capture["boundaries"]["numericType"], "number",
        "MARGIN source retains non-string symbol"
    );
    assert_eq!(capture["boundaries"]["surrogateCalls"], 1);
    assert!(
        serde_json::from_str::<Value>(capture["boundaries"]["surrogateJson"].as_str().unwrap())
            .is_err(),
        "MARGIN shared UTF-8 representation refusal"
    );
    assert_eq!(
        capture["boundaries"]["falsyCalls"], 1,
        "MARGIN source replaces falsy leverage entry"
    );
    assert!(
        serde_json::from_value::<FetchedLeverage>(Value::Null).is_err(),
        "MARGIN typed leverage refuses null entry"
    );
    assert_eq!(
        capture["boundaries"]["customPrototypeCalls"], 0,
        "MARGIN source custom prototype suppresses fetch"
    );
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 32, "MARGIN source total");
    let specs = expected_specs();
    let expected_calls = [
        0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 1,
    ];
    let expected_notifications = [
        0, 0, 0, 0, 0, 2, 1, 2, 2, 1, 3, 3, 3, 2, 3, 3, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 1,
    ];
    let expected_fetched = [
        0, 0, 0, 1, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 2,
    ];
    let mut comparisons = 0;
    let mut wire_results = Vec::new();
    for index in 0..32 {
        let row = rows.get(index).expect("MARGIN missing fixed source case");
        assert_eq!(
            row.get("spec"),
            Some(&specs[index]),
            "MARGIN source input content {index}"
        );
        for key in [
            "symbol",
            "enabled",
            "calls",
            "notifications",
            "events",
            "state",
            "thrown",
        ] {
            assert!(
                row.get(key).is_some(),
                "MARGIN required source key {key} case {index}"
            );
        }
        let symbol = specs[index].get("symbol").cloned().unwrap_or(json!("BTC"));
        let enabled = specs[index].get("enabled") != Some(&json!(false));
        assert_eq!(row["symbol"], symbol, "MARGIN source symbol {index}");
        assert_eq!(row["enabled"], enabled, "MARGIN source enabled {index}");
        let (venue, _, store) = setup(specs[index].clone(), false);
        let count = Rc::new(Cell::new(0));
        let observed = count.clone();
        let events = venue.events.clone();
        let fail = row["spec"]["fail"].as_array().cloned().unwrap_or_default();
        let unsubscribe = store.subscribe(Box::new(move |data, _| {
            let n = observed.get() + 1;
            observed.set(n);
            events
                .borrow_mut()
                .push(json!({"event":"notify","index":n,"state":view(data)}));
            if fail.contains(&json!(n)) {
                let mut error = ExchangeError::new(format!("notify {n}"));
                error.name = "TypeError".into();
                error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(n)));
                return Err(error.into());
            }
            Ok(())
        }));
        let future = handle_active_margin_symbol_change(&venue, enabled, symbol.as_str());
        let Poll::Ready(result) = pin!(future)
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("MARGIN unexpected pending");
        };
        assert_eq!(
            row["calls"], expected_calls[index],
            "MARGIN source call count {index}"
        );
        assert_eq!(
            row["notifications"], expected_notifications[index],
            "MARGIN source notification count {index}"
        );
        assert_eq!(
            store.positions().unwrap().len(),
            usize::from(index != 6),
            "MARGIN independent position count {index}"
        );
        assert_eq!(
            store.fetched_leverage().unwrap().len(),
            expected_fetched[index],
            "MARGIN independent fetched count {index}"
        );
        assert_native_state(store.as_ref(), &row["state"], index);
        assert_eq!(
            venue.calls.get(),
            row["calls"].as_u64().unwrap() as usize,
            "MARGIN native fetch calls {index}"
        );
        assert_eq!(
            count.get(),
            row["notifications"].as_u64().unwrap() as usize,
            "MARGIN native notifications {index}"
        );
        match (result.as_ref(), row.get("thrown").unwrap()) {
            (Ok(()), Value::Null) => {}
            (Err(error), expected) => {
                assert_eq!(
                    error.name,
                    expected.get("name").unwrap().as_str().unwrap(),
                    "MARGIN native error name {index}"
                );
                assert_eq!(
                    error.message,
                    expected.get("message").unwrap().as_str().unwrap(),
                    "MARGIN native error message {index}"
                );
                let code = error.code.as_ref().expect("MARGIN original code presence");
                let safe_cex_exchange::js_runtime::value::Js::Number(number) = code else {
                    panic!("MARGIN original numeric code type {index}");
                };
                assert_eq!(
                    number.to_bits(),
                    expected["code"].as_f64().unwrap().to_bits(),
                    "MARGIN native error code bits {index}"
                );
                let units = safe_cex_exchange::js_runtime::globals::stringify_json(code)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    String::from_utf16(&units).unwrap(),
                    row["codeJson"].as_str().unwrap(),
                    "MARGIN actual JS stringify code {index}"
                );
                assert_eq!(
                    error.store_subscriber_ids.len(),
                    1,
                    "MARGIN native subscriber identity {index}"
                );
            }
            _ => panic!("MARGIN native error presence {index}"),
        }
        let thrown = result
            .err()
            .map(|error| json!({"name":error.name,"message":error.message,"code":error.code.as_ref().map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())}));
        wire_results.push((
            numeric(
                json!({"calls":venue.calls.get(),"notifications":count.get(),"events":*venue.events.borrow(),"state":json!({"fetched":store.fetched_leverage().unwrap(),"leverage":store.positions().unwrap().iter().map(|p|p.leverage).collect::<Vec<_>>()}),"thrown":thrown})
            ),
            numeric(
                json!({"calls":row["calls"],"notifications":row["notifications"],"events":row["events"],"state":row["state"],"thrown":row["thrown"]})
            ),
        ));
        comparisons += 1;
        unsubscribe();
    }
    assert_eq!(comparisons, 32, "MARGIN native comparisons");
    assert_eq!(wire_results.len(), 32, "MARGIN wire comparison count");
    for (index, (actual, expected)) in wire_results.into_iter().enumerate() {
        assert_eq!(actual, expected, "MARGIN effects {index}");
    }
}
#[test]
fn margin_cleanup_restores_native_numeric_key_order() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ui_margin_key_order_oracle.mjs"
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
    let (venue, _, store) = setup(
        json!({"initialOther":true,"positions":false,"outcome":"none","concurrentNumeric":true}),
        false,
    );
    let publications = Rc::new(RefCell::new(Vec::new()));
    let observed = publications.clone();
    let unsubscribe = store.subscribe(Box::new(move |data, _| {
        observed
            .borrow_mut()
            .push(data.fetched_leverage.keys().cloned().collect::<Vec<_>>());
        Ok(())
    }));
    let future = handle_active_margin_symbol_change(&venue, true, Some("BTC"));
    let Poll::Ready(result) = pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    else {
        panic!("MARGIN cleanup unexpected pending");
    };
    result.unwrap();
    // Independent expectations guard native values before consulting source/wire projections.
    assert_eq!(venue.calls.get(), 1, "MARGIN cleanup native fetch count");
    assert_eq!(
        store.positions().unwrap().len(),
        0,
        "MARGIN cleanup native position count"
    );
    assert_eq!(
        store.fetched_leverage().unwrap().len(),
        2,
        "MARGIN cleanup native fetched count"
    );
    assert_eq!(
        store
            .fetched_leverage()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["42", "OTHER"],
        "MARGIN cleanup native fetched key order"
    );
    for (key, buy, sell) in [("42", 21.0_f64, 23.0_f64), ("OTHER", 7.0_f64, 8.0_f64)] {
        let fetched = store.fetched_leverage().unwrap();
        let actual = &fetched[key];
        assert_eq!(
            actual.buy_leverage.to_bits(),
            buy.to_bits(),
            "MARGIN cleanup native buy {key}"
        );
        assert_eq!(
            actual.sell_leverage.to_bits(),
            sell.to_bits(),
            "MARGIN cleanup native sell {key}"
        );
        assert_eq!(
            actual.is_fetching, None,
            "MARGIN cleanup native marker {key}"
        );
    }
    let publications = publications.borrow();
    assert_eq!(
        publications.len(),
        3,
        "MARGIN cleanup native publication count"
    );
    assert_eq!(
        publications[0],
        ["OTHER", "BTC"],
        "MARGIN cleanup native placeholder order"
    );
    assert_eq!(
        publications[1],
        ["OTHER", "BTC", "42"],
        "MARGIN cleanup native unsorted host precondition"
    );
    assert_eq!(
        publications[2],
        ["42", "OTHER"],
        "MARGIN cleanup native publication order"
    );
    assert_eq!(source["calls"], 1, "MARGIN cleanup source calls");
    assert_eq!(
        source["publications"].as_array().unwrap().len(),
        3,
        "MARGIN cleanup source publications"
    );
    assert_eq!(
        source["publications"][2],
        json!(["42", "OTHER"]),
        "MARGIN cleanup source key order"
    );
    unsubscribe();
}

#[test]
fn dropped_margin_fetch_retains_unowned_store_state_and_reports_cancellation() {
    let (venue, runtime, store) = setup(json!({}), true);
    let unpolled = handle_active_margin_symbol_change(&venue, true, Some("BTC"));
    drop(unpolled);
    assert!(store.fetched_leverage().unwrap().is_empty());
    assert!(runtime.exceptions.borrow().is_empty());
    let mut future = Box::pin(handle_active_margin_symbol_change(
        &venue,
        true,
        Some("BTC"),
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(
        store.fetched_leverage().unwrap()["BTC"].is_fetching,
        Some(true)
    );
    let mut fetched = store.fetched_leverage().unwrap().clone();
    fetched.insert("OTHER".into(), leverage(7.0, 8.0));
    store
        .update(
            StoreUpdate {
                fetched_leverage: Some(fetched),
                ..Default::default()
            },
            true,
        )
        .unwrap();
    drop(future);
    assert!(
        store.fetched_leverage().unwrap().contains_key("BTC"),
        "MARGIN cancellation retains marker without request identity"
    );
    assert_eq!(
        store.fetched_leverage().unwrap()["OTHER"].buy_leverage,
        7.0,
        "MARGIN cancellation retains concurrent symbol"
    );
    let errors = runtime.exceptions.borrow();
    assert_eq!(errors.len(), 1, "MARGIN cancellation visible once");
    assert_eq!(
        errors[0].0.message,
        "Worker leverage fetch cancelled; current Store marker retained"
    );
}

#[test]
fn dropped_margin_fetch_keeps_a_published_result() {
    let (venue, runtime, store) = setup(json!({}), true);
    let mut future = Box::pin(handle_active_margin_symbol_change(
        &venue,
        true,
        Some("BTC"),
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    let mut fetched = store.fetched_leverage().unwrap().clone();
    fetched.insert("BTC".into(), leverage(17.0, 19.0));
    store
        .update(
            StoreUpdate {
                fetched_leverage: Some(fetched),
                ..Default::default()
            },
            true,
        )
        .unwrap();
    drop(future);
    assert_eq!(
        store.fetched_leverage().unwrap()["BTC"].buy_leverage,
        17.0,
        "MARGIN cancellation preserves published result"
    );
    assert_eq!(runtime.exceptions.borrow().len(), 1);
}

#[test]
fn dropping_old_margin_fetch_does_not_clear_restarted_fetch_marker() {
    let (venue, runtime, store) = setup(json!({}), true);
    let mut old = Box::pin(handle_active_margin_symbol_change(
        &venue,
        true,
        Some("BTC"),
    ));
    assert!(
        old.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    store
        .update(
            StoreUpdate {
                fetched_leverage: Some(Default::default()),
                ..Default::default()
            },
            true,
        )
        .unwrap();
    let mut new = Box::pin(handle_active_margin_symbol_change(
        &venue,
        true,
        Some("BTC"),
    ));
    assert!(
        new.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(venue.calls.get(), 2, "MARGIN restart performs second fetch");
    drop(old);
    assert_eq!(
        store
            .fetched_leverage()
            .unwrap()
            .get("BTC")
            .map(|v| v.is_fetching),
        Some(Some(true)),
        "MARGIN old cancellation preserves restarted marker"
    );
    let mut third = Box::pin(handle_active_margin_symbol_change(
        &venue,
        true,
        Some("BTC"),
    ));
    assert!(matches!(
        third.as_mut().poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(()))
    ));
    assert_eq!(
        venue.calls.get(),
        2,
        "MARGIN restarted marker prevents third fetch"
    );
    assert_eq!(
        runtime.exceptions.borrow().len(),
        1,
        "MARGIN old cancellation reported once"
    );
    drop(new);
    assert_eq!(
        runtime.exceptions.borrow().len(),
        2,
        "MARGIN each cancelled future reported"
    );
}
