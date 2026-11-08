//! Server-owned authenticated connection bindings, never restore-payload authority.
use safe_cex_algo_twap::TwapOwner;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

fn next_generation() -> u64 {
    static NEXT: OnceLock<AtomicU64> = OnceLock::new();
    NEXT.get_or_init(|| {
        let mut seed = [0; 8];
        getrandom::getrandom(&mut seed).expect("OS entropy for TWAP binding generation");
        AtomicU64::new((u64::from_le_bytes(seed) & ((1 << 52) - 1)) + 1)
    })
    .try_update(Ordering::AcqRel, Ordering::Acquire, |generation| {
        generation
            .checked_add(1)
            .filter(|next| *next <= 9_007_199_254_740_991)
    })
    .expect("TWAP binding generations exhausted")
}

pub struct Binding {
    current: Mutex<TwapOwner>,
}

impl Binding {
    pub fn new(authenticated_user: &str) -> Self {
        Self {
            current: Mutex::new(TwapOwner {
                owner_id: authenticated_user.into(),
                generation: next_generation(),
            }),
        }
    }

    pub fn snapshot(&self) -> TwapOwner {
        self.current.lock().unwrap().clone()
    }

    /// Every authenticated attachment is a new generation, even on the same socket.
    pub fn rebind(&self) {
        self.current.lock().unwrap().generation = next_generation();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_attach_and_return_to_a_never_reuse_a_generation() {
        let binding = Binding::new("tenant:A");
        let a = binding.snapshot();
        binding.rebind();
        let b = binding.snapshot();
        assert_eq!(b.owner_id, a.owner_id);
        assert!(b.generation > a.generation);
        binding.rebind();
        assert!(binding.snapshot().generation > b.generation);
        assert_ne!(Binding::new("tenant:A").snapshot().generation, a.generation);
    }
}
