use super::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! { static BYTES: Cell<Option<usize>> = const { Cell::new(None) }; }

fn record(bytes: usize) {
    let _ = BYTES.try_with(|counter| {
        if let Some(total) = counter.get() {
            counter.set(Some(total.saturating_add(bytes)));
        }
    });
}

struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn allocated<T>(run: impl FnOnce() -> T) -> (T, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            BYTES.with(|counter| counter.set(None));
        }
    }
    BYTES.with(|counter| assert!(counter.replace(Some(0)).is_none()));
    let reset = Reset;
    let result = run();
    let bytes = BYTES.with(|counter| counter.get().unwrap());
    drop(reset);
    (result, bytes)
}

#[test]
fn allocation_counter_records_a_real_allocation() {
    let (data, bytes) = allocated(|| vec![7_u8; 64]);
    assert_eq!(data.len(), 64);
    assert_eq!(bytes, 64);
}

#[test]
fn peer_attribution_does_not_copy_native_ids_or_client_echoes() {
    let peers: Vec<_> = (0..8)
        .map(|i| format!("peer-{i}-{}", "p".repeat(240)))
        .collect();
    let new_id = format!("new-{}", "n".repeat(240));
    let mut identities: Vec<_> = peers
        .iter()
        .enumerate()
        .map(|(i, id)| Identity {
            order_id: Some(id.clone()),
            order_link_id: Some(format!("old-client-{i}-{}", "c".repeat(240))),
        })
        .collect();
    identities.push(Identity {
        order_id: Some(new_id.clone()),
        order_link_id: Some("new-client".into()),
    });
    let statuses = (0..identities.len())
        .map(|_| Status { code: 0, msg: None })
        .collect();
    let peer_refs: Vec<_> = peers.iter().map(String::as_str).collect();
    let (result, bytes) =
        allocated(|| created_with_peers(identities, statuses, &[Some("new-client")], &peer_refs));
    assert_eq!(result, Ok(vec![Row::Accepted(new_id)]));
    assert!(
        bytes <= 1024,
        "peer attribution allocated {bytes} bytes after decoding owned identities"
    );
}
