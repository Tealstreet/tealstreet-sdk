use safe_cex_types::store_model::{
    DisplayedMargin, FetchedLeverage, PositionSettings, StoreData, StoreDataLoaded, StoreOptions,
    StoreUpdateIds,
};
use std::{fs, path::Path};
use ts_rs::{Config, TS};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/store/raw");
    fs::create_dir_all(&raw)?;
    let config = Config::new().with_out_dir(&raw);
    fn export<T: TS + 'static>(
        config: &Config,
        raw: &Path,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        fs::write(
            raw.join(format!("{name}.ts")),
            T::export_to_string(config)?
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )?;
        Ok(())
    }
    export::<StoreOptions>(&config, &raw, "StoreOptions")?;
    export::<StoreDataLoaded>(&config, &raw, "StoreDataLoaded")?;
    export::<StoreUpdateIds>(&config, &raw, "StoreUpdateIds")?;
    export::<FetchedLeverage>(&config, &raw, "FetchedLeverage")?;
    export::<PositionSettings>(&config, &raw, "PositionSettings")?;
    export::<DisplayedMargin>(&config, &raw, "DisplayedMargin")?;
    export::<StoreData>(&config, &raw, "StoreData")?;
    fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("bindings/store/initial.json"),
        serde_json::to_string_pretty(&safe_cex_types::store_model::initial_store_data())? + "\n",
    )?;
    Ok(())
}
