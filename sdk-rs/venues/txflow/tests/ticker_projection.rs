#[path = "support/mod.rs"]
mod support;
use safe_cex_engine::model::Ticker;

fn tick(
    engine: &mut safe_cex_engine::Engine<safe_cex_venue_txflow::TxFlow>,
    socket: safe_cex_engine::SocketId,
    id: u32,
    price: u32,
) {
    support::message(
        engine,
        socket,
        4000.,
        &format!(
            r#"{{"channel":"ticker","data":{{"instrumentId":{id},"lastPrice":"{price}","coinVlm":"3","dayNtlVlm":"3000"}}}}"#
        ),
    );
}

#[test]
fn ticker_update_rejects_unknown_ids_and_tracks_current_table_identity() {
    let (mut engine, _, api) = support::ready();
    let btc = engine.store().ticker("BTC-USDC").unwrap().clone();
    engine.store_mut().set_tickers(vec![
        Ticker {
            symbol: "UNRELATED".into(),
            last: 17.,
            ..Default::default()
        },
        btc,
    ]);
    tick(&mut engine, api, 1, 990);
    assert_eq!(engine.store().ticker("BTC-USDC").unwrap().last, 990.);
    assert_eq!(engine.store().ticker("UNRELATED").unwrap().last, 17.);
    tick(&mut engine, api, u32::MAX, 42);
    assert_eq!(engine.store().ticker("BTC-USDC").unwrap().last, 990.);
    assert_eq!(engine.store().tickers().len(), 2);
}

#[test]
fn a_missing_seed_row_keeps_the_native_loaded_table_rule() {
    let (mut engine, _, api) = support::ready();
    engine.store_mut().set_tickers(Vec::new());
    tick(&mut engine, api, 1, 990);
    assert!(engine.store().tickers().is_empty());
    assert!(engine.store().loaded().tickers);
}
