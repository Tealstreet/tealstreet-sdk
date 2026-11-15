//! Emits the actual native projection used by the TS null-preservation regression.
#[path = "../src/history_output.rs"]
pub mod output;
use safe_cex_engine::{ClosedPositionFormat, Cx, VenueError, history, model, num};

fn main() {
    let output = output::ClosedPnlOutput::new(
        ClosedPositionFormat::Bybit,
        "fixture".into(),
        false,
        None,
        &[],
    );
    let rows = [
        r#"{"symbol":"BTCUSDT","side":"Sell"}"#.to_owned(),
        r#"{"symbol":"BTCUSDT","side":"Sell","createdTime":0,"updatedTime":0,"closedSize":"0","cumExitValue":"0","avgEntryPrice":"0","avgExitPrice":"0","closedPnl":"0","openFee":"0","closeFee":"0","leverage":"0"}"#.to_owned(),
    ];
    println!("{}", output.encode(&rows).unwrap());
}
