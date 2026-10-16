use safe_cex_num::{JsJsonValue, js_json_stringify};
use safe_cex_venue_bitunix::websocket_private::send_login;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, ready},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn obs(v: &JsJsonValue) -> Value {
    match v {
        JsJsonValue::Undefined => json!({"undefined":true}),
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(v) => json!(v),
        JsJsonValue::Number(v) => {
            json!({"bits":format!("{:016x}",if v.is_nan(){0x7ff8_0000_0000_0000}else{v.to_bits()})})
        }
        JsJsonValue::String(s) => json!(s),
        JsJsonValue::Array(a) => json!(a.iter().map(obs).collect::<Vec<_>>()),
        JsJsonValue::Object(o) => {
            json!({"entries":o.iter().map(|(k,v)|json!([k,obs(v)])).collect::<Vec<_>>()})
        }
        JsJsonValue::BigInt(_) => panic!("LOGIN unexpected BigInt"),
    }
}
fn settle<T>(f: impl Future<Output = T>) -> T {
    match pin!(f).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("LOGIN fixture future must be ready"),
    }
}
#[test]
fn actual_typescript_websocket_login() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/websocket-login-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "LOGIN actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 25, "LOGIN source floor");
    for row in rows {
        let f = &row["fixture"];
        let events = RefCell::new(vec![]);
        let remote = RefCell::new(f["remote"] == true);
        let secret = RefCell::new(f["secret"].as_str().unwrap_or(" S ").to_owned());
        let key = RefCell::new(f["key"].as_str().unwrap_or(" K ").to_owned());
        let signer = Rc::new(());
        let note = |name: &str, args: Vec<Value>| -> Result<(), String> {
            let mut e = vec![json!(name)];
            e.extend(args);
            let e = json!(e);
            let i = events.borrow().len();
            assert_eq!(e, row["events"][i], "LOGIN native effect {name}:{i}");
            events.borrow_mut().push(e);
            if f["fail"] == name {
                Err(format!("{name} failed"))
            } else {
                Ok(())
            }
        };
        let result = settle(send_login(
            || {
                note("clock", vec![])?;
                Ok(f64::from_bits(
                    u64::from_str_radix(f["timeBits"].as_str().unwrap_or("427950afe2400000"), 16)
                        .unwrap(),
                ))
            },
            || {
                note("nonce", vec![])?;
                Ok("N".into())
            },
            || {
                note("key", vec![])?;
                Ok(key.borrow().clone())
            },
            || {
                let r = note("sync", vec![]);
                if f["changeAtSync"] == true {
                    *key.borrow_mut() = "NEW".into();
                    *secret.borrow_mut() = "CHANGED".into();
                    *remote.borrow_mut() = true;
                }
                ready(r)
            },
            |delay| ready(note("sleep", vec![obs(&JsJsonValue::Number(delay))])),
            || {
                note("remote", vec![])?;
                Ok((*remote.borrow()).then(|| signer.clone()))
            },
            || {
                note("secret", vec![])?;
                Ok(secret.borrow().clone())
            },
            || {
                note("id", vec![])?;
                Ok("ID".into())
            },
            |s| {
                assert!(Rc::ptr_eq(s, &signer), "LOGIN signer account identity");
                note("account", vec![])?;
                Ok(JsJsonValue::String("account".into()))
            },
            |s, request| {
                assert!(Rc::ptr_eq(&s, &signer), "LOGIN signing identity");
                let r = note("sign", vec![obs(&request)]).map(|()| {
                    f.get("result")
                        .cloned()
                        .unwrap_or(json!({"wsAuthSignature":"REMOTE"}))
                });
                ready(r)
            },
            |message| note("report", vec![json!(message)]),
            |frame| {
                note("frame", vec![obs(&frame)])?;
                let wire = js_json_stringify(&frame).unwrap().unwrap();
                note("send", vec![json!(wire)])
            },
        ));
        assert_eq!(
            result.as_ref().err().map(String::as_str),
            row["error"].get("message").and_then(Value::as_str),
            "LOGIN native error"
        );
        assert_eq!(
            events.borrow().len(),
            row["events"].as_array().unwrap().len(),
            "LOGIN native event count"
        );
    }
}
