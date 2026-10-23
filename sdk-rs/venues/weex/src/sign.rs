//! WEEX signs the exact bytes sent; callers construct sorted, encoded queries.
use safe_cex_engine::signing::Identity;
pub use safe_cex_engine::url::encode;
use safe_cex_engine::{CredentialSlot, Method};

/// Opaque refs only; trusted setup and the injected provider own the material.
#[derive(Clone, Debug)]
pub struct Credentials {
    pub primary: Identity,
    pub read: Option<Identity>,
}
impl Credentials {
    /// GET reads use the secondary identity when provisioned; POST stays primary.
    pub fn slot(&self, method: Method) -> CredentialSlot {
        if method == Method::Get && self.read.is_some() {
            CredentialSlot::Secondary
        } else {
            CredentialSlot::Primary
        }
    }
    pub fn identity(&self, method: Method) -> &Identity {
        match (self.slot(method), &self.read) {
            (CredentialSlot::Secondary, Some(read)) => read,
            _ => &self.primary,
        }
    }
}
