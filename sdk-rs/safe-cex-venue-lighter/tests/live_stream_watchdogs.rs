use safe_cex_num::js_number_string;
use safe_cex_venue_lighter::{
    sdk_account_api::{AccountObject, AccountValue},
    transaction_builders::TransactionBuildError,
    transaction_signing::{HostThrownValue, SigningResult, TransactionSigningError},
    ws_public_ohlcv::{
        should_attempt_lighter_ohlcv_recovery, should_promote_lighter_ohlcv_recovery_to_proxy,
    },
    ws_public_watchdogs::*,
};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    process::Command,
    rc::{Rc, Weak},
};
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn event_key(prefix: &str, key: &[u16]) -> Vec<u16> {
    let mut v = u(prefix);
    v.extend_from_slice(key);
    v
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
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(AccountValue::Undefined))
    }
}
#[derive(Clone)]
struct Entry {
    key: Vec<u16>,
    value: AccountValue,
    enumerable: bool,
}
struct Record {
    id: String,
    entries: RefCell<Vec<Entry>>,
    env: Weak<Env>,
}
struct NativeCall {
    name: String,
    receiver: AccountValue,
    args: Vec<AccountValue>,
}
struct Timer {
    delay: f64,
    callback: WatchdogCallback,
}
struct Env {
    input: Value,
    state: RefCell<BTreeMap<String, AccountValue>>,
    trace: RefCell<Vec<Vec<u16>>>,
    calls: RefCell<Vec<NativeCall>>,
    queue: RefCell<Vec<Timer>>,
    literal_keys: RefCell<Vec<Vec<String>>>,
    objects: BTreeMap<String, Rc<dyn AccountObject>>,
    constructor: HostThrownValue,
    token: HostThrownValue,
    alternate: HostThrownValue,
    current: RefCell<BTreeMap<String, Rc<Record>>>,
    initial: RefCell<BTreeMap<String, Rc<Record>>>,
    replacements: RefCell<BTreeMap<String, Rc<Record>>>,
}
#[derive(Clone)]
struct Fixture(Rc<Env>);
fn field(kind: WatchdogRecord) -> &'static str {
    match kind {
        WatchdogRecord::Topics => "topics",
        WatchdogRecord::Handlers => "messageHandlers",
        WatchdogRecord::BookStale => "orderBookStaleTimeouts",
        WatchdogRecord::CandleNoData => "ohlcvNoDataTimeouts",
        WatchdogRecord::BookAttempts => "orderBookRecoveryAttempts",
        WatchdogRecord::BookRecovery => "lastOrderBookRecoveryAtByTopic",
    }
}
impl Fixture {
    fn decode(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "number" => AccountValue::Number(f64::from_bits(
                u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
            )),
            "string" => AccountValue::String(units(&v["units"])),
            "object" => AccountValue::Object(self.0.objects[v["id"].as_str().unwrap()].clone()),
            "function" => AccountValue::Function(self.0.constructor.clone()),
            _ => panic!("unwitnessed carrier"),
        }
    }
    fn input_bool(&self, k: &str) -> bool {
        self.0.input[k]["value"].as_bool().unwrap_or(false)
    }
    fn event(&self, event: Vec<u16>) -> SigningResult<()> {
        self.0.trace.borrow_mut().push(event.clone());
        if self.0.input["throwAt"]["kind"] == "string"
            && event == units(&self.0.input["throwAt"]["units"])
        {
            return Err(self.0.token.clone().into());
        }
        if self.0.input["replaceAt"]["kind"] == "string"
            && event == units(&self.0.input["replaceAt"]["units"])
        {
            let key = String::from_utf16(&units(&self.0.input["replaceKind"]["units"])).unwrap();
            self.0
                .current
                .borrow_mut()
                .insert(key.clone(), self.0.replacements.borrow()[&key].clone());
        }
        Ok(())
    }
    fn read(&self, k: &str) -> AccountValue {
        self.0
            .state
            .borrow()
            .get(k)
            .cloned()
            .unwrap_or(AccountValue::Undefined)
    }
    fn input_number(&self, k: &str) -> f64 {
        let AccountValue::Number(v) = self.decode(&self.0.input[k]) else {
            unreachable!()
        };
        v
    }
    fn owner(&self) -> AccountValue {
        AccountValue::Object(self.0.objects["S"].clone())
    }
    fn invoke(
        &self,
        name: &str,
        receiver: &AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        self.event(u(&format!("call:{name}")))?;
        self.0.calls.borrow_mut().push(NativeCall {
            name: name.into(),
            receiver: receiver.clone(),
            args: args.to_vec(),
        });
        match name {
            "isOrderBookTopic" => {
                let AccountValue::String(topic) = &args[0] else {
                    unreachable!()
                };
                Ok(AccountValue::Boolean(is_order_book_topic(topic)))
            }
            "isWsProxyRouteEnabled" => Ok(self.decode(&self.0.input["proxyEnabled"])),
            "Boolean" => Ok(AccountValue::Boolean(args[0].truthy())),
            "shouldAttemptLighterOhlcvRecovery" => {
                let AccountValue::Object(p) = &args[0] else {
                    unreachable!()
                };
                let has = p.get("hasTopicHandlers")?.truthy();
                let AccountValue::Number(now) = p.get("nowMs")? else {
                    unreachable!()
                };
                let AccountValue::Number(last) = p.get("lastRecoveryReconnectAtMs")? else {
                    unreachable!()
                };
                let AccountValue::Number(cooldown) = p.get("recoveryCooldownMs")? else {
                    unreachable!()
                };
                Ok(AccountValue::Boolean(
                    should_attempt_lighter_ohlcv_recovery(has, now, last, cooldown),
                ))
            }
            "shouldPromoteLighterOhlcvRecoveryToProxy" => {
                let AccountValue::Object(p) = &args[0] else {
                    unreachable!()
                };
                Ok(AccountValue::Boolean(
                    should_promote_lighter_ohlcv_recovery_to_proxy(
                        p.get("isWsProxyRouteEnabled")?.truthy(),
                        p.get("useProxyRoute")?.truthy(),
                        p.get("connectedViaProxyRoute")?.truthy(),
                    ),
                ))
            }
            "resolveLighterInterval" => {
                let AccountValue::String(interval) = &args[0] else {
                    unreachable!()
                };
                if ["1m", "5m", "15m", "30m", "1h", "4h", "12h", "1d"]
                    .into_iter()
                    .any(|s| u(s) == *interval)
                {
                    Ok(AccountValue::String(interval.clone()))
                } else {
                    let mut message = u("Lighter does not support raw ");
                    message.extend_from_slice(interval);
                    message.extend(u(
                        " candles. Supported resolutions: 1m, 5m, 15m, 30m, 1h, 4h, 12h, 1d.",
                    ));
                    Err(TransactionSigningError::Generated(TransactionBuildError {
                        name: "Error",
                        message,
                    }))
                }
            }
            "buildLighterCandleTopic" => {
                let mut result = u("candle/");
                result.extend(match &args[0] {
                    AccountValue::String(v) => v.clone(),
                    AccountValue::Number(v) => u(&js_number_string(*v)),
                    _ => panic!("typed market id"),
                });
                result.push(47);
                let AccountValue::String(resolution) = &args[1] else {
                    unreachable!()
                };
                result.extend_from_slice(resolution);
                Ok(AccountValue::String(result))
            }
            "super.dispose" => {
                self.0
                    .state
                    .borrow_mut()
                    .insert("isDisposed".into(), AccountValue::Boolean(true));
                Ok(AccountValue::Undefined)
            }
            _ => Ok(AccountValue::Undefined),
        }
    }
    fn assert_value(&self, a: &AccountValue, e: &Value, label: &str) {
        match (a, e["kind"].as_str().unwrap()) {
            (AccountValue::Undefined, "undefined") | (AccountValue::Null, "null") => {}
            (AccountValue::Boolean(v), "boolean") => {
                assert_eq!(*v, e["value"].as_bool().unwrap(), "{label} native boolean")
            }
            (AccountValue::Number(v), "number") => {
                let AccountValue::Number(x) = self.decode(e) else {
                    unreachable!()
                };
                if x.is_nan() {
                    assert!(v.is_nan(), "{label} native NaN")
                } else {
                    assert_eq!(v.to_bits(), x.to_bits(), "{label} native number bits")
                }
            }
            (AccountValue::String(v), "string") => {
                assert_eq!(*v, units(&e["units"]), "{label} native UTF16")
            }
            (AccountValue::Object(v), "object") => assert!(
                Rc::ptr_eq(v, &self.0.objects[e["id"].as_str().unwrap()]),
                "{label} native object identity"
            ),
            (AccountValue::Function(v), "function") => assert!(
                v.ptr_eq(&self.0.constructor),
                "{label} native function identity"
            ),
            (AccountValue::Object(v), "params") => {
                for (k, x) in e["fields"].as_object().unwrap() {
                    self.assert_value(&v.get(k).unwrap(), x, &format!("{label} params.{k}"));
                }
            }
            _ => panic!("{label} native kind mismatch"),
        }
    }
    fn assert_output(&self, a: SigningResult<AccountValue>, e: &Value, label: &str) {
        match (a, e["kind"].as_str().unwrap()) {
            (Ok(v), "ok") => self.assert_value(&v, &e["value"], &format!("{label} result")),
            (Err(TransactionSigningError::Thrown(v)), "thrown") => {
                assert!(v.ptr_eq(&self.0.token), "{label} native thrown identity")
            }
            (Err(TransactionSigningError::Thrown(v)), "alternate") => assert!(
                v.ptr_eq(&self.0.alternate),
                "{label} alternate native thrown identity"
            ),
            (Err(TransactionSigningError::Generated(v)), "generated") => {
                assert_eq!(
                    v.name,
                    e["name"].as_str().unwrap(),
                    "{label} native generated name"
                );
                assert_eq!(
                    v.message,
                    units(&e["message"]),
                    "{label} native generated UTF16"
                );
            }
            _ => panic!("{label} native completion mismatch"),
        }
    }
    fn assert_record(&self, r: &Record, e: &Value, label: &str) {
        assert_eq!(
            r.id,
            e["id"].as_str().unwrap(),
            "{label} native record alias"
        );
        let entries = r.entries.borrow();
        let expected = e["entries"].as_array().unwrap();
        for row in expected {
            let key = units(&row["key"]);
            let entry = entries
                .iter()
                .find(|x| x.key == key)
                .unwrap_or_else(|| panic!("{label} native missing UTF16 key"));
            self.assert_value(&entry.value, &row["value"], &format!("{label} entry value"));
            assert_eq!(
                entry.enumerable,
                row["enumerable"].as_bool().unwrap(),
                "{label} native enumerable"
            );
        }
        assert_eq!(
            entries.len(),
            expected.len(),
            "{label} independent source record size"
        );
        let keys: Vec<Vec<u16>> = entries.iter().map(|x| x.key.clone()).collect();
        let expected_keys: Vec<Vec<u16>> = expected.iter().map(|x| units(&x["key"])).collect();
        assert_eq!(keys, expected_keys, "{label} native UTF16 key order");
    }
    fn iterator_failure(&self, name: &str) -> bool {
        self.0.input["iteratorFailure"]["kind"] == "string"
            && units(&self.0.input["iteratorFailure"]["units"]) == u(name)
    }
    fn close_on_error(&self, error: TransactionSigningError) -> SigningResult<()> {
        self.0.trace.borrow_mut().push(u("return.get"));
        if !self.iterator_failure("closeGet") {
            self.0.trace.borrow_mut().push(u("return.call"));
        }
        Err(error)
    }
    fn for_of(
        &self,
        values: Vec<AccountValue>,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        self.event(u("iterator.get"))?;
        self.event(u("iterator.call"))?;
        self.event(u("next.get"))?;
        for value in values {
            self.event(u("next.call"))?;
            if self.iterator_failure("next") {
                return Err(self.0.token.clone().into());
            }
            self.event(u("done.get"))?;
            self.event(u("value.get"))?;
            if let Err(e) = visit(value) {
                return self.close_on_error(e);
            }
        }
        self.event(u("next.call"))?;
        if self.iterator_failure("next") {
            return Err(self.0.token.clone().into());
        }
        self.event(u("done.get"))?;
        Ok(())
    }
}
impl WatchdogRecordAccess for Record {
    fn get(&self, key: &[u16]) -> SigningResult<AccountValue> {
        let host = Fixture(self.env.upgrade().unwrap());
        host.event(event_key(&format!("get:{}:", self.id), key))?;
        Ok(self
            .entries
            .borrow()
            .iter()
            .find(|x| x.key == key)
            .map(|x| x.value.clone())
            .unwrap_or_else(|| {
                if key == u("constructor") {
                    AccountValue::Function(host.0.constructor.clone())
                } else {
                    AccountValue::Undefined
                }
            }))
    }
    fn set(&self, key: &[u16], value: AccountValue) -> SigningResult<()> {
        let host = Fixture(self.env.upgrade().unwrap());
        host.event(event_key(&format!("set:{}:", self.id), key))?;
        let mut entries = self.entries.borrow_mut();
        if let Some(e) = entries.iter_mut().find(|e| e.key == key) {
            e.value = value;
        } else {
            entries.push(Entry {
                key: key.to_vec(),
                value,
                enumerable: true,
            });
        }
        Ok(())
    }
    fn delete(&self, key: &[u16]) -> SigningResult<()> {
        let host = Fixture(self.env.upgrade().unwrap());
        host.event(event_key(&format!("delete:{}:", self.id), key))?;
        self.entries.borrow_mut().retain(|e| e.key != key);
        Ok(())
    }
}
impl Record {
    fn enumerate(&self, mode: &str) -> SigningResult<Vec<(Vec<u16>, AccountValue)>> {
        let host = Fixture(self.env.upgrade().unwrap());
        host.event(u(&format!("ownKeys:{}", self.id)))?;
        let keys: Vec<_> = self
            .entries
            .borrow()
            .iter()
            .map(|e| e.key.clone())
            .collect();
        let mut result = vec![];
        for key in keys {
            host.event(event_key(&format!("descriptor:{}:", self.id), &key))?;
            let is_enum = self
                .entries
                .borrow()
                .iter()
                .find(|e| e.key == key)
                .is_some_and(|e| e.enumerable);
            if is_enum {
                let value = if mode == "Object.keys" {
                    AccountValue::String(key.clone())
                } else {
                    self.get(&key)?
                };
                result.push((key, value));
            }
        }
        Ok(result)
    }
}
struct Captured {
    host: Fixture,
    name: String,
    receiver: AccountValue,
}
impl WatchdogCall for Captured {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.host.invoke(&self.name, &self.receiver, args)
    }
}
struct Schedule(Fixture);
impl WatchdogTimerCall for Schedule {
    fn call(&self, callback: WatchdogCallback, delay: f64) -> SigningResult<AccountValue> {
        self.0.event(u("call:setTimeout"))?;
        self.0.0.calls.borrow_mut().push(NativeCall {
            name: "setTimeout".into(),
            receiver: AccountValue::Undefined,
            args: vec![AccountValue::Number(delay)],
        });
        self.0.0.queue.borrow_mut().push(Timer { delay, callback });
        Ok(AccountValue::Object(self.0.0.objects["T"].clone()))
    }
}
struct Enumerate {
    host: Fixture,
    name: String,
}
impl WatchdogValuesCall for Enumerate {
    fn for_of(
        &self,
        record: Rc<dyn WatchdogRecordAccess>,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        self.host.event(u(&format!("call:{}", self.name)))?;
        let r = self
            .host
            .0
            .current
            .borrow()
            .values()
            .chain(self.host.0.initial.borrow().values())
            .chain(self.host.0.replacements.borrow().values())
            .find(|r| Rc::ptr_eq(&(Rc::clone(*r) as Rc<dyn WatchdogRecordAccess>), &record))
            .cloned()
            .unwrap();
        let values = r
            .enumerate(&self.name)?
            .into_iter()
            .map(|(_, v)| v)
            .collect();
        self.host.for_of(values, visit)
    }
}
impl WatchdogEntriesCall for Enumerate {
    fn for_of(
        &self,
        record: Rc<dyn WatchdogRecordAccess>,
        visit: &mut dyn FnMut(Vec<u16>, AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        self.host.event(u("call:Object.entries"))?;
        let r = self
            .host
            .0
            .current
            .borrow()
            .values()
            .chain(self.host.0.initial.borrow().values())
            .chain(self.host.0.replacements.borrow().values())
            .find(|r| Rc::ptr_eq(&(Rc::clone(*r) as Rc<dyn WatchdogRecordAccess>), &record))
            .cloned()
            .unwrap();
        let entries = r.enumerate("Object.entries")?;
        self.host.event(u("iterator.get"))?;
        self.host.event(u("iterator.call"))?;
        self.host.event(u("next.get"))?;
        for (key, value) in entries {
            self.host.event(u("next.call"))?;
            if self.host.iterator_failure("next") {
                return Err(self.host.0.token.clone().into());
            }
            self.host.event(u("done.get"))?;
            self.host.event(u("value.get"))?;
            if let Err(e) = visit(key, value) {
                return self.host.close_on_error(e);
            }
        }
        self.host.event(u("next.call"))?;
        if self.host.iterator_failure("next") {
            return Err(self.host.0.token.clone().into());
        }
        self.host.event(u("done.get"))?;
        Ok(())
    }
}
impl PublicWatchdogHost for Fixture {
    fn record(&self, kind: WatchdogRecord) -> SigningResult<Rc<dyn WatchdogRecordAccess>> {
        let k = field(kind);
        self.event(u(&format!("owner.get:{k}")))?;
        Ok(self.0.current.borrow()[k].clone())
    }
    fn replace_empty(&self, kind: WatchdogRecord) -> SigningResult<()> {
        let k = field(kind);
        self.event(u(&format!("owner.set:{k}")))?;
        self.0.current.borrow_mut().insert(
            k.into(),
            Rc::new(Record {
                id: format!("{k}/empty"),
                entries: RefCell::new(vec![]),
                env: Rc::downgrade(&self.0),
            }),
        );
        Ok(())
    }
    fn flag(&self, name: &str) -> SigningResult<bool> {
        self.event(u(&format!("read:{name}")))?;
        let AccountValue::Boolean(v) = self.read(name) else {
            unreachable!()
        };
        Ok(v)
    }
    fn set_flag(&self, name: &str, v: bool) -> SigningResult<()> {
        self.event(u(&format!("write:{name}")))?;
        self.0
            .state
            .borrow_mut()
            .insert(name.into(), AccountValue::Boolean(v));
        Ok(())
    }
    fn number(&self, name: &str) -> SigningResult<f64> {
        self.event(u(&format!("read:{name}")))?;
        let AccountValue::Number(v) = self.read(name) else {
            unreachable!()
        };
        Ok(v)
    }
    fn set_number(&self, name: &str, v: f64) -> SigningResult<()> {
        self.event(u(&format!("write:{name}")))?;
        self.0
            .state
            .borrow_mut()
            .insert(name.into(), AccountValue::Number(v));
        Ok(())
    }
    fn capture(&self, name: &str) -> SigningResult<Rc<dyn WatchdogCall>> {
        let receiver = match name {
            "parent.log" => {
                self.event(u("get:parent"))?;
                AccountValue::Object(self.0.objects["Parent"].clone())
            }
            "clearTimeout"
            | "Boolean"
            | "buildLighterCandleTopic"
            | "resolveLighterInterval"
            | "shouldAttemptLighterOhlcvRecovery"
            | "shouldPromoteLighterOhlcvRecoveryToProxy" => AccountValue::Undefined,
            _ => self.owner(),
        };
        self.event(u(&format!("get:{name}")))?;
        Ok(Rc::new(Captured {
            host: self.clone(),
            name: name.into(),
            receiver,
        }))
    }
    fn capture_timeout(&self) -> SigningResult<Rc<dyn WatchdogTimerCall>> {
        self.event(u("get:setTimeout"))?;
        Ok(Rc::new(Schedule(self.clone())))
    }
    fn capture_values(&self, name: &str) -> SigningResult<Rc<dyn WatchdogValuesCall>> {
        self.event(u(&format!("get:{name}")))?;
        Ok(Rc::new(Enumerate {
            host: self.clone(),
            name: name.into(),
        }))
    }
    fn capture_entries(&self) -> SigningResult<Rc<dyn WatchdogEntriesCall>> {
        self.event(u("get:Object.entries"))?;
        Ok(Rc::new(Enumerate {
            host: self.clone(),
            name: "Object.entries".into(),
        }))
    }
    fn array_first(&self, value: &AccountValue) -> SigningResult<Option<AccountValue>> {
        if let AccountValue::Object(o) = value {
            if Rc::ptr_eq(o, &self.0.objects["Array"]) {
                return Ok(Some(AccountValue::String(vec![
                    102, 105, 114, 115, 116, 47, 55296,
                ])));
            }
            if Rc::ptr_eq(o, &self.0.objects["Empty"]) || Rc::ptr_eq(o, &self.0.objects["Hole"]) {
                return Ok(Some(AccountValue::Undefined));
            }
        }
        Ok(None)
    }
    fn property(&self, value: &AccountValue, name: &str) -> SigningResult<AccountValue> {
        match value {
            AccountValue::Object(o) => {
                if Rc::ptr_eq(o, &self.0.objects["Market"]) {
                    self.event(u("market.id"))?;
                    return Ok(self.decode(&self.0.input["id"]));
                }
                if name == "length" {
                    for (id, len) in [
                        ("Array", 2.0),
                        ("Empty", 0.0),
                        ("Hole", 2.0),
                        ("Handlers", 1.0),
                    ] {
                        if Rc::ptr_eq(o, &self.0.objects[id]) {
                            return Ok(AccountValue::Number(len));
                        }
                    }
                }
                o.get(name)
            }
            AccountValue::String(s) if name == "length" => Ok(AccountValue::Number(s.len() as f64)),
            AccountValue::Function(_) if name == "length" => Ok(AccountValue::Number(1.0)),
            _ => Ok(AccountValue::Undefined),
        }
    }
    fn now(&self) -> SigningResult<f64> {
        self.event(u("clock.get"))?;
        self.event(u("clock.call"))?;
        Ok(self.input_number("now"))
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        let literal = Literal {
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        };
        self.0
            .literal_keys
            .borrow_mut()
            .push(literal.fields.iter().map(|(k, _)| k.clone()).collect());
        Ok(AccountValue::Object(Rc::new(literal)))
    }
    fn markets_by_symbol(&self) -> SigningResult<Rc<dyn WatchdogRecordAccess>> {
        for e in ["get:parent", "get:store", "get:markets"] {
            self.event(u(e))?;
        }
        Ok(self.0.current.borrow()["markets"].clone())
    }
}
impl WatchdogOptions for Fixture {
    fn symbol(&self) -> SigningResult<Vec<u16>> {
        self.event(u("opts.symbol"))?;
        Ok(units(&self.0.input["symbol"]["units"]))
    }
    fn interval(&self) -> SigningResult<AccountValue> {
        self.event(u("opts.interval"))?;
        Ok(self.decode(&self.0.input["interval"]))
    }
}
fn run(host: &Fixture, method: &str) -> SigningResult<AccountValue> {
    let topic = units(&host.0.input["topic"]["units"]);
    let reason = if host.0.input["reason"]["kind"] == "undefined" {
        None
    } else {
        Some(units(&host.0.input["reason"]["units"]))
    };
    let result = match method {
        "armOrderBookStaleTimeout" => arm_order_book_stale_timeout(
            Rc::new(host.clone()),
            &topic,
            host.decode(&host.0.input["channel"]),
        ),
        "isOrderBookTopic" => return Ok(AccountValue::Boolean(is_order_book_topic(&topic))),
        "armExistingOrderBookTimeouts" => arm_existing_order_book_timeouts(host),
        "armOhlcvNoDataTimeout" => arm_ohlcv_no_data_timeout(Rc::new(host.clone()), &topic),
        "armExistingOhlcvNoDataTimeouts" => arm_existing_ohlcv_no_data_timeouts(host),
        "attemptOhlcvRecovery" => attempt_ohlcv_recovery(host, &topic, reason.as_deref().unwrap()),
        "maybeRecoverOhlcvStream" => maybe_recover_ohlcv_stream(host, host, reason.as_deref()),
        "dispose" => dispose(host),
        _ => panic!("unwitnessed method"),
    };
    result.map(|_| AccountValue::Undefined)
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn stream_watchdogs_native_records_callbacks_recovery_and_disposal() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/stream-watchdogs-oracle.cjs"
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
        173,
        "independent authored floor, stopped iteration172"
    );
    let mut counts = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        *counts.entry(method).or_insert(0usize) += 1;
        let label = format!("{method} iteration{i}");
        let host = Fixture(Rc::new(Env {
            input: row["input"].clone(),
            state: RefCell::new(BTreeMap::new()),
            trace: RefCell::new(vec![]),
            calls: RefCell::new(vec![]),
            queue: RefCell::new(vec![]),
            literal_keys: RefCell::new(vec![]),
            objects: [
                "A", "B", "T", "Array", "Empty", "Hole", "Handlers", "Market", "S", "Parent",
                "Object",
            ]
            .into_iter()
            .map(|s| (s.into(), Rc::new(Marker) as Rc<dyn AccountObject>))
            .collect(),
            constructor: HostThrownValue::new("constructor"),
            token: HostThrownValue::new(i),
            alternate: HostThrownValue::new((i, "alternate")),
            current: RefCell::new(BTreeMap::new()),
            initial: RefCell::new(BTreeMap::new()),
            replacements: RefCell::new(BTreeMap::new()),
        }));
        for (k, v) in row["state"].as_object().unwrap() {
            host.0
                .state
                .borrow_mut()
                .insert(k.into(), host.decode(&row["input"][k]));
            let _ = v;
        }
        for (k, entries) in row["recordInputs"].as_object().unwrap() {
            let record = Rc::new(Record {
                id: format!("{k}/old"),
                entries: RefCell::new(
                    entries
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|e| Entry {
                            key: units(&e["key"]),
                            value: host.decode(&e["value"]),
                            enumerable: e["enumerable"].as_bool().unwrap(),
                        })
                        .collect(),
                ),
                env: Rc::downgrade(&host.0),
            });
            host.0.current.borrow_mut().insert(k.into(), record.clone());
            host.0.initial.borrow_mut().insert(k.into(), record);
            host.0.replacements.borrow_mut().insert(
                k.into(),
                Rc::new(Record {
                    id: format!("{k}/new"),
                    entries: RefCell::new(vec![Entry {
                        key: units(&host.0.input["topic"]["units"]),
                        value: AccountValue::Object(host.0.objects["B"].clone()),
                        enumerable: true,
                    }]),
                    env: Rc::downgrade(&host.0),
                }),
            );
        }
        host.0.current.borrow_mut().insert(
            "markets".into(),
            Rc::new(Record {
                id: "markets/old".into(),
                entries: RefCell::new(vec![Entry {
                    key: units(&host.0.input["symbol"]["units"]),
                    value: host.decode(&host.0.input["market"]),
                    enumerable: true,
                }]),
                env: Rc::downgrade(&host.0),
            }),
        );
        host.assert_output(run(&host, method), &row["output"], &label);
        if host.input_bool("fire") && !host.0.queue.borrow().is_empty() {
            if let Some(state) = host.0.input["fireState"].as_object() {
                for (k, v) in state {
                    host.0.state.borrow_mut().insert(k.into(), host.decode(v));
                }
            }
            let callback = host.0.queue.borrow()[0].callback.clone();
            host.assert_output(
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
        for (k, e) in row["state"].as_object().unwrap() {
            host.assert_value(&host.read(k), e, &format!("{label} state {k}"));
        }
        for (k, e) in row["current"].as_object().unwrap() {
            host.assert_record(
                &host.0.current.borrow()[k],
                e,
                &format!("{label} current {k}"),
            );
        }
        for (k, e) in row["initial"].as_object().unwrap() {
            host.assert_record(
                &host.0.initial.borrow()[k],
                e,
                &format!("{label} initial {k}"),
            );
        }
        let literals = host.0.literal_keys.borrow();
        let source_literals = row["literalKeys"].as_array().unwrap();
        assert_eq!(
            literals.len(),
            source_literals.len(),
            "{label} independent source literal count"
        );
        for (j, keys) in literals.iter().enumerate() {
            let expected: Vec<String> = if j == 0 {
                [
                    "hasTopicHandlers",
                    "nowMs",
                    "lastRecoveryReconnectAtMs",
                    "recoveryCooldownMs",
                ]
                .into_iter()
                .map(String::from)
                .collect()
            } else {
                [
                    "isWsProxyRouteEnabled",
                    "useProxyRoute",
                    "connectedViaProxyRoute",
                ]
                .into_iter()
                .map(String::from)
                .collect()
            };
            assert_eq!(*keys, expected, "{label} literal{j} native key order/count");
            let source_keys: Vec<String> = source_literals[j]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();
            assert_eq!(
                source_keys, expected,
                "independent authored source literal schema"
            );
        }
        drop(literals);
        let calls = host.0.calls.borrow();
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
            host.assert_value(
                &a.receiver,
                &e["receiver"],
                &format!("{label} call{j} receiver"),
            );
            let arity = match a.name.as_str() {
                "parent.log"
                | "buildLighterCandleTopic"
                | "attemptOhlcvRecovery"
                | "armOrderBookStaleTimeout" => 2,
                "attemptOrderBookRecovery" => 3,
                "setTimeout"
                | "clearTimeout"
                | "clearOrderBookStaleTimeout"
                | "clearOhlcvNoDataTimeout"
                | "isOrderBookTopic"
                | "armOhlcvNoDataTimeout"
                | "shouldAttemptLighterOhlcvRecovery"
                | "shouldPromoteLighterOhlcvRecoveryToProxy"
                | "Boolean"
                | "resolveLighterInterval" => 1,
                _ => 0,
            };
            assert_eq!(
                a.args.len(),
                arity,
                "{label} independent native argument floor"
            );
            assert_eq!(
                e["args"].as_array().unwrap().len(),
                arity,
                "independent source argument floor"
            );
            for k in 0..arity {
                host.assert_value(
                    &a.args[k],
                    &e["args"][k],
                    &format!("{label} call{j} argument{k}"),
                );
            }
        }
        let queue = host.0.queue.borrow();
        let expected = row["queue"].as_array().unwrap();
        assert_eq!(
            queue.len(),
            expected.len(),
            "{label} independent source scheduled floor"
        );
        for (j, (a, e)) in queue.iter().zip(expected).enumerate() {
            host.assert_value(
                &AccountValue::Number(a.delay),
                &e["delay"],
                &format!("{label} timer{j} delay"),
            );
        }
        let trace: Vec<Vec<u16>> = row["trace"].as_array().unwrap().iter().map(units).collect();
        assert_eq!(
            *host.0.trace.borrow(),
            trace,
            "{label} native UTF16 getter/iterator/call/write order"
        );
        drop(queue);
        drop(calls);
        host.0.queue.borrow_mut().clear();
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("armOrderBookStaleTimeout", 12),
            ("armOhlcvNoDataTimeout", 12),
            ("isOrderBookTopic", 12),
            ("armExistingOrderBookTimeouts", 17),
            ("armExistingOhlcvNoDataTimeouts", 17),
            ("attemptOhlcvRecovery", 55),
            ("maybeRecoverOhlcvStream", 26),
            ("dispose", 22)
        ]),
        "independent method floors"
    );
    assert_eq!(
        oracle["contentHash"], "56f1f65a11b8db02af4e25d08516357a041fccacc4eb92ff8d5b0a1eb235245b",
        "full source-only corpus pin after all native assertions"
    );
}
