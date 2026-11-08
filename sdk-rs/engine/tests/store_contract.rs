pub use safe_cex_engine::store;
#[path = "../src/store_output.rs"]
mod store_output;

#[test]
fn native_store_rails_keep_zero_independent_sides_and_omit_unknown() {
    use store::MarketSetting;
    assert_eq!(store_output::rails(&MarketSetting::default()), None);
    assert_eq!(
        store_output::rails(&MarketSetting {
            leverage: 0.0,
            ..MarketSetting::default()
        }),
        Some([0.0, 0.0])
    );
    assert_eq!(
        store_output::rails(&MarketSetting {
            buy_leverage: Some(15.0),
            sell_leverage: Some(30.0),
            ..MarketSetting::default()
        }),
        Some([15.0, 30.0])
    );
    assert_eq!(
        store_output::rails(&MarketSetting {
            leverage: 7.0,
            buy_leverage: Some(f64::NAN),
            ..MarketSetting::default()
        }),
        Some([7.0, 7.0])
    );
}
