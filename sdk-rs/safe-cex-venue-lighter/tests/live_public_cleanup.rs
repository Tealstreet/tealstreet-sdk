use safe_cex_venue_lighter::{
    sdk_account_api::{AccountObject, AccountValue},
    transaction_builders::TransactionBuildError,
    transaction_signing::*,
    ws_public_cleanup::*,
};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    process::Command,
    rc::Rc,
};
fn capture() -> &'static Value {
    static C: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/public-cleanup-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
struct Marker;
impl AccountObject for Marker {
    fn get(&self, _: &str) -> SigningResult<AccountValue> {
        Ok(AccountValue::Undefined)
    }
}
struct Pool {
    objects: HashMap<String, Rc<dyn AccountObject>>,
    functions: HashMap<String, HostThrownValue>,
}
impl Pool {
    fn new() -> Self {
        Self {
            objects: ["A", "B", "P"]
                .into_iter()
                .map(|id| (id.into(), Rc::new(Marker) as Rc<dyn AccountObject>))
                .collect(),
            functions: ["constructor", "toString"]
                .into_iter()
                .map(|id| (id.into(), HostThrownValue::new(id)))
                .collect(),
        }
    }
    fn decode(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "number" => AccountValue::Number(number(v)),
            "string" => AccountValue::String(units(&v["units"])),
            "object" => AccountValue::Object(self.objects[v["id"].as_str().unwrap()].clone()),
            "function" => AccountValue::Function(self.functions[v["id"].as_str().unwrap()].clone()),
            _ => panic!("unknown source fixture"),
        }
    }
    fn assert(&self, v: &AccountValue, w: &Value, label: &str) {
        match v {
            AccountValue::Undefined => {
                assert_eq!(w["kind"], "undefined", "{label} native undefined")
            }
            AccountValue::Null => assert_eq!(w["kind"], "null", "{label} native null"),
            AccountValue::Boolean(v) => {
                assert_eq!(w["kind"], "boolean");
                assert_eq!(*v, w["value"].as_bool().unwrap(), "{label} native bool")
            }
            AccountValue::Number(v) => {
                assert_eq!(w["kind"], "number");
                assert_eq!(
                    v.to_bits(),
                    number(w).to_bits(),
                    "{label} native number bits"
                )
            }
            AccountValue::String(v) => {
                assert_eq!(w["kind"], "string");
                assert_eq!(*v, units(&w["units"]), "{label} native UTF16")
            }
            AccountValue::Object(v) => {
                assert_eq!(w["kind"], "object");
                assert!(
                    Rc::ptr_eq(v, &self.objects[w["id"].as_str().unwrap()]),
                    "{label} native timer object identity"
                )
            }
            AccountValue::Function(v) => {
                assert_eq!(w["kind"], "function");
                assert!(
                    v.ptr_eq(&self.functions[w["id"].as_str().unwrap()]),
                    "{label} native inherited function identity"
                )
            }
            _ => panic!("unwitnessed fixture value"),
        }
    }
}
#[derive(Clone)]
struct Entry {
    key: Vec<u16>,
    value: AccountValue,
    enumerable: bool,
}
#[derive(Debug, Clone, PartialEq)]
struct Event {
    op: String,
    record: Option<String>,
    key: Option<Vec<u16>>,
}
type Trace = Rc<RefCell<Vec<Event>>>;
fn event(trace: &Trace, op: &str, record: Option<&str>, key: Option<&[u16]>) {
    trace.borrow_mut().push(Event {
        op: op.into(),
        record: record.map(str::to_owned),
        key: key.map(<[u16]>::to_vec),
    })
}
fn array_index(key: &[u16]) -> Option<u32> {
    if key.is_empty()
        || key.len() > 10
        || (key.len() > 1 && key[0] == 48)
        || key.iter().any(|v| !(48..=57).contains(v))
    {
        return None;
    }
    let mut n = 0u64;
    for v in key {
        n = n * 10 + u64::from(*v - 48)
    }
    (n < u64::from(u32::MAX)).then_some(n as u32)
}
struct Record {
    id: String,
    entries: RefCell<Vec<Entry>>,
    pool: Rc<Pool>,
    trace: Trace,
    scenario: String,
    token: HostThrownValue,
}
impl Record {
    fn keys(&self) -> Vec<Vec<u16>> {
        let entries = self.entries.borrow();
        let mut numeric: Vec<_> = entries
            .iter()
            .filter_map(|e| array_index(&e.key).map(|i| (i, e.key.clone())))
            .collect();
        numeric.sort_by_key(|v| v.0);
        numeric
            .into_iter()
            .map(|v| v.1)
            .chain(
                entries
                    .iter()
                    .filter(|e| array_index(&e.key).is_none())
                    .map(|e| e.key.clone()),
            )
            .collect()
    }
    fn remove(&self, key: &[u16]) {
        self.entries.borrow_mut().retain(|e| e.key != key)
    }
    fn assert_snapshot(&self, w: &Value, label: &str) {
        assert_eq!(
            self.id,
            w["id"].as_str().unwrap(),
            "{label} native record identity"
        );
        let expected = w["entries"].as_array().unwrap();
        let keys = self.keys();
        assert_eq!(
            keys,
            expected
                .iter()
                .map(|e| units(&e["key"]))
                .collect::<Vec<_>>(),
            "{label} native record keys"
        );
        for (key, w) in keys.iter().zip(expected) {
            let entries = self.entries.borrow();
            let e = entries.iter().find(|e| e.key == *key).unwrap();
            assert_eq!(
                e.enumerable,
                w["enumerable"].as_bool().unwrap(),
                "native property enumerability"
            );
            self.pool.assert(&e.value, &w["value"], label)
        }
    }
}
impl CleanupRecordAccess for Record {
    fn get(&self, key: &[u16]) -> SigningResult<AccountValue> {
        event(&self.trace, "get", Some(&self.id), Some(key));
        if self.scenario == "getThrow" || self.scenario == "valueGetterThrow" && key == u("2") {
            return Err(self.token.clone().into());
        }
        if self.scenario == "deleteLaterOnGet" && key == u("2") {
            self.remove(&u("10"))
        }
        if self.scenario == "appendOnGet" && key == u("2") {
            self.entries.borrow_mut().push(Entry {
                key: u("added"),
                value: AccountValue::Object(self.pool.objects["B"].clone()),
                enumerable: true,
            })
        }
        if let Some(e) = self.entries.borrow().iter().find(|e| e.key == key) {
            return Ok(e.value.clone());
        }
        Ok(if key == u("__proto__") {
            AccountValue::Object(self.pool.objects["P"].clone())
        } else if key == u("constructor") {
            AccountValue::Function(self.pool.functions["constructor"].clone())
        } else if key == u("toString") {
            AccountValue::Function(self.pool.functions["toString"].clone())
        } else {
            AccountValue::Undefined
        })
    }
    fn delete(&self, key: &[u16]) -> SigningResult<()> {
        event(&self.trace, "delete", Some(&self.id), Some(key));
        if self.scenario == "deleteThrow" {
            return Err(self.token.clone().into());
        }
        self.remove(key);
        Ok(())
    }
    fn values(&self) -> SigningResult<Vec<AccountValue>> {
        event(&self.trace, "ownKeys", Some(&self.id), None);
        let keys = self.keys();
        let mut result = vec![];
        for key in keys {
            event(&self.trace, "descriptor", Some(&self.id), Some(&key));
            let enumerable = self
                .entries
                .borrow()
                .iter()
                .find(|e| e.key == key)
                .is_some_and(|e| e.enumerable);
            if enumerable {
                result.push(self.get(&key)?);
            }
        }
        Ok(result)
    }
}
struct Array {
    id: String,
    entries: RefCell<BTreeMap<u32, AccountValue>>,
    length: RefCell<u32>,
    trace: Trace,
    scenario: String,
    token: HostThrownValue,
}
impl Array {
    fn assert_snapshot(&self, pool: &Pool, w: &Value, label: &str) {
        assert_eq!(
            self.id,
            w["id"].as_str().unwrap(),
            "{label} native array identity"
        );
        assert_eq!(
            *self.length.borrow(),
            w["length"].as_u64().unwrap() as u32,
            "{label} native array length"
        );
        let entries = self.entries.borrow();
        let expected = w["entries"].as_array().unwrap();
        assert_eq!(
            entries.len(),
            expected.len(),
            "source expected native sparse element count"
        );
        for ((index, v), w) in entries.iter().zip(expected) {
            assert_eq!(
                *index,
                w["index"].as_u64().unwrap() as u32,
                "native sparse index/order"
            );
            pool.assert(v, &w["value"], label)
        }
    }
}
impl CleanupArrayAccess for Array {
    fn length(&self) -> SigningResult<AccountValue> {
        event(&self.trace, "arrayGet", Some(&self.id), Some(&u("length")));
        if self.scenario == "lengthThrow" {
            return Err(self.token.clone().into());
        }
        Ok(AccountValue::Number(f64::from(*self.length.borrow())))
    }
    fn for_each(
        &self,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        event(&self.trace, "arrayGet", Some(&self.id), Some(&u("forEach")));
        if self.scenario == "forEachGetThrow" {
            return Err(self.token.clone().into());
        }
        event(&self.trace, "forEachCall", Some(&self.id), None);
        if self.scenario == "forEachCallThrow" {
            return Err(self.token.clone().into());
        }
        self.length()?;
        let length = *self.length.borrow();
        for index in 0..length {
            event(
                &self.trace,
                "has",
                Some(&self.id),
                Some(&u(&index.to_string())),
            );
            let value = self.entries.borrow().get(&index).cloned();
            if let Some(value) = value {
                event(
                    &self.trace,
                    "arrayGet",
                    Some(&self.id),
                    Some(&u(&index.to_string())),
                );
                visit(value)?;
            }
        }
        Ok(())
    }
}
fn kind(s: &str) -> CleanupRecord {
    match s {
        "ohlcv" => CleanupRecord::OhlcvNoData,
        "stale" => CleanupRecord::OrderBookStale,
        "initial" => CleanupRecord::OrderBookInitialData,
        "intervals" => CleanupRecord::RestIntervals,
        "inflight" => CleanupRecord::RestInFlight,
        "cooldown" => CleanupRecord::RestCooldown,
        _ => panic!("record kind"),
    }
}
fn name(k: CleanupRecord) -> &'static str {
    match k {
        CleanupRecord::OhlcvNoData => "ohlcv",
        CleanupRecord::OrderBookStale => "stale",
        CleanupRecord::OrderBookInitialData => "initial",
        CleanupRecord::RestIntervals => "intervals",
        CleanupRecord::RestInFlight => "inflight",
        CleanupRecord::RestCooldown => "cooldown",
    }
}
struct ValuesCall {
    trace: Trace,
    scenario: String,
    token: HostThrownValue,
    values: Rc<RefCell<Vec<AccountValue>>>,
    modified_at_lookup: bool,
}
impl CleanupValuesCall for ValuesCall {
    fn for_of(
        &self,
        record: Rc<dyn CleanupRecordAccess>,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        if self.scenario == "valuesNonCallable" {
            return Err(TransactionBuildError {
                name: "TypeError",
                message: u("Object.values is not a function or its return value is not iterable"),
            }
            .into());
        }
        assert!(
            !self.modified_at_lookup,
            "native callee captured before owner getter changes Object.values"
        );
        event(&self.trace, "valuesCall", None, None);
        if self.scenario == "valuesCallThrow" {
            return Err(self.token.clone().into());
        }
        *self.values.borrow_mut() = record.values()?;
        event(&self.trace, "iteratorGet", None, None);
        if self.scenario == "iteratorGetThrow" {
            return Err(self.token.clone().into());
        }
        event(&self.trace, "iteratorCall", None, None);
        event(&self.trace, "nextGet", None, None);
        let mut index = 0;
        loop {
            assert!(index <= 20, "independent values iterator ceiling");
            event(&self.trace, "nextCall", None, None);
            if self.scenario == "nextThrow" {
                return Err(self.token.clone().into());
            }
            event(&self.trace, "doneGet", None, None);
            if index >= self.values.borrow().len() {
                break;
            }
            event(&self.trace, "valueGet", None, None);
            let value = self.values.borrow()[index].clone();
            index += 1;
            if let Err(error) = visit(value) {
                event(&self.trace, "returnGet", None, None);
                if self.scenario != "closeGetterThrow" {
                    event(&self.trace, "returnCall", None, None);
                }
                // Actual source preserves the body throw even if return lookup,
                // call or non-object return would separately fail.
                return Err(error);
            }
        }
        Ok(())
    }
}
struct Call {
    kind: &'static str,
    value: AccountValue,
}
struct Host {
    scenario: String,
    primary: CleanupRecord,
    method: String,
    current: RefCell<HashMap<CleanupRecord, Rc<Record>>>,
    original: HashMap<CleanupRecord, Rc<Record>>,
    replacement: Rc<Record>,
    counts: RefCell<HashMap<CleanupRecord, u32>>,
    pending: RefCell<Rc<Array>>,
    original_pending: Rc<Array>,
    alternate_pending: Rc<Array>,
    pending_count: RefCell<u32>,
    trace: Trace,
    calls: RefCell<Vec<Call>>,
    values: Rc<RefCell<Vec<AccountValue>>>,
    values_modified: RefCell<bool>,
    token: HostThrownValue,
    pool: Rc<Pool>,
}
impl Host {
    fn set_record(&self, kind: CleanupRecord, v: Rc<Record>) -> SigningResult<()> {
        event(&self.trace, "ownerSet", Some(name(kind)), None);
        if self.scenario == "setterThrow" && kind == self.primary
            || self.scenario == "inflightSetterThrow" && kind == CleanupRecord::RestInFlight
            || self.scenario == "cooldownSetterThrow" && kind == CleanupRecord::RestCooldown
        {
            return Err(self.token.clone().into());
        }
        self.current.borrow_mut().insert(kind, v);
        Ok(())
    }
    fn set_pending(&self, v: Rc<Array>) -> SigningResult<()> {
        event(&self.trace, "pendingSet", None, None);
        if self.scenario == "setterThrow" {
            return Err(self.token.clone().into());
        }
        *self.pending.borrow_mut() = v;
        Ok(())
    }
    fn cancel(&self, kind: &'static str, value: &AccountValue) -> SigningResult<()> {
        event(&self.trace, kind, None, None);
        self.calls.borrow_mut().push(Call {
            kind,
            value: value.clone(),
        });
        if self.scenario == "timerThrow" || self.scenario.starts_with("close") {
            return Err(self.token.clone().into());
        }
        if self.calls.borrow().len() == 1 {
            match self.scenario.as_str() {
                "replaceDuringClear" => {
                    if self.method == "clearPendingSubscribeTimeouts" {
                        self.set_pending(self.alternate_pending.clone())?
                    } else {
                        self.set_record(self.primary, self.replacement.clone())?
                    }
                }
                "deleteDuringClear" => {
                    if self.method == "clearPendingSubscribeTimeouts" {
                        self.pending.borrow().entries.borrow_mut().remove(&1);
                    } else {
                        self.current.borrow()[&self.primary].remove(&u("10"));
                    }
                }
                "appendDuringClear" => {
                    let array = self.pending.borrow();
                    let index = *array.length.borrow();
                    array
                        .entries
                        .borrow_mut()
                        .insert(index, AccountValue::Object(self.pool.objects["B"].clone()));
                    *array.length.borrow_mut() += 1;
                }
                "appendValuesDuringClear" => {
                    self.values
                        .borrow_mut()
                        .push(AccountValue::Object(self.pool.objects["B"].clone()));
                }
                "rewriteDuringClear" => {
                    self.pending
                        .borrow()
                        .entries
                        .borrow_mut()
                        .insert(1, AccountValue::Object(self.pool.objects["A"].clone()));
                }
                _ => {}
            }
        }
        Ok(())
    }
}
impl PublicCleanupHost for Host {
    fn object_values_callback(&self) -> SigningResult<Rc<dyn CleanupValuesCall>> {
        event(&self.trace, "valuesLookup", None, None);
        if self.scenario == "valuesLookupThrow" {
            return Err(self.token.clone().into());
        }
        if self.scenario == "replaceRecordDuringLookup" {
            self.set_record(self.primary, self.replacement.clone())?;
        }
        Ok(Rc::new(ValuesCall {
            trace: self.trace.clone(),
            scenario: self.scenario.clone(),
            token: self.token.clone(),
            values: self.values.clone(),
            modified_at_lookup: *self.values_modified.borrow(),
        }))
    }

    fn record(&self, kind: CleanupRecord) -> SigningResult<Rc<dyn CleanupRecordAccess>> {
        event(&self.trace, "ownerGet", Some(name(kind)), None);
        if (self.scenario == "ownerThrow" || self.scenario == "valuesNonCallableOwnerThrow")
            && kind == self.primary
        {
            return Err(self.token.clone().into());
        }
        if self.scenario == "replaceValuesDuringOwnerGet" {
            *self.values_modified.borrow_mut() = true;
        }
        let mut counts = self.counts.borrow_mut();
        let count = counts.entry(kind).or_default();
        *count += 1;
        if self.scenario == "secondOwner" && kind == self.primary && *count == 2 {
            self.current
                .borrow_mut()
                .insert(kind, self.replacement.clone());
        }
        Ok(self.current.borrow()[&kind].clone())
    }
    fn replace_empty_record(&self, kind: CleanupRecord) -> SigningResult<()> {
        self.set_record(
            kind,
            Rc::new(Record {
                id: format!("{}/empty", name(kind)),
                entries: RefCell::new(vec![]),
                pool: self.pool.clone(),
                trace: self.trace.clone(),
                scenario: self.scenario.clone(),
                token: self.token.clone(),
            }),
        )
    }
    fn pending_subscriptions(&self) -> SigningResult<Rc<dyn CleanupArrayAccess>> {
        event(&self.trace, "pendingGet", None, None);
        if self.scenario == "ownerThrow" {
            return Err(self.token.clone().into());
        }
        *self.pending_count.borrow_mut() += 1;
        if self.scenario == "secondOwner" && *self.pending_count.borrow() == 2 {
            *self.pending.borrow_mut() = self.alternate_pending.clone();
        }
        Ok(self.pending.borrow().clone())
    }
    fn replace_empty_pending_subscriptions(&self) -> SigningResult<()> {
        self.set_pending(Rc::new(Array {
            id: "pending/empty".into(),
            entries: RefCell::new(BTreeMap::new()),
            length: RefCell::new(0),
            trace: self.trace.clone(),
            scenario: self.scenario.clone(),
            token: self.token.clone(),
        }))
    }
    fn clear_timeout(&self, value: &AccountValue) -> SigningResult<()> {
        self.cancel("timeout", value)
    }
    fn clear_interval(&self, value: &AccountValue) -> SigningResult<()> {
        self.cancel("interval", value)
    }
}
#[test]
#[ignore = "actual TypeScript public websocket cleanup member bodies"]
fn public_cleanup_native_state_and_timer_arguments() {
    let rows = capture()["cases"].as_array().unwrap();
    let mut counts = BTreeMap::new();
    for (iteration, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        let scenario = row["scenario"].as_str().unwrap().to_owned();
        let pool = Rc::new(Pool::new());
        let trace: Trace = Rc::new(RefCell::new(vec![]));
        let token = HostThrownValue::new(vec![0xd800u16]);
        let record = |id: String, input: &Value| {
            Rc::new(Record {
                id,
                entries: RefCell::new(
                    input
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|e| Entry {
                            key: units(&e["key"]),
                            value: pool.decode(&e["value"]),
                            enumerable: e["enumerable"].as_bool().unwrap(),
                        })
                        .collect(),
                ),
                pool: pool.clone(),
                trace: trace.clone(),
                scenario: scenario.clone(),
                token: token.clone(),
            })
        };
        let original: HashMap<_, _> = row["inputs"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (kind(k), record(format!("{k}/old"), v)))
            .collect();
        let replacement = record(
            format!("{}/replacement", row["primary"].as_str().unwrap()),
            &row["replacement"],
        );
        let array = |id: &str, input: &Value| {
            let entries: BTreeMap<_, _> = input
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    (
                        e["index"].as_u64().unwrap() as u32,
                        pool.decode(&e["value"]),
                    )
                })
                .collect();
            let length = entries.keys().last().map_or(0, |i| i + 1);
            Rc::new(Array {
                id: id.into(),
                entries: RefCell::new(entries),
                length: RefCell::new(length),
                trace: trace.clone(),
                scenario: scenario.clone(),
                token: token.clone(),
            })
        };
        let pending = array("pending/old", &row["pendingInput"]);
        let alternate_pending = array("pending/replacement", &row["alternatePendingInput"]);
        let host = Host {
            scenario,
            primary: kind(row["primary"].as_str().unwrap()),
            method: method.into(),
            current: RefCell::new(original.clone()),
            original,
            replacement,
            counts: RefCell::new(HashMap::new()),
            pending: RefCell::new(pending.clone()),
            original_pending: pending,
            alternate_pending,
            pending_count: RefCell::new(0),
            trace: trace.clone(),
            calls: RefCell::new(vec![]),
            values: Rc::new(RefCell::new(vec![])),
            values_modified: RefCell::new(false),
            token: token.clone(),
            pool: pool.clone(),
        };
        let topic = units(&row["topic"]);
        let result = match method {
            "clearOhlcvNoDataTimeout" => clear_ohlcv_no_data_timeout(&host, &topic),
            "clearOrderBookStaleTimeout" => clear_order_book_stale_timeout(&host, &topic),
            "clearOrderBookInitialDataTimeout" => {
                clear_order_book_initial_data_timeout(&host, &topic)
            }
            "clearAllOrderBookInitialDataTimeouts" => {
                clear_all_order_book_initial_data_timeouts(&host)
            }
            "clearAllOrderBookStaleTimeouts" => clear_all_order_book_stale_timeouts(&host),
            "stopOrderBookRestFallback" => stop_order_book_rest_fallback(&host, &topic),
            "stopAllOrderBookRestFallback" => stop_all_order_book_rest_fallback(&host),
            "clearPendingSubscribeTimeouts" => clear_pending_subscribe_timeouts(&host),
            _ => panic!("unknown method"),
        };
        let label = format!("{method} iteration{iteration}");
        match result {
            Ok(()) => assert_eq!(row["output"], "ok", "{label} native result"),
            Err(TransactionSigningError::Thrown(e)) => {
                assert_eq!(row["output"], "thrown", "native throw kind");
                assert!(e.ptr_eq(&token), "{label} native exception identity");
                assert_eq!(
                    e.value().downcast_ref::<Vec<u16>>().unwrap(),
                    &vec![0xd800],
                    "native exception payload"
                )
            }
            Err(TransactionSigningError::Generated(e)) => {
                assert_eq!(
                    row["output"]["kind"], "error",
                    "native generated error kind"
                );
                assert_eq!(
                    u(e.name),
                    units(&row["output"]["name"]),
                    "native generated error name"
                );
                assert_eq!(
                    e.message,
                    units(&row["output"]["message"]),
                    "native generated error UTF16"
                );
            }
        }
        for (kind, record) in host.current.borrow().iter() {
            record.assert_snapshot(&row["current"][name(*kind)], &format!("{label} current"));
        }
        for (kind, record) in &host.original {
            record.assert_snapshot(&row["original"][name(*kind)], &format!("{label} original"));
        }
        host.pending
            .borrow()
            .assert_snapshot(&pool, &row["pendingCurrent"], &label);
        host.original_pending
            .assert_snapshot(&pool, &row["pendingOriginal"], &label);
        host.alternate_pending
            .assert_snapshot(&pool, &row["pendingAlternate"], &label);
        let calls = host.calls.borrow();
        let expected = row["calls"].as_array().unwrap();
        assert_eq!(
            calls.len(),
            expected.len(),
            "{label} native timer-call count"
        );
        for (call, w) in calls.iter().zip(expected) {
            assert_eq!(
                call.kind,
                w["kind"].as_str().unwrap(),
                "{label} native timer operation"
            );
            pool.assert(&call.value, &w["value"], &label)
        }
        let expected_trace: Vec<_> = row["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| Event {
                op: v["op"].as_str().unwrap().into(),
                record: v["record"].as_str().map(str::to_owned),
                key: v.get("key").map(units),
            })
            .collect();
        assert_eq!(
            *trace.borrow(),
            expected_trace,
            "{label} native owner/property/timer call order"
        );
        *counts.entry(method).or_insert(0usize) += 1;
    }
    assert_eq!(
        rows.len(),
        159,
        "independent cleanup floor, stopped after iteration158"
    );
    assert_eq!(
        counts,
        BTreeMap::from([
            ("clearOhlcvNoDataTimeout", 20),
            ("clearOrderBookStaleTimeout", 20),
            ("clearOrderBookInitialDataTimeout", 20),
            ("stopOrderBookRestFallback", 20),
            ("clearAllOrderBookInitialDataTimeouts", 21),
            ("clearAllOrderBookStaleTimeouts", 21),
            ("stopAllOrderBookRestFallback", 23),
            ("clearPendingSubscribeTimeouts", 14)
        ]),
        "independent method counts"
    );
    assert_eq!(
        capture()["contentHash"],
        "25c4812ddf36e3dc5e8214113d685de9b2bad6a0bc5cbafa16c3e69ad42cd554",
        "authored source-only corpus pin after native assertions"
    );
}
