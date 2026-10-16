use super::*;
use safe_cex_venue_polymarket::{api_factory::*, classes::*};
pub(super) type SyncBody = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
fn obj(fields: Vec<(&str, V)>) -> V {
    object(
        None,
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )
}
fn state(h: &Host, key: &str) -> V {
    h.services
        .borrow()
        .iter()
        .rev()
        .find(|(k, _)| k == key)
        .map_or(V::U, |(_, v)| v.clone())
}
fn put(h: &Host, key: &str, v: V) {
    h.services.borrow_mut().push((key.into(), v));
}
fn plain(j: &J) -> V {
    match j {
        J::Null => V::Null,
        J::Bool(b) => V::Bool(*b),
        J::Number(n) => V::Number(n.as_f64().unwrap()),
        J::String(s) => text(s),
        J::Array(a) => list(None, a.iter().map(|v| Some(plain(v))).collect()),
        J::Object(o) => object(None, o.iter().map(|(k, v)| (k.clone(), plain(v))).collect()),
    }
}
fn stringify(v: &V) -> Option<String> {
    match v {
        V::U | V::Function(..) => None,
        V::Null => Some("null".into()),
        V::Bool(b) => Some(b.to_string()),
        V::Number(n) => Some(if n.is_finite() {
            safe_cex_num::js_number_string(*n)
        } else {
            "null".into()
        }),
        V::Text(t) => Some(serde_json::to_string(&String::from_utf16_lossy(t)).unwrap()),
        V::Array(_, a) => Some(format!(
            "[{}]",
            a.borrow()
                .iter()
                .map(|v| v.as_ref().and_then(stringify).unwrap_or("null".into()))
                .collect::<Vec<_>>()
                .join(",")
        )),
        V::Object(_, o) => Some(format!(
            "{{{}}}",
            o.borrow()
                .iter()
                .filter_map(|(k, v)| stringify(v)
                    .map(|v| format!("{}:{v}", serde_json::to_string(k).unwrap())))
                .collect::<Vec<_>>()
                .join(",")
        )),
        V::Bytes(_) => unreachable!(),
    }
}
impl FactoryHost for Host {
    fn table(&self, name: &'static str) -> Result<V, E> {
        use safe_cex_venue_polymarket::constants::*;
        Ok(match name {
            "POLYMARKET_CONFIG" => plain(&serde_json::to_value(POLYMARKET_CONFIG).unwrap()),
            "RATE_LIMITS" => plain(&serde_json::to_value(RATE_LIMITS).unwrap()),
            "ENDPOINTS" => object(
                None,
                ENDPOINTS
                    .iter()
                    .map(|(k, v)| ((*k).into(), text(v)))
                    .collect(),
            ),
            _ => panic!("native constant table"),
        })
    }
    fn function(&self, body: SyncBody) -> V {
        let id = self.sync_bodies.borrow().len();
        self.sync_bodies.borrow_mut().push(body);
        V::Function(Some(id as u32), "factory-sync".into(), Rc::new(()))
    }
    fn apply(&self, function: V, args: Vec<V>) -> Result<V, E> {
        let V::Function(id, name, _) = function else {
            return Err(self.new_error("non callable"));
        };
        match name.as_str() {
            "factory-sync" => {
                let f = self.sync_bodies.borrow()[id.unwrap() as usize].clone();
                f(args)
            }
            "binding" => {
                let f = self.callbacks.borrow()[id.unwrap() as usize].clone();
                f()
            }
            _ => Err(self.new_error("unbound native dependency")),
        }
    }
    fn is_string_value(&self, v: &V) -> bool {
        matches!(v, V::Text(_))
    }
    fn starts_with(&self, v: &V, prefix: V) -> Result<bool, E> {
        let V::Text(t) = v else {
            return Err(self.new_error("startsWith non string"));
        };
        Ok(t.starts_with(&self.string(&prefix)?))
    }
    fn qs_comma(&self, p: V) -> Result<V, E> {
        self.event(
            "qs.stringify",
            vec![p, obj(vec![("arrayFormat", text("comma"))])],
        );
        Ok(text("bound-comma-query"))
    }
    fn json_stringify(&self, v: V) -> Result<V, E> {
        Ok(stringify(&v).map_or(V::U, |s| text(&s)))
    }
    fn sleep(&self, delay: f64) -> Result<Rc<Op>, E> {
        self.delays.borrow_mut().push(delay);
        self.clock.set(self.clock.get() + delay);
        Ok(Rc::new(Op(Ok(V::U))))
    }
    fn promise_reject(&self, error: V) -> Result<V, E> {
        put(self, "rejected", error);
        Ok(obj(vec![("promise", V::Bool(true))]))
    }
    fn inherit(&self, prototype: V) -> Result<V, E> {
        let value = obj(vec![]);
        self.instance_prototypes
            .borrow_mut()
            .push((value.clone(), prototype));
        Ok(value)
    }
}
impl ClassHost for Host {
    fn define_class(
        &self,
        name: &'static str,
        base: Option<String>,
        members: Vec<(&'static str, V)>,
    ) -> Result<String, E> {
        if name == "PolymarketAPI" {
            assert!(base.is_none(), "native API ordinary Object heritage");
            assert_eq!(members.len(), 29, "native independent API prototype floor");
        } else {
            assert_eq!(
                base.as_deref(),
                Some("BaseWebSocket"),
                "native shared socket heritage"
            );
            assert_eq!(
                members.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
                if name == "PolymarketPrivateWebSocket" {
                    vec!["mapOrder"]
                } else {
                    vec![]
                },
                "native socket prototype members"
            );
        }
        self.class_count.set(self.class_count.get() + 1);
        let key = format!("{name}#{}", self.class_count.get());
        let mut fields = vec![(
            "constructor".into(),
            V::Function(None, format!("class-constructor:{key}"), Rc::new(())),
        )];
        fields.extend(members.into_iter().map(|(n, v)| (n.into(), v)));
        self.class_prototypes
            .borrow_mut()
            .push((key.clone(), object(None, fields)));
        Ok(key)
    }
    fn class_method(
        &self,
        class: &'static str,
        name: &'static str,
        receiver: Option<V>,
    ) -> Result<V, E> {
        let _ = receiver;
        Ok(V::Function(
            None,
            format!("class:{class}:{name}"),
            Rc::new(()),
        ))
    }
    fn throttle(&self, body: V, wait: f64, leading: bool, trailing: bool) -> Result<V, E> {
        put(
            self,
            "throttle",
            list(
                None,
                vec![
                    Some(V::Number(wait)),
                    Some(V::Bool(leading)),
                    Some(V::Bool(trailing)),
                ],
            ),
        );
        let h = self.fork(self.receiver.clone());
        Ok(FactoryHost::function(
            self,
            Rc::new(move |args| h.apply(body.clone(), args)),
        ))
    }
    fn for_each_set(&self, set: V, callback: Rc<dyn Fn(V) -> Result<(), E>>) -> Result<(), E> {
        let values = raw_field(&set, "values");
        let mut i = 0;
        while i < self.length(&values)? {
            if let Some(value) = self.at(&values, i)? {
                callback(value)?;
            }
            i += 1;
        }
        Ok(())
    }
    fn strict_zero(&self, v: &V) -> bool {
        matches!(v,V::Number(n)if*n==0.)
    }
}
pub(super) fn construct(h: &Host, name: &str, args: &[V]) -> Option<Result<V, E>> {
    if name == "Error" {
        return Some(Ok(obj(vec![("message", args[0].clone())])));
    }
    if name == "Set" {
        return Some(Ok(obj(vec![
            ("size", V::Number(0.)),
            ("values", list(None, vec![])),
        ])));
    }
    if name.starts_with("PolymarketAPI#")
        || name.starts_with("PolymarketPublicWebSocket#")
        || name.starts_with("PolymarketPrivateWebSocket#")
    {
        let value = if name.starts_with("PolymarketAPI#") {
            obj(vec![])
        } else {
            obj(vec![("parent", args[0].clone()), ("store", obj(vec![]))])
        };
        let prototype = h
            .class_prototypes
            .borrow()
            .iter()
            .find(|(n, _)| n == name)
            .unwrap()
            .1
            .clone();
        h.instance_prototypes
            .borrow_mut()
            .push((value.clone(), prototype));
        return Some(Ok(value));
    }
    None
}
pub(super) fn sync(h: &Host, c: &Call, args: &[V]) -> Option<Result<V, E>> {
    if let Some(function) = &c.function {
        return Some(h.apply(function.clone(), args.to_vec()));
    }
    let name = format!("{}.{}", c.namespace, c.method);
    if let Some(receiver) = &c.receiver {
        if c.method == "use" {
            let group = raw_field(receiver, "group");
            if h.is_text(&group, "request") {
                put(h, "request", args[0].clone());
            } else {
                put(h, "success", args[0].clone());
                put(h, "failure", args[1].clone());
            }
            return Some(Ok(V::U));
        }
        if c.method == "set" && same(receiver, &state(h, "headers")) {
            h.event("headers.set", args.to_vec());
            return Some(Ok(V::U));
        }
    }
    match name.as_str() {
        "Object.defineProperty" => {
            let descriptor = &args[2];
            let key = String::from_utf16_lossy(&h.string(&args[1]).unwrap());
            field_set(&args[0], &key, raw_field(descriptor, "value"));
            put(
                h,
                "message-descriptor",
                list(
                    None,
                    vec![
                        Some(raw_field(descriptor, "writable")),
                        Some(raw_field(descriptor, "enumerable")),
                        Some(V::Bool(false)),
                    ],
                ),
            );
            Some(Ok(args[0].clone()))
        }
        "imports.safeCexAxiosCreate" => {
            let xhr = obj(vec![(
                "interceptors",
                obj(vec![
                    ("request", obj(vec![("group", text("request"))])),
                    ("response", obj(vec![("group", text("response"))])),
                ]),
            )]);
            put(h, "client", xhr.clone());
            put(h, "client-config", args[0].clone());
            Some(Ok(xhr))
        }
        "imports.retry" => {
            put(h, "retry", args[1].clone());
            Some(Ok(V::U))
        }
        "imports.readsAndPosted" => Some(Ok(obj(vec![(
            "posted",
            list(None, args.iter().cloned().map(Some).collect()),
        )]))),
        "imports.networkErrorOnRead" => Some(Ok(obj(vec![(
            "policy",
            args.first().cloned().unwrap_or(V::U),
        )]))),
        "imports.createPolymarketAPI" => Some(create_polymarket_api(
            h.fork(h.receiver.clone()),
            args[0].clone(),
            args[1].clone(),
            args[2].clone(),
            args[3].clone(),
        )),
        "imports.buildL2Headers" => {
            h.event(&name, args.to_vec());
            Some(Ok(obj(vec![
                ("POLY_ADDRESS", args[1].clone()),
                ("POLY_SIGNATURE", text("fixture-signature")),
                ("POLY_TIMESTAMP", text("1000")),
                ("POLY_API_KEY", raw_field(&args[0], "apiKey")),
                ("POLY_PASSPHRASE", raw_field(&args[0], "passphrase")),
            ])))
        }
        "store.updateTicker" => {
            let patches = state(h, "patches");
            h.push_spread(
                &patches,
                &list(
                    None,
                    vec![Some(list(None, args.iter().cloned().map(Some).collect()))],
                ),
            )
            .unwrap();
            Some(Ok(V::U))
        }
        _ => None,
    }
}
pub(super) fn invoke(h: &Host, c: &Call, args: &[V]) -> Option<Result<Rc<Op>, E>> {
    if let Some(V::Function(_, name, _)) = &c.function {
        if name == "class:PolymarketAPI:getSpread" {
            use safe_cex_venue_polymarket::rest_forwarding::{MethodResponse, get_spread};
            let mut pending = Box::pin(std::future::ready(Ok::<_, E>(MethodResponse {
                data: h.value("payload"),
            })));
            let token = h.string(&args[0]).unwrap();
            let result = get_spread(
                |request| {
                    let token = request.params.unwrap().into_iter().next().unwrap().1;
                    h.event(
                        "client.get",
                        vec![
                            V::Text(request.url),
                            obj(vec![("params", obj(vec![("token_id", V::Text(token))]))]),
                        ],
                    );
                    pending.as_mut()
                },
                &token,
            );
            let mut result = Box::pin(result);
            let Poll::Ready(value) = result
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            else {
                panic!("native bound client response")
            };
            return Some(Ok(Rc::new(Op(value))));
        }
    }
    None
}
fn oracle() -> &'static J {
    static DATA: OnceLock<J> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/completion_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "completion source {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: J = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["cases"].as_array().unwrap().len(),
            64,
            "independent completion source floor"
        );
        value
    })
}
fn make(case: &J) -> Rc<Host> {
    let h = Host::new(case);
    put(&h, "patches", list(None, vec![]));
    h
}
fn callbacks(h: &Rc<Host>) -> (V, V) {
    let a = h.clone();
    let b = h.clone();
    (
        h.callback(Rc::new(move || Ok(a.value("credentials")))),
        h.callback(Rc::new(move || Ok(b.value("address")))),
    )
}
fn parent(_h: &Host) -> V {
    obj(vec![
        ("tokenToSymbol", obj(vec![("17", text("event-Yes"))])),
        ("log", V::Function(None, "binding".into(), Rc::new(()))),
    ])
}
fn execute_request(h: &Rc<Host>, request: V) -> Result<V, E> {
    let V::Function(Some(id), name, _) = state(h, "request") else {
        panic!("native registered request interceptor")
    };
    assert_eq!(name, "async");
    let f = h.async_bodies.borrow()[id as usize].clone();
    finish(h, f(vec![request]))
}
fn keys(v: &V) -> Vec<String> {
    let V::Object(_, fields) = v else {
        panic!("native own slots")
    };
    fields.borrow().iter().map(|(k, _)| k.clone()).collect()
}
fn run_family(name: &str) {
    for i in 0..8 {
        let case = oracle()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == name && v["i"] == i)
            .unwrap();
        let h = make(case);
        let (credentials, address) = callbacks(&h);
        let mut returned = V::U;
        if name == "factory" {
            let xhr = create_polymarket_api(
                h.clone(),
                h.value("options"),
                parent(&h),
                credentials,
                address,
            )
            .unwrap();
            assert!(
                same(&xhr, &state(&h, "client")),
                "native original factory client alias"
            );
            let config = state(&h, "client-config");
            txt(
                &raw_field(&config, "baseURL"),
                "https://clob.polymarket.com",
                "factory base URL",
            );
            num(
                &raw_field(&config, "timeout"),
                if i == 0 {
                    0.
                } else if i == 1 {
                    30000.
                } else {
                    47000.
                },
                "factory receive window",
            );
            let retries = state(&h, "retry");
            num(&raw_field(&retries, "retries"), 3., "factory retry count");
            let posted = raw_field(&raw_field(&retries, "retryCondition"), "policy");
            let paths = raw_field(&posted, "posted");
            txt(
                &h.at(&paths, 0).unwrap().unwrap(),
                "/books",
                "retry posted book read",
            );
            txt(
                &h.at(&paths, 1).unwrap().unwrap(),
                "/midpoints",
                "retry posted midpoint read",
            );
            let params = obj(vec![
                (
                    "token_ids",
                    list(None, vec![Some(text("a")), Some(text("b"))]),
                ),
                ("empty", V::Null),
            ]);
            let serializer = raw_field(&raw_field(&config, "paramsSerializer"), "serialize");
            let serialized = h.apply(serializer, vec![params.clone()]).unwrap();
            txt(&serialized, "bound-comma-query", "bound serializer result");
            assert!(
                same(&h.events.borrow()[0].1[0], &params),
                "native original serializer parameter alias"
            );
            let headers = obj(vec![(
                "set",
                V::Function(None, "binding".into(), Rc::new(())),
            )]);
            put(&h, "headers", headers.clone());
            let request = h.value("request");
            field_set(&request, "headers", headers);
            let result = execute_request(&h, request.clone());
            if i == 2 || i == 3 {
                txt(
                    &result.as_ref().unwrap_err().message,
                    if i == 2 {
                        "Polymarket credentials not initialized"
                    } else {
                        "Polymarket wallet address not initialized"
                    },
                    "native auth prerequisite error",
                );
                assert!(
                    !h.events.borrow().iter().any(|(n, _)| n == "headers.set"),
                    "native missing auth does not set headers"
                );
                assert_eq!(
                    project(&result.unwrap_err().message),
                    case["result"]["error"],
                    "source auth error after native checks"
                );
            } else {
                assert!(
                    same(result.as_ref().unwrap(), &request),
                    "native original intercepted request alias"
                );
                if i == 1 {
                    let _ = execute_request(&h, request.clone()).unwrap();
                    assert_eq!(
                        h.delays.borrow().len(),
                        1,
                        "native shared client rate limiter"
                    );
                    assert_eq!(
                        h.delays.borrow()[0].to_bits(),
                        200_f64.to_bits(),
                        "native private rate interval"
                    );
                }
                let events = h.events.borrow();
                let signed = events
                    .iter()
                    .filter(|(n, _)| n == "imports.buildL2Headers")
                    .collect::<Vec<_>>();
                let count = if i == 0 {
                    0
                } else if i == 1 {
                    2
                } else {
                    1
                };
                assert_eq!(signed.len(), count, "native private authentication count");
                if count > 0 {
                    txt(&signed[0].1[1], "fixture-address", "native signing address");
                    txt(
                        &signed[0].1[2],
                        if i == 4 { "GET" } else { "POST" },
                        "native uppercase request method",
                    );
                    let body = &signed[0].1[4];
                    if i == 4 {
                        assert!(
                            matches!(body, V::U),
                            "native missing signed body stays undefined"
                        );
                    } else if i == 5 {
                        txt(body, "raw-body", "native raw string signed without quotes");
                    } else if i == 6 {
                        txt(body, "null", "native null body is signed JSON");
                    } else {
                        txt(
                            body,
                            "{\"price\":0.44,\"size\":12.346}",
                            "native structured signed body order",
                        );
                    }
                }
                drop(events);
                if i == 7 {
                    let response = obj(vec![
                        (
                            "data",
                            obj(vec![
                                ("error", text("source-error")),
                                ("message", text("source-message")),
                            ]),
                        ),
                        ("config", obj(vec![("url", text("/order"))])),
                    ]);
                    h.apply(state(&h, "success"), vec![response.clone()])
                        .unwrap();
                    let error = state(&h, "rejected");
                    txt(
                        &raw_field(&error, "message"),
                        "Polymarket Error: source-message",
                        "native success-error message",
                    );
                    assert!(
                        same(&raw_field(&error, "response"), &response),
                        "native original error response alias"
                    );
                    assert!(
                        same(
                            &raw_field(&error, "config"),
                            &raw_field(&response, "config")
                        ),
                        "native original error config alias"
                    );
                    assert!(
                        matches!(raw_field(&error, "isAxiosError"), V::Bool(true)),
                        "native Axios error marker"
                    );
                    h.event(
                        "response.success-error",
                        vec![
                            raw_field(&error, "message"),
                            raw_field(&error, "isAxiosError"),
                        ],
                    );
                    let original = obj(vec![
                        (
                            "response",
                            obj(vec![(
                                "data",
                                obj(vec![("message", text("enhanced-message"))]),
                            )]),
                        ),
                        ("code", text("E_FIXTURE")),
                    ]);
                    h.apply(state(&h, "failure"), vec![original.clone()])
                        .unwrap();
                    let enhanced = state(&h, "rejected");
                    assert!(
                        h.instance_prototypes
                            .borrow()
                            .iter()
                            .any(|(v, p)| same(v, &enhanced) && same(p, &original)),
                        "native enhanced error inherits original"
                    );
                    assert_eq!(
                        keys(&enhanced),
                        ["message"],
                        "native enhanced error owns only message"
                    );
                    assert!(
                        matches!(
                            h.at(&state(&h, "message-descriptor"), 0).unwrap().unwrap(),
                            V::Bool(false)
                        ),
                        "native enhanced message readonly"
                    );
                    h.event(
                        "response.enhanced",
                        vec![
                            raw_field(&enhanced, "message"),
                            h.field(&enhanced, "code").unwrap(),
                        ],
                    );
                }
                assert_eq!(
                    h.events
                        .borrow()
                        .iter()
                        .map(|(n, a)| json!([n, a.iter().map(project).collect::<Vec<_>>()]))
                        .collect::<Vec<_>>(),
                    case["events"].as_array().unwrap().clone(),
                    "source factory boundary calls {i}"
                );
                assert_eq!(
                    project(result.as_ref().unwrap()),
                    case["result"]["value"],
                    "source original intercepted native config {i}"
                );
            }
            assert_eq!(
                *h.logs.borrow(),
                case["logs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| (v[0].as_str().unwrap().into(), v[1].as_str().unwrap().into()))
                    .collect::<Vec<_>>(),
                "source factory logging {i}"
            );
        } else {
            let classes = PolymarketClasses::new(h.clone()).unwrap();
            let actual_prototype;
            if name == "apiClass" || name == "apiCredentials" {
                let parent = parent(&h);
                let api = PolymarketAPI::new(
                    &classes,
                    h.value("options"),
                    parent.clone(),
                    credentials.clone(),
                    address.clone(),
                )
                .unwrap();
                let r = api.receiver();
                assert!(
                    same(&api.get_credentials.value().unwrap(), &credentials),
                    "native API callback property reference retains function alias"
                );
                assert_eq!(
                    keys(&r),
                    ["getCredentials", "xhr", "gammaXhr", "dataXhr", "exchange"],
                    "native API declaration slots"
                );
                assert!(
                    same(&raw_field(&r, "getCredentials"), &credentials),
                    "native original API credential function alias"
                );
                assert!(
                    same(&raw_field(&r, "exchange"), &parent),
                    "native original API exchange alias"
                );
                let expected = h.value("credentials");
                let got = api.get_credentials.call().unwrap();
                if matches!(expected, V::Null) {
                    assert!(matches!(got, V::Null), "native API null credentials");
                } else {
                    assert!(
                        same(&got, &expected),
                        "native API callback native credential alias"
                    );
                }
                let later = obj(vec![("apiKey", text("late"))]);
                field_set(&h.values, "credentials", later.clone());
                assert!(
                    same(&api.get_credentials.call().unwrap(), &later),
                    "native API callback reads live credentials"
                );
                let operation = api.call("getSpread", vec![text("17")]).unwrap();
                let Poll::Ready(value) = operation.poll(&mut Context::from_waker(Waker::noop()))
                else {
                    panic!("native composed API read")
                };
                returned = value.unwrap();
                assert!(
                    same(&returned, &h.value("payload")),
                    "native composed API result alias"
                );
                let second =
                    PolymarketAPI::new(&classes, h.value("options"), parent, credentials, address)
                        .unwrap();
                actual_prototype = assert_shared(&h, &r, &second.receiver());
                assert_eq!(
                    keys(&r),
                    case["shape"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                    "source API shape"
                );
            } else if name == "privateClass" || name == "privateCredentials" {
                let socket =
                    PolymarketPrivateWebSocket::new(&classes, parent(&h), credentials.clone())
                        .unwrap();
                assert!(
                    same(&socket.get_credentials.value().unwrap(), &credentials),
                    "native private callback property reference retains function alias"
                );
                let r = socket.receiver();
                assert_eq!(keys(&r).len(), 18, "native private socket own slot floor");
                assert!(
                    same(&raw_field(&r, "getCredentials"), &credentials),
                    "native private original credential function alias"
                );
                let expected = h.value("credentials");
                let got = socket.get_credentials.call().unwrap();
                if matches!(expected, V::Null) {
                    assert!(matches!(got, V::Null), "native private null credentials");
                } else {
                    assert!(
                        same(&got, &expected),
                        "native private callback native credential alias"
                    );
                }
                let later = obj(vec![("apiKey", text("late"))]);
                field_set(&h.values, "credentials", later.clone());
                assert!(
                    same(&socket.get_credentials.call().unwrap(), &later),
                    "native private callback reads live credentials"
                );
                let second =
                    PolymarketPrivateWebSocket::new(&classes, parent(&h), credentials).unwrap();
                actual_prototype = assert_shared(&h, &r, &second.receiver());
                assert_socket(&h, &r, false);
                assert_eq!(
                    keys(&r),
                    case["shape"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                    "source private socket shape"
                );
            } else {
                let socket = PolymarketPublicWebSocket::new(&classes, parent(&h)).unwrap();
                let r = socket.receiver();
                assert_eq!(keys(&r).len(), 22, "native public socket own slot floor");
                assert_socket(&h, &r, true);
                let second = PolymarketPublicWebSocket::new(&classes, parent(&h)).unwrap();
                actual_prototype = assert_shared(&h, &r, &second.receiver());
                assert!(
                    !same(
                        &raw_field(&r, "subscribedAssets"),
                        &raw_field(&second.receiver(), "subscribedAssets")
                    ),
                    "native per-instance subscription sets"
                );
                assert!(
                    !same(
                        &raw_field(&r, "orderbookCallbacks"),
                        &raw_field(&second.receiver(), "orderbookCallbacks")
                    ),
                    "native per-instance book callback maps"
                );
                let throttle = state(&h, "throttle");
                num(
                    &h.at(&throttle, 0).unwrap().unwrap(),
                    500.,
                    "native throttle interval",
                );
                assert!(
                    matches!(h.at(&throttle, 1).unwrap().unwrap(), V::Bool(true)),
                    "native throttle leading edge"
                );
                assert!(
                    matches!(h.at(&throttle, 2).unwrap().unwrap(), V::Bool(true)),
                    "native throttle trailing edge"
                );
                if name == "price" {
                    socket.dispatch("price_change", h.value("price")).unwrap();
                    returned = state(&h, "patches");
                    let count = match i {
                        3 | 4 | 5 | 7 => 0,
                        6 => 2,
                        _ => 1,
                    };
                    len(&h, &returned, count, "native price patch floor");
                    if count > 0 {
                        let pair = h.at(&returned, 0).unwrap().unwrap();
                        let patch = h.at(&pair, 1).unwrap().unwrap();
                        txt(
                            &raw_field(&patch, "id"),
                            "17",
                            "native price token selector",
                        );
                        if i == 1 {
                            assert!(
                                !keys(&patch).contains(&"bid".into()),
                                "native nonfinite bid omitted"
                            );
                        }
                        if i == 2 {
                            assert!(
                                !keys(&patch).contains(&"ask".into()),
                                "native nonfinite ask omitted"
                            );
                        }
                        if i == 1 || i == 2 {
                            assert!(
                                !keys(&patch).contains(&"mark".into())
                                    && !keys(&patch).contains(&"index".into()),
                                "native nonfinite midpoint omitted"
                            );
                        }
                        if i != 1 {
                            num(&raw_field(&patch, "bid"), 0.2, "native best bid");
                        }
                        if i != 2 {
                            num(&raw_field(&patch, "ask"), 0.8, "native best ask");
                        }
                        if i == 0 {
                            num(
                                &raw_field(&patch, "mark"),
                                0.5,
                                "native probability midpoint",
                            );
                            num(&raw_field(&patch, "index"), 0.5, "native probability index");
                        }
                    }
                }
                if name == "book" {
                    let books = list(None, vec![]);
                    put(&h, "books", books.clone());
                    let first = {
                        let h = h.clone();
                        let books = books.clone();
                        FactoryHost::function(
                            h.clone().as_ref(),
                            Rc::new(move |args| {
                                h.push_spread(&books, &list(None, vec![Some(args[0].clone())]))?;
                                Ok(V::U)
                            }),
                        )
                    };
                    let second = {
                        let h = h.clone();
                        let books = books.clone();
                        FactoryHost::function(
                            h.clone().as_ref(),
                            Rc::new(move |args| {
                                h.push_spread(&books, &list(None, vec![Some(args[0].clone())]))?;
                                Ok(V::U)
                            }),
                        )
                    };
                    if i != 7 {
                        field_set(
                            &raw_field(&r, "orderbookCallbacks"),
                            "17",
                            obj(vec![
                                ("size", V::Number(if i == 6 { 1. } else { 2. })),
                                (
                                    "values",
                                    list(
                                        None,
                                        if i == 6 {
                                            vec![Some(first)]
                                        } else {
                                            vec![Some(first), Some(second)]
                                        },
                                    ),
                                ),
                            ]),
                        );
                    }
                    socket.dispatch("book", h.value("book")).unwrap();
                    returned = books;
                    let count = if i == 1 || i == 7 {
                        0
                    } else if i == 6 {
                        1
                    } else {
                        2
                    };
                    len(&h, &returned, count, "native book callback floor");
                    if count > 0 {
                        let book = h.at(&returned, 0).unwrap().unwrap();
                        if count == 2 {
                            assert!(
                                same(&book, &h.at(&returned, 1).unwrap().unwrap()),
                                "native shared order book callback alias"
                            );
                        }
                        if i == 0 {
                            let bids = raw_field(&book, "bids");
                            let asks = raw_field(&book, "asks");
                            num(
                                &raw_field(&h.at(&bids, 0).unwrap().unwrap(), "price"),
                                0.4,
                                "native book first bid price",
                            );
                            num(
                                &raw_field(&h.at(&bids, 0).unwrap().unwrap(), "amount"),
                                2.,
                                "native first bid amount",
                            );
                            num(
                                &raw_field(&h.at(&bids, 0).unwrap().unwrap(), "total"),
                                2.,
                                "native first bid accumulated amount",
                            );
                            num(
                                &raw_field(&h.at(&bids, 1).unwrap().unwrap(), "amount"),
                                3.,
                                "native second bid amount",
                            );
                            num(
                                &raw_field(&h.at(&bids, 1).unwrap().unwrap(), "total"),
                                5.,
                                "native book cumulative bid amount",
                            );
                            num(
                                &raw_field(&h.at(&asks, 0).unwrap().unwrap(), "price"),
                                0.6,
                                "native book ask price",
                            );
                            num(
                                &raw_field(&h.at(&asks, 0).unwrap().unwrap(), "total"),
                                7.,
                                "native independent ask amount",
                            );
                        }
                    }
                }
                assert_eq!(
                    keys(&r),
                    case["shape"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                    "source public socket shape"
                );
            }
            assert_eq!(
                keys(&actual_prototype),
                case["prototypeMembers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>(),
                "source evaluated prototype members"
            );
            assert_eq!(
                project(&returned),
                case["result"]["value"],
                "source completion result {name}/{i}"
            );
        }
    }
}
fn assert_socket(h: &Host, r: &V, public: bool) {
    assert!(
        matches!(raw_field(r, "recoversAfterOffline"), V::Bool(false)),
        "native socket offline recovery policy"
    );
    txt(
        &raw_field(r, "connectUrl"),
        if public {
            "wss://ws-subscriptions-clob.polymarket.com/ws/market"
        } else {
            "wss://ws-subscriptions-clob.polymarket.com/ws/user"
        },
        "native socket channel URL",
    );
    let handlers = raw_field(r, "messageHandlers");
    assert_eq!(
        keys(&handlers),
        if public {
            vec!["ping", "pong", "price_change", "book", "last_trade_price"]
        } else {
            vec!["ping", "pong", "order", "trade"]
        },
        "native message handler topic floor"
    );
    for topic in keys(&handlers) {
        len(
            h,
            &raw_field(&handlers, &topic),
            1,
            "native one callback per topic",
        );
    }
}
fn assert_shared(h: &Host, a: &V, b: &V) -> V {
    let graph = h.instance_prototypes.borrow();
    let x = &graph.iter().find(|(r, _)| same(r, a)).unwrap().1;
    let y = &graph.iter().find(|(r, _)| same(r, b)).unwrap().1;
    assert!(same(x, y), "native shared evaluated child prototype");
    assert_eq!(
        h.class_count.get(),
        3,
        "native three child class definitions"
    );
    x.clone()
}
#[test]
fn factory_native() {
    run_family("factory")
}
#[test]
fn api_class_native() {
    run_family("apiClass")
}
#[test]
fn api_credentials_native() {
    run_family("apiCredentials")
}
#[test]
fn public_class_native() {
    run_family("publicClass")
}
#[test]
fn private_class_native() {
    run_family("privateClass")
}
#[test]
fn private_credentials_native() {
    run_family("privateCredentials")
}
#[test]
fn price_native() {
    run_family("price")
}
#[test]
fn book_native() {
    run_family("book")
}

#[test]
fn dropped_request_retains_authentication() {
    let case = oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "factory" && c["i"] == 1)
        .unwrap();
    let h = make(case);
    let (credentials, address) = callbacks(&h);
    let _ = create_polymarket_api(
        h.clone(),
        h.value("options"),
        parent(&h),
        credentials,
        address,
    )
    .unwrap();
    let headers = obj(vec![(
        "set",
        V::Function(None, "binding".into(), Rc::new(())),
    )]);
    put(&h, "headers", headers.clone());
    let request = h.value("request");
    field_set(&request, "headers", headers);
    let _ = execute_request(&h, request.clone()).unwrap();
    let V::Function(Some(id), _, _) = state(&h, "request") else {
        panic!("native request callback")
    };
    let body = h.async_bodies.borrow()[id as usize].clone();
    let observer = body(vec![request]);
    drop(observer);
    h.drain();
    assert_eq!(
        h.events
            .borrow()
            .iter()
            .filter(|(n, _)| n == "imports.buildL2Headers")
            .count(),
        2,
        "native dropped rate-limited request retains authentication"
    );
    assert_eq!(
        h.delays.borrow()[0].to_bits(),
        200_f64.to_bits(),
        "native dropped request rate interval"
    );
}
