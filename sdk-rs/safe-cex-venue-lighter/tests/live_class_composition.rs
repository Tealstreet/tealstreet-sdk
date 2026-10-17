use safe_cex_venue_lighter::{
    exchange_account_access::*,
    exchange_class::*,
    exchange_live_helpers::*,
    exchange_order_mapping::*,
    exchange_trading_preflight::*,
    sdk_account_api::{AccountObject, AccountValue},
    transaction_builders::TransactionBuildError,
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{
        AuthTokenEncoding, HostThrownValue, SigningResult, TransactionSigningError,
    },
};
use serde_json::Value;
use std::{cell::RefCell, collections::VecDeque, process::Command, rc::Rc};
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(u(s))
}
fn arg(a: &[AccountValue], i: usize) -> AccountValue {
    a.get(i).cloned().unwrap_or(AccountValue::Undefined)
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn err(name: &'static str, message: &str) -> TransactionSigningError {
    TransactionBuildError {
        name,
        message: u(message),
    }
    .into()
}
struct Object {
    fields: RefCell<Vec<(Vec<u16>, AccountValue)>>,
    array: bool,
    map: bool,
}
impl AccountObject for Object {
    fn get(&self, k: &str) -> SigningResult<AccountValue> {
        Ok(self
            .fields
            .borrow()
            .iter()
            .find(|(key, _)| *key == u(k))
            .map(|(_, v)| v.clone())
            .unwrap_or(AccountValue::Undefined))
    }
}
#[derive(Clone)]
enum Function {
    Arrow(usize, ExchangeMethod),
    Prototype(ExchangeMethod),
    Callback(TradingCallback),
    Builtin(String),
    Module(ModuleHelper),
}
#[derive(Default)]
struct Runtime {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl TransactionRuntime for Runtime {
    fn enqueue_microtask(&self, f: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(f)
    }
    fn now_millis(&self) -> f64 {
        10000.0
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("unused auth")
    }
}
struct State {
    expected_client: RefCell<Option<AccountValue>>,
    constructed_errors: RefCell<Vec<AccountValue>>,
    constructor: RefCell<Option<ClassConstructor>>,
    payload: RefCell<Option<AccountValue>>,
    requests: RefCell<usize>,
    bindings: RefCell<Vec<(String, AccountValue)>>,
    objects: RefCell<Vec<Rc<Object>>>,
    class: RefCell<Option<AccountValue>>,
    constants: Value,
    prototypes: RefCell<Vec<ExchangeMethod>>,
    helpers: RefCell<Vec<(&'static str, usize, ModuleHelper)>>,
    runtime: Rc<Runtime>,
    promise: RefCell<Vec<(AccountValue, NativePromise<AccountValue>)>>,
    log: RefCell<Vec<(AccountValue, AccountValue)>>,
}
#[derive(Clone)]
struct Realm(Rc<State>);
#[derive(Clone)]
struct Bound {
    realm: Realm,
    owner: AccountValue,
}
impl Realm {
    fn new(constants: Value) -> Self {
        Self(Rc::new(State {
            expected_client: RefCell::default(),
            constructed_errors: RefCell::default(),
            constructor: RefCell::default(),
            payload: RefCell::default(),
            requests: RefCell::new(0),
            bindings: RefCell::default(),
            objects: RefCell::default(),
            class: RefCell::default(),
            constants,
            prototypes: RefCell::default(),
            helpers: RefCell::default(),
            runtime: Rc::default(),
            promise: RefCell::default(),
            log: RefCell::default(),
        }))
    }
    fn object(
        &self,
        array: bool,
        map: bool,
        fields: Vec<(Vec<u16>, AccountValue)>,
    ) -> AccountValue {
        let o = Rc::new(Object {
            array,
            map,
            fields: RefCell::new(fields),
        });
        self.0.objects.borrow_mut().push(o.clone());
        AccountValue::Object(o)
    }
    fn record(&self, v: &AccountValue) -> Rc<Object> {
        let AccountValue::Object(v) = v else {
            panic!("native object")
        };
        self.0
            .objects
            .borrow()
            .iter()
            .find(|o| Rc::ptr_eq(&((*o).clone() as Rc<dyn AccountObject>), v))
            .unwrap()
            .clone()
    }
    fn index(&self, v: &AccountValue) -> usize {
        let AccountValue::Object(v) = v else {
            panic!("object index")
        };
        self.0
            .objects
            .borrow()
            .iter()
            .position(|o| Rc::ptr_eq(&((*o).clone() as Rc<dyn AccountObject>), v))
            .unwrap()
    }
    fn obj_index(&self, i: usize) -> AccountValue {
        AccountValue::Object(self.0.objects.borrow()[i].clone())
    }
    fn bound(&self, owner: AccountValue) -> Bound {
        Bound {
            realm: self.clone(),
            owner,
        }
    }
    fn function(&self, f: Function) -> AccountValue {
        AccountValue::Function(HostThrownValue::new(f))
    }
    fn set(&self, o: &AccountValue, k: Vec<u16>, v: AccountValue) {
        let r = self.record(o);
        let mut f = r.fields.borrow_mut();
        if let Some((_, value)) = f.iter_mut().find(|(key, _)| *key == k) {
            *value = v
        } else {
            f.push((k, v))
        }
    }
    fn decode(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "number" => AccountValue::Number(f64::from_bits(
                u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
            )),
            "bigint" => AccountValue::BigInt(v["value"].as_str().unwrap().parse().unwrap()),
            "string" => AccountValue::String(units(&v["units"])),
            "array" => self.object(
                true,
                false,
                v["values"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (u(&i.to_string()), self.decode(v)))
                    .collect(),
            ),
            "object" => self.object(
                false,
                false,
                v["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| (units(&f["keyUnits"]), self.decode(&f["value"])))
                    .collect(),
            ),
            _ => panic!("unproved input kind"),
        }
    }
    fn assert_value(&self, n: &AccountValue, e: &Value, label: &str) {
        match (n, e["kind"].as_str().unwrap()) {
            (AccountValue::Undefined, "undefined") | (AccountValue::Null, "null") => {}
            (AccountValue::Boolean(n), "boolean") => {
                assert_eq!(*n, e["value"].as_bool().unwrap(), "{label} nativeBool")
            }
            (AccountValue::String(n), "string") => {
                assert_eq!(*n, units(&e["units"]), "{label} nativeUtf16")
            }
            (AccountValue::BigInt(n), "bigint") => assert_eq!(
                *n,
                e["value"].as_str().unwrap().parse().unwrap(),
                "{label} nativeBigInt"
            ),
            (AccountValue::Number(n), "number") => {
                let e =
                    f64::from_bits(u64::from_str_radix(e["bits"].as_str().unwrap(), 16).unwrap());
                if e.is_nan() {
                    assert!(n.is_nan(), "{label} nativeNaN")
                } else {
                    assert_eq!(n.to_bits(), e.to_bits(), "{label} nativeNumberBits")
                }
            }
            (AccountValue::Function(f), "function") => {
                let m = match f.value().downcast_ref::<Function>().unwrap() {
                    Function::Arrow(_, m) | Function::Prototype(m) => m.metadata(),
                    _ => panic!("actual class function"),
                };
                assert_eq!(
                    m.name,
                    e["name"].as_str().unwrap(),
                    "{label} nativeFunctionName"
                );
                assert_eq!(
                    m.length,
                    e["length"].as_u64().unwrap() as usize,
                    "{label} nativeFunctionLength"
                );
                assert_eq!(
                    m.asynchronous,
                    e["async"].as_bool().unwrap(),
                    "{label} nativeFunctionAsync"
                )
            }
            (AccountValue::Object(_), "object") => {
                let f = self.record(n).fields.borrow().clone();
                let expected = e["fields"].as_array().unwrap();
                assert_eq!(f.len(), expected.len(), "{label} nativeOwnFieldCount");
                for ((k, v), e) in f.iter().zip(expected) {
                    assert_eq!(*k, units(&e["keyUnits"]), "{label} nativeOwnUTF16Key");
                    self.assert_value(v, &e["value"], label)
                }
            }
            (AccountValue::Object(_), "array") => {
                let o = self.record(n);
                assert!(o.array);
                let f = o.fields.borrow().clone();
                let expected = e["values"].as_array().unwrap();
                assert_eq!(f.len(), expected.len(), "{label} nativeArrayLength");
                for ((_, v), e) in f.iter().zip(expected) {
                    self.assert_value(v, e, label)
                }
            }
            (AccountValue::Object(_), "map") => {
                let o = self.record(n);
                assert!(o.map, "{label} actual nativeMap");
                assert_eq!(
                    o.fields.borrow().len(),
                    e["entries"].as_array().unwrap().len(),
                    "{label} nativeMapSize"
                );
                let fields = o.fields.borrow().clone();
                for ((key, value), expected) in fields.iter().zip(e["entries"].as_array().unwrap())
                {
                    self.assert_value(
                        &AccountValue::String(key.clone()),
                        &expected[0],
                        &format!("{label} nativeMapKey"),
                    );
                    self.assert_value(value, &expected[1], &format!("{label} nativeMapValue"));
                }
            }
            _ => panic!("{label} nativeKindMismatch vs {e}"),
        }
    }
    fn call(
        &self,
        f: AccountValue,
        receiver: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let AccountValue::Function(f) = f else {
            return Err(err("TypeError", "not callable"));
        };
        let f = f.value().downcast_ref::<Function>().unwrap().clone();
        match f {
            Function::Arrow(i, m) => m.invoke(self, self.obj_index(i), args),
            Function::Prototype(m) => m.invoke(self, receiver, args),
            Function::Callback(f) => f(args),
            Function::Module(m) => m.invoke(Rc::new(self.bound(receiver)), args),
            Function::Builtin(name) => self.builtin(&name, receiver, args),
        }
    }
}
impl ExchangeClassContexts for Realm {
    fn access(
        &self,
        owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_account_access::ExchangeAccountAccessHost> {
        Rc::new(self.bound(owner))
    }
    fn lookup(
        &self,
        owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_account_lookup::AccountLookupHost> {
        Rc::new(self.bound(owner))
    }
    fn cache(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_account_cache::AccountCacheHost> {
        panic!("unused context cache")
    }
    fn trading(
        &self,
        owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_trading_preflight::TradingHost> {
        Rc::new(self.bound(owner))
    }
    fn errors(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_error_recovery::ExchangeErrorRecoveryHost> {
        panic!("unused context errors")
    }
    fn initialization(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_runtime_initialization::RuntimeInitializationHost>
    {
        panic!("unused context initialization")
    }
    fn reconciliation(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_reconciliation::ReconciliationHost> {
        panic!("unused context reconciliation")
    }
    fn markets(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_market_reads::MarketReadHost> {
        panic!("unused context markets")
    }
    fn positions(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_positions::PositionReadHost> {
        panic!("unused context positions")
    }
    fn order_snapshot(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_order_snapshot::OrderSnapshotHost> {
        panic!("unused context order_snapshot")
    }
    fn fills(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_historical_fills::HistoricalFillsHost> {
        panic!("unused context fills")
    }
    fn ohlcv(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_ohlcv::OhlcvHost> {
        panic!("unused context ohlcv")
    }
    fn order_actions(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_order_actions::OrderActionHost> {
        panic!("unused context order_actions")
    }
    fn single_placement(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_single_placement::SinglePlacementHost> {
        panic!("unused context single_placement")
    }
    fn batch_placement(
        &self,
        _owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_batch_placement::BatchPlacementHost> {
        panic!("unused context batch_placement")
    }
    fn order_mapping(
        &self,
        owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_order_mapping::OrderMappingHost> {
        Rc::new(self.bound(owner))
    }
    fn live_helpers(
        &self,
        owner: AccountValue,
    ) -> Rc<dyn safe_cex_venue_lighter::exchange_live_helpers::LiveHelperHost> {
        Rc::new(self.bound(owner))
    }
    fn promise_value(&self, p: NativePromise<AccountValue>) -> AccountValue {
        let value = self.object(false, false, vec![]);
        self.0.promise.borrow_mut().push((value.clone(), p));
        value
    }
}
impl ExchangeClassRealm for Realm {
    fn install_module_helper(
        &self,
        n: &'static str,
        l: usize,
        h: ModuleHelper,
    ) -> SigningResult<()> {
        self.0.helpers.borrow_mut().push((n, l, h));
        Ok(())
    }
    fn create_derived_class(
        &self,
        base: AccountValue,
        n: &str,
        l: usize,
        body: ClassConstructor,
    ) -> SigningResult<AccountValue> {
        assert!(
            matches!(base, AccountValue::Function(_)),
            "actual captured base constructor"
        );
        self.0.constructor.replace(Some(body));
        assert_eq!(n, "LighterExchange");
        assert_eq!(l, 3);
        let class = self.object(false, false, vec![]);
        self.0.class.replace(Some(class.clone()));
        Ok(class)
    }
    fn define_prototype_method(
        &self,
        _class: &AccountValue,
        m: ExchangeMethod,
    ) -> SigningResult<()> {
        self.0.prototypes.borrow_mut().push(m);
        Ok(())
    }
    fn arrow(&self, owner: &AccountValue, m: ExchangeMethod) -> SigningResult<AccountValue> {
        Ok(self.function(Function::Arrow(self.index(owner), m)))
    }
    fn define_data(&self, o: &AccountValue, k: &str, v: AccountValue) -> SigningResult<()> {
        self.set(o, u(k), v);
        Ok(())
    }
    fn binding_value(&self, n: &str) -> SigningResult<AccountValue> {
        if let Some((_, v)) = self.0.bindings.borrow().iter().find(|(k, _)| k == n) {
            return Ok(v.clone());
        }
        let value = if n == "BaseExchange" {
            self.function(Function::Builtin("BaseExchange".into()))
        } else {
            self.decode(&self.0.constants[n])
        };
        self.0.bindings.borrow_mut().push((n.into(), value.clone()));
        Ok(value)
    }
    fn array_literal(&self, v: &[AccountValue]) -> SigningResult<AccountValue> {
        Ok(self.object(
            true,
            false,
            v.iter()
                .enumerate()
                .map(|(i, v)| (u(&i.to_string()), v.clone()))
                .collect(),
        ))
    }
    fn finish_class_binding(&self, _: &AccountValue) -> SigningResult<()> {
        Ok(())
    }
    fn construct_base(
        &self,
        class: AccountValue,
        new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        assert!(same(&class, &new_target), "actual default newTarget");
        Ok(self.object(
            false,
            false,
            vec![
                (u("name"), arg(args, 0)),
                (u("options"), arg(args, 1)),
                (u("store"), arg(args, 2)),
                (u("isDisposed"), AccountValue::Boolean(false)),
            ],
        ))
    }
}
fn same(a: &AccountValue, b: &AccountValue) -> bool {
    match (a, b) {
        (AccountValue::Object(a), AccountValue::Object(b)) => Rc::ptr_eq(a, b),
        (AccountValue::Function(a), AccountValue::Function(b)) => a.ptr_eq(b),
        (AccountValue::String(a), AccountValue::String(b)) => a == b,
        (AccountValue::Number(a), AccountValue::Number(b)) => a == b,
        (AccountValue::Undefined, AccountValue::Undefined)
        | (AccountValue::Null, AccountValue::Null) => true,
        _ => false,
    }
}
fn string(v: &AccountValue) -> Vec<u16> {
    match v {
        AccountValue::String(s) => s.clone(),
        AccountValue::Number(n) => u(&safe_cex_num::js_number_string(*n)),
        AccountValue::BigInt(n) => u(&n.to_string()),
        AccountValue::Boolean(b) => u(if *b { "true" } else { "false" }),
        AccountValue::Undefined => u("undefined"),
        AccountValue::Null => u("null"),
        _ => panic!("unproved object string coercion"),
    }
}
fn number(v: &AccountValue) -> f64 {
    match v {
        AccountValue::Number(n) => *n,
        AccountValue::String(s) => {
            let t = String::from_utf16(s).unwrap();
            if t.is_empty() {
                0.0
            } else {
                t.parse().unwrap_or(f64::NAN)
            }
        }
        AccountValue::Null => 0.0,
        AccountValue::Undefined => f64::NAN,
        AccountValue::Boolean(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        _ => panic!("unproved numeric coercion"),
    }
}
impl Bound {
    fn get_key(&self, o: &AccountValue, k: &[u16]) -> SigningResult<AccountValue> {
        if let AccountValue::String(s) = o {
            if k == u("length") {
                return Ok(AccountValue::Number(s.len() as f64));
            }
        }
        if matches!(o, AccountValue::Undefined | AccountValue::Null) {
            return Err(err(
                "TypeError",
                "Cannot read properties of undefined (reading 'info')",
            ));
        }
        let object = self.realm.record(o);
        if object.array && k == u("length") || object.map && k == u("size") {
            return Ok(AccountValue::Number(object.fields.borrow().len() as f64));
        }
        let value = object
            .fields
            .borrow()
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or(AccountValue::Undefined);
        Ok(value)
    }
}
struct Call {
    h: Bound,
    callee: AccountValue,
    receiver: AccountValue,
    name: String,
}
impl AccountAccessCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.h
            .realm
            .call(self.callee.clone(), self.receiver.clone(), args)
    }
    fn construct(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        match self.name.as_str() {
            "Map" => Ok(self.h.realm.object(false, true, vec![])),
            "OrderApi"
            | "CandlestickApi"
            | "LighterPublicWebsocket"
            | "LighterPrivateWebsocket" => Ok(self.h.realm.object(false, false, vec![])),
            _ => self.call(args),
        }
    }
}
impl ExchangeAccountAccessHost for Bound {
    fn owner(&self) -> AccountValue {
        self.owner.clone()
    }
    fn get(&self, o: &AccountValue, k: &str) -> SigningResult<AccountValue> {
        self.get_key(o, &u(k))
    }
    fn set(&self, o: &AccountValue, k: &str, v: AccountValue) -> SigningResult<()> {
        self.realm.set(o, u(k), v);
        Ok(())
    }
    fn member(&self, o: &AccountValue, k: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        let value = if matches!(o, AccountValue::Object(_)) {
            self.get(o, k)?
        } else {
            AccountValue::Undefined
        };
        let callee = if matches!(value, AccountValue::Function(_)) {
            value
        } else {
            self.realm.function(Function::Builtin(k.into()))
        };
        Ok(Rc::new(Call {
            h: self.clone(),
            callee,
            receiver: o.clone(),
            name: k.into(),
        }))
    }
    fn global(&self, n: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        let helper = self
            .realm
            .0
            .helpers
            .borrow()
            .iter()
            .find(|(name, _, _)| *name == n)
            .map(|(_, _, h)| *h);
        let f = helper
            .map(Function::Module)
            .unwrap_or_else(|| Function::Builtin(n.into()));
        Ok(Rc::new(Call {
            h: self.clone(),
            callee: self.realm.function(f),
            receiver: self.owner.clone(),
            name: n.into(),
        }))
    }
    fn literal(&self, f: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        Ok(self.realm.object(
            false,
            false,
            f.iter().map(|(k, v)| (u(k), v.clone())).collect(),
        ))
    }
    fn throw_value(&self, value: AccountValue) -> TransactionSigningError {
        assert!(
            self.realm
                .0
                .constructed_errors
                .borrow()
                .iter()
                .any(|v| same(v, &value)),
            "native original constructed error identity"
        );
        TransactionSigningError::Thrown(HostThrownValue::new(value))
    }
    fn resolve_return(&self, v: AccountValue, r: PromiseResolver<AccountValue>) {
        r.settle(Ok(v))
    }
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.realm.0.runtime.clone()
    }
    fn await_value(&self, v: AccountValue) -> SigningResult<NativePromise<AccountValue>> {
        if let Some((_, p)) = self
            .realm
            .0
            .promise
            .borrow()
            .iter()
            .find(|(value, _)| same(&v, value))
        {
            return Ok(p.clone());
        }
        Ok(NativePromise::settled(self.runtime(), Ok(v)))
    }
}
impl TradingHost for Bound {
    fn function(&self, f: TradingCallback) -> SigningResult<AccountValue> {
        Ok(self.realm.function(Function::Callback(f)))
    }
    fn array(&self, v: &[AccountValue]) -> SigningResult<AccountValue> {
        self.realm.array_literal(v)
    }
    fn index(&self, o: &AccountValue, k: AccountValue) -> SigningResult<AccountValue> {
        self.get_key(o, &string(&k))
    }
    fn constant(&self, n: &str) -> SigningResult<AccountValue> {
        if let Some(k) = n.strip_prefix("LighterExchange.") {
            return self.get(&self.class_constructor(), k);
        }
        let e = &self.realm.0.constants[n];
        assert!(!e.is_null(), "actual constant {n}");
        Ok(self.realm.decode(e))
    }
    fn error_value(&self, _: TransactionSigningError) -> AccountValue {
        panic!("unproved error projection")
    }
    fn template_string(&self, v: AccountValue) -> SigningResult<Vec<u16>> {
        Ok(string(&v))
    }
}
struct Iter {
    values: Vec<AccountValue>,
    index: usize,
}
impl OrderMapIterator for Iter {
    fn next(&mut self) -> SigningResult<Option<AccountValue>> {
        let v = self.values.get(self.index).cloned();
        self.index += 1;
        Ok(v)
    }
    fn close_return(&mut self) -> SigningResult<()> {
        Ok(())
    }
    fn close_throw(&mut self, e: TransactionSigningError) -> TransactionSigningError {
        e
    }
}
impl OrderMappingHost for Bound {
    fn iterator(&self, v: &AccountValue) -> SigningResult<Box<dyn OrderMapIterator>> {
        let object = self.realm.record(v);
        assert!(object.array);
        let values = object
            .fields
            .borrow()
            .iter()
            .map(|(_, v)| v.clone())
            .collect();
        Ok(Box::new(Iter { values, index: 0 }))
    }
    fn copy_data_properties(&self, s: &[AccountValue]) -> SigningResult<AccountValue> {
        let o = self.realm.object(false, false, vec![]);
        for source in s {
            for (k, v) in self.realm.record(source).fields.borrow().clone() {
                self.realm.set(&o, k, v)
            }
        }
        Ok(o)
    }
    fn binary(
        &self,
        op: OrderMapBinary,
        a: AccountValue,
        b: AccountValue,
    ) -> SigningResult<AccountValue> {
        if matches!(op, OrderMapBinary::Add) {
            if matches!(a, AccountValue::String(_)) || matches!(b, AccountValue::String(_)) {
                return Ok(AccountValue::String([string(&a), string(&b)].concat()));
            }
            if matches!(a, AccountValue::BigInt(_)) || matches!(b, AccountValue::BigInt(_)) {
                return Err(err(
                    "TypeError",
                    "Cannot mix BigInt and other types, use explicit conversions",
                ));
            }
        }
        let a = number(&a);
        let b = number(&b);
        Ok(match op {
            OrderMapBinary::Add => AccountValue::Number(a + b),
            OrderMapBinary::Subtract => AccountValue::Number(a - b),
            OrderMapBinary::Multiply => AccountValue::Number(a * b),
            OrderMapBinary::Divide => AccountValue::Number(a / b),
            OrderMapBinary::LessOrEqual => AccountValue::Boolean(a <= b),
            OrderMapBinary::GreaterThan => AccountValue::Boolean(a > b),
            OrderMapBinary::GreaterOrEqual => AccountValue::Boolean(a >= b),
        })
    }
    fn regex_literal(&self, pattern: &str, flags: &str) -> SigningResult<AccountValue> {
        assert_eq!(pattern, "0x[a-fA-F0-9]{40}");
        assert_eq!(flags, "");
        Ok(self
            .realm
            .object(false, false, vec![(u("pattern"), text(pattern))]))
    }
}
impl LiveHelperHost for Bound {
    fn class_constructor(&self) -> AccountValue {
        self.realm.0.class.borrow().as_ref().unwrap().clone()
    }
    fn global_value(&self, n: &str) -> SigningResult<AccountValue> {
        Ok(self.realm.function(Function::Builtin(n.into())))
    }
    fn less_than(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        Ok(AccountValue::Boolean(number(&a) < number(&b)))
    }
    fn negate(&self, v: AccountValue) -> SigningResult<AccountValue> {
        Ok(AccountValue::Number(-number(&v)))
    }
    fn define_data(&self, o: &AccountValue, k: &str, v: AccountValue) -> SigningResult<()> {
        self.set(o, k, v)
    }
}
impl Realm {
    fn builtin(
        &self,
        n: &str,
        r: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let a = arg(args, 0);
        let b = arg(args, 1);
        let bound = self.bound(r.clone());
        Ok(match n {
            "Error" => {
                let error = self.object(
                    false,
                    false,
                    vec![(u("name"), text("Error")), (u("message"), a)],
                );
                self.0.constructed_errors.borrow_mut().push(error.clone());
                error
            }
            "String" => AccountValue::String(string(&a)),
            "Number" => AccountValue::Number(number(&a)),
            "Boolean" => AccountValue::Boolean(a.truthy()),
            "BigInt" => {
                let s = String::from_utf16(&string(&a)).unwrap();
                AccountValue::BigInt(
                    s.parse()
                        .map_err(|_| err("SyntaxError", "invalid BigInt"))?,
                )
            }
            "Number.isFinite" => {
                AccountValue::Boolean(matches!(a,AccountValue::Number(n)if n.is_finite()))
            }
            "Number.parseFloat" => AccountValue::Number(safe_cex_num::parse_float(
                &String::from_utf16(&string(&a)).unwrap(),
            )),
            "Math.min" => AccountValue::Number(number(&a).min(number(&b))),
            "Math.max" => AccountValue::Number(number(&a).max(number(&b))),
            "Math.abs" => AccountValue::Number(number(&a).abs()),
            "Math.ceil" => AccountValue::Number(number(&a).ceil()),
            "virtualClock.getCurrentTimeMs" | "Date.now" => AccountValue::Number(10000.0),
            "getTickInterval" => AccountValue::Number(6000.0),
            "isWasmReady" => AccountValue::Boolean(false),
            "toLowerCase" => AccountValue::String(
                string(&r)
                    .into_iter()
                    .map(|c| if (65..=90).contains(&c) { c + 32 } else { c })
                    .collect(),
            ),
            "trim" => {
                let s = string(&r);
                let ws = |c: &u16| matches!(*c,0x9..=0xD|0x20|0xA0|0x1680|0x2000..=0x200A|0x2028|0x2029|0x202F|0x205F|0x3000|0xFEFF);
                let first = s.iter().position(|c| !ws(c)).unwrap_or(s.len());
                let last = s
                    .iter()
                    .rposition(|c| !ws(c))
                    .map(|i| i + 1)
                    .unwrap_or(first);
                AccountValue::String(s[first..last].to_vec())
            }
            "includes" => {
                let s = string(&r);
                let needle = string(&a);
                AccountValue::Boolean(
                    needle.is_empty() || s.windows(needle.len()).any(|w| w == needle),
                )
            }
            "endsWith" => AccountValue::Boolean(string(&r).ends_with(&string(&a))),
            "slice" => {
                let s = string(&r);
                let begin = number(&a).max(0.0) as usize;
                let end = number(&b);
                let end = if end < 0.0 {
                    (s.len() as f64 + end).max(0.0) as usize
                } else {
                    end.min(s.len() as f64) as usize
                };
                AccountValue::String(s[begin.min(end)..end].to_vec())
            }
            "split" => {
                let s = string(&r);
                let sep = string(&a);
                assert_eq!(sep, u("/"));
                let values = s
                    .split(|c| *c == 47)
                    .map(|s| AccountValue::String(s.to_vec()))
                    .collect::<Vec<_>>();
                self.array_literal(&values)?
            }
            "find" | "filter" | "every" | "some" => {
                let values = self.record(&r).fields.borrow().clone();
                let mut found = AccountValue::Undefined;
                let mut filtered = Vec::new();
                let mut all = true;
                for (i, (_, v)) in values.iter().enumerate() {
                    let result = self.call(
                        a.clone(),
                        AccountValue::Undefined,
                        &[v.clone(), AccountValue::Number(i as f64), r.clone()],
                    )?;
                    if result.truthy() {
                        if n == "find" || n == "some" {
                            found = v.clone();
                            break;
                        }
                        filtered.push(v.clone())
                    } else {
                        all = false;
                        if n == "every" {
                            break;
                        }
                    }
                }
                if n == "filter" {
                    self.array_literal(&filtered)?
                } else if n == "some" {
                    AccountValue::Boolean(!matches!(found, AccountValue::Undefined))
                } else if n == "every" {
                    AccountValue::Boolean(all)
                } else {
                    found
                }
            }
            "join" => {
                let mut s = Vec::new();
                for (i, (_, v)) in self.record(&r).fields.borrow().iter().enumerate() {
                    if i > 0 {
                        s.extend(string(&a))
                    }
                    if !v.is_nullish() {
                        s.extend(string(v))
                    }
                }
                AccountValue::String(s)
            }
            "get" => {
                let o = self.record(&r);
                if !o.map {
                    assert!(
                        same(
                            &r,
                            self.0
                                .expected_client
                                .borrow()
                                .as_ref()
                                .expect("actual constructed API client")
                        ),
                        "native original API receiver"
                    );
                    *self.0.requests.borrow_mut() += 1;
                    assert!(
                        matches!(a,AccountValue::String(ref s)if *s==u("/api/v1/account")),
                        "native actual account endpoint"
                    );
                    let params = bound.get(&b, "params")?;
                    assert!(
                        matches!(bound.get(&params,"by")?,AccountValue::String(s)if s==u("index")),
                        "native lookup by"
                    );
                    assert!(
                        matches!(bound.get(&params,"value")?,AccountValue::String(s)if s==u("7")),
                        "native lookup configured index"
                    );
                    let payload = self
                        .0
                        .payload
                        .borrow()
                        .as_ref()
                        .expect("native configured transport payload")
                        .clone();
                    return Ok(self.promise_value(NativePromise::settled(
                        self.0.runtime.clone(),
                        Ok(payload),
                    )));
                }
                let value = o
                    .fields
                    .borrow()
                    .iter()
                    .find(|(k, _)| *k == string(&a))
                    .map(|(_, v)| v.clone())
                    .unwrap_or(AccountValue::Undefined);
                value
            }
            "set" => {
                let object = self.record(&r);
                assert!(object.map);
                self.set(&r, string(&a), b);
                r.clone()
            }
            "toString" => AccountValue::String(string(&r)),
            "adjust" => {
                assert!(
                    matches!(b,AccountValue::Number(n)if n==1.0),
                    "authored precision argument"
                );
                a
            }
            "isLighterPartnerAttributionTierRejection"
            | "isLighterPartnerAttributionApprovalMissingRejection" => {
                let code = bound.get(&a, "code")?;
                let message = bound.get(&a, "message")?;
                let input = |v: &AccountValue| match v {
                    AccountValue::Number(n) => serde_json::json!(n),
                    AccountValue::String(s) => {
                        Value::String(String::from_utf16(s).expect("bounded imported primitive"))
                    }
                    AccountValue::Undefined => Value::Null,
                    _ => panic!("unproved import input"),
                };
                let code = input(&code);
                let message = input(&message);
                let result = if n == "isLighterPartnerAttributionTierRejection" {
                    safe_cex_venue_lighter::account_tier::is_lighter_partner_attribution_tier_rejection(Some(&code),Some(&message))
                } else {
                    safe_cex_venue_lighter::account_tier::is_lighter_partner_attribution_approval_missing_rejection(Some(&code),Some(&message))
                };
                AccountValue::Boolean(result.expect("bounded primitive coercion"))
            }
            "extractLighterAccountTierFromPayload" => {
                assert!(
                    matches!(a, AccountValue::Undefined),
                    "authored empty tier input"
                );
                assert!(safe_cex_venue_lighter::account_tier::extract_lighter_account_tier_from_payload(None).expect("bounded empty payload").is_none());
                AccountValue::Undefined
            }
            "push" => {
                let object = self.record(&r);
                assert!(object.array);
                let mut fields = object.fields.borrow_mut();
                for value in args {
                    let i = fields.len();
                    fields.push((u(&i.to_string()), value.clone()))
                }
                AccountValue::Number(fields.len() as f64)
            }
            "uniq" => {
                assert!(
                    self.record(&a).fields.borrow().is_empty(),
                    "authored empty address candidates"
                );
                self.array_literal(&[])?
            }
            "parseLighterPositiveIntegerString" => {
                assert!(
                    matches!(a,AccountValue::String(ref s)if *s==u("7")),
                    "authored configured applicationId"
                );
                let value = Value::String("7".into());
                text(
                    &safe_cex_venue_lighter::utils::parse_lighter_positive_integer_string(Some(
                        &value,
                    ))
                    .expect("bounded native imported parser")
                    .expect("authored valid index"),
                )
            }
            "parseLighterApiKeyIndex" => {
                assert!(
                    matches!(a, AccountValue::Undefined),
                    "authored absent legacy index"
                );
                assert!(
                    safe_cex_venue_lighter::utils::parse_lighter_api_key_index(None)
                        .expect("bounded native imported parser")
                        .is_none()
                );
                AccountValue::Undefined
            }
            "isLighterL1Address" => {
                let s = String::from_utf16(&string(&a)).expect("bounded address input");
                AccountValue::Boolean(
                    safe_cex_venue_lighter::utils::is_lighter_l1_address(Some(&Value::String(s)))
                        .expect("bounded native imported address predicate"),
                )
            }
            "match" => {
                assert_eq!(string(&r), u("7"), "bounded address match input");
                AccountValue::Null
            }
            "v" => bound.get_key(&a, &string(&b))?,
            "log" => {
                self.0.log.borrow_mut().push((a, b));
                AccountValue::Undefined
            }
            "resolveLighterDeploymentProfile" => self.object(
                false,
                false,
                vec![
                    (u("quoteCurrency"), text("USDC")),
                    (
                        u("marker"),
                        AccountValue::String([u("profile/"), vec![0xd800]].concat()),
                    ),
                ],
            ),
            "createLighterSDKClient" => {
                assert_eq!(
                    self.record(&b).fields.borrow().len(),
                    130,
                    "authored126derived+4base before SDK client"
                );
                let client = self.object(
                    false,
                    false,
                    vec![(
                        u("marker"),
                        AccountValue::String([u("client/"), vec![0xd801]].concat()),
                    )],
                );
                self.0.expected_client.replace(Some(client.clone()));
                client
            }
            _ => {
                assert_ne!(
                    n, "parseFloat",
                    "native liquidation captures Number.parseFloat"
                );
                panic!("unproved builtin {n}")
            }
        })
    }
}
fn construct(realm: &Realm, class: &AccountValue) -> AccountValue {
    let opts = realm.object(
        false,
        false,
        vec![(u("testnet"), AccountValue::Boolean(false))],
    );
    let empty = || realm.object(false, false, vec![]);
    let store = realm.object(
        false,
        false,
        vec![
            (u("marketsBySymbol"), empty()),
            (u("tickersBySymbol"), empty()),
            (u("positions"), realm.array_literal(&[]).unwrap()),
            (u("orders"), realm.array_literal(&[]).unwrap()),
        ],
    );
    let constructor = realm
        .0
        .constructor
        .borrow()
        .expect("actual registered constructor callback");
    constructor(
        realm,
        class.clone(),
        class.clone(),
        &[text("lighter"), opts, store],
    )
    .expect("native constructor")
}
fn source() -> Value {
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/class-composition-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "source class producer {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
#[ignore = "requires explicit maintained TypeScript oracle"]
fn native_class_fields_callbacks_and_live_helpers() {
    let corpus = source();
    let realm = Realm::new(corpus["constants"].clone());
    let class = LighterExchange::define(&realm).unwrap();
    let owner = construct(&realm, &class);
    let other = construct(&realm, &class);
    let fields = realm.record(&owner).fields.borrow().clone();
    assert_eq!(fields.len(), 130, "authored126derived+4base");
    let expected = corpus["fields"].as_array().unwrap();
    assert_eq!(expected.len(), 126, "independent source derived126");
    let mut arrows = 0;
    let mut maps = 0;
    for ((key, value), e) in fields[4..].iter().zip(expected) {
        let name = e["name"].as_str().unwrap();
        assert_eq!(*key, u(name), "native field order {name}");
        realm.assert_value(value, &e["value"], &format!("field {name}"));
        if matches!(value, AccountValue::Function(_)) {
            arrows += 1;
            assert!(
                !same(
                    value,
                    &realm.bound(other.clone()).get(&other, name).unwrap()
                ),
                "native arrow freshness {name}"
            )
        }
        if matches!(value, AccountValue::Object(_)) && realm.record(value).map {
            maps += 1;
            assert!(
                !same(
                    value,
                    &realm.bound(other.clone()).get(&other, name).unwrap()
                ),
                "native Map freshness {name}"
            )
        }
    }
    for name in ["grafanaEndpointsEvery", "grafanaEndpointsPeriodic"] {
        assert!(
            same(
                &realm.bound(owner.clone()).get(&owner, name).unwrap(),
                &realm.bound(other.clone()).get(&other, name).unwrap()
            ),
            "native shared imported endpoint binding {name}"
        );
    }
    assert!(
        !same(
            &realm
                .bound(owner.clone())
                .get(&owner, "grafanaEndpointsEvery")
                .unwrap(),
            &realm
                .bound(owner.clone())
                .get(&owner, "grafanaEndpointsPeriodic")
                .unwrap()
        ),
        "native distinct imported endpoint bindings"
    );
    assert_eq!(arrows, 92, "authored92fresh arrows");
    assert_eq!(maps, 5, "authored5fresh maps");
    assert_eq!(
        realm.0.helpers.borrow().len(),
        5,
        "authored5module bindings"
    );
    let proto = realm.0.prototypes.borrow().clone();
    assert_eq!(proto.len(), 2, "authored prototype2");
    for (m, e) in proto.iter().zip(corpus["prototypes"].as_array().unwrap()) {
        realm.assert_value(
            &realm.function(Function::Prototype(*m)),
            &e["value"],
            "prototype metadata",
        );
        assert_eq!(e["enumerable"], false)
    }
    for e in corpus["statics"].as_array().unwrap() {
        let value = realm
            .bound(owner.clone())
            .get(&class, e["name"].as_str().unwrap())
            .unwrap();
        realm.assert_value(&value, &e["value"], "static native value")
    }
    // A detached arrow still executes against its original lexical owner.
    let h = realm.bound(owner.clone());
    h.set(&owner, "accountSnapshotCacheVersion", text("1"))
        .unwrap();
    let detached = h.get(&owner, "invalidateAccountSnapshotCache").unwrap();
    realm.call(detached, other.clone(), &[]).unwrap();
    assert!(
        matches!(h.get(&owner,"accountSnapshotCacheVersion").unwrap(),AccountValue::String(s)if s==u("11")),
        "detached native lexical owner"
    );
    assert!(
        matches!(h.get(&other,"accountSnapshotCacheVersion").unwrap(),AccountValue::Number(n)if n==0.0),
        "foreign this untouched"
    );
    let rows = corpus["cases"].as_array().unwrap();
    let mut count = 0;
    for (i, row) in rows.iter().enumerate() {
        let realm = Realm::new(corpus["constants"].clone());
        let class = LighterExchange::define(&realm).unwrap();
        let owner = construct(&realm, &class);
        let h = realm.bound(owner.clone());
        for f in row["patch"]["fields"].as_array().unwrap() {
            realm.set(&owner, units(&f["keyUnits"]), realm.decode(&f["value"]))
        }
        for f in row["staticPatch"]["fields"].as_array().unwrap() {
            realm.set(&class, units(&f["keyUnits"]), realm.decode(&f["value"]))
        }
        let args = row["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| realm.decode(v))
            .collect::<Vec<_>>();
        let method = row["method"].as_str().unwrap();
        let call = if row["module"] == true {
            h.global(method).unwrap()
        } else {
            h.member(&owner, method).unwrap()
        };
        let result = call.call(&args);
        let label = format!("iteration{i} {method}");
        if [83, 84].contains(&i) {
            let Err(TransactionSigningError::Thrown(value)) = &result else {
                panic!("{label} nativeThrownOriginalErrorRequired")
            };
            let value = value
                .value()
                .downcast_ref::<AccountValue>()
                .expect("native original error carrier");
            let errors = realm.0.constructed_errors.borrow();
            assert_eq!(
                errors.len(),
                1,
                "{label} independent constructed Error count1"
            );
            assert!(
                same(value, &errors[0]),
                "{label} native sole constructed Error identity"
            );
        }
        if row["ok"] == true {
            realm.assert_value(
                &result.unwrap_or_else(|e| panic!("{label} nativeResultUnexpectedError: {e:?}")),
                &row["value"],
                &label,
            )
        } else {
            let error = result
                .err()
                .unwrap_or_else(|| panic!("{label} native abrupt expected"));
            let (name, message) = match error {
                TransactionSigningError::Generated(e) => (e.name.to_owned(), e.message),
                TransactionSigningError::Thrown(value) => {
                    let value = value
                        .value()
                        .downcast_ref::<AccountValue>()
                        .expect("native thrown value");
                    assert!(
                        realm
                            .0
                            .constructed_errors
                            .borrow()
                            .iter()
                            .any(|v| same(v, value)),
                        "{label} native error retains original identity"
                    );
                    let name = h.get(value, "name").unwrap();
                    (
                        String::from_utf16(&string(&name)).unwrap(),
                        string(&h.get(value, "message").unwrap()),
                    )
                }
            };
            assert_eq!(
                name,
                row["error"]["name"].as_str().unwrap(),
                "{label} native error name"
            );
            assert_eq!(
                message,
                units(&row["error"]["message"]),
                "{label} native error UTF16"
            );
        }
        realm.assert_value(
            &h.get(&owner, "accountSnapshotCacheVersion").unwrap(),
            &row["cacheVersion"],
            &format!("{label} native cache state"),
        );
        realm.assert_value(
            &h.get(&owner, "recentAccountSnapshot").unwrap(),
            &row["recentSnapshot"],
            &format!("{label} native snapshot state"),
        );
        realm.assert_value(
            &h.get(&owner, "placedOrderHints").unwrap(),
            &row["hints"],
            &format!("{label} native hint map"),
        );
        if method == "rememberRecentAccountSnapshot"
            && matches!(arg(&args,1),AccountValue::Number(n)if n==0.0)
        {
            let snapshot = h.get(&owner, "recentAccountSnapshot").unwrap();
            assert!(
                same(&h.get(&snapshot, "value").unwrap(), &args[0]),
                "native snapshot retains original payload alias"
            );
        }
        realm.assert_value(
            &h.get(&owner, "lastSignerWarningAt").unwrap(),
            &row["lastWarning"],
            &format!("{label} native warning clock"),
        );
        let logs = realm.0.log.borrow().clone();
        let expected_logs = &row["logs"]["values"];
        assert_eq!(
            logs.len(),
            expected_logs.as_array().unwrap().len(),
            "{label} native warning log count"
        );
        for ((message, severity), e) in logs.iter().zip(expected_logs.as_array().unwrap()) {
            realm.assert_value(
                message,
                &e["values"][0],
                &format!("{label} native warning message"),
            );
            realm.assert_value(
                severity,
                &e["values"][1],
                &format!("{label} native warning severity"),
            );
        }
        count += 1
    }
    assert_eq!(count, 85, "independent authored helper floor85");
    let mut async_count = 0;
    for (i, row) in corpus["asyncRows"].as_array().unwrap().iter().enumerate() {
        let realm = Realm::new(corpus["constants"].clone());
        let class = LighterExchange::define(&realm).unwrap();
        let owner = construct(&realm, &class);
        let h = realm.bound(owner.clone());
        let options = h.get(&owner, "options").unwrap();
        h.set(&options, "applicationId", text("7")).unwrap();
        h.set(&owner, "accountIndex", realm.decode(&row["cached"]))
            .unwrap();
        realm.0.payload.replace(Some(realm.decode(&row["payload"])));
        let result = h
            .member(&owner, row["method"].as_str().unwrap())
            .unwrap()
            .call(&[])
            .unwrap();
        let promise = h.await_value(result).unwrap();
        let settled = Rc::new(RefCell::new(None));
        let target = settled.clone();
        promise.subscribe(move |v| {
            target.replace(Some(v.map(|v| (*v).clone())));
        });
        let mut stopped = None;
        for iteration in 0..100 {
            let work = realm.0.runtime.queue.borrow_mut().pop_front();
            if let Some(work) = work {
                work()
            }
            if settled.borrow().is_some() {
                stopped = Some(iteration);
                break;
            }
        }
        assert!(
            stopped.is_some(),
            "native composed completion before FIFO100"
        );
        let value = settled.borrow_mut().take().unwrap().unwrap();
        realm.assert_value(
            &value,
            &row["value"],
            &format!("async{i} native composed return"),
        );
        realm.assert_value(
            &h.get(&owner, "accountIndex").unwrap(),
            &row["accountIndex"],
            &format!("async{i} native cached index"),
        );
        assert_eq!(
            *realm.0.requests.borrow(),
            [1, 1, 0][i],
            "independent native provider requests"
        );
        eprintln!("async{i} root completion at FIFO{}", stopped.unwrap());
        async_count += 1;
    }
    assert_eq!(async_count, 3, "independent composed async floor3");
    assert_eq!(
        corpus["pin"],
        "70e61aa83ff16f198d13cd35eb08ecce89b6f4f01f2292a8a098b4ad87e5d6de"
    );
    eprintln!(
        "85 native helper cases; stopped at iteration84; class126fields/92arrows/2prototype/3static census; no wire assertions; full realm GC and engine unproved"
    );
}
#[test]
#[ignore = "requires explicit maintained TypeScript oracle"]
fn class_source_mutation_must_fail_by_name() {
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/class-composition-source-control.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "sourceClassMutationMustFailNamed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

struct LookupIter {
    values: Vec<AccountValue>,
    index: RefCell<usize>,
}
impl safe_cex_venue_lighter::exchange_account_lookup::AccountLookupIterator for LookupIter {
    fn next(&self) -> SigningResult<Option<AccountValue>> {
        let mut i = self.index.borrow_mut();
        let v = self.values.get(*i).cloned();
        *i += 1;
        Ok(v)
    }
    fn close(&self, c: SigningResult<AccountValue>) -> SigningResult<AccountValue> {
        c
    }
}
impl safe_cex_venue_lighter::exchange_account_lookup::AccountLookupHost for Bound {
    fn append_spread(&self, a: &AccountValue, s: AccountValue) -> SigningResult<()> {
        let source = self.realm.record(&s).fields.borrow().clone();
        let target = self.realm.record(a);
        let mut fields = target.fields.borrow_mut();
        for (_, value) in source {
            let i = fields.len();
            fields.push((u(&i.to_string()), value))
        }
        Ok(())
    }
    fn iterator(
        &self,
        s: AccountValue,
    ) -> SigningResult<Rc<dyn safe_cex_venue_lighter::exchange_account_lookup::AccountLookupIterator>>
    {
        let o = self.realm.record(&s);
        assert!(o.array);
        let values = o.fields.borrow().iter().map(|(_, v)| v.clone()).collect();
        Ok(Rc::new(LookupIter {
            values,
            index: RefCell::new(0),
        }))
    }
}
