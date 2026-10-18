use safe_cex_venue_lighter::{
    sdk_account_api::{AccountObject, AccountValue},
    transaction_signing::{HostThrownValue, SigningResult, TransactionSigningError},
    ws_public_readiness::*,
};
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap, process::Command, rc::Rc};
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
struct Marker {
    trace: Rc<RefCell<Vec<String>>>,
    token: HostThrownValue,
    reject_then: bool,
}
impl AccountObject for Marker {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        if key == "then" && self.reject_then {
            self.trace.borrow_mut().push("read:fetched.then".into());
            return Err(self.token.clone().into());
        }
        Ok(AccountValue::Undefined)
    }
}
struct PingPayload {
    timestamp: f64,
}
impl AccountObject for PingPayload {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        Ok(match key {
            "type" => AccountValue::String(u("ping")),
            "timestamp" => AccountValue::Number(self.timestamp),
            _ => AccountValue::Undefined,
        })
    }
}
struct NativeCall {
    name: String,
    receiver: AccountValue,
    args: Vec<AccountValue>,
}
struct Timer {
    name: String,
    delay: f64,
    callback: ReadinessCallback,
}
#[derive(Clone)]
struct Fixture {
    input: Value,
    state: Rc<RefCell<BTreeMap<String, AccountValue>>>,
    trace: Rc<RefCell<Vec<String>>>,
    calls: Rc<RefCell<Vec<NativeCall>>>,
    queue: Rc<RefCell<Vec<Timer>>>,
    reads: Rc<RefCell<BTreeMap<String, usize>>>,
    objects: BTreeMap<String, Rc<dyn AccountObject>>,
    functions: BTreeMap<String, HostThrownValue>,
    token: HostThrownValue,
}
impl Fixture {
    fn decode(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "string" => AccountValue::String(units(&v["units"])),
            "number" => AccountValue::Number(f64::from_bits(
                u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
            )),
            "object" => AccountValue::Object(self.objects[v["id"].as_str().unwrap()].clone()),
            "function" => AccountValue::Function(self.functions[v["id"].as_str().unwrap()].clone()),
            _ => panic!("unwitnessed carrier"),
        }
    }
    fn input_bool(&self, name: &str) -> bool {
        self.input[name]["value"].as_bool().unwrap_or(false)
    }
    fn event(&self, event: String) -> SigningResult<()> {
        self.trace.borrow_mut().push(event.clone());
        if self.input["throwAt"]["kind"] == "string"
            && u(&event) == units(&self.input["throwAt"]["units"])
        {
            return Err(self.token.clone().into());
        }
        if self.input["mutateAt"]["kind"] == "string"
            && u(&event) == units(&self.input["mutateAt"]["units"])
        {
            let m = &self.input["mutation"];
            self.state
                .borrow_mut()
                .insert(m["name"].as_str().unwrap().into(), self.decode(&m["value"]));
        }
        Ok(())
    }
    fn read(&self, name: &str) -> AccountValue {
        self.state
            .borrow()
            .get(name)
            .cloned()
            .unwrap_or(AccountValue::Undefined)
    }
    fn get(&self, name: &str) -> SigningResult<AccountValue> {
        self.event(format!("read:{name}"))?;
        if let Some(values) = self.input["reads"][name].as_array() {
            let mut reads = self.reads.borrow_mut();
            let count = reads.entry(name.into()).or_default();
            let value = self.decode(&values[(*count).min(values.len() - 1)]);
            *count += 1;
            Ok(value)
        } else {
            Ok(self.read(name))
        }
    }
    fn invoke(
        &self,
        name: &str,
        receiver: &AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        self.event(format!("call:{name}"))?;
        self.calls.borrow_mut().push(NativeCall {
            name: name.into(),
            receiver: receiver.clone(),
            args: args.to_vec(),
        });
        if name == "hasHydratedTickerPrice" {
            return Ok(self.read("hydrated"));
        }
        if name == "isTickerRestFallbackCoolingDown" {
            return Ok(self.read("cooling"));
        }
        if name == "fetchAndApplyTickerFallback" {
            return Ok(if self.input["fetchReturn"].get("kind").is_some() {
                self.decode(&self.input["fetchReturn"])
            } else {
                AccountValue::Undefined
            });
        }
        if self.input_bool("implementHelpers") {
            match name {
                "clearTickerRestFallbackWatchdog" => clear_ticker_rest_fallback_watchdog(self)?,
                "primeTickerRestFallback" => prime_ticker_rest_fallback(self)?,
                "stopTickerRestFallback" => stop_ticker_rest_fallback(self, None)?,
                _ => {}
            }
        }
        Ok(AccountValue::Undefined)
    }
    fn assert_value(&self, actual: &AccountValue, expected: &Value, label: &str) {
        match (actual, expected["kind"].as_str().unwrap()) {
            (AccountValue::Undefined, "undefined") | (AccountValue::Null, "null") => {}
            (AccountValue::Boolean(a), "boolean") => assert_eq!(
                *a,
                expected["value"].as_bool().unwrap(),
                "{label} native boolean"
            ),
            (AccountValue::Number(a), "number") => {
                let AccountValue::Number(e) = self.decode(expected) else {
                    unreachable!()
                };
                if e.is_nan() {
                    assert!(a.is_nan(), "{label} native NaN")
                } else {
                    assert_eq!(a.to_bits(), e.to_bits(), "{label} native number bits")
                }
            }
            (AccountValue::String(a), "string") => {
                assert_eq!(*a, units(&expected["units"]), "{label} native UTF16")
            }
            (AccountValue::Object(a), "object") => assert!(
                Rc::ptr_eq(a, &self.objects[expected["id"].as_str().unwrap()]),
                "{label} native object identity"
            ),
            (AccountValue::Function(a), "function") => assert!(
                a.ptr_eq(&self.functions[expected["id"].as_str().unwrap()]),
                "{label} native function identity"
            ),
            (AccountValue::Object(a), "payload") => {
                self.assert_value(
                    &a.get("type").unwrap(),
                    &expected["type"],
                    &format!("{label} payload.type"),
                );
                self.assert_value(
                    &a.get("timestamp").unwrap(),
                    &expected["timestamp"],
                    &format!("{label} payload.timestamp"),
                );
                assert_eq!(
                    expected["keys"],
                    serde_json::json!(["type", "timestamp"]),
                    "independent source object literal field floor"
                );
            }
            _ => panic!("{label} native kind mismatch"),
        }
    }
    fn assert_output(&self, actual: SigningResult<()>, expected: &str, label: &str) {
        match (actual, expected) {
            (Ok(()), "ok") => {}
            (Err(TransactionSigningError::Thrown(t)), "thrown") => {
                assert!(t.ptr_eq(&self.token), "{label} native thrown identity")
            }
            _ => panic!("{label} native completion mismatch"),
        }
    }
    fn owner_receiver(&self) -> AccountValue {
        AccountValue::Object(self.objects["S"].clone())
    }
}
struct Captured {
    host: Fixture,
    name: String,
    receiver: AccountValue,
}
impl ReadinessCall for Captured {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.host.invoke(&self.name, &self.receiver, args)
    }
}
struct Schedule {
    host: Fixture,
    name: String,
}
impl ReadinessTimerCall for Schedule {
    fn call(&self, callback: ReadinessCallback, delay: f64) -> SigningResult<AccountValue> {
        self.host.event(format!("call:{}", self.name))?;
        self.host.calls.borrow_mut().push(NativeCall {
            name: self.name.clone(),
            receiver: AccountValue::Undefined,
            args: vec![AccountValue::Number(delay)],
        });
        self.host.queue.borrow_mut().push(Timer {
            name: self.name.clone(),
            delay,
            callback,
        });
        Ok(AccountValue::Object(self.host.objects["T"].clone()))
    }
}
impl PublicReadinessHost for Fixture {
    fn flag(&self, name: &str) -> SigningResult<bool> {
        let AccountValue::Boolean(v) = self.get(name)? else {
            panic!("source typed boolean contract")
        };
        Ok(v)
    }
    fn set_flag(&self, name: &str, v: bool) -> SigningResult<()> {
        self.set_value(name, AccountValue::Boolean(v))
    }
    fn value(&self, name: &str) -> SigningResult<AccountValue> {
        self.get(name)
    }
    fn set_value(&self, name: &str, value: AccountValue) -> SigningResult<()> {
        self.event(format!("write:{name}"))?;
        self.state.borrow_mut().insert(name.into(), value);
        Ok(())
    }
    fn capture(&self, name: &str) -> SigningResult<Rc<dyn ReadinessCall>> {
        let (mut captured_name, receiver) = (
            name.to_string(),
            match name {
                "clearTimeout" | "clearInterval" => AccountValue::Undefined,
                "parent.log" => {
                    self.event("get:parent".into())?;
                    AccountValue::Object(self.objects["Parent"].clone())
                }
                "emitter.off" | "emitter.on" => {
                    self.event("get:parent".into())?;
                    self.event("get:emitter".into())?;
                    self.read("emitter")
                }
                _ => self.owner_receiver(),
            },
        );
        if name.starts_with("emitter.") {
            let AccountValue::Object(ref emitter) = receiver else {
                unreachable!()
            };
            let id = if Rc::ptr_eq(emitter, &self.objects["E0"]) {
                "E0"
            } else {
                "E1"
            };
            captured_name = format!("{id}.{}", name.split('.').nth(1).unwrap());
        }
        self.event(format!("get:{captured_name}"))?;
        Ok(Rc::new(Captured {
            host: self.clone(),
            name: captured_name,
            receiver,
        }))
    }
    fn capture_timer(&self, name: &str) -> SigningResult<Rc<dyn ReadinessTimerCall>> {
        self.event(format!("get:{name}"))?;
        Ok(Rc::new(Schedule {
            host: self.clone(),
            name: name.into(),
        }))
    }
    fn performance_now(&self) -> SigningResult<f64> {
        self.event("performance.get".into())?;
        self.event("performance.call".into())?;
        let AccountValue::Number(v) = self.read("performanceNow") else {
            unreachable!()
        };
        Ok(v)
    }
    fn virtual_now(&self) -> SigningResult<f64> {
        self.event("virtual.get".into())?;
        self.event("virtual.call".into())?;
        let AccountValue::Number(v) = self.read("virtualNow") else {
            unreachable!()
        };
        Ok(v)
    }
    fn ping_payload(&self, timestamp: f64) -> SigningResult<AccountValue> {
        Ok(AccountValue::Object(Rc::new(PingPayload { timestamp })))
    }
}
fn optional_number(v: &Value) -> Option<f64> {
    if v["kind"] == "undefined" {
        None
    } else {
        Some(f64::from_bits(
            u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
        ))
    }
}
fn optional_string(v: &Value) -> Option<Vec<u16>> {
    if v["kind"] == "undefined" {
        None
    } else {
        Some(units(&v["units"]))
    }
}
fn run(host: &Fixture, method: &str) -> SigningResult<()> {
    let reason = optional_string(&host.input["reason"]);
    match method {
        "syncTickerRestFallbackToPriceHydration" => {
            sync_ticker_rest_fallback_to_price_hydration(host, reason.as_deref())
        }
        "primeTickerRestFallback" => prime_ticker_rest_fallback(host),
        "clearTickerRestFallbackWatchdog" => clear_ticker_rest_fallback_watchdog(host),
        "armTickerRestFallbackWatchdog" => arm_ticker_rest_fallback_watchdog(
            Rc::new(host.clone()),
            optional_number(&host.input["delay"]),
            reason.as_deref(),
        ),
        "stopTickerRestFallback" => {
            stop_ticker_rest_fallback(host, host.input["logRecovered"]["value"].as_bool())
        }
        "startTickerRestFallback" => {
            start_ticker_rest_fallback(Rc::new(host.clone()), reason.as_deref().unwrap())
        }
        "onOpen" => on_open(host),
        "ping" => ping(host),
        _ => panic!("unwitnessed method"),
    }
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn public_readiness_native_lifecycle_callbacks_clocks_and_identity() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-readiness-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        221,
        "independent authored floor, stopped iteration220"
    );
    let mut counts = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        *counts.entry(method).or_insert(0usize) += 1;
        let label = format!("{method} iteration{i}");
        let trace = Rc::new(RefCell::new(vec![]));
        let token = HostThrownValue::new(i);
        let host = Fixture {
            input: row["input"].clone(),
            state: Rc::new(RefCell::new(BTreeMap::new())),
            trace: trace.clone(),
            calls: Rc::new(RefCell::new(vec![])),
            queue: Rc::new(RefCell::new(vec![])),
            reads: Rc::new(RefCell::new(BTreeMap::new())),
            objects: ["A", "B", "T", "E0", "E1", "S", "Parent"]
                .into_iter()
                .map(|s| {
                    (
                        s.into(),
                        Rc::new(Marker {
                            trace: trace.clone(),
                            token: token.clone(),
                            reject_then: s == "A",
                        }) as Rc<dyn AccountObject>,
                    )
                })
                .collect(),
            functions: ["H0", "H1"]
                .into_iter()
                .map(|s| (s.into(), HostThrownValue::new(s)))
                .collect(),
            token,
        };
        for (k, v) in row["input"].as_object().unwrap() {
            if v.get("kind").is_some() {
                host.state.borrow_mut().insert(k.into(), host.decode(v));
            }
        }
        {
            let mut state = host.state.borrow_mut();
            state.insert(
                "tickerRestFallbackWatchdog".into(),
                host.decode(&host.input["timer"]),
            );
            state.insert(
                "tickerRestFallbackInterval".into(),
                host.decode(&host.input["interval"]),
            );
            state.insert(
                "onMarketsChanged".into(),
                AccountValue::Function(host.functions["H0"].clone()),
            );
            state.insert(
                "emitter".into(),
                AccountValue::Object(host.objects["E0"].clone()),
            );
        }
        let actual = run(&host, method).and_then(|_| {
            if host.input_bool("repeat") {
                run(&host, method)
            } else {
                Ok(())
            }
        });
        host.assert_output(actual, row["output"].as_str().unwrap(), &label);
        if host.input_bool("fire") && !host.queue.borrow().is_empty() {
            if let Some(state) = host.input["fireState"].as_object() {
                for (k, v) in state {
                    host.state.borrow_mut().insert(k.into(), host.decode(v));
                }
            }
            let callback = host.queue.borrow()[0].callback.clone();
            host.assert_output(
                callback(),
                row["callbackOutput"].as_str().unwrap(),
                &format!("{label} callback"),
            );
        } else {
            assert!(
                row["callbackOutput"].is_null(),
                "{label} independent callback not fired"
            );
        }
        for (field, expected) in row["state"].as_object().unwrap() {
            host.assert_value(
                &host.read(field),
                expected,
                &format!("{label} state {field}"),
            );
        }
        let calls = host.calls.borrow();
        let expected_calls = row["calls"].as_array().unwrap();
        assert_eq!(
            calls.len(),
            expected_calls.len(),
            "{label} independent source call count"
        );
        for (k, (a, e)) in calls.iter().zip(expected_calls).enumerate() {
            assert_eq!(
                a.name,
                e["name"].as_str().unwrap(),
                "{label} call{k} native callee"
            );
            host.assert_value(
                &a.receiver,
                &e["receiver"],
                &format!("{label} call{k} receiver"),
            );
            let arity = match a.name.as_str() {
                "parent.log" | "E0.off" | "E0.on" | "E1.off" | "E1.on" => 2,
                "setTimeout"
                | "setInterval"
                | "clearTimeout"
                | "clearInterval"
                | "startTickerRestFallback"
                | "sendPayload" => 1,
                _ => 0,
            };
            assert_eq!(a.args.len(), arity, "{label} independent argument floor");
            let args = e["args"].as_array().unwrap();
            assert_eq!(args.len(), arity, "independent source argument floor");
            for j in 0..arity {
                host.assert_value(
                    &a.args[j],
                    &args[j],
                    &format!("{label} call{k} argument{j}"),
                );
            }
        }
        if method == "ping" && host.input_bool("repeat") {
            assert_eq!(calls.len(), 2, "independent twice-ping native floor");
            let (AccountValue::Object(a), AccountValue::Object(b)) =
                (&calls[0].args[0], &calls[1].args[0])
            else {
                panic!("native payload kinds")
            };
            assert!(
                !Rc::ptr_eq(a, b),
                "{label} native fresh payload identity on second call"
            );
        }
        let queue = host.queue.borrow();
        let expected_queue = row["queue"].as_array().unwrap();
        assert_eq!(
            queue.len(),
            expected_queue.len(),
            "{label} independent source scheduled floor"
        );
        for (k, (a, e)) in queue.iter().zip(expected_queue).enumerate() {
            assert_eq!(
                a.name,
                e["name"].as_str().unwrap(),
                "{label} timer{k} native kind"
            );
            host.assert_value(
                &AccountValue::Number(a.delay),
                &e["delay"],
                &format!("{label} timer{k} delay"),
            );
        }
        let trace: Vec<String> = row["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        assert_eq!(
            *host.trace.borrow(),
            trace,
            "{label} native getter/call/write order"
        );
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("syncTickerRestFallbackToPriceHydration", 13),
            ("primeTickerRestFallback", 12),
            ("clearTickerRestFallbackWatchdog", 17),
            ("armTickerRestFallbackWatchdog", 36),
            ("stopTickerRestFallback", 42),
            ("startTickerRestFallback", 51),
            ("onOpen", 34),
            ("ping", 16)
        ]),
        "independent per-method floors"
    );
    assert_eq!(
        oracle["contentHash"], "d364ae06a488ca6af72ab211e70cfa4b10ee0c4099281d9198c19adb50b8ec49",
        "full source-only corpus pin after every native assertion"
    );
}
