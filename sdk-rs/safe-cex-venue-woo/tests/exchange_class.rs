#[path = "support/lifecycle.rs"]
mod fixture;
use fixture::*;
use safe_cex_venue_woo::{class_construction::ConstructionOps, exchange_class::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
type Meta = (V, String, usize, bool);
const FIELDS: [&str; 86] = [
    "clientOrderIdRule",
    "fetchHistoricalPositions",
    "xhr",
    "_primaryXhr",
    "_secondaryXhr",
    "_proxyEnforcedXhr",
    "publicWebsocket",
    "privateWebsocket",
    "grafanaEndpointsEvery",
    "grafanaEndpointsPeriodic",
    "_accountPositionMode",
    "_fillPositionRefreshTimers",
    "_fillPositionRefreshInFlight",
    "_fillExpectedPositionSignatureBySymbol",
    "_fundingRefreshTimer",
    "_fundingRefreshInFlight",
    "_positionMetadataCache",
    "_positionMetadataInFlight",
    "_positionMetadataRateLimitedUntil",
    "dispose",
    "startFundingRatesRefresh",
    "parseFiniteFundingNumber",
    "getWooResponseMessage",
    "fetchAccountInfo",
    "refreshFundingRates",
    "afterMethodCalledInWorker",
    "normalizeTimestampSeconds",
    "normalizeTimestampMilliseconds",
    "normalizeTransactionSymbol",
    "normalizeRequestedSymbol",
    "getTransactionHistorySymbolCandidates",
    "parseTransactionHistoryRow",
    "parseTransactionRowsFromResponse",
    "fetchLegacyTransactionHistoryRows",
    "fetchTransactionHistoryRows",
    "getPositionSignatureForSymbol",
    "scheduleFillPositionRefresh",
    "refreshPositionForSymbolFromFill",
    "applyOptimisticOneWayPositionFromFill",
    "handleFillForPositionSync",
    "getAccount",
    "validateAccount",
    "start",
    "tick",
    "fetchBalance",
    "fetchMarkets",
    "fetchTickers",
    "fetchTickersRaw",
    "parseTickers",
    "fetchPositionsAndBalance",
    "createPositionMetadataCacheKey",
    "buildPositionMetadataFromLeverage",
    "fetchPositionMetadataForSymbol",
    "updateCachedPositionMetadataLeverage",
    "fetchMarketIocTouchPrice",
    "confirmPositionModeAndRefreshPositions",
    "fetchPositionMetadata",
    "fetchPositions",
    "fetchOpenPositions",
    "fetchOrders",
    "fetchHistoricalFills",
    "fetchHistoricalTradesInternal",
    "fetchOHLCV",
    "listenOHLCV",
    "listenOrderBook",
    "updateOrder",
    "updateAlgoOrder",
    "fetchHistoricalOrders",
    "fetchOrdersRecursively",
    "fetchLimitOrders",
    "mapLimitOrder",
    "fetchAlgoOrders",
    "mapAlgoOrder",
    "cancelOrders",
    "cancelAlgoOrder",
    "getPositionSideForOrder",
    "placeOrder",
    "placeAlgoOrder",
    "placeStopMarket",
    "placeTrailingStopLossOrder",
    "placePositionalAlgoOrder",
    "cancelAllOrders",
    "cancelSymbolOrders",
    "setLeverage",
    "changePositionMode",
    "isAlgoOrder",
];
const COLLECTIONS: [(&str, &str); 6] = [
    ("_fillPositionRefreshTimers", "Map"),
    ("_fillPositionRefreshInFlight", "Set"),
    ("_fillExpectedPositionSignatureBySymbol", "Map"),
    ("_positionMetadataCache", "Map"),
    ("_positionMetadataInFlight", "Map"),
    ("_positionMetadataRateLimitedUntil", "Map"),
];
fn metadata(
    metas: &Rc<RefCell<Vec<Meta>>>,
    name: &str,
    arity: usize,
    asynchronous: bool,
    f: V,
) -> V {
    metas
        .borrow_mut()
        .push((f.clone(), name.into(), arity, asynchronous));
    f
}
#[test]
fn native_partial_class_layout_and_client_rule() {
    let runtime = Rc::new(Runtime::default());
    let mut life = runtime.operations();
    let metas = Rc::new(RefCell::new(Vec::<Meta>::new()));
    let random = Rc::new(Cell::new(0.));
    let conversions = Rc::new(Cell::new(0));
    let convert_count = conversions.clone();
    let number_fn = function(move |_, a| {
        convert_count.set(convert_count.get() + 1);
        Ok(n(safe_cex_num::number_from_string(&text(&a[0]))))
    });
    let safe_fn = function(|_, a| {
        let v = number(&a[0]);
        Ok(b(v.is_finite()
            && v.fract() == 0.
            && v.abs() <= 9_007_199_254_740_991.))
    });
    let number_handle = number_fn.clone();
    let safe_handle = safe_fn.clone();
    let l = Rc::get_mut(&mut life).unwrap();
    let v = Rc::get_mut(&mut l.value).unwrap();
    let old_get = v.get.clone();
    v.get = Rc::new(move |r, k| {
        if Rc::ptr_eq(r, &number_handle) && k == "isSafeInteger" {
            Ok(safe_handle.clone())
        } else {
            old_get(r, k)
        }
    });
    let draw = random.clone();
    let math = obj(vec![
        ("random", function(move |_, _| Ok(n(draw.get())))),
        ("floor", function(|_, a| Ok(n(number(&a[0]).floor())))),
    ]);
    v.global = Rc::new(move |name| {
        Ok(match name {
            "String" => function(|_, a| Ok(s(&text(&a[0])))),
            "Number" => number_fn.clone(),
            "Math" => math.clone(),
            "Map" => s("Map"),
            "Set" => s("Set"),
            _ => panic!("uncovered global {name}"),
        })
    });
    let meta = metas.clone();
    v.function = Rc::new(move |name, arity, f| {
        metadata(&meta, name, arity, false, function(move |_, a| f(a)))
    });
    let init = Rc::new(RefCell::new(None::<Initialize<V, E>>));
    let constructor = Rc::new(RefCell::new(None::<Constructor<V, E>>));
    let objects = Rc::new(RefCell::new(Vec::<V>::new()));
    let active = obj(vec![]);
    let target = obj(vec![]);
    let original = [s("woo"), obj(vec![]), obj(vec![])];
    let initial = init.clone();
    let instances = objects.clone();
    let original_args = original.clone();
    let a = active.clone();
    let t = target.clone();
    let sentinel: E = Rc::new("factory sentinel".into());
    let fail = sentinel.clone();
    let superclass = obj(vec![]);
    runtime
        .imports
        .borrow_mut()
        .insert("BaseExchange".into(), superclass.clone());
    runtime
        .imports
        .borrow_mut()
        .insert("GRAFANA_ENDPOINTS_EVERY".into(), s("every"));
    runtime
        .imports
        .borrow_mut()
        .insert("GRAFANA_ENDPOINTS_PERIODIC".into(), s("periodic"));
    runtime
        .imports
        .borrow_mut()
        .insert("createAPI".into(), obj(vec![]));
    let inspected = Rc::new(Cell::new(0));
    let count = inspected.clone();
    runtime.imports.borrow_mut().insert(
        "createApiWithSecondaryKey".into(),
        function(move |_, args| {
            assert_eq!(args.len(), 4);
            assert!(
                matches!(get(&args[1], "isAlgoOrder")?.as_ref(), Js::F(_)),
                "WOO-EXCHANGE native fields before constructor factory"
            );
            count.set(count.get() + 1);
            Err(fail.clone())
        }),
    );
    let construction = Rc::new(ConstructionOps {
        value: life,
        super_construct: Rc::new(move |current, new_target, args| {
            assert!(Rc::ptr_eq(&current, &a));
            assert!(Rc::ptr_eq(&new_target, &t));
            assert_eq!(args.len(), 3);
            for (i, arg) in args.iter().enumerate() {
                assert!(Rc::ptr_eq(arg, &original_args[i]));
            }
            let receiver = obj(vec![]);
            initial.borrow().as_ref().unwrap()(receiver.clone())?;
            instances.borrow_mut().push(receiver.clone());
            Ok(receiver)
        }),
        construct: Rc::new(|kind, args| {
            assert!(args.is_empty());
            Ok(obj(vec![("kind", kind)]))
        }),
        set: Rc::new(|r, k, v| {
            set(r, k, v);
            Ok(())
        }),
    });
    let ctor_slot = constructor.clone();
    let init_slot = init.clone();
    let meta = metas.clone();
    let proto = metas.clone();
    let ops = Rc::new(ExchangeClassOps {
        construction,
        derive: Rc::new(move |name, base, arity, ctor, initialize| {
            assert!(
                Rc::ptr_eq(&base, &superclass),
                "WOO-EXCHANGE native superclass identity"
            );
            assert_eq!(name, "WOOXExchange");
            assert_eq!(arity, 3);
            *ctor_slot.borrow_mut() = Some(ctor);
            *init_slot.borrow_mut() = Some(initialize);
            Ok(obj(vec![]))
        }),
        define: Rc::new(|r, k, v| {
            set(r, k, v);
            Ok(())
        }),
        unbound_member: Rc::new(move |_, name, arity, asynchronous| {
            let name_copy = name.to_owned();
            Ok(metadata(
                &meta,
                name,
                arity,
                asynchronous,
                function(move |_, _| Err(Rc::new(format!("unbound venue body {name_copy}")))),
            ))
        }),
        unbound_prototype: Rc::new(move |class, name, arity, asynchronous| {
            let f = metadata(
                &proto,
                name,
                arity,
                asynchronous,
                function(|_, _| Err(Rc::new("unbound prototype body".into()))),
            );
            set(class, name, f);
            Ok(())
        }),
        regexp: Rc::new(|pattern, flags| {
            assert_eq!(flags, "");
            assert_eq!(pattern, "^[1-9]\\d*$");
            Ok(obj(vec![(
                "test",
                function(|_, args| {
                    let value = text(&args[0]);
                    let digits = value.as_bytes();
                    Ok(b(!digits.is_empty()
                        && digits[0] >= b'1'
                        && digits[0] <= b'9'
                        && digits.iter().all(u8::is_ascii_digit)))
                }),
            )]))
        }),
        multiply: Rc::new(|a, b| Ok(n(number(&a) * number(&b)))),
        null: Rc::new(|| Rc::new(Js::Null)),
        is_string: Rc::new(|v| matches!(v.as_ref(), Js::S(_))),
    });
    let class = define_exchange_class(ops).unwrap();
    for _ in 0..2 {
        let error = constructor.borrow().as_ref().unwrap()(
            active.clone(),
            target.clone(),
            original.to_vec(),
        )
        .err()
        .unwrap();
        assert!(
            Rc::ptr_eq(&error, &sentinel),
            "WOO-EXCHANGE native constructor sentinel"
        );
    }
    assert_eq!(inspected.get(), 2);
    assert_eq!(objects.borrow().len(), 2);
    let objects = objects.borrow();
    let first = &objects[0];
    let second = &objects[1];
    for r in objects.iter() {
        let Js::O(fields) = r.as_ref() else {
            panic!("object")
        };
        assert_eq!(
            fields
                .borrow()
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            FIELDS,
            "WOO-EXCHANGE native field order"
        );
    }
    assert_eq!(text(&get(first, "grafanaEndpointsEvery").unwrap()), "every");
    assert_eq!(
        text(&get(first, "grafanaEndpointsPeriodic").unwrap()),
        "periodic"
    );
    assert!(matches!(
        get(first, "_accountPositionMode").unwrap().as_ref(),
        Js::Null
    ));
    assert!(
        matches!(
            get(first, "_fundingRefreshInFlight").unwrap().as_ref(),
            Js::B(false)
        ),
        "WOO-EXCHANGE native funding boolean kind"
    );
    for key in [
        "xhr",
        "_primaryXhr",
        "_secondaryXhr",
        "_proxyEnforcedXhr",
        "publicWebsocket",
        "privateWebsocket",
        "_fundingRefreshTimer",
    ] {
        assert!(matches!(get(first, key).unwrap().as_ref(), Js::U));
    }
    for (key, kind) in COLLECTIONS {
        let a = get(first, key).unwrap();
        let b = get(second, key).unwrap();
        assert!(!Rc::ptr_eq(&a, &b), "WOO-EXCHANGE native fresh collection");
        assert_eq!(text(&get(&a, "kind").unwrap()), kind);
    }
    let mut pairs = 0;
    for i in 0..6 {
        for j in i + 1..6 {
            assert!(
                !Rc::ptr_eq(
                    &get(first, COLLECTIONS[i].0).unwrap(),
                    &get(first, COLLECTIONS[j].0).unwrap()
                ),
                "WOO-EXCHANGE native distinct collection fields"
            );
            pairs += 1;
        }
    }
    assert_eq!(pairs, 15, "WOO-EXCHANGE native collection pair floor");
    let functions = metas.borrow();
    let prototype = get(&class, "getPositionMode").unwrap();
    let (_, name, arity, asynchronous) = functions
        .iter()
        .find(|(v, _, _, _)| Rc::ptr_eq(v, &prototype))
        .expect("WOO-EXCHANGE native prototype handle");
    assert_eq!(
        (name.as_str(), *arity, *asynchronous),
        ("getPositionMode", 0, true),
        "WOO-EXCHANGE native prototype metadata"
    );
    assert_eq!(functions.len(), 143, "WOO-EXCHANGE native function count");
    assert_eq!(
        functions
            .iter()
            .filter(|(_, name, arity, asynchronous)| name == "rateLimit"
                && *arity == 1
                && !asynchronous)
            .count(),
        2,
        "WOO-EXCHANGE native constructor callback count"
    );
    let layout = FIELDS
        .iter()
        .filter_map(|name| {
            let value = get(first, name).unwrap();
            functions
                .iter()
                .find(|(v, _, _, _)| Rc::ptr_eq(v, &value))
                .map(|(_, n, a, s)| json!([n, a, s]))
        })
        .collect::<Vec<_>>();
    assert_eq!(layout.len(), 68);
    for (key, arity, asynchronous) in [
        ("cancelAlgoOrder", 1, true),
        ("getPositionSideForOrder", 1, true),
        ("afterMethodCalledInWorker", 2, false),
        ("tick", 0, true),
        ("fetchHistoricalFills", 0, true),
        ("fetchOrdersRecursively", 1, true),
    ] {
        assert!(
            layout.contains(&json!([key, arity, asynchronous])),
            "WOO-EXCHANGE native special arrow metadata"
        );
    }
    for name in FIELDS {
        let a = get(first, name).unwrap();
        if matches!(a.as_ref(), Js::F(_)) {
            assert!(
                !Rc::ptr_eq(&a, &get(second, name).unwrap()),
                "WOO-EXCHANGE native fresh arrow"
            );
        }
    }
    for (key, expected) in [
        ("MAX_FILL_POSITION_REFRESH_ATTEMPTS", 4.),
        ("FILL_POSITION_REFRESH_BASE_DELAY_MS", 150.),
        ("FUNDING_REFRESH_INTERVAL_MS", 60000.),
    ] {
        assert!(
            matches!(get(&class, key).unwrap().as_ref(), Js::N(_)),
            "WOO-EXCHANGE native static number kind"
        );
        assert_eq!(
            number(&get(&class, key).unwrap()),
            expected,
            "WOO-EXCHANGE native static constant"
        );
    }
    let rule = get(first, "clientOrderIdRule").unwrap();
    assert!(!Rc::ptr_eq(
        &rule,
        &get(second, "clientOrderIdRule").unwrap()
    ));
    assert_eq!(text(&get(&rule, "label").unwrap()), "WOO");
    assert_eq!(
        text(&get(&rule, "requirement").unwrap()),
        "be a canonical positive decimal integer within the JavaScript safe-integer range"
    );
    for (key, expected_arity) in [("mint", 0), ("isValid", 1)] {
        let value = get(&rule, key).unwrap();
        let (_, name, arity, asynchronous) = functions
            .iter()
            .find(|(v, _, _, _)| Rc::ptr_eq(v, &value))
            .expect("WOO-EXCHANGE native client rule function handle");
        assert_eq!(
            (name.as_str(), *arity, *asynchronous),
            (key, expected_arity, false),
            "WOO-EXCHANGE native client rule function metadata"
        );
    }
    let mut minted = vec![];
    for (input, expected) in [
        (0., "100000000000000"),
        (0.5, "550000000000000"),
        (0.9999999999999999, "999999999999999"),
    ] {
        random.set(input);
        let returned = call(get(&rule, "mint").unwrap(), rule.clone(), vec![]).unwrap();
        let Js::S(actual) = returned.as_ref() else {
            panic!("WOO-EXCHANGE native minted string kind")
        };
        let actual = actual.clone();
        assert_eq!(actual, expected, "WOO-EXCHANGE native minted client ID");
        minted.push(actual);
    }
    let mut validated = vec![];
    for (input, expected) in [
        ("1", true),
        ("0", false),
        ("01", false),
        ("-1", false),
        ("+1", false),
        ("1.0", false),
        ("1e3", false),
        (" 1", false),
        ("1\n", false),
        ("1\r", false),
        ("1\u{2028}", false),
        ("\u{feff}1", false),
        ("9007199254740991", true),
        ("9007199254740992", false),
    ] {
        let before = conversions.get();
        let returned = call(get(&rule, "isValid").unwrap(), rule.clone(), vec![s(input)]).unwrap();
        let Js::B(actual) = returned.as_ref() else {
            panic!("WOO-EXCHANGE native validity boolean kind")
        };
        let actual = *actual;
        assert_eq!(
            actual, expected,
            "WOO-EXCHANGE native client ID validity {input:?}"
        );
        if !input.bytes().all(|c| c.is_ascii_digit()) || input.starts_with('0') {
            assert_eq!(
                conversions.get(),
                before,
                "WOO-EXCHANGE native validation short circuit"
            );
        }
        validated.push(actual);
    }
    let missing = call(get(first, "isAlgoOrder").unwrap(), first.clone(), vec![])
        .err()
        .unwrap();
    assert_eq!(
        missing.as_str(),
        "unbound venue body isAlgoOrder",
        "WOO-EXCHANGE native open binding finding"
    );
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-class-oracle.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"{}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reference["keys"], json!(FIELDS.as_slice()));
    assert_eq!(reference["functions"], json!(layout));
    assert_eq!(reference["minted"], json!(minted));
    assert_eq!(reference["validated"], json!(validated));
}

#[path = "support/class_response.rs"]
mod class_response;
