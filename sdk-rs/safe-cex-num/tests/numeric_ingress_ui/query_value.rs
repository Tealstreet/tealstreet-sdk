use safe_cex_num::numeric_ingress::JsNumberOutput;
use safe_cex_types::QueryValue;
struct Output {
    query: QueryValue,
}
impl JsNumberOutput for Output {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { query });
    }
}
fn main() {}
