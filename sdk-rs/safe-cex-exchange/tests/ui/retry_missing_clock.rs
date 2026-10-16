use safe_cex_transport::{RestAdapter, retry::RetryPolicy};
fn missing_clock(policy: &RetryPolicy, adapter: &dyn RestAdapter) {
    let _future = policy.execute(adapter);
}
fn main() {}
