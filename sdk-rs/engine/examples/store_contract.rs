use safe_cex_engine::{
    model::{Balance, MarginType},
    store::{MarketSetting, Store},
    wire::FrameWriter,
};
use serde_json::json;
use std::{fs, path::Path};

fn frame(store: &Store) -> Vec<u8> {
    let mut writer = FrameWriter::new(vec![], 1.0);
    store.write(&store.everything(), &mut writer);
    writer.finish()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::default();
    let empty = frame(&store);
    store.set_balance(Balance::default());
    for _ in 0..2 {
        store.set_orders(vec![]);
    }
    for _ in 0..3 {
        store.set_positions(vec![]);
    }
    store.set_hedged(true);
    store.set_latency(0.0);
    store.replace_settings([
        (
            "BTCUSDT",
            MarketSetting {
                leverage: 20.0,
                buy_leverage: Some(15.0),
                sell_leverage: Some(30.0),
                margin_type: Some(MarginType::Cross),
            },
        ),
        (
            "ZERO",
            MarketSetting {
                leverage: 0.0,
                ..MarketSetting::default()
            },
        ),
        ("UNKNOWN", MarketSetting::default()),
        (
            "FALLBACK",
            MarketSetting {
                leverage: 7.0,
                buy_leverage: Some(f64::NAN),
                ..MarketSetting::default()
            },
        ),
    ]);
    let ready = frame(&store);
    store.replace_settings(std::iter::empty());
    let cleared = frame(&store);
    let fixture =
        serde_json::to_string_pretty(&json!({"empty":empty,"ready":ready,"cleared":cleared}))?
            + "\n";
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/store-frames.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if std::env::args().any(|arg| arg == "--check") {
        assert_eq!(fs::read_to_string(path)?, fixture);
    } else {
        fs::write(path, fixture)?;
    }
    Ok(())
}
