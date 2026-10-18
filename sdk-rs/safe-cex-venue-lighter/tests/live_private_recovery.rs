use safe_cex_venue_lighter::{
    sdk_account_api::{AccountObject, AccountValue},
    transaction_signing::{HostThrownValue, SigningResult, TransactionSigningError},
    ws_private_recovery::*,
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
struct Marker;
impl AccountObject for Marker {
    fn get(&self, _: &str) -> SigningResult<AccountValue> {
        Ok(AccountValue::Undefined)
    }
}
struct NativeCall {
    name: String,
    args: Vec<AccountValue>,
}
struct Timer {
    delay: f64,
    callback: RecoveryCallback,
}
#[derive(Clone)]
struct Fixture {
    input: Value,
    state: Rc<RefCell<BTreeMap<String, AccountValue>>>,
    trace: Rc<RefCell<Vec<String>>>,
    calls: Rc<RefCell<Vec<NativeCall>>>,
    queue: Rc<RefCell<Vec<Timer>>>,
    objects: BTreeMap<String, Rc<dyn AccountObject>>,
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
    fn invoke(&self, name: &str, args: &[AccountValue]) -> SigningResult<()> {
        self.event(format!("call:{name}"))?;
        self.calls.borrow_mut().push(NativeCall {
            name: name.into(),
            args: args.to_vec(),
        });
        if self.input_bool("implementHelpers") {
            match name {
                "stopDiagnosticsLoop" => return stop_diagnostics_loop(self),
                "clearStreamRecoveryReconnectTimeout" => {
                    return clear_stream_recovery_reconnect_timeout(self);
                }
                "clearRateLimitResubscribeTimeout" => {
                    return clear_rate_limit_resubscribe_timeout(self);
                }
                _ => {}
            }
        }
        if name == "recordMessageHandlerStats" && self.input["statsCount"]["kind"] == "number" {
            self.state.borrow_mut().insert(
                "accountStreamMessageCount".into(),
                self.decode(&self.input["statsCount"]),
            );
        }
        if name == "clearPrivateStreamRateLimitErrorState" {
            let mut state = self.state.borrow_mut();
            state.insert(
                "privateStreamRateLimitFirstSeenAt".into(),
                AccountValue::Number(0.0),
            );
            state.insert(
                "privateStreamRateLimitErrorCount".into(),
                AccountValue::Number(0.0),
            );
        }
        if name == "super.dispose" {
            self.state
                .borrow_mut()
                .insert("isDisposed".into(), AccountValue::Boolean(true));
        }
        Ok(())
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
}
struct Captured {
    host: Fixture,
    name: String,
}
impl RecoveryCall for Captured {
    fn call(&self, args: &[AccountValue]) -> SigningResult<()> {
        self.host.invoke(&self.name, args)
    }
}
struct SetTimeout {
    host: Fixture,
}
impl RecoveryTimerCall for SetTimeout {
    fn call(&self, callback: RecoveryCallback, delay: f64) -> SigningResult<AccountValue> {
        self.host.event("call:setTimeout".into())?;
        self.host.calls.borrow_mut().push(NativeCall {
            name: "setTimeout".into(),
            args: vec![AccountValue::Number(delay)],
        });
        self.host.queue.borrow_mut().push(Timer { delay, callback });
        Ok(AccountValue::Object(self.host.objects["T"].clone()))
    }
}
impl PrivateRecoveryHost for Fixture {
    fn boolean(&self, name: &str) -> SigningResult<bool> {
        let value = if name == "isWsRateLimited()" {
            self.event("get:isWsRateLimited".into())?;
            self.event("call:isWsRateLimited".into())?;
            self.read("rateLimited")
        } else {
            self.event(format!("read:{name}"))?;
            self.read(name)
        };
        let AccountValue::Boolean(v) = value else {
            panic!("source typed boolean contract {name}")
        };
        Ok(v)
    }
    fn number(&self, name: &str) -> SigningResult<f64> {
        self.event(format!("read:{name}"))?;
        let AccountValue::Number(v) = self.read(name) else {
            panic!("source typed number contract {name}")
        };
        Ok(v)
    }
    fn set_number(&self, name: &str, value: f64) -> SigningResult<()> {
        self.event(format!("write:{name}"))?;
        self.state
            .borrow_mut()
            .insert(name.into(), AccountValue::Number(value));
        Ok(())
    }
    fn timer(&self, name: &str) -> SigningResult<AccountValue> {
        self.event(format!("read:{name}"))?;
        Ok(self.read(name))
    }
    fn set_timer(&self, name: &str, value: AccountValue) -> SigningResult<()> {
        self.event(format!("write:{name}"))?;
        self.state.borrow_mut().insert(name.into(), value);
        Ok(())
    }
    fn capture(&self, name: &str) -> SigningResult<Rc<dyn RecoveryCall>> {
        if name == "parent.log" {
            self.event("get:parent".into())?;
        }
        self.event(format!("get:{name}"))?;
        Ok(Rc::new(Captured {
            host: self.clone(),
            name: name.into(),
        }))
    }
    fn capture_set_timeout(&self) -> SigningResult<Rc<dyn RecoveryTimerCall>> {
        self.event("get:setTimeout".into())?;
        Ok(Rc::new(SetTimeout { host: self.clone() }))
    }
    fn now(&self) -> SigningResult<f64> {
        self.event("clock.get".into())?;
        self.event("clock.call".into())?;
        let AccountValue::Number(now) = self.read("now") else {
            unreachable!()
        };
        Ok(now)
    }
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn private_recovery_native_state_timers_callbacks_and_errors() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-recovery-oracle.cjs"
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
        207,
        "independent authored floor, stopped iteration206"
    );
    let mut counts = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        *counts.entry(method).or_insert(0usize) += 1;
        let label = format!("{method} iteration{i}");
        let host = Fixture {
            input: row["input"].clone(),
            state: Rc::new(RefCell::new(BTreeMap::new())),
            trace: Rc::new(RefCell::new(vec![])),
            calls: Rc::new(RefCell::new(vec![])),
            queue: Rc::new(RefCell::new(vec![])),
            objects: ["A", "B", "T"]
                .into_iter()
                .map(|s| (s.into(), Rc::new(Marker) as Rc<dyn AccountObject>))
                .collect(),
            token: HostThrownValue::new(i),
        };
        for (k, v) in row["input"].as_object().unwrap() {
            if v.get("kind").is_some() {
                host.state.borrow_mut().insert(k.into(), host.decode(v));
            }
        }
        for field in [
            "diagnosticsInterval",
            "streamRecoveryReconnectTimeout",
            "rateLimitResubscribeTimeout",
            "positionsThrottleTimeout",
            "balanceThrottleTimeout",
            "ordersThrottleTimeout",
        ] {
            host.state
                .borrow_mut()
                .insert(field.into(), host.decode(&row["input"]["timer"]));
        }
        let actual = match method {
            "stopDiagnosticsLoop" => stop_diagnostics_loop(&host),
            "clearStreamRecoveryReconnectTimeout" => clear_stream_recovery_reconnect_timeout(&host),
            "clearRateLimitResubscribeTimeout" => clear_rate_limit_resubscribe_timeout(&host),
            "scheduleRateLimitResubscribe" => {
                schedule_rate_limit_resubscribe(Rc::new(host.clone()))
            }
            "scheduleStreamRecoveryReconnectFallback" => {
                schedule_stream_recovery_reconnect_fallback(Rc::new(host.clone()))
            }
            "maybeRecoverPrivateStream" => maybe_recover_private_stream(&host),
            "noteAccountStreamActivity" => note_account_stream_activity(
                &host,
                &units(&host.input["statsTopic"]["units"]),
                &units(&host.input["channel"]["units"]),
            ),
            "dispose" => dispose(&host),
            _ => panic!("unwitnessed method"),
        };
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
            let arity = match a.name.as_str() {
                "parent.log" => 2,
                "setTimeout" | "clearTimeout" | "clearInterval" | "recordMessageHandlerStats" => 1,
                _ => 0,
            };
            assert_eq!(
                a.args.len(),
                arity,
                "{label} call{k} independent native argument floor"
            );
            let args = e["args"].as_array().unwrap();
            assert_eq!(args.len(), arity, "source argument floor");
            for j in 0..arity {
                host.assert_value(
                    &a.args[j],
                    &args[j],
                    &format!("{label} call{k} argument{j}"),
                );
            }
        }
        let queue = host.queue.borrow();
        let expected_queue = row["queue"].as_array().unwrap();
        assert_eq!(
            queue.len(),
            expected_queue.len(),
            "{label} independent source scheduled count"
        );
        for (k, (a, e)) in queue.iter().zip(expected_queue).enumerate() {
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
            ("stopDiagnosticsLoop", 17),
            ("clearStreamRecoveryReconnectTimeout", 17),
            ("clearRateLimitResubscribeTimeout", 17),
            ("scheduleRateLimitResubscribe", 34),
            ("scheduleStreamRecoveryReconnectFallback", 25),
            ("maybeRecoverPrivateStream", 38),
            ("noteAccountStreamActivity", 33),
            ("dispose", 26)
        ]),
        "independent method floors"
    );
    assert_eq!(
        oracle["contentHash"], "a0b32cdfa3eef3c28d8695369946c2e15bd6fbf43e740dbb1c1a7b6b59409001",
        "authored full source-only corpus pin after every native assertion"
    );
}
