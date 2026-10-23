use safe_cex_venue_okx::startup::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Debug, PartialEq)]
enum Native {
    Undefined,
    Null,
    Bool(bool),
    Number(i64),
    Text(Vec<u16>),
    Handle(&'static str),
    Object(Vec<(&'static str, Rc<Native>)>),
    Array(Vec<Rc<Native>>),
    Wire(Rc<Native>),
}
type V = Rc<Native>;
fn text(s: &str) -> V {
    Rc::new(Native::Text(s.encode_utf16().collect()))
}
fn undef() -> V {
    Rc::new(Native::Undefined)
}
fn truthy(v: &V) -> bool {
    match v.as_ref() {
        Native::Undefined | Native::Null | Native::Bool(false) => false,
        Native::Number(n) => *n != 0,
        Native::Text(s) => !s.is_empty(),
        _ => true,
    }
}
fn field(v: &V, name: &str) -> V {
    let Native::Object(fields) = v.as_ref() else {
        panic!("object {v:?}")
    };
    fields.iter().find(|(k, _)| *k == name).unwrap().1.clone()
}
fn array_values(v: &V) -> &[V] {
    let Native::Array(values) = v.as_ref() else {
        panic!("array {v:?}")
    };
    values
}
fn pack(v: &V) -> Value {
    match v.as_ref() {
        Native::Undefined => json!({"u":true}),
        Native::Null => Value::Null,
        Native::Bool(v) => json!(v),
        Native::Number(n) => json!(n),
        Native::Text(s) => json!({"s":s}),
        Native::Handle(s) => json!({"h":s}),
        Native::Object(fields) => {
            json!({"o":fields.iter().map(|(k,v)|json!([k,pack(v)])).collect::<Vec<_>>()})
        }
        Native::Array(v) => json!({"a":v.iter().map(pack).collect::<Vec<_>>()}),
        Native::Wire(v) => pack(v),
    }
}
#[derive(Debug)]
struct Failure {
    name: &'static str,
    message: &'static str,
    code: &'static str,
    ids: [u32; 2],
}
type E = Rc<Failure>;
fn error() -> E {
    Rc::new(Failure {
        name: "TypeError",
        message: "startup failure",
        code: "STARTUP",
        ids: [7, 3],
    })
}
fn assert_error(v: &E, wanted: &E) {
    assert!(Rc::ptr_eq(v, wanted), "STARTUP native error identity");
    assert_eq!(
        (v.name, v.message, v.code, v.ids),
        ("TypeError", "startup failure", "STARTUP", [7, 3]),
        "STARTUP native full error"
    );
}
fn run<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = std::pin::pin!(f);
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

const CONNECTIONS: [&str; 16] = [
    "normal",
    "disposed",
    "guard-error",
    "route-error",
    "tunnel",
    "relay",
    "fallback",
    "fallback-read-error",
    "info-error",
    "create-error",
    "listen-error",
    "resolver-reenter",
    "undefined-equal",
    "map-id-error",
    "map-retain",
    "map-reenter",
];
fn connect_case(kind: usize, scenario: &str) -> Value {
    let public = kind == 2;
    let label = if kind == 0 {
        "private"
    } else if kind == 1 {
        "business"
    } else {
        "public"
    };
    let events = RefCell::new(Vec::<String>::new());
    let failure = error();
    let route = Rc::new(Native::Handle("route"));
    let resolver = Rc::new(Native::Handle("resolver"));
    let emitter = Rc::new(Native::Handle("emitter"));
    let socket = Rc::new(Native::Handle("socket17"));
    let assigned = RefCell::new(None::<V>);
    let captured = RefCell::new(Vec::<V>::new());
    let channels = RefCell::new(Vec::<String>::new());
    let topic_values = RefCell::new(Vec::<V>::new());
    let retained = RefCell::new(Vec::<Rc<dyn Fn(V) -> Result<V, E>>>::new());
    let active = Rc::new(RefCell::new(None::<Rc<dyn Fn(V) -> Result<V, E>>>));
    let nested = Rc::new(RefCell::new(Vec::<V>::new()));
    let outer_id = text("OUTER");
    let host_changed = Cell::new(false);
    let getter_count = Cell::new(0);
    let guard = || {
        events.borrow_mut().push("disposed".into());
        if scenario == "guard-error" {
            Err(failure.clone())
        } else {
            Ok(scenario == "disposed")
        }
    };
    let resolve_route = |name: &str| {
        assert_eq!(
            name,
            if public {
                "wsPublic"
            } else if kind == 0 {
                "wsPrivate"
            } else {
                "wsBusiness"
            },
            "CONNECT native route selector"
        );
        events.borrow_mut().push("route".into());
        if scenario == "route-error" {
            Err(failure.clone())
        } else {
            Ok(route.clone())
        }
    };
    let direct = || {
        events.borrow_mut().push("direct".into());
        Ok(text(&format!("wss://ws.okx.com/ws/v5/{label}")))
    };
    let read = |name: &str| {
        assert_eq!(name, "endpoint");
        direct()
    };
    let get = |v: &V, key: &str| {
        assert!(Rc::ptr_eq(v, &route), "CONNECT native retained route");
        events.borrow_mut().push(format!("get:{key}"));
        if key == "fallback" {
            let n = getter_count.get();
            getter_count.set(n + 1);
            if n == 0 && scenario == "fallback-read-error" {
                return Err(failure.clone());
            }
        }
        Ok(match key {
            "tunnelled" => Rc::new(Native::Bool(matches!(
                scenario,
                "tunnel" | "resolver-reenter"
            ))),
            "host" => text(if host_changed.get() {
                "changed-host"
            } else {
                "host"
            }),
            "relay" => {
                if scenario == "undefined-equal" {
                    undef()
                } else {
                    text("relay")
                }
            }
            "fallback" => {
                if scenario == "undefined-equal" {
                    undef()
                } else {
                    text("fallback")
                }
            }
            _ => panic!("route key"),
        })
    };
    let call = |op: &str, args: &[&V], arg: Option<&str>| -> Result<V, E> {
        match op {
            "capture-resolve" => {
                events.borrow_mut().push("capture-resolve".into());
                if scenario == "resolver-reenter" {
                    host_changed.set(true);
                }
                return Ok(resolver.clone());
            }
            "relay-enabled" => {
                events.borrow_mut().push("relay-enabled".into());
                return Ok(Rc::new(Native::Bool(scenario == "relay")));
            }
            "resolve" => {
                events.borrow_mut().push("resolve".into());
                assert!(
                    Rc::ptr_eq(args[0], &resolver),
                    "CONNECT native captured resolver"
                );
                assert_eq!(
                    args.len(),
                    if public { 3 } else { 4 },
                    "CONNECT native independent resolver arity"
                );
                captured
                    .borrow_mut()
                    .extend(args[1..].iter().map(|v| (*v).clone()));
                return Ok(if scenario == "undefined-equal" {
                    undef()
                } else if matches!(scenario, "fallback" | "info-error") {
                    text("fallback")
                } else if scenario == "relay" {
                    text(if public { "fallback" } else { "relay" })
                } else {
                    args[1].clone()
                });
            }
            "capture-info" => {
                events.borrow_mut().push("capture-info".into());
                return Ok(emitter.clone());
            }
            "info" => {
                assert!(Rc::ptr_eq(args[0], &emitter));
                events.borrow_mut().push(format!("info:{}", arg.unwrap()));
                if scenario == "info-error" {
                    return Err(failure.clone());
                }
            }
            "create" => {
                events.borrow_mut().push("create".into());
                let wanted = if scenario == "undefined-equal" {
                    undef()
                } else if matches!(scenario, "fallback" | "info-error")
                    || public && scenario == "relay"
                {
                    text("fallback")
                } else if scenario == "relay" {
                    text("relay")
                } else if !public && matches!(scenario, "tunnel" | "resolver-reenter") {
                    text(if scenario == "resolver-reenter" {
                        "changed-host"
                    } else {
                        "host"
                    })
                } else {
                    text(&format!("wss://ws.okx.com/ws/v5/{label}"))
                };
                assert_eq!(*args[0], wanted, "CONNECT native selected URL value");
                if scenario == "create-error" {
                    return Err(failure.clone());
                }
                return Ok(socket.clone());
            }
            "set-ws" => {
                assert!(
                    Rc::ptr_eq(args[0], &socket),
                    "CONNECT native socket identity"
                );
                assert_eq!(args[0].as_ref(), &Native::Handle("socket17"));
                events.borrow_mut().push("set-ws".into());
                *assigned.borrow_mut() = Some(args[0].clone());
            }
            "listen" => {
                let event = arg.unwrap();
                events.borrow_mut().push(format!("listen:{event}"));
                assert!(Rc::ptr_eq(assigned.borrow().as_ref().unwrap(), &socket));
                if scenario == "listen-error" && event == "message" {
                    return Err(failure.clone());
                }
            }
            _ => panic!("call {op}"),
        }
        Ok(undef())
    };
    let map_and_assign = |channel: &str, mapper: Rc<dyn Fn(V) -> Result<V, E>>| -> Result<(), E> {
        assert!(Rc::ptr_eq(assigned.borrow().as_ref().unwrap(), &socket));
        events.borrow_mut().push(format!("map:{channel}"));
        channels.borrow_mut().push(channel.to_owned());
        if scenario == "map-retain" {
            retained.borrow_mut().push(mapper);
            topic_values
                .borrow_mut()
                .push(Rc::new(Native::Array(vec![])));
            return Ok(());
        }
        if scenario == "map-reenter" {
            *active.borrow_mut() = Some(mapper.clone());
            let record = mapper(Rc::new(Native::Handle("outer")))?;
            let Native::Object(fields) = record.as_ref() else {
                panic!("outer record")
            };
            assert_eq!(
                fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                vec!["channel", "instId"],
                "CONNECT native topic field order"
            );
            assert_eq!(field(&record, "channel"), text(channel));
            assert_eq!(
                field(&record, "instId"),
                text(if channel == "index-tickers" {
                    "IDX_OUTER"
                } else {
                    "OUTER"
                }),
                "CONNECT native reentrant outer ID"
            );
            topic_values
                .borrow_mut()
                .push(Rc::new(Native::Array(vec![record])));
            *active.borrow_mut() = None;
            return Ok(());
        }
        let mut records = vec![];
        for (i, id) in ["A", "B"].into_iter().enumerate() {
            let id = text(id);
            let market = Rc::new(Native::Object(vec![("id", id.clone())]));
            let record = mapper(market)?;
            let Native::Object(fields) = record.as_ref() else {
                panic!("record")
            };
            assert_eq!(
                fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                vec!["channel", "instId"],
                "CONNECT native topic field order"
            );
            assert_eq!(
                field(&record, "channel"),
                text(channel),
                "CONNECT native mapped channel"
            );
            assert_eq!(
                field(&record, "instId"),
                text(&if channel == "index-tickers" {
                    format!("IDX_{}", if i == 0 { "A" } else { "B" })
                } else {
                    if i == 0 { "A" } else { "B" }.to_owned()
                }),
                "CONNECT native mapped topic value"
            );
            if channel != "index-tickers" {
                assert!(
                    Rc::ptr_eq(&field(&record, "instId"), &id),
                    "CONNECT native uncoerced market ID identity"
                );
            }
            records.push(record);
        }
        topic_values
            .borrow_mut()
            .push(Rc::new(Native::Array(records)));
        Ok(())
    };
    let id_failure = failure.clone();
    let id_should_error = scenario == "map-id-error";
    let active_for_id = active.clone();
    let nested_for_id = nested.clone();
    let outer_for_id = outer_id.clone();
    let market_id: Rc<dyn Fn(&V) -> Result<V, E>> = Rc::new(move |m: &V| {
        if id_should_error {
            return Err(id_failure.clone());
        }
        if m.as_ref() == &Native::Handle("outer") {
            let mapper = active_for_id.borrow().as_ref().unwrap().clone();
            let inner = mapper(Rc::new(Native::Object(vec![("id", text("INNER"))])))?;
            nested_for_id.borrow_mut().push(inner);
            return Ok(outer_for_id.clone());
        }
        Ok(field(m, "id"))
    });
    let id_for_index = market_id.clone();
    let index_id = move |m: &V| -> Result<V, E> {
        let id = id_for_index(m)?;
        let Native::Text(units) = id.as_ref() else {
            panic!("id")
        };
        Ok(text(&format!("IDX_{}", String::from_utf16(units).unwrap())))
    };
    let literal = |s: &str| text(s);
    let object = |v| Rc::new(Native::Object(v));
    let equal = |a: &V, b: &V| a == b;
    let result = match kind {
        0 => connect_private(guard, resolve_route, read, get, truthy, equal, undef, call),
        1 => connect_business(guard, resolve_route, read, get, truthy, equal, undef, call),
        _ => connect_public(
            guard,
            resolve_route,
            direct,
            get,
            equal,
            call,
            map_and_assign,
            market_id,
            Rc::new(index_id),
            Rc::new(literal),
            Rc::new(object),
        ),
    };
    let mut expected = vec!["disposed".to_owned()];
    if !matches!(scenario, "disposed" | "guard-error") {
        expected.push("route".into());
        if scenario != "route-error" {
            if public {
                expected.push("direct".into());
            }
            expected.push("capture-resolve".into());
            if !public {
                expected.push("get:tunnelled".into());
                expected.push(
                    if matches!(scenario, "tunnel" | "resolver-reenter") {
                        "get:host"
                    } else {
                        "direct"
                    }
                    .into(),
                );
                expected.push("relay-enabled".into());
                if scenario == "relay" {
                    expected.push("get:relay".into());
                }
            }
            expected.push("get:fallback".into());
            if scenario != "fallback-read-error" {
                expected.extend(["resolve", "get:fallback"].map(str::to_owned));
                let info = matches!(
                    scenario,
                    "relay" | "fallback" | "info-error" | "undefined-equal"
                );
                if !public && !matches!(scenario, "fallback" | "info-error" | "undefined-equal") {
                    expected.push("get:relay".into());
                }
                if info {
                    expected.push("capture-info".into());
                    if !public {
                        expected.push("get:relay".into());
                    }
                    expected.push(format!(
                        "info:Using tealstreet okx {label} {}",
                        if !public && matches!(scenario, "relay" | "undefined-equal") {
                            "relay"
                        } else {
                            "proxy"
                        }
                    ));
                }
                expected.push("create".into());
                if scenario != "create-error" {
                    expected.push("set-ws".into());
                    if public && scenario == "map-id-error" {
                        expected.push("map:tickers".into());
                    } else {
                        if public {
                            expected.extend(
                                [
                                    "tickers",
                                    "mark-price",
                                    "index-tickers",
                                    "open-interest",
                                    "funding-rate",
                                ]
                                .map(|c| format!("map:{c}")),
                            );
                        }
                        expected.extend(["listen:open", "listen:message"].map(str::to_owned));
                        if scenario != "listen-error" {
                            expected.push("listen:close".into());
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        *events.borrow(),
        expected,
        "CONNECT native live route and catch order"
    );
    if !captured.borrow().is_empty() {
        let args = captured.borrow();
        let wanted = if !public && matches!(scenario, "tunnel" | "resolver-reenter") {
            text(if scenario == "resolver-reenter" {
                "changed-host"
            } else {
                "host"
            })
        } else {
            text(&format!("wss://ws.okx.com/ws/v5/{label}"))
        };
        assert_eq!(
            args[0], wanted,
            "CONNECT native uncoerced resolver arguments"
        );
        if !public {
            assert_eq!(
                args[1],
                if scenario == "relay" {
                    text("relay")
                } else {
                    undef()
                }
            );
        }
        assert_eq!(
            args.last().unwrap(),
            &if scenario == "undefined-equal" {
                undef()
            } else {
                text("fallback")
            }
        );
    }
    let throws = (public && scenario == "map-id-error")
        || matches!(
            scenario,
            "guard-error" | "route-error" | "fallback-read-error" | "create-error" | "listen-error"
        );
    assert_eq!(result.is_err(), throws, "CONNECT native scope");
    if let Err(e) = result {
        assert_error(&e, &failure);
    }
    let mapped = public && assigned.borrow().is_some();
    assert_eq!(
        *channels.borrow(),
        if mapped && scenario == "map-id-error" {
            vec!["tickers"]
        } else if mapped {
            vec![
                "tickers",
                "mark-price",
                "index-tickers",
                "open-interest",
                "funding-rate",
            ]
        } else {
            vec![]
        },
        "CONNECT native five topic assignment order"
    );
    let mut late = vec![];
    assert_eq!(
        retained.borrow().len(),
        if public && scenario == "map-retain" {
            5
        } else {
            0
        },
        "CONNECT native independent retained mapper floor"
    );
    for (i, mapper) in retained.borrow().iter().enumerate() {
        let id = text("T");
        let value = mapper(Rc::new(Native::Object(vec![("id", id.clone())]))).unwrap();
        let channel = [
            "tickers",
            "mark-price",
            "index-tickers",
            "open-interest",
            "funding-rate",
        ][i];
        let Native::Object(fields) = value.as_ref() else {
            panic!("late")
        };
        assert_eq!(
            fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec!["channel", "instId"],
            "CONNECT native retained topic field order"
        );
        assert_eq!(
            field(&value, "channel"),
            text(channel),
            "CONNECT native retained callback channel"
        );
        assert_eq!(
            field(&value, "instId"),
            text(if i == 2 { "IDX_T" } else { "T" }),
            "CONNECT native retained callback ID"
        );
        if i != 2 {
            assert!(Rc::ptr_eq(&field(&value, "instId"), &id));
        }
        late.push(value);
    }
    assert_eq!(
        nested.borrow().len(),
        if public && scenario == "map-reenter" {
            5
        } else {
            0
        },
        "CONNECT native independent reentry floor"
    );
    for (i, value) in nested.borrow().iter().enumerate() {
        let Native::Object(fields) = value.as_ref() else {
            panic!("inner")
        };
        assert_eq!(
            fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec!["channel", "instId"],
            "CONNECT native reentrant topic field order"
        );
        assert_eq!(
            field(value, "channel"),
            text(
                [
                    "tickers",
                    "mark-price",
                    "index-tickers",
                    "open-interest",
                    "funding-rate"
                ][i]
            )
        );
        assert_eq!(
            field(value, "instId"),
            text(if i == 2 { "IDX_INNER" } else { "INNER" }),
            "CONNECT native reentrant inner ID"
        );
    }
    json!({"kind":kind,"scenario":scenario,"events":events.borrow().clone(),"args":captured.borrow().iter().map(pack).collect::<Vec<_>>(),"assigned":assigned.borrow().is_some(),"topics":topic_values.borrow().iter().map(pack).collect::<Vec<_>>(),"late":late.iter().map(pack).collect::<Vec<_>>(),"nested":nested.borrow().iter().map(pack).collect::<Vec<_>>(),"throw":throws})
}

const OPEN: [&str; 14] = [
    "normal",
    "disabled",
    "disposed",
    "auth-false",
    "auth-zero",
    "auth-sync-error",
    "auth-reject",
    "dispose-await",
    "emit-error",
    "disregard-error",
    "reconnect-error",
    "ping-error",
    "ping-dispose",
    "snapshot-error",
];
fn open_case(business: bool, scenario: &str) -> Value {
    let events = RefCell::new(Vec::<String>::new());
    let failure = error();
    let disposed = Cell::new(scenario == "disposed");
    let guard = || {
        events.borrow_mut().push("disposed".into());
        Ok(disposed.get())
    };
    let auth = || {
        events.borrow_mut().push("auth".into());
        if scenario == "auth-sync-error" {
            return Err(failure.clone());
        }
        Ok(async {
            events.borrow_mut().push("auth-settled".into());
            if scenario == "dispose-await" {
                disposed.set(true);
            }
            if matches!(scenario, "auth-reject" | "emit-error") {
                Err(failure.clone())
            } else {
                Ok(if scenario == "auth-zero" {
                    Rc::new(Native::Number(0))
                } else {
                    Rc::new(Native::Bool(!matches!(
                        scenario,
                        "auth-false" | "disregard-error" | "reconnect-error"
                    )))
                })
            }
        })
    };
    let emit = |e: &E| {
        assert_error(e, &failure);
        events.borrow_mut().push("emit-error".into());
        if scenario == "emit-error" {
            Err(failure.clone())
        } else {
            Ok(())
        }
    };
    let invoke = |op: &str| {
        events.borrow_mut().push(op.into());
        if (scenario == "disregard-error" && op == "disregardSilence")
            || (scenario == "reconnect-error" && op == "reconnect")
            || (scenario == "ping-error" && op == "ping")
            || (scenario == "snapshot-error" && op != "ping")
        {
            return Err(failure.clone());
        }
        if scenario == "ping-dispose" && op == "ping" {
            disposed.set(true);
        }
        Ok(())
    };
    let enabled = || {
        events.borrow_mut().push("enabled".into());
        Ok(scenario != "disabled")
    };
    let result = if business {
        run(on_open_business(guard, auth, truthy, emit, invoke, enabled))
    } else {
        run(on_open_private(guard, auth, truthy, emit, invoke, enabled))
    };
    let mut expected = vec!["disposed"];
    if scenario != "disposed" {
        expected.push("auth");
        if scenario != "auth-sync-error" {
            expected.push("auth-settled");
        }
        if matches!(scenario, "auth-sync-error" | "auth-reject" | "emit-error") {
            expected.push("emit-error");
        }
        if scenario != "emit-error" {
            expected.push("disposed");
            if scenario != "dispose-await" {
                if matches!(
                    scenario,
                    "auth-false"
                        | "auth-zero"
                        | "auth-sync-error"
                        | "auth-reject"
                        | "disregard-error"
                        | "reconnect-error"
                ) {
                    expected.push("disregardSilence");
                    if scenario != "disregard-error" {
                        expected.push("reconnect");
                    }
                } else {
                    expected.push("ping");
                    if scenario != "ping-error" {
                        expected.push("enabled");
                        if scenario != "disabled" {
                            expected.push(if business {
                                "emitWsReconnected"
                            } else {
                                "private-snapshot"
                            });
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        *events.borrow(),
        expected,
        "OPEN native auth await recheck failure suppression"
    );
    let throws = matches!(
        scenario,
        "emit-error" | "disregard-error" | "reconnect-error" | "ping-error" | "snapshot-error"
    );
    assert_eq!(result.is_err(), throws);
    if let Err(e) = result {
        assert_error(&e, &failure);
    }
    json!({"business":business,"scenario":scenario,"events":events.borrow().clone(),"throw":throws})
}

const AUTH: [&str; 17] = [
    "local",
    "remote",
    "remote-error",
    "sign-sync-error",
    "sign-reject",
    "secret-error",
    "key-error",
    "send-error",
    "error-emitter-error",
    "remote-null-time",
    "local-null-time",
    "send-reenter",
    "sign-reenter",
    "error-changing",
    "stamp-error",
    "json-error",
    "remote-opaque-time",
];
fn auth_case(business: bool, scenario: &str) -> Value {
    let remote = !matches!(
        scenario,
        "local" | "secret-error" | "local-null-time" | "stamp-error"
    );
    let events = RefCell::new(Vec::<String>::new());
    let failure = error();
    let timestamp = Rc::new(
        if matches!(scenario, "remote-null-time" | "local-null-time") {
            Native::Null
        } else if scenario == "remote-opaque-time" {
            Native::Handle("stamp")
        } else {
            Native::Number(7)
        },
    );
    let signer = Rc::new(Native::Handle("signer"));
    let result = Rc::new(Native::Handle("result"));
    let emit_callee = Rc::new(Native::Handle("emit"));
    let send_callee = Rc::new(Native::Handle("send"));
    let json_callee = Rc::new(Native::Handle("json"));
    let request = RefCell::new(None::<V>);
    let frame = RefCell::new(None::<V>);
    let messages = RefCell::new(Vec::<V>::new());
    let reentered = Cell::new(false);
    let error_gets = Cell::new(0);
    let read = |op: &str| {
        events.borrow_mut().push(format!("read:{op}"));
        if (scenario == "stamp-error" && op == "timestamp")
            || (scenario == "key-error" && op == "key")
        {
            return Err(failure.clone());
        }
        Ok(match op {
            "timestamp" => timestamp.clone(),
            "remoteSigner" => Rc::new(Native::Bool(remote)),
            "nanoid" => text("N"),
            "name" => text("okx"),
            "remote-account" => text(if reentered.get() {
                "CHANGED"
            } else {
                "ACCOUNT"
            }),
            "key" => text(if reentered.get() && scenario == "send-reenter" {
                "CHANGED"
            } else {
                "KEY"
            }),
            "passphrase" => text("PASS"),
            _ => panic!("read {op}"),
        })
    };
    let get = |v: &V, key: &str| {
        assert!(Rc::ptr_eq(v, &result), "AUTH native retained signer result");
        events.borrow_mut().push(format!("get:{key}"));
        Ok(match key {
            "error" => {
                let n = error_gets.get();
                error_gets.set(n + 1);
                if matches!(
                    scenario,
                    "remote-error" | "error-emitter-error" | "error-changing"
                ) {
                    text(if scenario == "error-changing" && n > 0 {
                        "changed"
                    } else {
                        "down"
                    })
                } else {
                    undef()
                }
            }
            "wsAuthSignature" => text("REMOTE"),
            _ => panic!("get {key}"),
        })
    };
    let call = |op: &str, args: &[&V], arg: Option<&str>| -> Result<V, E> {
        events.borrow_mut().push(op.into());
        match op {
            "capture-sign" => {
                if scenario == "sign-reenter" {
                    reentered.set(true);
                }
                return Ok(signer.clone());
            }
            "capture-error" | "capture-send" | "capture-json" => {
                if op == "capture-send" && scenario == "send-reenter" {
                    reentered.set(true);
                }
                return Ok(if op == "capture-error" {
                    emit_callee.clone()
                } else if op == "capture-send" {
                    send_callee.clone()
                } else {
                    json_callee.clone()
                });
            }
            "emit-error" => {
                assert!(
                    Rc::ptr_eq(args[0], &emit_callee),
                    "AUTH native captured emitter identity"
                );
                assert_eq!(
                    *args[1],
                    text(&format!(
                        "Remote WS signing unavailable: {}",
                        if scenario == "error-changing" {
                            "changed"
                        } else {
                            "down"
                        }
                    )),
                    "AUTH native remote error message"
                );
                messages.borrow_mut().push(args[1].clone());
                if scenario == "error-emitter-error" {
                    return Err(failure.clone());
                }
            }
            "hmac" => {
                assert!(
                    Rc::ptr_eq(args[0], &timestamp),
                    "AUTH native raw HMAC timestamp"
                );
                assert_eq!(
                    arg,
                    Some("GET/users/self/verify"),
                    "AUTH native HMAC payload tail"
                );
                if scenario == "secret-error" {
                    return Err(failure.clone());
                }
                let payload = if scenario == "local-null-time" {
                    "GET/users/self/verify"
                } else {
                    "7GET/users/self/verify"
                };
                let signature = safe_cex_crypto::hmac_sha256(
                    b"SECRET",
                    payload.as_bytes(),
                    safe_cex_crypto::Encoding::Base64,
                );
                assert_eq!(
                    signature,
                    if scenario == "local-null-time" {
                        "rB/8nmOI9+AwPl00mEIG4pXW7XeVm/afbGQT1X0gKY4="
                    } else {
                        "JwNTn66vBdBbr690CLCDQDG5isEgMKDEUzNXdH5gujc="
                    },
                    "AUTH native independent HMAC signature"
                );
                return Ok(text(&signature));
            }
            "stringify" => {
                assert!(
                    Rc::ptr_eq(args[0], &json_callee),
                    "AUTH native captured JSON callee identity"
                );
                let keys = match args[1].as_ref() {
                    Native::Object(v) => v.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                    _ => panic!("frame"),
                };
                assert_eq!(keys, vec!["op", "args"], "AUTH native ordered login fields");
                assert_eq!(field(args[1], "op"), text("login"), "AUTH native login op");
                let args_value = field(args[1], "args");
                let entries = array_values(&args_value);
                assert_eq!(entries.len(), 1, "AUTH native independent login arity");
                let login = &entries[0];
                let Native::Object(fields) = login.as_ref() else {
                    panic!("login")
                };
                assert_eq!(
                    fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
                    vec!["apiKey", "passphrase", "timestamp", "sign"],
                    "AUTH native ordered argument fields"
                );
                assert_eq!(
                    field(login, "apiKey"),
                    text(if scenario == "send-reenter" {
                        "CHANGED"
                    } else {
                        "KEY"
                    }),
                    "AUTH native current key"
                );
                assert_eq!(field(login, "passphrase"), text("PASS"));
                assert!(
                    Rc::ptr_eq(&field(login, "timestamp"), &timestamp),
                    "AUTH native uncoerced login timestamp"
                );
                assert_eq!(
                    field(login, "sign"),
                    text(if remote {
                        "REMOTE"
                    } else if scenario == "local-null-time" {
                        "rB/8nmOI9+AwPl00mEIG4pXW7XeVm/afbGQT1X0gKY4="
                    } else {
                        "JwNTn66vBdBbr690CLCDQDG5isEgMKDEUzNXdH5gujc="
                    }),
                    "AUTH native signature field"
                );
                *frame.borrow_mut() = Some(args[1].clone());
                if scenario == "json-error" {
                    return Err(failure.clone());
                }
                return Ok(Rc::new(Native::Wire(args[1].clone())));
            }
            "send" => {
                assert!(
                    Rc::ptr_eq(args[0], &send_callee),
                    "AUTH native captured sender identity"
                );
                let Native::Wire(packet) = args[1].as_ref() else {
                    panic!("wire")
                };
                assert!(
                    Rc::ptr_eq(packet, frame.borrow().as_ref().unwrap()),
                    "AUTH native unchanged packet identity"
                );
                if scenario == "send-error" {
                    return Err(failure.clone());
                }
            }
            _ => panic!("call {op}"),
        }
        Ok(undef())
    };
    let sign = |callee: V, payload: V| {
        assert!(
            Rc::ptr_eq(&callee, &signer),
            "AUTH native captured signer identity"
        );
        let Native::Object(fields) = payload.as_ref() else {
            panic!("request")
        };
        assert_eq!(
            fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            vec!["id", "type", "exchangeName", "accountName", "wsAuth"],
            "AUTH native request field order"
        );
        assert_eq!(field(&payload, "id"), text("N"));
        assert_eq!(field(&payload, "type"), text("ws-auth"));
        assert_eq!(field(&payload, "exchangeName"), text("okx"));
        assert_eq!(
            field(&payload, "accountName"),
            text(if scenario == "sign-reenter" {
                "CHANGED"
            } else {
                "ACCOUNT"
            }),
            "AUTH native live account name"
        );
        let ws_auth = field(&payload, "wsAuth");
        let mut expected = if scenario == "remote-null-time" {
            "null".encode_utf16().collect::<Vec<_>>()
        } else if scenario == "remote-opaque-time" {
            vec![0xd800]
        } else {
            vec![55]
        };
        expected.extend("GET/users/self/verify".encode_utf16());
        assert_eq!(
            field(&ws_auth, "toSign").as_ref(),
            &Native::Text(expected),
            "AUTH native toSign payload"
        );
        assert_eq!(field(&ws_auth, "outputFormat"), text("base64"));
        assert!(
            Rc::ptr_eq(
                &field(&field(&ws_auth, "extraData"), "timestamp"),
                &timestamp
            ),
            "AUTH native extraData timestamp identity"
        );
        *request.borrow_mut() = Some(payload);
        events.borrow_mut().push("sign".into());
        if scenario == "sign-sync-error" {
            return Err(failure.clone());
        }
        Ok(async {
            events.borrow_mut().push("sign-settled".into());
            if scenario == "sign-reject" {
                Err(failure.clone())
            } else {
                Ok(result.clone())
            }
        })
    };
    let template = |v: &V| {
        Ok(match v.as_ref() {
            Native::Number(n) => n.to_string().encode_utf16().collect(),
            Native::Null => "null".encode_utf16().collect(),
            Native::Handle("stamp") => vec![0xd800],
            Native::Text(s) => s.clone(),
            _ => panic!("template"),
        })
    };
    let literal = |s: &str| text(s);
    let string = |s| Rc::new(Native::Text(s));
    let object = |v| Rc::new(Native::Object(v));
    let array = |v| Rc::new(Native::Array(v));
    let outcome = if business {
        run(auth_business(
            read, get, truthy, call, sign, template, literal, string, object, array,
        ))
    } else {
        run(auth_private(
            read, get, truthy, call, sign, template, literal, string, object, array,
        ))
    };
    let mut expected = vec!["read:timestamp".to_owned()];
    if scenario != "stamp-error" {
        expected.push("read:remoteSigner".into());
        if remote {
            expected.extend(
                [
                    "capture-sign",
                    "read:nanoid",
                    "read:name",
                    "read:remote-account",
                    "sign",
                ]
                .map(str::to_owned),
            );
            if scenario != "sign-sync-error" {
                expected.push("sign-settled".into());
                if scenario != "sign-reject" {
                    expected.push("get:error".into());
                    if matches!(
                        scenario,
                        "remote-error" | "error-emitter-error" | "error-changing"
                    ) {
                        expected.extend(
                            ["capture-error", "get:error", "emit-error"].map(str::to_owned),
                        );
                    }
                }
            }
        } else {
            expected.push("hmac".into());
        }
        if !matches!(
            scenario,
            "stamp-error"
                | "sign-sync-error"
                | "sign-reject"
                | "secret-error"
                | "remote-error"
                | "error-emitter-error"
                | "error-changing"
        ) {
            expected.extend(["capture-send", "capture-json", "read:key"].map(str::to_owned));
            if scenario != "key-error" {
                expected.push("read:passphrase".into());
                if remote {
                    expected.push("get:wsAuthSignature".into());
                }
                expected.push("stringify".into());
                if scenario != "json-error" {
                    expected.push("send".into());
                }
            }
        }
    }
    assert_eq!(
        *events.borrow(),
        expected,
        "AUTH native callee fields signing and error order"
    );
    let throws = matches!(
        scenario,
        "sign-sync-error"
            | "sign-reject"
            | "secret-error"
            | "key-error"
            | "send-error"
            | "error-emitter-error"
            | "stamp-error"
            | "json-error"
    );
    assert_eq!(outcome.is_err(), throws, "AUTH native rejection boundary");
    match outcome {
        Ok(v) => assert_eq!(
            v,
            !matches!(scenario, "remote-error" | "error-changing"),
            "AUTH native success flag"
        ),
        Err(e) => {
            assert!(throws);
            assert_error(&e, &failure);
        }
    }
    json!({"business":business,"scenario":scenario,"events":events.borrow().clone(),"request":request.borrow().as_ref().map(pack),"frame":frame.borrow().as_ref().map(pack),"messages":messages.borrow().iter().map(pack).collect::<Vec<_>>(),"throw":throws,"success":if throws{Value::Null}else{json!(!matches!(scenario,"remote-error"|"error-changing"))}})
}

#[test]
fn startup_native_before_source() {
    let connects = (0..3)
        .flat_map(|k| CONNECTIONS.into_iter().map(move |s| connect_case(k, s)))
        .collect::<Vec<_>>();
    assert_eq!(connects.len(), 48, "STARTUP independent connection floor");
    let opens = [false, true]
        .into_iter()
        .flat_map(|b| OPEN.into_iter().map(move |s| open_case(b, s)))
        .collect::<Vec<_>>();
    assert_eq!(opens.len(), 28, "STARTUP independent open floor");
    let auths = [false, true]
        .into_iter()
        .flat_map(|b| AUTH.into_iter().map(move |s| auth_case(b, s)))
        .collect::<Vec<_>>();
    assert_eq!(auths.len(), 34, "STARTUP independent auth floor");
    let called = Cell::new(0);
    let set: Rc<RefCell<indexmap::IndexSet<String>>> =
        create_order_channels(|entries| -> Result<_, E> {
            called.set(called.get() + 1);
            assert_eq!(
                entries,
                &["orders", "orders-algo", "algo-advance"],
                "SET native independent initializer"
            );
            let unique = entries
                .iter()
                .map(|s| s.to_string())
                .collect::<indexmap::IndexSet<_>>();
            Ok(Rc::new(RefCell::new(unique)))
        })
        .unwrap();
    assert_eq!(called.get(), 1);
    assert_eq!(set.borrow().len(), 3, "SET native independent cardinality");
    assert_eq!(
        set.borrow().iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["orders", "orders-algo", "algo-advance"],
        "SET native insertion order"
    );
    let alias = set.clone();
    assert!(alias.borrow_mut().insert("custom".into()));
    assert!(
        !alias.borrow_mut().insert("custom".into()),
        "SET native duplicate insertion unchanged"
    );
    assert!(
        Rc::ptr_eq(&alias, &set),
        "SET native mutable alias identity"
    );
    assert_eq!(
        set.borrow().iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["orders", "orders-algo", "algo-advance", "custom"],
        "SET native live shared value"
    );
    let expected = json!({"connects":connects,"opens":opens,"auths":auths,"set":set.borrow().iter().cloned().collect::<Vec<_>>()});
    let mut child = Command::new("node")
        .arg(format!(
            "{}/tests/startup-oracle.mjs",
            env!("CARGO_MANIFEST_DIR")
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
        .write_all(serde_json::to_string(&expected).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "maintained source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    for family in ["connects", "opens", "auths"] {
        let rows = expected[family].as_array().unwrap();
        assert_eq!(source[family].as_array().unwrap().len(), rows.len());
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(
                row, &source[family][i],
                "STARTUP maintained source after native assertions {family} {i}"
            );
        }
    }
    assert_eq!(expected["set"], source["set"]);
}
