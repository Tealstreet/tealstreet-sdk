use safe_cex_venue_okx::socket_boundaries as port;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};

#[derive(Debug)]
struct Handle {
    tag: &'static str,
    units: Vec<u16>,
}
#[derive(Debug)]
struct Failure {
    name: &'static str,
    message: &'static str,
    code: &'static str,
    ids: [u8; 2],
}
type Error = Rc<Failure>;
type Trace = Rc<RefCell<Vec<String>>>;
type Snapshot = Rc<Handle>;
type Mapper = Rc<dyn Fn(Snapshot) -> Result<(), Error>>;
fn handle(tag: &'static str) -> Rc<Handle> {
    Rc::new(Handle {
        tag,
        units: vec![0xd800, 65],
    })
}
fn failure() -> Error {
    Rc::new(Failure {
        name: "TypeError",
        message: "socket boundary failure",
        code: "SOCKET",
        ids: [9, 2],
    })
}
fn push(trace: &Trace, event: impl Into<String>) {
    trace.borrow_mut().push(event.into());
}
fn assert_error(actual: &Error, expected: &Error) {
    assert!(
        Rc::ptr_eq(actual, expected),
        "BOUNDARY native original error identity"
    );
    assert_eq!(actual.name, "TypeError");
    assert_eq!(actual.message, "socket boundary failure");
    assert_eq!(actual.code, "SOCKET");
    assert_eq!(actual.ids, [9, 2]);
}
fn wrapper(kind: &str, method: &str, scenario: &str) -> Value {
    let events = Trace::default();
    let err = failure();
    let arg = handle("arg");
    let callback = handle("callback");
    let result = handle("result");
    let callee = handle("captured");
    let external = matches!(scenario, "provider-true" | "provider-change");
    let _provider = || {
        push(&events, "read:provider");
        if scenario == "provider-error" {
            Err(err.clone())
        } else {
            Ok(external)
        }
    };
    let capture = |owner: &str, member: &str| {
        let actual_owner = if owner == "externalPublicDataProvider" {
            push(&events, "read:provider");
            if scenario == "provider-change" {
                "changed"
            } else {
                owner
            }
        } else {
            push(&events, format!("read:{owner}"));
            owner
        };
        let expected_owner = if kind == "spot" && external {
            "externalPublicDataProvider"
        } else if kind == "okx" && method == "listenOHLCV" {
            "businessWebsocket"
        } else {
            "publicWebsocket"
        };
        let expected_member = match (method, kind == "spot" && external) {
            ("listenOHLCV", true) => "subscribeCandle",
            ("listenOrderBook", true) => "subscribeOrderbook",
            _ => method,
        };
        assert_eq!(owner, expected_owner, "FORWARD native receiver selection");
        assert_eq!(member, expected_member, "FORWARD native member selection");
        push(&events, format!("capture:{actual_owner}.{member}"));
        if scenario == "capture-error" {
            Err(err.clone())
        } else {
            Ok((callee.clone(), actual_owner.to_string(), member.to_string()))
        }
    };
    let invoke =
        |(captured, owner, member): (Rc<Handle>, String, String), a: Rc<Handle>, b: Rc<Handle>| {
            assert!(
                Rc::ptr_eq(&captured, &callee),
                "FORWARD native captured callee identity"
            );
            assert!(Rc::ptr_eq(&a, &arg), "FORWARD native argument identity");
            assert!(
                Rc::ptr_eq(&b, &callback),
                "FORWARD native callback identity"
            );
            assert_eq!(a.tag, "arg");
            assert_eq!(a.units, [0xd800, 65]);
            assert_eq!(b.tag, "callback");
            push(&events, format!("invoke:{owner}.{member}"));
            if scenario == "invoke-error" {
                Err(err.clone())
            } else {
                Ok(result.clone())
            }
        };
    let outcome = if method == "listenOHLCV" {
        port::listen_ohlcv(arg.clone(), callback.clone(), capture, invoke)
    } else {
        port::listen_order_book(arg.clone(), callback.clone(), capture, invoke)
    };
    let throws = matches!(scenario, "capture-error" | "invoke-error")
        || (kind == "spot" && scenario == "provider-error");
    assert_eq!(outcome.is_err(), throws, "FORWARD native throw boundary");
    if let Err(actual) = outcome {
        assert_error(&actual, &err)
    } else {
        let returned = outcome.unwrap();
        assert!(
            Rc::ptr_eq(&returned, &result),
            "FORWARD native original return identity"
        );
        assert_eq!(returned.tag, "result");
    }
    let owner = if kind == "spot" && external {
        if scenario == "provider-change" {
            "changed"
        } else {
            "externalPublicDataProvider"
        }
    } else if kind == "okx" && method == "listenOHLCV" {
        "businessWebsocket"
    } else {
        "publicWebsocket"
    };
    let member = if kind == "spot" && external {
        if method == "listenOHLCV" {
            "subscribeCandle"
        } else {
            "subscribeOrderbook"
        }
    } else {
        method
    };
    let mut expected = vec![];
    if kind == "spot" {
        expected.push("read:provider".to_string());
    }
    if !(kind == "spot" && scenario == "provider-error") {
        expected.push(if kind == "spot" && external {
            "read:provider".to_string()
        } else {
            format!("read:{owner}")
        });
        expected.push(format!("capture:{owner}.{member}"));
        if scenario != "capture-error" {
            expected.push(format!("invoke:{owner}.{member}"));
        }
    }
    assert_eq!(
        *events.borrow(),
        expected,
        "FORWARD native independent trace"
    );
    json!({"kind":kind,"method":method,"scenario":scenario,"events":events.borrow().clone(),"throw":throws})
}
fn disposal(kind: &str, scenario: &str) -> Value {
    let events = Trace::default();
    let err = failure();
    let _retained = Rc::new(RefCell::new(Vec::<Mapper>::new()));
    let _active = Rc::new(RefCell::new(None::<Mapper>));
    let invoke = |expression: &str| {
        let action = match expression {
            "super.dispose" => "super",
            "publicWebsocket.dispose" => "publicWebsocket",
            "privateWebsocket.dispose" => "privateWebsocket",
            "businessWebsocket.dispose" => "businessWebsocket",
            _ => panic!("unexpected native dispose expression"),
        };
        if action != "super" {
            push(&events, format!("read:{action}"));
            push(&events, format!("capture:{action}"));
        }
        push(&events, action);
        if scenario == format!("{action}-error") {
            Err(err.clone())
        } else {
            Ok(())
        }
    };
    let outcome = port::dispose(invoke);
    let throws = scenario.ends_with("-error");
    assert_eq!(outcome.is_err(), throws, "DISPOSE native throw boundary");
    if let Err(actual) = outcome {
        assert_error(&actual, &err);
    }

    let mut expected = vec!["super".to_string()];
    if scenario != "super-error" {
        expected.extend(
            [
                "read:publicWebsocket",
                "capture:publicWebsocket",
                "publicWebsocket",
            ]
            .map(str::to_string),
        );
        if scenario != "publicWebsocket-error" {
            if kind == "futures" {
                expected.extend(["read:snapshots", "forEach"].map(str::to_string));
                if scenario != "forEach-error" {
                    if scenario == "reentrant-snapshot" {
                        expected.extend(["snapshot:OUTER", "snapshot:INNER"].map(str::to_string));
                    } else if !matches!(scenario, "empty-snapshots" | "retained-snapshot") {
                        expected.push("snapshot:A".into());
                        if scenario != "snapshots-error" {
                            expected.push("snapshot:B".into());
                        }
                    }
                    if scenario != "snapshots-error" {
                        expected.push("reset".into());
                    }
                }
            }
            if !matches!(
                scenario,
                "snapshots-error" | "forEach-error" | "reset-error"
            ) {
                expected.extend(
                    [
                        "read:privateWebsocket",
                        "capture:privateWebsocket",
                        "privateWebsocket",
                    ]
                    .map(str::to_string),
                );
                if kind == "okx" && scenario != "privateWebsocket-error" {
                    expected.extend(
                        [
                            "read:businessWebsocket",
                            "capture:businessWebsocket",
                            "businessWebsocket",
                        ]
                        .map(str::to_string),
                    );
                }
            }
        }
    }
    if scenario == "retained-snapshot" {
        expected.push("snapshot:LATE".into());
    }
    assert_eq!(
        *events.borrow(),
        expected,
        "DISPOSE native independent stop reset trace"
    );
    json!({"kind":kind,"scenario":scenario,"events":events.borrow().clone(),"throw":throws})
}
#[test]
fn socket_boundaries_native_before_source() {
    let mut wrappers = vec![];
    let mut disposals = vec![];
    for method in ["listenOHLCV", "listenOrderBook"] {
        for scenario in ["normal", "capture-error", "invoke-error"] {
            wrappers.push(wrapper("okx", method, scenario));
        }
    }
    for scenario in [
        "normal",
        "super-error",
        "publicWebsocket-error",
        "privateWebsocket-error",
        "businessWebsocket-error",
    ] {
        disposals.push(disposal("okx", scenario));
    }
    assert_eq!(wrappers.len(), 6, "independent wrapper fixture floor");
    assert_eq!(disposals.len(), 5, "independent disposal fixture floor");
    let input = json!({"venue":"okx","wrappers":wrappers,"disposals":disposals});
    let oracle =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/socket-boundaries-oracle.mjs");
    let mut child = Command::new("node")
        .arg(oracle)
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
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        input, source,
        "maintained source after all native assertions"
    );
}
