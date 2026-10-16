use safe_cex_venue_lighter::{
    sdk_account_api::AccountValue, transaction_signing::*, ws_private_warnings::*,
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
                "/scripts/private-warnings-oracle.cjs"
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
fn text(v: &Value) -> Vec<u16> {
    units(&v["units"])
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
fn bool_(v: &Value) -> bool {
    v["value"].as_bool().unwrap()
}
struct Effect {
    kind: &'static str,
    message: Vec<u16>,
    force_toast: bool,
}
struct Host {
    input: Value,
    trace: RefCell<Vec<String>>,
    effects: RefCell<Vec<Effect>>,
    token: HostThrownValue,
    now: RefCell<f64>,
    flags: RefCell<(bool, f64)>,
    param_counts: RefCell<BTreeMap<&'static str, usize>>,
}
impl Host {
    fn step(&self, label: &str) -> SigningResult<()> {
        self.trace.borrow_mut().push(label.into());
        if self.input["mutateAt"]
            .get("units")
            .is_some_and(|v| units(v) == label.encode_utf16().collect::<Vec<_>>())
        {
            if text(&self.input["mutateKind"]) == "readonly".encode_utf16().collect::<Vec<_>>() {
                self.flags.borrow_mut().0 = true;
            } else {
                self.flags.borrow_mut().1 = 1.0;
            }
        }
        if self.input["throwAt"]
            .get("units")
            .is_some_and(|v| units(v) == label.encode_utf16().collect::<Vec<_>>())
        {
            Err(self.token.clone().into())
        } else {
            Ok(())
        }
    }
    fn steps(&self, labels: &[&str]) -> SigningResult<()> {
        for label in labels {
            self.step(label)?;
        }
        Ok(())
    }
    fn option(&self, key: &str) -> Option<bool> {
        if self.input[key]["kind"] == "undefined" {
            None
        } else {
            Some(bool_(&self.input[key]))
        }
    }
    fn param(&self, key: &'static str, label: &str) -> SigningResult<bool> {
        self.step(label)?;
        let mut counts = self.param_counts.borrow_mut();
        let index = counts.entry(key).or_default();
        let values = self.input[key].as_array().unwrap();
        let value = values[(*index).min(values.len() - 1)].as_bool().unwrap();
        *index += 1;
        Ok(value)
    }
}
impl PrivateWarningHost for Host {
    fn now_ms(&self) -> SigningResult<f64> {
        self.step("clock")?;
        Ok(*self.now.borrow())
    }
    fn network(&self) -> SigningResult<Vec<u16>> {
        self.steps(&["profile.get", "network"])?;
        Ok(text(&self.input["network"]))
    }
    fn account_index(&self) -> SigningResult<Vec<u16>> {
        self.steps(&["account.get", "account.call"])?;
        Ok(text(&self.input["account"]))
    }
    fn rate_limit_guidance(&self) -> SigningResult<Vec<u16>> {
        self.steps(&["guidance.get", "guidance.call"])?;
        Ok(text(&self.input["guidance"]))
    }
    fn supports_ws_proxy(&self) -> SigningResult<bool> {
        self.steps(&["profile.get", "supports"])?;
        Ok(bool_(&self.input["supports"]))
    }
    fn ws_route_option(&self) -> SigningResult<Option<bool>> {
        self.steps(&["options.get", "route"])?;
        Ok(self.option("route"))
    }
    fn testnet_option(&self) -> SigningResult<Option<bool>> {
        self.steps(&["options.get", "testnet"])?;
        Ok(self.option("testnet"))
    }
    fn resolve_private_host(&self) -> SigningResult<AccountValue> {
        self.steps(&["name", "options.get", "resolve", "private.get", "host"])?;
        let v = &self.input["host"];
        Ok(match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "string" => AccountValue::String(text(v)),
            "number" => AccountValue::Number(number(v)),
            _ => panic!("host witness tag"),
        })
    }
    fn log_warning(&self, message: &[u16]) -> SigningResult<()> {
        self.steps(&["log.get", "log.call"])?;
        self.effects.borrow_mut().push(Effect {
            kind: "log",
            message: message.to_vec(),
            force_toast: false,
        });
        Ok(())
    }
    fn emit_warning(&self, message: &[u16], force_toast: bool) -> SigningResult<()> {
        self.steps(&["emitter.get", "emit.get", "emit.call"])?;
        self.effects.borrow_mut().push(Effect {
            kind: "emit",
            message: message.to_vec(),
            force_toast,
        });
        Ok(())
    }
}
impl PrivateWarningFlags for Host {
    fn readonly_fallback(&self) -> SigningResult<bool> {
        self.step("state.readonly")?;
        Ok(self.flags.borrow().0)
    }
    fn readonly_failures(&self) -> SigningResult<f64> {
        self.step("state.failures")?;
        Ok(self.flags.borrow().1)
    }
}
impl PrivateStreamWarningParams for Host {
    fn policy_restriction(&self) -> SigningResult<bool> {
        self.param("policy", "params.policy")
    }
    fn rate_limit(&self) -> SigningResult<bool> {
        self.param("rate", "params.rate")
    }
    fn error_message(&self) -> SigningResult<Vec<u16>> {
        self.step("params.message")?;
        Ok(text(&self.input["errMsg"]))
    }
}
fn run_rows(state_family: bool) {
    let rows = capture()["cases"].as_array().unwrap();
    let mut counts = BTreeMap::new();
    for (iteration, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        let is_state = matches!(
            method,
            "notePrivateStreamRateLimitError" | "clearPrivateStreamRateLimitErrorState"
        );
        if is_state != state_family {
            continue;
        }
        let input = &row["input"];
        let host = Host {
            input: input.clone(),
            trace: RefCell::new(vec![]),
            effects: RefCell::new(vec![]),
            token: HostThrownValue::new(vec![0xd800u16]),
            now: RefCell::new(number(&input["now"])),
            flags: RefCell::new((bool_(&input["readonly"]), number(&input["failures"]))),
            param_counts: RefCell::new(BTreeMap::new()),
        };
        let mut state = PrivateWarningState {
            use_readonly_fallback: bool_(&input["readonly"]),
            readonly_fallback_failures: number(&input["failures"]),
            rate_limit_error_count: number(&input["count"]),
            rate_limit_first_seen_at: number(&input["first"]),
        };
        let mut key = text(&input["network"]);
        key.push(58);
        key.extend(text(&input["account"]));
        let times: ReadonlyWarningTimes = Rc::new(RefCell::new(HashMap::new()));
        if input["initial"]["kind"] != "undefined" {
            times.borrow_mut().insert(key, number(&input["initial"]));
        }
        let result: SigningResult<AccountValue> = match method {
            "getRateLimitWarningMessage" => {
                get_rate_limit_warning_message(&host).map(AccountValue::String)
            }
            "getReadonlyFallbackWarningKey" => {
                get_readonly_fallback_warning_key(&host).map(AccountValue::String)
            }
            "emitReadonlyFallbackWarning" => {
                emit_readonly_fallback_warning(&host, &times, bool_(&input["log"]))
                    .map(|_| AccountValue::Undefined)
            }
            "isWsProxyRouteEnabled" => is_ws_proxy_route_enabled(&host).map(AccountValue::Boolean),
            "shouldUseGeorestrictedConnectivityWarning" => {
                should_use_georestricted_connectivity_warning(
                    &host,
                    &host,
                    input["policy"][0].as_bool().unwrap(),
                )
                .map(AccountValue::Boolean)
            }
            "getPrivateStreamErrorWarningMessage" => {
                get_private_stream_error_warning_message(&host, &host, &host)
                    .map(AccountValue::String)
            }
            "notePrivateStreamRateLimitError" => {
                state.note_private_stream_rate_limit_error(number(&input["now"]));
                Ok(AccountValue::Undefined)
            }
            "clearPrivateStreamRateLimitErrorState" => {
                state.clear_private_stream_rate_limit_error_state();
                Ok(AccountValue::Undefined)
            }
            "sharedWarning" => (|| {
                emit_readonly_fallback_warning(&host, &times, false)?;
                *host.now.borrow_mut() += 1.0;
                let other_instance_times = times.clone();
                emit_readonly_fallback_warning(&host, &other_instance_times, true)?;
                Ok(AccountValue::Undefined)
            })(),
            _ => panic!("unknown source method"),
        };
        let label = format!("{method} iteration{iteration}");
        let expected = &row["output"];
        match result {
            Ok(value) => {
                assert_eq!(expected["kind"], "ok", "{label} native result kind");
                match value {
                    AccountValue::String(value) => {
                        assert_eq!(expected["value"]["kind"], "string");
                        assert_eq!(value, text(&expected["value"]), "{label} native UTF16")
                    }
                    AccountValue::Boolean(value) => {
                        assert_eq!(expected["value"]["kind"], "boolean");
                        assert_eq!(value, bool_(&expected["value"]), "{label} native boolean")
                    }
                    AccountValue::Undefined => assert_eq!(
                        expected["value"]["kind"], "undefined",
                        "{label} native undefined"
                    ),
                    _ => panic!("unexpected native result"),
                }
            }
            Err(TransactionSigningError::Thrown(value)) => {
                assert_eq!(expected["kind"], "thrown", "{label} native throw kind");
                assert!(
                    value.ptr_eq(&host.token),
                    "{label} native exception identity"
                );
                assert_eq!(
                    value.value().downcast_ref::<Vec<u16>>().unwrap(),
                    &vec![0xd800],
                    "native exception payload"
                );
            }
            Err(_) => panic!("unexpected generated native error"),
        }
        assert_eq!(
            state.rate_limit_first_seen_at.to_bits(),
            number(&row["state"]["first"]).to_bits(),
            "{label} native first-seen bits"
        );
        assert_eq!(
            state.rate_limit_error_count.to_bits(),
            number(&row["state"]["count"]).to_bits(),
            "{label} native count bits"
        );
        let expected_map = row["map"].as_array().unwrap();
        assert_eq!(
            times.borrow().len(),
            expected_map.len(),
            "source expected native account-map count"
        );
        for entry in expected_map {
            assert_eq!(
                times
                    .borrow()
                    .get(&units(&entry["key"]))
                    .expect("native warning key")
                    .to_bits(),
                number(&entry["value"]).to_bits(),
                "{label} native cooldown timestamp bits"
            );
        }
        let actual = host.effects.borrow();
        let expected_effects = row["effects"].as_array().unwrap();
        assert_eq!(
            actual.len(),
            expected_effects.len(),
            "source expected native effect count"
        );
        for (effect, want) in actual.iter().zip(expected_effects) {
            assert_eq!(
                effect.kind,
                want["kind"].as_str().unwrap(),
                "native effect kind/order"
            );
            assert_eq!(
                effect.message,
                units(&want["message"]),
                "{label} native effect UTF16"
            );
            if effect.kind == "emit" {
                assert_eq!(
                    effect.force_toast,
                    want["forceToast"].as_bool().unwrap(),
                    "native forceToast"
                );
            } else {
                assert_eq!(
                    want["severity"], "warning",
                    "source warning severity contract"
                );
            }
        }
        assert_eq!(
            *host.trace.borrow(),
            row["trace"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "{label} native read/call order"
        );
        *counts.entry(method).or_insert(0usize) += 1;
    }
    assert_eq!(
        rows.len(),
        428,
        "independent corpus floor, stopped at iteration427"
    );
    if state_family {
        assert_eq!(
            counts,
            BTreeMap::from([
                ("notePrivateStreamRateLimitError", 96),
                ("clearPrivateStreamRateLimitErrorState", 8)
            ]),
            "independent rate-state floor, last iteration427"
        );
    } else {
        assert_eq!(
            counts,
            BTreeMap::from([
                ("isWsProxyRouteEnabled", 114),
                ("shouldUseGeorestrictedConnectivityWarning", 28),
                ("getPrivateStreamErrorWarningMessage", 57),
                ("getRateLimitWarningMessage", 3),
                ("getReadonlyFallbackWarningKey", 4),
                ("emitReadonlyFallbackWarning", 117),
                ("sharedWarning", 1)
            ]),
            "independent warning floor, last iteration323"
        );
    }
    assert_eq!(
        capture()["contentHash"],
        "4d15610b78f62faeafd505833d738ab216309648f27a895535d6796ac1586fd3",
        "authored source-only input/result pin after native assertions"
    );
}
#[test]
#[ignore = "actual TypeScript private warning member bodies"]
fn private_warning_native_results_and_effects() {
    run_rows(false)
}
#[test]
#[ignore = "actual TypeScript rate-limit state member bodies"]
fn private_rate_state_native_transitions() {
    run_rows(true)
}
