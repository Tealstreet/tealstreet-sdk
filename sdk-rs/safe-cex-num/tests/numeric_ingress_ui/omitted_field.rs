use safe_cex_num::js_number_output;
struct Record {
    visible: f64,
    hidden: serde_json::Value,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for Record {
    fn check_number_output(&self) {
        js_number_output!(self, struct { visible });
    }
}
fn main() {}
