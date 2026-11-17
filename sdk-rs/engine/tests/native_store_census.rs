use safe_cex_engine::{
    Store,
    model::*,
    wire::{self, FrameWriter},
};

#[test]
fn native_frames_publish_request_outcome_counters() {
    let store = Store::default();
    let mut writer = FrameWriter::new(Vec::new(), 1000.0);
    store.write(&store.everything(), &mut writer);
    let bytes = writer.finish();
    let payload = wire::sections(&bytes)
        .find(|(kind, _)| *kind == 35)
        .map(|(_, payload)| payload);
    assert!(
        payload.is_some(),
        "native snapshots must carry live requestStats"
    );
    assert_eq!(payload.unwrap(), &[0; 24]);
}

#[test]
fn fetched_leverage_updates_native_position_rows_on_each_hedged_side() {
    let mut store = Store::default();
    store.set_positions(
        [PositionSide::Long, PositionSide::Short]
            .into_iter()
            .map(|side| Position {
                id: format!("BTCUSDT:{side:?}"),
                symbol: "BTCUSDT".into(),
                side,
                entry_price: 100.0,
                notional: 200.0,
                leverage: 7.0,
                margin_type: Some(MarginType::Cross),
                position_mode: Some(PositionMode::Hedged),
                unrealized_pnl: 7.0,
                realized_pnl: None,
                contracts: 2.0,
                liquidation_price: 50.0,
                margin_size: None,
                last_update: None,
            })
            .collect(),
    );
    store.take_pending();
    store.update_setting("BTCUSDT", |setting| {
        setting.leverage = 10.0;
        setting.buy_leverage = Some(20.0);
        setting.sell_leverage = Some(30.0);
    });
    assert_eq!(store.positions()[0].leverage, 20.0);
    assert_eq!(store.positions()[1].leverage, 30.0);
    let mut writer = FrameWriter::new(Vec::new(), 2000.0);
    let pending = store.take_pending();
    store.write(&pending, &mut writer);
    let frame = writer.finish();
    assert!(
        wire::sections(&frame).any(|(kind, _)| kind == 8),
        "settings completion must emit new canonical position rows"
    );
}

#[test]
fn request_outcomes_are_saturating_monotonic_native_counters() {
    let mut store = Store::default();
    for status in [
        Some(200),
        Some(401),
        Some(403),
        Some(418),
        Some(429),
        None,
        Some(503),
    ] {
        store.record_request_outcome(status);
    }
    let mut writer = FrameWriter::new(Vec::new(), 1000.0);
    store.write(&store.everything(), &mut writer);
    let bytes = writer.finish();
    let payload = wire::sections(&bytes)
        .find(|(kind, _)| *kind == 35)
        .unwrap()
        .1;
    let counts: Vec<_> = payload[..20]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| u32::from_le_bytes(*v))
        .collect();
    assert_eq!(counts, [1, 2, 2, 1, 1]);
}
