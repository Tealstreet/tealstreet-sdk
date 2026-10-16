//! Bitget's scope: the shared js_runtime host handle plus venue-only fields.
use safe_cex_exchange::js_runtime::scope::{Scope, ScopeShared};

pub(crate) struct BitgetExtra {
    pub variant: Variant,
    pub name: safe_cex_types::ExchangeName,
    pub options: safe_cex_types::ExchangeOptions,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

pub(crate) type Shared = ScopeShared<BitgetExtra>;
pub(crate) type Rt = Scope<BitgetExtra>;

pub(crate) use safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Variant {
    Classic,
    Inverse,
    Usdc,
}
