use safe_cex_engine::{
    store::{MarketSetting, Store},
    wire::{FrameWriter, section, sections},
};

#[test]
fn native_settings_omit_unusable_rails_before_the_frame_consumer() {
    let mut store = Store::default();
    store.replace_settings([
        ("UNKNOWN", MarketSetting::default()),
        (
            "ZERO",
            MarketSetting {
                leverage: 0.0,
                ..MarketSetting::default()
            },
        ),
        (
            "RAILS",
            MarketSetting {
                buy_leverage: Some(15.0),
                sell_leverage: Some(30.0),
                ..MarketSetting::default()
            },
        ),
    ]);
    let mut writer = FrameWriter::new(vec![], 0.0);
    store.write(&store.everything(), &mut writer);
    let bytes = writer.finish();
    let (_, settings) = sections(&bytes)
        .find(|(kind, _)| *kind == section::SETTINGS)
        .unwrap();
    assert_eq!(
        u32::from_le_bytes(settings[..4].try_into().unwrap()),
        2,
        "native output must omit unknown settings"
    );
}
