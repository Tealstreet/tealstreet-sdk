use super::*;
use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType, Position, PositionSide};
use std::collections::BTreeSet;

fn position(symbol: &str, contracts: f64, notional: f64) -> Position {
    Position {
        id: format!("{symbol}-long"),
        symbol: symbol.into(),
        side: PositionSide::Long,
        entry_price: 100.,
        notional,
        leverage: 1.,
        margin_type: None,
        position_mode: None,
        unrealized_pnl: 0.,
        realized_pnl: None,
        contracts,
        liquidation_price: 0.,
        margin_size: None,
        last_update: None,
    }
}
fn order(symbol: &str) -> Order {
    Order {
        id: "synthetic-open-order".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: symbol.into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 100.,
        amount: 1.,
        filled: 0.,
        remaining: 1.,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
fn source() -> Store {
    let mut source = Store::default();
    source.set_markets(vec![market("BTC"), market("ETH"), market("SOL")]);
    source.set_tickers(vec![
        ticker("BTC", 100.),
        ticker("ETH", 200.),
        ticker("SOL", 300.),
    ]);
    source
}
fn selection(symbols: &[&str]) -> TickerSelection {
    Some(Arc::new(
        symbols.iter().map(|symbol| (*symbol).to_owned()).collect(),
    ))
}
fn check_financial(view: &Store) {
    for position in view.positions() {
        if position.contracts != 0. || position.notional != 0. {
            assert!(
                view.ticker(&position.symbol).is_some(),
                "a live position has no ticker after an output frame"
            );
        }
    }
    for order in view.orders() {
        if !matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
            assert!(
                view.ticker(&order.symbol).is_some(),
                "an open order has no ticker after an output frame"
            );
        }
    }
}
fn apply_initial(view: &mut Store, state: &mut IngestState, frames: Vec<Arc<[u8]>>) {
    for bytes in frames {
        ingest::apply(view, state, &ingest::parse(&bytes).unwrap(), false).unwrap();
    }
    check_financial(view);
}
fn apply(view: &mut Store, state: &mut IngestState, frames: Vec<Arc<[u8]>>) {
    for bytes in frames {
        ingest::apply(view, state, &ingest::parse(&bytes).unwrap(), false).unwrap();
        check_financial(view);
    }
}
fn symbols(view: &Store) -> BTreeSet<&str> {
    view.tickers()
        .iter()
        .map(|row| row.symbol.as_str())
        .collect()
}
fn private_sections(frames: &[Arc<[u8]>]) -> Vec<(u16, u16, Vec<u8>)> {
    let mut result = Vec::new();
    for bytes in frames {
        let frame = ingest::parse(bytes).unwrap();
        for part in frame
            .sections
            .iter()
            .filter(|part| matches!(part.kind, section::ORDERS | section::POSITIONS))
        {
            result.push((part.kind, part.revision, part.payload.to_vec()));
        }
    }
    result
}

#[test]
fn selected_publication_keeps_financial_references_at_every_frame_boundary() {
    let mut source = source();
    source.set_positions(vec![position("BTC", 1., 100.)]);
    source.set_orders(vec![order("ETH")]);
    let mut publisher = Publisher::new(0, 1_000_000);
    publisher.set_tickers(selection(&[])).unwrap();
    let (mut view, mut state) = (Store::default(), IngestState::default());
    let original = encode(&mut source);
    let output = publisher.receive(original.clone(), true).unwrap();
    assert_eq!(private_sections(&output), private_sections(&[original]));
    assert!(
        !private_sections(&[output[0].clone()]).is_empty(),
        "the first snapshot chunk retains private snapshot semantics"
    );
    apply_initial(&mut view, &mut state, output);
    assert_eq!(symbols(&view), BTreeSet::from(["BTC", "ETH"]));
    source.set_positions(vec![position("SOL", 0., 5.)]);
    source.set_orders(Vec::new());
    apply(
        &mut view,
        &mut state,
        publisher.receive(encode(&mut source), false).unwrap(),
    );
    assert_eq!(symbols(&view), BTreeSet::from(["SOL"]));
    source.set_positions(Vec::new());
    apply(
        &mut view,
        &mut state,
        publisher.receive(encode(&mut source), false).unwrap(),
    );
    assert!(view.tickers().is_empty());
}

#[test]
fn paced_financial_only_updates_get_their_price_before_the_private_rows() {
    let mut source = source();
    let mut publisher = Publisher::new(5_000, 1_000_000);
    publisher.set_tickers(selection(&["ETH"])).unwrap();
    let (mut view, mut state) = (Store::default(), IngestState::default());
    apply_initial(
        &mut view,
        &mut state,
        publisher.receive(encode(&mut source), true).unwrap(),
    );
    source.set_positions(vec![position("SOL", 1., 300.)]);
    apply(
        &mut view,
        &mut state,
        publisher.receive(encode(&mut source), false).unwrap(),
    );
    assert_eq!(symbols(&view), BTreeSet::from(["ETH", "SOL"]));
}

#[test]
fn legacy_full_table_keeps_financial_only_bytes_unchanged() {
    let mut source = source();
    let mut publisher = Publisher::new(0, 1_000_000);
    publisher.receive(encode(&mut source), true).unwrap();
    source.set_positions(vec![position("SOL", 1., 300.)]);
    let bytes = encode(&mut source);
    let output = publisher.receive(bytes.clone(), false).unwrap();
    assert_eq!(output.len(), 1);
    assert!(Arc::ptr_eq(&bytes, &output[0]));
}

#[test]
fn adding_a_financial_reference_preserves_its_source_observation_times() {
    let mut source = source();
    let mut publisher = Publisher::new(0, 1_000_000);
    publisher.set_tickers(selection(&[])).unwrap();
    publisher.receive(encode(&mut source), true).unwrap();
    let mut observations = FrameWriter::new(Vec::new(), 20.);
    observations.begin(section::TICKER_OBSERVATIONS);
    observations.f64s([2., 2., 3., 4.]);
    observations.end();
    publisher
        .receive(observations.finish().into(), false)
        .unwrap();
    source.set_positions(vec![position("SOL", 1., 300.)]);
    let mut rows = Vec::new();
    for bytes in publisher.receive(encode(&mut source), false).unwrap() {
        let frame = ingest::parse(&bytes).unwrap();
        for part in frame
            .sections
            .iter()
            .filter(|part| part.kind == section::TICKER_OBSERVATIONS)
        {
            for row in part.payload.as_chunks::<32>().0 {
                rows.push(
                    row.as_chunks::<8>()
                        .0
                        .iter()
                        .map(|value| f64::from_le_bytes(*value))
                        .collect::<Vec<_>>(),
                );
            }
        }
    }
    assert_eq!(rows, [vec![0., 2., 3., 4.]]);
}

#[test]
fn unreadable_initial_snapshot_keeps_the_full_source_table_fallback() {
    let mut publisher = Publisher::new(0, 1_000_000);
    publisher.set_tickers(selection(&["BTC"])).unwrap();
    let mut writer = FrameWriter::new(Vec::new(), 10.);
    writer.begin_revision(section::TICKERS, u16::MAX);
    writer.u32s([0, 0]);
    writer.end();
    let future: Arc<[u8]> = writer.finish().into();
    let initial = publisher.receive(future.clone(), true).unwrap();
    assert!(Arc::ptr_eq(&initial[0], &future));
    let subsequent = publisher.receive(future.clone(), false).unwrap();
    assert!(Arc::ptr_eq(&subsequent[0], &future));
}

#[tokio::test(start_paused = true)]
async fn financial_baseline_restarts_public_cadence_without_delaying_private_rows() {
    let mut source = source();
    let mut publisher = Publisher::new(500, 1_000_000);
    publisher.set_tickers(selection(&[])).unwrap();
    let (mut view, mut state) = (Store::default(), IngestState::default());
    apply_initial(
        &mut view,
        &mut state,
        publisher.receive(encode(&mut source), true).unwrap(),
    );
    tokio::time::advance(Duration::from_millis(400)).await;
    source.set_positions(vec![position("SOL", 1., 300.)]);
    apply(
        &mut view,
        &mut state,
        publisher.receive(encode(&mut source), false).unwrap(),
    );
    assert_eq!(view.positions().len(), 1);
    source.update_ticker("SOL", |ticker| ticker.last = 333.);
    apply(
        &mut view,
        &mut state,
        publisher.receive(encode(&mut source), false).unwrap(),
    );
    assert_eq!(view.ticker("SOL").unwrap().last, 300.);
    assert_eq!(
        publisher.deadline(),
        Some(Instant::now() + Duration::from_millis(500))
    );
    tokio::time::advance(Duration::from_millis(500)).await;
    apply(&mut view, &mut state, publisher.flush().unwrap());
    assert_eq!(view.ticker("SOL").unwrap().last, 333.);
}
