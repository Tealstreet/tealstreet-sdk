// Native property selection and literal query/retry observations.
use crate::private_event_value::{EventOps, EventValue};
use crate::request_weight::Failure;
use serde_json::{Value, json};
pub fn optional(
    input: &Value,
    function: impl FnOnce(EventValue, &str) -> Result<EventValue, Failure>,
) -> Value {
    let child = EventValue::object(vec![("keep", EventValue::Number(-0.0))]);
    let source = match input["profile"].as_str() {
        Some("undefined") => EventValue::Undefined,
        Some("identity") => EventValue::object(vec![("selected", child.clone())]),
        Some("own-undefined") => EventValue::object(vec![("selected", EventValue::Undefined)]),
        _ => EventValue::from_json(input["data"].clone()),
    };
    match function(source.clone(), input["key"].as_str().unwrap()) {
        Ok(value) => {
            let observed = match &value {
                EventValue::Undefined => json!({"kind":"undefined"}),
                EventValue::Null => json!({"kind":"null"}),
                EventValue::Bool(v) => json!({"bool":v}),
                EventValue::Number(v) => json!({"bits":format!("{:016x}",v.to_bits())}),
                EventValue::String(v) => json!({"units":v.as_ref()}),
                _ => json!({"kind":value.type_of()}),
            };
            let identity = if input["profile"] == "identity" {
                assert!(value.same_reference(&child));
                child.set("keep", EventValue::Bool(false)).unwrap();
                value
                    .get("keep")
                    .unwrap()
                    .strict_equals(&EventValue::Bool(false))
            } else {
                false
            };
            json!({"value":observed,"retained":identity,"own":source.has_own(input["key"].as_str().unwrap())})
        }
        Err(e) => json!({"error":e.name,"message":e.message}),
    }
}
