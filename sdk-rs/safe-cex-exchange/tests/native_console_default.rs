//! BaseExchange logging uses the actual runtime default and production native sink.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    native::native_runtime,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use std::{cell::RefCell, process::Command, rc::Rc};

const MESSAGES: [(&str, &str); 3] = [
    ("info", "  café 火 %s %d \0 tail  "),
    ("debug", ""),
    ("error", "first line\nsecond line"),
];

#[test]
fn base_log_uses_production_native_console_default_without_projection() {
    if std::env::var("SAFE_CEX_CONSOLE_DEFAULT_CHILD").as_deref() == Ok("1") {
        let base = BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(serde_json::json!({"key":"fixture","secret":"fixture"}))
                .unwrap(),
            Rc::new(DefaultStore::new()),
            native_runtime(),
            ExchangeSettings::default(),
        );
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        base.on_args(
            "log",
            Rc::new(move |_, values| {
                seen.borrow_mut().push(values.to_vec());
                Ok(())
            }),
        );
        for (severity, message) in MESSAGES {
            base.log(message, severity).unwrap();
        }
        let events = events.borrow();
        assert_eq!(events.len(), MESSAGES.len());
        for (values, (severity, message)) in events.iter().zip(MESSAGES) {
            assert_eq!(values.len(), 2, "production log event keeps source arity");
            assert_eq!(
                values[0].to_js_string().unwrap().as_ref(),
                format!("[bybit] {message}")
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_slice()
            );
            assert_eq!(
                values[1].to_js_string().unwrap().as_ref(),
                severity.encode_utf16().collect::<Vec<_>>().as_slice()
            );
        }
        return;
    }
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "base_log_uses_production_native_console_default_without_projection",
            "--nocapture",
        ])
        .env("SAFE_CEX_CONSOLE_DEFAULT_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "production console child failed: {}{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    let expected = MESSAGES
        .into_iter()
        .map(|(severity, message)| format!("[safe-cex {severity}] {message}\n"))
        .collect::<String>();
    assert_eq!(
        String::from_utf8(child.stderr).unwrap(),
        expected,
        "actual NativeRuntime must receive the exact severity and one original string argument"
    );
}

#[test]
fn native_report_context_uses_terminal_json_without_changing_retained_input() {
    use safe_cex_exchange::{
        js_runtime::value::Js,
        runtime::{ExchangeError, ExchangeRuntime},
    };
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    const CHILD: &str = "SAFE_CEX_REPORT_CONTEXT_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let context = Input::Object(vec![
            (
                "tags".encode_utf16().collect(),
                Input::Object(vec![(
                    "recentHttpStatus".encode_utf16().collect(),
                    Input::Float(404.),
                )]),
            ),
            (
                "text".encode_utf16().collect(),
                Input::Text(vec![0xd800, 120, 0xdc00]),
            ),
            ("undefined".encode_utf16().collect(), Input::Undefined),
            (
                "values".encode_utf16().collect(),
                Input::Array(vec![
                    Input::Float(f64::NAN),
                    Input::Float(f64::INFINITY),
                    Input::Float(-0.),
                    Input::Undefined,
                ]),
            ),
        ]);
        let runtime = native_runtime();
        runtime.report_exception(&ExchangeError::new("typed"), &context);
        runtime.report_raw_exception(&Js::str("raw"), &context);
        runtime.report_exception(&ExchangeError::new("undefined"), &Input::Undefined);
        runtime.report_exception(
            &ExchangeError::new("invalid"),
            &Input::Lexeme("invalid".into()),
        );
        let Input::Array(values) = context.get("values").unwrap() else {
            panic!("retained array")
        };
        assert!(matches!(values[0], Input::Float(n) if n.is_nan()));
        assert!(matches!(values[1], Input::Float(n) if n == f64::INFINITY));
        assert!(matches!(values[2], Input::Float(n) if n.to_bits() == (-0.0_f64).to_bits()));
        assert!(matches!(values[3], Input::Undefined));
        assert_eq!(
            context.get("text"),
            Some(&Input::Text(vec![0xd800, 120, 0xdc00]))
        );
        assert_eq!(context.get("undefined"), Some(&Input::Undefined));
        return;
    }
    let oracle = Command::new("node").args(["-e", r#"process.stdout.write(JSON.stringify({tags:{recentHttpStatus:404},text:'\ud800x\udc00',undefined:undefined,values:[NaN,Infinity,-0,undefined]}))"#]).output().unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let json = String::from_utf8(oracle.stdout).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native_report_context_uses_terminal_json_without_changing_retained_input",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert_eq!(
        String::from_utf8(child.stderr).unwrap(),
        format!(
            "[safe-cex uncaught] Error: typed {json}\n[safe-cex uncaught] raw {json}\n[safe-cex uncaught] Error: undefined undefined\n[safe-cex uncaught] Error: invalid <TypeError: Invalid retained numeric lexeme>\n"
        )
    );
}
