//! Isolated Balance generation; the integrator registers the same module in lib.rs.
pub use safe_cex_types::{model_field, object_map, retained_json, strict_decode};
#[path = "../src/balance.rs"]
mod balance;
pub use balance::{Balance, CoinBalance, CurrencyBalance};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use ts_rs::{Config, TS};
    let directory = std::env::args().nth(1).ok_or("expected output directory")?;
    let config = Config::new().with_out_dir(&directory);
    Balance::export_all(&config)?;
    CoinBalance::export_all(&config)?;
    std::fs::write(
        std::path::Path::new(&directory).join("empty.json"),
        serde_json::to_string(&balance::empty_balance())?,
    )?;
    Ok(())
}
