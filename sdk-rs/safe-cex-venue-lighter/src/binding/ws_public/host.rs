//! One LighterPublicWebsocket owner seen through every public-socket host
//! family the ported bodies take: socket, readiness, subscriptions, cleanup
//! and watchdogs. Every operation is the live js_runtime operation.
use super::super::{
    ops::{JsIterator, Relation, copy_data, relation, to_string},
    realm::{Bound, Realm},
    value::{js_to_native, native_to_js, to_account, to_js, to_signing, to_thrown},
};
use crate::{
    exchange_account_access::ExchangeAccountAccessHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{SigningResult, TransactionSigningError},
    ws_public_class::{PublicSocketHost, Relational, SocketCallback},
    ws_public_cleanup::{
        CleanupArrayAccess, CleanupRecord, CleanupRecordAccess, CleanupValuesCall,
        PublicCleanupHost,
    },
    ws_public_readiness::{PublicReadinessHost, ReadinessCall, ReadinessTimerCall},
    ws_public_subscriptions::{PublicSubscriptionHost, SubscriptionCall, SubscriptionTimer},
    ws_public_watchdogs::{
        PublicWatchdogHost, WatchdogCall, WatchdogEntriesCall, WatchdogRecord,
        WatchdogRecordAccess, WatchdogTimerCall, WatchdogValuesCall,
    },
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::{cell::RefCell, rc::Rc};

type Issued = Rc<RefCell<Vec<Rc<Record>>>>;

/// Unqualified names the bodies capture that are module bindings, not owner
/// members (lighter.ws-public.ts imports and the timer globals).
const MODULE_BINDINGS: &[&str] = &[
    "clearTimeout",
    "clearInterval",
    "Boolean",
    "buildLighterCandleTopic",
    "resolveLighterInterval",
    "shouldAttemptLighterOhlcvRecovery",
    "shouldPromoteLighterOhlcvRecoveryToProxy",
];

pub(crate) struct Socket {
    realm: Realm,
    owner: Js,
    /// The class's [[HomeObject]].[[Prototype]]: `super.member` reads here.
    home_parent: Js,
    issued: Issued,
}

fn js<T>(result: Result<T, Thrown>) -> SigningResult<T> {
    result.map_err(to_signing)
}

fn not_object(value: &Js) -> Thrown {
    Thrown::type_error(&format!(
        "Cannot convert undefined or null to object (received {})",
        value.to_display()
    ))
}

fn delete(object: &Js, key: &[u16]) -> Result<(), Thrown> {
    if object.is_nullish() {
        return Err(not_object(object));
    }
    object.delete(key);
    Ok(())
}

/// GetIterator/for-of; a visitor failure closes the iterator and propagates.
fn iterate(
    value: &Js,
    visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
) -> SigningResult<()> {
    let iterator = js(JsIterator::open(value))?;
    while let Some(item) = iterator.step().map_err(to_signing)? {
        if let Err(error) = visit(to_account(&item)) {
            let _ = iterator.close();
            return Err(error);
        }
    }
    Ok(())
}

fn literal(fields: &[(&str, AccountValue)]) -> AccountValue {
    to_account(&Js::object(
        fields.iter().map(|(k, v)| (*k, to_js(v))).collect(),
    ))
}

/// A captured callee and receiver; Call fails only after the arguments exist.
struct Call {
    callee: Js,
    this: Js,
}

impl Call {
    fn invoke(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        if !self.callee.is_callable() {
            return Err(to_signing(Thrown::type_error(&format!(
                "{} is not a function",
                self.callee.to_display()
            ))));
        }
        js(self
            .callee
            .call(self.this.clone(), args.iter().map(to_js).collect()))
        .map(|v| to_account(&v))
    }
}

impl ReadinessCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.invoke(args)
    }
}
impl SubscriptionCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.invoke(args)
    }
}
impl WatchdogCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.invoke(args)
    }
}

/// A captured `setTimeout`/`setInterval` global, called with a fresh closure.
struct Timer(Call);

impl Timer {
    fn schedule(
        &self,
        callback: Rc<dyn Fn() -> SigningResult<()>>,
        delay: f64,
    ) -> SigningResult<AccountValue> {
        let function = Js::function("", move |_, _| {
            callback().map(|()| Js::Undefined).map_err(to_thrown)
        });
        self.0
            .invoke(&[to_account(&function), AccountValue::Number(delay)])
    }
}

impl ReadinessTimerCall for Timer {
    fn call(
        &self,
        callback: Rc<dyn Fn() -> SigningResult<()>>,
        delay: f64,
    ) -> SigningResult<AccountValue> {
        self.schedule(callback, delay)
    }
}
impl SubscriptionTimer for Timer {
    fn call(
        &self,
        callback: Rc<dyn Fn() -> SigningResult<()>>,
        delay: f64,
    ) -> SigningResult<AccountValue> {
        self.schedule(callback, delay)
    }
}
impl WatchdogTimerCall for Timer {
    fn call(
        &self,
        callback: Rc<dyn Fn() -> SigningResult<()>>,
        delay: f64,
    ) -> SigningResult<AccountValue> {
        self.schedule(callback, delay)
    }
}

/// An owner record (or the markets map) read at hand-out time.
struct Record(Js);

impl Record {
    fn get_key(&self, key: &[u16]) -> SigningResult<AccountValue> {
        js(self.0.get_key(key)).map(|v| to_account(&v))
    }
    fn set_key(&self, key: &[u16], value: AccountValue) -> SigningResult<()> {
        js(self.0.set_key(key, to_js(&value)))
    }
}

impl CleanupRecordAccess for Record {
    fn get(&self, key: &[u16]) -> SigningResult<AccountValue> {
        self.get_key(key)
    }
    fn delete(&self, key: &[u16]) -> SigningResult<()> {
        js(delete(&self.0, key))
    }
    fn values(&self) -> SigningResult<Vec<AccountValue>> {
        if self.0.is_nullish() {
            return Err(to_signing(not_object(&self.0)));
        }
        Ok(js(self.0.own_entries())?
            .iter()
            .map(|(_, v)| to_account(v))
            .collect())
    }
}

impl WatchdogRecordAccess for Record {
    fn get(&self, key: &[u16]) -> SigningResult<AccountValue> {
        self.get_key(key)
    }
    fn set(&self, key: &[u16], value: AccountValue) -> SigningResult<()> {
        self.set_key(key, value)
    }
    fn delete(&self, key: &[u16]) -> SigningResult<()> {
        js(delete(&self.0, key))
    }
}

/// The Js behind a record this host handed out, by allocation identity.
fn issued_value(issued: &Issued, record: *const ()) -> SigningResult<Js> {
    issued
        .borrow()
        .iter()
        .find(|r| Rc::as_ptr(r) as *const () == record)
        .map(|r| r.0.clone())
        .ok_or_else(|| {
            to_signing(Thrown::type_error(
                "record was not obtained from this socket host",
            ))
        })
}

/// A captured `Object.values`/`Object.keys`/`Object.entries` call.
struct Enumerate {
    call: Call,
    issued: Issued,
}

impl Enumerate {
    fn result(&self, record: *const ()) -> SigningResult<Js> {
        let target = issued_value(&self.issued, record)?;
        self.call.invoke(&[to_account(&target)]).map(|v| to_js(&v))
    }
}

impl CleanupValuesCall for Enumerate {
    fn for_of(
        &self,
        record: Rc<dyn CleanupRecordAccess>,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        iterate(&self.result(Rc::as_ptr(&record) as *const ())?, visit)
    }
}

impl WatchdogValuesCall for Enumerate {
    fn for_of(
        &self,
        record: Rc<dyn WatchdogRecordAccess>,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        iterate(&self.result(Rc::as_ptr(&record) as *const ())?, visit)
    }
}

impl WatchdogEntriesCall for Enumerate {
    fn for_of(
        &self,
        record: Rc<dyn WatchdogRecordAccess>,
        visit: &mut dyn FnMut(Vec<u16>, AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        let entries = self.result(Rc::as_ptr(&record) as *const ())?;
        iterate(&entries, &mut |entry| {
            // `const [topic, value] of ...`: destructure through the entry's iterator.
            let pair = js(JsIterator::open(&to_js(&entry)))?;
            let key = pair.step().map_err(to_signing)?.unwrap_or(Js::Undefined);
            let value = pair.step().map_err(to_signing)?.unwrap_or(Js::Undefined);
            pair.close().map_err(to_signing)?;
            let key = js(to_string(&key))?;
            visit(key.to_vec(), to_account(&value))
        })
    }
}

/// `this.pendingSubscribeTimeouts`: length and receiver-bound forEach.
struct Pending(Js);

impl CleanupArrayAccess for Pending {
    fn length(&self) -> SigningResult<AccountValue> {
        js(self.0.get("length")).map(|v| to_account(&v))
    }
    fn for_each(
        &self,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        let for_each = js(self.0.get("forEach"))?;
        if !for_each.is_callable() {
            return Err(to_signing(Thrown::type_error(&format!(
                "{}.forEach is not a function",
                self.0.to_display()
            ))));
        }
        if let Js::Array(items) = &self.0 {
            let length = items.borrow().len();
            for index in 0..length {
                let present = items.borrow().get(index).cloned().flatten();
                if let Some(value) = present {
                    visit(to_account(&value))?;
                }
            }
            return Ok(());
        }
        // A non-array receiver runs its own forEach; visits follow it in order.
        let seen: Rc<RefCell<Vec<Js>>> = Rc::default();
        let sink = seen.clone();
        let collect = Js::function("", move |_, args| {
            sink.borrow_mut()
                .push(args.first().cloned().unwrap_or(Js::Undefined));
            Ok(Js::Undefined)
        });
        js(for_each.call(self.0.clone(), vec![collect]))?;
        let values = seen.borrow().clone();
        for value in values {
            visit(to_account(&value))?;
        }
        Ok(())
    }
}

impl Socket {
    pub(super) fn new(realm: Realm, owner: Js, home_parent: Js) -> Rc<Self> {
        Rc::new(Self {
            realm,
            owner,
            home_parent,
            issued: Rc::default(),
        })
    }
    fn own(&self, key: &str) -> SigningResult<Js> {
        js(self.owner.get(key))
    }
    fn assign(&self, key: &str, value: Js) -> SigningResult<()> {
        js(self.owner.set(key, value))
    }
    fn issue(&self, value: Js) -> Rc<Record> {
        let record = Rc::new(Record(value));
        self.issued.borrow_mut().push(record.clone());
        record
    }
    fn global_value(&self, name: &str) -> SigningResult<Js> {
        js(self.realm.global(name))
    }
    /// A module binding call: `a.b` keeps `a` as receiver, a bare name has none.
    fn module_call(&self, name: &str) -> SigningResult<Call> {
        Ok(match name.split_once('.') {
            Some((head, member)) => {
                let this = self.global_value(head)?;
                Call {
                    callee: js(this.get(member))?,
                    this,
                }
            }
            None => Call {
                callee: self.global_value(name)?,
                this: Js::Undefined,
            },
        })
    }
    /// `this.x(...)`, `this.a.b(...)`, `super.x(...)` or an imported binding.
    fn owner_call(&self, name: &str) -> SigningResult<Call> {
        if let Some(member) = name.strip_prefix("super.") {
            return Ok(Call {
                callee: js(self.home_parent.get(member))?,
                this: self.owner.clone(),
            });
        }
        if let Some((head, member)) = name.split_once('.') {
            let this = self.own(head)?;
            return Ok(Call {
                callee: js(this.get(member))?,
                this,
            });
        }
        if MODULE_BINDINGS.contains(&name) {
            return self.module_call(name);
        }
        Ok(Call {
            callee: self.own(name)?,
            this: self.owner.clone(),
        })
    }
    fn timer(&self, name: &str) -> SigningResult<Rc<Timer>> {
        Ok(Rc::new(Timer(self.module_call(name)?)))
    }
    fn clock(&self, object: &str, method: &str) -> SigningResult<f64> {
        let receiver = self.global_value(object)?;
        js(js(receiver.call_method(method, vec![]))?.to_number())
    }
    fn enumerate(&self, name: &str) -> SigningResult<Rc<Enumerate>> {
        Ok(Rc::new(Enumerate {
            call: self.module_call(name)?,
            issued: self.issued.clone(),
        }))
    }
    fn clear(&self, name: &str, value: &AccountValue) -> SigningResult<()> {
        self.module_call(name)?
            .invoke(std::slice::from_ref(value))?;
        Ok(())
    }
    fn property_key(key: &AccountValue) -> SigningResult<Vec<u16>> {
        js(to_string(&to_js(key))).map(|k| k.to_vec())
    }
}

fn cleanup_field(kind: CleanupRecord) -> &'static str {
    match kind {
        CleanupRecord::OhlcvNoData => "ohlcvNoDataTimeouts",
        CleanupRecord::OrderBookStale => "orderBookStaleTimeouts",
        CleanupRecord::OrderBookInitialData => "orderBookInitialDataTimeouts",
        CleanupRecord::RestIntervals => "orderBookRestFallbackIntervals",
        CleanupRecord::RestInFlight => "orderBookRestFallbackInFlight",
        CleanupRecord::RestCooldown => "orderBookRestFallbackCooldownUntilByTopic",
    }
}

fn watchdog_field(kind: WatchdogRecord) -> &'static str {
    match kind {
        WatchdogRecord::Topics => "topics",
        WatchdogRecord::Handlers => "messageHandlers",
        WatchdogRecord::BookStale => "orderBookStaleTimeouts",
        WatchdogRecord::CandleNoData => "ohlcvNoDataTimeouts",
        WatchdogRecord::BookAttempts => "orderBookRecoveryAttempts",
        WatchdogRecord::BookRecovery => "lastOrderBookRecoveryAtByTopic",
    }
}

impl PublicSocketHost for Socket {
    fn owner(&self) -> AccountValue {
        to_account(&self.owner)
    }
    fn get(&self, object: &AccountValue, key: &AccountValue) -> SigningResult<AccountValue> {
        let key = Self::property_key(key)?;
        js(to_js(object).get_key(&key)).map(|v| to_account(&v))
    }
    fn set(
        &self,
        object: &AccountValue,
        key: &AccountValue,
        value: AccountValue,
    ) -> SigningResult<()> {
        let key = Self::property_key(key)?;
        js(to_js(object).set_key(&key, to_js(&value)))
    }
    fn delete(&self, object: &AccountValue, key: &AccountValue) -> SigningResult<()> {
        let key = Self::property_key(key)?;
        js(delete(&to_js(object), &key))
    }
    fn binding(&self, name: &str) -> SigningResult<AccountValue> {
        self.global_value(name).map(|v| to_account(&v))
    }
    fn invoke(
        &self,
        callee: &AccountValue,
        this: &AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        Call {
            callee: to_js(callee),
            this: to_js(this),
        }
        .invoke(args)
    }
    fn construct(
        &self,
        callee: &AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        js(to_js(callee).construct(args.iter().map(to_js).collect(), None)).map(|v| to_account(&v))
    }

    fn function(&self, callback: SocketCallback) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::function("", move |_, args| {
            let args: Vec<_> = args.iter().map(to_account).collect();
            callback(&args).map(|v| to_js(&v)).map_err(to_thrown)
        })))
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        Ok(literal(fields))
    }
    fn array(&self, values: &[AccountValue]) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::array(values.iter().map(to_js).collect())))
    }
    fn copy_object(&self, source: &AccountValue) -> SigningResult<AccountValue> {
        js(copy_data(&[to_js(source)])).map(|v| to_account(&v))
    }
    fn define(&self, object: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()> {
        js(to_js(object).define(key, to_js(&value)))
    }
    fn for_of(
        &self,
        value: &AccountValue,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        iterate(&to_js(value), visit)
    }
    fn template(&self, value: &AccountValue) -> SigningResult<Vec<u16>> {
        js(to_string(&to_js(value))).map(|s| s.to_vec())
    }
    fn relational(
        &self,
        op: Relational,
        left: &AccountValue,
        right: &AccountValue,
    ) -> SigningResult<bool> {
        let op = match op {
            Relational::Less => Relation::Less,
            Relational::Greater => Relation::Greater,
            Relational::LessEqual => Relation::LessOrEqual,
        };
        js(relation(op, &to_js(left), &to_js(right)))
    }
    fn error_value(&self, error: TransactionSigningError) -> AccountValue {
        to_account(&to_thrown(error).value)
    }
    fn throw_value(&self, value: AccountValue) -> TransactionSigningError {
        to_signing(Thrown::new(to_js(&value)))
    }
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.realm.0.extra.microtasks.clone()
    }
    fn await_value(&self, value: AccountValue) -> SigningResult<NativePromise<AccountValue>> {
        Ok(js_to_native(
            &self.realm.exchange_runtime(),
            self.realm.0.extra.microtasks.clone(),
            to_js(&value),
        ))
    }
    fn resolve_return(&self, value: AccountValue, resolver: PromiseResolver<AccountValue>) {
        let bound = Bound {
            realm: self.realm.clone(),
            owner: self.owner.clone(),
        };
        ExchangeAccountAccessHost::resolve_return(&bound, value, resolver);
    }
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue {
        to_account(&native_to_js(&self.realm.exchange_runtime(), promise))
    }
}

impl PublicReadinessHost for Socket {
    fn flag(&self, name: &str) -> SigningResult<bool> {
        Ok(self.own(name)?.truthy())
    }
    fn set_flag(&self, name: &str, value: bool) -> SigningResult<()> {
        self.assign(name, Js::Bool(value))
    }
    fn value(&self, name: &str) -> SigningResult<AccountValue> {
        self.own(name).map(|v| to_account(&v))
    }
    fn set_value(&self, name: &str, value: AccountValue) -> SigningResult<()> {
        self.assign(name, to_js(&value))
    }
    fn capture(&self, name: &str) -> SigningResult<Rc<dyn ReadinessCall>> {
        Ok(Rc::new(self.owner_call(name)?))
    }
    fn capture_timer(&self, name: &str) -> SigningResult<Rc<dyn ReadinessTimerCall>> {
        Ok(self.timer(name)?)
    }
    fn performance_now(&self) -> SigningResult<f64> {
        self.clock("performance", "now")
    }
    fn virtual_now(&self) -> SigningResult<f64> {
        self.clock("virtualClock", "getCurrentTimeMs")
    }
    fn ping_payload(&self, timestamp: f64) -> SigningResult<AccountValue> {
        Ok(literal(&[
            (
                "type",
                AccountValue::String("ping".encode_utf16().collect()),
            ),
            ("timestamp", AccountValue::Number(timestamp)),
        ]))
    }
}

impl PublicSubscriptionHost for Socket {
    fn owner(&self) -> AccountValue {
        to_account(&self.owner)
    }
    fn get(&self, object: &AccountValue, key: &[u16]) -> SigningResult<AccountValue> {
        js(to_js(object).get_key(key)).map(|v| to_account(&v))
    }
    fn set(&self, object: &AccountValue, key: &[u16], value: AccountValue) -> SigningResult<()> {
        js(to_js(object).set_key(key, to_js(&value)))
    }
    fn delete(&self, object: &AccountValue, key: &[u16]) -> SigningResult<()> {
        js(delete(&to_js(object), key))
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn SubscriptionCall>> {
        Ok(Rc::new(self.module_call(name)?))
    }
    fn member(&self, object: &AccountValue, key: &str) -> SigningResult<Rc<dyn SubscriptionCall>> {
        let this = to_js(object);
        Ok(Rc::new(Call {
            callee: js(this.get(key))?,
            this,
        }))
    }
    fn interval(&self) -> SigningResult<Rc<dyn SubscriptionTimer>> {
        Ok(self.timer("setInterval")?)
    }
    fn for_of(
        &self,
        value: &AccountValue,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        iterate(&to_js(value), visit)
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        Ok(literal(fields))
    }
    fn less_than(&self, left: f64, right: AccountValue) -> SigningResult<bool> {
        js(relation(Relation::Less, &Js::Number(left), &to_js(&right)))
    }
}

impl PublicCleanupHost for Socket {
    fn object_values_callback(&self) -> SigningResult<Rc<dyn CleanupValuesCall>> {
        Ok(self.enumerate("Object.values")?)
    }
    fn record(&self, kind: CleanupRecord) -> SigningResult<Rc<dyn CleanupRecordAccess>> {
        Ok(self.issue(self.own(cleanup_field(kind))?))
    }
    fn replace_empty_record(&self, kind: CleanupRecord) -> SigningResult<()> {
        self.assign(cleanup_field(kind), Js::object(vec![]))
    }
    fn pending_subscriptions(&self) -> SigningResult<Rc<dyn CleanupArrayAccess>> {
        Ok(Rc::new(Pending(self.own("pendingSubscribeTimeouts")?)))
    }
    fn replace_empty_pending_subscriptions(&self) -> SigningResult<()> {
        self.assign("pendingSubscribeTimeouts", Js::array(vec![]))
    }
    fn clear_timeout(&self, value: &AccountValue) -> SigningResult<()> {
        self.clear("clearTimeout", value)
    }
    fn clear_interval(&self, value: &AccountValue) -> SigningResult<()> {
        self.clear("clearInterval", value)
    }
}

impl PublicWatchdogHost for Socket {
    fn record(&self, kind: WatchdogRecord) -> SigningResult<Rc<dyn WatchdogRecordAccess>> {
        Ok(self.issue(self.own(watchdog_field(kind))?))
    }
    fn replace_empty(&self, kind: WatchdogRecord) -> SigningResult<()> {
        self.assign(watchdog_field(kind), Js::object(vec![]))
    }
    fn flag(&self, name: &str) -> SigningResult<bool> {
        Ok(self.own(name)?.truthy())
    }
    fn set_flag(&self, name: &str, value: bool) -> SigningResult<()> {
        self.assign(name, Js::Bool(value))
    }
    fn number(&self, name: &str) -> SigningResult<f64> {
        js(self.own(name)?.to_number())
    }
    fn set_number(&self, name: &str, value: f64) -> SigningResult<()> {
        self.assign(name, Js::Number(value))
    }
    fn capture(&self, name: &str) -> SigningResult<Rc<dyn WatchdogCall>> {
        Ok(Rc::new(self.owner_call(name)?))
    }
    fn capture_timeout(&self) -> SigningResult<Rc<dyn WatchdogTimerCall>> {
        Ok(self.timer("setTimeout")?)
    }
    fn capture_values(&self, name: &str) -> SigningResult<Rc<dyn WatchdogValuesCall>> {
        Ok(self.enumerate(name)?)
    }
    fn capture_entries(&self) -> SigningResult<Rc<dyn WatchdogEntriesCall>> {
        Ok(self.enumerate("Object.entries")?)
    }
    fn array_first(&self, value: &AccountValue) -> SigningResult<Option<AccountValue>> {
        let value = to_js(value);
        if !value.is_array() {
            return Ok(None);
        }
        js(value.get("0")).map(|v| Some(to_account(&v)))
    }
    fn property(&self, value: &AccountValue, name: &str) -> SigningResult<AccountValue> {
        js(to_js(value).get(name)).map(|v| to_account(&v))
    }
    fn now(&self) -> SigningResult<f64> {
        self.clock("virtualClock", "getCurrentTimeMs")
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        Ok(literal(fields))
    }
    fn markets_by_symbol(&self) -> SigningResult<Rc<dyn WatchdogRecordAccess>> {
        let store = js(self.own("parent")?.get("store"))?;
        Ok(self.issue(js(store.get("marketsBySymbol"))?))
    }
}
