//! Production assembly of the existing Aster bodies on shared JS values.
mod globals;
mod host;
mod members;
mod sockets;

use crate::{
    account_fetch::AccountFetch, order_execution::OrderExecution, types::AsterDEXAPIVersion,
};
use safe_cex_exchange::{
    js_runtime::{
        base_exchange::BaseExchangeParts,
        scope::{Scope, ScopeShared},
        value::{Js, Thrown},
    },
    runtime::ExchangeError,
};
use std::{
    cell::{OnceCell, RefCell},
    rc::{Rc, Weak},
};

pub(crate) type Shared = ScopeShared<State>;
pub(crate) type Rt = Scope<State>;

pub(crate) struct State {
    pub parts: RefCell<Option<BaseExchangeParts>>,
    pub deps: crate::native_exchange::AsterDEXDependencies,
    pub version: AsterDEXAPIVersion,
    pub owner: OnceCell<Js>,
    pub account: OnceCell<Rc<AccountFetch>>,
    pub orders: OnceCell<Rc<OrderExecution>>,
    pub clock: OnceCell<Rc<crate::virtual_clock::AsterDEXVirtualClock>>,
    pub public: OnceCell<Rc<crate::public_stream::PublicStream>>,
    pub private: OnceCell<Rc<crate::private_stream::PrivateStream>>,
    pub listeners: Rc<crate::public_stream::PublicDataListeners>,
}
impl Drop for State {
    fn drop(&mut self) {
        // Arrow fields and API interceptor receivers retain their own JS object.
        // Break that graph after the last pending runtime observer releases it.
        if let Some(Js::Object(owner)) = self.owner.get() {
            owner.props.borrow_mut().clear();
        }
    }
}

pub(crate) fn scope(weak: &Weak<Shared>, this: Js) -> Result<Rt, Thrown> {
    Ok(Rt {
        shared: weak
            .upgrade()
            .ok_or_else(|| Thrown::error("Error", "AsterDEX exchange was dropped"))?,
        this,
    })
}
pub(crate) fn error(error: ExchangeError) -> Thrown {
    let response = error.retained_response_input();
    let thrown = Thrown::from(error);
    if let Some(response) = response {
        let _ = thrown.value.define(
            "response",
            safe_cex_exchange::js_runtime::value::from_retained_input(response),
        );
    }
    thrown
}
pub(crate) fn text(value: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&value.to_js_string()?))
}

pub(crate) fn assemble(
    parts: BaseExchangeParts,
    deps: crate::native_exchange::AsterDEXDependencies,
) -> Result<Rt, ExchangeError> {
    let version = crate::helpers::resolve_aster_dex_api_version(&parts.options);
    let name = safe_cex_exchange::js_runtime::value::from_typed(&parts.name)?;
    let options = safe_cex_exchange::js_runtime::value::from_typed(&parts.options)?;
    let store = safe_cex_exchange::js_runtime::store::facade(parts.store.clone());
    // Module-level axios (the clock) has no account signing or private route.
    // createAPI separately asks the factory for each lane's real options.
    let mut raw_options = parts.options.clone();
    raw_options.key.clear();
    raw_options.secret.clear();
    raw_options.remote_signer = None;
    raw_options.secondary_key = None;
    raw_options.secondary_secret = None;
    raw_options.secondary_proxy_url = None;
    raw_options.tealstreet_proxy = None;
    raw_options.tealstreet_tunnel = None;
    raw_options.override_proxy_url = None;
    let default_http = (deps.http)(&raw_options)?;
    let shared = Rc::new_cyclic(|weak| {
        ScopeShared::new(
            deps.runtime.clone(),
            default_http,
            parts.store.clone(),
            globals::build(weak.clone()),
            State {
                parts: RefCell::new(Some(parts)),
                deps,
                version,
                owner: OnceCell::new(),
                account: OnceCell::new(),
                orders: OnceCell::new(),
                clock: OnceCell::new(),
                public: OnceCell::new(),
                private: OnceCell::new(),
                listeners: Rc::default(),
            },
        )
    });
    let rt = Rt {
        shared,
        this: Js::Undefined,
    };
    let class = crate::exchange_runtime::AsterDEXExchange::new(
        Rc::new(host::Host::new(&rt)),
        name,
        options,
        store,
    )
    .map_err(|value| Thrown::new(value).to_exchange_error())?;
    let this = class.owner;
    let _ = rt.shared.extra.owner.set(this.clone());
    *rt.shared.exchange.borrow_mut() = this.clone();
    let rt = rt.with_this(this);
    let adapter = |key| -> Result<Rc<dyn safe_cex_transport::RestAdapter>, ExchangeError> {
        Ok(Rc::new(
            safe_cex_exchange::js_runtime::client::JsRestAdapter::new(
                rt.exchange_runtime(),
                rt.this.get(key)?,
            ),
        ))
    };
    let _ = rt.shared.extra.account.set(Rc::new(AccountFetch::new(
        rt.base().clone(),
        adapter("primaryAPI")?,
        adapter("publicAPI")?,
        rt.shared.extra.version.clone(),
    )));
    let _ = rt.shared.extra.orders.set(OrderExecution::new(
        rt.base().clone(),
        adapter("primaryAPI")?,
        adapter("secondaryAPI")?,
        adapter("publicAPI")?,
        rt.shared.extra.version.clone(),
    ));
    Ok(rt)
}

pub(crate) fn initialize_sockets(rt: &Rt, venue: &Rc<dyn safe_cex_exchange::exchange::Exchange>) {
    sockets::initialize(rt, venue);
}
