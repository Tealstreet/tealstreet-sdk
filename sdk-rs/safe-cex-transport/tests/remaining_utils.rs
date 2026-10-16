use safe_cex_transport::polling::*;
#[test]
fn native_polling_wildcard_constant_and_fallback() {
    assert_eq!(ALL_EXCHANGES, "*");
    set_tick_interval_multipliers(Some(
        &[("*".into(), 3.0), ("bybit".into(), 5.0)]
            .into_iter()
            .collect(),
    ));
    assert_eq!(get_tick_interval_multiplier("unknown"), 3.0);
    assert_eq!(get_tick_interval_multiplier("bybit"), 5.0);
    set_tick_interval_multipliers(None);
    assert_eq!(get_tick_interval_multiplier("unknown"), 1.0);
}
