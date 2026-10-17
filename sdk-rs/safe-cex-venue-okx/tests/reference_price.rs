use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_okx::reference_price::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};

type Error = Rc<ExchangeError>;
type Trace = Rc<RefCell<Vec<String>>>;
struct Ticker {
    values: [Cell<Option<f64>>; 3],
    trace: Trace,
    error: Error,
    fail_field: Option<TickerPriceField>,
    mutate_last: Option<f64>,
}
impl ReferenceTicker<Error> for Ticker {
    fn price(&self, field: TickerPriceField) -> Result<Option<f64>, Error> {
        let (name, index) = match field {
            TickerPriceField::Mark => ("mark", 0),
            TickerPriceField::Index => ("index", 1),
            TickerPriceField::Last => ("last", 2),
        };
        self.trace.borrow_mut().push(format!("read:{name}"));
        if self.fail_field == Some(field) {
            return Err(self.error.clone());
        }
        if field == TickerPriceField::Mark
            && let Some(value) = self.mutate_last
        {
            self.values[2].set(Some(value));
        }
        Ok(self.values[index].get())
    }
}
fn error() -> Error {
    let mut error = ExchangeError::new("reference failure");
    error.name = "TypeError".into();
    error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "PRICE"
    )));
    error.store_subscriber_ids = vec![7, 3];
    Rc::new(error)
}
fn ticker(
    mark: Option<f64>,
    index: Option<f64>,
    last: Option<f64>,
    trace: &Trace,
    error: &Error,
) -> Rc<Ticker> {
    Rc::new(Ticker {
        values: [Cell::new(mark), Cell::new(index), Cell::new(last)],
        trace: trace.clone(),
        error: error.clone(),
        fail_field: None,
        mutate_last: None,
    })
}
fn bits(value: Option<f64>) -> Value {
    value
        .map(|v| json!(format!("{:016x}", v.to_bits())))
        .unwrap_or(Value::Null)
}
fn expect(result: Result<f64, Error>, expected: Option<f64>, error: &Error, label: &str) -> Value {
    match (result, expected) {
        (Ok(actual), Some(expected)) => {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "OKX-REFERENCE native {label}"
            );
            json!({"bits":format!("{:016x}", actual.to_bits())})
        }
        (Err(actual), None) => {
            assert!(Rc::ptr_eq(&actual, error), "native {label} error pointer");
            assert_eq!(&actual, error, "native {label} complete error");
            json!({"error":{"name":actual.name,"message":actual.message,"same":true}})
        }
        _ => panic!("native {label} wrong success/error branch"),
    }
}
fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).into()).collect()
}

#[test]
fn reference_prices_native_before_source() {
    let error = error();
    let mut lookups = vec![];
    let mut native_lookups = vec![];
    for key in [
        vec![],
        "__proto__".encode_utf16().collect(),
        vec![0xd800, 65, 0xdc00],
    ] {
        for id_hit in [false, true] {
            for symbol_hit in [false, true] {
                for fail_id in [false, true] {
                    let by_id_value = Rc::new(7);
                    let by_symbol_value = Rc::new(13);
                    let trace = RefCell::new(vec![]);
                    let result = get_ticker(
                        || {
                            trace.borrow_mut().push("id");
                            if fail_id {
                                Err(error.clone())
                            } else {
                                Ok(id_hit.then(|| by_id_value.clone()))
                            }
                        },
                        || {
                            trace.borrow_mut().push("symbol");
                            Ok(symbol_hit.then(|| by_symbol_value.clone()))
                        },
                    );
                    let expected_trace = if fail_id || id_hit {
                        vec!["id"]
                    } else {
                        vec!["id", "symbol"]
                    };
                    assert_eq!(
                        *trace.borrow(),
                        expected_trace,
                        "OKX-REFERENCE native lookup order"
                    );
                    let outcome = if fail_id {
                        let actual = result.unwrap_err();
                        assert!(Rc::ptr_eq(&actual, &error), "native lookup error pointer");
                        assert_eq!(actual, error, "native complete lookup error");
                        json!({"error":true})
                    } else {
                        let result = result.unwrap();
                        let expected = if id_hit {
                            Some(&by_id_value)
                        } else if symbol_hit {
                            Some(&by_symbol_value)
                        } else {
                            None
                        };
                        assert_eq!(
                            result.is_some(),
                            expected.is_some(),
                            "native lookup presence"
                        );
                        if let Some(expected) = expected {
                            assert!(
                                Rc::ptr_eq(result.as_ref().unwrap(), expected),
                                "OKX-REFERENCE native lookup pointer"
                            );
                            assert_eq!(
                                **result.as_ref().unwrap(),
                                **expected,
                                "native lookup pointee"
                            );
                        }
                        json!({"value":result.map(|v| *v)})
                    };
                    lookups.push(json!({"key":key,"symbol":[66,0xd800],"idHit":id_hit,"symbolHit":symbol_hit,"failId":fail_id}));
                    native_lookups.push(json!({"trace":*trace.borrow(),"outcome":outcome}));
                }
            }
        }
    }
    assert_eq!(lookups.len(), 24, "declared lookup floor");
    let refs = [
        (Some(5.), Some(7.), Some(9.), Some(5.), None, None, &[][..]),
        (
            None,
            Some(7.),
            Some(9.),
            Some(7.),
            None,
            None,
            &["get", "read:mark"][..],
        ),
        (
            None,
            Some(-7.),
            Some(9.),
            Some(9.),
            None,
            None,
            &["get", "read:mark", "read:last"][..],
        ),
        (
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(f64::NEG_INFINITY),
            Some(0.),
            None,
            None,
            &["get", "read:mark", "read:last"][..],
        ),
        (
            Some(-0.),
            Some(-0.),
            Some(-0.),
            Some(0.),
            None,
            None,
            &["get", "read:mark", "read:last"][..],
        ),
        (
            Some(-3.),
            None,
            Some(2.),
            Some(2.),
            None,
            None,
            &["get", "read:mark", "read:last"][..],
        ),
        (
            Some(f64::INFINITY),
            Some(1e-300),
            Some(9.),
            Some(1e-300),
            None,
            None,
            &["get", "read:mark"][..],
        ),
        (
            Some(1e21),
            Some(7.),
            Some(9.),
            Some(1e21),
            None,
            None,
            &[][..],
        ),
        (
            None,
            Some(-1.),
            Some(2.),
            Some(19.),
            Some(19.),
            None,
            &["get", "read:mark", "read:last"][..],
        ),
        (
            None,
            Some(7.),
            Some(9.),
            None,
            None,
            Some(TickerPriceField::Mark),
            &["get", "read:mark"][..],
        ),
        (
            None,
            Some(-7.),
            Some(9.),
            None,
            None,
            Some(TickerPriceField::Last),
            &["get", "read:mark", "read:last"][..],
        ),
        (
            None,
            None,
            None,
            Some(0.),
            None,
            None,
            &["get", "read:mark", "read:last"][..],
        ),
    ];
    let mut reference = vec![];
    let mut native_reference = vec![];
    for (reference_value, mark, last, expected, mutate_last, fail_field, expected_trace) in refs {
        let trace = Rc::new(RefCell::new(vec![]));
        let mut native = ticker(mark, None, last, &trace, &error);
        Rc::get_mut(&mut native).unwrap().mutate_last = mutate_last;
        Rc::get_mut(&mut native).unwrap().fail_field = fail_field;
        let result = resolve_reference_price(reference_value, || {
            trace.borrow_mut().push("get".into());
            Ok(native.clone().into())
        });
        let outcome = expect(result, expected, &error, "reference value");
        assert_eq!(
            *trace.borrow(),
            strings(expected_trace),
            "native reference getter order"
        );
        reference.push(json!({"reference":bits(reference_value),"mark":bits(mark),"last":bits(last),"mutateLast":bits(mutate_last),"failField":fail_field.map(|f|if f==TickerPriceField::Mark {"mark"}else{"last"})}));
        native_reference.push(json!({"trace":*trace.borrow(),"outcome":outcome}));
    }
    assert_eq!(reference.len(), 12, "declared reference floor");
    let ext_specs = [
        (
            Some(5.),
            Some(3.),
            Some(false),
            None,
            false,
            [false, true, true],
        ),
        (
            Some(5.),
            Some(5.),
            Some(true),
            None,
            false,
            [false, false, true],
        ),
        (
            Some(0.),
            Some(3.),
            Some(true),
            None,
            false,
            [false, false, true],
        ),
        (
            Some(f64::NAN),
            Some(3.),
            None,
            None,
            false,
            [false, false, false],
        ),
        (
            Some(9.),
            Some(0.),
            Some(false),
            None,
            false,
            [false, false, false],
        ),
        (
            Some(5.),
            Some(5.),
            Some(false),
            Some(13.),
            false,
            [false, false, true],
        ),
        (
            Some(5.),
            Some(3.),
            Some(false),
            None,
            true,
            [false, true, false],
        ),
    ];
    let mut external = vec![];
    let mut native_external = vec![];
    for (price, last, reported, mutate, throws, expected) in ext_specs {
        for (config_index, config) in [
            ExternalConfig::Absent,
            ExternalConfig::Present,
            ExternalConfig::InProcess,
        ]
        .into_iter()
        .enumerate()
        {
            for source in [PriceSource::Mark, PriceSource::Index] {
                let trace = Rc::new(RefCell::new(vec![]));
                let native = ticker(price, price, last, &trace, &error);
                let source_name = if source == PriceSource::Mark {
                    "mark"
                } else {
                    "index"
                };
                let result = has_external_trigger_reference_price(
                    config,
                    source,
                    Some(&*native),
                    |actual_source| {
                        assert_eq!(actual_source, source, "native external method selected");
                        trace.borrow_mut().push(format!("external:{source_name}"));
                        if throws {
                            return Err(error.clone());
                        }
                        if let Some(value) = mutate {
                            native.values[if source == PriceSource::Mark { 0 } else { 1 }]
                                .set(Some(value));
                        }
                        Ok(reported)
                    },
                );
                let outcome = if throws && config == ExternalConfig::InProcess {
                    let actual = result.unwrap_err();
                    assert!(Rc::ptr_eq(&actual, &error), "native external error pointer");
                    assert_eq!(actual, error, "native complete external error");
                    json!({"error":true})
                } else {
                    let actual = result.unwrap();
                    assert_eq!(
                        actual, expected[config_index],
                        "OKX-REFERENCE native external availability"
                    );
                    json!({"value":actual})
                };
                let mut expected_trace = vec![];
                if config == ExternalConfig::InProcess {
                    expected_trace.push(format!("external:{source_name}"));
                }
                if config != ExternalConfig::Absent
                    && !(config == ExternalConfig::InProcess && (throws || reported == Some(true)))
                {
                    expected_trace.extend([format!("read:{source_name}"), "read:last".into()]);
                }
                assert_eq!(
                    *trace.borrow(),
                    expected_trace,
                    "native external getter order"
                );
                external.push(json!({"config":config_index,"source":source_name,"price":bits(price),"last":bits(last),"reported":reported,"mutate":bits(mutate),"throws":throws}));
                native_external.push(json!({"trace":*trace.borrow(),"outcome":outcome}));
            }
        }
    }
    assert_eq!(external.len(), 42, "declared external floor");

    let trigger_specs = [
        (
            "index",
            true,
            false,
            9.,
            5.,
            None,
            None,
            Some(7.),
            &["get:0", "read:index"][..],
        ),
        (
            "index",
            false,
            false,
            9.,
            5.,
            None,
            None,
            Some(0.),
            &["get:0", "external:index"][..],
        ),
        (
            "index",
            false,
            true,
            9.,
            5.,
            None,
            None,
            Some(7.),
            &["get:0", "external:index", "read:index"][..],
        ),
        (
            "mark",
            true,
            false,
            9.,
            5.,
            None,
            None,
            Some(11.),
            &["get:0", "read:mark"][..],
        ),
        (
            "mark",
            false,
            false,
            9.,
            5.,
            None,
            None,
            Some(0.),
            &["get:0", "external:mark"][..],
        ),
        (
            "mark",
            false,
            true,
            9.,
            5.,
            None,
            None,
            Some(11.),
            &["get:0", "external:mark", "read:mark"][..],
        ),
        (
            "last",
            false,
            false,
            9.,
            5.,
            None,
            None,
            Some(9.),
            &["get:0", "read:last"][..],
        ),
        (
            "surrogate",
            false,
            false,
            9.,
            5.,
            None,
            None,
            Some(9.),
            &["get:0", "read:last"][..],
        ),
        (
            "last",
            false,
            false,
            0.,
            5.,
            None,
            None,
            Some(5.),
            &["get:0", "read:last"][..],
        ),
        (
            "last",
            false,
            false,
            0.,
            0.,
            None,
            None,
            Some(17.),
            &["get:0", "read:last", "get:1", "read:mark"][..],
        ),
        (
            "last",
            false,
            false,
            0.,
            f64::NAN,
            None,
            None,
            Some(17.),
            &["get:0", "read:last", "get:1", "read:mark"][..],
        ),
        (
            "index",
            true,
            false,
            9.,
            5.,
            Some(f64::NAN),
            None,
            Some(0.),
            &["get:0", "read:index"][..],
        ),
        (
            "mark",
            true,
            false,
            9.,
            5.,
            Some(-0.),
            None,
            Some(0.),
            &["get:0", "read:mark"][..],
        ),
        (
            "index",
            false,
            true,
            9.,
            5.,
            None,
            Some("mutate"),
            Some(23.),
            &["get:0", "external:index", "read:index"][..],
        ),
        (
            "last",
            false,
            false,
            9.,
            5.,
            None,
            Some("get:0"),
            None,
            &["get:0"][..],
        ),
        (
            "index",
            false,
            true,
            9.,
            5.,
            None,
            Some("external"),
            None,
            &["get:0", "external:index"][..],
        ),
        (
            "mark",
            true,
            false,
            9.,
            5.,
            None,
            Some("read"),
            None,
            &["get:0", "read:mark"][..],
        ),
        (
            "last",
            false,
            false,
            0.,
            0.,
            None,
            Some("get:1"),
            None,
            &["get:0", "read:last", "get:1"][..],
        ),
    ];
    let mut trigger = vec![];
    let mut native_trigger = vec![];
    for (kind, loaded, ext, last, fallback, override_price, failure, expected, expected_trace) in
        trigger_specs
    {
        let trace = Rc::new(RefCell::new(vec![]));
        let mut first = ticker(Some(11.), Some(7.), Some(last), &trace, &error);
        if let Some(value) = override_price {
            first.values[if kind == "index" { 1 } else { 0 }].set(Some(value));
        }
        if failure == Some("read") {
            Rc::get_mut(&mut first).unwrap().fail_field = Some(TickerPriceField::Mark);
        }
        let second = ticker(Some(17.), Some(19.), Some(29.), &trace, &error);
        let id = vec![77, 0xd800];
        let key: Vec<u16> = if kind == "surrogate" {
            vec![0xd800]
        } else {
            kind.encode_utf16().collect()
        };
        let index_ids = RefCell::new(if loaded {
            std::collections::HashSet::from([id.clone()])
        } else {
            Default::default()
        });
        let mark_ids = RefCell::new(index_ids.borrow().clone());
        let gets = Cell::new(0);
        let result = resolve_trigger_reference_price(
            Some(&key),
            Some(fallback),
            || Ok(index_ids.borrow().contains(&id)),
            || Ok(mark_ids.borrow().contains(&id)),
            || {
                let i = gets.get();
                gets.set(i + 1);
                trace.borrow_mut().push(format!("get:{i}"));
                if failure == Some(if i == 0 { "get:0" } else { "get:1" }) {
                    return Err(error.clone());
                }
                Ok(Some(if i == 0 {
                    first.clone()
                } else {
                    second.clone()
                }))
            },
            |source, ticker| {
                assert!(
                    std::ptr::eq(ticker.unwrap(), &*first),
                    "native external sees first ticker identity"
                );
                trace.borrow_mut().push(format!(
                    "external:{}",
                    if source == PriceSource::Index {
                        "index"
                    } else {
                        "mark"
                    }
                ));
                if failure == Some("external") {
                    return Err(error.clone());
                }
                if failure == Some("mutate") {
                    first.values[1].set(Some(23.));
                }
                Ok(ext)
            },
        );
        let outcome = expect(result, expected, &error, "trigger value");
        assert_eq!(
            *trace.borrow(),
            strings(expected_trace),
            "native trigger lazy lookup/getter order"
        );
        trigger.push(json!({"kind":key,"loaded":loaded,"external":ext,"last":bits(Some(last)),"fallback":bits(Some(fallback)),"override":bits(override_price),"failure":failure}));
        native_trigger.push(json!({"trace":*trace.borrow(),"outcome":outcome}));
    }
    assert_eq!(trigger.len(), 18, "declared trigger floor");
    let live_symbol = RefCell::new(vec![65]);
    let wanted = Rc::new(29);
    let actual = get_ticker(
        || {
            *live_symbol.borrow_mut() = vec![66, 0xd800];
            Ok::<_, Error>(None)
        },
        || {
            assert_eq!(
                *live_symbol.borrow(),
                [66, 0xd800],
                "native lazy symbol read after ID lookup mutation"
            );
            Ok(Some(wanted.clone()))
        },
    )
    .unwrap()
    .unwrap();
    assert!(
        Rc::ptr_eq(&actual, &wanted),
        "native live symbol lookup pointer"
    );
    let absent = resolve_reference_price(None, || Ok::<_, Error>(None::<Rc<Ticker>>)).unwrap();
    assert_eq!(
        absent.to_bits(),
        0.0_f64.to_bits(),
        "native absent ticker reference"
    );
    let live_id = RefCell::new(vec![65]);
    let witness_trace = Rc::new(RefCell::new(vec![]));
    let witness_ticker = ticker(Some(11.), Some(7.), Some(9.), &witness_trace, &error);
    let value = resolve_trigger_reference_price(
        Some(&"index".encode_utf16().collect::<Vec<_>>()),
        None,
        || Ok(*live_id.borrow() == vec![66]),
        || Ok(false),
        || {
            *live_id.borrow_mut() = vec![66];
            Ok::<_, Error>(Some(witness_ticker.clone()))
        },
        |_, _| panic!("external after live ID was loaded"),
    )
    .unwrap();
    assert_eq!(
        value.to_bits(),
        7.0_f64.to_bits(),
        "native loaded predicate after ticker changes market ID"
    );
    assert_eq!(
        *witness_trace.borrow(),
        vec!["read:index"],
        "native live ID loaded ticker getter"
    );
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reference-price-oracle.mjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&serde_json::to_vec(&json!({"lookups":lookups,"reference":reference,"external":external,"trigger":trigger})).unwrap()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source,
        json!({"lookups":native_lookups,"reference":native_reference,"external":native_external,"trigger":native_trigger}),
        "live oracle after every native value/error/getter assertion"
    );
}
