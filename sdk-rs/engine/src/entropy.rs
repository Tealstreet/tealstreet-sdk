//! Host-owned entropy. Identity/jitter seeding never replaces signing entropy.
use std::cell::RefCell;
use zeroize::Zeroize;

#[cfg(not(target_arch = "wasm32"))]
pub type Fill = Box<dyn FnMut(&mut [u8]) -> Result<(), String> + Send>;
#[cfg(target_arch = "wasm32")]
pub type Fill = Box<dyn FnMut(&mut [u8]) -> Result<(), String>>;

/// Both providers are explicit; only hosts choose cryptographic backends.
pub struct Entropy {
    identity: RefCell<Fill>,
    signing: RefCell<Fill>,
}
impl Entropy {
    pub fn new(identity: Fill, signing: Fill) -> Self {
        Self {
            identity: RefCell::new(identity),
            signing: RefCell::new(signing),
        }
    }
    pub(crate) fn identity(&self, bytes: &mut [u8]) -> Result<(), String> {
        Self::fill(&self.identity, bytes)
    }
    pub(crate) fn signing(&self, bytes: &mut [u8]) -> Result<(), String> {
        Self::fill(&self.signing, bytes)
    }
    fn fill(source: &RefCell<Fill>, bytes: &mut [u8]) -> Result<(), String> {
        let result = source
            .try_borrow_mut()
            .map_err(|_| "Entropy provider reentered".to_owned())
            .and_then(|mut fill| fill(bytes));
        if result.is_err() {
            bytes.zeroize();
        }
        result
    }
    pub(crate) fn set_identity(&mut self, fill: Fill) {
        *self.identity.get_mut() = fill;
    }
    pub(crate) fn set_signing(&mut self, fill: Fill) {
        *self.signing.get_mut() = fill;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_replacement_does_not_consume_or_replace_signing() {
        let mut entropy = Entropy::new(
            Box::new(|bytes| {
                bytes.fill(1);
                Ok(())
            }),
            Box::new(|bytes| {
                bytes.fill(2);
                Ok(())
            }),
        );
        entropy.set_identity(Box::new(|bytes| {
            bytes.fill(3);
            Ok(())
        }));
        let mut bytes = [0; 8];
        entropy.identity(&mut bytes).unwrap();
        assert_eq!(bytes, [3; 8]);
        entropy.signing(&mut bytes).unwrap();
        assert_eq!(bytes, [2; 8]);
        entropy.set_signing(Box::new(|bytes| {
            bytes.fill(4);
            Ok(())
        }));
        entropy.signing(&mut bytes).unwrap();
        assert_eq!(bytes, [4; 8]);
        entropy.identity(&mut bytes).unwrap();
        assert_eq!(bytes, [3; 8]);
    }

    #[test]
    fn explicit_native_fixture_stream_is_chunk_invariant_and_engine_local() {
        let first = crate::test_support::seeded_entropy(19);
        let second = crate::test_support::seeded_entropy(19);
        let mut whole = [0; 64];
        let mut split = [0; 64];
        first.identity(&mut whole).unwrap();
        second.identity(&mut split[..21]).unwrap();
        second.identity(&mut []).unwrap();
        second.identity(&mut split[21..]).unwrap();
        assert_eq!(whole, split);
        first.identity(&mut whole).unwrap();
        assert_ne!(whole, split);
    }

    #[test]
    fn failed_or_reentered_provider_clears_partial_output() {
        let source: RefCell<Fill> = RefCell::new(Box::new(|bytes| {
            bytes.fill(99);
            Err("unavailable".into())
        }));
        let mut bytes = [1; 32];
        assert!(Entropy::fill(&source, &mut bytes).is_err());
        assert_eq!(bytes, [0; 32]);
        let _borrow = source.borrow_mut();
        bytes.fill(1);
        assert!(Entropy::fill(&source, &mut bytes).is_err());
        assert_eq!(bytes, [0; 32]);
    }
}
