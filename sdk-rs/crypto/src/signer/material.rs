//! Native protected storage preserves the exact WASM map representation.
use zeroize::Zeroizing;
#[cfg(target_arch = "wasm32")]
pub(super) type Material = Zeroizing<Vec<u8>>;
#[cfg(target_arch = "wasm32")]
pub(super) fn heap(bytes: Zeroizing<Vec<u8>>) -> Material {
    bytes
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) enum Material {
    Heap(Zeroizing<Vec<u8>>),
    Protected(crate::protected::ProtectedSecret),
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn heap(bytes: Zeroizing<Vec<u8>>) -> Material {
    Material::Heap(bytes)
}
#[cfg(not(target_arch = "wasm32"))]
impl Material {
    pub(super) fn as_slice(&self) -> &[u8] {
        match self {
            Self::Heap(v) => v.as_slice(),
            Self::Protected(v) => v.as_bytes(),
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl std::ops::Deref for Material {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}
