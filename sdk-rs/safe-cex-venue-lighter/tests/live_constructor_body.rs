use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_constructor::{ExchangeConstructorHost, construct_exchange_body},
    sdk_account_api::{AccountObject, AccountValue},
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{HostThrownValue, SigningResult, TransactionSigningError},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    process::Command,
    rc::Rc,
};

struct Object {
    fields: RefCell<Vec<(String, AccountValue)>>,
    local: bool,
}
impl AccountObject for Object {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        Ok(self
            .fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(AccountValue::Undefined))
    }
}
struct Env {
    case: usize,
    row: Value,
    events: RefCell<VecDeque<Value>>,
    objects: RefCell<BTreeMap<String, Rc<Object>>>,
    functions: RefCell<BTreeMap<String, HostThrownValue>>,
    symbol: HostThrownValue,
}
#[derive(Clone)]
struct Host(Rc<Env>);
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
impl Host {
    fn object(&self, id: &str) -> Rc<Object> {
        if let Some(value) = self.0.objects.borrow().get(id).cloned() {
            return value;
        }
        let value = Rc::new(Object {
            fields: RefCell::default(),
            local: false,
        });
        self.0.objects.borrow_mut().insert(id.into(), value.clone());
        let record = self.0.row["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .cloned();
        if let Some(record) = record {
            value.fields.replace(
                record["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| (f["key"].as_str().unwrap().into(), self.decode(&f["value"])))
                    .collect(),
            );
        }
        value
    }
    fn obj(&self, id: &str) -> AccountValue {
        AccountValue::Object(self.object(id))
    }
    fn marker(&self, v: &AccountValue) -> Rc<Object> {
        let AccountValue::Object(actual) = v else {
            panic!("case{} native object", self.0.case)
        };
        self.0
            .objects
            .borrow()
            .values()
            .find(|o| Rc::ptr_eq(&((*o).clone() as Rc<dyn AccountObject>), actual))
            .cloned()
            .expect("registered native object")
    }
    fn decode(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "number" => AccountValue::Number(f64::from_bits(
                u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
            )),
            "string" => AccountValue::String(units(&v["units"])),
            "symbol" => AccountValue::Symbol(self.0.symbol.clone()),
            "object" => self.obj(v["id"].as_str().unwrap()),
            "function" => {
                let id = v["id"].as_str().unwrap();
                let found = self.0.functions.borrow().get(id).cloned();
                AccountValue::Function(found.unwrap_or_else(|| {
                    let value = HostThrownValue::new(id.to_owned());
                    self.0
                        .functions
                        .borrow_mut()
                        .insert(id.into(), value.clone());
                    value
                }))
            }
            _ => panic!("unexpected source value"),
        }
    }
    fn check(&self, actual: &AccountValue, expected: &Value, label: &str) {
        let prefix = format!("case{} {label}", self.0.case);
        match (expected["kind"].as_str().unwrap(), actual) {
            ("undefined", AccountValue::Undefined) | ("null", AccountValue::Null) => (),
            ("boolean", AccountValue::Boolean(v)) => assert_eq!(
                *v,
                expected["value"].as_bool().unwrap(),
                "{prefix} nativeBool"
            ),
            ("number", AccountValue::Number(v)) => assert_eq!(
                v.to_bits(),
                u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap(),
                "{prefix} nativeNumberBits"
            ),
            ("string", AccountValue::String(v)) => {
                assert_eq!(*v, units(&expected["units"]), "{prefix} nativeUtf16")
            }
            ("symbol", AccountValue::Symbol(v)) => {
                assert!(v.ptr_eq(&self.0.symbol), "{prefix} nativeSymbolIdentity")
            }
            ("function", AccountValue::Function(v)) => {
                let AccountValue::Function(wanted) = self.decode(expected) else {
                    unreachable!()
                };
                assert!(v.ptr_eq(&wanted), "{prefix} nativeCalleeIdentity")
            }
            ("object", AccountValue::Object(_)) => {
                let own = self.marker(actual);
                if let Some(fields) = expected.get("fields") {
                    assert!(own.local, "{prefix} native fresh literal");
                    let own_fields = own.fields.borrow();
                    let fields = fields.as_array().unwrap();
                    assert_eq!(own_fields.len(), fields.len(), "{prefix} nativeOwnCount");
                    for ((key, value), field) in own_fields.iter().zip(fields) {
                        assert_eq!(
                            key,
                            field["key"].as_str().unwrap(),
                            "{prefix} nativeOwnOrder"
                        );
                        self.check(value, &field["value"], label)
                    }
                    let id = expected["id"].as_str().unwrap();
                    let existing = self.0.objects.borrow().get(id).cloned();
                    if let Some(other) = existing {
                        assert!(Rc::ptr_eq(&own, &other), "{prefix} literal identity")
                    } else {
                        self.0.objects.borrow_mut().insert(id.into(), own.clone());
                    }
                } else {
                    assert!(
                        Rc::ptr_eq(&own, &self.object(expected["id"].as_str().unwrap())),
                        "{prefix} nativeObjectIdentity"
                    )
                }
            }
            _ => panic!("{prefix} nativeKind"),
        }
    }
    fn next(&self, op: &str) -> Value {
        let e = self
            .0
            .events
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| panic!("case{} extra native op{op}", self.0.case));
        assert_eq!(e["op"], op, "case{} nativeOperationOrder", self.0.case);
        e
    }
    fn result(&self, e: &Value) -> SigningResult<AccountValue> {
        if let Some(error) = e.get("error") {
            Err(self.throw_value(self.decode(error)))
        } else {
            Ok(self.decode(&e["value"]))
        }
    }
    fn args(&self, args: &[AccountValue], e: &Value) {
        let expected = e["args"].as_array().unwrap();
        assert_eq!(
            args.len(),
            expected.len(),
            "case{} nativeArgCount",
            self.0.case
        );
        for (index, (actual, wanted)) in args.iter().zip(expected).enumerate() {
            self.check(
                actual,
                wanted,
                &format!("{} arg{index}", e["name"].as_str().unwrap()),
            )
        }
    }
    fn invoke(
        &self,
        name: &str,
        construct: bool,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let e = self.next(if construct { "construct" } else { "call" });
        assert_eq!(e["name"], name, "case{} nativeCalleeName", self.0.case);
        self.args(args, &e);
        if e.get("error").is_some() {
            return self.result(&e);
        }
        // Required imported constructor's authored reentrant mutation, not a
        // substituted Lighter constructor-body result. The next native Get must
        // read this actual changed field from shared owner state.
        if name == "OrderApi" && (self.0.case == 4 || self.0.case == 27) {
            self.set(&self.owner(), "lighterApiClient", self.obj("ChangedApi"))?;
        }
        self.result(&e)
    }
}
struct Call {
    host: Host,
    name: String,
}
impl AccountAccessCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.host.invoke(&self.name, false, args)
    }
    fn construct(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.host.invoke(&self.name, true, args)
    }
}
impl ExchangeAccountAccessHost for Host {
    fn owner(&self) -> AccountValue {
        self.obj("Owner")
    }
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        let e = self.next("get");
        self.check(
            object,
            &json!({"kind":"object","id":e["object"]}),
            "Get receiver",
        );
        assert_eq!(e["key"], key, "case{} nativeGetKey", self.0.case);
        if e.get("error").is_some() {
            return self.result(&e);
        }
        let actual = self.marker(object).get(key)?;
        self.check(&actual, &e["value"], "Get actual native state");
        Ok(actual)
    }
    fn set(&self, object: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()> {
        let e = self.next("set");
        self.check(
            object,
            &json!({"kind":"object","id":e["object"]}),
            "Set receiver",
        );
        assert_eq!(e["key"], key, "case{} nativeSetKey", self.0.case);
        self.check(&value, &e["input"], "Set input");
        self.result(&e)?;
        let own = self.marker(object);
        let mut fields = own.fields.borrow_mut();
        if let Some((_, old)) = fields.iter_mut().find(|(name, _)| name == key) {
            *old = value
        } else {
            fields.push((key.into(), value))
        }
        Ok(())
    }
    fn member(&self, _: &AccountValue, _: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        panic!("unexpected constructor member lookup")
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        let e = self.next("global");
        assert_eq!(e["name"], name, "case{} nativeGlobalName", self.0.case);
        self.result(&e)?;
        Ok(Rc::new(Call {
            host: self.clone(),
            name: name.into(),
        }))
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        let object = Rc::new(Object {
            fields: RefCell::new(
                fields
                    .iter()
                    .map(|(key, value)| ((*key).into(), value.clone()))
                    .collect(),
            ),
            local: true,
        });
        let id = format!("$native{}", self.0.objects.borrow().len());
        self.0.objects.borrow_mut().insert(id, object.clone());
        Ok(AccountValue::Object(object))
    }
    fn throw_value(&self, value: AccountValue) -> TransactionSigningError {
        TransactionSigningError::Thrown(HostThrownValue::new(value))
    }
    fn resolve_return(&self, _: AccountValue, _: PromiseResolver<AccountValue>) {
        panic!("constructor has no async return")
    }
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        panic!("constructor has no runtime scheduling")
    }
    fn await_value(&self, _: AccountValue) -> SigningResult<NativePromise<AccountValue>> {
        panic!("constructor has no await")
    }
}
impl ExchangeConstructorHost for Host {
    fn super_and_instance_elements(
        &self,
        name: AccountValue,
        opts: AccountValue,
        store: AccountValue,
    ) -> SigningResult<AccountValue> {
        self.invoke("super-and-instance-elements", true, &[name, opts, store])
    }
}
#[test]
#[ignore = "live TypeScript constructor-body oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_constructor_body_native_floor() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/constructor-body-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    let expected_counts = [
        6, 6, 6, 6, 6, 0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 5, 3, 3,
    ];
    for (case, row) in rows.iter().enumerate() {
        let h = Host(Rc::new(Env {
            case,
            row: row.clone(),
            events: RefCell::new(row["events"].as_array().unwrap().iter().cloned().collect()),
            objects: RefCell::default(),
            functions: RefCell::default(),
            symbol: HostThrownValue::new("source symbol identity"),
        }));
        let args = row["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| h.decode(v))
            .collect::<Vec<_>>();
        let result = construct_exchange_body(&h, args[0].clone(), args[1].clone(), args[2].clone());
        match result {
            Ok(value) => {
                assert_eq!(row["outcome"]["ok"], true, "case{case} native success");
                h.check(
                    &value,
                    &row["outcome"]["value"],
                    "constructor returned native owner",
                )
            }
            Err(TransactionSigningError::Thrown(value)) => {
                assert_eq!(
                    row["outcome"]["ok"], false,
                    "case{case} native original failure"
                );
                let value = value
                    .value()
                    .downcast_ref::<AccountValue>()
                    .expect("original thrown native value");
                h.check(
                    value,
                    &row["outcome"]["value"],
                    "constructor original throw",
                )
            }
            Err(_) => panic!("case{case} unexpected generated error"),
        }
        assert_eq!(
            h.0.events.borrow().len(),
            0,
            "case{case} independent SourceEventCount"
        );
        let owner = h.object("Owner");
        let fields = owner.fields.borrow();
        let snapshot = row["snapshot"].as_array().unwrap();
        assert_eq!(
            fields.len(),
            expected_counts[case],
            "case{case} authored partial assignment count"
        );
        assert_eq!(
            fields.len(),
            snapshot.len(),
            "case{case} native snapshot own count"
        );
        for ((key, value), field) in fields.iter().zip(snapshot) {
            assert_eq!(
                key,
                field["key"].as_str().unwrap(),
                "case{case} native own assignment order"
            );
            h.check(value, &field["value"], "native retained partial field")
        }
        if case == 0 {
            let keys = [
                "lighterDeploymentProfile",
                "lighterApiClient",
                "orderApi",
                "candlestickApi",
                "publicWebsocket",
                "privateWebsocket",
            ];
            let ids = [
                "Profile",
                "Api",
                "OrderApi",
                "CandleApi",
                "PublicSocket",
                "PrivateSocket",
            ];
            for (index, (key, value)) in fields.iter().enumerate() {
                assert_eq!(key, keys[index], "authored independent assignment name");
                let actual = h.marker(value);
                assert!(
                    Rc::ptr_eq(&actual, &h.object(ids[index])),
                    "authored independent constructor result identity"
                )
            }
        }
        if case == 4 || case == 27 {
            let (_, api) = fields
                .iter()
                .find(|(key, _)| key == "lighterApiClient")
                .unwrap();
            assert!(
                Rc::ptr_eq(&h.marker(api), &h.object("ChangedApi")),
                "authored reentrant API mutation retained"
            )
        }
    }
    assert_eq!(rows.len(), 28, "independent authored constructor floor28");
    assert_eq!(
        corpus["contentHash"], "c703122e4614a8b663b1e2a098d687c176996b16e3e398c582ecb0869fbc3a3e",
        "source/import pin after native values"
    );
    eprintln!(
        "28 native constructor-body cases; stopped at iteration27; required super/instance-elements and imported constructors are bounded fixture operations, not native class proof"
    );
}
#[test]
#[ignore = "live TypeScript constructor-body oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn constructor_body_source_mutation_must_fail_by_name() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/constructor-body-source-control.cjs"
        ))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "sourceConstructorBodyMutationMustFailNamed"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("authored six constructor result identities"),
        "sourceConstructorBodyNamedAssertion: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
