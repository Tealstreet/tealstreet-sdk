use safe_cex_num::{js_number_output, numeric_ingress::js_from_str};
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
struct Envelope<T> {
    data: T,
}
impl<T: safe_cex_num::numeric_ingress::JsNumberOutput> safe_cex_num::numeric_ingress::JsNumberOutput
    for Envelope<T>
{
    fn check_number_output(&self) {
        js_number_output!(self, struct { data });
    }
}
fn main() {
    let _ = js_from_str::<Envelope<serde_json::Value>>(r#"{"data":1e400}"#);
}

fn reference_is_not_an_eligibility_bypass(value: &serde_json::Value) {
    safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(&value);
}

fn tuple_does_not_hide_a_value(value: &(f64, serde_json::Value)) {
    safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(value);
}

fn ordered_maps_and_nested_containers_do_not_hide_value() {
    fn eligible<T: safe_cex_num::numeric_ingress::JsNumberOutput>() {}
    eligible::<safe_cex_types::object_map::JsObjectMap<serde_json::Value>>();
    eligible::<indexmap::IndexMap<String, serde_json::Value>>();
    eligible::<indexmap::IndexMap<serde_json::Value, f64>>();
    eligible::<(f64, f64, serde_json::Value)>();
    eligible::<(f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, serde_json::Value)>();
    eligible::<Box<[serde_json::Value]>>();
}
