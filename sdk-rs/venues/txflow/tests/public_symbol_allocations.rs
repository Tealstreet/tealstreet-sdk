//! Context updates borrow the resolved symbol just like instrument-ID tickers.
mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! { static SYMBOL_ALLOCS: Cell<Option<usize>> = const { Cell::new(None) }; }
struct Counter;
fn allocated(size: usize) {
    // The fixture's symbol is BTC-USDC. Count its clones, including the Store's
    // debug identity guard, without coupling the test to unrelated allocations.
    if size == "BTC-USDC".len() {
        let _ = SYMBOL_ALLOCS.try_with(|calls| {
            if let Some(count) = calls.get() {
                calls.set(Some(count + 1));
            }
        });
    }
}
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocated(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocated(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocated(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;

fn allocations(text: &str) -> usize {
    let (mut engine, _, api) = support::ready();
    SYMBOL_ALLOCS.with(|calls| calls.set(Some(0)));
    support::message(&mut engine, api, 4000., text);
    let calls = SYMBOL_ALLOCS.with(|calls| calls.replace(None).unwrap());
    assert_eq!(engine.store().ticker("BTC-USDC").unwrap().mark, 66000.);
    calls
}

#[test]
fn contexts_do_not_clone_resolved_symbols() {
    let ticker = allocations(
        r#"{"channel":"ticker","data":{"instrumentId":1,"nodeCtx":{"markPx":"66000"}}}"#,
    );
    for coin in ["1", "BTC-USDC", "btc-usdc", "BtC-uSdC"] {
        for text in [
            format!(
                r#"{{"channel":"activeAssetCtx","data":{{"coin":"{coin}","ctx":{{"markPx":"66000"}}}}}}"#
            ),
            format!(
                r#"{{"channel":"allAssetCtxs","data":{{"ctxs":[{{"coin":"{coin}","ctx":{{"markPx":"66000"}}}}]}}}}"#
            ),
        ] {
            // Case folding may allocate the alias; a resolved-symbol clone must
            // never add another allocation. Numeric/lowercase aliases borrow.
            let folded = usize::from(coin.bytes().any(|c| c.is_ascii_uppercase()));
            assert_eq!(allocations(&text), ticker + folded, "{text}");
        }
    }
}
