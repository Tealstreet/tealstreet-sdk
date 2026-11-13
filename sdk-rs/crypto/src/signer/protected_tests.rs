use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use zeroize::Zeroize;

struct Buffer {
    bytes: Vec<u8>,
    releases: Arc<AtomicUsize>,
}
impl Zeroize for Buffer {
    fn zeroize(&mut self) {
        self.bytes.as_mut_slice().zeroize();
    }
}
impl ProtectedBytes for Buffer {
    fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl Drop for Buffer {
    fn drop(&mut self) {
        assert!(self.bytes.iter().all(|byte| *byte == 0));
        self.releases.fetch_add(1, Ordering::SeqCst);
    }
}
fn setup() -> LocalSetup {
    LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("synthetic-key".into()),
            secret: Secret::new("synthetic-secret".into()),
            passphrase: Some(Secret::new("synthetic-passphrase".into())),
            ..Default::default()
        }),
        secondary: None,
    }
}

#[test]
fn protected_storage_preserves_local_recipes_and_erases_on_rotation_remove_and_drop() {
    let (plain, identities) = LocalSigner::from_setup(setup()).unwrap();
    let (local, _) = LocalSigner::from_setup(setup()).unwrap();
    let releases = Arc::new(AtomicUsize::new(0));
    let mut protected = local
        .with_protected_storage(|bytes| {
            Ok(Box::new(Buffer {
                bytes: bytes.to_vec(),
                releases: releases.clone(),
            }))
        })
        .unwrap();
    assert!(protected.incarnation().is_none());
    let primary = identities.primary.unwrap();
    let request = safe_cex_engine::signing::hmac_sha256_work(
        &primary.credential,
        primary.signing_ref().unwrap(),
        vec![PayloadPart::Bytes {
            bytes: b"synthetic-message".to_vec(),
        }],
        Default::default(),
        Default::default(),
    );
    assert_eq!(plain.sign_now(&request), protected.sign_now(&request));
    protected.remove(&KeyRef("primary.passphrase".into()));
    assert_eq!(releases.load(Ordering::SeqCst), 1);
    protected.insert(
        primary.signing_ref().unwrap().clone(),
        b"synthetic-replacement".to_vec(),
    );
    assert!(protected.incarnation().is_none());
    assert_eq!(releases.load(Ordering::SeqCst), 2);
    drop(protected);
    assert_eq!(releases.load(Ordering::SeqCst), 3);
}

#[test]
fn failed_protected_allocation_erases_the_completed_prefix() {
    let (local, _) = LocalSigner::from_setup(setup()).unwrap();
    let releases = Arc::new(AtomicUsize::new(0));
    let mut calls = 0;
    let result = local.with_protected_storage(|bytes| {
        calls += 1;
        if calls == 2 {
            return Err(SignError::Unavailable);
        }
        Ok(Box::new(Buffer {
            bytes: bytes.to_vec(),
            releases: releases.clone(),
        }))
    });
    assert!(matches!(result, Err(SignError::Unavailable)));
    assert_eq!(releases.load(Ordering::SeqCst), 1);
}

#[test]
fn panicking_protected_allocation_erases_the_completed_prefix() {
    let (local, _) = LocalSigner::from_setup(setup()).unwrap();
    let releases = Arc::new(AtomicUsize::new(0));
    let mut calls = 0;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        local.with_protected_storage(|bytes| {
            calls += 1;
            assert_ne!(calls, 2, "synthetic protected allocation fault");
            Ok(Box::new(Buffer {
                bytes: bytes.to_vec(),
                releases: releases.clone(),
            }))
        })
    }));
    assert!(result.is_err());
    assert_eq!(releases.load(Ordering::SeqCst), 1);
}
