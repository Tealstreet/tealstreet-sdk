//! Kraken's scope: the shared js_runtime host handle plus venue-only fields.
use crate::{api::FuturesModule, construction::Venue};
use safe_cex_exchange::js_runtime::scope::{Scope, ScopeShared};
use std::{cell::RefCell, rc::Rc};

pub(crate) struct KrakenExtra {
    pub venue: Venue,
    pub name: safe_cex_types::ExchangeName,
    pub options: safe_cex_types::ExchangeOptions,
    pub module: Rc<FuturesModule>,
    pub spot_ids: Rc<RefCell<SpotIds>>,
    pub sockets: RefCell<Vec<Rc<super::sockets::SocketHost>>>,
    pub startup: crate::startup::StartupState<safe_cex_exchange::js_runtime::value::Js>,
    pub external_public_data: Option<ExternalPublicDataFactory>,
    pub journal: Option<Rc<dyn safe_cex_exchange::journal::JournalFolder>>,
    pub prototypes: RefCell<
        Vec<(
            crate::classes::Kind,
            safe_cex_exchange::js_runtime::value::Js,
        )>,
    >,
}

pub use safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory;

pub(crate) type Shared = ScopeShared<KrakenExtra>;
pub(crate) type Rt = Scope<KrakenExtra>;

/// The source module-scoped `createKrakenSpotClientOrderId` generator.
pub type SpotIds = Box<dyn FnMut() -> Result<String, safe_cex_num::JsCoercionError>>;
