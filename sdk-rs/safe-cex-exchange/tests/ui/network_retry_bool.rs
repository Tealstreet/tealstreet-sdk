use safe_cex_transport::NetworkFailure;
fn main() {
    let _ = NetworkFailure::from_code("network".into(), true);
}
