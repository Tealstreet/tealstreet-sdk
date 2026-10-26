//! Isolated allocator observer: inspect owned credential bytes while they are still
//! allocated, immediately before System frees them. No production unsafe code.
use safe_cex_engine::ReadCredentials;
use safe_cex_venue_kraken::{KrakenSpot, Options};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Watched {
    pointer: AtomicUsize,
    length: AtomicUsize,
    result: AtomicUsize,
}
impl Watched {
    const fn new() -> Self {
        Self {
            pointer: AtomicUsize::new(0),
            length: AtomicUsize::new(0),
            result: AtomicUsize::new(0),
        }
    }
    fn arm(&self, value: &str) {
        assert!(!value.is_empty());
        assert_eq!(self.pointer.load(Ordering::Acquire), 0);
        self.result.store(0, Ordering::Relaxed);
        self.length.store(value.len(), Ordering::Relaxed);
        self.pointer
            .store(value.as_ptr() as usize, Ordering::Release);
    }
}
static WATCHED: [Watched; 5] = [const { Watched::new() }; 5];

struct ObservingAllocator;
// SAFETY: all allocation operations delegate to System unchanged. Only an exact
// registered live String allocation is inspected, before forwarding deallocation.
unsafe impl GlobalAlloc for ObservingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded with the caller's original layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        for watched in &WATCHED {
            // Disarm before freeing, so reuse of this address cannot be mistaken
            // for the original String. This path never allocates or panics.
            if watched
                .pointer
                .compare_exchange(pointer as usize, 0, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let length = watched.length.load(Ordering::Relaxed);
                let cleared = length <= layout.size()
                    && (0..length).all(|index| {
                        // SAFETY: these initialized bytes belonged to the watched
                        // String and remain allocated until System.dealloc below.
                        unsafe { pointer.add(index).read_volatile() == 0 }
                    });
                watched
                    .result
                    .store(if cleared { 1 } else { 2 }, Ordering::Release);
            }
        }
        // SAFETY: forwarded exactly once with the original allocation and layout.
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: ObservingAllocator = ObservingAllocator;

#[test]
fn original_spot_credentials_are_zeroed_before_engine_drop_frees_them() {
    for (scenario, key, public_data_only) in [
        ("private", Some("probe-key"), false),
        ("incomplete", None, false),
        ("public-only", Some("probe-key"), true),
    ] {
        let options = Options {
            key: key.map(str::to_owned),
            // Valid base64 of a short, public synthetic credential.
            secret: Some("cHJvYmU=".into()),
            read_credentials: Some(ReadCredentials {
                key: "read-key".into(),
                secret: "read-secret".into(),
                passphrase: Some("read-pass".into()),
            }),
            public_data_only,
            ..Options::default()
        };
        let read = options.read_credentials.as_ref().unwrap();
        let credentials = [
            options.key.as_deref(),
            options.secret.as_deref(),
            Some(read.key.as_str()),
            Some(read.secret.as_str()),
            read.passphrase.as_deref(),
        ];
        for (watched, value) in WATCHED.iter().zip(credentials) {
            if let Some(value) = value {
                watched.arm(value);
            }
        }
        // Move the actual original String allocations through the public venue
        // constructor and engine owner. Do not call an erasure helper explicitly.
        let engine = safe_cex_engine::test_support::seeded_engine(KrakenSpot::new(options));
        for (index, watched) in WATCHED.iter().enumerate() {
            if index != 0 || key.is_some() {
                assert_ne!(watched.pointer.load(Ordering::Acquire), 0, "{scenario}");
                assert_eq!(watched.result.load(Ordering::Acquire), 0, "{scenario}");
            }
        }
        drop(engine);
        for (index, watched) in WATCHED.iter().enumerate() {
            if index != 0 || key.is_some() {
                assert_eq!(watched.pointer.load(Ordering::Acquire), 0, "{scenario}");
                assert_eq!(
                    watched.result.load(Ordering::Acquire),
                    1,
                    "{scenario}: original credential buffer {index} was not cleared before free"
                );
            }
        }
    }
}
