use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_okx::margin_conversion::*;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};

type Error = Rc<ExchangeError>;
fn error() -> Error {
    let mut e = ExchangeError::new("conversion failure");
    e.name = "TypeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "CONVERT"
    )));
    e.store_subscriber_ids = vec![7, 3];
    Rc::new(e)
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn bits(n: f64) -> Value {
    if n.is_nan() {
        json!("nan")
    } else {
        json!(format!("{:016x}", n.to_bits()))
    }
}
fn observe(
    actual: Result<f64, Error>,
    expected: f64,
    fails: bool,
    cause: &Error,
    label: &str,
) -> Value {
    if fails {
        let e = actual.expect_err(label);
        assert_eq!(*e, **cause, "{label} native error fields");
        assert!(Rc::ptr_eq(&e, cause), "{label} native error identity");
        json!({"error":true})
    } else {
        let n = actual.expect(label);
        if expected.is_nan() {
            assert!(n.is_nan(), "{label} native NaN classification");
        } else {
            assert_eq!(
                n.to_bits(),
                expected.to_bits(),
                "{label} native scalar value"
            );
        }
        json!({"bits":bits(n)})
    }
}
fn call(
    trace: &RefCell<Vec<&'static str>>,
    name: &'static str,
    fail: &str,
    cause: &Error,
) -> Result<(), Error> {
    trace.borrow_mut().push(name);
    if fail == name {
        Err(cause.clone())
    } else {
        Ok(())
    }
}
#[test]
fn margin_conversion_native_before_source() {
    let mut input = json!({"finite":[],"currency":[],"margin":[],"base":[]});
    let mut native = json!({"finite":[],"currency":[],"margin":[],"base":[]});
    let finite = [
        ("undefined", "", 0.0),
        ("null", "", 0.0),
        ("numberZero", "0", 0.0),
        ("stringZero", "-0", -0.0),
        ("prefix", "12.5junk", 12.5),
        ("invalid", "no", 0.0),
        ("infinity", "Infinity", 0.0),
        ("nan", "NaN", 0.0),
        ("object", "-3.75x", -3.75),
        ("space", "\u{feff}\u{a0}2e2end", 200.0),
        ("surrogate", "", 17.0),
        ("symbol", "Symbol(3)", 0.0),
    ];
    for (kind, text, expected) in finite {
        for fail in [false, true] {
            let cause = error();
            let trace = RefCell::new(vec![]);
            let text = if kind == "surrogate" {
                vec![49, 55, 0xd800, 57]
            } else {
                units(text)
            };
            let actual = to_finite_number(|| {
                call(&trace, "string", if fail { "string" } else { "" }, &cause)?;
                Ok(text.clone())
            });
            let outcome = observe(actual, expected, fail, &cause, "FINITE");
            assert_eq!(
                *trace.borrow(),
                vec!["string"],
                "FINITE native coercion count"
            );
            input["finite"]
                .as_array_mut()
                .unwrap()
                .push(json!({"kind":kind,"fail":fail}));
            native["finite"].as_array_mut().unwrap().push(outcome);
        }
    }
    let currencies = [
        ("missing", false, ""),
        ("null", false, ""),
        ("false", false, ""),
        ("zero", false, ""),
        ("nan", false, ""),
        ("empty", false, ""),
        ("lower", true, "BTC"),
        ("object", true, "ETH"),
        ("expansion", true, "SS"),
        ("surrogate", true, ""),
    ];
    for (kind, truthy, expected) in currencies {
        for fail in ["", "read", "coerce"] {
            let cause = error();
            let trace = RefCell::new(vec![]);
            let passed = RefCell::new(None);
            let expected = if kind == "surrogate" {
                vec![0xd800, 66, 84, 67]
            } else {
                units(expected)
            };
            let actual: Result<Vec<u16>, Error> = get_contract_currency(
                || {
                    call(&trace, "read", fail, &cause)?;
                    Ok(Some(kind))
                },
                |_| truthy,
                |v| {
                    *passed.borrow_mut() = Some(v);
                    call(&trace, "coerce", fail, &cause)?;
                    Ok(expected.clone())
                },
            );
            let mut expected_trace = vec!["read"];
            if fail != "read" {
                expected_trace.push("coerce");
            }
            assert_eq!(
                *trace.borrow(),
                expected_trace,
                "CURRENCY native read/coercion order"
            );
            if fail != "read" {
                assert_eq!(
                    *passed.borrow(),
                    Some(if truthy { Some(kind) } else { None }),
                    "CURRENCY native truthy fallback"
                );
            }
            let outcome = if fail.is_empty() {
                let v = actual.unwrap();
                assert_eq!(v, expected, "CURRENCY native UTF16 units");
                json!({"units":v})
            } else {
                let e = actual.unwrap_err();
                assert_eq!(*e, *cause, "CURRENCY native error fields");
                assert!(Rc::ptr_eq(&e, &cause), "CURRENCY native error identity");
                json!({"error":true})
            };
            input["currency"]
                .as_array_mut()
                .unwrap()
                .push(json!({"kind":kind,"fail":fail}));
            native["currency"]
                .as_array_mut()
                .unwrap()
                .push(json!({"trace":trace.into_inner(),"outcome":outcome}));
        }
    }
    for direction in ["usd", "margin"] {
        for scenario in 0..14 {
            let has_market = scenario != 0;
            let currency = match scenario {
                1 => None,
                2 => Some(vec![]),
                13 => Some(units("ß")),
                _ => Some(units("btc")),
            };
            let inverse = scenario != 3;
            let matches = scenario != 5;
            let price = match scenario {
                6 => 0.0,
                7 => -0.0,
                8 => f64::NAN,
                9 => -5.0,
                10 => f64::INFINITY,
                _ => 5.0,
            };
            let amount = match scenario {
                11 => -0.0,
                12 => 0.0,
                _ => 6.0,
            };
            let reaches = scenario >= 4 && matches;
            let expected = if reaches && (price == 0.0 || price.is_nan()) {
                0.0
            } else if reaches {
                if direction == "usd" {
                    amount * price
                } else {
                    amount / price
                }
            } else {
                amount
            };
            let full_trace: Vec<&'static str> = if scenario <= 2 {
                vec!["finite"]
            } else if scenario == 3 {
                vec!["finite", "currency", "inverse"]
            } else if !matches {
                vec!["finite", "currency", "inverse", "base"]
            } else {
                vec!["finite", "currency", "inverse", "base", "price"]
            };
            for fail in ["", "finite", "currency", "inverse", "base", "price"] {
                let cause = error();
                let trace = RefCell::new(vec![]);
                let f = || {
                    call(&trace, "finite", fail, &cause)?;
                    Ok(amount)
                };
                let c = |_: &[u16]| {
                    call(&trace, "currency", fail, &cause)?;
                    Ok(units(if scenario == 13 { "SS" } else { "BTC" }))
                };
                let i = || {
                    call(&trace, "inverse", fail, &cause)?;
                    Ok(inverse)
                };
                let b = || {
                    call(&trace, "base", fail, &cause)?;
                    Ok(units(if !matches {
                        "ETH"
                    } else if scenario == 13 {
                        "SS"
                    } else {
                        "BTC"
                    }))
                };
                let p = || {
                    call(&trace, "price", fail, &cause)?;
                    Ok(price)
                };
                let actual = if direction == "usd" {
                    margin_to_usd(f, has_market, currency.as_deref(), c, i, b, p)
                } else {
                    usd_to_margin(f, has_market, currency.as_deref(), c, i, b, p)
                };
                let stop = full_trace.iter().position(|s| *s == fail);
                let expected_trace = &full_trace[..stop.map_or(full_trace.len(), |n| n + 1)];
                assert_eq!(
                    *trace.borrow(),
                    expected_trace,
                    "MARGIN native getter/guard order"
                );
                let outcome = observe(
                    actual,
                    expected,
                    stop.is_some(),
                    &cause,
                    if direction == "usd" {
                        "MARGIN-USD"
                    } else {
                        "USD-MARGIN"
                    },
                );
                input["margin"].as_array_mut().unwrap().push(json!({"direction":direction,"scenario":scenario,"fail":fail,"amount":bits(amount),"price":bits(price)}));
                native["margin"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"trace":trace.into_inner(),"outcome":outcome}));
            }
        }
    }
    let base = [
        (2.0, Some(3.0), false, true, 4.0, 6.0),
        (0.01, Some(0.01), false, true, 4.0, 0.0001),
        (2.0, None, false, true, 4.0, 2.0),
        (2.0, Some(0.0), false, true, 4.0, 2.0),
        (2.0, Some(f64::NAN), false, true, 4.0, 2.0),
        (2.0, Some(3.0), true, true, 4.0, 1.5),
        (2.0, Some(3.0), true, true, 0.0, 0.0),
        (2.0, Some(3.0), true, true, -4.0, -1.5),
        (2.0, Some(3.0), true, false, 4.0, 6.0),
        (-0.0, Some(3.0), true, true, 4.0, -0.0),
        (2.0, Some(f64::INFINITY), false, true, 4.0, f64::INFINITY),
    ];
    for (scenario, (contracts, size, inverse, matches, price, expected)) in
        base.into_iter().enumerate()
    {
        let full_trace = if !inverse {
            vec!["size", "ct", "inverse"]
        } else if !matches {
            vec!["size", "ct", "inverse", "quote"]
        } else {
            vec!["size", "ct", "inverse", "quote", "price"]
        };
        for fail in ["", "size", "ct", "inverse", "quote", "price"] {
            let trace = RefCell::new(vec![]);
            let cause = error();
            let actual = contracts_to_base(
                contracts,
                || {
                    call(&trace, "size", fail, &cause)?;
                    Ok(size)
                },
                || {
                    call(&trace, "ct", fail, &cause)?;
                    Ok(units("USD"))
                },
                || {
                    call(&trace, "inverse", fail, &cause)?;
                    Ok(inverse)
                },
                || {
                    call(&trace, "quote", fail, &cause)?;
                    Ok(units(if matches { "USD" } else { "BTC" }))
                },
                || {
                    call(&trace, "price", fail, &cause)?;
                    Ok(price)
                },
            );
            let stop = full_trace.iter().position(|s| *s == fail);
            let expected_trace = &full_trace[..stop.map_or(full_trace.len(), |n| n + 1)];
            assert_eq!(
                *trace.borrow(),
                expected_trace,
                "BASE native lazy getter order"
            );
            let outcome = observe(actual, expected, stop.is_some(), &cause, "BASE");
            input["base"].as_array_mut().unwrap().push(json!({"scenario":scenario,"contracts":bits(contracts),"size":size.map(bits),"inverse":inverse,"matches":matches,"price":bits(price),"fail":fail}));
            native["base"]
                .as_array_mut()
                .unwrap()
                .push(json!({"trace":trace.into_inner(),"outcome":outcome}));
        }
    }
    assert_eq!(
        input["finite"].as_array().unwrap().len(),
        24,
        "FINITE independent floor"
    );
    assert_eq!(
        input["currency"].as_array().unwrap().len(),
        30,
        "CURRENCY independent floor"
    );
    assert_eq!(
        input["margin"].as_array().unwrap().len(),
        168,
        "MARGIN independent floor"
    );
    assert_eq!(
        input["base"].as_array().unwrap().len(),
        66,
        "BASE independent floor"
    );
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/margin-conversion-oracle.mjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&input).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        native, source,
        "source/wire comparison after all native assertions"
    );
}
