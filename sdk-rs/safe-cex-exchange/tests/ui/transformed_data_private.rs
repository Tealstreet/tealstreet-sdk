use safe_cex_transport::provider::TransformedData;
fn main() {
    let _ = TransformedData { value: serde_json::json!("guessed bytes") };
}
