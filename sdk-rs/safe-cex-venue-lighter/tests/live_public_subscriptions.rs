use safe_cex_venue_lighter::{
    sdk_account_api::{AccountObject, AccountValue},
    transaction_builders::TransactionBuildError,
    transaction_signing::{HostThrownValue, SigningResult, TransactionSigningError},
    ws_public_subscriptions::*,
};
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap, process::Command, rc::Rc};
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u16)
        .collect()
}
struct Marker;
impl AccountObject for Marker {
    fn get(&self, _: &str) -> SigningResult<AccountValue> {
        Ok(AccountValue::Undefined)
    }
}
struct Literal {
    fields: Vec<(String, AccountValue)>,
}
impl AccountObject for Literal {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        Ok(self
            .fields
            .iter()
            .find(|x| x.0 == key)
            .map(|x| x.1.clone())
            .unwrap_or(AccountValue::Undefined))
    }
}
#[derive(Clone)]
struct Entry {
    key: Vec<u16>,
    value: AccountValue,
    enumerable: bool,
}
struct Invocation {
    name: String,
    receiver: AccountValue,
    args: Vec<AccountValue>,
}
struct Tick {
    callback: SubscriptionCallback,
    delay: f64,
}
struct Env {
    input: Value,
    objects: BTreeMap<String, Rc<dyn AccountObject>>,
    functions: BTreeMap<String, HostThrownValue>,
    token: HostThrownValue,
    props: RefCell<BTreeMap<String, Vec<Entry>>>,
    arrays: RefCell<BTreeMap<String, Vec<AccountValue>>>,
    literals: RefCell<Vec<Rc<Literal>>>,
    trace: RefCell<Vec<Vec<u16>>>,
    seen: RefCell<BTreeMap<Vec<u16>, usize>>,
    calls: RefCell<Vec<Invocation>>,
    queue: RefCell<Vec<Tick>>,
    iter_receivers: RefCell<Vec<AccountValue>>,
}
#[derive(Clone)]
struct Fixture(Rc<Env>);
impl Fixture {
    fn id(&self, v: &AccountValue) -> String {
        match v {
            AccountValue::Object(obj) => {
                for (id, a) in &self.0.objects {
                    if Rc::ptr_eq(obj, a) {
                        return id.clone();
                    }
                }
                for (i, a) in self.0.literals.borrow().iter().enumerate() {
                    let b: Rc<dyn AccountObject> = a.clone();
                    if Rc::ptr_eq(obj, &b) {
                        return format!("L{i}");
                    }
                }
                panic!("unclassified native object")
            }
            AccountValue::Function(f) => self
                .0
                .functions
                .iter()
                .find(|(_, v)| f.ptr_eq(v))
                .unwrap()
                .0
                .clone(),
            _ => panic!("native id requires object/function"),
        }
    }
    fn obj(&self, id: &str) -> AccountValue {
        AccountValue::Object(self.0.objects[id].clone())
    }
    fn decode(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "string" => AccountValue::String(units(&v["units"])),
            "number" => AccountValue::Number(f64::from_bits(
                u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
            )),
            "object" => self.obj(v["id"].as_str().unwrap()),
            "function" => {
                AccountValue::Function(self.0.functions[v["id"].as_str().unwrap()].clone())
            }
            _ => panic!("source input native kind"),
        }
    }
    fn value(&self, name: &str) -> AccountValue {
        self.decode(&self.0.input[name])
    }
    fn boolean_optional(&self, name: &str) -> bool {
        self.0.input[name]["value"] == true
    }
    fn boolean(&self, name: &str) -> bool {
        matches!(self.value(name), AccountValue::Boolean(true))
    }
    fn input_text(&self, name: &str) -> Option<Vec<u16>> {
        match self.0.input[name]["kind"].as_str() {
            Some("string") => Some(units(&self.0.input[name]["units"])),
            _ => None,
        }
    }
    fn throw(&self) -> TransactionSigningError {
        TransactionSigningError::Thrown(self.0.token.clone())
    }
    fn write(&self, id: &str, key: &[u16], value: AccountValue) {
        let mut props = self.0.props.borrow_mut();
        let fields = props.get_mut(id).unwrap();
        if let Some(e) = fields.iter_mut().find(|x| x.key == key) {
            e.value = value
        } else {
            fields.push(Entry {
                key: key.to_vec(),
                value,
                enumerable: true,
            })
        }
    }
    fn trace(&self, event: String) -> SigningResult<()> {
        self.trace_units(u(&event))
    }
    fn trace_units(&self, event: Vec<u16>) -> SigningResult<()> {
        self.0.trace.borrow_mut().push(event.clone());
        let nth = {
            let mut s = self.0.seen.borrow_mut();
            let n = s.entry(event.clone()).or_insert(0);
            *n += 1;
            *n
        };
        if self.input_text("effectAt") == Some(event.clone()) {
            let target = match self.0.input["effectNth"]["kind"].as_str() {
                Some("number") => {
                    let AccountValue::Number(n) = self.value("effectNth") else {
                        unreachable!()
                    };
                    n as usize
                }
                _ => 1,
            };
            if nth == target {
                match self.input_text("effect").as_deref() {
                    Some(s) if s == u("topics") => {
                        self.write("Owner", &u("topics"), self.obj("Topics1"))
                    }
                    Some(s) if s == u("intervals") => self.write(
                        "Owner",
                        &u("orderBookRestFallbackIntervals"),
                        self.obj("Intervals1"),
                    ),
                    Some(s) if s == u("socket") => {
                        self.write("Owner", &u("ws"), self.obj("Socket1"))
                    }
                    _ => panic!("native effect"),
                }
            }
        }
        if self.input_text("throwAt") == Some(event.clone()) {
            Err(self.throw())
        } else {
            Ok(())
        }
    }
    // Trace keys must preserve isolated surrogates; avoid Rust String as their carrier.
    fn keyed_trace(&self, prefix: &str, id: &str, key: &[u16]) -> SigningResult<()> {
        let mut e = u(prefix);
        e.extend(u(id));
        e.push(':' as u16);
        e.extend_from_slice(key);
        self.trace_units(e)
    }
    fn assert_value(&self, a: &AccountValue, e: &Value, label: &str) {
        match (a, e["kind"].as_str().unwrap()) {
            (AccountValue::Undefined, "undefined") | (AccountValue::Null, "null") => {}
            (AccountValue::Boolean(a), "boolean") => {
                assert_eq!(*a, e["value"].as_bool().unwrap(), "{label} native boolean")
            }
            (AccountValue::Number(a), "number") => {
                let AccountValue::Number(b) = self.decode(e) else {
                    unreachable!()
                };
                if b.is_nan() {
                    assert!(a.is_nan(), "{label} native NaN")
                } else {
                    assert_eq!(a.to_bits(), b.to_bits(), "{label} native number bits")
                }
            }
            (AccountValue::String(a), "string") => {
                assert_eq!(*a, units(&e["units"]), "{label} native UTF16")
            }
            (AccountValue::Object(a), "object") => assert!(
                Rc::ptr_eq(a, &self.0.objects[e["id"].as_str().unwrap()]),
                "{label} native object identity"
            ),
            (AccountValue::Function(a), "function") => assert!(
                a.ptr_eq(&self.0.functions[e["id"].as_str().unwrap()]),
                "{label} native function identity"
            ),
            (AccountValue::Object(a), "literal") => {
                let n = e["id"]
                    .as_str()
                    .unwrap()
                    .strip_prefix('L')
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                let literals = self.0.literals.borrow();
                let b: Rc<dyn AccountObject> = literals[n].clone();
                assert!(Rc::ptr_eq(a, &b), "{label} native literal identity");
                self.assert_fields(&literals[n].fields, &e["fields"], label)
            }
            _ => panic!("{label} native kind mismatch"),
        }
    }
    fn assert_fields(&self, a: &[(String, AccountValue)], e: &Value, label: &str) {
        let expected = e.as_array().unwrap();
        assert_eq!(
            a.len(),
            expected.len(),
            "{label} independent source field count"
        );
        for (i, ((key, v), row)) in a.iter().zip(expected).enumerate() {
            assert_eq!(key, row[0].as_str().unwrap(), "{label} native key order");
            self.assert_value(v, &row[1], &format!("{label} field{i} {key}"));
        }
    }
    fn assert_output(&self, a: SigningResult<AccountValue>, e: &Value, label: &str) {
        match (a, e["kind"].as_str().unwrap()) {
            (Ok(a), "ok") => self.assert_value(&a, &e["value"], &format!("{label} result")),
            (Err(TransactionSigningError::Thrown(e)), "thrown") => {
                assert!(e.ptr_eq(&self.0.token), "{label} native thrown identity")
            }
            (Err(TransactionSigningError::Generated(a)), "generated") => {
                assert_eq!(
                    a.name,
                    e["name"].as_str().unwrap(),
                    "{label} native generated name"
                );
                assert_eq!(
                    a.message,
                    units(&e["message"]),
                    "{label} native generated UTF16"
                );
            }
            _ => panic!("{label} native completion mismatch"),
        }
    }
}
struct Call {
    host: Fixture,
    name: String,
    receiver: AccountValue,
}
impl SubscriptionCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        let h = &self.host;
        h.0.calls.borrow_mut().push(Invocation {
            name: self.name.clone(),
            receiver: self.receiver.clone(),
            args: args.to_vec(),
        });
        h.trace(format!("call:{}", self.name))?;
        Ok(match self.name.as_str() {
            "resolveTransportRouting" => {
                if h.boolean_optional("lostHostOnResolve")
                    && h.0.seen.borrow().get(&u("call:resolveTransportRouting")) == Some(&2)
                {
                    h.write("Public", &u("host"), AccountValue::Undefined);
                }
                h.obj("Routing")
            }
            "String.includes" => {
                let AccountValue::String(value) = &self.receiver else {
                    unreachable!()
                };
                let AccountValue::String(search) = &args[0] else {
                    unreachable!()
                };
                AccountValue::Boolean(
                    search.is_empty() || value.windows(search.len()).any(|w| w == search),
                )
            }
            "nanoid" => h.value("nanoid"),
            "virtualClock.getCurrentTimeMs" => h.value("now"),
            "JSON.stringify" => AccountValue::String(
                u("wire/\u{d7ff}")
                    .into_iter()
                    .map(|x| if x == 0xd7ff { 0xd800 } else { x })
                    .collect(),
            ),
            "String" => AccountValue::String(match &args[0] {
                AccountValue::String(s) => s.clone(),
                AccountValue::Number(n) => u(&safe_cex_num::js_number_string(*n)),
                AccountValue::Boolean(v) => u(if *v { "true" } else { "false" }),
                AccountValue::Undefined => u("undefined"),
                AccountValue::Null => u("null"),
                _ => panic!("fixture String domain"),
            }),
            "Array.isArray" => AccountValue::Boolean(
                matches!(&args[0], AccountValue::Object(_))
                    && h.0.arrays.borrow().contains_key(&h.id(&args[0])),
            ),
            "Object.keys" => {
                let id = h.id(&args[0]);
                h.trace(format!("ownKeys:{id}"))?;
                let fields = h.0.props.borrow()[&id].clone();
                let mut keys = vec![];
                for e in fields {
                    h.keyed_trace("descriptor:", &id, &e.key)?;
                    if e.enumerable {
                        keys.push(AccountValue::String(e.key))
                    }
                }
                h.0.arrays.borrow_mut().insert("Keys0".into(), keys);
                h.obj("Keys0")
            }
            "Owner.isWsProxyRouteEnabled" => AccountValue::Boolean(if h.boolean("composeProxy") {
                is_ws_proxy_route_enabled(h)?
            } else {
                h.boolean("proxy")
            }),
            "Owner.createWebsocket" => h.obj("Socket0"),
            "Owner.isOrderBookRestFallbackCoolingDown" => AccountValue::Boolean(h.boolean("cool")),
            "Owner.fetchAndEmitOrderBookFromRest" => h.obj("FetchReturn"),
            _ => AccountValue::Undefined,
        })
    }
}
struct Timer(Fixture);
impl SubscriptionTimer for Timer {
    fn call(&self, callback: SubscriptionCallback, delay: f64) -> SigningResult<AccountValue> {
        self.0.0.calls.borrow_mut().push(Invocation {
            name: "setInterval".into(),
            receiver: AccountValue::Undefined,
            args: vec![AccountValue::Number(delay)],
        });
        self.0.trace("call:setInterval".into())?;
        self.0.0.queue.borrow_mut().push(Tick { callback, delay });
        Ok(self.0.obj("Timer"))
    }
}
impl PublicSubscriptionHost for Fixture {
    fn owner(&self) -> AccountValue {
        self.obj("Owner")
    }
    fn get(&self, object: &AccountValue, key: &[u16]) -> SigningResult<AccountValue> {
        let id = self.id(object);
        self.keyed_trace("get:", &id, key)?;
        if id == "FetchReturn" && key == u("then") {
            return Err(self.throw());
        }
        if key == u("length") {
            if let Some(a) = self.0.arrays.borrow().get(&id) {
                return Ok(AccountValue::Number(a.len() as f64));
            }
        }
        Ok(self
            .0
            .props
            .borrow()
            .get(&id)
            .and_then(|p| p.iter().find(|e| e.key == key))
            .map(|e| e.value.clone())
            .unwrap_or_else(|| {
                if key == u("constructor") {
                    AccountValue::Function(self.0.functions["constructor"].clone())
                } else {
                    AccountValue::Undefined
                }
            }))
    }
    fn set(&self, object: &AccountValue, key: &[u16], value: AccountValue) -> SigningResult<()> {
        let id = self.id(object);
        self.keyed_trace("set:", &id, key)?;
        self.write(&id, key, value);
        Ok(())
    }
    fn delete(&self, object: &AccountValue, key: &[u16]) -> SigningResult<()> {
        let id = self.id(object);
        self.keyed_trace("delete:", &id, key)?;
        self.0
            .props
            .borrow_mut()
            .get_mut(&id)
            .unwrap()
            .retain(|e| e.key != key);
        Ok(())
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn SubscriptionCall>> {
        self.trace(format!("get:{name}"))?;
        let receiver = match name.split_once('.') {
            Some((id, _)) => self.obj(id),
            None => AccountValue::Undefined,
        };
        Ok(Rc::new(Call {
            host: self.clone(),
            name: name.into(),
            receiver,
        }))
    }
    fn member(&self, object: &AccountValue, key: &str) -> SigningResult<Rc<dyn SubscriptionCall>> {
        if matches!(object, AccountValue::Undefined) {
            return Err(TransactionSigningError::Generated(TransactionBuildError {
                name: "TypeError",
                message: u("Cannot read properties of undefined (reading 'includes')"),
            }));
        }
        if matches!(object, AccountValue::String(_)) && key == "includes" {
            let name = "String.includes";
            self.trace(format!("get:{name}"))?;
            return Ok(Rc::new(Call {
                host: self.clone(),
                name: name.into(),
                receiver: object.clone(),
            }));
        }

        let id = self.id(object);
        if id != "Owner" {
            self.keyed_trace("get:", &id, &u(key))?
        }
        let name = format!("{id}.{key}");
        self.trace(format!("get:{name}"))?;
        Ok(Rc::new(Call {
            host: self.clone(),
            name,
            receiver: object.clone(),
        }))
    }
    fn interval(&self) -> SigningResult<Rc<dyn SubscriptionTimer>> {
        self.trace("get:setInterval".into())?;
        Ok(Rc::new(Timer(self.clone())))
    }
    fn for_of(
        &self,
        value: &AccountValue,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        let id = self.id(value);
        self.trace(format!("iter.get:{id}"))?;
        self.0.iter_receivers.borrow_mut().push(value.clone());
        self.trace(format!("iter.call:{id}"))?;
        self.trace(format!("next.get:{id}"))?;
        let mut i = 0;
        loop {
            self.trace(format!("next.call:{id}:{i}"))?;
            if self.input_text("iteratorFailure") == Some(u("next")) {
                return Err(self.throw());
            }
            self.trace(format!("done.get:{id}:{i}"))?;
            if i >= self.0.arrays.borrow()[&id].len() {
                return Ok(());
            }
            self.trace(format!("value.get:{id}:{i}"))?;
            let v = self.0.arrays.borrow()[&id][i].clone();
            i += 1;
            if let Err(error) = visit(v) {
                let _ = self.trace(format!("return.get:{id}"));
                if self.input_text("iteratorFailure") != Some(u("closeGet")) {
                    let _ = self.trace(format!("return.call:{id}"));
                }
                return Err(error);
            }
        }
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        let value = Rc::new(Literal {
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        });
        self.0.literals.borrow_mut().push(value.clone());
        Ok(AccountValue::Object(value))
    }
    fn less_than(&self, left: f64, right: AccountValue) -> SigningResult<bool> {
        let right = match right {
            AccountValue::Number(n) => n,
            AccountValue::Function(_) => f64::NAN,
            _ => panic!("fixture relational Number/function domain"),
        };
        Ok(left < right)
    }
}
fn run(h: &Fixture, method: &str) -> SigningResult<AccountValue> {
    let topic = match h.value("topic") {
        AccountValue::String(s) => s,
        _ => unreachable!(),
    };
    let outcome = match method {
        "isWsProxyRouteEnabled" => return is_ws_proxy_route_enabled(h).map(AccountValue::Boolean),
        "connectAndSubscribe" => connect_and_subscribe(h, None),
        "onMarketsChanged" => on_markets_changed(h, &h.obj("Event")),
        "sendPayload" => send_payload(h, &h.obj("Payload")),
        "syncBboTickerTopics" => sync_bbo_ticker_topics(h),
        "handleTickersEvent" => handle_tickers_event(h, &h.obj("Json")),
        "isOrderBookRestFallbackCoolingDown" => {
            return is_order_book_rest_fallback_cooling_down(h, &topic).map(AccountValue::Boolean);
        }
        "startOrderBookRestFallback" => {
            let AccountValue::String(id) = h.value("marketId") else {
                unreachable!()
            };
            let AccountValue::String(reason) = h.value("reason") else {
                unreachable!()
            };
            start_order_book_rest_fallback(
                Rc::new(h.clone()),
                &topic,
                &id,
                h.obj("Book"),
                AccountValue::Function(h.0.functions["Callback"].clone()),
                &reason,
            )
        }
        _ => unreachable!(),
    };
    outcome.map(|_| AccountValue::Undefined)
}
#[test]
#[ignore = "executes actual TypeScript source at SAFE_CEX_TYPESCRIPT_PATH"]
fn public_subscriptions_native_routes_topics_aliases_callbacks() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-subscriptions-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        209,
        "independent authored floor stopped iteration208"
    );
    let mut counts = BTreeMap::new();
    let mut forwarding_checks = vec![];
    for (i, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        *counts.entry(method).or_insert(0usize) += 1;
        let label = format!("{method} iteration{i}");
        let mut ids: Vec<String> = row["recordInputs"]
            .as_object()
            .unwrap()
            .keys()
            .chain(row["arrayInputs"].as_object().unwrap().keys())
            .cloned()
            .collect();
        ids.extend(["Owner", "virtualClock", "JSON", "Object", "Array"].map(String::from));
        let h = Fixture(Rc::new(Env {
            input: row["input"].clone(),
            objects: ids
                .into_iter()
                .map(|id| (id, Rc::new(Marker) as Rc<dyn AccountObject>))
                .collect(),
            functions: ["Callback", "Open", "Message", "Close", "constructor"]
                .into_iter()
                .map(|id| (id.into(), HostThrownValue::new((i, id))))
                .collect(),
            token: HostThrownValue::new(i),
            props: RefCell::new(BTreeMap::new()),
            arrays: RefCell::new(BTreeMap::new()),
            literals: RefCell::new(vec![]),
            trace: RefCell::new(vec![]),
            seen: RefCell::new(BTreeMap::new()),
            calls: RefCell::new(vec![]),
            queue: RefCell::new(vec![]),
            iter_receivers: RefCell::new(vec![]),
        }));
        for (id, rows) in row["recordInputs"].as_object().unwrap() {
            h.0.props.borrow_mut().insert(
                id.clone(),
                rows.as_array()
                    .unwrap()
                    .iter()
                    .map(|e| Entry {
                        key: units(&e["key"]),
                        value: h.decode(&e["value"]),
                        enumerable: e["enumerable"].as_bool().unwrap(),
                    })
                    .collect(),
            );
        }
        for (id, values) in row["arrayInputs"].as_object().unwrap() {
            h.0.arrays.borrow_mut().insert(
                id.clone(),
                values
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| h.decode(v))
                    .collect(),
            );
        }
        h.assert_output(run(&h, method), &row["output"], &label);
        if h.boolean("fire") && !h.0.queue.borrow().is_empty() {
            if row["input"]["fireDisposed"]["kind"] == "boolean" {
                h.write("Owner", &u("isDisposed"), h.value("fireDisposed"));
            }
            let callback = h.0.queue.borrow()[0].callback.clone();
            h.assert_output(
                callback().map(|_| AccountValue::Undefined),
                &row["callbackOutput"],
                &format!("{label} callback"),
            );
        } else {
            assert!(
                row["callbackOutput"].is_null(),
                "{label} source no callback floor"
            );
        }
        for (id, expected) in row["finalProps"].as_object().unwrap() {
            let props = h.0.props.borrow();
            let actual = &props[id];
            let expected = expected.as_array().unwrap();
            for e in expected {
                let key = units(&e["key"]);
                let a = actual
                    .iter()
                    .find(|v| v.key == key)
                    .unwrap_or_else(|| panic!("{label} {id} native missing key"));
                h.assert_value(
                    &a.value,
                    &e["value"],
                    &format!("{label} {id} native entry value"),
                );
                assert_eq!(
                    a.enumerable,
                    e["enumerable"].as_bool().unwrap(),
                    "{label} {id} native enumerable"
                );
            }
            assert_eq!(
                actual.len(),
                expected.len(),
                "{label} {id} independent source size"
            );
            assert_eq!(
                actual.iter().map(|v| v.key.clone()).collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|v| units(&v["key"]))
                    .collect::<Vec<_>>(),
                "{label} {id} native UTF16 key order"
            );
        }
        let literals = h.0.literals.borrow();
        let source_literals = row["literals"].as_array().unwrap();
        assert_eq!(
            literals.len(),
            source_literals.len(),
            "{label} independent source literal count"
        );
        for (j, (a, e)) in literals.iter().zip(source_literals).enumerate() {
            let keys: Vec<&str> = a.fields.iter().map(|v| v.0.as_str()).collect();
            let authored = if method == "handleTickersEvent" {
                vec!["data"]
            } else {
                vec!["type", "channel"]
            };
            assert_eq!(keys, authored, "{label} literal{j} native key order/count");
            h.assert_fields(&a.fields, e, &format!("{label} literal{j}"));
            for b in literals.iter().take(j) {
                assert!(!Rc::ptr_eq(a, b), "{label} native fresh literal identity");
            }
        }
        drop(literals);
        let calls = h.0.calls.borrow();
        let expected = row["calls"].as_array().unwrap();
        assert_eq!(
            calls.len(),
            expected.len(),
            "{label} independent source call count"
        );
        for (j, (a, e)) in calls.iter().zip(expected).enumerate() {
            assert_eq!(
                a.name,
                e["name"].as_str().unwrap(),
                "{label} call{j} native callee"
            );
            h.assert_value(
                &a.receiver,
                &e["receiver"],
                &format!("{label} call{j} receiver"),
            );
            let arity = match a.name.as_str() {
                "resolveTransportRouting"
                | "Socket0.addEventListener"
                | "Socket1.addEventListener"
                | "Owner.armTickerRestFallbackWatchdog"
                | "Parent.log" => 2,
                "Owner.fetchAndEmitOrderBookFromRest" => 4,
                "Owner.createWebsocket"
                | "Owner.send"
                | "Owner.handleTickerEvent"
                | "Owner.isOrderBookRestFallbackCoolingDown"
                | "Owner.stopOrderBookRestFallback"
                | "String.includes"
                | "String"
                | "Array.isArray"
                | "JSON.stringify"
                | "Object.keys"
                | "setInterval" => 1,
                _ => 0,
            };
            assert_eq!(a.args.len(), arity, "{label} call{j} authored native arity");
            assert_eq!(
                e["args"].as_array().unwrap().len(),
                arity,
                "independent source arity"
            );
            for (k, (v, x)) in a.args.iter().zip(e["args"].as_array().unwrap()).enumerate() {
                let argument_label = format!("{label} call{j} argument{k}");
                if a.name == "Owner.send" {
                    forwarding_checks.push((v.clone(), x.clone(), argument_label));
                } else {
                    h.assert_value(v, x, &argument_label);
                }
            }
        }
        drop(calls);
        let receivers = h.0.iter_receivers.borrow();
        let source_receivers = row["iterReceivers"].as_array().unwrap();
        assert_eq!(
            receivers.len(),
            source_receivers.len(),
            "{label} independent source iterator count"
        );
        for (j, (a, e)) in receivers.iter().zip(source_receivers).enumerate() {
            h.assert_value(a, e, &format!("{label} iterator{j} receiver"));
        }
        drop(receivers);
        let queue = h.0.queue.borrow();
        let expected = row["queue"].as_array().unwrap();
        assert_eq!(
            queue.len(),
            expected.len(),
            "{label} independent source timer count"
        );
        for (j, (a, e)) in queue.iter().zip(expected).enumerate() {
            h.assert_value(
                &AccountValue::Number(a.delay),
                &e["delay"],
                &format!("{label} timer{j} delay"),
            );
        }
        drop(queue);
        assert_eq!(
            *h.0.trace.borrow(),
            row["trace"]
                .as_array()
                .unwrap()
                .iter()
                .map(units)
                .collect::<Vec<_>>(),
            "{label} native UTF16 getter iterator call write order"
        );
        h.0.queue.borrow_mut().clear();
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("isWsProxyRouteEnabled", 46),
            ("connectAndSubscribe", 47),
            ("onMarketsChanged", 19),
            ("sendPayload", 5),
            ("syncBboTickerTopics", 15),
            ("handleTickersEvent", 14),
            ("isOrderBookRestFallbackCoolingDown", 30),
            ("startOrderBookRestFallback", 33)
        ]),
        "independent authored method floors"
    );
    // Serializer is a required host, returning a fixed wire sentinel. Every native
    // assertion for every case precedes these forwarding checks; no codec proof.
    for (actual, expected, label) in forwarding_checks {
        let AccountValue::String(actual) = actual else {
            panic!("{label} forwarding kind");
        };
        assert_eq!(
            actual,
            units(&expected["units"]),
            "{label} serializer-return forwarding UTF16"
        );
    }
    assert_eq!(
        corpus["contentHash"], "6e8b8b88f38b02b6eeddc7246bdb11b26da0ee5098bc141fd2cadac2dddb2723",
        "full source-only corpus pin after native assertions"
    );
}
