//! Diagnostic text reaches the real private protocol on a constructed venue.
#![cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
#[path = "support/exchange_trait_runtime.rs"]
mod support;
use safe_cex_exchange::{exchange::Exchange, socket_lifecycle::SocketSlot};
use safe_cex_transport::socket::SocketFrame;
use std::{cell::RefCell, process::Command, rc::Rc};

#[test]
fn private_login_failure_uses_javascript_json_text() {
    let cases = [
        ("1.0", "1"),
        ("-0.0", "0"),
        ("1e3", "1000"),
        ("0.000001", "0.000001"),
        ("1e-7", "1e-7"),
        ("1e+16", "10000000000000000"),
        ("1e400", "null"),
        ("[-0.0,1.0,1e-7]", "[0,1,1e-7]"),
        (r#"{"2":2.0,"1":1.0,"x":-0.0}"#, r#"{"1":1,"2":2,"x":0}"#),
        ("12345678901234567", r#""12345678901234567""#),
    ];
    let mut observed = 0;
    for (input, expected) in cases {
        let (venue, runtime, _, _) = support::fixture();
        runtime.diagnostics.set(true);
        let messages = Rc::new(RefCell::new(Vec::new()));
        let sink = messages.clone();
        venue.base().on(
            "grafana-pws",
            Rc::new(move |_, message| {
                let message = safe_cex_exchange::js_runtime::value::to_json(message).unwrap();
                sink.borrow_mut().push(message.as_str().unwrap().to_owned());
                Ok(())
            }),
        );
        let frame = format!(r#"{{"channel":"rs.login","data":"failure","value":{input}}}"#);
        venue
            .base()
            .get_socket(SocketSlot::Private)
            .unwrap()
            .on_message(&SocketFrame::Text(frame))
            .unwrap();
        let expected = format!(
            r#"[mexc private] [mexc login] failed {{"channel":"rs.login","data":"failure","value":{expected}}}"#
        );
        assert_eq!(
            messages.borrow().len(),
            2,
            "MEXC-JSON receive diagnostic count"
        );
        assert!(
            messages.borrow().contains(&expected),
            "MEXC-JSON diagnostic for {input}: {:?}",
            messages.borrow()
        );
        observed += 1;
    }
    assert_eq!(observed, 10, "MEXC-JSON native profile floor");
}

#[test]
#[ignore = "requires isolated TypeScript; venue oracle includes ignored witnesses"]
fn maintained_login_handler_uses_javascript_json_text() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/diagnostic_json_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "MEXC-JSON actual source: 10 diagnostic profiles\n"
    );
}
