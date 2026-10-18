use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Deserialize)]
struct NormalizesSign(f64);
impl Serialize for NormalizesSign {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.0.abs())
    }
}
impl JsNumberOutput for NormalizesSign {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, tuple(value));
    }
}
#[derive(Debug, Deserialize, Serialize)]
struct Nested {
    payload: Vec<NormalizesSign>,
}
impl JsNumberOutput for Nested {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { payload });
    }
}

#[test]
fn numeric_output_invariant_refuses_changed_bits_at_scalar_and_nested_paths() {
    for raw in ["-0", "-2.5"] {
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        for result in [
            js_from_str::<NormalizesSign>(raw),
            js_from_value::<NormalizesSign>(value.clone()),
            js_from_value_ref::<NormalizesSign>(&value),
        ] {
            assert_eq!(
                result.unwrap_err().to_string(),
                "numeric ingress changed Number bits at $"
            );
        }
        let raw = format!(r#"{{"payload":[{raw}]}}"#);
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        for result in [
            js_from_str::<Nested>(&raw),
            js_from_value::<Nested>(value.clone()),
            js_from_value_ref::<Nested>(&value),
        ] {
            assert_eq!(
                result.unwrap_err().to_string(),
                "numeric ingress changed Number bits at $.payload[0]"
            );
        }
    }
    for raw in ["0", "2.5"] {
        assert_eq!(
            js_from_str::<NormalizesSign>(raw).unwrap().0,
            raw.parse::<f64>().unwrap()
        );
    }
}
