//! Slice-only generation: cargo run -j2 --locked -p safe-cex-types --example generate_execution.
pub use safe_cex_types::{ExecutionPositionSide, OrderSide, strict_decode};
#[path = "../src/execution.rs"]
mod execution;

use execution::{Execution, OrderFillEvent, PublicTrade};
use ts_rs::{Config, TS};

fn main() {
    let config = Config::new().with_out_dir("bindings/raw");
    let sources = [
        (
            "OrderFillEvent",
            OrderFillEvent::export_to_string(&config).unwrap(),
        ),
        ("Execution", Execution::export_to_string(&config).unwrap()),
        (
            "PublicTrade",
            PublicTrade::export_to_string(&config).unwrap(),
        ),
    ];
    serde_json::to_writer(std::io::stdout(), &sources).unwrap();
}
