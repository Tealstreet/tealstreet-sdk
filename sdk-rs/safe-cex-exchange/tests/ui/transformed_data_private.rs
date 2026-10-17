use safe_cex_transport::provider::TransformedData;
use safe_cex_types::retained_json::RetainedJsonInput;
fn main() {
    let _ = TransformedData { value: RetainedJsonInput::Null };
}
