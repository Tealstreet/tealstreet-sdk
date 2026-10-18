//! BaseExchange logging uses the actual runtime default and production native sink.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    native::native_runtime,
};
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
