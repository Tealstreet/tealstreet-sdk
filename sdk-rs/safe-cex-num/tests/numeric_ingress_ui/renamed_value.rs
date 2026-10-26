use safe_cex_num::js_number_output;
use serde::{Deserialize, Serialize};
#[derive(Deserialize, Serialize)]
#[serde(tag = "kind")]
enum Opaque {
    Data {
        #[serde(rename(serialize = "elsewhere", deserialize = "data"))]
        data: serde_json::Value,
    },
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for Opaque {
    fn check_number_output(&self) {
        js_number_output!(self, enum { Data { data } });
    }
}
fn main() {}
