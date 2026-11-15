use crate::{BitgetFutures, Options};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }

struct CountingAllocator;

fn allocated() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(previous) = count.get() {
            count.set(Some(previous + 1));
        }
    });
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocated();
        unsafe { System.realloc(pointer, layout, size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations(run: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|count| count.set(None));
        }
    }
    ALLOCATIONS.with(|count| count.set(Some(0)));
    let _reset = Reset;
    run();
    ALLOCATIONS.with(|count| count.get().unwrap())
}

#[test]
fn protective_quote_updates_reuse_known_symbols() {
    let mut venue = BitgetFutures::new(Options::default());
    venue.observe_protective_quote("BTCUSDT", Some(50.0), Some(100.0), 0.0);
    let count = allocations(|| {
        for step in 0..1_000 {
            let offset = f64::from(step);
            std::hint::black_box(&mut venue).observe_protective_quote(
                "BTCUSDT",
                Some(50.0 + offset),
                (step % 2 == 0).then_some(100.0 + offset),
                offset + 1.0,
            );
        }
        venue.observe_protective_quote("BTCUSDT", None, None, 2_000.0);
        venue.observe_protective_quote("absent", None, None, 2_000.0);
    });
    assert_eq!(count, 0, "known ticker symbols need no fresh allocation");
    let quote = &venue.trading.references["BTCUSDT"];
    assert_eq!(quote.last, Some((1_049.0, 1_000.0)));
    assert_eq!(quote.mark, Some((1_098.0, 999.0)));
    assert!(!venue.trading.references.contains_key("absent"));
    venue.observe_protective_quote("ETHUSDT", None, Some(200.0), 3_000.0);
    let quote = &venue.trading.references["ETHUSDT"];
    assert_eq!(quote.last, None);
    assert_eq!(quote.mark, Some((200.0, 3_000.0)));
}
