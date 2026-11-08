use safe_cex_types::retained_json;
use safe_cex_types::{ExecutionPositionSide, OrderSide, OrderStatus, OrderType};
use serde::Deserialize;
use std::{fs, path::Path};
use ts_rs::{Config, TS};

#[path = "../src/history.rs"]
mod history;
#[path = "../tests/support/history_output.rs"]
mod output_fixture;
#[path = "../src/strict_decode.rs"]
mod strict_decode;

fn deserialize_present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let raw = root.join("bindings/history/raw");
    fs::create_dir_all(&raw)?;
    let config = Config::new().with_out_dir(&raw);
    fn export<T: TS + 'static>(
        config: &Config,
        raw: &Path,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let generated = T::export_to_string(config)?;
        let normalized = generated
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(raw.join(format!("{name}.ts")), normalized)?;
        Ok(())
    }
    export::<history::HistoricalOrder>(&config, &raw, "HistoricalOrder")?;
    export::<history::HistoricalTrade>(&config, &raw, "HistoricalTrade")?;
    export::<history::ClosedPnlEntry>(&config, &raw, "ClosedPnlEntry")?;
    export::<history::IncomeRecord>(&config, &raw, "IncomeRecord")?;
    export::<history::DataProvenance>(&config, &raw, "DataProvenance")?;
    export::<history::IncomeCategory>(&config, &raw, "IncomeCategory")?;
    fs::write(
        root.join("tests/fixtures/history-output.json"),
        serde_json::to_string_pretty(&output_fixture::fixture())? + "\n",
    )?;
    Ok(())
}
