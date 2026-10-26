//! Credential ownership helpers. Redaction is unconditional, including nested options.
use std::ops::{Deref, DerefMut};
use zeroize::Zeroize;

pub fn erase<T: Zeroize>(value: &mut T) {
    value.zeroize();
}

pub trait EraseSecrets {
    fn erase_secrets(&mut self);
}

/// Owns an internal credential carrier and scrubs it on normal and error-path drop.
/// Public option structs retain ordinary fields for source-compatible struct literals.
pub struct Sensitive<T: EraseSecrets>(T);
impl<T: EraseSecrets> Sensitive<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }
}
impl<T: EraseSecrets + Clone> Clone for Sensitive<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: EraseSecrets> Deref for Sensitive<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: EraseSecrets> DerefMut for Sensitive<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}
impl<T: EraseSecrets> Drop for Sensitive<T> {
    fn drop(&mut self) {
        self.0.erase_secrets();
    }
}
impl<T: EraseSecrets> std::fmt::Debug for Sensitive<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[macro_export]
macro_rules! redacted_debug {
    ($ty:ty) => {
        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(concat!(stringify!($ty), " { [REDACTED] }"))
            }
        }
    };
}

#[macro_export]
macro_rules! erase_credentials_on_drop {
    ($ty:ty, $($field:ident),+ $(,)?) => {
        impl $crate::secrets::EraseSecrets for $ty {
            fn erase_secrets(&mut self) { $( $crate::secrets::erase(&mut self.$field); )+ }
        }
        impl Drop for $ty {
            fn drop(&mut self) { $crate::secrets::EraseSecrets::erase_secrets(self); }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    struct Probe {
        secret: String,
        erased: Arc<AtomicBool>,
    }
    impl EraseSecrets for Probe {
        fn erase_secrets(&mut self) {
            erase(&mut self.secret);
            self.erased.store(self.secret.is_empty(), Ordering::SeqCst);
        }
    }
    #[test]
    fn owned_secrets_are_scrubbed_on_normal_and_error_paths() {
        let erased = Arc::new(AtomicBool::new(false));
        let fail = || -> Result<(), ()> {
            let _secret = Sensitive::new(Probe {
                secret: "private credential".into(),
                erased: erased.clone(),
            });
            Err(())?;
            Ok(())
        };
        assert!(fail().is_err());
        assert!(erased.load(Ordering::SeqCst));
        erased.store(false, Ordering::SeqCst);
        drop(Sensitive::new(Probe {
            secret: "another credential".into(),
            erased: erased.clone(),
        }));
        assert!(erased.load(Ordering::SeqCst));
    }
}
