//! WEEX signs the exact bytes sent; callers construct sorted, encoded queries.
use safe_cex_engine::signing::Identity;
pub use safe_cex_engine::url::encode;
use safe_cex_engine::{CredentialSlot, Method};

/// Opaque refs only; trusted setup and the injected provider own the material.
#[derive(Clone, Debug, Default)]
pub struct Credentials {
    /// A complete private primary identity; absent for public accounts.
    pub primary: Option<Identity>,
    pub read: Option<Identity>,
}
impl Credentials {
    /// GET reads use the secondary identity when provisioned, even without a
    /// usable primary; every other method requires the primary.
    pub fn select(&self, method: Method) -> Option<(&Identity, CredentialSlot)> {
        match (&self.read, &self.primary) {
            (Some(read), _) if method == Method::Get => Some((read, CredentialSlot::Secondary)),
            (_, Some(primary)) => Some((primary, CredentialSlot::Primary)),
            _ => None,
        }
    }
}
