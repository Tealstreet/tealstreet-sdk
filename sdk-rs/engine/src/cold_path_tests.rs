//! The venue-independent halves of shared-ticker restoration and the
//! `orders.held` notice, pinned to the behaviour they had inline.
use super::{HashMap, Store, copy_shared_tickers};
use crate::model::{Limits, Market, MinMax, Precision, Ticker};
use crate::public_feed::PublicTicker;

fn market(symbol: &str) -> Market {
    Market {
        id: format!("native-{symbol}"),
        symbol: symbol.into(),
        pretty_symbol: None,
        base: symbol.into(),
        quote: "USD".into(),
        active: true,
        precision: Precision {
            amount: 0.1,
            price: 0.5,
        },
        limits: Limits {
            amount: MinMax { min: 1., max: 100. },
            leverage: MinMax { min: 1., max: 10. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}

fn quote(json: &str) -> PublicTicker {
    serde_json::from_str(json).unwrap()
}

#[test]
fn shared_quotes_add_missing_rows_then_overwrite_only_the_fields_they_carry() {
    let mut store = Store::default();
    store.set_markets(vec![market("BTC"), market("ETH")]);
    store.set_tickers(vec![Ticker {
        id: "native-ETH".into(),
        symbol: "ETH".into(),
        bid: 1.,
        last: 7.,
        ..Default::default()
    }]);
    let shared: HashMap<String, PublicTicker> = [
        ("BTC".into(), quote(r#"{"symbol":"BTC","bid":10,"ask":11}"#)),
        ("ETH".into(), quote(r#"{"symbol":"ETH","bid":2}"#)),
        ("GONE".into(), quote(r#"{"symbol":"GONE","bid":3}"#)),
    ]
    .into_iter()
    .collect();
    let before = store.ticker_revision();
    copy_shared_tickers(&mut store, &shared);
    assert_ne!(store.ticker_revision(), before);
    let btc = store.ticker("BTC").unwrap();
    assert_eq!(
        (btc.id.as_str(), btc.bid, btc.ask),
        ("native-BTC", 10., 11.)
    );
    let eth = store.ticker("ETH").unwrap();
    assert_eq!(
        (eth.bid, eth.last),
        (2., 7.),
        "omitted fields keep the venue's"
    );
    assert!(store.ticker("GONE").is_none(), "no row without a market");
    assert_eq!(store.tickers().len(), 2);
}

#[cfg(feature = "order-locks")]
#[test]
fn orders_held_names_each_standing_fence_once_with_the_grace_of_its_clock() {
    use super::orders_held_notice;
    use crate::frame_fed::{FENCE_GRACE_MS, UNSYNCED_FENCE_GRACE_MS};
    let mut locks = crate::order_locks::OrderLocks::default();
    locks.fence(Some(&["ETHUSDT".into(), "BTCUSDT".into()]));
    let epoch = locks
        .uncertain_epochs()
        .map(|(_, epoch)| epoch)
        .max()
        .unwrap();
    let marks = [(epoch, 1_000.)];
    for (synced, grace) in [(true, FENCE_GRACE_MS), (false, UNSYNCED_FENCE_GRACE_MS)] {
        let mut held = vec!["SOLUSDT".to_owned()];
        assert_eq!(
            orders_held_notice(&locks, &mut held, &marks, synced, 1_500.),
            Some((
                "BTCUSDT,ETHUSDT".to_owned(),
                (1_000. + grace - 1_500.).max(0.)
            ))
        );
        assert_eq!(held, ["BTCUSDT", "ETHUSDT"], "names what stands, sorted");
        assert_eq!(
            orders_held_notice(&locks, &mut held, &marks, synced, 1_600.),
            None
        );
    }
    let mut held = Vec::new();
    assert_eq!(
        orders_held_notice(&locks, &mut held, &[], true, 0.),
        None,
        "no mark, no ask"
    );
    assert!(held.is_empty());
}

#[cfg(feature = "order-locks")]
#[test]
fn orders_held_forgets_lifted_names_floors_the_delay_and_asks_for_the_account() {
    use super::orders_held_notice;
    use crate::frame_fed::FENCE_GRACE_MS;
    let mut locks = crate::order_locks::OrderLocks::default();
    locks.fence(Some(&["ETHUSDT".into(), "BTCUSDT".into()]));
    let marks = [(u64::MAX, 1_000.)];
    let mut held = Vec::new();
    assert!(orders_held_notice(&locks, &mut held, &marks, true, 1_000.).is_some());
    // A lifted fence drops out of what was named, so fencing it again asks again.
    let btc = locks
        .uncertain_epochs()
        .find(|(s, _)| *s == "BTCUSDT")
        .unwrap()
        .1;
    locks.reconciled(Some("BTCUSDT"), btc);
    assert_eq!(
        orders_held_notice(&locks, &mut held, &marks, true, 1_000.),
        None
    );
    assert_eq!(held, ["ETHUSDT"]);
    locks.fence(Some(&["BTCUSDT".into()]));
    assert_eq!(
        orders_held_notice(&locks, &mut held, &marks, true, 1_000.),
        Some(("BTCUSDT,ETHUSDT".to_owned(), FENCE_GRACE_MS))
    );
    // Past the oldest mark's grace, the read can lift it now.
    held.clear();
    assert_eq!(
        orders_held_notice(
            &locks,
            &mut held,
            &marks,
            true,
            1_000. + FENCE_GRACE_MS + 5_000.
        ),
        Some(("BTCUSDT,ETHUSDT".to_owned(), 0.))
    );
    // An all-symbols fence standing beside named ones asks for the account.
    locks.fence(None);
    let (named, _) = orders_held_notice(&locks, &mut held, &marks, true, 1_000.).unwrap();
    assert_eq!(named, "");
}

#[test]
fn shared_reference_observations_keep_source_time_even_when_values_do_not_change() {
    use crate::model::StopTriggerType::{IndexPrice, LastPrice, MarkPrice};
    let mut store = Store::default();
    store.set_clock(1_000.);
    store.set_markets(vec![market("BTC")]);
    store.set_tickers(vec![Ticker {
        symbol: "BTC".into(),
        mark: 100.,
        last: 99.,
        index: 98.,
        ..Default::default()
    }]);
    let shared = |text| [("BTC".into(), quote(text))].into_iter().collect();
    store.set_clock(9_000.);
    copy_shared_tickers(
        &mut store,
        &shared(r#"{"symbol":"BTC","mark":100,"observedAt":[null,8500,null]}"#),
    );
    assert_eq!(store.reference_seen_at("BTC", MarkPrice), Some(8_500.));
    assert_eq!(store.reference_seen_at("BTC", LastPrice), Some(1_000.));
    assert_eq!(store.reference_seen_at("BTC", IndexPrice), Some(1_000.));
    store.set_clock(20_000.);
    copy_shared_tickers(
        &mut store,
        &shared(r#"{"symbol":"BTC","mark":100,"observedAt":[null,8500,null]}"#),
    );
    assert_eq!(
        store.reference_seen_at("BTC", MarkPrice),
        Some(8_500.),
        "replay and local restoration never date cached data as now"
    );
    assert!(crate::protective_amend::fresh_reference(&store, "BTC", MarkPrice, 20_000.).is_none());
    copy_shared_tickers(
        &mut store,
        &shared(r#"{"symbol":"BTC","mark":101,"observedAt":[null,null,null]}"#),
    );
    assert!(
        crate::protective_amend::fresh_reference(&store, "BTC", MarkPrice, 20_000.).is_none(),
        "explicitly unobserved values are not fresh"
    );
    copy_shared_tickers(&mut store, &shared(r#"{"symbol":"BTC","mark":102}"#));
    assert_eq!(
        store.reference_seen_at("BTC", MarkPrice),
        Some(20_000.),
        "old producer retains changed-value behavior"
    );
}

#[test]
fn unchanged_source_observation_is_exported_without_redating_other_fields() {
    use crate::wire::{FrameWriter, sections};
    let mut store = Store::default();
    store.export_ticker_observations = true;
    store.set_clock(1_000.);
    store.set_tickers(vec![Ticker {
        symbol: "BTC".into(),
        mark: 100.,
        ..Default::default()
    }]);
    store.take_pending();
    store.set_clock(8_500.);
    store.observe_ticker_fields("BTC", 1 << 3);
    let pending = store.take_pending();
    let mut writer = FrameWriter::new(vec![], 9_000.);
    store.write(&pending, &mut writer);
    let bytes = writer.finish();
    let (_, payload) = sections(&bytes)
        .find(|(kind, _)| *kind == 24)
        .expect("source observations must reach shared consumers even without a value delta");
    let numbers: Vec<_> = payload
        .as_chunks::<8>()
        .0
        .iter()
        .map(|v| f64::from_le_bytes(*v))
        .collect();
    assert_eq!(numbers, [0., 1_000., 8_500., 1_000.]);
}

#[test]
fn direct_account_ticker_frame_is_byte_identical_to_the_legacy_wire() {
    use crate::wire::FrameWriter;
    let mut store = Store::default();
    store.set_clock(1_000.);
    store.set_tickers(vec![Ticker {
        symbol: "BTC".into(),
        mark: 99.,
        ..Default::default()
    }]);
    store.take_pending();
    store.set_clock(2_000.);
    store.update_ticker("BTC", |ticker| ticker.mark = 100.);
    let pending = store.take_pending();
    let mut writer = FrameWriter::new(vec![], 2_100.);
    store.write(&pending, &mut writer);
    // Format 1, one revision-0 TICKER_DELTAS section, [row=0, mask=8, mark=100].
    let expected = [
        b"SCXF".as_slice(),
        &1u16.to_le_bytes(),
        &1u16.to_le_bytes(),
        &2100f64.to_le_bytes(),
        &4u16.to_le_bytes(),
        &0u16.to_le_bytes(),
        &24u32.to_le_bytes(),
        &0f64.to_le_bytes(),
        &8f64.to_le_bytes(),
        &100f64.to_le_bytes(),
    ]
    .concat();
    assert_eq!(writer.finish(), expected);
    store.set_clock(3_000.);
    store.observe_ticker_fields("BTC", 1 << 3);
    let mut writer = FrameWriter::new(vec![], 3_100.);
    let pending = store.take_pending();
    store.write(&pending, &mut writer);
    assert_eq!(
        writer.finish(),
        [
            b"SCXF".as_slice(),
            &1u16.to_le_bytes(),
            &0u16.to_le_bytes(),
            &3100f64.to_le_bytes()
        ]
        .concat()
    );
}

#[test]
fn source_observations_survive_frame_fed_snapshots_and_time_only_deltas() {
    use crate::{ingest, model::StopTriggerType::MarkPrice, wire::FrameWriter};
    let mut source = Store::default();
    source.export_ticker_observations = true;
    source.set_clock(1_000.);
    source.set_markets(vec![market("BTC")]);
    source.set_tickers(vec![Ticker {
        id: "native-BTC".into(),
        symbol: "BTC".into(),
        mark: 100.,
        ..Default::default()
    }]);
    let mut target = Store::default();
    target.set_clock(20_000.);
    let mut state = ingest::IngestState::default();
    let mut writer = FrameWriter::new(vec![], 19_000.);
    source.write(&source.everything(), &mut writer);
    let bytes = writer.finish();
    ingest::apply(
        &mut target,
        &mut state,
        &ingest::parse(&bytes).unwrap(),
        true,
    )
    .unwrap();
    assert_eq!(target.reference_seen_at("BTC", MarkPrice), Some(1_000.));
    source.take_pending();
    source.set_clock(8_500.);
    source.observe_ticker_fields("BTC", 1 << 3);
    let pending = source.take_pending();
    let mut writer = FrameWriter::new(vec![], 19_500.);
    source.write(&pending, &mut writer);
    let bytes = writer.finish();
    ingest::apply(
        &mut target,
        &mut state,
        &ingest::parse(&bytes).unwrap(),
        false,
    )
    .unwrap();
    assert_eq!(target.reference_seen_at("BTC", MarkPrice), Some(8_500.));
    assert!(crate::protective_amend::fresh_reference(&target, "BTC", MarkPrice, 20_000.).is_none());
}

#[test]
fn partial_shared_updates_cannot_freshen_cached_omitted_references() {
    let mut cached = quote(r#"{"symbol":"BTC","last":99,"mark":100,"index":98}"#);
    cached.merge(quote(
        r#"{"symbol":"BTC","bid":97,"observedAt":[9000,9000,9000]}"#,
    ));
    assert_eq!(cached.observed_at, Some([None; 3]));
    cached.merge(quote(
        r#"{"symbol":"BTC","mark":100,"observedAt":[null,8500,null]}"#,
    ));
    assert_eq!(cached.observed_at, Some([None, Some(8500.), None]));
    cached.merge(quote(
        r#"{"symbol":"BTC","last":99,"observedAt":[9000,null,null]}"#,
    ));
    assert_eq!(cached.observed_at, Some([Some(9000.), Some(8500.), None]));
    let mut store = Store::default();
    store.set_clock(20_000.);
    store.set_markets(vec![market("BTC")]);
    copy_shared_tickers(&mut store, &[("BTC".into(), cached)].into_iter().collect());
    assert_eq!(
        store.reference_seen_at("BTC", crate::model::StopTriggerType::MarkPrice),
        Some(8500.)
    );
    assert!(
        crate::protective_amend::fresh_reference(
            &store,
            "BTC",
            crate::model::StopTriggerType::MarkPrice,
            8000.
        )
        .is_none(),
        "a future source clock cannot grant freshness"
    );
}

#[test]
fn malformed_observation_batch_does_not_freshen_any_reference() {
    use crate::{ingest, model::StopTriggerType::MarkPrice, wire::FrameWriter};
    let mut target = Store::default();
    target.set_clock(1000.);
    target.set_tickers(vec![Ticker {
        symbol: "BTC".into(),
        mark: 100.,
        ..Default::default()
    }]);
    let mut state = ingest::IngestState::default();
    state.source_tickers = vec!["BTC".into()];
    for invalid in [f64::NAN, f64::INFINITY, -1.] {
        let mut writer = FrameWriter::new(vec![], 20_000.);
        writer.begin(24);
        writer.f64s([0., 19_000., 19_000., 19_000., 0., 19_000., invalid, 19_000.]);
        writer.end();
        let bytes = writer.finish();
        assert!(
            ingest::apply(
                &mut target,
                &mut state,
                &ingest::parse(&bytes).unwrap(),
                false
            )
            .is_err()
        );
        assert_eq!(target.reference_seen_at("BTC", MarkPrice), Some(1000.));
    }
}
