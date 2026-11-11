use super::KeyRef;
#[cfg(not(target_arch = "wasm32"))]
use super::{Incarnation, LocalSigner, SignError};
use std::collections::HashMap;
use zeroize::Zeroizing;

#[cfg(target_arch = "wasm32")]
macro_rules! heap {
    ($bytes:expr) => {
        $bytes
    };
}
#[cfg(not(target_arch = "wasm32"))]
macro_rules! heap {
    ($bytes:expr) => {
        Material::Heap($bytes)
    };
}
pub(super) use heap;

/// Optional native host storage for installed local signing material.
/// The provider erases it before releasing the host buffer.
#[cfg(not(target_arch = "wasm32"))]
pub trait ProtectedBytes: zeroize::Zeroize + Send + Sync {
    fn as_bytes(&self) -> &[u8];
}

// WASM keeps the original map value and drop glue; protected storage is native-only.
#[cfg(target_arch = "wasm32")]
pub(super) type KeyMap = HashMap<KeyRef, Zeroizing<Vec<u8>>>;

#[cfg(not(target_arch = "wasm32"))]
pub(super) enum Material {
    Heap(Zeroizing<Vec<u8>>),
    Protected(Box<dyn ProtectedBytes>),
}
#[cfg(not(target_arch = "wasm32"))]
impl Material {
    pub(super) fn as_slice(&self) -> &[u8] {
        match self {
            Self::Heap(bytes) => bytes.as_slice(),
            Self::Protected(bytes) => bytes.as_bytes(),
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl Drop for Material {
    fn drop(&mut self) {
        if let Self::Protected(bytes) = self {
            bytes.zeroize();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub(super) struct KeyMap {
    bytes: HashMap<KeyRef, Material>,
    protected_storage: bool,
}
#[cfg(not(target_arch = "wasm32"))]
impl KeyMap {
    pub(super) fn new() -> Self {
        Self::default()
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl std::ops::Deref for KeyMap {
    type Target = HashMap<KeyRef, Material>;
    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl std::ops::DerefMut for KeyMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bytes
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl LocalSigner {
    /// Ends every seal made under the previous material.
    pub(super) fn redraw(&mut self) {
        if self.keys.protected_storage {
            self.incarnation = None;
            return;
        }
        self.incarnation = Incarnation::fresh().ok();
    }
    /// Move installed bytes to a host-owned protected allocation. Ref bindings,
    /// policy and signing operations stay with this local provider. Setup seals
    /// are retired for host-owned storage. Failure drops
    /// and erases every completed allocation and every remaining input buffer.
    pub fn with_protected_storage(
        mut self,
        mut allocate: impl FnMut(&[u8]) -> Result<Box<dyn ProtectedBytes>, SignError>,
    ) -> Result<Self, SignError> {
        self.keys.protected_storage = true;
        self.incarnation = None;
        let mut protected = HashMap::with_capacity(self.keys.len());
        for (reference, bytes) in std::mem::take(&mut self.keys.bytes) {
            protected.insert(reference, Material::Protected(allocate(bytes.as_slice())?));
        }
        self.keys.bytes = protected;
        Ok(self)
    }
}
