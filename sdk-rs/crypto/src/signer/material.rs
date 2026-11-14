//! Native protected storage preserves the exact WASM map representation.
use zeroize::Zeroizing;
#[cfg(target_arch = "wasm32")]
pub(super) type Material = Zeroizing<Vec<u8>>;
#[cfg(target_arch = "wasm32")]
pub(super) fn heap(bytes: Zeroizing<Vec<u8>>) -> Material {
    bytes
}
#[cfg(target_arch = "wasm32")]
pub(super) use heap as presentable_heap;
#[cfg(not(target_arch = "wasm32"))]
pub(super) enum Material {
    Heap(Zeroizing<Vec<u8>>),
    // Permission belongs to the bytes installed by trusted credential/passphrase
    // setup, never the spelling of a ref. Replacing material drops permission.
    PresentableHeap(Zeroizing<Vec<u8>>),
    Protected {
        bytes: crate::protected::ProtectedSecret,
        presentable: bool,
    },
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn heap(bytes: Zeroizing<Vec<u8>>) -> Material {
    Material::Heap(bytes)
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn presentable_heap(bytes: Zeroizing<Vec<u8>>) -> Material {
    Material::PresentableHeap(bytes)
}
#[cfg(not(target_arch = "wasm32"))]
impl Material {
    pub(super) fn as_slice(&self) -> &[u8] {
        match self {
            Self::Heap(v) | Self::PresentableHeap(v) => v.as_slice(),
            Self::Protected { bytes, .. } => bytes.as_bytes(),
        }
    }
    pub(super) fn protect(self) -> Result<Self, safe_cex_engine::signing::SignError> {
        let presentable = matches!(self, Self::PresentableHeap(_));
        Ok(Self::Protected {
            bytes: crate::protected::ProtectedSecret::copy(self.as_slice())?,
            presentable,
        })
    }
    pub(super) fn refuses_presentation(&self) -> bool {
        matches!(
            self,
            Self::Protected {
                presentable: false,
                ..
            }
        )
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl std::ops::Deref for Material {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}
