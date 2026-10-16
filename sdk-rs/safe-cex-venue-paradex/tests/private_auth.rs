use safe_cex_venue_paradex::auth_operations as api;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Error = Rc<str>;
type Token = Option<Rc<Vec<u16>>>;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn surrogate(s: &str) -> Vec<u16> {
    units(s).into_iter().chain([0xd800]).collect()
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn decode(v: &Value) -> Vec<u16> {
    serde_json::from_value(v.clone()).unwrap()
}
fn poll<F: Future>(f: Pin<&mut F>) -> Poll<F::Output> {
    f.poll(&mut Context::from_waker(Waker::noop()))
}
#[derive(Clone)]
struct Held<T: Clone> {
    ready: Rc<Cell<bool>>,
    result: Result<T, Error>,
}
impl<T: Clone> Future for Held<T> {
    type Output = Result<T, Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready.get() {
            Poll::Ready(self.result.clone())
        } else {
            Poll::Pending
        }
    }
}
fn outcome<T>(result: &Result<T, Error>, expected: &Value, errors: &[Error], op: &str) {
    match result {
        Ok(_) => assert_eq!(expected["ok"], true, "{op}_native_success"),
        Err(e) => {
            assert_eq!(expected["ok"], false, "{op}_native_failure");
            let wanted = errors
                .iter()
                .find(|m| m.as_ref() == expected["error"].as_str().unwrap())
                .unwrap();
            assert!(Rc::ptr_eq(e, wanted), "{op}_native_error_identity");
        }
    }
}
fn constructor(row: &Value) {
    let kind = row["kind"].as_str().unwrap();
    let provided = row["provided"].as_bool().unwrap();
    let testnet = row["testnet"].as_bool().unwrap();
    let failure = row["failure"].as_str().unwrap();
    let trace = RefCell::new(Vec::new());
    let error: Error = Rc::from("primary");
    let values = RefCell::new(
        json!({"sdk":false,"options":"unset","subkey":null,"address":null,"url":null}),
    );
    let account = Rc::new(());
    let original = Rc::new(());
    let touch = |name: &str| {
        trace.borrow_mut().push(name.to_owned());
        if name == failure {
            Err(error.clone())
        } else {
            Ok(())
        }
    };
    let write_account = |given: &Rc<()>| {
        assert!(
            Rc::ptr_eq(given, &account),
            "signer_constructor_native_account_identity"
        );
        touch("sdkAccount")?;
        values.borrow_mut()["sdk"] = json!(true);
        Ok(())
    };
    let write_options = |given: Rc<()>| {
        touch("options")?;
        values.borrow_mut()["options"] = json!(if Rc::ptr_eq(&given, &original) {
            "original"
        } else {
            "empty"
        });
        Ok(())
    };
    let options = provided.then(|| original.clone());
    let empty = || Ok(Rc::new(()));
    let result = if kind == "signer" {
        api::initialize_request_signer(&account, options, empty, write_account, write_options)
    } else {
        api::initialize_jwt_manager(
            &account,
            testnet,
            surrogate("subkey-"),
            options,
            empty,
            write_account,
            write_options,
            |v| {
                assert_eq!(v, surrogate("subkey-"), "jwt_constructor_native_subkey");
                touch("subkeyAddress")?;
                values.borrow_mut()["subkey"] = json!(v);
                Ok(())
            },
            |given| {
                assert!(Rc::ptr_eq(given, &account));
                touch("account-get")?;
                touch("address-get")?;
                Ok(surrogate("address-"))
            },
            |v| {
                assert_eq!(v, surrogate("address-"), "jwt_constructor_native_address");
                touch("mainAccountAddress")?;
                values.borrow_mut()["address"] = json!(v);
                Ok(())
            },
            |url| {
                assert_eq!(
                    url,
                    if testnet {
                        "https://api.testnet.paradex.trade/v1"
                    } else {
                        "https://api.prod.paradex.trade/v1"
                    },
                    "jwt_constructor_native_url"
                );
                touch("apiBaseUrl")?;
                values.borrow_mut()["url"] = json!(url);
                Ok(())
            },
        )
    };
    outcome(&result, &row["outcome"], &[error], kind);
    assert_eq!(
        *values.borrow(),
        row["values"],
        "{kind}_constructor_native_properties"
    );
    assert_eq!(
        json!(*trace.borrow()),
        row["trace"],
        "{kind}_constructor_native_write_order"
    );
}
struct TokenState {
    trace: RefCell<Vec<Value>>,
    token: RefCell<Token>,
    expiry: Cell<f64>,
    refresh: Cell<Option<usize>>,
    token_reads: Cell<usize>,
    refresh_reads: Cell<usize>,
    writes: Cell<usize>,
    generated: Cell<usize>,
    now_index: Cell<usize>,
    held: [Held<Token>; 2],
    errors: [Error; 3],
    mode: String,
    race: bool,
}
impl TokenState {
    fn touch(&self, name: &str) -> Result<(), Error> {
        self.trace.borrow_mut().push(json!(name));
        if self.mode == name {
            Err(self.errors[0].clone())
        } else {
            Ok(())
        }
    }
    fn token_read(&self) -> Result<Token, Error> {
        let i = self.token_reads.get();
        self.token_reads.set(i + 1);
        if !self.race {
            self.touch(&format!("token{i}"))?;
            if self.mode == "token-return" && i == 1 {
                return Err(self.errors[0].clone());
            }
        }
        Ok(self.token.borrow().clone())
    }
    fn refresh_read(&self) -> Result<Option<usize>, Error> {
        let i = self.refresh_reads.get();
        self.refresh_reads.set(i + 1);
        if !self.race {
            self.touch(&format!("refresh{i}"))?;
        }
        Ok(self.refresh.get())
    }
    fn refresh_write(&self, v: Option<usize>) -> Result<(), Error> {
        self.writes.set(self.writes.get() + 1);
        self.trace.borrow_mut().push(if self.race {
            json!(["refresh", v])
        } else {
            json!(if v.is_some() {
                "refresh-write"
            } else {
                "clear"
            })
        });
        if v.is_none() && self.mode == "clear-error" {
            return Err(self.errors[2].clone());
        }
        self.refresh.set(v);
        Ok(())
    }
    async fn run(&self) -> Result<Token, Error> {
        api::get_token(
            || {
                let i = self.now_index.get();
                self.now_index.set(i + 1);
                let n = if self.race && i > 0 { 4000.0 } else { 1000.0 };
                if self.race {
                    self.trace.borrow_mut().push(json!(["now", bits(n)]));
                } else {
                    self.touch("now")?;
                }
                Ok(n)
            },
            || Ok(self.token_read()?.as_ref().is_some_and(|v| !v.is_empty())),
            || {
                if !self.race {
                    self.touch("expiry")?;
                }
                Ok(self.expiry.get())
            },
            || self.token_read(),
            || Ok(self.refresh_read()?.is_some()),
            || {
                let i = self.generated.get();
                self.generated.set(i + 1);
                if self.race {
                    self.trace.borrow_mut().push(json!(["generate", i]));
                } else {
                    self.touch("generate")?;
                    if self.mode == "generator-error" {
                        return Err(self.errors[0].clone());
                    }
                }
                Ok(i)
            },
            |v| self.refresh_write(Some(v)),
            || {
                let i = self.refresh_read()?.unwrap();
                Ok(self.held[i].clone())
            },
            |v| {
                if self.race {
                    self.trace
                        .borrow_mut()
                        .push(json!(["token", v.as_deref().map(Vec::as_slice)]));
                } else {
                    self.touch("token-write")?;
                }
                *self.token.borrow_mut() = v;
                Ok(())
            },
            |v| {
                if self.race {
                    self.trace.borrow_mut().push(json!(["expiry", bits(v)]));
                } else {
                    self.touch("expiry-write")?;
                }
                self.expiry.set(v);
                Ok(())
            },
            |prefix, e| {
                assert_eq!(prefix, "[Paradex JWT] ✗ Failed to obtain token:");
                assert!(
                    Rc::ptr_eq(e, &self.errors[0]),
                    "token_native_logged_error_identity"
                );
                assert!(!self.race);
                self.trace.borrow_mut().push(json!("logger"));
                if self.mode == "logger-error" {
                    Err(self.errors[1].clone())
                } else {
                    Ok(())
                }
            },
            || self.refresh_write(None),
        )
        .await
    }
    fn new(mode: &str, race: bool) -> Self {
        let errors: [Error; 3] = [Rc::from("primary"), Rc::from("logger"), Rc::from("clear")];
        let failure = ["shared-error", "fresh-error", "logger-error"].contains(&mode);
        let first = if race {
            surrogate("first-")
        } else {
            surrogate("new-")
        };
        Self {
            trace: RefCell::new(vec![]),
            token: RefCell::new(if ["cached", "boundary", "expired"].contains(&mode) {
                Some(Rc::new(surrogate("cached-")))
            } else if mode == "empty" {
                Some(Rc::new(vec![]))
            } else {
                None
            }),
            expiry: Cell::new(if mode == "cached" {
                200000.0
            } else if mode == "boundary" {
                121000.0
            } else {
                0.0
            }),
            refresh: Cell::new(mode.starts_with("shared").then_some(0)),
            token_reads: Cell::new(0),
            refresh_reads: Cell::new(0),
            writes: Cell::new(0),
            generated: Cell::new(0),
            now_index: Cell::new(0),
            held: [
                Held {
                    ready: Rc::new(Cell::new(false)),
                    result: if failure {
                        Err(errors[0].clone())
                    } else {
                        Ok(Some(Rc::new(first)))
                    },
                },
                Held {
                    ready: Rc::new(Cell::new(false)),
                    result: Ok(Some(Rc::new(surrogate("second-")))),
                },
            ],
            errors,
            mode: mode.to_owned(),
            race,
        }
    }
    fn check_result(&self, result: &Result<Token, Error>, row: &Value) {
        outcome(result, row, &self.errors, "token");
        if let Ok(v) = result {
            match v {
                None => assert_eq!(row["value"], "null", "token_native_value"),
                Some(v) => {
                    assert_eq!(v.as_slice(), decode(&row["value"]), "token_native_value");
                    let expected = if self.mode == "cached" {
                        self.token.borrow().as_ref().unwrap().clone()
                    } else {
                        self.held[if self.race && v.as_slice() == surrogate("second-") {
                            1
                        } else {
                            0
                        }]
                        .result
                        .as_ref()
                        .unwrap()
                        .as_ref()
                        .unwrap()
                        .clone()
                    };
                    assert!(Rc::ptr_eq(v, &expected), "token_native_identity");
                }
            }
        }
    }
}
fn token_case(row: &Value) {
    let s = TokenState::new(row["mode"].as_str().unwrap(), false);
    let mut f = std::pin::pin!(s.run());
    let first = poll(f.as_mut());
    let before = json!({"trace":*s.trace.borrow(),"settled":first.is_ready(),"token":s.token.borrow().as_deref().map(Vec::as_slice),"expiry":bits(s.expiry.get()),"refresh":s.refresh.get().is_some()});
    assert_eq!(before, row["before"], "token_native_await_boundary");
    s.held[0].ready.set(true);
    let result = match first {
        Poll::Ready(r) => r,
        Poll::Pending => match poll(f.as_mut()) {
            Poll::Ready(r) => r,
            Poll::Pending => panic!("token_native_completion"),
        },
    };
    s.check_result(&result, &row["outcome"]);
    assert_eq!(
        json!(s.token.borrow().as_deref().map(Vec::as_slice)),
        row["token"],
        "token_native_cache"
    );
    assert_eq!(bits(s.expiry.get()), row["expiry"], "token_native_expiry");
    assert_eq!(
        s.refresh.get().is_some(),
        row["refresh"],
        "token_native_finally"
    );
    assert_eq!(s.writes.get(), row["refreshWrites"]);
    assert_eq!(json!(*s.trace.borrow()), row["trace"]);
}
fn token_race(row: &Value) {
    let clear = row["clearBetween"].as_bool().unwrap();
    let s = TokenState::new("race", true);
    let mut first = std::pin::pin!(s.run());
    assert!(poll(first.as_mut()).is_pending());
    if clear {
        *s.token.borrow_mut() = None;
        s.expiry.set(0.0);
        s.refresh.set(None);
        s.trace.borrow_mut().push(json!(["external-clear"]));
    }
    let mut second = std::pin::pin!(s.run());
    assert!(poll(second.as_mut()).is_pending());
    assert_eq!(
        s.generated.get(),
        if clear { 2 } else { 1 },
        "token_race_native_deduplication"
    );
    assert_eq!(
        json!({"generated":(0..s.generated.get()).collect::<Vec<_>>(),"refresh":s.refresh.get()}),
        row["before"]
    );
    s.held[0].ready.set(true);
    let Poll::Ready(a) = poll(first.as_mut()) else {
        panic!("token_race_first_ready")
    };
    let b = poll(second.as_mut());
    assert_eq!(
        json!({"token":s.token.borrow().as_deref().map(Vec::as_slice),"expiry":bits(s.expiry.get()),"refresh":s.refresh.get()}),
        row["afterFirst"],
        "token_race_native_first_state"
    );
    s.check_result(&a, &row["outcomes"][0]);
    s.held[1].ready.set(true);
    let b = match b {
        Poll::Ready(r) => r,
        Poll::Pending => match poll(second.as_mut()) {
            Poll::Ready(r) => r,
            Poll::Pending => panic!("token_race_second_ready"),
        },
    };
    s.check_result(&b, &row["outcomes"][1]);
    assert_eq!(
        json!(s.token.borrow().as_deref().map(Vec::as_slice)),
        row["token"]
    );
    assert_eq!(bits(s.expiry.get()), row["expiry"]);
    assert_eq!(json!(s.refresh.get()), row["refresh"]);
    assert_eq!(json!(*s.trace.borrow()), row["trace"]);
}
fn open(row: &Value) {
    let disposed = row["disposed"].as_bool().unwrap();
    let failure = row["failure"].as_str().unwrap();
    let trace = RefCell::new(vec![]);
    let authenticated = Cell::new(true);
    let id = Cell::new(Some(99.0));
    let error: Error = Rc::from("primary");
    let touch = |name: &str| {
        trace.borrow_mut().push(name.to_owned());
        if failure == name {
            Err(error.clone())
        } else {
            Ok(())
        }
    };
    let ready = Rc::new(Cell::new(false));
    let held = Held {
        ready: ready.clone(),
        result: if failure == "auth-async" {
            Err(error.clone())
        } else {
            Ok(())
        },
    };
    let mut f = std::pin::pin!(api::private_on_open(
        || {
            touch("disposed")?;
            Ok(disposed)
        },
        |v| {
            assert!(!v, "private_open_native_authenticated");
            touch("authenticated")?;
            authenticated.set(v);
            Ok(())
        },
        |v| {
            assert!(v.is_none(), "private_open_native_auth_id");
            touch("auth-id")?;
            id.set(v);
            Ok(())
        },
        |text, severity| {
            assert_eq!(text, "Paradex private WebSocket connected");
            assert_eq!(severity, "info");
            touch("log")
        },
        || touch("ping"),
        || {
            touch("auth-sync")?;
            Ok(held)
        }
    ));
    let first = poll(f.as_mut());
    assert_eq!(
        json!({"trace":*trace.borrow(),"settled":first.is_ready(),"authenticated":authenticated.get(),"authId":id.get().map(bits)}),
        row["before"],
        "private_open_native_await_boundary"
    );
    ready.set(true);
    let result = match first {
        Poll::Ready(r) => r,
        Poll::Pending => match poll(f.as_mut()) {
            Poll::Ready(r) => r,
            Poll::Pending => panic!("open_ready"),
        },
    };
    outcome(
        &result,
        &row["outcome"],
        std::slice::from_ref(&error),
        "private_open",
    );
    assert_eq!(authenticated.get(), row["authenticated"]);
    assert_eq!(json!(id.get().map(bits)), row["authId"]);
    assert_eq!(json!(*trace.borrow()), row["trace"]);
}
fn authenticate(row: &Value) {
    let init = row["init"].as_str().unwrap();
    let manager = row["manager"].as_str().unwrap();
    let failure = row["failure"].as_str().unwrap();
    let initial = number(&row["id"]);
    let trace = RefCell::new(vec![]);
    let requests = RefCell::new(vec![]);
    let current = Cell::new(initial);
    let auth_id = Cell::new(None);
    let errors: [Error; 3] = [Rc::from("primary"), Rc::from("format"), Rc::from("logger")];
    let touch = |name: &str| {
        trace.borrow_mut().push(name.to_owned());
        if failure == name {
            Err(errors[0].clone())
        } else {
            Ok(())
        }
    };
    let init_ready = Rc::new(Cell::new(false));
    let token_ready = Rc::new(Cell::new(false));
    let held_init = Held {
        ready: init_ready.clone(),
        result: if init == "error" {
            Err(errors[0].clone())
        } else {
            Ok(())
        },
    };
    let token = Rc::new(surrogate("token-"));
    let held_token = Held {
        ready: token_ready.clone(),
        result: if manager == "error" {
            Err(errors[0].clone())
        } else {
            Ok(token.clone())
        },
    };
    let mut f = std::pin::pin!(api::authenticate(
        || {
            touch("init0")?;
            Ok(init != "none")
        },
        || {
            touch("init1")?;
            Ok(held_init)
        },
        || {
            touch("manager0")?;
            Ok(manager != "none")
        },
        || {
            touch("manager1")?;
            touch("token")?;
            Ok(held_token)
        },
        || {
            touch("id-read")?;
            Ok(current.get())
        },
        |v| {
            assert_eq!(bits(v), bits(initial + 1.0), "authenticate_native_counter");
            touch("id-write")?;
            current.set(v);
            Ok(())
        },
        |v| {
            assert_eq!(bits(v), bits(initial), "authenticate_native_auth_id");
            touch("auth-id")?;
            auth_id.set(Some(v));
            Ok(())
        },
        |request| {
            assert_eq!(request.jsonrpc, "2.0", "authenticate_native_rpc");
            assert_eq!(request.method, "auth", "authenticate_native_method");
            assert!(
                Rc::ptr_eq(&request.bearer, &token),
                "authenticate_native_bearer_identity"
            );
            assert_eq!(request.bearer.as_slice(), surrogate("token-"));
            assert_eq!(
                bits(request.id),
                bits(initial),
                "authenticate_native_request_id"
            );
            requests.borrow_mut().push(json!({"jsonrpc":request.jsonrpc,"method":request.method,"bearer":*request.bearer,"id":bits(request.id)}));
            touch("serialize")?;
            touch("send")
        },
        |e| {
            assert!(
                Rc::ptr_eq(e, &errors[0]),
                "authenticate_native_caught_identity"
            );
            touch("error-log-read")?;
            trace.borrow_mut().push("error-string".to_owned());
            if failure == "error-string" {
                Err(errors[1].clone())
            } else {
                Ok(surrogate("Failed to authenticate WebSocket: cause-"))
            }
        },
        |message, severity| {
            assert_eq!(
                message,
                surrogate("Failed to authenticate WebSocket: cause-"),
                "authenticate_native_error_message"
            );
            assert_eq!(severity, "error");
            trace.borrow_mut().push("error-log".to_owned());
            if failure == "error-log" {
                Err(errors[2].clone())
            } else {
                Ok(())
            }
        }
    ));
    let first = poll(f.as_mut());
    assert_eq!(
        json!({"trace":*trace.borrow(),"settled":first.is_ready()}),
        row["before"],
        "authenticate_native_init_boundary"
    );
    init_ready.set(true);
    let middle = match first {
        Poll::Ready(r) => Poll::Ready(r),
        Poll::Pending => poll(f.as_mut()),
    };
    assert_eq!(
        json!({"trace":*trace.borrow(),"settled":middle.is_ready()}),
        row["middle"],
        "authenticate_native_token_boundary"
    );
    token_ready.set(true);
    let result = match middle {
        Poll::Ready(r) => r,
        Poll::Pending => match poll(f.as_mut()) {
            Poll::Ready(r) => r,
            Poll::Pending => panic!("authenticate_ready"),
        },
    };
    outcome(&result, &row["outcome"], &errors, "authenticate");
    assert_eq!(
        bits(current.get()),
        row["current"],
        "authenticate_native_final_counter"
    );
    assert_eq!(json!(auth_id.get().map(bits)), row["authId"]);
    assert_eq!(json!(*requests.borrow()), row["requests"]);
    assert_eq!(json!(*trace.borrow()), row["trace"]);
}
#[test]
fn original_private_auth_bodies() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private_auth_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "private_auth_source_observer: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let floors = [
        ("constructors", 38),
        ("tokens", 14),
        ("races", 2),
        ("opens", 16),
        ("auths", 131),
    ];
    for (name, count) in floors {
        assert_eq!(
            data[name].as_array().unwrap().len(),
            count,
            "private_auth_independent_floor_{name}"
        );
    }
    for row in data["constructors"].as_array().unwrap() {
        constructor(row);
    }
    for row in data["tokens"].as_array().unwrap() {
        token_case(row);
    }
    for row in data["races"].as_array().unwrap() {
        token_race(row);
    }
    for row in data["opens"].as_array().unwrap() {
        open(row);
    }
    for row in data["auths"].as_array().unwrap() {
        authenticate(row);
    }
}
