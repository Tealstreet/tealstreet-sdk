//! Route-specific request layout; the signing itself is `dispatch` work.
pub mod dispatch;

pub use safe_cex_engine::url::encode;

pub fn query(params: &[(String, String)], sorted: bool) -> String {
    let mut rows: Vec<_> = params.iter().collect();
    if sorted {
        rows.sort_by(|a, b| a.0.cmp(&b.0));
    }
    rows.into_iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}
