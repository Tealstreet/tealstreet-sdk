//! Server-owned authenticated connection bindings, never restore-payload authority.
use safe_cex_algo_twap::TwapOwner;
use std::sync::Mutex;

const MAX_OWNER_GENERATION: u64 = (1 << 53) - 1;

/// The initial generation, stable across account rebuilds, restarts and boxes so
/// the approved 60-second COSIGNED resume window and cross-box transfer restore
/// the same user's rows; a different owner gets a different generation.
fn generation_for(authenticated_user: &str) -> u64 {
    let digest =
        safe_cex_crypto::sha256(format!("twap-owner-generation:{authenticated_user}").as_bytes());
    let bits = u64::from_le_bytes(digest[..8].try_into().expect("8-byte digest prefix"));
    (bits & ((1 << 52) - 1)) + 1
}

pub struct Binding {
    current: Mutex<TwapOwner>,
}

impl Binding {
    pub fn new(authenticated_user: &str) -> Self {
        Self {
            current: Mutex::new(TwapOwner {
                owner_id: authenticated_user.into(),
                generation: generation_for(authenticated_user),
            }),
        }
    }

    pub fn snapshot(&self) -> TwapOwner {
        self.current.lock().unwrap().clone()
    }

    /// Bump the generation only when the authenticated owner changes. A same-user
    /// reconnect, resume or signing handover keeps it, so the approved 60-second
    /// COSIGNED resume window keeps the account's TWAPs. Returns whether it changed.
    /// Returning to an earlier owner never reinstates its retired generation.
    pub fn rebind_owner(&self, authenticated_user: &str) -> bool {
        let mut current = self.current.lock().unwrap();
        if current.owner_id == authenticated_user {
            return false;
        }
        let generation = current
            .generation
            .checked_add(1)
            .filter(|generation| *generation <= MAX_OWNER_GENERATION)
            .expect("TWAP owner generation exhausted");
        *current = TwapOwner {
            owner_id: authenticated_user.into(),
            generation,
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generation_follows_the_owner_across_rebinds_and_rebuilds() {
        let binding = Binding::new("tenant:A");
        let a = binding.snapshot();
        assert!(!binding.rebind_owner("tenant:A"));
        assert_eq!(binding.snapshot(), a);
        // A rebuilt account, a restart or another box derives the same generation.
        assert_eq!(Binding::new("tenant:A").snapshot(), a);
        assert!(binding.rebind_owner("tenant:B"));
        let b = binding.snapshot();
        assert_eq!(b.owner_id, "tenant:B");
        assert_ne!(b.generation, a.generation);
        assert!(binding.rebind_owner("tenant:A"));
        assert_eq!(binding.snapshot().owner_id, a.owner_id);
        assert_ne!(binding.snapshot().generation, a.generation);
        let returned = binding.snapshot();
        assert!(!binding.rebind_owner("tenant:A"));
        assert_eq!(binding.snapshot(), returned);
    }
}
