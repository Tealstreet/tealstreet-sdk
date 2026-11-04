//! The page's lift rule (docs/plans/RUST_FRAME_FED_READ_COVER.md §9.3): a
//! source advertising `order_read_covers` lifts a fence only through its read
//! covers, per symbol; one that does not keeps the frame-stamp rule.
use crate::book::Book;
use crate::engine::{Command, Cx, Engine, Venue};
use crate::order_read_cover::{OrderReadMark, OrderReadScope};
use crate::wire::{FrameWriter, section};
use crate::{Capabilities, Response, SocketId, Store, TimerId};

struct Page;

impl Venue for Page {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

/// When the fences below are raised, on both clocks (the offset is measured
/// as zero, so the grace is `FENCE_GRACE_MS`).
const MARKED: f64 = 1_000.;
const FRESH: f64 = MARKED + crate::frame_fed::FENCE_GRACE_MS;

fn page() -> Engine<Page> {
    let mut engine = crate::test_support::seeded_engine(Page);
    engine.set_frame_fed().unwrap();
    engine.set_source_clock_offset(Some(0.));
    engine
}

/// An uncertain fence on `symbols` (`None`: every symbol), marked at `MARKED`.
fn fence(engine: &mut Engine<Page>, symbols: Option<&[&str]>) {
    let symbols: Option<Vec<String>> = symbols.map(|s| s.iter().map(|&s| s.into()).collect());
    engine.io.core.order_locks.fence(symbols.as_deref());
    engine.reconcile_uncertain(MARKED);
}

fn fenced(engine: &Engine<Page>, symbol: &str) -> bool {
    engine.io.core.order_locks.uncertain(symbol)
}

enum Read {
    Account(f64),
    Symbol(&'static str, f64),
}

/// A source frame written at `stamp`: its ORDERS (no rows), the covers of
/// `reads`, and its CAPABILITIES when `bit` says what it advertises.
fn source(stamp: f64, bit: Option<bool>, reads: &[Read]) -> Vec<u8> {
    let mut store = Store::default();
    store.set_orders(vec![]);
    for read in reads {
        let (sent, scope) = match read {
            Read::Account(sent) => (
                *sent,
                OrderReadScope::Account {
                    key: "all",
                    account: &["all"],
                },
            ),
            Read::Symbol(symbol, sent) => {
                (*sent, OrderReadScope::Symbols(std::slice::from_ref(symbol)))
            }
        };
        let read = OrderReadMark {
            seq: store.order_read_sequence(),
            sent,
        };
        store.commit_orders_read(read, scope, |_| true, vec![], &[]);
    }
    let mut writer = FrameWriter::new(Vec::new(), stamp);
    if let Some(bit) = bit {
        Capabilities {
            order_read_covers: bit,
            ..Capabilities::BASE
        }
        .write(&mut writer);
    }
    store.write(&store.account_snapshot(), &mut writer);
    writer.finish()
}

/// A page whose source's snapshot said `bit`, with a BTCUSDT fence.
fn fenced_page(bit: bool) -> Engine<Page> {
    let mut engine = page();
    engine
        .apply_frame(0., &source(0., Some(bit), &[]), true)
        .unwrap();
    fence(&mut engine, Some(&["BTCUSDT"]));
    engine
}

#[test]
fn a_source_without_the_bit_keeps_the_stamp_rule_h2_included() {
    let mut engine = fenced_page(false);
    // A read sent before the fence, written out well after it: master lifts.
    let late = source(FRESH + 1., None, &[Read::Account(0.)]);
    engine.apply_frame(FRESH + 1., &late, false).unwrap();
    assert!(!fenced(&engine, "BTCUSDT"), "the stamp rule, as on master");
}

/// H2: the read went out before the fence; however late its frame, it lifts
/// nothing. A read sent a grace after the mark does.
#[test]
fn with_the_bit_only_a_read_sent_well_after_the_mark_lifts() {
    let mut engine = fenced_page(true);
    let late = source(FRESH + 60_000., None, &[Read::Account(0.)]);
    engine.apply_frame(FRESH + 60_000., &late, false).unwrap();
    assert!(fenced(&engine, "BTCUSDT"), "H2: an old read, however late");
    let early = source(FRESH + 60_000., None, &[Read::Account(FRESH - 1.)]);
    engine.apply_frame(FRESH + 60_000., &early, false).unwrap();
    assert!(fenced(&engine, "BTCUSDT"), "inside the grace");
    let fresh = source(FRESH + 60_000., None, &[Read::Account(FRESH)]);
    engine.apply_frame(FRESH + 60_000., &fresh, false).unwrap();
    assert!(!fenced(&engine, "BTCUSDT"));
}

#[test]
fn a_symbols_read_lifts_its_own_fence_and_leaves_the_others() {
    let mut engine = fenced_page(true);
    fence(&mut engine, Some(&["ETHUSDT"]));
    let frame = source(FRESH, None, &[Read::Symbol("BTCUSDT", FRESH)]);
    engine.apply_frame(FRESH, &frame, false).unwrap();
    assert!(!fenced(&engine, "BTCUSDT"));
    assert!(fenced(&engine, "ETHUSDT"));
    // ETH's mark survives the prune: a later ETH read still finds it.
    let frame = source(FRESH, None, &[Read::Symbol("ETHUSDT", FRESH)]);
    engine.apply_frame(FRESH, &frame, false).unwrap();
    assert!(!fenced(&engine, "ETHUSDT"));
}

#[test]
fn the_all_symbols_fence_lifts_only_on_the_account_cover() {
    let mut engine = page();
    engine
        .apply_frame(0., &source(0., Some(true), &[]), true)
        .unwrap();
    fence(&mut engine, None);
    let symbols = source(
        FRESH,
        None,
        &[
            Read::Symbol("BTCUSDT", FRESH),
            Read::Symbol("ETHUSDT", FRESH),
        ],
    );
    engine.apply_frame(FRESH, &symbols, false).unwrap();
    assert!(
        fenced(&engine, "SOLUSDT"),
        "a symbol cover can't speak for every symbol"
    );
    let account = source(FRESH, None, &[Read::Account(FRESH)]);
    engine.apply_frame(FRESH, &account, false).unwrap();
    assert!(!fenced(&engine, "SOLUSDT"));
}

#[test]
fn with_the_bit_orders_without_a_valid_cover_lift_nothing() {
    let mut engine = fenced_page(true);
    let bare = source(FRESH + 60_000., None, &[]);
    engine.apply_frame(FRESH + 60_000., &bare, false).unwrap();
    assert!(fenced(&engine, "BTCUSDT"), "ORDERS without 23");
    // An `ORDERS_READ` of another revision beside a fresh ORDERS.
    let mut writer = FrameWriter::new(Vec::new(), FRESH + 60_000.);
    let store = {
        let mut store = Store::default();
        store.set_orders(vec![]);
        store
    };
    store.write(&store.account_snapshot(), &mut writer);
    writer.begin_revision(section::ORDERS_READ, 0);
    writer.f64(FRESH + 1.);
    writer.end();
    engine
        .apply_frame(FRESH + 60_000., &writer.finish(), false)
        .expect("never an ingest error");
    assert!(fenced(&engine, "BTCUSDT"), "a malformed 23");
}

/// H4: a snapshot without CAPABILITIES (`account_snapshot_frame`) keeps the
/// bit of the last one that had them, so its late stamp lifts nothing.
#[test]
fn a_snapshot_without_capabilities_keeps_the_bit() {
    let mut engine = fenced_page(true);
    let account = source(FRESH + 60_000., None, &[Read::Account(0.)]);
    engine.apply_frame(FRESH + 60_000., &account, true).unwrap();
    assert!(fenced(&engine, "BTCUSDT"));
}
