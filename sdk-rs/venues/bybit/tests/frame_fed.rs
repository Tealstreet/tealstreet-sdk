//! A frame-fed Bybit engine: public instrument metadata only, no sockets, no
//! account reads or polls, and only writes (after markets load) are executed.
mod support;
use safe_cex_engine::Command;
use safe_cex_engine::frame_fed::{DATA_SOURCE_ONLY, MARKETS_NOT_LOADED};
use support::{Harness, Options};

fn frame_fed() -> Harness {
    let mut engine = support::engine(Options {
        key: Some("fixture-public".into()),
        secret: Some("fixture-provider".into()),
        ..Default::default()
    });
    engine.set_frame_fed().unwrap();
    Harness::started_engine(engine)
}

fn load(h: &mut Harness) {
    let paths = h.paths();
    assert!(
        paths
            .iter()
            .all(|path| path.contains("/v5/market/instruments-info")),
        "only public instrument metadata: {paths:?}"
    );
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    assert!(
        h.requests.is_empty(),
        "nothing after metadata: {:?}",
        h.paths()
    );
}

#[test]
fn a_frame_fed_engine_loads_public_metadata_once_and_opens_no_socket() {
    let mut h = frame_fed();
    load(&mut h);
    assert!(h.engine.store().loaded().markets);
    h.advance(120_000.0);
    assert!(h.requests.is_empty(), "no polls: {:?}", h.paths());
    assert!(h.sockets.is_empty(), "no sockets: {:?}", h.sockets);
}

#[test]
fn writes_before_markets_and_non_writes_are_refused_without_a_request() {
    let mut h = frame_fed();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![support::order()],
    });
    assert!(
        matches!(h.result(1), Some(Err(message)) if message.contains(MARKETS_NOT_LOADED)),
        "{:?}",
        h.result(1)
    );
    load(&mut h);
    h.command(Command::FetchAccount { command: 2 });
    assert!(
        matches!(h.result(2), Some(Err(message)) if message.contains(DATA_SOURCE_ONLY)),
        "{:?}",
        h.result(2)
    );
    assert!(h.requests.is_empty());
}

#[test]
fn a_frame_fed_engine_ingests_frames_and_a_normal_engine_refuses_them() {
    let mut h = frame_fed();
    load(&mut h);
    let frame = h.engine.snapshot_frame(h.now).to_vec();
    assert!(h.engine.apply_frame(h.now, &frame, true).is_ok());
    assert!(h.engine.apply_frame(h.now, b"not a frame", false).is_err());
    let mut normal = Harness::started(Options::default());
    assert!(normal.engine.apply_frame(normal.now, &frame, true).is_err());
    normal.collect();
}
