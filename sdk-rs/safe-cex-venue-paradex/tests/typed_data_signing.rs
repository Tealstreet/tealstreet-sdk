use safe_cex_venue_paradex::typed_data_signing::{
    SigningFuture, TypedDataSigningHost, native_message_hash, native_sign_message,
    sign_paradex_typed_data,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type V = Rc<Value>;
type E = Rc<()>;
const PROFILES: [&str; 16] = [
    "remote",
    "remote-changing",
    "local-object",
    "local-array",
    "empty",
    "three",
    "sparse",
    "mixed",
    "inherited",
    "custom-map",
    "bigint-object",
    "unicode",
    "missing",
    "remote-null",
    "string-replace",
    "bigint-method",
];
const FAILURES: [&str; 28] = [
    "none",
    "get:options:payloadSigner#0",
    "import#0",
    "get:module:typedData#0",
    "get:options:payloadSigner#1",
    "get:options:key#0",
    "get:typedData:getMessageHash#0",
    "get:sdk:account#0",
    "get:account:address#0",
    "hash#0",
    "remote#0",
    "get:account:signMessage#0",
    "sign#0",
    "is-array#0",
    "get:sig:map#0",
    "map#0",
    "string#0",
    "string#1",
    "get:sig:r#0",
    "get:sig:s#0",
    "stringify#0",
    "import-reject#0",
    "sign-reject#0",
    "remote-reject#0",
    "string-select#0",
    "string-select#1",
    "bigint-get#0",
    "bigint-call#0",
];
fn v(value: Value) -> V {
    Rc::new(value)
}
fn s(value: &str) -> V {
    v(json!(value))
}
fn block<T>(future: impl Future<Output = T>) -> T {
    let mut f = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..64 {
        if let Poll::Ready(value) = f.as_mut().poll(&mut cx) {
            return value;
        }
    }
    panic!("signing_native_future_budget");
}
struct Host<'a> {
    profile: &'a str,
    failure: &'a str,
    trace: RefCell<Vec<String>>,
    counts: RefCell<BTreeMap<String, usize>>,
    error: E,
    returned: V,
    typed: V,
    request: RefCell<Option<Value>>,
    local_input: RefCell<bool>,
    crypto: Option<&'a Value>,
    signature: RefCell<Option<Vec<String>>>,
}
impl Host<'_> {
    fn touch(&self, key: &str) -> Result<(), E> {
        let mut counts = self.counts.borrow_mut();
        let n = counts.entry(key.into()).or_default();
        let label = format!("{key}#{n}");
        *n += 1;
        self.trace.borrow_mut().push(label.clone());
        if label == self.failure {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn remote(&self) -> bool {
        self.profile.starts_with("remote")
    }
    fn array_profile(&self) -> bool {
        self.crypto.is_some()
            || matches!(
                self.profile,
                "local-array"
                    | "empty"
                    | "three"
                    | "sparse"
                    | "mixed"
                    | "custom-map"
                    | "unicode"
                    | "bigint-method"
            )
    }
    fn elements(&self) -> Vec<Option<Value>> {
        if let Some(signature) = self.signature.borrow().as_ref() {
            return signature.iter().map(|part| Some(json!(part))).collect();
        }
        match self.profile {
            "bigint-method" => vec![Some(json!({"bigint":"19"})), Some(json!({"bigint":"23"}))],
            "empty" => vec![],
            "three" => vec![
                Some(json!({"bigint":"1"})),
                Some(json!({"bigint":"2"})),
                Some(json!({"bigint":"3"})),
            ],
            "sparse" => vec![
                None,
                Some(json!({"undefined":true})),
                Some(json!({"bigint":"1"})),
                Some(json!({"number":"NaN"})),
            ],
            "mixed" => vec![
                Some(Value::Null),
                Some(json!(true)),
                Some(json!({"undefined":true})),
                Some(json!({"number":"Infinity"})),
                Some(json!(0)),
                Some(json!("quote\"\n")),
            ],
            "unicode" => vec![
                Some(json!({"units":[108,111,110,101,45,55296]})),
                Some(json!("snow-雪")),
            ],
            _ => vec![Some(json!({"bigint":"11"})), Some(json!("0x22"))],
        }
    }
    fn expected_signature(&self) -> &str {
        match self.profile {
            "local-array" => "[\"11\",\"0x22\"]",
            "bigint-method" => "[\"19\",\"23\"]",
            "string-replace" => "[\"old:1\",\"new:2\"]",
            "empty" => "[]",
            "three" => "[\"1\",\"2\",\"3\"]",
            "sparse" => "[null,\"undefined\",\"1\",\"NaN\"]",
            "mixed" => "[\"null\",\"true\",\"undefined\",\"Infinity\",\"0\",\"quote\\\"\\n\"]",
            "inherited" => "[\"31\",\"0x42\"]",
            "custom-map" => "[\"custom\",\"9\"]",
            "bigint-object" => "[\"51\",\"62\"]",
            "unicode" => "[\"lone-\\ud800\",\"snow-雪\"]",
            "missing" => "[\"undefined\",\"undefined\"]",
            _ => "[\"0x11\",\"0x22\"]",
        }
    }
}
impl TypedDataSigningHost for Host<'_> {
    type Value = V;
    type Error = E;
    fn get(&self, value: &V, key: &str) -> Result<V, E> {
        if let Some(big) = value.get("bigint") {
            if self.profile == "bigint-method" {
                self.touch("bigint-get")?;
            }
            assert_eq!(key, "toString");
            return Ok(s(&format!("@big-toString:{}", big.as_str().unwrap())));
        }
        let label = value.as_str().unwrap();
        if label != "@bigint" {
            self.touch(&format!("get:{}:{key}", label.trim_start_matches('@')))?;
        }
        Ok(match (label, key) {
            ("@options", "payloadSigner") => {
                if self.remote() {
                    if self.profile == "remote-changing"
                        && self.counts.borrow().get("get:options:payloadSigner") == Some(&1)
                    {
                        s("@truthy-first")
                    } else {
                        s("@remote-method")
                    }
                } else {
                    v(json!({"undefined":true}))
                }
            }
            ("@options", "key") => s(self
                .crypto
                .map_or("key-route", |c| c["key"].as_str().unwrap())),
            ("@module", "typedData") => s("@typedData"),
            ("@typedData", "getMessageHash") => s("@hash-method"),
            ("@sdk", "account") => s("@account"),
            ("@account", "address") => s(self
                .crypto
                .map_or("0x1234", |c| c["address"].as_str().unwrap())),
            ("@account", "signMessage") => s("@sign-method"),
            ("@sig", "map") => s("@map-method"),
            ("@sig", "r" | "s") => {
                let r = key == "r";
                match self.profile {
                    "string-replace" => v(json!(if r { 1 } else { 2 })),
                    "missing" => v(json!({"undefined":true})),
                    "inherited" => v(if r {
                        json!({"bigint":"31"})
                    } else {
                        json!("0x42")
                    }),
                    "bigint-object" => v(json!({"bigint":if r{"51"}else{"62"}})),
                    _ => s(if r { "0x11" } else { "0x22" }),
                }
            }
            _ => panic!("native unexpected get {label} {key}"),
        })
    }
    fn truthy(&self, value: &V) -> bool {
        matches!(value.as_str(), Some("@remote-method" | "@truthy-first"))
    }
    fn literal(&self, text: &str) -> V {
        s(text)
    }
    fn object(&self, fields: Vec<(&str, V)>) -> Result<V, E> {
        if fields
            .first()
            .is_some_and(|(key, _)| *key == "exchangeName")
        {
            assert_eq!(
                fields.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
                vec!["exchangeName", "key", "scheme", "encoding", "payload"],
                "remote_native_property_order"
            );
        }
        Ok(v(Value::Object(
            fields
                .into_iter()
                .map(|(k, v)| (k.into(), (*v).clone()))
                .collect(),
        )))
    }
    fn array(&self, values: Vec<V>) -> Result<V, E> {
        Ok(v(Value::Array(
            values.into_iter().map(|v| (*v).clone()).collect(),
        )))
    }
    fn import_starknet(&self) -> Result<V, E> {
        self.touch("import")?;
        Ok(s("@import"))
    }
    fn await_value(&self, value: V) -> SigningFuture<'_, V, E> {
        Box::pin(async move {
            let stage = match value.as_str() {
                Some("@import") => "import",
                Some("@signed") => "sign",
                Some("@returned") => "remote",
                _ => panic!("unexpected await"),
            };
            let fault = format!("{stage}-reject");
            if self.failure == format!("{fault}#0") {
                self.touch(&fault)?;
            }
            Ok(match stage {
                "import" => s("@module"),
                "sign" => {
                    if let Some(c) = self.crypto {
                        self.signature.replace(Some(
                            native_sign_message(
                                c["raw"].as_str().unwrap(),
                                c["address"].as_str().unwrap(),
                                c["key"].as_str().unwrap(),
                            )
                            .unwrap(),
                        ));
                    }
                    s("@sig")
                }
                _ => {
                    if self.profile == "remote-null" {
                        v(Value::Null)
                    } else {
                        self.returned.clone()
                    }
                }
            })
        })
    }
    fn call(&self, method: V, receiver: &V, args: Vec<V>) -> Result<V, E> {
        let method = method.as_str().unwrap();
        if let Some(big) = method.strip_prefix("@big-toString:") {
            if self.profile == "bigint-method" {
                self.touch("bigint-call")?;
            }
            assert_eq!(
                receiver.get("bigint").and_then(Value::as_str),
                Some(big),
                "bigint_native_receiver"
            );
            assert!(args.is_empty(), "bigint_native_arguments");
            return Ok(s(big));
        }
        assert_ne!(method, "@truthy-first", "remote_native_selected_method");
        match method {
            "@hash-method" => {
                self.touch("hash")?;
                assert_eq!(
                    receiver.as_str(),
                    Some("@typedData"),
                    "hash_native_receiver"
                );
                assert!(Rc::ptr_eq(&args[0], &self.typed), "hash_native_input");
                assert_eq!(
                    *args[1],
                    json!(
                        self.crypto
                            .map_or("0x1234", |c| c["address"].as_str().unwrap())
                    ),
                    "hash_native_address"
                );
                Ok(s(&self.crypto.map_or_else(
                    || "0xabcd".into(),
                    |c| {
                        native_message_hash(c["raw"].as_str().unwrap(), args[1].as_str().unwrap())
                            .unwrap()
                    },
                )))
            }
            "@remote-method" => {
                self.touch("remote")?;
                assert_eq!(
                    receiver.as_str(),
                    Some("@options"),
                    "remote_native_receiver"
                );
                self.request.replace(Some((*args[0]).clone()));
                Ok(s("@returned"))
            }
            "@sign-method" => {
                self.touch("sign")?;
                assert_eq!(receiver.as_str(), Some("@account"), "local_native_receiver");
                let same = Rc::ptr_eq(&args[0], &self.typed);
                assert!(same, "local_native_input");
                self.local_input.replace(same);
                Ok(s("@signed"))
            }
            _ => panic!("unexpected method"),
        }
    }
    fn is_array(&self, _: &V) -> Result<bool, E> {
        self.touch("is-array")?;
        Ok(self.array_profile())
    }
    fn is_bigint(&self, value: &V) -> bool {
        value.get("bigint").is_some()
    }
    fn select_string(&self) -> Result<V, E> {
        self.touch("string-select")?;
        Ok(s(if self.profile == "string-replace" {
            if self.counts.borrow().contains_key("get:sig:r") {
                "@string-new"
            } else {
                "@string-old"
            }
        } else {
            "@string-plain"
        }))
    }
    fn string(&self, method: V, value: V) -> Result<V, E> {
        self.touch("string")?;
        assert!(
            matches!(
                method.as_str(),
                Some("@string-new" | "@string-old" | "@string-plain")
            ),
            "string_native_selected_method"
        );
        let result = if let Some(text) = value.get("bigint") {
            v(text.clone())
        } else if value.get("undefined").is_some() {
            s("undefined")
        } else if let Some(text) = value.get("number") {
            v(text.clone())
        } else if value.get("units").is_some() || value.is_string() {
            value
        } else {
            s(&value.to_string())
        };
        Ok(match method.as_str().unwrap() {
            "@string-old" => s(&format!("old:{}", result.as_str().unwrap())),
            "@string-new" => s(&format!("new:{}", result.as_str().unwrap())),
            _ => result,
        })
    }
    fn stringify(&self, value: V) -> Result<V, E> {
        self.touch("stringify")?;
        let parts: Vec<String> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                if v.get("units").is_some() {
                    "\"lone-\\ud800\"".into()
                } else {
                    v.to_string()
                }
            })
            .collect();
        Ok(s(&format!("[{}]", parts.join(","))))
    }
    fn map(
        &self,
        method: V,
        receiver: &V,
        callback: &mut dyn FnMut(V) -> Result<V, E>,
    ) -> Result<V, E> {
        assert_eq!(method.as_str(), Some("@map-method"), "map_native_method");
        assert_eq!(receiver.as_str(), Some("@sig"), "map_native_receiver");
        self.touch("map")?;
        if self.profile == "custom-map" {
            return Ok(v(json!(["custom", &*callback(v(json!({"bigint":"9"})))?])));
        }
        self.touch("get:sig:length")?;
        self.touch("get:sig:constructor")?;
        let mut out = vec![];
        for (i, value) in self.elements().into_iter().enumerate() {
            if let Some(value) = value {
                self.touch(&format!("get:sig:{i}"))?;
                let value = v(value);
                out.push((*callback(value)?).clone());
            } else {
                out.push(Value::Null);
            }
        }
        Ok(v(json!(out)))
    }
}
fn assert_native_trace(h: &Host<'_>) {
    let mut keys = vec!["get:options:payloadSigner".to_owned()];
    if h.remote() {
        keys.extend(
            [
                "import",
                "get:module:typedData",
                "get:options:payloadSigner",
                "get:options:key",
                "get:typedData:getMessageHash",
                "get:sdk:account",
                "get:account:address",
                "hash",
                "remote",
            ]
            .map(str::to_owned),
        );
    } else {
        keys.extend(
            [
                "get:sdk:account",
                "get:account:signMessage",
                "sign",
                "is-array",
            ]
            .map(str::to_owned),
        );
        if h.array_profile() {
            keys.extend(["get:sig:map", "map"].map(str::to_owned));
            if h.profile != "custom-map" {
                keys.extend(["get:sig:length", "get:sig:constructor"].map(str::to_owned));
                for (i, value) in h.elements().iter().enumerate() {
                    if let Some(value) = value {
                        keys.push(format!("get:sig:{i}"));
                        if value.get("bigint").is_none() {
                            keys.extend(["string-select".into(), "string".into()]);
                        } else if h.profile == "bigint-method" {
                            keys.extend(["bigint-get".into(), "bigint-call".into()]);
                        }
                    }
                }
            }
        } else {
            keys.extend(
                [
                    "string-select",
                    "get:sig:r",
                    "string",
                    "string-select",
                    "get:sig:s",
                    "string",
                ]
                .map(str::to_owned),
            );
        }
        keys.push("stringify".into());
    }
    let rejection = h
        .failure
        .strip_suffix("#0")
        .filter(|key| key.ends_with("-reject"));
    if let Some(rejection) = rejection {
        let site = rejection.trim_end_matches("-reject");
        if let Some(i) = keys.iter().position(|key| key == site) {
            keys.insert(i + 1, rejection.into());
        }
    }
    let mut counts = BTreeMap::<String, usize>::new();
    let mut expected = vec![];
    for key in keys {
        let n = counts.entry(key.clone()).or_default();
        let label = format!("{key}#{n}");
        *n += 1;
        expected.push(label.clone());
        if label == h.failure {
            break;
        }
    }
    assert_eq!(
        *h.trace.borrow(),
        expected,
        "signing_native_order {} {}",
        h.profile,
        h.failure
    );
}
#[test]
fn source_oracle() {
    let mut receipts = vec![];
    for profile in PROFILES {
        for failure in FAILURES {
            let h = Host {
                profile,
                failure,
                trace: RefCell::new(vec![]),
                counts: RefCell::new(BTreeMap::new()),
                error: Rc::new(()),
                returned: v(json!({"marker":"remote-result","headers":{"extra":"retained"}})),
                typed: v(json!({"marker":"typed"})),
                request: RefCell::new(None),
                local_input: RefCell::new(false),
                crypto: None,
                signature: RefCell::new(None),
            };
            let result = block(sign_paradex_typed_data(
                &h,
                &s("@sdk"),
                &h.typed,
                &s("@options"),
            ));
            assert_native_trace(&h);
            let reached = h.trace.borrow().iter().any(|label| label == failure);
            let outcome = match result {
                Err(error) => {
                    assert!(reached, "failure_native_reachability");
                    assert!(Rc::ptr_eq(&error, &h.error), "error_native_identity");
                    json!({"ok":false,"original":true})
                }
                Ok(value) => {
                    assert!(!reached, "failure_native_outcome");
                    if h.remote() {
                        if profile == "remote-null" {
                            assert!(value.is_null(), "remote_native_null");
                        } else {
                            assert!(Rc::ptr_eq(&value, &h.returned), "remote_native_identity");
                        }
                        json!({"ok":true,"remote":profile!="remote-null","null":profile=="remote-null"})
                    } else {
                        assert_eq!(
                            value["signature"],
                            h.expected_signature(),
                            "signature_native_value {profile}"
                        );
                        json!({"ok":true,"remote":false,"null":false,"signature":value["signature"].as_str().unwrap().encode_utf16().collect::<Vec<_>>()})
                    }
                }
            };
            if let Some(request) = h.request.borrow().as_ref() {
                assert_eq!(
                    request,
                    &json!({"exchangeName":"paradex","key":"key-route","scheme":"stark","encoding":"felt-pair","payload":"0xabcd"}),
                    "remote_native_request"
                );
            }
            receipts.push(json!({"profile":profile,"failure":failure,"trace":*h.trace.borrow(),"request":*h.request.borrow(),"localInput":*h.local_input.borrow(),"outcome":outcome}));
        }
    }
    assert_eq!(receipts.len(), 448, "native_case_floor");
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/typed_data_signing_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["profiles"], json!(PROFILES));
    assert_eq!(source["failures"], json!(FAILURES));
    assert_eq!(source["cases"].as_array().unwrap().len(), 448);
    assert_eq!(source["crypto"].as_array().unwrap().len(), 12);
    let mut expected = std::collections::BTreeSet::new();
    for address in ["0x1234", "0x5678"] {
        for key in ["0x123", "0x456"] {
            for kind in ["Order", "ModifyOrder", "Request"] {
                expected.insert(vec![address.to_owned(), key.to_owned(), kind.to_owned()]);
            }
        }
    }
    let inventory: std::collections::BTreeSet<_> = source["crypto"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let data: Value = serde_json::from_str(c["raw"].as_str().unwrap()).unwrap();
            vec![
                c["address"].as_str().unwrap().to_owned(),
                c["key"].as_str().unwrap().to_owned(),
                data["primaryType"].as_str().unwrap().to_owned(),
            ]
        })
        .collect();
    assert_eq!(inventory, expected, "crypto_native_declared_inventory");
    for case in source["crypto"].as_array().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        let address = case["address"].as_str().unwrap();
        let key = case["key"].as_str().unwrap();
        for profile in ["remote", "local-array"] {
            let h = Host {
                profile,
                failure: "none",
                trace: RefCell::new(vec![]),
                counts: RefCell::new(BTreeMap::new()),
                error: Rc::new(()),
                returned: v(json!({"native_remote":true})),
                typed: v(serde_json::from_str(raw).unwrap()),
                request: RefCell::new(None),
                local_input: RefCell::new(false),
                crypto: Some(case),
                signature: RefCell::new(None),
            };
            let result = block(sign_paradex_typed_data(
                &h,
                &s("@sdk"),
                &h.typed,
                &s("@options"),
            ))
            .unwrap();
            if profile == "remote" {
                assert!(
                    Rc::ptr_eq(&result, &h.returned),
                    "crypto_adapter_native_remote_identity"
                );
                assert_eq!(
                    h.request.borrow().as_ref().unwrap(),
                    &case["request"],
                    "crypto_adapter_native_request"
                );
            } else {
                assert!(*h.local_input.borrow(), "crypto_adapter_native_input");
                assert_eq!(
                    result["signature"], case["signature"],
                    "crypto_adapter_native_signature"
                );
            }
        }
        let hash = native_message_hash(raw, address).unwrap();
        assert_eq!(hash, case["hash"], "crypto_native_hash");
        let sig = native_sign_message(raw, address, key).unwrap();
        assert_eq!(sig.len(), 2, "crypto_native_pair_length");
        assert_ne!(sig[0], sig[1], "crypto_native_distinct_fields");
        assert_eq!(
            serde_json::to_string(&sig).unwrap(),
            case["signature"],
            "crypto_native_signature"
        );
        assert_ne!(
            native_message_hash(raw, "0x9999").unwrap(),
            hash,
            "crypto_native_address_separation"
        );
        assert_ne!(
            native_sign_message(raw, address, "0x789").unwrap(),
            sig,
            "crypto_native_key_separation"
        );
    }
    for i in 0..448 {
        assert_eq!(
            receipts[i], source["cases"][i],
            "supplemental_source_receipt {i}"
        );
    }
}
