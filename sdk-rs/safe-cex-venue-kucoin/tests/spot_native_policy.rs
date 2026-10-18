// Reuse finite language operations without constructing a socket in this target.
#[allow(dead_code)]
#[path = "support/private_class_runtime.rs"]
mod runtime;
#[path = "support/spot_private_values.rs"]
mod values;
use runtime::Runtime;
use safe_cex_venue_kucoin::{futures_public_class::ConstructionOps, spot_native_policy as port};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
use values::*;
fn at(value: &V, key: &str) -> V {
    get(value, key).unwrap()
}
fn keys(value: &V) -> Vec<String> {
    let Js::O(fields) = value.as_ref() else {
        panic!("native object")
    };
    fields.borrow().iter().map(|(key, _)| key.clone()).collect()
}
fn rows(value: &V) -> Vec<V> {
    let Js::A(rows) = value.as_ref() else {
        panic!("native array")
    };
    rows.borrow().clone()
}
fn assert_n(value: &V, want: f64) {
    let Js::N(actual) = value.as_ref() else {
        panic!("KUCOIN-POLICY native number kind")
    };
    assert_eq!(
        actual.to_bits(),
        want.to_bits(),
        "KUCOIN-POLICY native number bits"
    );
}
fn assert_s(value: &V, want: &str) {
    let Js::S(actual) = value.as_ref() else {
        panic!("KUCOIN-POLICY native string kind")
    };
    assert_eq!(actual, want, "KUCOIN-POLICY native string value");
}
fn same(a: &V, b: &V) {
    assert!(
        Rc::ptr_eq(a, b),
        "KUCOIN-POLICY original reference identity"
    );
}
fn position(value: &V, id: &str, symbol: &str) {
    assert_eq!(
        keys(value),
        [
            "id",
            "symbol",
            "side",
            "entryPrice",
            "notional",
            "leverage",
            "unrealizedPnl",
            "contracts",
            "liquidationPrice",
            "marginType",
            "positionMode",
            "info"
        ],
        "KUCOIN-POLICY complete position keys"
    );
    assert_s(&at(value, "id"), id);
    assert_s(&at(value, "symbol"), symbol);
    assert_s(&at(value, "side"), "long");
    assert_s(&at(value, "marginType"), "isolated");
    assert_s(&at(value, "positionMode"), "oneway");
    for key in [
        "entryPrice",
        "notional",
        "unrealizedPnl",
        "contracts",
        "liquidationPrice",
    ] {
        assert_n(&at(value, key), 0.);
    }
    assert_n(&at(value, "leverage"), 1.);
    let info = at(value, "info");
    assert_eq!(keys(&info), ["spot"]);
    assert!(
        matches!(at(&info, "spot").as_ref(), Js::B(true)),
        "KUCOIN-POLICY native spot bool"
    );
}
struct Outcome {
    value: V,
    reads: Rc<Cell<usize>>,
    sets: Rc<Cell<usize>>,
    trace: Rc<RefCell<Vec<String>>>,
    extra: Vec<(&'static str, V)>,
}
impl Outcome {
    fn project(&self) -> Value {
        let mut out = json!({"value":literal(&self.value),"reads":self.reads.get(),"setCalls":self.sets.get(),"trace":*self.trace.borrow()});
        for (key, value) in &self.extra {
            out[*key] = literal(value);
        }
        out
    }
}
fn run(kind: &str) -> Outcome {
    let rt = Rc::new(Runtime::default());
    let ops = rt.operations();
    let receiver = obj(vec![]);
    let store = obj(vec![]);
    set(&receiver, "store", store.clone());
    rt.globals
        .borrow_mut()
        .insert("PositionSide".into(), obj(vec![("Long", s("long"))]));
    rt.globals
        .borrow_mut()
        .insert("MarginType".into(), obj(vec![("Isolated", s("isolated"))]));
    rt.globals
        .borrow_mut()
        .insert("PositionMode".into(), obj(vec![("OneWay", s("oneway"))]));
    let reads = Rc::new(Cell::new(0));
    let sets = Rc::new(Cell::new(0));
    let trace = Rc::new(RefCell::new(Vec::<String>::new()));
    let mut extra = vec![];
    let value;
    if let Some(currency) = kind.strip_prefix("price-") {
        let raw = obj(vec![("marker", n(71.))]);
        let table = match currency {
            "BTC" => obj(vec![
                ("BTC-USDT", obj(vec![("last", n(11.))])),
                ("BTC-USDC", obj(vec![("last", n(97.))])),
            ]),
            "ETH" => obj(vec![("ETH-USDC", obj(vec![("last", n(13.))]))]),
            "SOL" => obj(vec![("SOL-USD", obj(vec![("last", n(17.))]))]),
            "ZERO" => obj(vec![
                ("ZERO-USDT", obj(vec![("last", n(0.))])),
                ("ZERO-USDC", obj(vec![("last", n(19.))])),
            ]),
            "RAW" => obj(vec![("RAW-USDT", obj(vec![("last", raw.clone())]))]),
            _ => obj(vec![]),
        };
        let count = reads.clone();
        let live = currency == "LIVE";
        rt.getters.borrow_mut().push((
            store.clone(),
            "tickersBySymbol".into(),
            Rc::new(move || {
                count.set(count.get() + 1);
                Ok(if live {
                    if count.get() == 1 {
                        obj(vec![])
                    } else {
                        obj(vec![("LIVE-USDC", obj(vec![("last", n(23.))]))])
                    }
                } else {
                    table.clone()
                })
            }),
        ));
        value = port::get_usd_price(&ops, &rt.frame(&receiver), s(currency)).unwrap();
        if currency == "RAW" {
            same(&value, &raw);
            assert_n(&at(&value, "marker"), 71.);
            set(&raw, "marker", n(101.));
            assert_n(&at(&value, "marker"), 101.);
        } else {
            assert_n(
                &value,
                match currency {
                    "USD" | "USDT" | "USDC" => 1.,
                    "BTC" => 11.,
                    "ETH" => 13.,
                    "SOL" => 17.,
                    "LIVE" => 23.,
                    _ => 0.,
                },
            );
        }
        assert_eq!(
            reads.get(),
            match currency {
                "USD" | "USDT" | "USDC" => 0,
                "BTC" | "ZERO" | "RAW" => 1,
                "ETH" | "LIVE" => 2,
                _ => 3,
            },
            "KUCOIN-POLICY live ticker dictionary read count"
        );
    } else if kind.starts_with("positions-") {
        let market = obj(vec![("symbol", s("A")), ("active", b(true))]);
        let markets = if kind == "positions-live-symbol" {
            let row = obj(vec![("active", b(true))]);
            let count = reads.clone();
            rt.getters.borrow_mut().push((
                row.clone(),
                "symbol".into(),
                Rc::new(move || {
                    count.set(count.get() + 1);
                    Ok(s(if count.get() == 1 { "ID1" } else { "VALUE2" }))
                }),
            ));
            arr(vec![row])
        } else {
            arr(vec![
                market.clone(),
                obj(vec![("symbol", s("B")), ("active", b(false))]),
                obj(vec![("symbol", s("C")), ("active", b(true))]),
                market,
                obj(vec![("symbol", s("D")), ("active", n(0.))]),
            ])
        };
        set(&store, "markets", markets.clone());
        let predicates = Rc::new(RefCell::new(Vec::new()));
        let results = predicates.clone();
        let filter = at(&markets, "filter");
        rt.getters.borrow_mut().push((
            markets.clone(),
            "filter".into(),
            Rc::new(move || {
                let collected = results.clone();
                let original = filter.clone();
                Ok(function(move |receiver, args| {
                    let callback = args[0].clone();
                    let results = collected.clone();
                    let wrapper = function(move |this, args| {
                        let result = call(callback.clone(), this, args)?;
                        results.borrow_mut().push(result.clone());
                        Ok(result)
                    });
                    call(original.clone(), receiver, vec![wrapper])
                }))
            }),
        ));
        let count = sets.clone();
        let ctor = function(move |_, args| {
            count.set(count.get() + 1);
            let values = rows(&args[0]);
            Ok(obj(vec![(
                "has",
                function(move |_, args| {
                    Ok(b(values.iter().any(|value| {
                        match (value.as_ref(), args[0].as_ref()) {
                            (Js::S(a), Js::S(b)) => a == b,
                            (Js::N(a), Js::N(b)) => a == b || (a.is_nan() && b.is_nan()),
                            _ => Rc::ptr_eq(value, &args[0]),
                        }
                    })))
                }),
            )]))
        });
        rt.globals.borrow_mut().insert("Set".into(), ctor.clone());
        let construction = ConstructionOps {
            derive: Rc::new(|_, _, _, _| panic!("no class derivation")),
            superclass: Rc::new(|_| panic!("no superclass")),
            super_construct: Rc::new(|_, _, _| panic!("no super call")),
            construct: Rc::new(move |actual, args| {
                assert!(
                    Rc::ptr_eq(&actual, &ctor),
                    "KUCOIN-POLICY global constructor identity"
                );
                call(actual, obj(vec![]), args)
            }),
        };
        let symbols = match kind {
            "positions-empty" => arr(vec![]),
            "positions-C" => arr(vec![s("C")]),
            "positions-A-duplicates" => arr(vec![s("A"), s("A")]),
            _ => u(),
        };
        value =
            port::fetch_positions(ops.clone(), &construction, receiver.clone(), symbols).unwrap();
        let positions = rows(&value);
        let expected: &[(&str, &str)] = match kind {
            "positions-empty" => &[],
            "positions-C" => &[("C:long", "C")],
            "positions-A-duplicates" => &[("A:long", "A"), ("A:long", "A")],
            "positions-live-symbol" => &[("ID1:long", "VALUE2")],
            _ => &[("A:long", "A"), ("C:long", "C"), ("A:long", "A")],
        };
        assert_eq!(
            positions.len(),
            expected.len(),
            "KUCOIN-POLICY independent position count"
        );
        for (row, (id, symbol)) in positions.iter().zip(expected) {
            position(row, id, symbol);
        }
        if ["positions-all", "positions-A-duplicates"].contains(&kind) {
            let last = positions.last().unwrap();
            assert!(
                !Rc::ptr_eq(&positions[0], last),
                "KUCOIN-POLICY fresh duplicate positions"
            );
            assert!(
                !Rc::ptr_eq(&at(&positions[0], "info"), &at(last, "info")),
                "KUCOIN-POLICY fresh duplicate info"
            );
        }
        assert_eq!(
            sets.get(),
            usize::from(
                ["positions-empty", "positions-C", "positions-A-duplicates"].contains(&kind)
            ),
            "KUCOIN-POLICY Set construction count"
        );
        assert_eq!(
            reads.get(),
            if kind == "positions-live-symbol" {
                2
            } else {
                0
            },
            "KUCOIN-POLICY distinct id/symbol live reads"
        );
        let results = predicates.borrow();
        assert_eq!(
            results.len(),
            if kind == "positions-live-symbol" {
                1
            } else {
                5
            },
            "KUCOIN-POLICY filter call count"
        );
        if kind != "positions-live-symbol" {
            assert_n(&results[4], 0.);
            for (index, want) in [0, 1, 2, 3].into_iter().zip(match kind {
                "positions-empty" => [false, false, false, false],
                "positions-C" => [false, false, true, false],
                "positions-A-duplicates" => [true, false, false, true],
                _ => [true, false, true, true],
            }) {
                assert!(
                    matches!(results[index].as_ref(),Js::B(actual)if *actual==want),
                    "KUCOIN-POLICY native predicate result"
                );
            }
        }
        assert_eq!(
            rt.metas
                .borrow()
                .iter()
                .map(|(_, name, len, asynchronous)| (name.as_str(), *len, *asynchronous))
                .collect::<Vec<_>>(),
            [("", 1, false), ("", 1, false)],
            "KUCOIN-POLICY two callback metadata entries"
        );
        extra.push(("predicates", arr(results.clone())));
    } else if kind.starts_with("leverage-") {
        let old = obj(vec![("buyLeverage", n(11.)), ("sellLeverage", n(13.))]);
        let original = obj(vec![
            ("old", old.clone()),
            (
                "BTC-USDT",
                obj(vec![("buyLeverage", n(17.)), ("sellLeverage", n(19.))]),
            ),
        ]);
        let updates = Rc::new(RefCell::new(Vec::new()));
        let actual = updates.clone();
        let captured = trace.clone();
        let expected_receiver = store.clone();
        let error: E = Rc::new("subscriber".into());
        let thrown = error.clone();
        let fail = kind == "leverage-error";
        let update = Rc::new(RefCell::new(function(move |receiver, args| {
            same(&receiver, &expected_receiver);
            captured.borrow_mut().push("call-original".into());
            actual.borrow_mut().push(args[0].clone());
            if fail { Err(thrown.clone()) } else { Ok(u()) }
        })));
        let current = update.clone();
        let captured = trace.clone();
        rt.getters.borrow_mut().push((
            store.clone(),
            "update".into(),
            Rc::new(move || {
                captured.borrow_mut().push("get-update".into());
                Ok(current.borrow().clone())
            }),
        ));
        let current = update;
        let captured = trace.clone();
        let fetched = original.clone();
        let replace = kind == "leverage-live-update";
        rt.getters.borrow_mut().push((
            store.clone(),
            "fetchedLeverage".into(),
            Rc::new(move || {
                captured.borrow_mut().push("get-fetched".into());
                if replace {
                    *current.borrow_mut() =
                        function(|_, _| panic!("late replacement must not run"));
                }
                Ok(fetched.clone())
            }),
        ));
        let result = port::set_leverage(
            &rt.frame(&receiver),
            s(if kind == "leverage-proto" {
                "__proto__"
            } else {
                "BTC-USDT"
            }),
        );
        value = if fail {
            let Err(actual) = result else {
                panic!("KUCOIN-POLICY subscriber failure must propagate")
            };
            assert!(
                Rc::ptr_eq(&actual, &error),
                "KUCOIN-POLICY original subscriber error identity"
            );
            s("subscriber")
        } else {
            let Ok(value) = result else {
                panic!("unexpected error")
            };
            assert!(matches!(value.as_ref(), Js::U));
            value
        };
        assert_eq!(
            *trace.borrow(),
            ["get-update", "get-fetched", "call-original"],
            "KUCOIN-POLICY captured update before live dictionary getter"
        );
        let updates = updates.borrow();
        assert_eq!(updates.len(), 1, "KUCOIN-POLICY one update");
        assert_eq!(keys(&updates[0]), ["fetchedLeverage"]);
        let dictionary = at(&updates[0], "fetchedLeverage");
        assert!(
            !Rc::ptr_eq(&dictionary, &original),
            "KUCOIN-POLICY fresh leverage dictionary"
        );
        same(&at(&dictionary, "old"), &old);
        assert_n(&at(&at(&dictionary, "old"), "buyLeverage"), 11.);
        assert_n(&at(&at(&dictionary, "old"), "sellLeverage"), 13.);
        let selected = if kind == "leverage-proto" {
            "__proto__"
        } else {
            "BTC-USDT"
        };
        assert_eq!(
            keys(&dictionary),
            if kind == "leverage-proto" {
                vec!["old", "BTC-USDT", "__proto__"]
            } else {
                vec!["old", "BTC-USDT"]
            },
            "KUCOIN-POLICY own computed dictionary key"
        );
        let item = at(&dictionary, selected);
        assert_eq!(keys(&item), ["buyLeverage", "sellLeverage"]);
        assert_n(&at(&item, "buyLeverage"), 1.);
        assert_n(&at(&item, "sellLeverage"), 1.);
        assert_n(&at(&at(&original, "BTC-USDT"), "buyLeverage"), 17.);
        assert_n(&at(&at(&original, "BTC-USDT"), "sellLeverage"), 19.);
        extra.push(("updates", arr(updates.clone())));
        extra.push(("old", original));
    } else {
        assert_eq!(kind, "ignored-arguments");
        let h = rt.frame(&receiver);
        let max = port::fetch_symbol_max_leverage(&h);
        assert_n(&max, 1.);
        let margin = port::change_margin_type(&h);
        let position = port::change_position_mode(&h);
        assert!(matches!(margin.as_ref(), Js::U));
        assert!(matches!(position.as_ref(), Js::U));
        value = arr(vec![max, margin, position]);
        assert!(trace.borrow().is_empty());
    }
    Outcome {
        value,
        reads,
        sets,
        trace,
        extra,
    }
}
fn typed_leverage_snapshot_native_assertions() -> impl FnOnce() {
    use safe_cex_exchange::native_store::DefaultStore;
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_types::FetchedLeverage;
    let store = DefaultStore::new();
    let record = |buy, sell| FetchedLeverage {
        buy_leverage: buy,
        sell_leverage: sell,
        margin_type: None,
        position_mode: None,
        is_fetching: None,
    };
    let initial = indexmap::IndexMap::from([
        ("old".into(), record(11., 13.)),
        ("BTC-USDT".into(), record(17., 19.)),
    ]);
    Store::update(
        &store,
        StoreUpdate {
            fetched_leverage: Some(initial),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let mut prior = store.fetched_leverage().unwrap().clone();
    safe_cex_venue_kucoin::spot_helpers::set_leverage(&store, "BTC-USDT", 97., None).unwrap();
    assert_eq!(
        store.fetched_leverage().unwrap().len(),
        2,
        "KUCOIN-POLICY typed retained dictionary count"
    );
    assert_eq!(
        store
            .fetched_leverage()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["old", "BTC-USDT"]
    );
    prior.get_mut("old").unwrap().buy_leverage = 77.;
    assert_eq!(
        prior["old"].buy_leverage.to_bits(),
        77f64.to_bits(),
        "KUCOIN-POLICY typed prior value mutation"
    );
    let current = store.fetched_leverage().unwrap();
    assert_eq!(
        current["old"].buy_leverage.to_bits(),
        11f64.to_bits(),
        "KUCOIN-POLICY typed untouched record snapshot"
    );
    assert_eq!(current["old"].sell_leverage.to_bits(), 13f64.to_bits());
    assert_eq!(current["BTC-USDT"].buy_leverage.to_bits(), 1f64.to_bits());
    assert_eq!(current["BTC-USDT"].sell_leverage.to_bits(), 1f64.to_bits());
    move || {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-native-policy.cjs"
            ))
            .arg("typed-leverage-alias")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual Source Store alias witness: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let source: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            source,
            json!({"priorBuy":77,"currentBuy":77,"currentSell":13,"selectedBuy":1,"selectedSell":1,"shared":true}),
            "KUCOIN-POLICY source retained record alias"
        );
    }
}

fn fixture_prototype_operations_match_native_property_kinds() -> impl FnOnce() {
    let rt = Rc::new(Runtime::default());
    let h = rt.frame(&u());
    let values = obj(vec![("buyLeverage", n(1.)), ("sellLeverage", n(1.))]);
    let dictionary = obj(vec![("old", n(11.))]);
    (h.intrinsic)(
        "set-index",
        vec![dictionary.clone(), s("__proto__"), values.clone()],
    )
    .unwrap();
    assert_eq!(
        keys(&dictionary),
        ["old"],
        "KUCOIN-POLICY assignment invokes inherited prototype setter"
    );
    same(&rt.read(&dictionary, "__proto__").unwrap(), &values);
    assert_n(&rt.read(&dictionary, "buyLeverage").unwrap(), 1.);
    let null_prototype = obj(vec![]);
    rt.write(&null_prototype, "__proto__", Rc::new(Js::Null))
        .unwrap();
    rt.write(&null_prototype, "__proto__", values.clone())
        .unwrap();
    assert_eq!(
        keys(&null_prototype),
        ["__proto__"],
        "KUCOIN-POLICY absent inherited setter creates own property"
    );
    same(&rt.read(&null_prototype, "__proto__").unwrap(), &values);
    let own = obj(vec![]);
    (h.intrinsic)(
        "define-field",
        vec![own.clone(), s("__proto__"), values.clone()],
    )
    .unwrap();
    assert_eq!(
        keys(&own),
        ["__proto__"],
        "KUCOIN-POLICY definition creates own property"
    );
    same(&rt.read(&own, "__proto__").unwrap(), &values);
    move || {
        let output = Command::new("node").args(["-e", "const a=require('node:assert/strict'),v={buyLeverage:1,sellLeverage:1},d={old:11};d.__proto__=v;a.deepEqual(Object.keys(d),['old']);a.equal(d.__proto__,v);a.equal(d.buyLeverage,1);const n={};n.__proto__=null;n.__proto__=v;a.deepEqual(Object.keys(n),['__proto__']);a.equal(n.__proto__,v);const o={['__proto__']:v};a.deepEqual(Object.keys(o),['__proto__']);a.equal(o.__proto__,v);"]).output().unwrap();
        assert!(
            output.status.success(),
            "source property-kind assertions: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn native_spot_policy_values_precede_source_projection() {
    let cases = [
        "price-USD",
        "price-USDT",
        "price-USDC",
        "price-BTC",
        "price-ETH",
        "price-SOL",
        "price-ZERO",
        "price-RAW",
        "price-MISS",
        "price-LIVE",
        "positions-all",
        "positions-empty",
        "positions-C",
        "positions-A-duplicates",
        "positions-live-symbol",
        "leverage-BTC",
        "leverage-proto",
        "leverage-live-update",
        "leverage-error",
        "ignored-arguments",
    ];
    assert_eq!(cases.len(), 20);
    let outcomes = cases.map(run);
    let fixture_source = fixture_prototype_operations_match_native_property_kinds();
    let typed_source = typed_leverage_snapshot_native_assertions();
    fixture_source();
    typed_source();
    for (index, native) in outcomes.iter().enumerate() {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-native-policy.cjs"
            ))
            .arg(cases[index])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source case {}: {}",
            cases[index],
            String::from_utf8_lossy(&output.stderr)
        );
        let source: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            native.project(),
            source,
            "source case {index}/{}",
            cases[index]
        );
    }
}
