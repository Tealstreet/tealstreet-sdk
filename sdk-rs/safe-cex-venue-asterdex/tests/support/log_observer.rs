use safe_cex_exchange::{js_runtime::value::Js, runtime::console_text};

pub fn venue_log_text(severity: &str, values: &[Js]) -> String {
    let report = severity == "error"
        && values.first().is_some_and(|value| {
            value.strict_equals(&Js::str("exchange error"))
                || value.strict_equals(&Js::str("exchange error (disposed, suppressing)"))
        });
    if severity == "trace" || report {
        return console_text(values);
    }
    assert!(matches!(severity, "debug" | "info" | "warning" | "error"));
    let [message, observed_severity] = values else {
        panic!("BaseExchange.log requires message and severity: {severity}: {values:?}");
    };
    assert!(
        observed_severity.strict_equals(&Js::str(severity)),
        "BaseExchange.log console severity argument must match its recorded channel"
    );
    console_text(std::slice::from_ref(message))
}
