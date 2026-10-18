//! Live factory witness for the shared response boundary; this does not implement the venue factory.
use safe_cex_transport::{
    RestAdapter, RestRequest, RestResponse, TransportError, TransportFuture,
    how::{HowRestAdapter, big_int_as_string_json_parse},
    how_config::CreateAxiosHttpOverWebsocketAdapterOptions,
    socket::{
        SocketCapabilities, SocketFrame, SocketRuntime, TunnelDialOptions, TunnelHost,
        TunnelSocket, TunnelTask,
    },
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::{Future, poll_fn},
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};

struct Socket {
    frames: Rc<RefCell<VecDeque<SocketFrame>>>,
    raw: String,
}
impl TunnelSocket for Socket {
    fn send(&self, frame: SocketFrame) -> Result<(), TransportError> {
        let SocketFrame::Text(text) = frame else {
            panic!("fixture text frame");
        };
        let frame: Value = serde_json::from_str(&text).unwrap();
        if frame["type"] == "rpc.http" {
            self.frames.borrow_mut().push_back(SocketFrame::Text(
                json!({
                    "type":"rpc.http", "nonce":frame["nonce"],
                    "data":{"config":{"url":"/fixture"},"status":200,"status_text":"OK","headers":{"x-fixture":"yes"},"data_raw":self.raw}
                })
                .to_string(),
            ));
        }
        Ok(())
    }
    fn receive(&self) -> TransportFuture<'static, SocketFrame> {
        let frames = self.frames.clone();
        Box::pin(poll_fn(move |_| {
            frames
                .borrow_mut()
                .pop_front()
                .map_or(Poll::Pending, |f| Poll::Ready(Ok(f)))
        }))
    }
    fn close(&self, _: Option<u16>, _: &str) -> Result<(), TransportError> {
        Ok(())
    }
}
struct Task(Rc<Cell<bool>>);
impl TunnelTask for Task {
    fn cancel(&self) {
        self.0.set(true);
    }
}
struct Scheduled {
    cancelled: Rc<Cell<bool>>,
    future: Pin<Box<dyn Future<Output = ()>>>,
}
struct Host {
    socket: Rc<Socket>,
    now: Rc<Cell<u64>>,
    tasks: RefCell<Vec<Scheduled>>,
}
impl Host {
    fn new(raw: &str) -> Rc<Self> {
        Rc::new(Self {
            socket: Rc::new(Socket {
                frames: Default::default(),
                raw: raw.into(),
            }),
            now: Rc::new(Cell::new(1000)),
            tasks: Default::default(),
        })
    }
    fn run<T>(&self, future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        for _ in 0..100 {
            if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
            let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
            for mut task in tasks {
                if !task.cancelled.get() && task.future.as_mut().poll(&mut cx).is_pending() {
                    self.tasks.borrow_mut().push(task);
                }
            }
            self.now.set(self.now.get() + 1);
        }
        panic!("production HOW fixture did not settle");
    }
}
impl TunnelHost for Host {
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
        let cancelled = Rc::new(Cell::new(false));
        self.tasks.borrow_mut().push(Scheduled {
            cancelled: cancelled.clone(),
            future,
        });
        Box::new(Task(cancelled))
    }
    fn capabilities(&self) -> SocketCapabilities {
        SocketCapabilities {
            runtime: SocketRuntime::Native,
            upgrade_headers: true,
            alpn: true,
        }
    }
    fn now_ms(&self) -> u64 {
        self.now.get()
    }
    fn sleep_until(&self, deadline: u64) -> TransportFuture<'static, ()> {
        let now = self.now.clone();
        Box::pin(poll_fn(move |_| {
            if now.get() >= deadline {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn connect(
        &self,
        _: String,
        _: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        for frame in [
            json!({"type":"serverTime","serverTime":1000}),
            json!({"type":"rpc.auth","status":"success"}),
        ] {
            self.socket
                .frames
                .borrow_mut()
                .push_back(SocketFrame::Text(frame.to_string()));
        }
        let socket = self.socket.clone() as Rc<dyn TunnelSocket>;
        Box::pin(async move { Ok(socket) })
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        Box::pin(async { Ok("fixture-token".into()) })
    }
    fn is_token_expired(&self, _: &str) -> bool {
        false
    }
    fn sync_server_time(&self, time: u64) {
        assert_eq!(time, 1000);
    }
}
fn oracle(vectors: &Value) -> Value {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-api.mjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("factory oracle Node");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(vectors.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn tag_numbers(value: &safe_cex_types::retained_json::RetainedJsonInput) -> Value {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    match value {
        Input::Float(number) => json!({"numberBits": format!("{:016x}", number.to_bits())}),
        Input::Lexeme(number) => {
            json!({"numberBits": format!("{:016x}", number.parse::<f64>().unwrap().to_bits())})
        }
        Input::Array(values) => Value::Array(values.iter().map(tag_numbers).collect()),
        Input::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (String::from_utf16(key).unwrap(), tag_numbers(value)))
                .collect(),
        ),
        Input::Text(units) => Value::String(String::from_utf16(units).unwrap()),
        Input::Bool(value) => Value::Bool(*value),
        Input::Null => Value::Null,
        Input::Undefined => panic!("JSON source fixture has no own undefined"),
    }
}

#[test]
#[ignore = "mandatory live source witness: yarn venue-oracle:binance"]
fn public_factory_response_provenance_finding() {
    let texts = [
        r#""123""#,
        r#""true""#,
        r#""null""#,
        r#""hello""#,
        r#""{\"x\":1}""#,
        r#"{"id":9007199254740993}"#,
        r#"[1,2]"#,
        r#"null"#,
        "-0",
        "1e400",
        "-1e-400",
    ];
    let mut vectors = vec![];
    for text in texts {
        for tunnel in [false, true] {
            vectors.push(json!({"options":{"tealstreetTunnel":tunnel},"productionTunnel":tunnel,"remote":true,"request":{"url":"/fixture","headers":{"X-MBX-APIKEY":"must-remove"},"timeout":1},"responseText":text}));
        }
    }
    let source = oracle(&json!(vectors));
    assert_eq!(source.as_array().unwrap().len(), 22);
    for (i, text) in texts.iter().enumerate() {
        let direct = &source[2 * i];
        let tunnel = &source[2 * i + 1];
        for result in [direct, tunnel] {
            assert_eq!(
                result["remoteCalls"], 0,
                "public factory never asks remote signer"
            );
            assert_eq!(result["stats"], json!({"perMinute":1,"total":1,"errors":0}));
            assert_eq!(
                result["diagnostics"],
                json!(["start", "response", "success"])
            );
        }
        assert_eq!(
            direct["trace"][0]["headers"],
            json!([["Content-Type", "application/json, charset=utf-8"]]),
            "public interceptor replaces caller/default headers"
        );
        assert_eq!(
            direct["trace"][0]["timeout"], 20000,
            "public interceptor overrides caller timeout"
        );
        assert_eq!(
            tunnel["transportCalls"],
            json!([{"exchange":"binance","variant":"bigInt"}])
        );
        if i < 5 {
            assert_ne!(
                direct["result"]["data"], tunnel["result"]["data"],
                "source tunnel applies parser to decoded string {text}"
            );
        } else {
            assert_eq!(
                direct["result"]["data"], tunnel["result"]["data"],
                "source object/array/null control {text}"
            );
        }
        let host = Host::new(text);
        let adapter = HowRestAdapter::with_options(
            host.clone(),
            CreateAxiosHttpOverWebsocketAdapterOptions {
                region: None,
                custom_parser: Some(Rc::new(|text| Ok(big_int_as_string_json_parse(text)))),
            },
        )
        .unwrap();
        let response = host
            .run(adapter.execute(RestRequest {
                method: "get".into(),
                url: "/fixture".into(),
                base_url: Some("https://fapi.binance.com".into()),
                timeout_ms: Some(20000),
                ..Default::default()
            }))
            .unwrap();
        // ts-bug: createPublicAPI parses the already-parsed HOW string again.
        // The model view must not itself claim that this second parser ran.
        let direct_response = RestResponse {
            status: 200,
            reason: None,
            headers: response.headers.clone(),
            body: safe_cex_transport::RestResponseBody::Encoded(text.as_bytes().to_vec()),
        };
        assert!(
            matches!(
                response.body,
                safe_cex_transport::RestResponseBody::Transformed(_)
            ),
            "HOW custom parser must retain transformed provenance"
        );
        for (response, expected, path) in [
            (direct_response, direct, "direct"),
            (response, tunnel, "HOW"),
        ] {
            let parsed = response
                .body
                .transform(|data| match data {
                    safe_cex_types::retained_json::RetainedJsonInput::Text(text) => {
                        safe_cex_transport::how::big_int_as_string_json_parse_utf16(&text)
                    }
                    data => data,
                })
                .into_data();
            use safe_cex_types::retained_json::RetainedJsonInput as Input;
            let data_type = match &parsed {
                Input::Null | Input::Array(_) | Input::Object(_) => "object",
                Input::Bool(_) => "boolean",
                Input::Float(_) | Input::Lexeme(_) => "number",
                Input::Text(_) => "string",
                Input::Undefined => "undefined",
            };
            assert_eq!(
                data_type, expected["result"]["dataType"],
                "response current-data parser typeof {path} {text}"
            );
            assert_eq!(
                tag_numbers(&parsed),
                expected["result"]["data"],
                "response current-data parser bits and value {path} {text}"
            );
        }
    }
    assert_eq!(
        source[0]["result"]["dataType"], "string",
        "direct source typeof before serialization"
    );
    assert_eq!(
        source[1]["result"]["dataType"], "number",
        "HOW source typeof before serialization"
    );
    assert_eq!(source[0]["result"]["data"], json!("123"));
    assert_eq!(
        source[1]["result"]["data"],
        json!({"numberBits":"405ec00000000000"}),
        "tunnel string becomes numeric 123"
    );
}

#[test]
#[ignore = "mandatory live source control: yarn venue-oracle:binance"]
fn public_factory_missing_runtime_import_fails_loudly() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-api.mjs"
        ))
        .arg("--missing-retry-import")
        .output()
        .expect("Node import control");
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("MODULE_NOT_FOUND") && error.contains("axios-retry-deliberately-absent"),
        "missing actual runtime import must fail loudly: {error}"
    );
}
