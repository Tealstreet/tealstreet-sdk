//! A host token cannot identify another lifecycle after its engine is dropped.
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(1);

pub(super) fn next() -> u64 {
    allocate(&NEXT)
}

fn allocate(counter: &AtomicU64) -> u64 {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
            next.checked_add(1)
        })
        .expect("HTTP token issuer namespace exhausted")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhaustion_never_wraps_or_reissues_an_issuer() {
        let counter = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate(&counter), u64::MAX - 1);
        for _ in 0..2 {
            assert!(std::panic::catch_unwind(|| allocate(&counter)).is_err());
            assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
        }
    }
}
