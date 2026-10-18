use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown, from_json},
    native_store::DefaultStore,
    runtime::ExchangeError,
};
use safe_cex_num::js_number_string;
use safe_cex_store::{Store, StoreUpdate, model_patch::ModelPatch};
use safe_cex_types::{StoreSubscriberError, Ticker};
use safe_cex_venue_bitget::ticker::{TickerHandlerError, handle_ticker_topic};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
    rc::Rc,
};
const SCENES: [&str; 22] = [
    "symbol-sentinel",
    "symbol-no-sentinel",
    "own-shadow",
    "id-field",
    "number-positive",
    "number-negative-zero",
    "number-nan",
    "number-infinity",
    "optional-absent",
    "optional-positive",
    "truthy-info-stripped",
    "info-false",
    "info-null",
    "info-id-error-stripped",
    "empty-update",
    "patched-prototype",
    "second-prototype",
    "retained-prototype-alias",
    "replacement-clear",
    "reset-clear",
    "own-constructor",
    "notification-throws",
];
fn ticker(id: &str, symbol: &str) -> Ticker {
    Ticker {
        id: id.into(),
        symbol: symbol.into(),
        bid: 0.,
        ask: 0.,
        last: 0.,
        mark: 0.,
        index: Some(0.),
        mid_price: None,
        percentage: 0.,
        open_interest: 0.,
        funding_rate: 0.,
        funding_time: 0.,
        volume: 0.,
        quote_volume: 0.,
        timestamp: None,
        info: None,
    }
}
fn replace(store: &DefaultStore, rows: Vec<Ticker>) {
    Store::update(
        store,
        StoreUpdate {
            tickers: Some(rows),
            ..Default::default()
        },
        false,
    )
    .unwrap()
}
fn assert_rows(actual: &[Ticker], expected: &[Ticker], label: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "PROTO native independent row floor {label}"
    );
    for (a, e) in actual.iter().zip(expected) {
        assert_eq!(a.id, e.id, "PROTO native row ID {label}");
        assert_eq!(a.symbol, e.symbol, "PROTO native row symbol {label}");
        for (name, n, v) in [
            ("bid", a.bid, e.bid),
            ("ask", a.ask, e.ask),
            ("last", a.last, e.last),
            ("mark", a.mark, e.mark),
            ("index", a.index.unwrap(), e.index.unwrap()),
            ("percentage", a.percentage, e.percentage),
            ("openInterest", a.open_interest, e.open_interest),
            ("fundingRate", a.funding_rate, e.funding_rate),
            ("fundingTime", a.funding_time, e.funding_time),
            ("volume", a.volume, e.volume),
            ("quoteVolume", a.quote_volume, e.quote_volume),
        ] {
            assert_eq!(
                n.to_bits(),
                v.to_bits(),
                "PROTO native numeric bits {label} {name}"
            )
        }
        assert_eq!(
            a.mid_price.map(f64::to_bits),
            e.mid_price.map(f64::to_bits),
            "PROTO native optional bits {label}"
        );
        assert_eq!(a.timestamp.map(f64::to_bits), e.timestamp.map(f64::to_bits));
        assert_eq!(a.info, e.info, "PROTO native retained input info {label}")
    }
}
struct Observed {
    name: &'static str,
    rows: Vec<Ticker>,
    first: usize,
    second: usize,
    error: Option<(&'static str, &'static str)>,
}
fn run(scene: &'static str) -> Observed {
    let mut poison = ticker("poison", "__proto__");
    match scene {
        "number-positive" | "second-prototype" => poison.last = 3.,
        "number-negative-zero" => poison.last = -0.,
        "number-nan" => poison.last = f64::NAN,
        "number-infinity" => poison.last = f64::INFINITY,
        "optional-positive" => poison.mid_price = Some(42.),
        "truthy-info-stripped" => poison.info = Some(json!({"id":"undefined"}).into()),
        "info-false" => poison.info = Some(json!(false).into()),
        "info-null" => poison.info = Some(Value::Null.into()),
        "info-id-error-stripped" => poison.info = Some(json!({"id":{"toString":1}}).into()),
        _ => {}
    }
    let sentinel = || ticker("undefined", "sentinel");
    let mut seed = vec![poison];
    match scene {
        "symbol-no-sentinel" => {}
        "own-shadow" => {
            seed.push(ticker("own", "symbol"));
            seed.push(sentinel());
        }
        "own-constructor" => {
            seed.push(ticker("own", "constructor"));
            seed.push(sentinel());
        }
        "second-prototype" => {
            seed.push(ticker("second", "__proto__"));
            seed.push(sentinel());
        }
        _ => seed.push(sentinel()),
    }
    let store = DefaultStore::new();
    replace(&store, seed.clone());
    let mut expected = seed.clone();
    if ["truthy-info-stripped", "info-id-error-stripped"].contains(&scene) {
        assert!(
            Store::tickers(&store).unwrap()[0].info.is_none(),
            "PROTO native required actual ingress strips truthy info"
        );
        expected[0].info = None;
    }
    match scene {
        "retained-prototype-alias" => {
            let mut p = ModelPatch::default();
            p.set_json("symbol", json!("renamed"));
            Store::update_ticker(&store, "poison", &p).unwrap();
            expected[0].symbol = "renamed".into();
        }
        "replacement-clear" => {
            expected = vec![ticker("btc", "BTC"), sentinel()];
            replace(&store, expected.clone());
        }
        "reset-clear" => {
            Store::reset(&store).unwrap();
            expected.clear();
        }
        _ => {}
    }
    assert_rows(
        &Store::tickers(&store).unwrap(),
        &expected,
        "producer input before handler",
    );
    if scene == "symbol-sentinel" {
        assert!(
            !Store::tickers_by_symbol(&store)
                .unwrap()
                .contains_key("__proto__"),
            "PROTO native plain symbol dictionary has no own prototype key"
        );
        let lookup = Store::lookup_ticker_by_symbol(&store, "symbol").unwrap();
        assert!(
            lookup.is_truthy(),
            "PROTO native real shared prototype lookup entry"
        );
        assert_eq!(
            lookup.update_id().unwrap(),
            Some("undefined".into()),
            "PROTO native inherited primitive id uses source property-key coercion"
        );
    }
    let key = match scene {
        "id-field" => "id",
        "number-positive"
        | "number-negative-zero"
        | "number-nan"
        | "number-infinity"
        | "second-prototype" => "last",
        "optional-absent" | "optional-positive" => "midPrice",
        "truthy-info-stripped" | "info-false" | "info-null" | "info-id-error-stripped" => "info",
        "own-constructor" => "constructor",
        _ => "symbol",
    };
    let trait_lookup = Store::lookup_ticker_by_symbol(&store, key).unwrap();
    let direct_lookup = store.lookup_ticker_by_symbol(key).unwrap();
    assert_eq!(
        (trait_lookup.is_truthy(), trait_lookup.update_id().unwrap()),
        (
            direct_lookup.is_truthy(),
            direct_lookup.update_id().unwrap()
        ),
        "PROTO native trait dispatch uses actual Store view"
    );
    let input = match scene {
        "empty-update" => json!({"data":[{"instId":key}]}),
        "patched-prototype" => {
            json!({"data":[{"instId":"__proto__","lastPr":"5"},{"instId":"last","lastPr":"9"}]})
        }
        "notification-throws" => {
            json!({"data":[{"instId":"symbol","lastPr":"9"},{"instId":"__proto__","lastPr":"5"}]})
        }
        _ => json!({"data":[{"instId":key,"lastPr":"9"}]}),
    };
    let before = expected.clone();
    let expected_writes = match scene {
        "symbol-sentinel"
        | "own-shadow"
        | "id-field"
        | "number-positive"
        | "number-infinity"
        | "optional-positive"
        | "retained-prototype-alias"
        | "own-constructor"
        | "notification-throws" => 1,
        "patched-prototype" => 2,
        _ => 0,
    };
    match scene {
        "own-shadow" | "own-constructor" => expected[1].last = 9.,
        "symbol-sentinel"
        | "id-field"
        | "number-positive"
        | "number-infinity"
        | "optional-positive"
        | "retained-prototype-alias"
        | "notification-throws" => expected[1].last = 9.,
        "patched-prototype" => {
            expected[0].last = 5.;
            expected[1].last = 9.;
        }
        _ => {}
    }
    let first = Rc::new(RefCell::new(Vec::<Vec<Ticker>>::new()));
    let second = Rc::new(RefCell::new(0));
    let snapshots = first.clone();
    let throws = scene == "notification-throws";
    let original = StoreSubscriberError::with_response("subscriber witness", json!({"code":17}))
        .with_cause(Rc::new(std::io::Error::other("original subscriber cause")));
    let original_for_subscriber = original.clone();
    let _sub = Store::subscribe(
        &store,
        Box::new(move |data, _| {
            snapshots.borrow_mut().push(data.tickers.clone());
            if throws {
                Err(original_for_subscriber.clone())
            } else {
                Ok(())
            }
        }),
    );
    let count = second.clone();
    let _sub2 = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            *count.borrow_mut() += 1;
            Ok(())
        }),
    );
    let result = handle_ticker_topic(&store, Some(&input));
    if throws {
        let Err(TickerHandlerError::Store(safe_cex_store::StoreMutationError::Notification(error))) =
            result
        else {
            panic!("PROTO native original Store notification expected")
        };
        assert_eq!(
            error.subscriber_id(),
            0,
            "PROTO native first subscriber identity"
        );
        assert_eq!(
            error.cause(),
            &original,
            "PROTO native original error cause identity"
        );
        assert_eq!(error.message(), "subscriber witness");
    } else {
        assert!(
            result.is_ok(),
            "PROTO native handler success {scene}: {result:?}"
        )
    }
    let rows = Store::tickers(&store).unwrap().to_vec();
    assert_rows(&rows, &expected, scene);
    assert_eq!(
        first.borrow().len(),
        expected_writes,
        "PROTO native independent first subscriber floor {scene}"
    );
    assert_eq!(
        *second.borrow(),
        if throws { 0 } else { expected_writes },
        "PROTO native independent second subscriber floor {scene}"
    );
    for (i, state) in first.borrow().iter().enumerate() {
        if scene == "patched-prototype" && i == 0 {
            let mut intermediate = before.clone();
            intermediate[0].last = 5.;
            assert_rows(state, &intermediate, "committed first prototype update");
        } else {
            assert_rows(state, &expected, "subscriber observes committed values");
        }
    }
    let first_count = first.borrow().len();
    let second_count = *second.borrow();
    Observed {
        name: scene,
        rows,
        first: first_count,
        second: second_count,
        error: throws.then_some(("Error", "subscriber witness")),
    }
}
fn project(row: &Observed) -> Value {
    json!({"name":row.name,"error":row.error.map(|(name,message)|json!({"name":name,"message":message})),"first":row.first,"second":row.second,"after":row.rows.iter().map(|t|{let n=t.last;json!({"id":t.id,"symbol":t.symbol,"last":{"$number":if n==0.&&n.is_sign_negative(){"-0".into()}else{js_number_string(n)},"bits":format!("{:016x}",n.to_bits())}})}).collect::<Vec<_>>()})
}
#[test]
fn prototype_native_before_source() {
    let mut observed = vec![];
    let mut failures = vec![];
    for scene in SCENES {
        match catch_unwind(AssertUnwindSafe(|| run(scene))) {
            Ok(o) => observed.push(o),
            Err(_) => failures.push(scene),
        }
    }
    assert!(failures.is_empty(), "PROTO native failures {failures:?}");
    assert_eq!(
        observed.len(),
        22,
        "PROTO native independent completed floor"
    );
    let direct = direct_id_cases();
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ticker-handler-capture.mjs"
        ))
        .arg("--prototype-floor")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "PROTO source {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let value: Value = serde_json::from_slice(&source.stdout).unwrap();
    let handlers = value.as_array().unwrap();
    assert_eq!(handlers.len(), 2, "PROTO independent handler floor");
    for (handler, file) in handlers
        .iter()
        .zip(["bitget.ws-public.ts", "bitget.ws-secondary-public.ts"])
    {
        assert_eq!(handler["file"], file);
        assert_eq!(handler["last_index"], 21, "PROTO final source iteration");
        assert_eq!(
            handler["getter"],
            json!({"hasOwnPrototype":false,"hasOwnSymbol":false,"inheritedSymbol":"__proto__","idKey":"undefined","prototypeIsRow":true}),
            "PROTO maintained Store dictionary prototype and inherited primitive shape"
        );
        assert_eq!(
            handler["cases"],
            json!(observed.iter().map(project).collect::<Vec<_>>()),
            "PROTO wire comparison after every native assertion"
        );
    }
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ticker-handler-capture.mjs"
        ))
        .arg("--id-dictionary-floor")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "PROTO direct source {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let value: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(value["last_index"], 10, "PROTO direct source stop");
    assert_eq!(
        value["cases"],
        json!(direct),
        "PROTO direct wire after all native assertions"
    );
}
fn direct_id_cases() -> Vec<Value> {
    let mut outputs = vec![];
    for name in [
        "string",
        "number",
        "falsy-number",
        "empty-patch",
        "info-only",
        "own-shadow",
        "updated-prototype",
        "reset-clear",
        "readonly-length",
        "readonly-index",
        "noncanonical-index",
    ] {
        let store = DefaultStore::new();
        let mut proto = ticker("__proto__", "__proto__");
        if name == "number" {
            proto.last = 7.0;
        }
        let mut expected = vec![proto];
        if name == "own-shadow" {
            expected.push(ticker("symbol", "OWN"));
        }
        replace(&store, expected.clone());
        if name == "updated-prototype" {
            let mut patch = ModelPatch::default();
            patch.set_number("last", 7.0);
            Store::update_ticker(&store, "__proto__", &patch).unwrap();
            expected[0].last = 7.0;
        }
        if name == "reset-clear" {
            Store::reset(&store).unwrap();
            expected.clear();
        }
        if name != "reset-clear" {
            assert!(
                !Store::tickers_by_id(&store)
                    .unwrap()
                    .contains_key("__proto__"),
                "PROTO native plain ID dictionary has no own prototype key"
            );
            assert!(
                Store::tickers(&store).unwrap()[0].info.is_none(),
                "PROTO native direct actual stored input"
            );
        }
        let first = Rc::new(RefCell::new(0));
        let second = Rc::new(RefCell::new(0));
        let count = first.clone();
        let _first = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                *count.borrow_mut() += 1;
                Ok(())
            }),
        );
        let count = second.clone();
        let _second = Store::subscribe(
            &store,
            Box::new(move |_, _| {
                *count.borrow_mut() += 1;
                Ok(())
            }),
        );
        let mut patch = ModelPatch::default();
        if name == "info-only" {
            patch.set_json("info", json!({"ignored":true}));
        } else if name != "empty-patch" {
            patch.set_number(
                match name {
                    "readonly-length" => "length",
                    "readonly-index" => "0",
                    "noncanonical-index" => "00",
                    _ => "last",
                },
                9.0,
            );
        }
        let key = if ["number", "falsy-number", "updated-prototype"].contains(&name) {
            "last"
        } else {
            "symbol"
        };
        if name == "string" {
            let Err(safe_cex_store::StoreMutationError::Access(error)) =
                Store::update_ticker(&store, key, &patch)
            else {
                panic!("typed Ticker projection must visibly refuse an inherited primitive")
            };
            assert_eq!(error.phase(), safe_cex_types::StoreReadPhase::Projection);
            let original = error
                .native_cause()
                .downcast_ref::<Thrown>()
                .unwrap()
                .value
                .clone();
            let recovered = Thrown::from(ExchangeError::from(error));
            assert!(
                recovered.value.same_reference(&original),
                "typed projection refusal retains original cause"
            );
            assert_eq!(
                recovered.message(),
                "invalid type: string \"__proto__\", expected struct Ticker"
            );
        }
        let changes = if name == "info-only" {
            from_json(&json!({"info":{"ignored":true}}))
        } else if name == "empty-patch" {
            Js::object(vec![])
        } else {
            Js::object(vec![(
                match name {
                    "readonly-length" => "length",
                    "readonly-index" => "0",
                    "noncanonical-index" => "00",
                    _ => "last",
                },
                Js::Number(9.0),
            )])
        };
        let result = store.facade().unwrap().call_method(
            "updateTicker",
            vec![Js::object(vec![("id", Js::str(key))]), changes],
        );
        let throws = [
            "string",
            "number",
            "updated-prototype",
            "readonly-length",
            "readonly-index",
            "noncanonical-index",
        ]
        .contains(&name);
        let message = match name {
            "string" => "Cannot create property 'last' on string '__proto__'",
            "readonly-length" => {
                "Cannot assign to read only property 'length' of string '__proto__'"
            }
            "readonly-index" => "Cannot assign to read only property '0' of string '__proto__'",
            "noncanonical-index" => "Cannot create property '00' on string '__proto__'",
            _ => "Cannot create property 'last' on number '7'",
        };
        let error = if throws {
            let thrown = result.unwrap_err();
            assert_eq!(
                thrown.value.get("name").unwrap().to_js_string().unwrap(),
                "TypeError".encode_utf16().collect::<Vec<_>>().into(),
                "PROTO native original mutation error name"
            );
            assert_eq!(
                thrown.message(),
                message,
                "PROTO native original failed assignment field and message"
            );
            Some(("TypeError", message))
        } else {
            assert!(
                result.is_ok(),
                "PROTO native direct ID success {name}: {result:?}"
            );
            None
        };
        if name == "own-shadow" {
            expected[1].last = 9.0;
        }
        let rows = Store::tickers(&store).unwrap().to_vec();
        assert_rows(&rows, &expected, name);
        assert_eq!(
            *first.borrow(),
            usize::from(name == "own-shadow"),
            "PROTO native direct ID first subscriber floor"
        );
        assert_eq!(
            *second.borrow(),
            usize::from(name == "own-shadow"),
            "PROTO native direct ID second subscriber floor"
        );
        let observed = Observed {
            name,
            rows,
            first: *first.borrow(),
            second: *second.borrow(),
            error,
        };
        outputs.push(project(&observed));
    }
    assert_eq!(outputs.len(), 11, "PROTO native completed direct ID floor");
    outputs
}
