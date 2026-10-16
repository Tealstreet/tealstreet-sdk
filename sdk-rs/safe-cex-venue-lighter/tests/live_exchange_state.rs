use safe_cex_venue_lighter::{
    exchange_state::*,
    sdk_account_api::{AccountObject, AccountValue},
    transaction_builders::TransactionBuildError,
    transaction_signing::*,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    process::Command,
    rc::Rc,
};
fn capture() -> &'static Value {
    static C: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/exchange-state-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "source oracle: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
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
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
struct Obj {
    props: RefCell<Vec<(String, AccountValue)>>,
    symbols: RefCell<Vec<(HostThrownValue, AccountValue)>>,
}
impl AccountObject for Obj {
    fn get(&self, k: &str) -> SigningResult<AccountValue> {
        Ok(self
            .props
            .borrow()
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or(AccountValue::Undefined))
    }
}
type Map = Vec<(Vec<u16>, AccountValue)>;
struct Fixture {
    input: Value,
    objects: RefCell<HashMap<String, Rc<Obj>>>,
    trace: RefCell<Vec<String>>,
    calls: RefCell<Vec<(String, Vec<AccountValue>)>>,
    token: HostThrownValue,
    symbol_key: HostThrownValue,
    maps: RefCell<Vec<Map>>,
    current: Cell<usize>,
    version: Cell<f64>,
    last: Cell<f64>,
    recent: RefCell<Option<Option<RecentAccountSnapshot>>>,
    find_count: Cell<usize>,
}
impl Fixture {
    fn new(input: &Value) -> Rc<Self> {
        let objects = input["objects"]
            .as_object()
            .unwrap()
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    Rc::new(Obj {
                        props: RefCell::new(vec![]),
                        symbols: RefCell::new(vec![]),
                    }),
                )
            })
            .collect();
        let h = Rc::new(Self {
            input: input.clone(),
            objects: RefCell::new(objects),
            trace: RefCell::new(vec![]),
            calls: RefCell::new(vec![]),
            token: HostThrownValue::new("fixture throw"),
            symbol_key: HostThrownValue::new("proof symbol"),
            maps: RefCell::new(vec![vec![], vec![]]),
            current: Cell::new(0),
            version: Cell::new(number(&input["version"])),
            last: Cell::new(number(&input["last"])),
            recent: RefCell::new(None),
            find_count: Cell::new(0),
        });
        // Author the source insertion order independently: the three copied hint keys.
        for (id, props) in input["objects"].as_object().unwrap() {
            let keys: Vec<String> = if id == "I" {
                ["createdAt", "side", "symbol", "Symbol(proof)"]
                    .map(str::to_owned)
                    .to_vec()
            } else {
                props.as_object().unwrap().keys().cloned().collect()
            };
            for key in keys {
                h.objects.borrow()[id]
                    .props
                    .borrow_mut()
                    .push((key.clone(), h.decode(&props[&key])));
            }
        }
        h.objects.borrow()["I"]
            .symbols
            .borrow_mut()
            .push((h.symbol_key.clone(), h.object("A")));
        let mut map = vec![];
        if input["emptyOldest"].as_bool().unwrap() {
            map.push((vec![], h.object("A")));
        }
        for i in 0..input["seedCount"].as_u64().unwrap() {
            map.push((u(&format!("seed/{i}")), h.object("A")));
        }
        for entry in input["entries"].as_array().unwrap() {
            map.push((units(&entry[0]), h.decode(&entry[1])));
        }
        h.maps.borrow_mut()[0] = map;
        h
    }
    fn object(&self, id: &str) -> AccountValue {
        AccountValue::Object(self.objects.borrow()[id].clone())
    }
    fn decode(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" | "hole" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "number" => AccountValue::Number(number(v)),
            "string" => AccountValue::String(units(&v["units"])),
            "object" => self.object(v["id"].as_str().unwrap()),
            _ => panic!("unwitnessed input"),
        }
    }
    fn assert_value(&self, n: &AccountValue, w: &Value, label: &str) {
        match n {
            AccountValue::Undefined => {
                assert_eq!(w["kind"], "undefined", "{label} native undefined")
            }
            AccountValue::Null => assert_eq!(w["kind"], "null", "{label} native null"),
            AccountValue::Boolean(v) => {
                assert_eq!(w["kind"], "boolean", "{label} native kind");
                assert_eq!(*v, w["value"].as_bool().unwrap(), "{label} native boolean")
            }
            AccountValue::Number(v) => {
                assert_eq!(w["kind"], "number", "{label} native kind");
                assert_eq!(
                    v.to_bits(),
                    number(w).to_bits(),
                    "{label} native number bits"
                )
            }
            AccountValue::String(v) => {
                assert_eq!(w["kind"], "string", "{label} native kind");
                assert_eq!(*v, units(&w["units"]), "{label} native UTF16")
            }
            AccountValue::Object(v) => {
                assert_eq!(w["kind"], "object", "{label} native kind");
                let expected: Rc<dyn AccountObject> =
                    self.objects.borrow()[w["id"].as_str().unwrap()].clone();
                assert!(Rc::ptr_eq(v, &expected), "{label} native object identity")
            }
            _ => panic!("unwitnessed output"),
        }
    }
    fn touch(&self, event: &str) -> SigningResult<()> {
        self.trace.borrow_mut().push(event.into());
        if self.input["throwAt"].as_str() == Some(event) {
            return Err(TransactionSigningError::Thrown(self.token.clone()));
        }
        match (self.input["action"].as_str(), event) {
            (Some("replaceHints"), "clock:wall") => self.current.set(1),
            (Some("changeLast"), "clock:virtual") => self.last.set(7999.),
            (Some("changeExposed"), "find:2") => {
                self.set_prop("P1", "leverage", AccountValue::Number(11.))
            }
            (Some("changeFinite"), "lookup:NumberFinite") => {
                self.set_prop("P1", "leverage", AccountValue::Number(f64::NAN))
            }
            _ => {}
        }
        Ok(())
    }
    fn set_prop(&self, id: &str, key: &str, value: AccountValue) {
        let o = self.objects.borrow()[id].clone();
        let mut p = o.props.borrow_mut();
        if let Some((_, v)) = p.iter_mut().find(|(k, _)| k == key) {
            *v = value
        } else {
            p.push((key.into(), value))
        }
    }
    fn numeric(value: AccountValue) -> f64 {
        match value {
            AccountValue::Number(n) => n,
            AccountValue::Undefined => f64::NAN,
            AccountValue::Null => 0.,
            AccountValue::Boolean(v) => {
                if v {
                    1.
                } else {
                    0.
                }
            }
            AccountValue::String(v) => {
                let s = String::from_utf16(&v).unwrap();
                if s.trim().is_empty() {
                    0.
                } else {
                    s.trim().parse().unwrap_or(f64::NAN)
                }
            }
            _ => panic!("numeric fixture not enumerated"),
        }
    }
    fn identify(&self, value: &AccountValue) -> Option<String> {
        if let AccountValue::Object(v) = value {
            for (id, obj) in self.objects.borrow().iter() {
                let candidate: Rc<dyn AccountObject> = obj.clone();
                if Rc::ptr_eq(v, &candidate) {
                    return Some(id.clone());
                }
            }
        }
        None
    }
    fn assert_map(&self, n: &Map, w: &Value, label: &str) {
        let w = w.as_array().unwrap();
        assert_eq!(n.len(), w.len(), "{label} native map count");
        for ((key, value), want) in n.iter().zip(w) {
            assert_eq!(*key, units(&want["key"]), "{label} native map UTF16 key");
            self.assert_value(value, &want["value"], label);
        }
    }
}
struct Call {
    h: Rc<Fixture>,
    kind: StateCallKind,
    map: usize,
}
impl StateCall for Call {
    fn call(&self, a: &[AccountValue]) -> SigningResult<AccountValue> {
        let kind = format!("{:?}", self.kind);
        self.h.touch(&format!("call:{kind}"))?;
        self.h.calls.borrow_mut().push((kind, a.to_vec()));
        match self.kind {
            StateCallKind::HintSet => {
                let AccountValue::String(key) = &a[0] else {
                    panic!("native map key")
                };
                let mut maps = self.h.maps.borrow_mut();
                let m = &mut maps[self.map];
                if let Some((_, v)) = m.iter_mut().find(|(k, _)| k == key) {
                    *v = a[1].clone()
                } else {
                    m.push((key.clone(), a[1].clone()));
                }
                Ok(AccountValue::Undefined)
            }
            StateCallKind::HintGet => {
                let AccountValue::String(key) = &a[0] else {
                    panic!("native map key")
                };
                Ok(self.h.maps.borrow()[self.map]
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.clone())
                    .unwrap_or(AccountValue::Undefined))
            }
            StateCallKind::HintDelete => {
                let AccountValue::String(key) = &a[0] else {
                    panic!("native map key")
                };
                self.h.maps.borrow_mut()[self.map].retain(|(k, _)| k != key);
                Ok(AccountValue::Boolean(true))
            }
            StateCallKind::MarginGet => Ok(self.h.decode(&self.h.input["cached"])),
            StateCallKind::TierExtract => Ok(self.h.decode(&self.h.input["tier"])),
            StateCallKind::MarketMode => Ok(self.h.decode(&self.h.input["configured"])),
            StateCallKind::MarginTypeToMode | StateCallKind::SnapTier => Ok(a[0].clone()),
            StateCallKind::Boolean => Ok(AccountValue::Boolean(a[0].truthy())),
            StateCallKind::NumberFinite => Ok(AccountValue::Boolean(
                matches!(a[0],AccountValue::Number(n) if n.is_finite()),
            )),
            StateCallKind::MathMin | StateCallKind::MathMax => {
                let x = Fixture::numeric(a[0].clone());
                let y = Fixture::numeric(a[1].clone());
                let n = if x.is_nan() || y.is_nan() {
                    f64::NAN
                } else if self.kind == StateCallKind::MathMin {
                    x.min(y)
                } else {
                    x.max(y)
                };
                Ok(AccountValue::Number(n))
            }
        }
    }
}
struct Finder {
    h: Rc<Fixture>,
    values: Vec<AccountValue>,
}
impl PositionFinder for Finder {
    fn find(
        &self,
        p: &mut dyn FnMut(AccountValue) -> SigningResult<AccountValue>,
    ) -> SigningResult<AccountValue> {
        let n = self.h.find_count.get() + 1;
        self.h.find_count.set(n);
        self.h.touch(&format!("find:{n}"))?;
        for value in &self.values {
            if p(value.clone())?.truthy() {
                return Ok(value.clone());
            }
        }
        Ok(AccountValue::Undefined)
    }
}
// The fixture implements only the listed primitive operations. Production hosts
// must implement the full fallible ECMAScript contract documented on the trait.
struct Host(Rc<Fixture>);
impl std::ops::Deref for Host {
    type Target = Rc<Fixture>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl ExchangeStateHost for Host {
    fn callable(&self, kind: StateCallKind) -> SigningResult<Rc<dyn StateCall>> {
        match kind {
            StateCallKind::HintSet | StateCallKind::HintGet | StateCallKind::HintDelete => {
                self.touch("owner:hints")?
            }
            StateCallKind::MarginGet => self.touch("owner:margin")?,
            _ => {}
        }
        let map = self.current.get();
        self.touch(&format!("lookup:{kind:?}"))?;
        Ok(Rc::new(Call {
            h: self.0.clone(),
            kind,
            map,
        }))
    }
    fn order_id_string(&self, id: &num_bigint::BigInt) -> SigningResult<AccountValue> {
        self.touch("string:orderId")?;
        Ok(AccountValue::String(u(&id.to_string())))
    }
    fn copy_hint(&self, hint: &AccountValue) -> SigningResult<AccountValue> {
        let id = self.identify(hint).unwrap();
        let entries = self.objects.borrow()[&id].props.borrow().clone();
        let mut props = vec![];
        for (key, value) in entries {
            self.touch(&format!("property:{id}:{key}"))?;
            props.push((key, value));
        }
        let mut symbols = vec![];
        for (key, value) in self.objects.borrow()[&id].symbols.borrow().clone() {
            self.touch(&format!("property:{id}:Symbol(proof)"))?;
            symbols.push((key, value));
        }
        self.objects.borrow_mut().insert(
            "H".into(),
            Rc::new(Obj {
                props: RefCell::new(props),
                symbols: RefCell::new(symbols),
            }),
        );
        Ok(self.object("H"))
    }
    fn define_created_at(&self, hint: &AccountValue, value: AccountValue) -> SigningResult<()> {
        self.set_prop(&self.identify(hint).unwrap(), "createdAt", value);
        Ok(())
    }
    fn wall_now(&self) -> SigningResult<AccountValue> {
        self.touch("clock:wall")?;
        Ok(self.decode(&self.input["wall"]))
    }
    fn virtual_now(&self) -> SigningResult<AccountValue> {
        self.touch("clock:virtual")?;
        Ok(self.decode(&self.input["clock"]))
    }
    fn hint_size(&self) -> SigningResult<f64> {
        self.touch("owner:hints")?;
        self.touch("size:hints")?;
        Ok(self.maps.borrow()[self.current.get()].len() as f64)
    }
    fn oldest_hint_key(&self) -> SigningResult<AccountValue> {
        self.touch("owner:hints")?;
        self.touch("keys:hints")?;
        Ok(self.maps.borrow()[self.current.get()]
            .first()
            .map(|(k, _)| AccountValue::String(k.clone()))
            .unwrap_or(AccountValue::Undefined))
    }
    fn property(&self, value: &AccountValue, name: &str) -> SigningResult<AccountValue> {
        if let Some(id) = self.identify(value) {
            self.touch(&format!("property:{id}:{name}"))?;
            self.objects.borrow()[&id].get(name)
        } else {
            Err(TransactionSigningError::Generated(TransactionBuildError {
                name: "TypeError",
                message: u(&format!(
                    "Cannot read properties of undefined (reading '{name}')"
                )),
            }))
        }
    }
    fn age_greater_than(
        &self,
        now: AccountValue,
        at: AccountValue,
        threshold: f64,
    ) -> SigningResult<bool> {
        Ok(Fixture::numeric(now) - Fixture::numeric(at) > threshold)
    }
    fn age_less_than(
        &self,
        now: AccountValue,
        at: AccountValue,
        threshold: AccountValue,
    ) -> SigningResult<bool> {
        Ok(Fixture::numeric(now) - Fixture::numeric(at) < Fixture::numeric(threshold))
    }
    fn greater_than_zero(&self, value: AccountValue) -> SigningResult<bool> {
        Ok(Fixture::numeric(value) > 0.)
    }
    fn cache_version(&self) -> SigningResult<f64> {
        self.touch("get:version")?;
        Ok(self.version.get())
    }
    fn set_cache_version(&self, value: f64) -> SigningResult<()> {
        self.touch("set:version")?;
        self.version.set(value);
        Ok(())
    }
    fn set_recent_snapshot(&self, value: Option<RecentAccountSnapshot>) -> SigningResult<()> {
        self.touch("set:recent")?;
        *self.recent.borrow_mut() = Some(value);
        Ok(())
    }
    fn account_limits(&self) -> SigningResult<AccountValue> {
        self.touch("get:limits")?;
        Ok(self.object("L"))
    }
    fn orders_fetch_promise(&self) -> SigningResult<AccountValue> {
        self.touch("get:promise")?;
        Ok(self.decode(&self.input["promise"]))
    }
    fn last_orders_fetch_at(&self) -> SigningResult<f64> {
        self.touch("get:last")?;
        Ok(self.last.get())
    }
    fn market_by_symbol(&self, _: &[u16]) -> SigningResult<AccountValue> {
        self.touch("owner:store")?;
        self.touch("get:markets")?;
        self.touch("get:market")?;
        Ok(self.object("M"))
    }
    fn positions_find(&self) -> SigningResult<Rc<dyn PositionFinder>> {
        self.touch("owner:store")?;
        self.touch("get:positions")?;
        self.touch("lookup:find")?;
        Ok(Rc::new(Finder {
            h: self.0.clone(),
            values: self.input["positions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| self.decode(v))
                .collect(),
        }))
    }
}
fn run_family(start: usize, end: usize) {
    let oracle = capture();
    assert_eq!(
        oracle["rows"].as_array().unwrap().len(),
        82,
        "independent authored case floor"
    );
    let mut counts = std::collections::BTreeMap::new();
    for i in start..end {
        let row = &oracle["rows"][i];
        let input = &row["input"];
        let method = input["method"].as_str().unwrap();
        *counts.entry(method.to_owned()).or_insert(0usize) += 1;
        let label = format!("{method} iteration{i} {}", input["label"].as_str().unwrap());
        let h = Host(Fixture::new(input));
        let symbol = units(&input["symbol"]);
        let result = match method {
            "rememberPlacedOrderHint" => remember_placed_order_hint(
                &h,
                &input["id"].as_str().unwrap().parse().unwrap(),
                &h.object("I"),
            )
            .map(|()| AccountValue::Undefined),
            "readPlacedOrderHint" => {
                let id = h.decode(&input["readId"]);
                read_placed_order_hint(
                    &h,
                    match &id {
                        AccountValue::String(v) => Some(v),
                        AccountValue::Undefined => None,
                        _ => panic!("read id floor"),
                    },
                )
            }
            "rememberRecentAccountSnapshot" => {
                remember_recent_account_snapshot(&h, h.object("A"), number(&input["fetchVersion"]))
                    .map(|()| AccountValue::Undefined)
            }
            "invalidateAccountSnapshotCache" => {
                invalidate_account_snapshot_cache(&h).map(|()| AccountValue::Undefined)
            }
            "getCachedAccountTier" => get_cached_account_tier(&h),
            "shouldSkipOrderReconciliationFetch" => {
                should_skip_order_reconciliation_fetch(&h, Some(h.decode(&input["cooldown"])))
            }
            "getCurrentMarginModeForSymbol" => get_current_margin_mode_for_symbol(&h, &symbol),
            "getCurrentLeverageForSymbol" => {
                get_current_leverage_for_symbol(&h, &symbol, h.object("M"))
            }
            _ => panic!("unknown body"),
        };
        match result {
            Ok(value) => {
                assert!(row["error"].is_null(), "{label} native success");
                h.assert_value(&value, &row["result"], &label)
            }
            Err(TransactionSigningError::Thrown(value)) => {
                assert_eq!(row["error"]["kind"], "token", "{label} native throw kind");
                assert!(value.ptr_eq(&h.token), "{label} native exception identity")
            }
            Err(TransactionSigningError::Generated(error)) => {
                assert_eq!(
                    row["error"]["kind"], "generated",
                    "{label} native generated kind"
                );
                assert_eq!(
                    error.name,
                    row["error"]["name"].as_str().unwrap(),
                    "{label} native error name"
                );
                assert_eq!(
                    error.message,
                    units(&row["error"]["message"]),
                    "{label} native error UTF16"
                )
            }
        }
        assert_eq!(
            h.version.get().to_bits(),
            number(&row["version"]).to_bits(),
            "{label} native version bits"
        );
        match &*h.recent.borrow() {
            None => assert_eq!(
                row["snapshot"]["kind"], "old",
                "{label} native old snapshot"
            ),
            Some(None) => assert_eq!(
                row["snapshot"]["kind"], "undefined",
                "{label} native cleared snapshot"
            ),
            Some(Some(v)) => {
                assert_eq!(row["snapshot"]["kind"], "recent");
                h.assert_value(&v.fetched_at, &row["snapshot"]["fetchedAt"], &label);
                assert_eq!(
                    v.cache_version.to_bits(),
                    number(&row["snapshot"]["cacheVersion"]).to_bits(),
                    "{label} native snapshot version"
                );
                h.assert_value(&v.value, &row["snapshot"]["value"], &label)
            }
        }
        assert_eq!(
            h.current.get() == 0,
            row["sameMap"].as_bool().unwrap(),
            "{label} native map owner identity"
        );
        h.assert_map(
            &h.maps.borrow()[h.current.get()],
            &row["currentMap"],
            &label,
        );
        h.assert_map(&h.maps.borrow()[0], &row["oldMap"], &label);
        if !row["copied"].as_array().unwrap().is_empty() {
            let obj = h.objects.borrow()["H"].clone();
            let props = obj.props.borrow();
            assert_eq!(
                props.len(),
                4,
                "{label} independently authored copied string keys"
            );
            for (entry, (key, value)) in row["copied"].as_array().unwrap().iter().zip(props.iter())
            {
                assert_eq!(
                    key,
                    entry["key"].as_str().unwrap(),
                    "{label} native spread order"
                );
                h.assert_value(value, &entry["value"], &label);
            }
            let symbols = obj.symbols.borrow();
            assert_eq!(
                symbols.len(),
                1,
                "{label} independent native symbol key count"
            );
            assert_eq!(
                row["copiedSymbolKeys"],
                serde_json::json!(["proof"]),
                "independent source symbol witness"
            );
            assert!(
                symbols[0].0.ptr_eq(&h.symbol_key),
                "{label} native copied symbol key identity"
            );
            h.assert_value(&symbols[0].1, &row["copySymbol"], &label);
            h.assert_value(
                &props[3].1,
                &serde_json::json!({"kind":"object","id":"L"}),
                &label,
            );
        }
        let calls = h.calls.borrow();
        let expected = row["calls"].as_array().unwrap();
        assert_eq!(calls.len(), expected.len(), "{label} native call count");
        for ((kind, args), w) in calls.iter().zip(expected) {
            assert_eq!(
                kind,
                w["kind"].as_str().unwrap(),
                "{label} native call kind"
            );
            assert_eq!(
                args.len(),
                w["args"].as_array().unwrap().len(),
                "{label} native argument count"
            );
            for (value, want) in args.iter().zip(w["args"].as_array().unwrap()) {
                h.assert_value(value, want, &label)
            }
        }
        let expected: Vec<String> = row["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        assert_eq!(
            *h.trace.borrow(),
            expected,
            "{label} native operation order"
        );
    }
    let expected = match start {
        0 => vec![("rememberPlacedOrderHint", 13), ("readPlacedOrderHint", 14)],
        27 => vec![
            ("rememberRecentAccountSnapshot", 6),
            ("invalidateAccountSnapshotCache", 8),
            ("getCachedAccountTier", 6),
            ("shouldSkipOrderReconciliationFetch", 13),
        ],
        60 => vec![
            ("getCurrentMarginModeForSymbol", 10),
            ("getCurrentLeverageForSymbol", 12),
        ],
        _ => panic!("unauthored family"),
    };
    assert_eq!(
        counts,
        expected
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
        "independent authored per-method floor"
    );
    assert_eq!(
        oracle["contentHash"], "86ba68a9ddd6d1e93a429360080c90d07d3021c2c2cf0d76c65d21402c376fbe",
        "authored source-only full corpus pin after native assertions"
    );
    // Source provenance is checked only AFTER all native assertions in the family.
    assert_eq!(
        oracle["members"].as_array().unwrap().len(),
        8,
        "independent selected body count"
    );
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn hints_native_cache_and_identity() {
    run_family(0, 27)
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn snapshots_native_version_and_cooldown() {
    run_family(27, 60)
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn positions_native_selection_and_clamping() {
    run_family(60, 82)
}
