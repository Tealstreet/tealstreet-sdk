//! Deterministic allocation regression, not a timing benchmark.
mod support;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use support::Harness;

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
struct Counted;
fn note() {
    let _ = ALLOCATIONS.try_with(|slot| {
        if let Some(count) = slot.get() {
            slot.set(Some(count + 1));
        }
    });
}
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        note();
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counted = Counted;

fn allocations(run: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|slot| slot.set(None));
        }
    }
    ALLOCATIONS.with(|slot| {
        assert!(slot.get().is_none());
        slot.set(Some(0));
    });
    let reset = Reset;
    run();
    let count = ALLOCATIONS.with(|slot| slot.get().unwrap());
    drop(reset);
    count
}

#[test]
fn repeated_protective_quote_observations_do_not_allocate_a_throwaway_symbol() {
    const QUOTE: &str = r#"{"arg":{"channel":"ticker"},"data":[{"symbol":"BTCUSDT","lastPr":"100","markPrice":"101"}]}"#;
    const NO_QUOTE: &str = r#"{"arg":{"channel":"ticker"},"data":[{"symbol":"BTCUSDT"}]}"#;
    let mut h = Harness::public();
    h.message(QUOTE);
    h.message(NO_QUOTE);
    let without_quote = allocations(|| h.message(NO_QUOTE));
    let with_quote = allocations(|| h.message(QUOTE));
    assert_eq!(
        with_quote, without_quote,
        "an existing quote-cache key must be borrowed instead of allocated and dropped"
    );
    let ticker = h.engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!((ticker.last, ticker.mark), (100.0, 101.0));
}
