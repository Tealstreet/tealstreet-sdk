use safe_cex_num::{
    JsCoercionError, JsNumberObject, JsPrimitive, js_number_object, js_to_number_object,
};
use serde_json::{Value, json};
struct Hooks<'a> {
    row: &'a Value,
    calls: Vec<&'static str>,
}
fn result(kind: &str) -> Result<Option<JsPrimitive>, JsCoercionError> {
    Ok(match kind {
        "absent" | "object" => None,
        "undefined" => Some(JsPrimitive::Undefined),
        "null" => Some(JsPrimitive::Null),
        "false" => Some(JsPrimitive::Boolean(false)),
        "-0" => Some(JsPrimitive::Number(-0.0)),
        "string" => Some(JsPrimitive::String("\u{feff}0x10\u{feff}".into())),
        "bigint" => Some(JsPrimitive::BigInt(
            "123456789012345678901234567890".parse().unwrap(),
        )),
        "symbol" => Some(JsPrimitive::Symbol),
        "throw" => {
            return Err(JsCoercionError {
                name: "TypeError",
                message: "hook failed",
            });
        }
        _ => unreachable!(),
    })
}
impl JsNumberObject for Hooks<'_> {
    fn to_primitive(&mut self) -> Result<Option<JsPrimitive>, JsCoercionError> {
        if self.row["exotic"] != true {
            return Ok(None);
        }
        self.calls.push("exotic");
        let kind = self.row["first"].as_str().unwrap();
        if kind == "object" {
            return Err(JsCoercionError {
                name: "TypeError",
                message: "Cannot convert object to primitive value",
            });
        }
        if kind == "absent" {
            return Ok(Some(JsPrimitive::Undefined));
        }
        result(kind)
    }
    fn value_of(&mut self) -> Result<Option<JsPrimitive>, JsCoercionError> {
        let kind = self.row["first"].as_str().unwrap();
        if kind != "absent" {
            self.calls.push("valueOf");
        }
        result(kind)
    }
    fn to_string(&mut self) -> Result<Option<JsPrimitive>, JsCoercionError> {
        let kind = self.row["second"].as_str().unwrap();
        if kind != "absent" {
            self.calls.push("toString");
        }
        result(kind)
    }
}
#[test]
fn number_hooks_match_live_node_order_and_errors() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_number_hooks.cjs"
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 400);
    for row in rows {
        let mut bigint_hooks = Hooks {
            row: &row,
            calls: vec![],
        };
        let bigint_actual = match safe_cex_num::js_bigint_object(&mut bigint_hooks) {
            Ok(v) => json!({"ok":v.to_string()}),
            Err(e) => json!({"error":{"name":e.kind.name(),"message":e.message}}),
        };
        assert_eq!(json!(bigint_hooks.calls), row["bigintCalls"], "{row}");
        assert_eq!(bigint_actual, row["bigintExpected"], "{row}");
        let mut hooks = Hooks {
            row: &row,
            calls: vec![],
        };
        let actual = if row["unary"] == true {
            js_to_number_object(&mut hooks)
        } else {
            js_number_object(&mut hooks)
        };
        assert_eq!(json!(hooks.calls), row["calls"], "{row}");
        match actual {
            Ok(n) => {
                let expected: f64 = row["expected"]["ok"]
                    .as_str()
                    .expect("success")
                    .parse()
                    .unwrap();
                assert!(
                    n.is_nan() && expected.is_nan() || n.to_bits() == expected.to_bits(),
                    "{row}: {n:?}"
                );
            }
            Err(e) => assert_eq!(
                json!({"error":{"name":e.name,"message":e.message}}),
                row["expected"],
                "{row}"
            ),
        }
    }
}
