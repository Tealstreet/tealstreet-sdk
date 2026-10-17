use indexmap::IndexMap;
use safe_cex_venue_coinbase::{
    api_factory::*,
    classes::*,
    market_position_reads::ReadMarket,
    order_edit::*,
    order_sync::{SyncedOrder, SyncedOrderRoot, SyncedOrders},
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::CoinbaseStarbasePromise as Promise,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    process::Command,
    rc::Rc,
};
#[path = "support/microtasks.rs"]
mod microtasks;
use microtasks::{Microtasks, completed, is_pending};
const FIXTURE: &str = include_str!("fixtures/completion.json");
fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}
fn source() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "d5ba57ed057073e602e12393b8876138f1cdceaab7a522aaa95ad48af60fcaea",
        "completion corpus content pin"
    );
    let f = fixture();
    for (k, n) in [("edits", 44), ("apis", 16), ("classes", 10)] {
        assert_eq!(
            f[k].as_array().unwrap().len(),
            n,
            "independent input floor {k}"
        );
    }
    let o = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/completion.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                // CI supplies an isolated compiler; this fallback only
                // serves a local run from the repo root.
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").to_owned()
            }),
        )
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "live source oracle: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let s: Value = serde_json::from_slice(&o.stdout).unwrap();
    for (k, n) in [("edits", 44), ("apis", 16), ("classes", 10)] {
        assert_eq!(
            s[k].as_array().unwrap().len(),
            n,
            "independent source floor {k}"
        );
    }
    s
}
fn failure(v: Value) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(v)))
}
fn num(a: f64, e: &Value, n: &str) {
    let b = u64::from_str_radix(e["$bits"].as_str().expect("source numeric bits"), 16).unwrap();
    if f64::from_bits(b).is_nan() {
        assert!(a.is_nan(), "{n}: native NaN")
    } else {
        assert_eq!(a.to_bits(), b, "{n}: native numeric bits")
    }
}
fn value(a: Option<&Value>, e: &Value, n: &str) {
    if e.get("$undefined") == Some(&json!(true)) {
        assert!(a.is_none(), "{n}: native undefined");
        return;
    }
    let a = a.expect("native value");
    match a {
        Value::Number(v) => {
            if e.get("$bits").is_some() {
                num(v.as_f64().unwrap(), e, n)
            } else {
                assert_eq!(a, e, "{n}: native primitive metadata scalar")
            }
        }
        Value::Array(v) => {
            let es = e.as_array().unwrap();
            assert_eq!(v.len(), es.len(), "{n}: source array count");
            for (a, e) in v.iter().zip(es) {
                value(Some(a), e, n)
            }
        }
        Value::Object(v) => {
            let es = e.as_object().unwrap();
            assert_eq!(v.len(), es.len(), "{n}: source own field count");
            for (k, e) in es {
                value(v.get(k), e, n)
            }
        }
        _ => assert_eq!(a, e, "{n}: native scalar"),
    }
}
fn frame(a: Option<Frame>, e: &Value, n: &str) {
    if e.get("$undefined") == Some(&json!(true)) {
        assert!(a.is_none(), "{n}: native undefined");
        return;
    }
    match a.expect("native frame") {
        Frame::Null => assert!(e.is_null(), "{n}: null"),
        Frame::Bool(v) => assert_eq!(Some(v), e.as_bool(), "{n}: bool"),
        Frame::Number(v) => num(v, e, n),
        Frame::String(v) => assert_eq!(
            v,
            e.as_str().unwrap().encode_utf16().collect::<Vec<_>>(),
            "{n}: UTF16"
        ),
        Frame::Array(v) => {
            let v = v.borrow();
            let es = e.as_array().unwrap();
            assert_eq!(v.len(), es.len(), "{n}: array count");
            for (a, e) in v.iter().zip(es) {
                frame(Some(a.clone()), e, n)
            }
        }
        Frame::Object(v) => {
            let v = v.borrow();
            let es = e.as_object().unwrap();
            assert_eq!(v.len(), es.len(), "{n}: own fields");
            for (k, e) in es {
                frame(v.get(&k.encode_utf16().collect::<Vec<_>>()).cloned(), e, n)
            }
        }
    }
}
fn same(a: &Frame, b: &Frame) -> bool {
    match (a, b) {
        (Frame::Object(a), Frame::Object(b)) => Rc::ptr_eq(a, b),
        (Frame::Array(a), Frame::Array(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn set(v: &Frame, k: &str, x: Frame) {
    let Frame::Object(v) = v else {
        panic!("object")
    };
    v.borrow_mut().insert(k.encode_utf16().collect(), x);
}
fn ready<T: Clone + 'static>(_q: &Microtasks, x: T) -> Promise<T> {
    Promise::resolved(x)
}
struct Trace {
    expected: RefCell<VecDeque<Value>>,
    label: String,
}
impl Trace {
    fn new(v: &Value, label: &str) -> Self {
        Self {
            expected: RefCell::new(v.as_array().unwrap().iter().cloned().collect()),
            label: label.into(),
        }
    }
    fn next(&self, kind: &str) -> Value {
        let e = self
            .expected
            .borrow_mut()
            .pop_front()
            .expect("independent source trace floor exceeded");
        assert_eq!(e["kind"], kind, "{}: native callback order", self.label);
        e
    }
    fn done(&self) {
        assert!(
            self.expected.borrow().is_empty(),
            "{}: source trace unconsumed",
            self.label
        )
    }
}
fn assert_error(a: &PortfolioFailure, e: &Value, n: &str) {
    value(a.value.as_ref(), e, n)
}
struct EditHost {
    q: Microtasks,
    input: Value,
    trace: Trace,
    provided: SyncedOrderRoot,
    canonical: Option<SyncedOrderRoot>,
    market: Option<ReadMarket>,
    post_error: Option<PortfolioFailure>,
    cancel_error: Option<PortfolioFailure>,
    place_error: Option<PortfolioFailure>,
    emit_error: Option<PortfolioFailure>,
    payloads: RefCell<Vec<safe_cex_venue_coinbase::types::EditOrderRequest>>,
}
impl OrderEditHost for EditHost {
    fn selected_portfolio_type(&self) -> Result<Option<String>, PortfolioFailure> {
        Ok(self.input["selected"].as_str().map(str::to_owned))
    }
    fn orders(&self) -> Result<SyncedOrders, PortfolioFailure> {
        Ok(Rc::new(RefCell::new(
            self.canonical.iter().cloned().collect(),
        )))
    }
    fn market_by_symbol(&self, symbol: String) -> Result<Option<ReadMarket>, PortfolioFailure> {
        if self.input["mutateLookup"] == true {
            self.canonical.as_ref().unwrap().borrow_mut().fields.id = "lookup-id".into();
        }
        Ok(self
            .market
            .as_ref()
            .filter(|m| m.borrow().symbol == symbol)
            .cloned())
    }
    fn cancel_orders(&self, o: SyncedOrders) -> EditAsync<()> {
        let e = self.trace.next("cancel");
        assert_eq!(o.borrow().len(), 1, "independent cancelled count");
        assert!(
            Rc::ptr_eq(&o.borrow()[0], &self.provided),
            "cancel receives original provided order root"
        );
        assert_eq!(e["same"], true);
        Ok(ready(
            &self.q,
            self.cancel_error.clone().map_or(Ok(()), Err),
        ))
    }
    fn place_order(&self, o: EditReplacement) -> EditAsync<Vec<String>> {
        let e = self.trace.next("place");
        let v = &e["order"];
        assert_eq!(o.symbol, v["symbol"].as_str().unwrap());
        assert_eq!(o.r#type, safe_cex_types::OrderType::Limit);
        assert_eq!(o.side, safe_cex_types::OrderSide::Buy);
        match o.price {
            Some(a) => num(a, &v["price"], "native replacement price"),
            None => assert_eq!(v["price"], json!({"$undefined":true})),
        };
        match o.amount {
            Some(a) => num(a, &v["amount"], "native replacement size"),
            None => assert_eq!(v["amount"], json!({"$undefined":true})),
        };
        assert_eq!(o.reduce_only.into_value(), v["reduceOnly"].as_bool());
        Ok(ready(
            &self.q,
            self.place_error
                .clone()
                .map_or(Ok(vec!["replacement".into()]), Err),
        ))
    }
    fn post_edit(
        &self,
        endpoint: &'static str,
        p: safe_cex_venue_coinbase::types::EditOrderRequest,
    ) -> EditAsync<Value> {
        let e = self.trace.next("post");
        assert_eq!(endpoint, e["endpoint"].as_str().unwrap());
        let expected = &e["payload"];
        assert_eq!(
            p.order_id,
            expected["order_id"].as_str().unwrap(),
            "native edit uses canonical id"
        );
        assert_eq!(
            p.price,
            expected["price"].as_str().unwrap(),
            "native edit price respects price precision"
        );
        assert_eq!(
            p.size,
            expected["size"].as_str().unwrap(),
            "native edit size respects amount precision"
        );
        self.payloads.borrow_mut().push(p);
        Ok(ready(
            &self.q,
            self.post_error
                .clone()
                .map_or_else(|| Ok(self.input["response"].clone()), Err),
        ))
    }
    fn schedule_reconciliation(&self, reason: &'static str) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("reconcile");
        assert_eq!(
            reason,
            e["reason"].as_str().unwrap(),
            "native timer reason and order"
        );
        if self.input["mutateReturn"] == true {
            self.canonical.as_ref().unwrap().borrow_mut().fields.id = "after-reconcile".into()
        }
        Ok(())
    }
    fn schedule_write_check(&self) -> Result<(), PortfolioFailure> {
        self.trace.next("writeCheck");
        Ok(())
    }
    fn emit_and_log_error(&self, message: Option<Value>) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("emit");
        value(message.as_ref(), &e["message"], "native edit diagnostic");
        if self.input["mutateEmit"] == true {
            self.canonical.as_ref().unwrap().borrow_mut().fields.id = "emit-id".into();
            self.market.as_ref().unwrap().borrow_mut().contract_size = Some(99.0);
        }
        self.emit_error.clone().map_or(Ok(()), Err)
    }
}
fn order(v: &Value) -> SyncedOrderRoot {
    let mut fields: safe_cex_types::Order = serde_json::from_value(v.clone()).unwrap();
    fields.info = (None).into();
    Rc::new(RefCell::new(SyncedOrder {
        fields,
        info: Rc::new(RefCell::new(v["info"].clone())),
    }))
}
#[test]
fn edit_native_payload_precision_preflight_and_error_identity() {
    let f = fixture();
    let s = source();
    for (input, e) in f["edits"]
        .as_array()
        .unwrap()
        .iter()
        .zip(s["edits"].as_array().unwrap())
    {
        let q = Microtasks::default();
        let provided = order(&input["order"]);
        let host = EditHost {
            q: q.clone(),
            input: input.clone(),
            trace: Trace::new(&e["trace"], input["label"].as_str().unwrap()),
            provided: provided.clone(),
            canonical: (!input["canonical"].is_null()).then(|| order(&input["canonical"])),
            market: (!input["market"].is_null()).then(|| {
                Rc::new(RefCell::new(
                    serde_json::from_value(input["market"].clone()).unwrap(),
                ))
            }),
            post_error: input.get("postError").cloned().map(failure),
            cancel_error: input.get("cancelError").cloned().map(failure),
            place_error: input.get("placeError").cloned().map(failure),
            emit_error: input.get("emitError").cloned().map(failure),
            payloads: RefCell::new(vec![]),
        };
        let r = Rc::new(OrderEditReceiver {
            host,
            spawn: q.spawner(),
        });
        let u = &input["update"];
        let update = Rc::new(RefCell::new(EditUpdate {
            has_amount: u.get("amount").is_some(),
            has_price: u.get("price").is_some(),
            amount: u["amount"].as_f64(),
            price: u["price"].as_f64(),
        }));
        let p = update_order(r.clone(), provided, update);
        q.drain();
        match completed(p) {
            Ok(v) => {
                assert!(e["error"].is_null(), "source edit succeeded");
                assert_eq!(
                    v,
                    e["value"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                    "native edit returns retained canonical id after timers"
                )
            }
            Err(err) => {
                assert_error(&err, &e["error"], "native edit failure");
                if let Some(original) = r
                    .host
                    .emit_error
                    .as_ref()
                    .or(r.host.post_error.as_ref())
                    .or(r.host.cancel_error.as_ref())
                    .or(r.host.place_error.as_ref())
                {
                    assert!(
                        Rc::ptr_eq(&err, original),
                        "native edit original failure carrier"
                    )
                }
            }
        }
        if input["mutateEmit"] == true {
            assert_eq!(
                r.host.canonical.as_ref().unwrap().borrow().fields.id,
                e["emittedCanonicalId"].as_str().unwrap(),
                "native preflight callback can mutate retained order root"
            );
            num(
                r.host
                    .market
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .contract_size
                    .unwrap(),
                &e["emittedMarketContract"],
                "native preflight callback can mutate retained market root",
            );
        }
        r.host.trace.done();
    }
}
struct ApiHost {
    q: Microtasks,
    input: Value,
    trace: Trace,
    options: Frame,
    exchange: Frame,
    original: ApiRequest,
    client: Frame,
    condition: Frame,
    interceptor: RefCell<Option<ApiInterceptor>>,
    serializer: RefCell<Option<ApiSerializer>>,
    sign_error: Option<PortfolioFailure>,
}
fn record(v: &Value) -> ApiRequest {
    Rc::new(RefCell::new(
        v.as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.encode_utf16().collect(), Some(Frame::from_json(v))))
            .collect(),
    ))
}
fn get(r: &ApiRequest, k: &str) -> Option<Frame> {
    r.borrow()
        .get(&k.encode_utf16().collect::<Vec<_>>())
        .cloned()
        .flatten()
}
fn assert_record(r: &ApiRequest, e: &Value) {
    let r = r.borrow();
    let expected = e.as_object().unwrap();
    assert_eq!(r.len(), expected.len(), "native API own field count");
    for (k, e) in expected {
        let key = k.encode_utf16().collect::<Vec<_>>();
        assert!(r.contains_key(&key), "native API own property {k}");
        frame(r[&key].clone(), e, &format!("native API result.{k}"));
    }
}
impl CoinbaseApiHost for ApiHost {
    type Client = Frame;
    fn safe_create(
        &self,
        c: ApiCreateConfig,
        o: Frame,
        x: Frame,
    ) -> Result<Frame, PortfolioFailure> {
        let e = self.trace.next("create");
        assert_eq!(c.base_url, e["baseURL"].as_str().unwrap());
        frame(Some(c.timeout), &e["timeout"], "native API nullish timeout");
        assert_eq!(c.headers.len(), 2, "independent initial header count");
        for (k, v) in c.headers {
            frame(v, &e["headers"][k], "native factory header");
        }
        assert!(
            same(&o, &self.options) && same(&x, &self.exchange),
            "native factory original options/exchange roots"
        );
        assert_eq!(e["sameOptions"], true);
        assert_eq!(e["sameExchange"], true);
        *self.serializer.borrow_mut() = Some(c.serialize);
        Ok(self.client.clone())
    }
    fn serialize_repeat(&self, p: Frame) -> Result<String, PortfolioFailure> {
        let e = self.trace.next("serialize");
        frame(Some(p), &e["params"], "native serializer params");
        assert_eq!(e["options"]["arrayFormat"], "repeat");
        Ok("repeat-wire".into())
    }
    fn network_error_on_read(&self) -> Result<Frame, PortfolioFailure> {
        self.trace.next("condition");
        Ok(self.condition.clone())
    }
    fn retry(&self, c: Frame, n: f64, p: Frame) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("retry");
        num(n, &e["retries"], "native retries");
        assert!(
            same(&c, &self.client) && same(&p, &self.condition),
            "native retry roots"
        );
        assert_eq!(e["sameClient"], true);
        assert_eq!(e["sameCondition"], true);
        Ok(())
    }
    fn install_interceptor(&self, c: Frame, i: ApiInterceptor) -> Result<(), PortfolioFailure> {
        self.trace.next("interceptor");
        assert!(same(&c, &self.client));
        *self.interceptor.borrow_mut() = Some(i);
        Ok(())
    }
    fn install_transport_adapter(
        &self,
        c: Frame,
        o: Frame,
        n: Option<Frame>,
    ) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("install");
        assert!(same(&c, &self.client) && same(&o, &self.options));
        frame(n, &e["name"], "native adapter exchange name");
        Ok(())
    }
    fn replace_url_params(
        &self,
        u: Option<Frame>,
        p: Option<Frame>,
    ) -> Result<ApiRequest, PortfolioFailure> {
        let e = self.trace.next("replace");
        frame(u.clone(), &e["url"], "native URL replace argument");
        frame(p.clone(), &e["params"], "native params replace argument");
        if let (Some(p), Some(original)) = (&p, get(&self.original, "params")) {
            assert!(same(p, &original), "replace retains original params root");
        }
        let u = if self.input["replacePublic"] == true {
            Some(Frame::from_json(&json!(
                "https://api.international.coinbase.com/api/v1/instruments/"
            )))
        } else {
            u.map(|u| match u {
                Frame::String(v) => {
                    let text = String::from_utf16_lossy(&v).replace(
                        "{id}",
                        self.input["config"]["params"]["id"]
                            .as_str()
                            .unwrap_or("undefined"),
                    );
                    Frame::String(text.encode_utf16().collect())
                }
                v => v,
            })
        };
        Ok(Rc::new(RefCell::new(IndexMap::from([
            ("url".encode_utf16().collect(), u),
            ("params".encode_utf16().collect(), p),
        ]))))
    }
    fn mark_skip_remote_signing(&self, c: ApiRequest) -> Result<(), PortfolioFailure> {
        self.trace.next("mark");
        c.borrow_mut()
            .insert("skip".encode_utf16().collect(), Some(Frame::Bool(true)));
        Ok(())
    }
    fn proxy_prefix(
        &self,
        exchange: &'static str,
        o: Frame,
        c: ApiRequest,
    ) -> Result<Vec<u16>, PortfolioFailure> {
        let t = self.trace.next("transport");
        assert!(same(&o, &self.options));
        assert_eq!(t["sameOptions"], true);
        let e = self.trace.next("proxy");
        assert_eq!(exchange, e["exchange"].as_str().unwrap());
        if self.input["mutateProxyNull"] == true {
            c.borrow_mut()
                .insert("url".encode_utf16().collect(), Some(Frame::Null));
        }
        Ok(self.input["proxy"]
            .as_str()
            .unwrap()
            .encode_utf16()
            .collect())
    }
    fn headers(&self, f: Frame, from: bool) -> Result<Frame, PortfolioFailure> {
        let e = self.trace.next("headers");
        assert_eq!(
            Some(from),
            e["from"].as_bool(),
            "native public AxiosHeaders.from branch"
        );
        frame(
            Some(f.clone()),
            &e["fields"],
            "native header replacement and tunnel selection",
        );
        Ok(f)
    }
    fn sign(
        &self,
        k: Option<Frame>,
        s: Option<Frame>,
        u: Vec<u16>,
    ) -> Result<Promise<Result<Frame, PortfolioFailure>>, PortfolioFailure> {
        let e = self.trace.next("sign");
        frame(k, &e["key"], "native signing key");
        frame(s, &e["secret"], "native signing secret");
        assert_eq!(
            u,
            e["uri"]
                .as_str()
                .unwrap()
                .encode_utf16()
                .collect::<Vec<_>>(),
            "native signing URI before proxy/public branches"
        );
        if self.input["mutateTunnel"] == true {
            set(&self.options, "tealstreetTunnel", Frame::Bool(true))
        }
        Ok(ready(
            &self.q,
            self.sign_error
                .clone()
                .map_or_else(|| Ok(Frame::from_json(&self.input["token"])), Err),
        ))
    }
}
#[test]
fn api_native_fields_aliases_branch_order_and_async_headers() {
    let f = fixture();
    let s = source();
    for (input, e) in f["apis"]
        .as_array()
        .unwrap()
        .iter()
        .zip(s["apis"].as_array().unwrap())
    {
        let q = Microtasks::default();
        let options = Frame::from_json(&input["options"]);
        let exchange = Frame::from_json(&json!({"name":"coinbase"}));
        let original = record(&input["config"]);
        let r = Rc::new(CoinbaseApiReceiver {
            host: ApiHost {
                q: q.clone(),
                input: input.clone(),
                trace: Trace::new(&e["trace"], input["label"].as_str().unwrap()),
                options: options.clone(),
                exchange: exchange.clone(),
                original: original.clone(),
                client: Frame::from_json(&json!({"client":true})),
                condition: Frame::from_json(&json!({"condition":true})),
                interceptor: RefCell::new(None),
                serializer: RefCell::new(None),
                sign_error: input.get("signError").cloned().map(failure),
            },
            options,
            exchange,
            spawn: q.spawner(),
        });
        let c = create_api(r.clone()).unwrap();
        assert!(same(&c, &r.host.client));
        let serialize = r.host.serializer.borrow().as_ref().unwrap().clone();
        let serialized =
            serialize(Frame::from_json(&json!({"status":["OPEN","PENDING"]}))).unwrap();
        let callback = r.host.interceptor.borrow().as_ref().unwrap().clone();
        let mut p = callback(original.clone());
        assert!(is_pending(&mut p));
        q.drain();
        match completed(p) {
            Ok(v) => {
                assert!(e["error"].is_null());
                assert_record(&v, &e["value"]);
                assert!(
                    !Rc::ptr_eq(&v, &original),
                    "interceptor produces fresh own-property root"
                );
                for (k, flag) in [
                    ("data", "sameData"),
                    ("params", "sameParams"),
                    ("headers", "sameHeaders"),
                ] {
                    if let (Some(a), Some(b)) = (get(&v, k), get(&original, k)) {
                        assert_eq!(
                            same(&a, &b),
                            e[flag].as_bool().unwrap(),
                            "native shallow alias {k}"
                        );
                    }
                }
            }
            Err(err) => {
                assert_error(&err, &e["error"], "native API failure");
                assert!(
                    Rc::ptr_eq(&err, r.host.sign_error.as_ref().unwrap()),
                    "native API signing failure identity"
                )
            }
        }
        r.host.trace.done();
        assert_eq!(
            serialized,
            e["serialized"].as_str().unwrap(),
            "serializer wire after every native assertion"
        );
    }
}
// Socket transport leaves are not invoked by the constructor. Their existence is
// required by the native class types; dispatch bodies have their own source gates.
struct SocketHost {
    parent: Frame,
    options: Frame,
    store: Frame,
    emitter: Frame,
}
use safe_cex_venue_coinbase::{
    private_stream_dispatch as private, public_stream_dispatch as public,
};
impl public::PublicStreamHost for SocketHost {
    fn is_disposed(&self) -> bool {
        unreachable!()
    }
    fn testnet(&self) -> bool {
        unreachable!()
    }
    fn reconnecting_enabled(&self) -> bool {
        unreachable!()
    }
    fn create_websocket(
        &self,
        _: &'static str,
    ) -> Result<public::PublicSocket, Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn add_listener(
        &self,
        _: public::PublicSocket,
        _: public::PublicSocketListener,
    ) -> Result<(), Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn reset_orderbook(&self) -> Result<(), Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn emit_ws_reconnected(&self) -> Result<(), Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn markets_symbols(&self) -> Result<Vec<String>, Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn send_subscription(
        &self,
        _: public::PublicSubscription,
    ) -> Result<(), Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn track_last_received(&self) -> Result<(), Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn json_parse(&self, _: &str) -> Option<Frame> {
        unreachable!()
    }
    fn emit_error(&self, _: Option<Frame>) -> Result<(), Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn console_error_text(&self, _: Vec<u16>) -> Result<(), Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn console_error_frame(&self, _: Frame) -> Result<(), Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn handle_pong_event(&self) -> Result<(), Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
    fn record_message_handler_stats(&self, _: &str) -> Result<(), Rc<public::PublicStreamFailure>> {
        unreachable!()
    }
}
impl private::PrivateStreamHost for SocketHost {
    fn is_disposed(&self) -> bool {
        unreachable!()
    }
    fn options(&self) -> private::PrivateOptions {
        unreachable!()
    }
    fn exchange_name(&self) -> String {
        unreachable!()
    }
    fn testnet(&self) -> bool {
        unreachable!()
    }
    fn resolve_private_route(
        &self,
        _: String,
        _: private::PrivateOptions,
    ) -> Result<private::PrivateStreamRoute, PortfolioFailure> {
        unreachable!()
    }
    fn create_websocket(&self, _: Option<Value>) -> Result<public::PublicSocket, PortfolioFailure> {
        unreachable!()
    }
    fn add_listener(
        &self,
        _: public::PublicSocket,
        _: public::PublicSocketListener,
    ) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn reset_order_snapshot_assembly(&self) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn close_socket(&self, _: public::PublicSocket) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn send_with_log(&self, _: private::PrivateSubscription) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn sign_local(
        &self,
        _: Option<Value>,
        _: Option<Value>,
    ) -> Promise<Result<Value, PortfolioFailure>> {
        unreachable!()
    }
    fn track_last_received(&self) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn emit_ws_diagnostic_receive(&self, _: &str) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn json_parse(&self, _: &str) -> Result<Frame, PortfolioFailure> {
        unreachable!()
    }
    fn mark_private_user_stream_auth_failed(
        &self,
        _: Option<Frame>,
    ) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn mark_private_user_stream_message_received(&self) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn emit_and_log_error(&self, _: Option<Frame>) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn handle_user_event_orders(&self, _: Frame) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn handle_pong_event(&self) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
}
struct ClassHost {
    input: Value,
    trace: Trace,
    options: Frame,
    store: Frame,
    identity: RefCell<Option<Frame>>,
    emitter: Frame,
    original_error: PortfolioFailure,
    mint: RefCell<Option<ClassMint>>,
    uuid_count: Cell<f64>,
    api_count: Cell<usize>,
    policy: RefCell<Option<Rc<ClassPolicyLeaves>>>,
    captured_finder: RefCell<Option<ClassFindMethod<ClassHost>>>,
    captured_supported: RefCell<Option<ClassSupportedMethod<ClassHost>>>,
}
impl ClassHost {
    fn step(&self, k: &str) -> Result<Value, PortfolioFailure> {
        let e = self.trace.next(k);
        if self.input["fail"].as_str() == Some(k) {
            Err(self.original_error.clone())
        } else {
            Ok(e)
        }
    }
    fn socket(&self, k: &str, parent: Frame) -> Result<SocketHost, PortfolioFailure> {
        let e = self.step(k)?;
        assert!(
            same(&parent, self.identity.borrow().as_ref().unwrap()),
            "native socket original parent identity"
        );
        assert!(same(&parent.get("options").unwrap(), &self.options));
        assert!(same(&parent.get("store").unwrap(), &self.store));
        assert_eq!(e["sameOptions"], true);
        assert_eq!(e["sameStore"], true);
        Ok(SocketHost {
            parent,
            options: self.options.clone(),
            store: self.store.clone(),
            emitter: self.emitter.clone(),
        })
    }
}
impl CoinbaseApiHost for ClassHost {
    type Client = Frame;
    fn safe_create(
        &self,
        c: ApiCreateConfig,
        o: Frame,
        x: Frame,
    ) -> Result<Frame, PortfolioFailure> {
        let n = self.api_count.get() + 1;
        self.api_count.set(n);
        let e = self.step(&format!("create{n}"))?;
        assert!(
            same(&o, &self.options) && same(&x, self.identity.borrow().as_ref().unwrap()),
            "native constructor API roots"
        );
        assert_eq!(e["sameOptions"], true);
        assert_eq!(c.base_url, "https://api.coinbase.com");
        assert_eq!(c.headers.len(), 2);
        Ok(Frame::from_json(&json!({"tag":format!("client{n}")})))
    }
    fn serialize_repeat(&self, _: Frame) -> Result<String, PortfolioFailure> {
        unreachable!()
    }
    fn network_error_on_read(&self) -> Result<Frame, PortfolioFailure> {
        Ok(Frame::Bool(true))
    }
    fn retry(&self, _: Frame, n: f64, _: Frame) -> Result<(), PortfolioFailure> {
        assert_eq!(n, 3.0, "native constructor actual factory retries");
        Ok(())
    }
    fn install_interceptor(&self, _: Frame, _: ApiInterceptor) -> Result<(), PortfolioFailure> {
        Ok(())
    }
    fn install_transport_adapter(
        &self,
        _: Frame,
        _: Frame,
        _: Option<Frame>,
    ) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn replace_url_params(
        &self,
        _: Option<Frame>,
        _: Option<Frame>,
    ) -> Result<ApiRequest, PortfolioFailure> {
        unreachable!()
    }
    fn mark_skip_remote_signing(&self, _: ApiRequest) -> Result<(), PortfolioFailure> {
        unreachable!()
    }
    fn proxy_prefix(
        &self,
        _: &'static str,
        _: Frame,
        _: ApiRequest,
    ) -> Result<Vec<u16>, PortfolioFailure> {
        unreachable!()
    }
    fn headers(&self, _: Frame, _: bool) -> Result<Frame, PortfolioFailure> {
        unreachable!()
    }
    fn sign(
        &self,
        _: Option<Frame>,
        _: Option<Frame>,
        _: Vec<u16>,
    ) -> Result<Promise<Result<Frame, PortfolioFailure>>, PortfolioFailure> {
        unreachable!()
    }
}
impl CoinbaseConstructionHost for ClassHost {
    type Public = SocketHost;
    type Private = SocketHost;
    fn construct_base(
        &self,
        r: Frame,
        name: String,
        o: Frame,
        s: Frame,
    ) -> Result<(), PortfolioFailure> {
        let e = self.step("base")?;
        assert_eq!(name, e["name"].as_str().unwrap());
        assert!(same(&o, &self.options) && same(&s, &self.store));
        set(&r, "name", Frame::from_json(&json!(name)));
        set(&r, "options", o);
        set(&r, "store", s);
        set(&r, "emitter", self.emitter.clone());
        *self.identity.borrow_mut() = Some(r);
        Ok(())
    }
    fn pattern_client_order_id_rule(
        &self,
        name: &'static str,
        pattern: &'static str,
        mint: ClassMint,
    ) -> Result<Frame, PortfolioFailure> {
        let e = self.step("rule")?;
        assert_eq!(name, e["name"].as_str().unwrap());
        assert_eq!(pattern, e["pattern"].as_str().unwrap());
        *self.mint.borrow_mut() = Some(mint);
        Ok(Frame::from_json(&json!({"name":name,"pattern":pattern})))
    }
    fn uuid(&self) -> Result<String, PortfolioFailure> {
        self.uuid_count.set(self.uuid_count.get() + 1.0);
        Ok("uuid".into())
    }
    fn create_secondary(
        &self,
        r: Rc<CoinbaseExchange<Self>>,
        o: Frame,
        f: ClassApiFactory,
        rate: ClassRateLimit,
    ) -> Result<Frame, PortfolioFailure> {
        let e = self.step("secondary")?;
        *self.captured_finder.borrow_mut() = Some(r.capture_finder());
        *self.captured_supported.borrow_mut() = Some(r.capture_supported());
        assert!(same(&o, &r.options.borrow()));
        assert_eq!(e["sameOptions"], true);
        assert!(
            r.selected_portfolio.borrow().is_none(),
            "native constructor selected portfolio initialized before secondary callback"
        );
        assert!(e["initialSelected"].is_null());
        assert_eq!(r.support_spot.get(), e["supportSpot"].as_bool().unwrap());
        assert_eq!(r.next_orders_poll_at.get(), 0.0);
        assert_eq!(r.last_private_user_stream_message_at.get(), 0.0);
        assert!(!r.has_warned_private_user_stream_degraded.get());
        assert!(
            r.order_write_reconcile_timeout.borrow().is_none()
                && r.private_user_stream_write_check_timeout.borrow().is_none()
        );
        assert!(r.client_order_id_rule.borrow().is_some());
        assert!(r.xhr.borrow().is_none() && r.unlimited_xhr.borrow().is_none());
        assert!(r.public_websocket.borrow().is_none() && r.private_websocket.borrow().is_none());
        rate(f(o, r.identity.clone())?)
    }
    fn rate_limit(&self, c: Frame, n: f64) -> Result<Frame, PortfolioFailure> {
        let e = self.step("rate")?;
        num(n, &e["maxRPS"], "native constructor rate limit three");
        Ok(c)
    }
    fn construct_public_base(&self, p: Frame) -> Result<SocketHost, PortfolioFailure> {
        self.socket("publicBase", p)
    }
    fn construct_private_base(&self, p: Frame) -> Result<SocketHost, PortfolioFailure> {
        self.socket("privateBase", p)
    }
}
fn assert_class(r: &CoinbaseExchange<ClassHost>, e: &Value) {
    assert!(r.selected_portfolio.borrow().is_none());
    assert!(e["selected"].is_null());
    num(
        r.next_orders_poll_at.get(),
        &e["next"],
        "native class polling zero",
    );
    num(
        r.last_private_user_stream_message_at.get(),
        &e["last"],
        "native class health zero",
    );
    assert_eq!(
        r.has_warned_private_user_stream_degraded.get(),
        e["warned"].as_bool().unwrap()
    );
    assert_eq!(
        r.support_spot.get(),
        e["support"].as_bool().unwrap(),
        "native spot support field"
    );
    assert!(
        r.order_write_reconcile_timeout.borrow().is_none()
            && r.private_user_stream_write_check_timeout.borrow().is_none()
    );
    assert_eq!(e["orderTimer"]["$undefined"], true);
    assert_eq!(e["writeTimer"]["$undefined"], true);
    for (a, key) in [
        (r.grafana_endpoints_every, "every"),
        (r.grafana_endpoints_periodic, "periodic"),
    ] {
        assert_eq!(
            a,
            e[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            "native class grafana alias values"
        );
    }
    assert!(std::ptr::eq(
        r.grafana_endpoints_every,
        safe_cex_venue_coinbase::tables::GRAFANA_ENDPOINTS_EVERY
    ));
    let p = r.public_websocket.borrow();
    let p = p.as_ref().unwrap();
    let v = r.private_websocket.borrow();
    let v = v.as_ref().unwrap();
    let topics = p.receiver.topics.borrow();
    assert_eq!(topics.len(), 1, "independent initial topic count");
    num(
        topics["heartbeats"],
        &e["publicTopics"]["heartbeats"],
        "native public heartbeat initial subscription count",
    );
    let handlers = p.receiver.message_handlers.borrow();
    let expected = [
        "candles",
        "market_trades",
        "l2_data",
        "heartbeats",
        "subscriptions",
    ];
    assert_eq!(
        handlers.keys().map(String::as_str).collect::<Vec<_>>(),
        expected,
        "native public initial handler order"
    );
    for key in expected {
        num(
            handlers[key].borrow().len() as f64,
            &e["publicHandlers"][key],
            "native public initial handler count",
        );
    }
    num(
        v.receiver.subscription_attempt.get(),
        &e["attempt"],
        "native private subscription initial attempt",
    );
    assert!(
        !v.snapshot.borrow().has_pending_snapshot(),
        "native private snapshot initially absent"
    );
    assert_eq!(e["snapshot"]["$undefined"], true);
    num(
        v.snapshot.borrow().pending_raw_order_count() as f64,
        &e["rawCount"],
        "native private snapshot initial raw count",
    );
    assert!(same(&p.parent, &r.identity) && same(&v.parent, &r.identity));
    assert_eq!(e["parents"], true);
    assert!(
        same(&p.receiver.host.parent, &r.identity) && same(&v.receiver.host.parent, &r.identity)
    );
    assert!(
        same(&p.receiver.host.store, &r.store)
            && same(&p.receiver.host.options, &r.options.borrow())
            && same(&v.receiver.host.emitter, &r.host.emitter)
    );
    assert_eq!(e["baseAliases"], true);
    assert!(
        !same(
            r.xhr.borrow().as_ref().unwrap(),
            r.unlimited_xhr.borrow().as_ref().unwrap()
        ),
        "native constructor distinct clients"
    );
    assert_eq!(e["distinctClients"], true);
    num(
        r.host.uuid_count.get(),
        &e["uuidBeforeMint"],
        "native class lazy UUID mint",
    );
}
#[test]
fn classes_native_initializers_constructor_order_and_shared_slots() {
    let f = fixture();
    let s = source();
    for (input, e) in f["classes"]
        .as_array()
        .unwrap()
        .iter()
        .zip(s["classes"].as_array().unwrap())
    {
        let q = Microtasks::default();
        q.enqueue(|| {});
        q.drain();
        let h = Rc::new(ClassHost {
            input: input.clone(),
            trace: Trace::new(&e["trace"], input["label"].as_str().unwrap()),
            options: Frame::from_json(&json!({"key":"key"})),
            store: Frame::from_json(&json!({"tag":"store"})),
            identity: RefCell::new(None),
            emitter: Frame::from_json(&json!({"tag":"emitter"})),
            original_error: failure(
                json!({"message":format!("failed {}",input["fail"].as_str().unwrap_or("undefined"))}),
            ),
            mint: RefCell::new(None),
            uuid_count: Cell::new(0.0),
            api_count: Cell::new(0),
            policy: RefCell::new(None),
            captured_finder: RefCell::new(None),
            captured_supported: RefCell::new(None),
        });
        let new = || {
            if input["spot"] == true {
                CoinbaseSpotExchange::new(
                    h.clone(),
                    "coinbase".into(),
                    h.options.clone(),
                    h.store.clone(),
                    q.spawner(),
                )
                .map(|s| s.exchange)
            } else {
                CoinbaseExchange::new(
                    h.clone(),
                    "coinbase".into(),
                    h.options.clone(),
                    h.store.clone(),
                    q.spawner(),
                )
            }
        };
        match new() {
            Err(err) => {
                assert_error(&err, &e["error"], "native constructor abrupt failure");
                assert!(
                    Rc::ptr_eq(&err, &h.original_error),
                    "native constructor original failure identity"
                );
            }
            Ok(r) => {
                assert!(e["error"].is_null());
                assert_class(&r, &e["value"]);
                if input["label"] == "lazy mint" {
                    assert_eq!(
                        h.mint.borrow().as_ref().unwrap()().unwrap(),
                        e["value"]["mint"].as_str().unwrap(),
                        "native class lazy prefixed mint"
                    );
                    assert_eq!(h.uuid_count.get(), 1.0);
                }
                if input["label"] == "independent instances" {
                    let second = new().unwrap();
                    let p = r.public_websocket.borrow();
                    let p = p.as_ref().unwrap();
                    let other = second.public_websocket.borrow();
                    let other = other.as_ref().unwrap();
                    assert!(
                        !Rc::ptr_eq(&p.receiver, &other.receiver),
                        "native class independent registries"
                    );
                    p.receiver
                        .topics
                        .borrow_mut()
                        .insert("isolated".into(), 9.0);
                    assert!(!other.receiver.topics.borrow().contains_key("isolated"));
                    p.receiver.message_handlers.borrow()["candles"]
                        .borrow_mut()
                        .clear();
                    assert_eq!(
                        other.receiver.message_handlers.borrow()["candles"]
                            .borrow()
                            .len(),
                        1
                    );
                    assert_eq!(e["value"]["independentHandlers"], true);
                    assert_eq!(e["value"]["independentTopics"], true);
                }
                let portfolios = r.portfolio_receiver((), false);
                let markets = r.market_receiver(());
                let lifecycle = r.lifecycle_receiver(());
                assert!(
                    Rc::ptr_eq(&r.selected_portfolio, &portfolios.selected)
                        && Rc::ptr_eq(&portfolios.selected, &markets.selected)
                        && Rc::ptr_eq(&markets.selected, &lifecycle.selected),
                    "native exchange projections share selected portfolio slot"
                );
                assert!(
                    Rc::ptr_eq(
                        &r.order_write_reconcile_timeout,
                        &lifecycle.order_write_reconcile_timeout
                    ) && Rc::ptr_eq(
                        &r.private_user_stream_write_check_timeout,
                        &lifecycle.private_user_stream_write_check_timeout
                    ),
                    "native exchange projections share timer slots"
                );
            }
        }
        h.trace.done();
    }
}

// Constructor fixture leaves are not called. The class composition tests run real leaves.
mod constructor_socket_capabilities {
    use super::*;
    use safe_cex_venue_coinbase::{
        public_data::PublicData, public_market_listeners::*,
        public_market_reads::PublicOHLCVOptionsRoot,
    };
    impl PublicMarketListenerHost for SocketHost {
        fn is_connected(&self) -> bool {
            unreachable!("constructor-only socket leaf")
        }
        fn public_data_only_option(&self) -> bool {
            unreachable!("constructor-only socket leaf")
        }
        fn timeframe_seconds(&self, _interval: &str) -> Option<f64> {
            unreachable!("constructor-only socket leaf")
        }
        fn daily_candle(&self, _symbol: &str) -> Option<PublicData> {
            unreachable!("constructor-only socket leaf")
        }
        fn set_daily_candle(
            &self,
            _symbol: String,
            _candle: PublicData,
        ) -> Result<(), ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn utc_end_of_day_ms(&self) -> Result<f64, ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn fetch_ohlcv_detached(
            &self,
            _opts: PublicOHLCVOptionsRoot,
        ) -> Result<(), ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn dayjs_unix(&self, _value: Option<Frame>) -> Result<f64, ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn add(&self, _left: Option<Frame>, _right: f64) -> Result<f64, ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn warn(&self, _message: &str, _value: Option<Frame>) -> Result<(), ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn set_timeout(
            &self,
            _callback: PublicCleanup,
            _delay_ms: f64,
        ) -> Result<(), ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn cleanup_message_handler_stats(&self, _topic: &str) -> Result<(), ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn nanoid(&self) -> Result<String, ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn update_order_book(
            &self,
            _book: PublicListenerBook,
            _side: &str,
            _level: Rc<RefCell<PublicBookLevel>>,
        ) -> Result<(), ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn sort_order_book(&self, _book: PublicListenerBook) -> Result<(), ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn calc_order_book_total(&self, _book: PublicListenerBook) -> Result<(), ListenerFailure> {
            unreachable!("constructor-only socket leaf")
        }
    }
    impl CoinbasePrivateClassHost for SocketHost {
        fn private_order_markets(
            &self,
        ) -> Result<safe_cex_venue_coinbase::order_sync::PrivateOrderMarkets, PortfolioFailure>
        {
            unreachable!("constructor-only socket leaf")
        }
        fn resolve_private_order_market(
            &self,
            _product: String,
            _markets: safe_cex_venue_coinbase::order_sync::PrivateOrderMarkets,
        ) -> Result<
            Option<safe_cex_venue_coinbase::market_position_reads::ReadMarket>,
            PortfolioFailure,
        > {
            unreachable!("constructor-only socket leaf")
        }
        fn private_order_console_error(
            &self,
            _message: &str,
            _value: &str,
        ) -> Result<(), PortfolioFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn private_order_now_ms(&self) -> Result<f64, PortfolioFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn private_orders_loaded(&self) -> Result<Frame, PortfolioFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn publish_private_order_snapshot(
            &self,
            _orders: safe_cex_venue_coinbase::order_sync::SyncedOrders<Frame>,
            _loaded: Frame,
        ) -> Result<(), PortfolioFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn add_private_orders(
            &self,
            _orders: safe_cex_venue_coinbase::order_sync::SyncedOrders<Frame>,
        ) -> Result<(), PortfolioFailure> {
            unreachable!("constructor-only socket leaf")
        }
        fn emit_private_order_fill(
            &self,
            _fill: safe_cex_venue_coinbase::order_sync::PrivateOrderFill<Frame>,
        ) -> Result<(), PortfolioFailure> {
            unreachable!("constructor-only socket leaf")
        }
    }
}

struct ClassPolicyLeaves {
    portfolios: safe_cex_venue_coinbase::portfolios_balances::Portfolios,
    calls: RefCell<Vec<(&'static str, Option<String>)>>,
    errors: RefCell<Vec<Vec<Option<Value>>>>,
    pending_breakdown: RefCell<
        Option<
            Promise<
                Result<
                    safe_cex_venue_coinbase::portfolios_balances::PortfolioReply,
                    PortfolioFailure,
                >,
            >,
        >,
    >,
}
impl ClassPolicyLeaves {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            portfolios: Rc::new(RefCell::new(
                [("intx", "INTX"), ("ordinary", "DEFAULT")]
                    .into_iter()
                    .map(|(uuid, kind)| {
                        Rc::new(RefCell::new(
                            serde_json::from_value(
                                json!({"uuid":uuid,"name":uuid,"type":kind,"deleted":false}),
                            )
                            .unwrap(),
                        ))
                    })
                    .collect(),
            )),
            calls: RefCell::new(vec![]),
            errors: RefCell::new(vec![]),
            pending_breakdown: RefCell::new(None),
        })
    }
}
impl ClassHost {
    fn policy(&self) -> Rc<ClassPolicyLeaves> {
        self.policy
            .borrow()
            .clone()
            .expect("constructor must not invoke imported portfolio leaves")
    }
}
impl safe_cex_venue_coinbase::portfolios_balances::PortfolioHost for ClassHost {
    fn get(
        &self,
        endpoint: &'static str,
        uuid: Option<String>,
    ) -> Promise<
        Result<safe_cex_venue_coinbase::portfolios_balances::PortfolioReply, PortfolioFailure>,
    > {
        let leaves = self.policy();
        leaves.calls.borrow_mut().push((endpoint, uuid));
        if endpoint == "/api/v3/brokerage/portfolios/{portfolio_uuid}" {
            if let Some(pending) = leaves.pending_breakdown.borrow_mut().take() {
                return pending;
            }
        }
        match endpoint {
            "/api/v3/brokerage/portfolios" => Promise::resolved(Ok(
                safe_cex_venue_coinbase::portfolios_balances::PortfolioReply {
                    data: json!({}),
                    portfolios: Some(leaves.portfolios.clone()),
                },
            )),
            "/api/v3/brokerage/portfolios/{portfolio_uuid}" => Promise::resolved(Ok(
                safe_cex_venue_coinbase::portfolios_balances::PortfolioReply {
                    data: json!({}),
                    portfolios: None,
                },
            )),
            "/api/v3/brokerage/cfm/balance_summary" => {
                Promise::resolved(Err(failure(json!({"response":{"status":403}}))))
            }
            _ => panic!("unreviewed native class policy endpoint {endpoint}"),
        }
    }
    fn emit_and_log_error(&self, args: &[Option<Value>]) -> Result<(), PortfolioFailure> {
        self.policy().errors.borrow_mut().push(args.to_vec());
        Ok(())
    }
    fn cached_balance(&self) -> safe_cex_venue_coinbase::portfolios_balances::Balance {
        panic!("policy must not read cached portfolio balance")
    }
    fn position_upnls(&self) -> Vec<f64> {
        panic!("policy must not read cached positions")
    }
}

fn policy_class(
    spot: bool,
    options: Value,
    queue: &Microtasks,
) -> (Rc<CoinbaseExchange<ClassHost>>, Rc<ClassPolicyLeaves>) {
    let input = fixture()["classes"][usize::from(spot)].clone();
    assert_eq!(
        input["label"],
        if spot {
            "spot subtype"
        } else {
            "exchange default"
        },
        "independent constructor fixture identity"
    );
    let capture = source()["classes"][usize::from(spot)].clone();
    let host = Rc::new(ClassHost {
        input,
        trace: Trace::new(&capture["trace"], "native class policy"),
        options: Frame::from_json(&options),
        store: Frame::from_json(&json!({"tag":"store"})),
        identity: RefCell::new(None),
        emitter: Frame::from_json(&json!({"tag":"emitter"})),
        original_error: failure(json!({"message":"unreachable constructor error"})),
        mint: RefCell::new(None),
        uuid_count: Cell::new(0.0),
        api_count: Cell::new(0),
        policy: RefCell::new(None),
        captured_finder: RefCell::new(None),
        captured_supported: RefCell::new(None),
    });
    let exchange = if spot {
        CoinbaseSpotExchange::new(
            host.clone(),
            "coinbase".into(),
            host.options.clone(),
            host.store.clone(),
            queue.spawner(),
        )
        .unwrap()
        .exchange
    } else {
        CoinbaseExchange::new(
            host.clone(),
            "coinbase".into(),
            host.options.clone(),
            host.store.clone(),
            queue.spawner(),
        )
        .unwrap()
    };
    let leaves = ClassPolicyLeaves::new();
    *host.policy.borrow_mut() = Some(leaves.clone());
    (exchange, leaves)
}
fn policy_value(
    promise: ClassSupportedResult,
    queue: &Microtasks,
) -> safe_cex_venue_coinbase::portfolios_balances::SupportedTypes {
    queue.drain();
    completed(promise).unwrap()
}
fn policy_portfolio(
    promise: ClassFindResult,
    queue: &Microtasks,
) -> safe_cex_venue_coinbase::portfolios_balances::Portfolio {
    queue.drain();
    completed(promise).unwrap()
}
fn policy_calls(leaves: &ClassPolicyLeaves, expected: &[(&str, Option<&str>)]) {
    assert_eq!(
        leaves
            .calls
            .borrow()
            .iter()
            .map(|(endpoint, uuid)| (*endpoint, uuid.as_deref()))
            .collect::<Vec<_>>(),
        expected,
        "class native HTTP call-site content"
    );
}
fn policy_source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/live-class-policy.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source class policy: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        capture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "no-credentials",
            "key-without-signer",
            "local-selection-and-account",
            "remote-signer",
            "replace-options-slot",
            "live-support-spot",
            "spot-override-and-captured-base-field",
            "signed-captured-base-calls-spot-finder",
            "retained-base-finder-versus-current-spot",
            "supported-types-promise-retains-array-root",
            "account-promise-retains-object-root",
            "public-only-promise-retains-array-root",
            "spot-promise-retains-array-root",
            "live-policy-after-pending-selection"
        ],
        "independent native class policy source content pin"
    );
    capture
}
#[test]
fn class_policy_reads_live_slots_keeps_captured_base_bodies_and_retains_native_promise_roots() {
    let source = policy_source();
    let queue = Microtasks::default();
    let mut visited = vec![];
    for (index, options) in [(0, json!({})), (1, json!({"key":"fixture-key"}))] {
        let (class, leaves) = policy_class(false, options, &queue);
        let value = policy_value(class.supported_types(), &queue);
        assert_eq!(
            *value.borrow(),
            ["intx"],
            "class native missing signer is public only"
        );
        assert!(leaves.calls.borrow().is_empty());
        assert!(class.selected_portfolio.borrow().is_none());
        assert_eq!(
            *value.borrow(),
            source["cases"][index]["types"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        visited.push(index);
    }
    {
        let (class, leaves) = policy_class(
            false,
            json!({"key":"fixture-key","secret":"fixture-secret"}),
            &queue,
        );
        leaves.portfolios.borrow_mut().remove(0);
        let value = policy_value(class.supported_types(), &queue);
        assert_eq!(
            *value.borrow(),
            ["spot"],
            "class native signing policy selects ordinary portfolio"
        );
        policy_calls(
            &leaves,
            &[
                ("/api/v3/brokerage/portfolios", None),
                (
                    "/api/v3/brokerage/portfolios/{portfolio_uuid}",
                    Some("ordinary"),
                ),
                ("/api/v3/brokerage/cfm/balance_summary", None),
            ],
        );
        assert_eq!(leaves.calls.borrow().len(), 3);
        assert!(
            Rc::ptr_eq(
                class.selected_portfolio.borrow().as_ref().unwrap(),
                &leaves.portfolios.borrow()[0]
            ),
            "class native selected portfolio shares imported root"
        );
        let account = class.get_account();
        queue.drain();
        let account = completed(account).unwrap();
        assert_eq!(account.borrow().user_id, "ordinary");
        assert_eq!(leaves.calls.borrow().len(), 3);
        assert_eq!(
            account.borrow().user_id,
            source["cases"][2]["account"]["userId"]
        );
        visited.push(2);
    }
    {
        let (class, leaves) = policy_class(
            false,
            json!({"key":"fixture-key","remoteSigner":true}),
            &queue,
        );
        *class.selected_portfolio.borrow_mut() = Some(leaves.portfolios.borrow()[1].clone());
        let value = policy_value(class.supported_types(), &queue);
        assert_eq!(*value.borrow(), ["spot"]);
        assert!(leaves.calls.borrow().is_empty());
        assert_eq!(
            *value.borrow(),
            source["cases"][3]["types"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        visited.push(3);
    }
    {
        let (class, leaves) = policy_class(
            false,
            json!({"key":"fixture-key","secret":"fixture-secret"}),
            &queue,
        );
        let original = class.options.borrow().clone();
        *class.options.borrow_mut() = Frame::from_json(&json!({}));
        let value = policy_value(class.supported_types(), &queue);
        assert_eq!(
            *value.borrow(),
            ["intx"],
            "class native replacement options slot controls current signing policy"
        );
        assert!(!same(&original, &class.options.borrow()));
        assert!(same(&original, &class.host.options));
        assert!(
            leaves.calls.borrow().is_empty(),
            "class native replaced options avoids portfolio side effects"
        );
        assert!(
            class.selected_portfolio.borrow().is_none(),
            "class native replaced options keeps selection empty"
        );
        assert_eq!(source["cases"][4]["factoriesKeepOriginal"], true);
        visited.push(4);
    }
    {
        let (class, leaves) = policy_class(
            false,
            json!({"key":"fixture-key","secret":"fixture-secret"}),
            &queue,
        );
        *class.selected_portfolio.borrow_mut() = Some(Rc::new(RefCell::new(
            serde_json::from_value(json!({"uuid":"cfm","name":"cfm","type":"CFM","deleted":false}))
                .unwrap(),
        )));
        let receiver = class.portfolio_receiver(class.host.clone(), false);
        class.support_spot.set(false);
        let without = policy_value(
            safe_cex_venue_coinbase::portfolios_balances::get_supported_types(receiver.clone()),
            &queue,
        );
        class.support_spot.set(true);
        let with = policy_value(
            safe_cex_venue_coinbase::portfolios_balances::get_supported_types(receiver),
            &queue,
        );
        assert_eq!(
            *without.borrow(),
            ["cfm"],
            "class native retained receiver reads changed supportSpot"
        );
        assert_eq!(*with.borrow(), ["cfm", "spot"]);
        assert!(leaves.calls.borrow().is_empty());
        assert_eq!(
            *with.borrow(),
            source["cases"][5]["withSpot"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        visited.push(5);
    }
    {
        let (class, leaves) = policy_class(true, json!({}), &queue);
        let base = class
            .host
            .captured_supported
            .borrow()
            .as_ref()
            .unwrap()
            .clone();
        let current = class.capture_supported();
        assert!(
            !base.same_function(&current),
            "class native Spot replaces base supported arrow"
        );
        assert!(
            current.same_function(&class.capture_supported()),
            "class native current arrow read preserves function identity"
        );
        let retained = policy_value(base.call(), &queue);
        let now = policy_value(current.call(), &queue);
        assert_eq!(
            *retained.borrow(),
            ["intx"],
            "class native retained base supported arrow keeps base body"
        );
        assert_eq!(
            *now.borrow(),
            ["spot"],
            "class native current supported arrow installs Spot body"
        );
        policy_calls(
            &leaves,
            &[
                ("/api/v3/brokerage/portfolios", None),
                (
                    "/api/v3/brokerage/portfolios/{portfolio_uuid}",
                    Some("ordinary"),
                ),
            ],
        );
        assert_eq!(
            class
                .selected_portfolio
                .borrow()
                .as_ref()
                .unwrap()
                .borrow()
                .uuid,
            "ordinary",
            "class native current Spot finder selects ordinary root"
        );
        assert_eq!(leaves.calls.borrow().len(), 2);
        assert_eq!(
            *retained.borrow(),
            source["cases"][6]["capturedTypes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            *now.borrow(),
            source["cases"][6]["currentTypes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        visited.push(6);
    }
    {
        let (class, leaves) = policy_class(
            true,
            json!({"key":"fixture-key","secret":"fixture-secret"}),
            &queue,
        );
        leaves.portfolios.borrow()[1].borrow_mut().r#type = "CFM".into();
        let base = class
            .host
            .captured_supported
            .borrow()
            .as_ref()
            .unwrap()
            .clone();
        let retained = policy_value(base.call(), &queue);
        let now = policy_value(class.supported_types(), &queue);
        assert_eq!(
            *retained.borrow(),
            ["cfm", "spot"],
            "class native retained base arrow calls current Spot finder"
        );
        assert_eq!(*now.borrow(), ["spot"]);
        policy_calls(
            &leaves,
            &[
                ("/api/v3/brokerage/portfolios", None),
                (
                    "/api/v3/brokerage/portfolios/{portfolio_uuid}",
                    Some("ordinary"),
                ),
            ],
        );
        assert_eq!(leaves.calls.borrow().len(), 2);
        assert!(Rc::ptr_eq(
            class.selected_portfolio.borrow().as_ref().unwrap(),
            &leaves.portfolios.borrow()[1]
        ));
        assert_eq!(
            *retained.borrow(),
            source["cases"][7]["retained"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        visited.push(7);
    }
    {
        let (class, leaves) = policy_class(
            true,
            json!({"key":"fixture-key","secret":"fixture-secret"}),
            &queue,
        );
        leaves.portfolios.borrow()[1].borrow_mut().r#type = "CFM".into();
        let base = class
            .host
            .captured_finder
            .borrow()
            .as_ref()
            .unwrap()
            .clone();
        let retained = policy_portfolio(base.call(), &queue);
        let current = policy_portfolio(class.find_selected(), &queue);
        assert!(
            Rc::ptr_eq(&retained, &leaves.portfolios.borrow()[0]),
            "class native retained base finder keeps base body"
        );
        assert!(
            Rc::ptr_eq(&current, &leaves.portfolios.borrow()[1]),
            "class native current finder is Spot override"
        );
        assert!(class.selected_portfolio.borrow().is_none());
        policy_calls(
            &leaves,
            &[
                ("/api/v3/brokerage/portfolios", None),
                (
                    "/api/v3/brokerage/portfolios/{portfolio_uuid}",
                    Some("intx"),
                ),
                (
                    "/api/v3/brokerage/portfolios/{portfolio_uuid}",
                    Some("ordinary"),
                ),
                ("/api/v3/brokerage/portfolios", None),
                (
                    "/api/v3/brokerage/portfolios/{portfolio_uuid}",
                    Some("ordinary"),
                ),
            ],
        );
        assert_eq!(leaves.calls.borrow().len(), 5);
        assert_eq!(retained.borrow().uuid, source["cases"][8]["retainedUuid"]);
        assert_eq!(current.borrow().uuid, source["cases"][8]["currentUuid"]);
        visited.push(8);
    }
    for (index, spot, options, expected) in [
        (
            9,
            false,
            json!({"key":"fixture-key","secret":"fixture-secret"}),
            vec!["cfm", "spot"],
        ),
        (11, false, json!({}), vec!["intx"]),
        (12, true, json!({}), vec!["spot"]),
    ] {
        let (class, leaves) = policy_class(spot, options, &queue);
        *class.selected_portfolio.borrow_mut() = Some(Rc::new(RefCell::new(
            serde_json::from_value(json!({"uuid":"cfm","name":"cfm","type":"CFM","deleted":false}))
                .unwrap(),
        )));
        let promise = class.supported_types();
        let first = policy_value(promise.clone(), &queue);
        assert_eq!(*first.borrow(), expected);
        first.borrow_mut().push("intx".into());
        let second = policy_value(promise, &queue);
        assert!(
            Rc::ptr_eq(&first, &second),
            "class native same Promise keeps mutable supported array"
        );
        let fresh = policy_value(class.supported_types(), &queue);
        assert!(!Rc::ptr_eq(&first, &fresh));
        assert_eq!(*fresh.borrow(), expected);
        assert!(leaves.calls.borrow().is_empty());
        assert_eq!(
            *second.borrow(),
            source["cases"][index]["second"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        visited.push(index);
    }
    {
        let (class, leaves) = policy_class(
            false,
            json!({"key":"fixture-key","secret":"fixture-secret"}),
            &queue,
        );
        let selected = Rc::new(RefCell::new(
            serde_json::from_value(json!({"uuid":"cfm","name":"cfm","type":"CFM","deleted":false}))
                .unwrap(),
        ));
        *class.selected_portfolio.borrow_mut() = Some(selected.clone());
        let promise = class.get_account();
        queue.drain();
        let first = completed(promise.clone()).unwrap();
        first.borrow_mut().user_id = "caller-mutated".into();
        let second = completed(promise).unwrap();
        assert!(
            Rc::ptr_eq(&first, &second),
            "class native same account Promise keeps mutable object"
        );
        assert_eq!(second.borrow().user_id, "caller-mutated");
        let fresh = class.get_account();
        queue.drain();
        let fresh = completed(fresh).unwrap();
        assert!(!Rc::ptr_eq(&first, &fresh));
        assert_eq!(fresh.borrow().user_id, "cfm");
        assert_eq!(selected.borrow().uuid, "cfm");
        assert!(leaves.calls.borrow().is_empty());
        assert_eq!(second.borrow().user_id, source["cases"][10]["secondUserId"]);
        visited.push(10);
    }
    {
        let (class, leaves) = policy_class(
            false,
            json!({"key":"fixture-key","secret":"fixture-secret"}),
            &queue,
        );
        leaves.portfolios.borrow_mut().remove(0);
        let (pending, resolve) = Promise::pending();
        *leaves.pending_breakdown.borrow_mut() = Some(pending);
        let mut request = class.supported_types();
        queue.drain();
        assert!(
            is_pending(&mut request),
            "class native selection stays suspended before external completion"
        );
        policy_calls(
            &leaves,
            &[
                ("/api/v3/brokerage/portfolios", None),
                (
                    "/api/v3/brokerage/portfolios/{portfolio_uuid}",
                    Some("ordinary"),
                ),
            ],
        );
        assert!(class.selected_portfolio.borrow().is_none());
        *class.options.borrow_mut() = Frame::from_json(&json!({}));
        class.support_spot.set(false);
        resolve.resolve(Ok(
            safe_cex_venue_coinbase::portfolios_balances::PortfolioReply {
                data: json!({}),
                portfolios: None,
            },
        ));
        queue.drain();
        let value = completed(request).unwrap();
        assert!(
            value.borrow().is_empty(),
            "class native supportSpot is reread after pending selection"
        );
        assert!(Rc::ptr_eq(
            class.selected_portfolio.borrow().as_ref().unwrap(),
            &leaves.portfolios.borrow()[0]
        ));
        policy_calls(
            &leaves,
            &[
                ("/api/v3/brokerage/portfolios", None),
                (
                    "/api/v3/brokerage/portfolios/{portfolio_uuid}",
                    Some("ordinary"),
                ),
                ("/api/v3/brokerage/cfm/balance_summary", None),
            ],
        );
        assert_eq!(
            *leaves.errors.borrow(),
            [vec![Some(json!("No supported portfolios found"))]],
            "class native empty supported types emits exactly one owned error"
        );
        assert_eq!(value.borrow().len(), 0);
        assert_eq!(source["cases"][13]["types"], json!([]));
        assert_eq!(source["cases"][13]["selectedUuid"], "ordinary");
        visited.push(13);
    }
    visited.sort();
    assert_eq!(
        visited,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13],
        "independent native class policy scenario floor"
    );
}

#[test]
fn arrow_value_transplanted_supported_arrow_retains_origin() {
    let q = Microtasks::default();
    let (origin, _) = policy_class(
        false,
        json!({"key":"fixture-key","secret":"fixture-secret"}),
        &q,
    );
    *origin.selected_portfolio.borrow_mut() = Some(Rc::new(RefCell::new(
        serde_json::from_value(
            json!({"uuid":"origin-cfm","name":"origin","type":"CFM","deleted":false}),
        )
        .unwrap(),
    )));
    origin.support_spot.set(false);
    let weak = Rc::downgrade(&origin);
    let (target, _) = policy_class(false, json!({}), &q);
    let original_target = target.capture_supported();
    target.set_supported(origin.capture_supported());
    drop(origin.host.captured_finder.borrow_mut().take());
    drop(origin.host.captured_supported.borrow_mut().take());
    drop(target.host.captured_finder.borrow_mut().take());
    drop(target.host.captured_supported.borrow_mut().take());
    drop(origin);
    assert_eq!(
        &*policy_value(target.supported_types(), &q).borrow(),
        &["cfm"],
        "native transplanted supported arrow retains original receiver"
    );
    assert!(
        target.selected_portfolio.borrow().is_none(),
        "native target selection stays untouched"
    );
    let retained = weak
        .upgrade()
        .expect("native bound field keeps origin alive");
    assert!(
        target
            .capture_supported()
            .same_function(&retained.capture_supported()),
        "native transplant preserves original function value identity"
    );
    drop(retained);
    let weak_target = Rc::downgrade(&target);
    target.set_supported(original_target);
    q.drain();
    assert!(
        weak.upgrade().is_none(),
        "native field replacement releases origin without an ordinary self cycle"
    );
    drop(target);
    q.drain();
    assert!(
        weak_target.upgrade().is_none(),
        "native self assignment releases target without a redundant receiver edge"
    );
    let expected = arrow_source_case("supported-arrow-retains-origin");
    assert_eq!(expected["types"], json!(["cfm"]));
    assert_eq!(expected["sameFunction"], true);
    assert!(expected["targetSelected"].is_null());
}
#[test]
fn arrow_value_transplanted_finder_retains_origin() {
    let q = Microtasks::default();
    let (origin, leaves) = policy_class(
        false,
        json!({"key":"fixture-key","secret":"fixture-secret"}),
        &q,
    );
    let expected = leaves.portfolios.borrow()[0].clone();
    expected.borrow_mut().uuid = "origin-intx".into();
    let (target, target_leaves) = policy_class(
        false,
        json!({"key":"fixture-key","secret":"fixture-secret"}),
        &q,
    );
    target_leaves.portfolios.borrow()[0].borrow_mut().uuid = "target-intx".into();
    target.set_finder(origin.capture_finder());
    drop(origin.host.captured_finder.borrow_mut().take());
    drop(origin.host.captured_supported.borrow_mut().take());
    drop(target.host.captured_finder.borrow_mut().take());
    drop(target.host.captured_supported.borrow_mut().take());
    drop(origin);
    let selected = policy_portfolio(target.find_selected(), &q);
    assert_eq!(
        selected.borrow().uuid,
        "origin-intx",
        "native transplanted finder reads original receiver HTTP leaves"
    );
    assert!(
        Rc::ptr_eq(&selected, &expected),
        "native transplanted finder returns original portfolio root"
    );
    policy_calls(
        &leaves,
        &[
            ("/api/v3/brokerage/portfolios", None),
            (
                "/api/v3/brokerage/portfolios/{portfolio_uuid}",
                Some("origin-intx"),
            ),
            (
                "/api/v3/brokerage/portfolios/{portfolio_uuid}",
                Some("ordinary"),
            ),
        ],
    );
    policy_calls(&target_leaves, &[]);
    assert!(target.selected_portfolio.borrow().is_none());
    let expected = arrow_source_case("finder-arrow-retains-origin");
    assert_eq!(expected["selectedUuid"], "origin-intx");
    assert_eq!(expected["samePortfolio"], true);
    assert_eq!(expected["targetCalls"], 0);
}
#[test]
fn arrow_value_transplanted_arrow_retains_live_origin_option_slot() {
    let q = Microtasks::default();
    let (origin, _) = policy_class(
        false,
        json!({"key":"fixture-key","secret":"fixture-secret"}),
        &q,
    );
    *origin.selected_portfolio.borrow_mut() = Some(Rc::new(RefCell::new(
        serde_json::from_value(
            json!({"uuid":"origin-cfm","name":"origin","type":"CFM","deleted":false}),
        )
        .unwrap(),
    )));
    origin.support_spot.set(false);
    let weak = Rc::downgrade(&origin);
    let (target, _) = policy_class(false, json!({}), &q);
    target.set_supported(origin.capture_supported());
    drop(origin.host.captured_finder.borrow_mut().take());
    drop(origin.host.captured_supported.borrow_mut().take());
    drop(target.host.captured_finder.borrow_mut().take());
    drop(target.host.captured_supported.borrow_mut().take());
    drop(origin);
    let retained = weak
        .upgrade()
        .expect("native transplanted arrow retains origin after external root drop");
    *retained.options.borrow_mut() = Frame::from_json(&json!({}));
    assert_eq!(
        &*policy_value(target.supported_types(), &q).borrow(),
        &["intx"],
        "native transplanted arrow reads replaced origin options"
    );
    *retained.options.borrow_mut() =
        Frame::from_json(&json!({"key":"fixture-key","secret":"fixture-secret"}));
    assert_eq!(
        &*policy_value(target.supported_types(), &q).borrow(),
        &["cfm"],
        "native transplanted arrow rereads restored origin options"
    );
    let expected = arrow_source_case("transplanted-arrow-reads-origin-option-slot");
    assert_eq!(expected["without"], json!(["intx"]));
    assert_eq!(expected["withCredentials"], json!(["cfm"]));
}
#[test]
fn arrow_value_reassigned_arrow_keeps_old_code_and_origin() {
    let q = Microtasks::default();
    let (origin, _) = policy_class(
        false,
        json!({"key":"fixture-key","secret":"fixture-secret"}),
        &q,
    );
    *origin.selected_portfolio.borrow_mut() = Some(Rc::new(RefCell::new(
        serde_json::from_value(
            json!({"uuid":"origin-cfm","name":"origin","type":"CFM","deleted":false}),
        )
        .unwrap(),
    )));
    origin.support_spot.set(false);
    let (target, _) = policy_class(false, json!({}), &q);
    let (replacement, _) = policy_class(true, json!({}), &q);
    *replacement.selected_portfolio.borrow_mut() = Some(Rc::new(RefCell::new(
        serde_json::from_value(
            json!({"uuid":"replacement-cfm","name":"replacement","type":"CFM","deleted":false}),
        )
        .unwrap(),
    )));
    let original = origin.capture_supported();
    target.set_supported(origin.capture_supported());
    origin.set_supported(replacement.capture_supported());
    assert_eq!(
        &*policy_value(origin.supported_types(), &q).borrow(),
        &["spot"]
    );
    origin.set_supported(target.capture_supported());
    assert_eq!(
        &*policy_value(origin.supported_types(), &q).borrow(),
        &["cfm"]
    );
    assert_eq!(
        &*policy_value(target.supported_types(), &q).borrow(),
        &["cfm"],
        "native reassigned arrow retains original code and receiver"
    );
    assert!(target.capture_supported().same_function(&original));
    assert!(origin.capture_supported().same_function(&original));
    let expected = arrow_source_case("reassigned-arrow-keeps-old-code-and-origin");
    assert_eq!(expected["intermediate"], json!(["spot"]));
    assert_eq!(expected["restored"], json!(["cfm"]));
    assert_eq!(expected["throughTarget"], json!(["cfm"]));
    assert_eq!(expected["sameFunction"], true);
}

fn arrow_source_case(id: &str) -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/live-arrow-values.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live arrow value source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = value["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4, "independent arrow value source floor");
    assert_eq!(
        cases
            .iter()
            .map(|case| case["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "supported-arrow-retains-origin",
            "finder-arrow-retains-origin",
            "transplanted-arrow-reads-origin-option-slot",
            "reassigned-arrow-keeps-old-code-and-origin"
        ],
        "independent arrow value source content"
    );
    cases.iter().find(|case| case["id"] == id).unwrap().clone()
}
