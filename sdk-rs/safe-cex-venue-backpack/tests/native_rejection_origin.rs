#[path = "support/adapter_runtime.rs"]
mod support;

use safe_cex_exchange::js_runtime::{
    globals::stringify_json,
    value::{Js, Thrown},
};
use serde_json::{Value, json};
use std::{cell::Cell, process::Command, rc::Rc};

#[test]
fn constructed_backpack_trigger_repair_preserves_native_getter_rejection() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/native-rejection-origin.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let (owner, runtime, _, wire) = support::fixture();
    let code = Js::object(vec![
        ("nan", Js::Number(f64::NAN)),
        ("zero", Js::Number(-0.0)),
    ]);
    let values = [
        Js::object(vec![
            ("name", Js::utf16(&[0xd800])),
            ("message", Js::utf16(&[0xdc00])),
            ("code", code.clone()),
            ("undefined", Js::Undefined),
        ]),
        Js::Number(-0.0),
        Js::Number(f64::from_bits(0x7ff8_0000_0000_0042)),
        Js::utf16(&[0xdfff]),
    ];
    let mut observed = Vec::new();
    for original in values {
        let reads = Rc::new(Cell::new(0));
        let count = reads.clone();
        let rejection = original.clone();
        let row = Js::object(vec![]);
        row.define_accessor(
            "triggerPrice",
            Some(Rc::new(move |_| {
                count.set(count.get() + 1);
                Err(Thrown::new(rejection.clone()))
            })),
            None,
        )
        .unwrap();
        let before = wire.requests.lock().unwrap().len();
        let error = support::ready(
            &runtime,
            owner.fix_trigger_orders_without_trigger_by(Js::array(vec![row])),
        )
        .unwrap_err();
        let returned = Thrown::from(error).value;
        let same = match (&original, &returned) {
            (Js::Number(a), Js::Number(b)) => a.to_bits() == b.to_bits(),
            _ if original.is_object() => original.same_reference(&returned),
            _ => original.strict_equals(&returned),
        };
        assert!(
            same,
            "BACKPACK_NATIVE_REJECTION original value must survive the typed host"
        );
        if original.is_object() {
            assert!(returned.get("code").unwrap().same_reference(&code));
            assert!(returned.has_own("undefined"));
        }
        let bits = match &returned {
            Js::Number(n) => Some(format!("{:016x}", n.to_bits())),
            _ => None,
        };
        let text = match &returned {
            Js::String(s) => Some(s.to_vec()),
            _ => None,
        };
        observed.push(json!({
            "same":same, "reads":reads.get(),
            "http":wire.requests.lock().unwrap().len() - before,
            "type":returned.type_of(), "bits":bits, "units":text,
            "json":String::from_utf16(&stringify_json(&returned).unwrap().unwrap()).unwrap(),
        }));
    }
    assert_eq!(
        Value::Array(observed),
        expected,
        "actual maintained Backpack method and native binding"
    );
}
