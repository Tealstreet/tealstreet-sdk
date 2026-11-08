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

    /// Bump the generation only when the authenticated owner changes. A same-user
    /// reconnect, resume or signing handover keeps it, so the approved 60-second
    /// COSIGNED resume window keeps the account's TWAPs. Returns whether it changed.
    pub fn rebind_owner(&self, authenticated_user: &str) -> bool {
        let mut current = self.current.lock().unwrap();
        if current.owner_id == authenticated_user {
            return false;
        }
        *current = TwapOwner {
            owner_id: authenticated_user.into(),
            generation: next_generation(),
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_user_rebind_keeps_the_generation_and_an_owner_change_never_reuses_one() {
        let binding = Binding::new("tenant:A");
        let a = binding.snapshot();
        assert!(!binding.rebind_owner("tenant:A"));
        assert_eq!(binding.snapshot(), a);
        assert!(binding.rebind_owner("tenant:B"));
        let b = binding.snapshot();
        assert_eq!(b.owner_id, "tenant:B");
        assert!(b.generation > a.generation);
        assert!(binding.rebind_owner("tenant:A"));
        assert!(binding.snapshot().generation > b.generation);
        assert_ne!(Binding::new("tenant:A").snapshot().generation, a.generation);
    }
}
