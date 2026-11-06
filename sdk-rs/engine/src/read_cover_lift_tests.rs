//! The page's lift rule (docs/plans/RUST_FRAME_FED_READ_COVER.md §9.3): a
//! source advertising `order_read_covers` lifts a fence only through its read
//! covers, per symbol; one that does not keeps the frame-stamp rule.
use crate::book::Book;
use crate::engine::{Command, Cx, Engine, Venue};
use crate::order_read_cover::{OrderReadMark, OrderReadScope};
use crate::wire::{FrameWriter, section};
use crate::{Capabilities, Response, SocketId, Store, TimerId};

struct Page {
    observed: Vec<Option<crate::ingest::ReadCovers>>,
}

impl Venue for Page {
    type Pending = ();
    fn on_frame_ingested(&mut self, cx: &mut Cx<'_, ()>) {
        self.observed.push(cx.source_orders_covers().cloned());
    }
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
    let mut engine = crate::test_support::seeded_engine(Page { observed: vec![] });
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
            epoch: 0,
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

/// The `orders.held` asks emitted since the last call: (symbols, liftAfterMs).
fn held(engine: &mut Engine<Page>) -> Vec<(String, f64)> {
    crate::test_support::prepared_actions(engine, 0.)
        .into_iter()
        .filter_map(|action| match action {
            crate::Action::Notice(crate::Notice::Event(event))
                if event.code == crate::log::code::ORDERS_HELD =>
            {
                let field = |name| event.fields.iter().find(|(key, _)| *key == name);
                let Some((_, crate::log::Value::Str(symbols))) = field("symbols") else {
                    panic!("symbols: {event:?}");
                };
                let Some((_, crate::log::Value::Num(after))) = field("liftAfterMs") else {
                    panic!("liftAfterMs: {event:?}");
                };
                Some((symbols.clone(), *after))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_fence_asks_once_for_its_symbols_timed_for_its_grace() {
    let mut engine = fenced_page(true);
    assert_eq!(
        held(&mut engine),
        [("BTCUSDT".to_owned(), crate::frame_fed::FENCE_GRACE_MS)]
    );
    engine.reconcile_uncertain(MARKED + 10.);
    assert!(
        held(&mut engine).is_empty(),
        "asked once per standing fence"
    );
}

#[test]
fn a_source_without_the_bit_is_never_asked() {
    let mut engine = fenced_page(false);
    assert!(held(&mut engine).is_empty());
}

/// A symbol fenced while an ask is out is named in a new ask with the others;
/// the timing stays the oldest mark's.
#[test]
fn a_newly_fenced_symbol_is_asked_for_beside_the_standing_ones() {
    let mut engine = fenced_page(true);
    held(&mut engine);
    engine.io.core.order_locks.fence(Some(&["ETHUSDT".into()]));
    engine.reconcile_uncertain(MARKED + 1_000.);
    assert_eq!(
        held(&mut engine),
        [(
            "BTCUSDT,ETHUSDT".to_owned(),
            crate::frame_fed::FENCE_GRACE_MS - 1_000.
        )]
    );
}

/// After a partial lift the fences still standing are asked for afresh; after
/// a full lift nothing is.
#[test]
fn a_partial_lift_re_asks_for_what_still_stands() {
    let mut engine = fenced_page(true);
    engine.io.core.order_locks.fence(Some(&["ETHUSDT".into()]));
    engine.reconcile_uncertain(MARKED + 500.);
    held(&mut engine);
    let btc = source(FRESH, None, &[Read::Symbol("BTCUSDT", FRESH)]);
    engine.apply_frame(FRESH, &btc, false).unwrap();
    assert_eq!(
        held(&mut engine),
        [(
            "ETHUSDT".to_owned(),
            MARKED + 500. + crate::frame_fed::FENCE_GRACE_MS - FRESH
        )]
    );
    let account = source(FRESH + 500., None, &[Read::Account(FRESH + 500.)]);
    engine.apply_frame(FRESH + 500., &account, false).unwrap();
    assert!(!fenced(&engine, "ETHUSDT"));
    assert!(held(&mut engine).is_empty());
}

#[test]
fn an_all_symbols_fence_asks_for_the_account() {
    let mut engine = page();
    engine
        .apply_frame(0., &source(0., Some(true), &[]), true)
        .unwrap();
    fence(&mut engine, None);
    assert_eq!(
        held(&mut engine),
        [(String::new(), crate::frame_fed::FENCE_GRACE_MS)]
    );
}

fn strip_sections(frame: &[u8], drop: &[u16]) -> Vec<u8> {
    let mut out = frame[..16].to_vec();
    let mut at = 16;
    let mut count = 0u16;
    while at < frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        let end = at + 8 + len;
        if !drop.contains(&kind) {
            out.extend_from_slice(&frame[at..end]);
            count += 1;
        }
        at = end;
    }
    out[6..8].copy_from_slice(&count.to_le_bytes());
    out
}
fn fence_at(engine: &mut Engine<Page>, symbols: Option<&[&str]>, now: f64) {
    let symbols: Option<Vec<String>> = symbols.map(|s| s.iter().map(|s| s.to_string()).collect());
    engine.io.core.order_locks.fence(symbols.as_deref());
    engine.reconcile_uncertain(now);
}
#[test]
fn valid_rollback_snapshot_clears_old_bit_and_restores_legacy_rule() {
    let mut e = fenced_page(true);
    let account = source(FRESH + 1000., None, &[Read::Account(0.)]);
    e.apply_frame(FRESH + 1000., &account, true).unwrap();
    assert!(fenced(&e, "BTCUSDT"));
    let old_source = source(FRESH + 2000., Some(false), &[]);
    e.apply_frame(FRESH + 2000., &old_source, true).unwrap();
    assert!(!e.source_read_covers);
    assert!(
        !fenced(&e, "BTCUSDT"),
        "old source returns to accepted legacy stamp rule"
    );
    fence_at(&mut e, Some(&["BTCUSDT"]), FRESH + 3000.);
    let later = source(FRESH + 3000. + crate::frame_fed::FENCE_GRACE_MS, None, &[]);
    e.apply_frame(
        FRESH + 3000. + crate::frame_fed::FENCE_GRACE_MS,
        &later,
        false,
    )
    .unwrap();
    assert!(!fenced(&e, "BTCUSDT"));
}
#[test]
fn upgrade_snapshot_uses_cover_rule_before_its_fresh_stamp() {
    let mut e = fenced_page(false);
    let frame = source(FRESH + 10000., Some(true), &[Read::Account(0.)]);
    e.apply_frame(FRESH + 10000., &frame, true).unwrap();
    assert!(fenced(&e, "BTCUSDT"));
}
#[test]
fn cold_legacy_source_without_any_capabilities_keeps_stamp_rule() {
    let mut e = page();
    fence_at(&mut e, Some(&["BTCUSDT"]), MARKED);
    let frame = source(FRESH + 1., None, &[]);
    e.apply_frame(FRESH + 1., &frame, true).unwrap();
    assert!(!fenced(&e, "BTCUSDT"));
}
#[test]
fn orders_really_without_23_do_not_lift_cover_source() {
    let mut e = fenced_page(true);
    let frame = strip_sections(
        &source(FRESH + 10000., None, &[Read::Account(FRESH)]),
        &[section::ORDERS_READ],
    );
    assert!(!crate::wire::sections(&frame).any(|(kind, _)| kind == section::ORDERS_READ));
    e.apply_frame(FRESH + 10000., &frame, false).unwrap();
    assert!(fenced(&e, "BTCUSDT"));
}
#[test]
fn detached_23_is_not_cached_to_lift_later_orders() {
    let mut e = fenced_page(true);
    let frame = source(FRESH + 10000., None, &[Read::Account(FRESH)]);
    let cover_only = strip_sections(&frame, &[section::ORDERS]);
    e.apply_frame(FRESH + 10000., &cover_only, false).unwrap();
    assert!(fenced(&e, "BTCUSDT"));
    let orders_only = strip_sections(&frame, &[section::ORDERS_READ]);
    e.apply_frame(FRESH + 10001., &orders_only, false).unwrap();
    assert!(fenced(&e, "BTCUSDT"));
    e.apply_frame(FRESH + 10002., &frame, false).unwrap();
    assert!(!fenced(&e, "BTCUSDT"));
}
#[test]
fn malformed_cover_payloads_are_absent_and_hold_fence() {
    for mode in 0..4 {
        let mut e = fenced_page(true);
        let mut frame = source(FRESH + 10000., None, &[Read::Account(FRESH)]);
        let mut at = 16;
        while at < frame.len() {
            let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
            let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
            if kind == section::ORDERS_READ {
                match mode {
                    0 => frame[at + 2..at + 4].copy_from_slice(&2u16.to_le_bytes()),
                    1 => frame[at + 16..at + 24].copy_from_slice(&f64::INFINITY.to_le_bytes()),
                    2 => frame[at + 8..at + 12].copy_from_slice(&u32::MAX.to_le_bytes()),
                    _ => frame[at + 16..at + 24].copy_from_slice(&f64::NEG_INFINITY.to_le_bytes()),
                }
                break;
            }
            at += 8 + len;
        }
        e.apply_frame(FRESH + 10000., &frame, false).unwrap();
        assert!(fenced(&e, "BTCUSDT"), "mode{mode}");
    }
}
#[test]
fn partial_lift_and_second_lost_write_keep_each_seals_time() {
    let mut e = fenced_page(true);
    fence_at(&mut e, Some(&["ETHUSDT"]), MARKED + 500.);
    let frame = source(FRESH + 1000., None, &[Read::Symbol("BTCUSDT", FRESH)]);
    e.apply_frame(FRESH + 1000., &frame, false).unwrap();
    assert!(!fenced(&e, "BTCUSDT"));
    assert!(fenced(&e, "ETHUSDT"));
    let second = FRESH + 1001.;
    fence_at(&mut e, Some(&["BTCUSDT"]), second);
    let restamped = source(second + 60000., None, &[Read::Symbol("BTCUSDT", FRESH)]);
    e.apply_frame(second + 60000., &restamped, false).unwrap();
    assert!(fenced(&e, "BTCUSDT"));
    assert!(fenced(&e, "ETHUSDT"));
    let fresh = source(
        second + 60001.,
        None,
        &[Read::Symbol(
            "BTCUSDT",
            second + crate::frame_fed::FENCE_GRACE_MS,
        )],
    );
    e.apply_frame(second + 60001., &fresh, false).unwrap();
    assert!(!fenced(&e, "BTCUSDT"));
    assert!(fenced(&e, "ETHUSDT"));
}
#[test]
fn account_cover_lifts_old_all_fence_but_keeps_newer_specific_seal() {
    let mut e = page();
    e.apply_frame(0., &source(0., Some(true), &[]), true)
        .unwrap();
    fence_at(&mut e, None, MARKED);
    fence_at(&mut e, Some(&["ETHUSDT"]), MARKED + 1000.);
    e.apply_frame(
        FRESH + 10000.,
        &source(FRESH + 10000., None, &[Read::Account(FRESH)]),
        false,
    )
    .unwrap();
    assert!(!fenced(&e, "BTCUSDT"));
    assert!(fenced(&e, "ETHUSDT"));
    e.apply_frame(
        FRESH + 10001.,
        &source(FRESH + 10001., None, &[Read::Account(FRESH + 1000.)]),
        false,
    )
    .unwrap();
    assert!(!fenced(&e, "ETHUSDT"));
}
#[test]
fn measured_and_unsynced_clock_graces_are_applied_to_read_time() {
    for offset in [None, Some(1500.), Some(-1500.)] {
        let mut e = fenced_page(true);
        e.set_source_clock_offset(offset);
        let threshold = MARKED
            + offset.unwrap_or(0.)
            + if offset.is_some() {
                crate::frame_fed::FENCE_GRACE_MS
            } else {
                crate::frame_fed::UNSYNCED_FENCE_GRACE_MS
            };
        e.apply_frame(
            threshold + 10000.,
            &source(threshold + 10000., None, &[Read::Account(threshold - 1.)]),
            false,
        )
        .unwrap();
        assert!(fenced(&e, "BTCUSDT"));
        e.apply_frame(
            threshold + 10001.,
            &source(threshold + 10001., None, &[Read::Account(threshold)]),
            false,
        )
        .unwrap();
        assert!(!fenced(&e, "BTCUSDT"));
    }
}

#[test]
fn source_cover_cutover_keeps_account_and_stamp_host_times() {
    for (bit, expected) in [(true, 6_750.), (false, 16_750.)] {
        let mut engine = page();
        engine.set_source_clock_offset(Some(250.));
        engine
            .apply_frame(
                20_000.,
                &source(20_000., Some(bit), &[Read::Account(10_000.)]),
                true,
            )
            .unwrap();
        let observed = engine.venue.observed.last().unwrap().as_ref().unwrap();
        assert_eq!(observed.account, Some(expected));
        assert_eq!(observed.cover("BTCUSDT"), Some(expected));
        assert_eq!(observed.cover("ETHUSDT"), Some(expected));
        assert!(engine.io.core.source_orders_covers.is_none());
    }
}
#[test]
fn source_cover_cutover_preserves_symbol_isolation_and_unsynced_grace() {
    let mut engine = page();
    engine.set_source_clock_offset(None);
    engine
        .apply_frame(
            50_000.,
            &source(
                50_000.,
                Some(true),
                &[
                    Read::Symbol("BTCUSDT", 40_000.),
                    Read::Symbol("ETHUSDT", 45_000.),
                ],
            ),
            true,
        )
        .unwrap();
    let observed = engine.venue.observed.last().unwrap().as_ref().unwrap();
    assert_eq!(observed.account, None);
    assert_eq!(observed.cover("BTCUSDT"), Some(10_000.));
    assert_eq!(observed.cover("ETHUSDT"), Some(15_000.));
    assert_eq!(observed.cover("SOLUSDT"), None);
    assert!(engine.io.core.source_orders_covers.is_none());
}
