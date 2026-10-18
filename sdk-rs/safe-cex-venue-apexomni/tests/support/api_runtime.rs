use super::*;
use safe_cex_venue_apexomni::api_runtime::{
    ApexApi, ApexApiConfiguration, ApexApiHost, create_api,
};
use std::rc::Weak;
fn api_corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/api_runtime.cjs"
            ))
            .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "API source {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
struct ApiHost {
    runtime: Rc<Host>,
}
fn header_value(value: J) -> Result<J, ExchangeError> {
    Ok(match value {
        J::Null | J::Undefined | J::Bool(false) => value,
        J::Array(values) => J::Array(
            values
                .into_iter()
                .map(header_value)
                .collect::<Result<_, _>>()?,
        ),
        value => match value {
            J::String(value) => s(value),
            J::Bool(value) => s(value.to_string()),
            J::Number(value) => s(safe_cex_num::js_number_string(value)),
            _ => panic!("fixture header scalar"),
        },
    })
}
fn normalized(fields: J) -> Result<J, ExchangeError> {
    let J::Object(fields) = fields else {
        panic!("header fixture object")
    };
    let mut result: Vec<(String, J)> = Vec::new();
    for (name, value) in fields {
        let name = safe_cex_num::js_trim(&name).to_owned();
        let value = header_value(value)?;
        if let Some((_, prior)) = result
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case(&name))
        {
            if *prior != J::Bool(false) {
                *prior = value;
            }
        } else {
            result.push((name, value));
        }
    }
    Ok(J::Object(result))
}
impl ApexApiHost for ApiHost {
    fn create_client(
        &self,
        cfg: ApexApiConfiguration,
        _: &ExchangeOptions,
        parent: &Rc<BaseExchange>,
    ) -> Result<(), ExchangeError> {
        assert_eq!(parent.name, ExchangeName::Apexomni);
        let probe =
            (cfg.params_serializer)(&obj(vec![("a", n(0.)), ("z", J::Bool(false))])).unwrap();
        self.runtime.record(
            "create",
            vec![s(cfg.base_url), n(cfg.timeout_ms), cfg.headers, s(probe)],
        );
        Ok(())
    }
    fn configure_retry(&self, retries: u8) -> Result<(), ExchangeError> {
        self.runtime
            .record("configure:retry", vec![n(retries.into()), J::Bool(true)]);
        Ok(())
    }
    fn register_request_handler(&self, api: Weak<ApexApi>) -> Result<(), ExchangeError> {
        assert!(api.upgrade().is_some());
        self.runtime.record("register:request", vec![]);
        Ok(())
    }
    fn register_response_handler(&self, api: Weak<ApexApi>) -> Result<(), ExchangeError> {
        assert!(api.upgrade().is_some());
        self.runtime.record("register:response", vec![]);
        Ok(())
    }
    fn install_transport_adapter(
        &self,
        _: &ExchangeOptions,
        name: &str,
    ) -> Result<(), ExchangeError> {
        self.runtime.record("install:transport", vec![s(name)]);
        Ok(())
    }
    fn now_ms(&self) -> f64 {
        1700001234567.
    }
    fn legacy_timestamp_ms(&self, time: f64) -> Result<f64, ExchangeError> {
        Ok((safe_cex_num::js_time_clip(time) / 1000.).floor() * 1000.)
    }
    fn axios_headers(&self, fields: J) -> Result<J, ExchangeError> {
        normalized(fields)
    }
    fn set_content_type(&self, headers: &mut J, value: &str) -> Result<(), ExchangeError> {
        let J::Object(fields) = headers else { panic!() };
        if let Some((_, prior)) = fields
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case("Content-Type"))
        {
            if *prior != J::Bool(false) {
                *prior = s(value);
            }
        } else {
            fields.push(("Content-Type".into(), s(value)));
        }
        Ok(())
    }
    fn request_retry(
        &self,
        config: safe_cex_exchange::js_runtime::value::Js,
    ) -> ExchangeFuture<'_, safe_cex_exchange::js_runtime::value::Js> {
        let config = safe_cex_exchange::js_runtime::value::to_js_json(&config).unwrap();
        self.runtime.record("retry", vec![config.clone()]);
        Box::pin(async move {
            Ok(native::heap(&obj(vec![
                ("data", obj(vec![("code", n(0.)), ("msg", s("retried"))])),
                ("config", config),
            ])))
        })
    }
}
fn run_api(kind: &str) {
    let rows = api_corpus()["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 27, "API source floor");
    let mut count = 0;
    for row in rows.iter().filter(|r| r["kind"] == kind) {
        let runtime = Rc::new(Host::default());
        let store = fixture(&runtime);
        let variant = row["variant"].as_u64().unwrap_or(0);
        let options:ExchangeOptions=serde_json::from_value(json!({"key":"key","secret":"secret-é","passphrase":"pass","testnet":variant==1,"tealstreetTunnel":variant==2,"extra":{"recvWindow":if variant==2{0.}else{20000.}}})).unwrap();
        let base = Rc::new(BaseExchange::new(
            ExchangeName::Apexomni,
            options.clone(),
            store,
            runtime.clone(),
            Default::default(),
        ));
        let api = create_api(
            options,
            base,
            Rc::new(ApiHost {
                runtime: runtime.clone(),
            }),
        )
        .unwrap();
        if kind != "factory" {
            runtime.operations.borrow_mut().clear();
            let input = native::input(&row["input"]);
            let actual = if kind == "request" {
                api.request_handler(&input)
            } else {
                futures::executor::block_on(api.response_handler(&native::heap(&input)))
                    .map(|v| safe_cex_exchange::js_runtime::value::to_js_json(&v).unwrap())
            };
            assert_eq!(
                actual.is_ok(),
                row["result"]["ok"].as_bool().unwrap(),
                "API {kind} native result kind"
            );
            if row["result"]["ok"] == true {
                native::check_fields(
                    &actual.unwrap(),
                    &row["result"]["value"],
                    &format!("API {kind} native result"),
                );
            } else {
                let error = actual.unwrap_err();
                assert_eq!(
                    error.message,
                    row["result"]["message"].as_str().unwrap(),
                    "API {kind} native error cause"
                );
                assert_eq!(
                    error.name,
                    row["result"]["name"].as_str().unwrap(),
                    "API {kind} native error name"
                );
            }
        }
        let J::Array(expected) = native::input(&row["ops"]) else {
            panic!()
        };
        let source_errors: Vec<_> = expected
            .iter()
            .filter_map(|v| {
                if let J::Array(a) = v
                    && a.first() == Some(&s("error"))
                {
                    if let J::String(message) = &a[1] {
                        Some(message.clone())
                    } else {
                        panic!()
                    }
                } else {
                    None
                }
            })
            .collect();
        let native_errors: Vec<_> = runtime
            .operations
            .borrow()
            .iter()
            .filter_map(|v| {
                if let J::Array(a) = v
                    && a.first() == Some(&s("reported:error"))
                {
                    if let J::String(message) = &a[1] {
                        Some(message.clone())
                    } else {
                        panic!()
                    }
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            native_errors,
            source_errors
                .iter()
                .map(|m| format!("Exchange Error: {m}"))
                .collect::<Vec<_>>(),
            "API {kind} native error dispatch"
        );
        let expected: Vec<J> = expected
            .into_iter()
            .filter(|v| !matches!(v,J::Array(a)if a.first()==Some(&s("error"))))
            .collect();
        let actual:Vec<J>=runtime.operations.borrow().iter().filter(|v|!matches!(v,J::Array(a)if a.first()==Some(&s("reported:error"))||(a.first()==Some(&s("log"))&&matches!(a.get(2),Some(J::String(level))if level=="error"||level=="trace")))).cloned().collect();
        fn sorted(value: J) -> J {
            match value {
                J::Object(mut fields) => {
                    fields.sort_by(|a, b| a.0.cmp(&b.0));
                    J::Object(fields.into_iter().map(|(k, v)| (k, sorted(v))).collect())
                }
                J::Array(v) => J::Array(v.into_iter().map(sorted).collect()),
                v => v,
            }
        }
        native::native_eq(
            &sorted(J::Array(actual)),
            &sorted(J::Array(expected)),
            &format!("API {kind} native operations"),
        );
        count += 1;
    }
    assert_eq!(
        count,
        match kind {
            "factory" => 3,
            "request" => 12,
            "response" => 12,
            _ => panic!(),
        },
        "API comparison floor"
    );
}
#[test]
fn api_factory_native() {
    run_api("factory");
}
#[test]
fn api_request_native() {
    run_api("request");
}
#[test]
fn api_response_native() {
    run_api("response");
}
