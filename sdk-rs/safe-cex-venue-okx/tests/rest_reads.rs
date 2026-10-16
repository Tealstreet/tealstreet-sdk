use safe_cex_venue_okx::rest_reads as port;
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
    Bool(bool),
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
        Native::Undefined => false,
        Native::Bool(b) => *b,
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
        Native::Bool(b) => json!(b),
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
    let outcome = if kind == "okx" {
        run(port::validate_account(
            request,
            fetch,
            capture,
            error_value,
            invoke,
            truthy,
            text,
            undef,
            object,
        ))
    } else {
        run(port::validate_eu_account(
            request,
            capture,
            error_value,
            invoke,
            truthy,
            text,
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
fn delegation(method: &str, scenario: &str) -> Value {
    let events = Trace::default();
    let err = failure();
    let symbols = Rc::new(Native::Handle("symbols"));
    let response = Rc::new(Native::Handle("response"));
    let returned = Rc::new(Native::Handle("returned"));
    let fetch = |name: &str| {
        assert_eq!(
            name,
            match method {
                "fetchPositions" => "fetchBalanceAndPositions",
                "fetchBalance" => "fetchBalanceAndOpenPositions",
                _ => "fetchTickersRaw",
            },
            "DELEGATE native method selection"
        );
        push(&events, format!("call:{name}"));
        if scenario == "sync-error" {
            Err(err.clone())
        } else {
            Ok(ready(
                if scenario == "reject" {
                    Err(err.clone())
                } else {
                    Ok(response.clone())
                },
                &events,
                "read",
            ))
        }
    };
    let read = |value: V, key: &str| {
        assert!(
            Rc::ptr_eq(&value, &response),
            "DELEGATE native original response identity"
        );
        assert_eq!(
            key,
            if method == "fetchPositions" {
                "positions"
            } else {
                "balance"
            },
            "DELEGATE native property selection"
        );
        push(&events, format!("read:{key}"));
        if scenario == "result-error" {
            Err(err.clone())
        } else {
            Ok(returned.clone())
        }
    };
    let outcome = if method == "fetchPositions" {
        run(port::fetch_positions(
            symbols.clone(),
            |name, arg| {
                assert!(
                    Rc::ptr_eq(&arg, &symbols),
                    "DELEGATE native symbols identity"
                );
                fetch(name)
            },
            read,
        ))
    } else if method == "fetchBalance" {
        run(port::fetch_balance(fetch, read))
    } else {
        run(port::fetch_tickers(fetch, |value| {
            assert!(
                Rc::ptr_eq(&value, &response),
                "DELEGATE native raw identity"
            );
            push(&events, "parse");
            if scenario == "result-error" {
                Err(err.clone())
            } else {
                Ok(returned.clone())
            }
        }))
    };
    assert_eq!(
        outcome.is_err(),
        scenario != "normal",
        "DELEGATE native failure boundary"
    );
    match &outcome {
        Err(e) => original(e, &err),
        Ok(v) => assert!(Rc::ptr_eq(v, &returned), "DELEGATE native return identity"),
    };
    let mut expected = vec![format!(
        "call:{}",
        match method {
            "fetchPositions" => "fetchBalanceAndPositions",
            "fetchBalance" => "fetchBalanceAndOpenPositions",
            _ => "fetchTickersRaw",
        }
    )];
    if scenario != "sync-error" {
        expected.push("settle:read".into());
        if scenario != "reject" {
            expected.push(if method == "fetchTickers" {
                "parse".into()
            } else {
                format!(
                    "read:{}",
                    if method == "fetchPositions" {
                        "positions"
                    } else {
                        "balance"
                    }
                )
            });
        }
    }
    assert_eq!(
        *events.borrow(),
        expected,
        "DELEGATE native independent await trace"
    );
    json!({"method":method,"scenario":scenario,"events":events.borrow().clone(),"result":result(&outcome,&err)})
}
fn config(scenario: &str) -> Value {
    let events = Trace::default();
    let err = failure();
    let errors = Rc::new(RefCell::new(Vec::<V>::new()));
    let mode = Rc::new(Native::Handle("cached-mode"));
    let level = Rc::new(Native::Handle("cached-level"));
    let fresh_mode = Rc::new(Native::Handle("fresh-mode"));
    let fresh_level = Rc::new(Native::Handle("fresh-level"));
    let cached = matches!(scenario, "cached" | "cached-changing" | "forced");
    let values = Rc::new(RefCell::new((
        if cached { mode.clone() } else { undef() },
        if cached { level.clone() } else { number(0.0) },
    )));
    let mode_reads = Cell::new(0);
    let level_reads = Cell::new(0);
    let changed = Cell::new(false);
    let cache = |key: &str| {
        push(&events, format!("cache:{key}"));
        if scenario == "getter-error" {
            return Err(err.clone());
        }
        let index = if key == "positionMode" {
            &mode_reads
        } else {
            &level_reads
        };
        let n = index.get();
        index.set(n + 1);
        if scenario == "cached-changing" && n > 0 {
            Ok(if key == "positionMode" {
                fresh_mode.clone()
            } else {
                fresh_level.clone()
            })
        } else {
            Ok(if key == "positionMode" {
                values.borrow().0.clone()
            } else {
                values.borrow().1.clone()
            })
        }
    };
    let request = |route: &str| {
        assert_eq!(route, "ACCOUNT_CONFIG", "CONFIG native endpoint");
        push(&events, "request");
        if scenario == "request-sync" {
            Err(err.clone())
        } else {
            Ok(ready(
                if matches!(scenario, "request-reject" | "emitter-reenter") {
                    Err(err.clone())
                } else {
                    Ok(Rc::new(Native::Handle("response")))
                },
                &events,
                "request",
            ))
        }
    };
    let decode = |response: V| {
        assert_eq!(*response, Native::Handle("response"));
        push(&events, "decode");
        if scenario == "decode-error" {
            Err(err.clone())
        } else {
            Ok((
                text(if scenario == "unknown-mode" {
                    "net_mode"
                } else {
                    "long_short_mode"
                }),
                if scenario == "zero-level" {
                    number(0.0)
                } else {
                    text(if scenario == "level-nan" {
                        "wat"
                    } else {
                        "3.5"
                    })
                },
            ))
        }
    };
    let assign = |key: &str, value: V| {
        push(&events, format!("assign:{key}"));
        if scenario == "assign-error" && key == "accountLevel" {
            return Err(err.clone());
        }
        if key == "positionMode" {
            values.borrow_mut().0 = value;
        } else {
            values.borrow_mut().1 = value;
        }
        Ok(())
    };
    let parse = |value: V| {
        push(&events, "parse");
        let Native::Text(s) = value.as_ref() else {
            panic!("parse input")
        };
        assert_eq!(
            s,
            if scenario == "zero-level" {
                "2"
            } else if scenario == "level-nan" {
                "wat"
            } else {
                "3.5"
            },
            "CONFIG native parse argument fallback"
        );
        Ok(number(safe_cex_num::parse_float(s)))
    };
    let emitter = Rc::new(Native::Handle("emitter"));
    let constructor = Rc::new(Native::Handle("Error"));
    let capture = |key: &str| {
        push(&events, format!("capture:{key}"));
        if scenario == "emitter-reenter" && key == "emitAndLogError" {
            changed.set(true);
        }
        Ok(if key == "Error" {
            constructor.clone()
        } else {
            emitter.clone()
        })
    };
    let error_value = |e: &E| {
        original(e, &err);
        push(&events, "error-value");
        Ok(text(if changed.get() { "changed" } else { "down" }))
    };
    let emit = |actual: V, msg: V| {
        assert!(Rc::ptr_eq(&actual, &emitter));
        push(&events, "emit");
        errors.borrow_mut().push(msg);
        Ok(())
    };
    let new = |actual: V, msg: V| {
        assert!(
            Rc::ptr_eq(&actual, &constructor),
            "CONFIG native captured Error constructor"
        );
        push(&events, "new-error");
        let Native::Text(message) = msg.as_ref() else {
            panic!("error msg")
        };
        Ok(Rc::new(Failure {
            name: "Error",
            message: message.clone(),
            code: "",
            ids: vec![],
            response: undef(),
        }))
    };
    let force = if scenario == "forced" {
        Some(Rc::new(Native::Bool(true)))
    } else {
        None
    };
    let outcome = run(port::get_or_fetch_position_mode_and_account_level(
        force,
        |b| Rc::new(Native::Bool(b)),
        cache,
        truthy,
        request,
        decode,
        equal_string,
        assign,
        parse,
        text,
        object,
        capture,
        error_value,
        emit,
        new,
    ));
    let throws = matches!(
        scenario,
        "request-sync"
            | "request-reject"
            | "decode-error"
            | "assign-error"
            | "getter-error"
            | "emitter-reenter"
    );
    assert_eq!(outcome.is_err(), throws, "CONFIG native catch boundary");
    match &outcome {
        Err(actual) => {
            if scenario == "getter-error" {
                original(actual, &err);
            } else {
                assert!(
                    !Rc::ptr_eq(actual, &err),
                    "CONFIG native fresh Error identity"
                );
                assert_eq!(actual.name, "Error");
                assert_eq!(
                    actual.message,
                    if scenario == "emitter-reenter" {
                        "changed"
                    } else {
                        "down"
                    }
                );
                assert_eq!(actual.ids, Vec::<u8>::new());
                assert_eq!(actual.code, "");
            }
        }
        Ok(value) => {
            assert_eq!(
                keys(value),
                ["positionMode", "accountLevel"],
                "CONFIG native object key order"
            );
            if matches!(scenario, "cached" | "cached-changing") {
                assert!(
                    Rc::ptr_eq(
                        &field(value, "positionMode"),
                        if scenario == "cached" {
                            &mode
                        } else {
                            &fresh_mode
                        }
                    ),
                    "CONFIG native live mode identity"
                );
                assert!(
                    Rc::ptr_eq(
                        &field(value, "accountLevel"),
                        if scenario == "cached" {
                            &level
                        } else {
                            &fresh_level
                        }
                    ),
                    "CONFIG native live level identity"
                );
            } else {
                assert_eq!(
                    field(value, "positionMode"),
                    text(if scenario == "unknown-mode" {
                        "oneway"
                    } else {
                        "hedged"
                    }),
                    "CONFIG native decoded mode"
                );
                let actual = field(value, "accountLevel");
                let Native::Number(n) = actual.as_ref() else {
                    panic!("level")
                };
                if scenario == "level-nan" {
                    assert!(n.is_nan(), "CONFIG native NaN retained");
                } else {
                    assert_eq!(
                        n.to_bits(),
                        if scenario == "zero-level" {
                            2.0f64.to_bits()
                        } else {
                            3.5f64.to_bits()
                        },
                        "CONFIG native parsed level bits"
                    );
                }
            }
        }
    }
    if scenario == "assign-error" {
        assert_eq!(
            values.borrow().0,
            text("hedged"),
            "CONFIG native partial cache update"
        );
        assert_eq!(values.borrow().1, number(0.0));
    }
    let mut expected = vec!["cache:positionMode".to_string()];
    if scenario != "getter-error" {
        if cached {
            expected.push("cache:accountLevel".into());
        }
        if matches!(scenario, "cached" | "cached-changing") {
            expected.extend(["cache:positionMode", "cache:accountLevel"].map(str::to_string));
        } else {
            expected.push("request".into());
            if scenario != "request-sync" {
                expected.push("settle:request".into());
                if !matches!(scenario, "request-reject" | "emitter-reenter") {
                    expected.push("decode".into());
                    if scenario != "decode-error" {
                        expected.extend(
                            ["assign:positionMode", "parse", "assign:accountLevel"]
                                .map(str::to_string),
                        );
                        if scenario != "assign-error" {
                            expected.extend(
                                ["cache:positionMode", "cache:accountLevel"].map(str::to_string),
                            );
                        }
                    }
                }
            }
            if throws {
                expected.extend(
                    [
                        "capture:emitAndLogError",
                        "error-value",
                        "emit",
                        "capture:Error",
                        "error-value",
                        "new-error",
                    ]
                    .map(str::to_string),
                );
            }
        }
    }
    assert_eq!(
        *events.borrow(),
        expected,
        "CONFIG native independent cache order"
    );
    assert_eq!(
        errors.borrow().len(),
        usize::from(throws && scenario != "getter-error"),
        "CONFIG native emission floor"
    );
    if throws && scenario != "getter-error" {
        assert_eq!(
            errors.borrow()[0],
            text(if scenario == "emitter-reenter" {
                "changed"
            } else {
                "down"
            })
        );
    }
    json!({"scenario":scenario,"events":events.borrow().clone(),"messages":errors.borrow().iter().map(pack).collect::<Vec<_>>(),"result":result(&outcome,&err)})
}

#[test]
fn rest_reads_native_before_source() {
    let mut validators = vec![];
    let mut delegates = vec![];
    let mut configs = vec![];
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
        "emitter-reenter",
    ] {
        validators.push(validator("okx", scenario));
    }
    for scenario in [
        "normal",
        "request-sync",
        "request-reject",
        "emit-error",
        "error-read-error",
        "message-changing",
        "empty-error",
        "emitter-reenter",
    ] {
        validators.push(validator("eu", scenario));
    }
    for method in ["fetchPositions", "fetchBalance", "fetchTickers"] {
        for scenario in ["normal", "sync-error", "reject", "result-error"] {
            delegates.push(delegation(method, scenario));
        }
    }
    for scenario in [
        "normal",
        "cached",
        "cached-changing",
        "forced",
        "zero-level",
        "unknown-mode",
        "level-nan",
        "request-sync",
        "request-reject",
        "decode-error",
        "assign-error",
        "getter-error",
        "emitter-reenter",
    ] {
        configs.push(config(scenario));
    }
    assert_eq!(validators.len(), 18);
    assert_eq!(delegates.len(), 12);
    assert_eq!(configs.len(), 13);
    oracle(json!({"venue":"okx","validators":validators,"delegates":delegates,"configs":configs}));
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
