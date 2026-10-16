//! Bitget's scope: the shared js_runtime host handle plus venue-only fields.
use safe_cex_exchange::js_runtime::scope::{Scope, ScopeShared};
use std::rc::Rc;

pub(crate) struct BitgetExtra {
    pub variant: Variant,
    pub name: safe_cex_types::ExchangeName,
    pub options: safe_cex_types::ExchangeOptions,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

pub(crate) type Shared = ScopeShared<BitgetExtra>;
pub(crate) type Rt = Scope<BitgetExtra>;

pub type ExternalPublicDataFactory = Rc<
    dyn Fn(
        &safe_cex_types::ExchangeOptionsExternalPublicData,
    ) -> Rc<dyn safe_cex_exchange::external::ExternalPublicData>,
>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Variant {
    Classic,
    Inverse,
    Usdc,
}
