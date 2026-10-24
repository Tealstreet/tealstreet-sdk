//! WEEX signs the exact bytes sent; callers construct sorted, encoded queries.
use safe_cex_engine::signing::Identity;
pub use safe_cex_engine::url::encode;

/// Opaque refs only; trusted setup and the injected provider own the material.
#[derive(Clone, Debug, Default)]
pub struct Credentials {
    /// A complete private primary identity; absent for public accounts.
    pub primary: Option<Identity>,
    pub read: Option<Identity>,
}
