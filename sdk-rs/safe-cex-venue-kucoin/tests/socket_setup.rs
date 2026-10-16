use safe_cex_venue_kucoin::socket_setup::*;
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
    Object(&'static str),
    Text(Vec<u16>),
    Number(f64),
    Bool(bool),
    Null,
    Undefined,
}
type V = Rc<Native>;
fn text(s: &str) -> V {
    Rc::new(Native::Text(s.encode_utf16().collect()))
}
fn undefined() -> V {
    Rc::new(Native::Undefined)
}
fn truthy(v: &V) -> bool {
    match v.as_ref() {
        Native::Undefined | Native::Null | Native::Bool(false) => false,
        Native::Number(n) => *n != 0.0 && !n.is_nan(),
        Native::Text(s) => !s.is_empty(),
        _ => true,
    }
}
#[derive(Debug)]
struct Failure {
    name: &'static str,
    message: &'static str,
    code: Option<&'static str>,
    ids: Option<[u32; 2]>,
}
type Error = Rc<Failure>;
fn failure() -> Error {
    Rc::new(Failure {
        name: "TypeError",
        message: "setup failure",
        code: Some("SETUP"),
        ids: Some([7, 3]),
    })
}
fn assert_error(actual: &Error, expected: &Error) {
    assert!(Rc::ptr_eq(actual, expected), "SETUP native error identity");
    assert_eq!(
        (actual.name, actual.message, actual.code, actual.ids),
        if expected.name == "RangeError" {
            ("RangeError", "Invalid array length", None, None)
        } else {
            ("TypeError", "setup failure", Some("SETUP"), Some([7, 3]))
        },
        "SETUP native full error"
    );
}
fn run<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop().clone();
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(v) = future.as_mut().poll(&mut context) {
            return v;
        }
    }
}
struct Response<'a> {
    pending: bool,
    reject: bool,
    dispose: bool,
    disposed: &'a Cell<bool>,
    events: &'a RefCell<Vec<String>>,
    value: V,
    error: Error,
}
impl Future for Response<'_> {
    type Output = Result<V, Error>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.pending {
            self.pending = false;
            return Poll::Pending;
        }
        self.events.borrow_mut().push("resolved".into());
        if self.dispose {
            self.disposed.set(true);
        }
        Poll::Ready(if self.reject {
            Err(self.error.clone())
        } else {
            Ok(self.value.clone())
        })
    }
}
fn wire(v: &V) -> Value {
    match v.as_ref() {
        Native::Object(s) => json!({"object":s}),
        Native::Text(s) => json!({"text":s}),
        Native::Number(n) => {
            if n.fract() == 0.0 {
                json!({"number":*n as i64})
            } else {
                json!({"number":n})
            }
        }
        Native::Bool(v) => json!({"bool":v}),
        Native::Null => json!({"null":true}),
        Native::Undefined => json!({"undefined":true}),
    }
}

const SCENARIOS: [&str; 19] = [
    "normal",
    "disposed",
    "guard-error",
    "post-sync-error",
    "post-reject",
    "bad-code",
    "numeric-code",
    "missing-data",
    "no-servers",
    "zero-length",
    "missing-endpoint",
    "ping-zero",
    "dispose-await",
    "endpoint-reenter",
    "listener-replace",
    "emit-error-throws",
    "retry-throws",
    "endpoint-throws",
    "live-endpoint",
];

fn connect_case(kind: usize, scenario: &str) -> Value {
    let spot = kind >= 2;
    let private = kind % 2 == 1;
    let events = RefCell::new(Vec::<String>::new());
    let error = failure();
    let disposed = Cell::new(scenario == "disposed");
    let token = text("TK");
    let endpoint = Rc::new(Native::Text(vec![119, 115, 115, 58, 47, 47, 0xd800]));
    let date = Rc::new(Native::Number(41.0));
    let socket = Rc::new(Native::Object("new"));
    let state = RefCell::new((
        text("OLD"),
        text("OLD-ENDPOINT"),
        Rc::new(Native::Number(5000.0)),
        Rc::new(Native::Object("old")),
    ));
    let guard = || {
        events.borrow_mut().push("disposed".into());
        if scenario == "guard-error" {
            Err(error.clone())
        } else {
            Ok(disposed.get())
        }
    };
    let opaque_url = Rc::new(Native::Object("endpoint-url"));
    let post = |selector: &str| {
        assert_eq!(
            selector,
            if private {
                "GET_PRIVATE_WS_TOKEN"
            } else {
                "GET_PUBLIC_WS_TOKEN"
            },
            "CONNECT native endpoint operation selector"
        );
        let url = if scenario == "live-endpoint" {
            events
                .borrow_mut()
                .extend(["post:get".into(), "endpoint:get".into()]);
            opaque_url.clone()
        } else {
            text(if private {
                "/api/v1/bullet-private"
            } else {
                "/api/v1/bullet-public"
            })
        };
        if scenario == "live-endpoint" {
            assert!(
                Rc::ptr_eq(&url, &opaque_url),
                "CONNECT native uncoerced live endpoint identity"
            );
            assert_eq!(
                url.as_ref(),
                &Native::Object("endpoint-url"),
                "CONNECT native live endpoint payload"
            );
            events.borrow_mut().push("post:opaque".into());
        } else {
            let Native::Text(units) = url.as_ref() else {
                panic!("decoded endpoint")
            };
            assert_eq!(
                *units,
                if private {
                    "/api/v1/bullet-private"
                } else {
                    "/api/v1/bullet-public"
                }
                .encode_utf16()
                .collect::<Vec<_>>(),
                "CONNECT native endpoint argument"
            );
            events
                .borrow_mut()
                .push(format!("post:{}", String::from_utf16(units).unwrap()));
        }
        if scenario == "post-sync-error" {
            return Err(error.clone());
        }
        Ok(Response {
            pending: true,
            reject: matches!(scenario, "post-reject" | "retry-throws"),
            dispose: scenario == "dispose-await",
            disposed: &disposed,
            events: &events,
            value: Rc::new(Native::Object("response")),
            error: error.clone(),
        })
    };
    let get = |v: &V, key: &str| -> Result<V, Error> {
        let Native::Object(tag) = v.as_ref() else {
            if key == "?.endpoint" {
                events.borrow_mut().push("get:optional.endpoint".into());
                return Ok(undefined());
            }
            return Err(error.clone());
        };
        events.borrow_mut().push(format!("get:{tag}.{key}"));
        Ok(match (*tag, key) {
            ("response", "data") => Rc::new(Native::Object("data")),
            ("data", "code") => {
                if scenario == "numeric-code" {
                    Rc::new(Native::Number(200000.0))
                } else {
                    text(if matches!(scenario, "bad-code" | "emit-error-throws") {
                        "bad"
                    } else {
                        "200000"
                    })
                }
            }
            ("data", "data") => {
                if scenario == "missing-data" {
                    undefined()
                } else {
                    Rc::new(Native::Object("bucket"))
                }
            }
            ("bucket", "token") => token.clone(),
            ("bucket", "instanceServers") => {
                if scenario == "no-servers" {
                    Rc::new(Native::Null)
                } else {
                    Rc::new(Native::Object("servers"))
                }
            }
            ("bucket", "instanceServers?.0") => {
                events
                    .borrow_mut()
                    .push("get:bucket.instanceServers".into());
                if scenario == "no-servers" {
                    undefined()
                } else {
                    events.borrow_mut().push("get:servers.0".into());
                    Rc::new(Native::Object("server"))
                }
            }
            ("servers", "length") => Rc::new(Native::Number(if scenario == "zero-length" {
                0.0
            } else {
                1.0
            })),
            ("servers", "0") => Rc::new(Native::Object("server")),
            ("server", "endpoint" | "?.endpoint") => {
                if key == "?.endpoint" {
                    events.borrow_mut().pop();
                    events.borrow_mut().push("get:server.endpoint".into());
                }
                if scenario == "endpoint-throws" {
                    return Err(error.clone());
                }
                if scenario == "missing-endpoint" {
                    undefined()
                } else {
                    endpoint.clone()
                }
            }
            ("server", "pingInterval") => Rc::new(Native::Number(if scenario == "ping-zero" {
                0.0
            } else {
                7000.0
            })),
            _ => panic!("unmodeled get {tag} {key}"),
        })
    };
    let code_ok = |v: &V| matches!(v.as_ref(),Native::Text(s) if *s=="200000".encode_utf16().collect::<Vec<_>>());
    let read_state = |key: &str| {
        events.borrow_mut().push(format!("state:{key}"));
        let s = state.borrow();
        Ok(match key {
            "token" => s.0.clone(),
            "socketEndpoint" => s.1.clone(),
            "pingInterval" => s.2.clone(),
            _ => panic!("state {key}"),
        })
    };
    let call = |op: &str, v: Option<&V>, arg: Option<&str>| -> Result<V, Error> {
        match op {
            "set:token" => {
                assert!(
                    Rc::ptr_eq(v.unwrap(), &token),
                    "CONNECT native raw token identity"
                );
                events.borrow_mut().push(op.into());
                state.borrow_mut().0 = v.unwrap().clone();
            }
            "set:socketEndpoint" => {
                assert!(
                    matches!(v.unwrap().as_ref(), Native::Text(_)),
                    "CONNECT native decoded URL string"
                );
                events.borrow_mut().push(op.into());
                state.borrow_mut().1 = v.unwrap().clone();
            }
            "set:pingInterval" => {
                events.borrow_mut().push(op.into());
                state.borrow_mut().2 = v.unwrap().clone();
            }
            "create-current" | "create-captured" => {
                events.borrow_mut().push("create:get".into());
                if op == "create-current" {
                    events.borrow_mut().push("state:socketEndpoint".into());
                } else {
                    assert!(
                        Rc::ptr_eq(v.unwrap(), &state.borrow().1),
                        "CONNECT native captured private endpoint"
                    );
                }
                events.borrow_mut().push("create:call".into());
                return Ok(socket.clone());
            }
            "set:ws" => {
                assert!(
                    Rc::ptr_eq(v.unwrap(), &socket),
                    "CONNECT native created socket identity"
                );
                events.borrow_mut().push(op.into());
                state.borrow_mut().3 = v.unwrap().clone();
            }
            "add-listener" => {
                let event = arg.unwrap();
                let current = state.borrow().3.clone();
                assert_eq!(
                    current.as_ref(),
                    &Native::Object(if scenario == "listener-replace" && event != "open" {
                        "replacement"
                    } else {
                        "new"
                    }),
                    "CONNECT native live listener socket"
                );
                events.borrow_mut().push(format!("listener:{event}"));
                if scenario == "listener-replace" && event == "open" {
                    state.borrow_mut().3 = Rc::new(Native::Object("replacement"));
                }
            }
            "emit-error" | "emit-token-error" => {
                let mut message = arg.unwrap().to_owned();
                if op == "emit-token-error" {
                    assert_eq!(v.unwrap().as_ref(), &Native::Object("data"));
                    events.borrow_mut().push("get:data.msg".into());
                    message = "venue message".into();
                }
                events.borrow_mut().push(format!("emit:{message}"));
                if scenario == "emit-error-throws" {
                    return Err(error.clone());
                }
            }
            _ => panic!("call {op}"),
        }
        Ok(undefined())
    };
    let template = |v: &V| {
        if Rc::ptr_eq(v, &endpoint) {
            events.borrow_mut().push("format:endpoint".into());
            if scenario == "endpoint-reenter" {
                state.borrow_mut().0 = text("NEW");
            }
        } else if Rc::ptr_eq(v, &date) {
            events.borrow_mut().push("format:date".into());
        } else {
            events.borrow_mut().push("format:token".into());
        }
        Ok(match v.as_ref() {
            Native::Text(s) => s.clone(),
            Native::Undefined => "undefined".encode_utf16().collect(),
            Native::Number(n) => safe_cex_num::js_number_string(*n).encode_utf16().collect(),
            _ => panic!("format {v:?}"),
        })
    };
    let date_now = || {
        events.borrow_mut().push("date".into());
        Ok(date.clone())
    };
    let string_value = |s| Rc::new(Native::Text(s));
    let retry = |actual: &Error, fallback: &str| {
        assert_error(actual, &error);
        assert_eq!(
            fallback,
            match kind {
                0 => "Failed to connect to WebSocket",
                1 => "Failed to connect to private WebSocket",
                2 => "Failed to connect KuCoin Spot public WebSocket",
                _ => "Failed to connect KuCoin Spot private WebSocket",
            },
            "CONNECT native fallback"
        );
        events.borrow_mut().push("retry:setup failure".into());
        if scenario == "retry-throws" {
            Err(error.clone())
        } else {
            Ok(())
        }
    };
    let result = match kind {
        0 => run(connect_futures_public(
            guard,
            post,
            get,
            code_ok,
            truthy,
            read_state,
            call,
            template,
            date_now,
            string_value,
            retry,
        )),
        1 => run(connect_futures_private(
            guard,
            post,
            get,
            code_ok,
            truthy,
            read_state,
            call,
            template,
            date_now,
            string_value,
            retry,
        )),
        2 => run(connect_spot_public(
            guard,
            post,
            get,
            code_ok,
            truthy,
            read_state,
            call,
            template,
            date_now,
            string_value,
            retry,
        )),
        _ => run(connect_spot_private(
            guard,
            post,
            get,
            code_ok,
            truthy,
            read_state,
            call,
            template,
            date_now,
            string_value,
            retry,
        )),
    };
    let mut expected = vec!["disposed".to_owned()];
    let mut token_expected = text("OLD");
    let mut endpoint_expected = text("OLD-ENDPOINT");
    let mut ping_expected = 5000.0;
    let mut ws_expected = "old";
    if !matches!(scenario, "disposed" | "guard-error") {
        if scenario == "live-endpoint" {
            expected.extend(["post:get", "endpoint:get", "post:opaque"].map(str::to_owned));
        } else {
            expected.push(format!(
                "post:{}",
                if private {
                    "/api/v1/bullet-private"
                } else {
                    "/api/v1/bullet-public"
                }
            ));
        }
        if !matches!(scenario, "post-sync-error") {
            expected.push("resolved".into());
        }
        if matches!(scenario, "post-sync-error" | "post-reject" | "retry-throws") {
            expected.push("retry:setup failure".into());
        } else {
            expected.extend(["get:response.data", "get:data.code"].map(str::to_owned));
            let bad = matches!(scenario, "bad-code" | "numeric-code" | "emit-error-throws");
            if !bad {
                expected.push("get:data.data".into());
            }
            if bad || scenario == "missing-data" {
                if spot {
                    expected.push("get:data.msg".into());
                }
                expected.push(format!(
                    "emit:{}",
                    if spot {
                        "venue message"
                    } else if private {
                        "Failed to get private WebSocket token"
                    } else {
                        "Failed to get WebSocket token"
                    }
                ));
                if scenario == "emit-error-throws" {
                    expected.push("retry:setup failure".into());
                }
            } else {
                if !spot {
                    expected.extend(
                        ["get:data.data", "get:bucket.token", "set:token"].map(str::to_owned),
                    );
                    token_expected = token.clone();
                }
                expected.push("get:data.data".into());
                if spot {
                    expected.push("get:bucket.instanceServers?.0".into());
                }
                expected.push("get:bucket.instanceServers".into());
                if spot && scenario != "no-servers" {
                    expected.push("get:servers.0".into());
                }
                if !spot && scenario != "no-servers" {
                    expected.push("get:servers.length".into());
                }
                if spot {
                    expected.push(
                        if scenario == "no-servers" {
                            "get:optional.endpoint"
                        } else {
                            "get:server.endpoint"
                        }
                        .into(),
                    );
                }
                let invalid = if spot {
                    matches!(scenario, "no-servers" | "missing-endpoint")
                } else {
                    matches!(scenario, "no-servers" | "zero-length")
                };
                if spot && scenario == "endpoint-throws" {
                    expected.push("retry:setup failure".into());
                } else if invalid {
                    expected.push(format!(
                        "emit:{}",
                        match kind {
                            0 => "No WebSocket servers available",
                            1 => "No private WebSocket servers available",
                            2 => "No KuCoin Spot public WebSocket servers available",
                            _ => "No KuCoin Spot private WebSocket servers available",
                        }
                    ));
                } else {
                    if !spot {
                        expected.push("get:servers.0".into());
                    } else {
                        expected.extend(
                            [
                                "get:data.data",
                                "get:bucket.token",
                                "set:token",
                                "get:server.pingInterval",
                            ]
                            .map(str::to_owned),
                        );
                        token_expected = token.clone();
                        if scenario == "ping-zero" {
                            expected.push("state:pingInterval".into());
                        } else {
                            ping_expected = 7000.0;
                        }
                        expected.push("set:pingInterval".into());
                    }
                    expected.push("get:server.endpoint".into());
                    if scenario == "endpoint-throws" {
                        expected.push("retry:setup failure".into());
                    } else {
                        expected.push(
                            if scenario == "missing-endpoint" {
                                "format:token"
                            } else {
                                "format:endpoint"
                            }
                            .into(),
                        );
                        if scenario == "endpoint-reenter" {
                            token_expected = text("NEW");
                        }
                        expected.extend(
                            [
                                "state:token",
                                "format:token",
                                "date",
                                "format:date",
                                "set:socketEndpoint",
                            ]
                            .map(str::to_owned),
                        );
                        let mut url = if scenario == "missing-endpoint" {
                            "undefined".encode_utf16().collect::<Vec<_>>()
                        } else {
                            vec![119, 115, 115, 58, 47, 47, 0xd800]
                        };
                        url.extend(
                            format!(
                                "?token={}&connectId=41",
                                if scenario == "endpoint-reenter" {
                                    "NEW"
                                } else {
                                    "TK"
                                }
                            )
                            .encode_utf16(),
                        );
                        endpoint_expected = Rc::new(Native::Text(url));
                        if kind == 1 {
                            expected.push("state:socketEndpoint".into());
                        }
                        expected.push("create:get".into());
                        if kind != 1 {
                            expected.push("state:socketEndpoint".into());
                        }
                        expected.extend(
                            [
                                "create:call",
                                "set:ws",
                                "listener:open",
                                "listener:message",
                                "listener:close",
                            ]
                            .map(str::to_owned),
                        );
                        ws_expected = if scenario == "listener-replace" {
                            "replacement"
                        } else {
                            "new"
                        };
                    }
                }
            }
        }
    }
    assert_eq!(
        *events.borrow(),
        expected,
        "CONNECT native live order and catch scope"
    );
    let s = state.borrow();
    assert_eq!(s.0, token_expected, "CONNECT native token state");
    assert_eq!(s.1, endpoint_expected, "CONNECT native UTF16 URL state");
    assert_eq!(
        s.2.as_ref(),
        &Native::Number(ping_expected),
        "CONNECT native ping state"
    );
    assert_eq!(
        s.3.as_ref(),
        &Native::Object(ws_expected),
        "CONNECT native socket state"
    );
    let should_throw = matches!(scenario, "guard-error" | "retry-throws");
    assert_eq!(
        result.is_err(),
        should_throw,
        "CONNECT native outer throw boundary"
    );
    if let Err(e) = result {
        assert_error(&e, &error);
    }
    json!({"kind":kind,"scenario":scenario,"events":events.borrow().clone(),"token":wire(&s.0),"endpoint":wire(&s.1),"ping":wire(&s.2),"socket":wire(&s.3),"throw":should_throw})
}

#[test]
fn socket_setup_native_before_source() {
    let connects = (0..4)
        .flat_map(|kind| SCENARIOS.into_iter().map(move |s| connect_case(kind, s)))
        .collect::<Vec<_>>();
    assert_eq!(connects.len(), 76, "SETUP independent connection floor");
    let mut reconnects = vec![];
    for private in [false, true] {
        for scenario in [
            "normal",
            "disposed",
            "null-socket",
            "remove-error",
            "close-error",
            "connect-sync-error",
            "connect-reject",
            "replace-socket",
        ] {
            let events = RefCell::new(Vec::new());
            let old = Rc::new(17);
            let ws = RefCell::new(Some(old.clone()));
            let error = failure();
            let guard = || Ok(scenario == "disposed");
            let capture = || {
                events.borrow_mut().push("capture".to_owned());
                Ok(if scenario == "null-socket" {
                    None
                } else {
                    Some(old.clone())
                })
            };
            let socket_call = |v: &Option<Rc<i32>>, op: &str| {
                if let Some(v) = v {
                    assert!(
                        Rc::ptr_eq(v, &old),
                        "RECONNECT native captured socket identity"
                    );
                    assert_eq!(**v, 17, "RECONNECT native captured payload");
                    events.borrow_mut().push(op.into());
                    if (scenario == "remove-error" && op == "message")
                        || (scenario == "close-error" && op == "close-socket")
                    {
                        return Err(error.clone());
                    }
                    if scenario == "replace-socket" && op == "open" {
                        *ws.borrow_mut() = Some(Rc::new(9));
                    }
                    if op == "close-socket" {
                        assert!(
                            ws.borrow().is_none(),
                            "RECONNECT native undefined before close"
                        );
                    }
                }
                Ok(())
            };
            let effect = |op: &str| {
                events.borrow_mut().push(op.into());
                if op == "ws:undefined" {
                    *ws.borrow_mut() = None;
                }
                Ok(())
            };
            let connect = || {
                events.borrow_mut().push("connect".into());
                if scenario == "connect-sync-error" {
                    return Err(error.clone());
                }
                Ok(std::future::ready(if scenario == "connect-reject" {
                    Err(error.clone())
                } else {
                    Ok(())
                }))
            };
            let result = if private {
                run(reconnect_futures_private(
                    guard,
                    capture,
                    socket_call,
                    effect,
                    connect,
                ))
            } else {
                run(reconnect_futures_public(
                    guard,
                    capture,
                    socket_call,
                    effect,
                    connect,
                ))
            };
            let expected: Vec<&str> = match scenario {
                "disposed" => vec![],
                "remove-error" => vec!["capture", "open", "message"],
                "close-error" => vec![
                    "capture",
                    "open",
                    "message",
                    "close",
                    "ws:undefined",
                    "close-socket",
                ],
                "null-socket" => {
                    if private {
                        vec![
                            "capture",
                            "ws:undefined",
                            "token:null",
                            "socketEndpoint:null",
                            "subscribed:false",
                            "connect",
                        ]
                    } else {
                        vec![
                            "capture",
                            "ws:undefined",
                            "token:null",
                            "socketEndpoint:null",
                            "connect",
                        ]
                    }
                }
                _ => {
                    if private {
                        vec![
                            "capture",
                            "open",
                            "message",
                            "close",
                            "ws:undefined",
                            "close-socket",
                            "token:null",
                            "socketEndpoint:null",
                            "subscribed:false",
                            "connect",
                        ]
                    } else {
                        vec![
                            "capture",
                            "open",
                            "message",
                            "close",
                            "ws:undefined",
                            "close-socket",
                            "token:null",
                            "socketEndpoint:null",
                            "connect",
                        ]
                    }
                }
            };
            assert_eq!(
                *events.borrow(),
                expected,
                "RECONNECT native teardown reset await order"
            );
            let throws = matches!(
                scenario,
                "remove-error" | "close-error" | "connect-sync-error" | "connect-reject"
            );
            assert_eq!(result.is_err(), throws);
            if let Err(e) = result {
                assert_error(&e, &error);
            }
            reconnects.push(json!({"private":private,"scenario":scenario,"events":events.borrow().clone(),"throw":throws}));
        }
    }
    assert_eq!(reconnects.len(), 16, "SETUP independent reconnect floor");
    let mut constructors = vec![];
    for scenario in [
        "absent",
        "null",
        "defaults",
        "false-zero",
        "values",
        "super-error",
        "role-error",
        "index-error",
    ] {
        let events = RefCell::new(Vec::new());
        let assigned = RefCell::new(Vec::<(&str, V)>::new());
        let error = failure();
        let parent = Rc::new(17);
        let got_parent = parent.clone();
        let result = public_constructor(
            parent,
            |p| {
                assert!(Rc::ptr_eq(&p, &got_parent), "CTOR native parent identity");
                events.borrow_mut().push("super".to_owned());
                if scenario == "super-error" {
                    Err(error.clone())
                } else {
                    Ok(())
                }
            },
            || {
                events.borrow_mut().push("initialize".into());
                Ok(())
            },
            |field| {
                events.borrow_mut().push(format!("get:{field}"));
                if (scenario == "role-error" && field == "role")
                    || (scenario == "index-error" && field == "shardIndex")
                {
                    return Err(error.clone());
                }
                Ok(if scenario == "false-zero" {
                    Some(if field == "role" {
                        Rc::new(Native::Bool(false))
                    } else {
                        Rc::new(Native::Number(0.0))
                    })
                } else if scenario == "values" {
                    Some(if field == "role" {
                        text("snapshot")
                    } else {
                        Rc::new(Native::Number(if field == "shardIndex" {
                            2.0
                        } else {
                            3.0
                        }))
                    })
                } else {
                    None
                })
            },
            |fallback| match fallback {
                "detail" => text("detail"),
                "0" => Rc::new(Native::Number(0.0)),
                _ => Rc::new(Native::Number(1.0)),
            },
            |field, v| {
                events.borrow_mut().push(format!("set:{field}"));
                assigned.borrow_mut().push((
                    if field == "role" {
                        "role"
                    } else if field == "shardIndex" {
                        "shardIndex"
                    } else {
                        "shardCount"
                    },
                    v,
                ));
                Ok(())
            },
        );
        let expected = match scenario {
            "super-error" => vec!["super"],
            "role-error" => vec!["super", "initialize", "get:role"],
            "index-error" => vec![
                "super",
                "initialize",
                "get:role",
                "set:role",
                "get:shardIndex",
            ],
            _ => vec![
                "super",
                "initialize",
                "get:role",
                "set:role",
                "get:shardIndex",
                "set:shardIndex",
                "get:shardCount",
                "set:shardCount",
            ],
        };
        assert_eq!(
            *events.borrow(),
            expected,
            "CTOR native super initialization nullish order"
        );
        if !matches!(scenario, "super-error" | "role-error" | "index-error") {
            let a = assigned.borrow();
            assert_eq!(
                a[0].1,
                if scenario == "false-zero" {
                    Rc::new(Native::Bool(false))
                } else {
                    text(if scenario == "values" {
                        "snapshot"
                    } else {
                        "detail"
                    })
                },
                "CTOR native nullish role"
            );
            assert_eq!(
                a[1].1.as_ref(),
                &Native::Number(if scenario == "values" { 2.0 } else { 0.0 })
            );
            assert_eq!(
                a[2].1.as_ref(),
                &Native::Number(if scenario == "values" {
                    3.0
                } else if scenario == "false-zero" {
                    0.0
                } else {
                    1.0
                }),
                "CTOR native nullish shardCount"
            );
        }
        let throws = matches!(scenario, "super-error" | "role-error" | "index-error");
        assert_eq!(result.is_err(), throws);
        if let Err(e) = result {
            assert_error(&e, &error);
        }
        constructors.push(json!({"scenario":scenario,"events":events.borrow().clone(),"fields":assigned.borrow().iter().map(|(f,v)|json!([f,wire(v)])).collect::<Vec<_>>(),"throw":throws}));
    }
    assert_eq!(constructors.len(), 8, "SETUP independent constructor floor");
    let mut pools = vec![];
    for (tag, count, expected) in [
        ("negative", -1.0, 1usize),
        ("zero", 0.0, 1),
        ("380", 380.0, 1),
        ("381", 381.0, 2),
        ("760", 760.0, 2),
        ("761", 761.0, 3),
        ("fraction", 380.5, 2),
        ("nan", f64::NAN, 0),
        ("infinity", f64::INFINITY, 0),
        ("construct-error", 761.0, 3),
    ] {
        let error = if tag == "infinity" {
            Rc::new(Failure {
                name: "RangeError",
                message: "Invalid array length",
                code: None,
                ids: None,
            })
        } else {
            failure()
        };
        let events = RefCell::new(Vec::new());
        let parent = Rc::new(17);
        let seen = Cell::new(0);
        let result = create_snapshot_websockets(
            &parent,
            count,
            |length, make| {
                if tag == "nan" {
                    assert!(length.is_nan(), "POOL native NaN propagation");
                    return Ok(vec![]);
                }
                if tag == "infinity" {
                    assert!(length.is_infinite());
                    return Err(error.clone());
                }
                assert_eq!(
                    length, expected as f64,
                    "POOL native independent shard ceiling"
                );
                let mut sockets = vec![];
                for index in 0..expected {
                    sockets.push(make(index)?);
                }
                Ok(sockets)
            },
            |p, role, index, shards| {
                assert!(Rc::ptr_eq(p, &parent));
                assert_eq!(**p, 17);
                assert_eq!(role, "snapshot", "POOL native role");
                assert_eq!(shards, expected as f64);
                assert_eq!(index, seen.get());
                seen.set(seen.get() + 1);
                events.borrow_mut().push(index);
                if tag == "construct-error" && index == 1 {
                    return Err(error.clone());
                }
                Ok(Rc::new(index))
            },
        );
        let visits = if tag == "construct-error" {
            2
        } else {
            expected
        };
        assert_eq!(seen.get(), visits, "POOL native constructor floor");
        if let Ok(sockets) = &result {
            assert_eq!(sockets.len(), expected);
            for (i, v) in sockets.iter().enumerate() {
                assert_eq!(**v, i, "POOL native decoded socket");
            }
        }
        let throws = matches!(tag, "infinity" | "construct-error");
        assert_eq!(result.is_err(), throws);
        if let Err(e) = result {
            assert_error(&e, &error);
        }
        pools.push(json!({"scenario":tag,"events":events.borrow().clone(),"throw":throws}));
    }
    assert_eq!(pools.len(), 10, "SETUP independent pool floor");
    let mut subscriptions = vec![];
    for scenario in [
        "normal",
        "mutate-first",
        "mutate-later",
        "first-error",
        "candle-error",
        "enumerate-error",
    ] {
        let error = failure();
        let events = RefCell::new(Vec::<Vec<u16>>::new());
        let keys = RefCell::new(vec![
            "candle_BTC_1min".encode_utf16().collect::<Vec<_>>(),
            "orderBook_BTC".encode_utf16().collect(),
            "candle_ETH".encode_utf16().collect(),
            "candle_X__extra".encode_utf16().collect(),
            vec![99, 97, 110, 100, 108, 101, 95, 0xd800, 95, 49],
            "other".encode_utf16().collect(),
        ]);
        let result = subscribe_spot_public(
            |visit| {
                if scenario == "enumerate-error" {
                    return Err(error.clone());
                }
                let snapshot = keys.borrow().clone();
                for key in snapshot {
                    visit(key)?;
                }
                Ok(())
            },
            |topic| {
                events.borrow_mut().push(topic.to_vec());
                let n = events.borrow().len();
                if scenario == "mutate-first" && n == 1 {
                    keys.borrow_mut()
                        .push("orderBook_NEW".encode_utf16().collect());
                }
                if scenario == "mutate-later" && n == 2 {
                    keys.borrow_mut().clear();
                }
                if (scenario == "first-error" && n == 1) || (scenario == "candle-error" && n == 2) {
                    return Err(error.clone());
                }
                Ok(())
            },
        );
        let mut expected = vec!["/market/ticker:all".encode_utf16().collect::<Vec<_>>()];
        if !matches!(scenario, "first-error" | "enumerate-error") {
            expected.push("/market/candles:BTC_1min".encode_utf16().collect());
            if scenario != "candle-error" {
                expected.extend(
                    [
                        "/spotMarket/level2Depth50:BTC",
                        "/market/candles:ETH_undefined",
                        "/market/candles:X_",
                    ]
                    .map(|s| s.encode_utf16().collect::<Vec<_>>()),
                );
                let mut lone = "/market/candles:".encode_utf16().collect::<Vec<_>>();
                lone.extend([0xd800, 95, 49]);
                expected.push(lone);
                if scenario == "mutate-first" {
                    expected.push("/spotMarket/level2Depth50:NEW".encode_utf16().collect());
                }
            }
        }
        assert_eq!(
            *events.borrow(),
            expected,
            "SUBSCRIBE native snapshot UTF16 topics and undefined interval"
        );
        let throws = matches!(scenario, "first-error" | "candle-error" | "enumerate-error");
        assert_eq!(result.is_err(), throws);
        if let Err(e) = result {
            assert_error(&e, &error);
        }
        subscriptions
            .push(json!({"scenario":scenario,"topics":events.borrow().clone(),"throw":throws}));
    }
    assert_eq!(subscriptions.len(), 6, "SETUP independent subscribe floor");
    let expected = json!({"connects":connects,"reconnects":reconnects,"constructors":constructors,"pools":pools,"subscriptions":subscriptions});
    let mut child = Command::new("node")
        .arg(format!(
            "{}/scripts/socket-setup.cjs",
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
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    for family in [
        "connects",
        "reconnects",
        "constructors",
        "pools",
        "subscriptions",
    ] {
        let rows = expected[family].as_array().unwrap();
        assert_eq!(
            source[family].as_array().unwrap().len(),
            rows.len(),
            "source family length {family}"
        );
        for (index, row) in rows.iter().enumerate() {
            assert_eq!(
                row, &source[family][index],
                "SETUP maintained source after all native assertions: {family} {index}"
            );
        }
    }
}
