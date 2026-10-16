use safe_cex_num::JsJsonValue;
use safe_cex_venue_bitunix::api;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};

fn obs(v: &JsJsonValue) -> Value {
    match v {
        JsJsonValue::Undefined => json!({"undefined":true}),
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(v) => json!(v),
        JsJsonValue::Number(v) => {
            json!({"bits":format!("{:016x}",if v.is_nan(){0x7ff8_0000_0000_0000}else{v.to_bits()})})
        }
        JsJsonValue::String(v) => json!(v),
        JsJsonValue::Array(a) => json!(a.iter().map(obs).collect::<Vec<_>>()),
        JsJsonValue::Object(o) => {
            json!({"entries":o.iter().map(|(k,v)|json!([k,obs(v)])).collect::<Vec<_>>()})
        }
        JsJsonValue::BigInt(_) => panic!("API fixture BigInt"),
    }
}
fn value_obs(v: &Value) -> Value {
    obs(&JsJsonValue::from(v.clone()))
}
fn strv(v: &str) -> Value {
    json!(v)
}
fn note(
    events: &RefCell<Vec<Value>>,
    row: &Value,
    fixture: &Value,
    name: &str,
    args: Vec<Value>,
) -> Result<(), String> {
    let mut event = vec![strv(name)];
    event.extend(args);
    let event = json!(event);
    let i = events.borrow().len();
    assert_eq!(event, row["events"][i], "API native effect {name}:{i}");
    events.borrow_mut().push(event);
    if fixture["fail"] == name {
        Err(format!("{name} failed"))
    } else {
        Ok(())
    }
}
fn outcome<T>(result: &Result<T, String>, row: &Value, events: &RefCell<Vec<Value>>) {
    assert_eq!(
        result.as_ref().err().map(String::as_str),
        row["error"].get("message").and_then(Value::as_str),
        "API native error"
    );
    assert_eq!(
        events.borrow().len(),
        row["events"].as_array().unwrap().len(),
        "API native event count"
    );
}
#[test]
fn actual_typescript_api() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/api-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "API actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    for (name, floor) in [("factory", 8), ("requests", 17), ("responses", 17)] {
        assert_eq!(
            corpus[name].as_array().unwrap().len(),
            floor,
            "API source floor"
        );
    }
    for row in corpus["factory"].as_array().unwrap() {
        let f = &row["fixture"];
        let events = RefCell::new(vec![]);
        let client = Rc::new(());
        let token = Rc::new(());
        let n = |name, args| note(&events, row, f, name, args);
        let result = api::create_api(
            f["options"]["testnet"] == true,
            f["options"]["publicDataOnly"] == true,
            |config| {
                n("create", vec![obs(&config)])?;
                Ok(client.clone())
            },
            || {
                n("condition", vec![])?;
                Ok(token.clone())
            },
            |c, r, retries| {
                assert!(Rc::ptr_eq(c, &client));
                assert!(Rc::ptr_eq(&r, &token));
                n("retry", vec![obs(&JsJsonValue::Number(retries))])
            },
            |c| {
                assert!(Rc::ptr_eq(c, &client));
                n("transport", vec![strv("bitunix")])
            },
            |c, handler| {
                assert!(Rc::ptr_eq(c, &client));
                assert!(
                    handler as usize == api::sign_request as *const () as usize,
                    "API callable request"
                );
                n("request", vec![])
            },
            |c, handler| {
                assert!(Rc::ptr_eq(c, &client));
                assert!(
                    handler as usize == api::receive_response as *const () as usize,
                    "API callable response"
                );
                n("response", vec![])
            },
        );
        outcome(&result, row, &events);
        assert_eq!(
            result.as_ref().is_ok_and(|c| Rc::ptr_eq(c, &client)),
            row["identity"],
            "API native client identity"
        );
    }
    for row in corpus["requests"].as_array().unwrap() {
        let f: Value = serde_json::from_str(row["fixture"].as_str().unwrap()).unwrap();
        let events = RefCell::new(vec![]);
        let n = |name, args| note(&events, row, &f, name, args);
        let options = f
            .get("options")
            .cloned()
            .unwrap_or(json!({"key":"K","secret":"S"}));
        let result = api::sign_request(
            &f["config"],
            &options,
            &mut |params, format, sort| {
                n("query", vec![value_obs(params), strv(format), strv(sort)])?;
                Ok(row["queryResult"].as_str().unwrap().into())
            },
            &mut || {
                n("clock", vec![])?;
                Ok(f64::from_bits(
                    u64::from_str_radix(f["timeBits"].as_str().unwrap_or("427950afe2400000"), 16)
                        .unwrap(),
                ))
            },
            &mut || {
                n("nonce", vec![])?;
                Ok(f["nonce"].as_str().unwrap_or("N").into())
            },
        )
        .and_then(|headers| {
            if let Some(headers) = headers.as_ref() {
                n("headers", vec![obs(headers)])?;
            }
            Ok(headers)
        });
        outcome(&result, row, &events);
        assert_eq!(
            result.as_ref().is_ok_and(Option::is_none),
            row["identity"],
            "API native unsigned identity"
        );
    }
    for row in corpus["responses"].as_array().unwrap() {
        let f: Value = serde_json::from_str(row["fixture"].as_str().unwrap()).unwrap();
        let events = RefCell::new(vec![]);
        let reads = RefCell::new(0);
        let mut config = json!({});
        for (input, key) in [
            ("errorFlag", "__suppressExchangeError"),
            ("rateFlag", "__suppressExchangeRateLimitError"),
        ] {
            if let Some(v) = f.get(input) {
                config[key] = v.clone();
            }
        }
        let mut response =
            json!({"config":config,"data":f.get("data").cloned().unwrap_or(json!({"msg":"bad"}))});
        if let Some(s) = f.get("status") {
            response["status"] = s.clone();
        }
        let n = |name, args| note(&events, row, &f, name, args);
        let result = api::receive_response(
            &response,
            &mut |name| {
                n("header", vec![strv(name)])?;
                let value = if name == "x-tealstreet-suppress-exchange-error" {
                    if let Some(values) = f.get("headerValues") {
                        let i = *reads.borrow();
                        *reads.borrow_mut() += 1;
                        values.get(i).cloned()
                    } else {
                        f.get("headerValue").cloned()
                    }
                } else {
                    f.get("rateValue").cloned()
                };
                Ok(value)
            },
            &mut |message, r| {
                assert!(
                    std::ptr::eq(r, &response),
                    "API original response report identity"
                );
                n("report", vec![strv(&message)])?;
                let message = if let Some(s) = r["status"].as_f64().filter(|s| *s >= 400.) {
                    let status = safe_cex_num::js_number_string(s);
                    if message.contains(&format!("HTTP {status}")) {
                        message
                    } else {
                        format!("{message} (HTTP {status})")
                    }
                } else {
                    message
                };
                n("emit", vec![strv(&message)])
            },
        );
        outcome(&result, row, &events);
        assert_eq!(
            result.as_ref().is_ok_and(|r| std::ptr::eq(*r, &response)),
            row["identity"],
            "API native response identity"
        );
    }
}
