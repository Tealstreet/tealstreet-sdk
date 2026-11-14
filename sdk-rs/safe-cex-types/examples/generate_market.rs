//! Scoped generation until the integrator wires Market into the shared publisher.
pub use safe_cex_types::{
    MarketLimits, MarketPrecision, MarketRuntime, OptionTerms, retained_json,
};
#[path = "../src/market.rs"]
pub mod market;

fn main() {
    use ts_rs::TS;
    print!(
        "{}",
        market::Market::export_to_string(&ts_rs::Config::default()).unwrap()
    );
}
