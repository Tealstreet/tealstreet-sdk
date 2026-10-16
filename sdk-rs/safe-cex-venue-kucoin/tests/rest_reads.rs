use safe_cex_venue_kucoin::rest_reads as port;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Debug, PartialEq)]
enum Native {
    Undefined,
    Null,
    Text(String),
    Number(f64),
    Handle(&'static str),
    Object(Vec<(&'static str, V)>),
}
type V = Rc<Native>;
#[derive(Debug)]
struct Failure {
    name: &'static str,
    message: String,
    code: &'static str,
    ids: Vec<u8>,
    response: V,
}
type E = Rc<Failure>;
type Trace = Rc<RefCell<Vec<String>>>;
type F = Pin<Box<dyn Future<Output = Result<V, E>>>>;
fn run<T>(future: impl Future<Output = T>) -> T {
    let w = Waker::noop().clone();
    let mut c = Context::from_waker(&w);
    let mut f = Box::pin(future);
    match f.as_mut().poll(&mut c) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture await handle unexpectedly pending"),
    }
}
fn text(s: &str) -> V {
    Rc::new(Native::Text(s.into()))
}
fn undef() -> V {
    Rc::new(Native::Undefined)
}
fn number(n: f64) -> V {
    Rc::new(Native::Number(n))
}
fn object(fields: Vec<(&'static str, V)>) -> V {
    Rc::new(Native::Object(fields))
}
fn truthy(v: &V) -> bool {
    match v.as_ref() {
        Native::Undefined | Native::Null => false,
        Native::Number(n) => *n != 0.0 && !n.is_nan(),
        Native::Text(s) => !s.is_empty(),
        _ => true,
    }
}
fn equal_string(v: &V, s: &str) -> bool {
    matches!(v.as_ref(),Native::Text(t) if t==s)
}
fn field(v: &V, k: &str) -> V {
    let Native::Object(fields) = v.as_ref() else {
        panic!("expected native object")
    };
    fields.iter().find(|(key, _)| *key == k).unwrap().1.clone()
}
fn keys(v: &V) -> Vec<&'static str> {
    let Native::Object(fields) = v.as_ref() else {
        panic!("expected native object")
    };
    fields.iter().map(|(k, _)| *k).collect()
}
fn push(t: &Trace, s: impl Into<String>) {
    t.borrow_mut().push(s.into());
}
fn failure() -> E {
    Rc::new(Failure {
        name: "TypeError",
        message: "transport".into(),
        code: "READ",
        ids: vec![4, 7],
        response: Rc::new(Native::Handle("error-response")),
    })
}
fn original(e: &E, expected: &E) {
    assert!(
        Rc::ptr_eq(e, expected),
        "READ native original error identity"
    );
    assert_eq!(e.name, "TypeError");
    assert_eq!(e.message, "transport");
    assert_eq!(e.code, "READ");
    assert_eq!(e.ids, [4, 7]);
    assert!(Rc::ptr_eq(&e.response, &expected.response));
}
fn ready(value: Result<V, E>, events: &Trace, label: &str) -> F {
    let t = events.clone();
    let label = label.to_string();
    Box::pin(async move {
        push(&t, format!("settle:{label}"));
        value
    })
}
fn pack(v: &V) -> Value {
    match v.as_ref() {
        Native::Undefined => json!({"u":true}),
        Native::Null => Value::Null,
        Native::Text(s) => json!({"s":s}),
        Native::Number(n) => {
            if n.is_nan() {
                json!({"nan":true})
            } else {
                json!({"n":format!("{:016x}",n.to_bits())})
            }
        }
        Native::Handle(tag) => json!({"h":tag}),
        Native::Object(f) => {
            json!({"o":f.iter().map(|(k,v)|json!([k,pack(v)])).collect::<Vec<_>>()})
        }
    }
}
fn result(outcome: &Result<V, E>, expected_error: &E) -> Value {
    match outcome {
        Ok(v) => json!({"value":pack(v)}),
        Err(e) => {
            let same = Rc::ptr_eq(e, expected_error);
            json!({"error":{"original":same,"name":e.name,"message":e.message,"code":if same{Some(e.code)}else{None},"ids":e.ids}})
        }
    }
}
fn validator(kind: &str, scenario: &str) -> Value {
    let events = Trace::default();
    let err = failure();
    let balance = Rc::new(Native::Handle("balance"));
    let returned = balance.clone();
    let response = Rc::new(Native::Handle("response"));
    let data = Rc::new(Native::Handle("data"));
    let emitter = Rc::new(Native::Handle("emitter"));
    let errors = Rc::new(RefCell::new(Vec::<V>::new()));
    let message_reads = Cell::new(0);
    let changed = Cell::new(false);
    let request = |route: &str| {
        assert_eq!(
            route,
            if kind == "okx" {
                "BALANCE"
            } else if kind == "eu" {
                "ACCOUNT_CONFIG"
            } else {
                "/api/v1/account-overview"
            },
            "VALIDATE native route"
        );
        push(&events, "request");
        if scenario == "request-sync" {
            Err(err.clone())
        } else {
            Ok(ready(
                if matches!(
                    scenario,
                    "request-reject"
                        | "emit-error"
                        | "error-read-error"
                        | "message-changing"
                        | "empty-error"
                        | "emitter-reenter"
                ) {
                    Err(err.clone())
                } else {
                    Ok(response.clone())
                },
                &events,
                "request",
            ))
        }
    };
    let fetch = || {
        push(&events, "balance");
        if scenario == "balance-sync" {
            Err(err.clone())
        } else {
            Ok(ready(
                if scenario == "balance-reject" {
                    Err(err.clone())
                } else {
                    Ok(returned)
                },
                &events,
                "balance",
            ))
        }
    };
    let capture = || {
        push(&events, "capture-emitter");
        if scenario == "emitter-reenter" {
            changed.set(true);
        }
        Ok(emitter.clone())
    };
    let invoke = |actual: V, message: V| {
        assert!(
            Rc::ptr_eq(&actual, &emitter),
            "VALIDATE native captured emitter identity"
        );
        push(&events, "emit");
        errors.borrow_mut().push(message);
        if scenario == "emit-error" {
            Err(err.clone())
        } else {
            Ok(())
        }
    };
    let error_value = |actual: &E, expression: &str| {
        original(actual, &err);
        if expression == "message" {
            push(&events, "error-message");
            return Ok(if scenario == "empty-error" {
                text("")
            } else {
                text("transport")
            });
        }
        push(&events, "error-response");
        let n = message_reads.get();
        message_reads.set(n + 1);
        if scenario == "error-read-error" {
            return Err(err.clone());
        }
        if scenario == "empty-error" {
            push(&events, "error-message");
            Ok(text(""))
        } else {
            Ok(text(
                if changed.get() || (scenario == "message-changing" && n > 0) {
                    "changed"
                } else {
                    "down"
                },
            ))
        }
    };
    let outcome = {
        let read = |value: &V, key: &str| {
            push(&events, format!("read:{key}"));
            if Rc::ptr_eq(value, &response) {
                assert_eq!(key, "data");
                Ok(data.clone())
            } else {
                assert!(Rc::ptr_eq(value, &data));
                Ok(if key == "code" {
                    text(if matches!(scenario, "api-code-error" | "api-code-empty") {
                        "500000"
                    } else {
                        "200000"
                    })
                } else if scenario == "api-code-empty" {
                    text("")
                } else {
                    text("down")
                })
            }
        };
        let catch = |promise: F, handler: Rc<dyn Fn(E) -> V>| {
            push(&events, "catch");
            let t = events.clone();
            Ok::<F, E>(Box::pin(async move {
                match promise.await {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        push(&t, "handler");
                        Ok(handler(e))
                    }
                }
            }))
        };
        run(port::validate_account(
            request,
            read,
            equal_string,
            truthy,
            fetch,
            catch,
            capture,
            invoke,
            |e| error_value(e, "response-or-message"),
            text,
            undef,
            object,
        ))
    };
    let emitted = matches!(
        scenario,
        "request-sync"
            | "request-reject"
            | "emit-error"
            | "error-read-error"
            | "message-changing"
            | "empty-error"
            | "emitter-reenter"
    ) || (kind == "kucoin"
        && matches!(
            scenario,
            "balance-sync" | "api-code-error" | "api-code-empty"
        ));
    let throws = matches!(scenario, "emit-error" | "error-read-error");
    assert_eq!(outcome.is_err(), throws, "VALIDATE native failure scope");
    if let Err(actual) = &outcome {
        original(actual, &err);
    } else {
        let value = outcome.as_ref().unwrap();
        let has_balance = kind != "eu" && !emitted;
        assert_eq!(
            keys(value),
            if has_balance {
                vec!["error", "balance"]
            } else {
                vec!["error"]
            },
            "VALIDATE native field order"
        );
        let expected = if !emitted {
            ""
        } else if scenario == "empty-error" {
            if kind == "kucoin" {
                "Unknown error validating account"
            } else {
                ""
            }
        } else if kind == "kucoin" && scenario == "api-code-empty" {
            "Failed to validate account"
        } else if scenario == "message-changing" && kind != "kucoin"
            || scenario == "emitter-reenter" && kind != "kucoin"
        {
            "changed"
        } else {
            "down"
        };
        assert_eq!(
            field(value, "error"),
            text(expected),
            "VALIDATE native returned error value"
        );
        if has_balance {
            let actual = field(value, "balance");
            if matches!(scenario, "balance-reject" | "balance-sync") {
                assert_eq!(actual, undef(), "VALIDATE native best effort balance")
            } else {
                assert!(
                    Rc::ptr_eq(&actual, &balance),
                    "VALIDATE native balance identity"
                );
            }
        }
    }
    let mut expected = vec!["request".to_string()];
    if scenario != "request-sync" {
        expected.push("settle:request".into());
    }
    if kind == "kucoin"
        && !matches!(
            scenario,
            "request-sync"
                | "request-reject"
                | "emit-error"
                | "error-read-error"
                | "message-changing"
                | "empty-error"
                | "emitter-reenter"
        )
    {
        expected.extend(["read:data", "read:code"].map(str::to_string));
        if matches!(scenario, "api-code-error" | "api-code-empty") {
            expected.extend(["read:msg", "capture-emitter", "emit"].map(str::to_string));
        }
    }
    if emitted && !matches!(scenario, "api-code-error" | "api-code-empty") {
        if kind == "kucoin" && scenario == "balance-sync" {
            expected.push("balance".into());
        }
        if kind == "kucoin" {
            expected.push("error-response".into());
            if scenario == "empty-error" {
                expected.push("error-message".into());
            }
            if scenario != "error-read-error" {
                expected.extend(["capture-emitter", "emit"].map(str::to_string));
            }
        } else {
            expected.extend(["capture-emitter", "error-response"].map(str::to_string));
            if scenario == "empty-error" {
                expected.push("error-message".into());
            }
            if scenario != "error-read-error" {
                expected.push("emit".into());
                if scenario != "emit-error" {
                    expected.push("error-response".into());
                    if scenario == "empty-error" {
                        expected.extend(["error-message", "error-message"].map(str::to_string));
                    }
                }
            }
        }
    } else if !emitted && kind != "eu" {
        expected.push("balance".into());
        if kind == "kucoin" {
            expected.push("catch".into());
        }
        if scenario != "balance-sync" {
            expected.push("settle:balance".into());
        }
        if kind == "kucoin" && scenario == "balance-reject" {
            expected.push("handler".into());
        }
    }
    assert_eq!(
        *events.borrow(),
        expected,
        "VALIDATE native independent capture await trace"
    );
    {
        let messages = errors.borrow();
        assert_eq!(
            messages.len(),
            if emitted && scenario != "error-read-error" {
                1
            } else {
                0
            },
            "VALIDATE native independent emission count"
        );
        if emitted && scenario != "error-read-error" {
            assert_eq!(
                messages[0],
                text(if scenario == "empty-error" {
                    if kind == "kucoin" {
                        "Unknown error validating account"
                    } else {
                        ""
                    }
                } else if scenario == "api-code-empty" {
                    "Failed to validate account"
                } else if scenario == "emitter-reenter" && kind != "kucoin" {
                    "changed"
                } else {
                    "down"
                }),
                "VALIDATE native emitted error value"
            );
        }
    }
    json!({"kind":kind,"scenario":scenario,"events":events.borrow().clone(),"messages":errors.borrow().iter().map(pack).collect::<Vec<_>>(),"result":result(&outcome,&err)})
}
fn balance(scenario: &str) -> Value {
    let events = Trace::default();
    let err = failure();
    let response = Rc::new(Native::Handle("response"));
    let data = Rc::new(Native::Handle("data"));
    let account = Rc::new(Native::Handle("account"));
    let stored = Rc::new(Native::Handle("stored-balance"));
    let currency = Rc::new(Native::Handle("currency"));
    let callee = object(vec![
        ("receiver", Rc::new(Native::Handle("callee"))),
        ("url", text("/api/v1/account-overview")),
    ]);
    let changed_url = Cell::new(false);
    let emitter = Rc::new(Native::Handle("emitter"));
    let messages = Rc::new(RefCell::new(Vec::<V>::new()));
    let capture = |route: &str| {
        assert_eq!(route, "BALANCE", "BALANCE native route");
        push(&events, "capture-request");
        push(&events, "endpoint");
        assert!(
            !changed_url.get(),
            "BALANCE native endpoint captured before currency"
        );
        Ok(callee.clone())
    };
    let coin = || {
        push(&events, "currency");
        if scenario == "currency-reenter" {
            changed_url.set(true);
        }
        if scenario == "currency-error" {
            Err(err.clone())
        } else {
            Ok(currency.clone())
        }
    };
    let invoke = |actual: V, options: V| {
        assert!(
            Rc::ptr_eq(&actual, &callee),
            "BALANCE native bound callee identity"
        );
        assert_eq!(
            keys(&options),
            ["params"],
            "BALANCE native option key order"
        );
        assert_eq!(
            field(&actual, "url"),
            text("/api/v1/account-overview"),
            "BALANCE native captured endpoint value"
        );
        assert_eq!(changed_url.get(), scenario == "currency-reenter");
        let params = field(&options, "params");
        assert_eq!(keys(&params), ["currency"]);
        assert!(
            Rc::ptr_eq(&field(&params, "currency"), &currency),
            "BALANCE native currency identity"
        );
        push(&events, "request");
        if scenario == "request-sync" {
            Err(err.clone())
        } else {
            Ok(ready(
                if scenario == "request-reject" {
                    Err(err.clone())
                } else {
                    Ok(response.clone())
                },
                &events,
                "request",
            ))
        }
    };
    let read = |value: &V, key: &str| {
        push(&events, format!("read:{key}"));
        if Rc::ptr_eq(value, &response) {
            assert_eq!(key, "data");
            Ok(data.clone())
        } else {
            assert!(Rc::ptr_eq(value, &data));
            Ok(match key {
                "code" => {
                    if scenario == "numeric-code" {
                        number(200000.0)
                    } else {
                        text(if matches!(scenario, "bad-code" | "emit-error") {
                            "500000"
                        } else {
                            "200000"
                        })
                    }
                }
                "msg" => text(""),
                "data" => {
                    if scenario == "missing-data" {
                        Rc::new(Native::Null)
                    } else {
                        account.clone()
                    }
                }
                _ => panic!("read"),
            })
        }
    };
    let cap_emit = || {
        push(&events, "capture-emitter");
        Ok(emitter.clone())
    };
    let emit = |actual: V, msg: V| {
        assert!(Rc::ptr_eq(&actual, &emitter));
        push(&events, "emit");
        messages.borrow_mut().push(msg);
        if scenario == "emit-error" {
            Err(err.clone())
        } else {
            Ok(())
        }
    };
    let store = || {
        push(&events, "store-balance");
        Ok(stored.clone())
    };
    let parse = |value: &V, key: &str, fallback: &str| {
        assert!(Rc::ptr_eq(value, &account));
        assert_eq!(fallback, "0");
        push(&events, "capture-parseFloat");
        push(&events, format!("field:{key}"));
        if scenario == "parse-error" && key == "orderMargin" {
            return Err(err.clone());
        }
        let s = if scenario == "zero-values" {
            "0"
        } else {
            match key {
                "availableBalance" => "1.25tail",
                "orderMargin" => "-2.5",
                "accountEquity" => "wat",
                "unrealisedPNL" => "-0",
                _ => panic!("parse"),
            }
        };
        Ok(number(safe_cex_num::parse_float(s)))
    };
    let outcome = run(port::fetch_balance(
        capture,
        coin,
        invoke,
        read,
        equal_string,
        truthy,
        cap_emit,
        emit,
        store,
        parse,
        text,
        object,
    ));
    let throws = matches!(
        scenario,
        "request-sync" | "request-reject" | "currency-error" | "parse-error" | "emit-error"
    );
    let old = matches!(scenario, "missing-data" | "bad-code" | "numeric-code");
    assert_eq!(outcome.is_err(), throws, "BALANCE native failure boundary");
    match &outcome {
        Err(e) => original(e, &err),
        Ok(v) => {
            if old {
                assert!(
                    Rc::ptr_eq(v, &stored),
                    "BALANCE native original store identity"
                )
            } else {
                assert_eq!(
                    keys(v),
                    ["free", "used", "total", "upnl"],
                    "BALANCE native decoded key order"
                );
                for (key, expected) in [
                    ("free", 1.25),
                    ("used", -2.5),
                    ("total", f64::NAN),
                    ("upnl", -0.0),
                ] {
                    let actual = field(v, key);
                    let Native::Number(n) = actual.as_ref() else {
                        panic!("number")
                    };
                    if scenario == "zero-values" {
                        assert_eq!(n.to_bits(), 0.0f64.to_bits(), "BALANCE native zero default");
                    } else if expected.is_nan() {
                        assert!(n.is_nan(), "BALANCE native NaN retained");
                    } else {
                        assert_eq!(
                            n.to_bits(),
                            expected.to_bits(),
                            "BALANCE native independent numeric bits"
                        );
                    }
                }
            }
        }
    };
    let mut expected = vec![
        "capture-request".to_string(),
        "endpoint".into(),
        "currency".into(),
    ];
    if scenario != "currency-error" {
        expected.push("request".into());
        if scenario != "request-sync" {
            expected.push("settle:request".into());
            if scenario != "request-reject" {
                expected.extend(["read:data", "read:code"].map(str::to_string));
                if matches!(scenario, "bad-code" | "numeric-code" | "emit-error") {
                    expected.extend(["capture-emitter", "read:msg", "emit"].map(str::to_string));
                    if scenario != "emit-error" {
                        expected.push("store-balance".into());
                    }
                } else {
                    expected.push("read:data".into());
                    if scenario == "missing-data" {
                        expected.push("store-balance".into());
                    } else {
                        for key in [
                            "availableBalance",
                            "orderMargin",
                            "accountEquity",
                            "unrealisedPNL",
                        ] {
                            expected.push("capture-parseFloat".into());
                            expected.push(format!("field:{key}"));
                            if scenario == "parse-error" && key == "orderMargin" {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        *events.borrow(),
        expected,
        "BALANCE native independent request parse trace"
    );
    assert_eq!(
        messages.borrow().len(),
        usize::from(matches!(
            scenario,
            "bad-code" | "numeric-code" | "emit-error"
        )),
        "BALANCE native emission floor"
    );
    for m in messages.borrow().iter() {
        assert_eq!(
            m,
            &text("Failed to fetch balance"),
            "BALANCE native emitted fallback"
        );
    }
    json!({"scenario":scenario,"events":events.borrow().clone(),"messages":messages.borrow().iter().map(pack).collect::<Vec<_>>(),"result":result(&outcome,&err)})
}

#[test]
fn rest_reads_native_before_source() {
    let mut validators = vec![];
    let mut balances = vec![];
    for scenario in [
        "normal",
        "request-sync",
        "request-reject",
        "balance-sync",
        "balance-reject",
        "emit-error",
        "error-read-error",
        "message-changing",
        "empty-error",
        "api-code-error",
        "api-code-empty",
    ] {
        validators.push(validator("kucoin", scenario));
    }
    for scenario in [
        "normal",
        "zero-values",
        "currency-reenter",
        "missing-data",
        "bad-code",
        "numeric-code",
        "request-sync",
        "request-reject",
        "currency-error",
        "parse-error",
        "emit-error",
    ] {
        balances.push(balance(scenario));
    }
    assert_eq!(validators.len(), 11);
    assert_eq!(balances.len(), 11);
    oracle(json!({"venue":"kucoin","validators":validators,"balances":balances}));
}
fn oracle(input: Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rest-reads-oracle.mjs");
    let mut child = Command::new("node")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        input,
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        "maintained source after all native assertions"
    );
}
