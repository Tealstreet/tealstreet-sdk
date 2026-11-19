//! Scoped native allocation accounting; other test threads are unmeasured.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! { static LIVE:Cell<Option<isize>>=const {Cell::new(None)}; }
fn change(bytes: isize) {
    let _ = LIVE.try_with(|live| {
        if let Some(value) = live.get() {
            live.set(Some(value + bytes));
        }
    });
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        change(layout.size() as isize);
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        change(layout.size() as isize);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        change(-(layout.size() as isize));
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        change(size as isize - layout.size() as isize);
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
pub(crate) fn retained(run: impl FnOnce()) -> isize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            LIVE.with(|live| live.set(None));
        }
    }
    LIVE.with(|live| {
        assert!(live.get().is_none());
        live.set(Some(0));
    });
    let reset = Reset;
    run();
    let bytes = LIVE.with(|live| live.get().unwrap());
    drop(reset);
    bytes
}
