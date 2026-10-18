use safe_cex_venue_lighter::{
    exchange_error_recovery::*,
    sdk_account_api::{AccountObject, AccountValue},
    transaction_builders::TransactionBuildError,
    transaction_signing::{HostThrownValue, SigningResult, TransactionSigningError},
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
        .map(|x| x.as_u64().unwrap() as u16)
        .collect()
}
struct Marker;
impl AccountObject for Marker {
    fn get(&self, _: &str) -> SigningResult<AccountValue> {
        Ok(AccountValue::Undefined)
    }
}
struct Invocation {
    name: String,
    receiver: AccountValue,
    args: Vec<AccountValue>,
}
struct Env {
    input: Value,
    objects: BTreeMap<String, Rc<dyn AccountObject>>,
    symbol: HostThrownValue,
    function: HostThrownValue,
    token: HostThrownValue,
    alternate: HostThrownValue,
    state: RefCell<BTreeMap<String, AccountValue>>,
    props: RefCell<BTreeMap<String, Vec<(String, AccountValue)>>>,
    trace: RefCell<Vec<Vec<u16>>>,
    seen: RefCell<BTreeMap<String, usize>>,
    calls: RefCell<Vec<Invocation>>,
}
#[derive(Clone)]
struct Fixture(Rc<Env>);
impl Fixture {
    fn obj(&self, id: &str) -> AccountValue {
        AccountValue::Object(self.0.objects[id].clone())
    }
    fn id(&self, v: &AccountValue) -> String {
        let AccountValue::Object(v) = v else {
            panic!("native object required")
        };
        self.0
            .objects
            .iter()
            .find(|(_, a)| Rc::ptr_eq(v, a))
            .unwrap()
            .0
            .clone()
    }
    fn decode(&self, e: &Value) -> AccountValue {
        match e["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(e["value"].as_bool().unwrap()),
            "number" => AccountValue::Number(f64::from_bits(
                u64::from_str_radix(e["bits"].as_str().unwrap(), 16).unwrap(),
            )),
            "string" => AccountValue::String(units(&e["units"])),
            "bigint" => AccountValue::BigInt(e["digits"].as_str().unwrap().parse().unwrap()),
            "symbol" => AccountValue::Symbol(self.0.symbol.clone()),
            "function" => AccountValue::Function(self.0.function.clone()),
            "object" => self.obj(e["id"].as_str().unwrap()),
            _ => panic!("source native kind"),
        }
    }
    fn val(&self, name: &str) -> AccountValue {
        self.decode(&self.0.input[name])
    }
    fn flag(&self, name: &str) -> bool {
        self.0.input[name]["value"] == true
    }
    fn input_text(&self, name: &str) -> Option<Vec<u16>> {
        if self.0.input[name]["kind"] == "string" {
            Some(units(&self.0.input[name]["units"]))
        } else {
            None
        }
    }
    fn read(&self, id: &str, key: &str) -> AccountValue {
        if id == "Owner" {
            return self.0.state.borrow()[key].clone();
        }
        self.0.props.borrow()[id]
            .iter()
            .find(|x| x.0 == key)
            .map(|x| x.1.clone())
            .unwrap_or(AccountValue::Undefined)
    }
    fn write(&self, id: &str, key: &str, value: AccountValue) {
        if id == "Owner" {
            self.0.state.borrow_mut().insert(key.into(), value);
            return;
        }
        let mut p = self.0.props.borrow_mut();
        let fields = p.get_mut(id).unwrap();
        if let Some(x) = fields.iter_mut().find(|x| x.0 == key) {
            x.1 = value
        } else {
            fields.push((key.into(), value))
        }
    }
    fn trace(&self, event: String) -> SigningResult<()> {
        self.0.trace.borrow_mut().push(u(&event));
        let nth = {
            let mut seen = self.0.seen.borrow_mut();
            let n = seen.entry(event.clone()).or_insert(0);
            *n += 1;
            *n
        };
        if self.input_text("changeAt") == Some(u(&event)) {
            let target = if let AccountValue::Number(n) = self.val_optional("changeNth") {
                n as usize
            } else {
                1
            };
            if nth == target {
                match self.input_text("change").as_deref() {
                    Some(x) if x == u("errorMessage") => {
                        self.write("Err", "message", self.val("nextMessage"))
                    }
                    Some(x) if x == u("signerMessage") => {
                        self.write("Owner", "signerInitErrorMessage", self.val("nextMessage"))
                    }
                    Some(x) if x == u("snapshotTime") => self.write(
                        "Owner",
                        "lastTransientSnapshotWarningAt",
                        self.val("nextTime"),
                    ),
                    _ => panic!("native change"),
                }
            }
        }
        if self.input_text("throwAt") == Some(u(&event))
            || self.flag("jsonThrow") && event == "call:JSON.stringify"
            || self.flag("jsonGetThrow") && event == "get:JSON.stringify"
        {
            return Err(TransactionSigningError::Thrown(self.0.token.clone()));
        }
        if self.flag("stringThrow") && event == "call:String" {
            return Err(TransactionSigningError::Thrown(self.0.alternate.clone()));
        }
        Ok(())
    }
    fn val_optional(&self, name: &str) -> AccountValue {
        if self.0.input[name].is_null() {
            AccountValue::Undefined
        } else {
            self.val(name)
        }
    }
    fn render(&self, value: &AccountValue, template: bool) -> SigningResult<Vec<u16>> {
        Ok(match value {
            AccountValue::Undefined => u("undefined"),
            AccountValue::Null => u("null"),
            AccountValue::Boolean(x) => u(if *x { "true" } else { "false" }),
            AccountValue::Number(x) => u(&safe_cex_num::js_number_string(*x)),
            AccountValue::BigInt(x) => u(&x.to_string()),
            AccountValue::String(x) => x.clone(),
            AccountValue::Symbol(_) => {
                if template {
                    return Err(TransactionSigningError::Generated(TransactionBuildError {
                        name: "TypeError",
                        message: u("Cannot convert a Symbol value to a string"),
                    }));
                }
                let mut s = u("Symbol(sym/");
                s.push(0xd800);
                s.extend(u(")"));
                s
            }
            AccountValue::Function(_) => u("function paramFunction(){}"),
            AccountValue::Object(_) => {
                let id = self.id(value);
                self.trace(format!("get:{id}:Symbol(Symbol.toPrimitive)"))?;
                self.trace(format!("get:{id}:toString"))?;
                if id == "Err" {
                    let name = self.get(value, "name")?;
                    let message = self.get(value, "message")?;
                    let name = self.render(&name, false)?;
                    let message = self.render(&message, false)?;
                    if name.is_empty() {
                        message
                    } else if message.is_empty() {
                        name
                    } else {
                        let mut s = name;
                        s.extend(u(": "));
                        s.extend(message);
                        s
                    }
                } else {
                    self.trace(format!("get:{id}:Symbol(Symbol.toStringTag)"))?;
                    u("[object Object]")
                }
            }
            _ => panic!("fixture coercion domain"),
        })
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
            (AccountValue::BigInt(a), "bigint") => assert_eq!(
                *a,
                e["digits"]
                    .as_str()
                    .unwrap()
                    .parse::<num_bigint::BigInt>()
                    .unwrap(),
                "{label} native BigInt"
            ),
            (AccountValue::Symbol(a), "symbol") => {
                assert!(a.ptr_eq(&self.0.symbol), "{label} native symbol identity")
            }
            (AccountValue::Function(a), "function") => assert!(
                a.ptr_eq(&self.0.function),
                "{label} native function identity"
            ),
            (AccountValue::Object(a), "object") => assert!(
                Rc::ptr_eq(a, &self.0.objects[e["id"].as_str().unwrap()]),
                "{label} native object identity"
            ),
            _ => panic!("{label} native kind mismatch"),
        }
    }
    fn assert_output(&self, a: SigningResult<AccountValue>, e: &Value, label: &str) {
        match (a, e["kind"].as_str().unwrap()) {
            (Ok(a), "ok") => self.assert_value(&a, &e["value"], &format!("{label} result")),
            (Err(TransactionSigningError::Thrown(a)), "thrown") => {
                assert!(a.ptr_eq(&self.0.token), "{label} native thrown identity")
            }
            (Err(TransactionSigningError::Thrown(a)), "alternate") => assert!(
                a.ptr_eq(&self.0.alternate),
                "{label} alternate native thrown identity"
            ),
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
                )
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
fn number(v: AccountValue) -> f64 {
    let AccountValue::Number(v) = v else {
        panic!("fixture Number")
    };
    v
}
fn case(s: &[u16], upper: bool) -> Vec<u16> {
    s.iter()
        .map(|x| {
            if upper && *x >= 97 && *x <= 122 {
                x - 32
            } else if !upper && *x >= 65 && *x <= 90 {
                x + 32
            } else {
                *x
            }
        })
        .collect()
}
impl ErrorRecoveryCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        let h = &self.host;
        h.0.calls.borrow_mut().push(Invocation {
            name: self.name.clone(),
            receiver: self.receiver.clone(),
            args: args.to_vec(),
        });
        h.trace(format!("call:{}", self.name))?;
        Ok(match self.name.as_str() {
            "virtualClock.getCurrentTimeMs" => h.val("now"),
            "isWasmReady" => AccountValue::Boolean(h.flag("ready")),
            "String" => AccountValue::String(h.render(&args[0], false)?),
            "JSON.stringify" => {
                if h.flag("jsonUndefined") {
                    AccountValue::Undefined
                } else {
                    let mut s = u("wire/");
                    s.push(0xd800);
                    AccountValue::String(s)
                }
            }
            "Math.max" => AccountValue::Number(safe_cex_num::math_max(
                number(args[0].clone()),
                number(args[1].clone()),
            )),
            "Math.ceil" => AccountValue::Number(number(args[0].clone()).ceil()),
            "String.toUpperCase" | "String.toLowerCase" => {
                let AccountValue::String(s) = &self.receiver else {
                    panic!("fixture String receiver")
                };
                AccountValue::String(case(s, self.name.ends_with("UpperCase")))
            }
            "Owner.getCachedAccountTier" => h.val("tier"),
            "Owner.isWasmRuntimeIssueMessage" => AccountValue::Boolean(if h.flag("composeIssue") {
                let lowered = h.member(&args[0], "toLowerCase")?.call(&[])?;
                let AccountValue::String(lower) = lowered else {
                    panic!("fixture typed issue")
                };
                [
                    "wasm not initialized",
                    "failed to fetch dynamically imported module",
                    "signerclient export not found",
                    "leverage signer unavailable in runtime",
                    "node:fs",
                ]
                .iter()
                .any(|p| lower.windows(p.len()).any(|s| s == u(p)))
            } else {
                h.flag("issue")
            }),
            "Owner.extractOrderErrorMessage" => {
                if h.flag("composeExtract") {
                    extract_order_error_message(h, &args[0])?
                } else {
                    h.val("extracted")
                }
            }
            "Owner.isTransientLighterRestTransportError" => {
                AccountValue::Boolean(if h.flag("composeTransient") {
                    is_transient_lighter_rest_transport_error(h, &args[0])?
                } else {
                    h.flag("transient")
                })
            }
            _ => AccountValue::Undefined,
        })
    }
}
impl ExchangeErrorRecoveryHost for Fixture {
    fn owner(&self) -> AccountValue {
        self.obj("Owner")
    }
    fn get(&self, obj: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        if matches!(obj, AccountValue::Object(_)) {
            let id = self.id(obj);
            self.trace(format!("get:{id}:{key}"))?;
            Ok(self.read(&id, key))
        } else {
            Ok(AccountValue::Undefined)
        }
    }
    fn set(&self, obj: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()> {
        let id = self.id(obj);
        self.trace(format!("set:{id}:{key}"))?;
        self.write(&id, key, value);
        Ok(())
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn ErrorRecoveryCall>> {
        self.trace(format!("get:{name}"))?;
        let receiver = if let Some((id, _)) = name.split_once('.') {
            self.obj(id)
        } else {
            AccountValue::Undefined
        };
        Ok(Rc::new(Call {
            host: self.clone(),
            name: name.into(),
            receiver,
        }))
    }
    fn member(&self, obj: &AccountValue, key: &str) -> SigningResult<Rc<dyn ErrorRecoveryCall>> {
        if matches!(obj, AccountValue::Undefined) {
            return Err(TransactionSigningError::Generated(TransactionBuildError {
                name: "TypeError",
                message: u("Cannot read properties of undefined (reading 'toLowerCase')"),
            }));
        }
        let name = if matches!(obj, AccountValue::String(_)) {
            format!("String.{key}")
        } else {
            format!("{}.{key}", self.id(obj))
        };
        self.trace(format!("get:{name}"))?;
        Ok(Rc::new(Call {
            host: self.clone(),
            name,
            receiver: obj.clone(),
        }))
    }
    fn instance_of_error(&self, value: &AccountValue) -> SigningResult<bool> {
        self.trace("get:Error".into())?;
        if matches!(value, AccountValue::Object(_)) {
            let id = self.id(value);
            self.trace(format!("proto:{id}"))?;
            Ok(id == "Err")
        } else {
            Ok(false)
        }
    }
    fn timeout_regex_test(&self, value: &AccountValue) -> SigningResult<bool> {
        self.trace("get:RegExp.test".into())?;
        self.0.calls.borrow_mut().push(Invocation {
            name: "RegExp.test".into(),
            receiver: self.obj("Regex"),
            args: vec![value.clone()],
        });
        self.trace("call:RegExp.test".into())?;
        let AccountValue::String(s) = value else {
            panic!("fixture regex String")
        };
        let s = case(s, false);
        let prefix = u("timeout of ");
        let suffix = u("ms exceeded");
        for i in 0..s.len() {
            if !s[i..].starts_with(&prefix) {
                continue;
            }
            let first = i + prefix.len();
            let mut j = first;
            while j < s.len() && (48..=57).contains(&s[j]) {
                j += 1
            }
            if j > first && s[j..].starts_with(&suffix) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn template_string(&self, value: AccountValue) -> SigningResult<Vec<u16>> {
        self.render(&value, true)
    }
}
fn run(h: &Fixture, method: &str, error: &AccountValue) -> SigningResult<AccountValue> {
    match method {
        "getAccountRateLimitGuidanceMessage" => get_account_rate_limit_guidance_message(h),
        "normalizeSignerErrorMessage" => normalize_signer_error_message(h, error),
        "shouldShortCircuitSignerRetry" => {
            should_short_circuit_signer_retry(h).map(AccountValue::Boolean)
        }
        "getSignerRetryBlockMessage" => get_signer_retry_block_message(h),
        "warnSignerUnavailable" => {
            let AccountValue::String(message) = h.val("message") else {
                unreachable!()
            };
            warn_signer_unavailable(h, &message).map(|_| AccountValue::Undefined)
        }
        "extractOrderErrorMessage" => extract_order_error_message(h, error),
        "isTransientLighterRestTransportError" => {
            is_transient_lighter_rest_transport_error(h, error).map(AccountValue::Boolean)
        }
        "handlePrivateSnapshotFetchError" => {
            handle_private_snapshot_fetch_error(h, error).map(|_| AccountValue::Undefined)
        }
        _ => unreachable!(),
    }
}
#[test]
#[ignore = "executes actual TypeScript at SAFE_CEX_TYPESCRIPT_PATH"]
fn exchange_error_recovery_native_values_getters_cooldowns_and_throws() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-error-recovery-oracle.cjs"
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
        221,
        "independent authored floor stopped iteration220"
    );
    let mut counts = BTreeMap::new();
    let mut serializer_returns = vec![];
    for (i, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        let label = format!("{method} iteration{i}");
        *counts.entry(method).or_insert(0usize) += 1;
        let h = Fixture(Rc::new(Env {
            input: row["input"].clone(),
            objects: [
                "Owner",
                "Raw",
                "Response",
                "Data",
                "Nested",
                "Err",
                "virtualClock",
                "JSON",
                "Math",
                "Regex",
            ]
            .into_iter()
            .map(|id| (id.into(), Rc::new(Marker) as Rc<dyn AccountObject>))
            .collect(),
            symbol: HostThrownValue::new((i, "symbol")),
            function: HostThrownValue::new((i, "function")),
            token: HostThrownValue::new(i),
            alternate: HostThrownValue::new((i, "alternate")),
            state: RefCell::new(BTreeMap::new()),
            props: RefCell::new(BTreeMap::new()),
            trace: RefCell::new(vec![]),
            seen: RefCell::new(BTreeMap::new()),
            calls: RefCell::new(vec![]),
        }));
        for (k, v) in row["stateInput"].as_object().unwrap() {
            h.0.state.borrow_mut().insert(k.clone(), h.decode(v));
        }
        for (id, fields) in row["recordInputs"].as_object().unwrap() {
            h.0.props.borrow_mut().insert(
                id.clone(),
                fields
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| {
                        (
                            e["key"].as_str().unwrap().to_string(),
                            h.decode(&e["value"]),
                        )
                    })
                    .collect(),
            );
        }
        let error = h.decode(&row["error"]);
        let actual = run(&h, method, &error);
        let mut sentinel = u("wire/");
        sentinel.push(0xd800);
        if row["output"]["kind"] == "ok"
            && row["output"]["value"]["kind"] == "string"
            && units(&row["output"]["value"]["units"]) == sentinel
        {
            let Ok(AccountValue::String(actual)) = actual else {
                panic!("{label} native serializer return kind");
            };
            serializer_returns.push((actual, sentinel, label.clone()));
        } else {
            h.assert_output(actual, &row["output"], &label);
        }
        for (k, e) in row["state"].as_object().unwrap() {
            h.assert_value(&h.read("Owner", k), e, &format!("{label} state {k}"));
        }
        for (id, fields) in row["props"].as_object().unwrap() {
            let props = h.0.props.borrow();
            let actual = &props[id];
            let expected = fields.as_array().unwrap();
            assert_eq!(
                actual.len(),
                expected.len(),
                "{label} {id} independent source field count"
            );
            for ((key, a), e) in actual.iter().zip(expected) {
                assert_eq!(
                    key,
                    e["key"].as_str().unwrap(),
                    "{label} {id} native key order"
                );
                h.assert_value(a, &e["value"], &format!("{label} {id}.{key}"));
            }
        }
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
                "Math.max" | "Owner.log" => 2,
                "Math.ceil"
                | "String"
                | "JSON.stringify"
                | "Owner.isWasmRuntimeIssueMessage"
                | "Owner.extractOrderErrorMessage"
                | "Owner.isTransientLighterRestTransportError"
                | "Owner.emitAndLogError"
                | "RegExp.test" => 1,
                _ => 0,
            };
            assert_eq!(a.args.len(), arity, "{label} call{j} authored native arity");
            assert_eq!(
                e["args"].as_array().unwrap().len(),
                arity,
                "source authored arity"
            );
            for (k, (a, e)) in a.args.iter().zip(e["args"].as_array().unwrap()).enumerate() {
                h.assert_value(a, e, &format!("{label} call{j} argument{k}"));
            }
        }
        assert_eq!(
            *h.0.trace.borrow(),
            row["trace"]
                .as_array()
                .unwrap()
                .iter()
                .map(units)
                .collect::<Vec<_>>(),
            "{label} native UTF16 getter callee write order"
        );
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("getAccountRateLimitGuidanceMessage", 8),
            ("normalizeSignerErrorMessage", 21),
            ("shouldShortCircuitSignerRetry", 29),
            ("getSignerRetryBlockMessage", 40),
            ("warnSignerUnavailable", 12),
            ("extractOrderErrorMessage", 34),
            ("isTransientLighterRestTransportError", 40),
            ("handlePrivateSnapshotFetchError", 37)
        ]),
        "independent authored method floors"
    );
    // Fixed serializer sentinel forwarding is deferred until all native checks.
    // The concrete JSON codec is a required host, not proved by this fixture.
    for (actual, expected, label) in serializer_returns {
        assert_eq!(
            actual, expected,
            "{label} serializer return forwarding UTF16"
        );
    }
    assert_eq!(
        corpus["contentHash"], "039f62261ded914f44ed4a0674f3637c06c88e8f6e5c01f777ab52482e199846",
        "full source-only corpus pin after native assertions"
    );
}
