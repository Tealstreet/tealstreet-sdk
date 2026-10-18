//! Binance COIN-M: source-corresponding native venue assembly.
mod api;
mod exchange;
mod globals;
mod lifecycle;
mod ops;
mod orders;
mod reads;
#[path = "../generated/binanceinverse.rs"]
pub mod types;
mod ws_private;
mod ws_public;

use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    js_runtime::{
        base_exchange::{self, BaseExchangeParts, ExternalPublicDataFactory},
        client::JsHttpHost,
        globals::{install_builtins, install_enums},
        library::install_library,
        scope::{Scope, ScopeShared},
        store,
        value::{Js, from_typed},
    },
    runtime::ExchangeError,
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::rc::Rc;

pub struct BinanceInverseDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Rc<dyn JsHttpHost>,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

pub(crate) struct ExchangeState {
    pub name: ExchangeName,
    pub options: ExchangeOptions,
    pub external_public_data: Option<ExternalPublicDataFactory>,
}
pub(crate) type Shared = ScopeShared<ExchangeState>;
pub(crate) type Rt = Scope<ExchangeState>;

pub struct BinanceInverseExchange {
    scope: Rt,
    capabilities: safe_cex_exchange::capabilities::ExchangeCapabilities,
}

impl BinanceInverseExchange {
    pub fn new(
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
        deps: BinanceInverseDependencies,
    ) -> Result<Rc<Self>, ExchangeError> {
        let shared: Rc<Shared> = Rc::new(ScopeShared::new(
            deps.runtime,
            deps.http,
            store.clone(),
            Js::object(vec![]),
            ExchangeState {
                name,
                options,
                external_public_data: deps.external_public_data,
            },
        ));
        install_builtins(&shared.globals, Rc::downgrade(&shared));
        install_enums(&shared.globals);
        install_library(&shared.globals, Rc::downgrade(&shared));
        globals::install(&shared.globals).map_err(|e| e.to_exchange_error())?;
        let (receiver, base) = base_exchange::create(
            &shared.sockets,
            BaseExchangeParts {
                name: shared.extra.name.clone(),
                options: shared.extra.options.clone(),
                store: store.clone(),
                settings: ExchangeSettings::default(),
                external_public_data: shared.extra.external_public_data.clone(),
            },
            from_typed(&shared.extra.name).map_err(|e| e.to_exchange_error())?,
            from_typed(&shared.extra.options).map_err(|e| e.to_exchange_error())?,
            store::facade(store),
            |receiver| {
                for name in [
                    "positionStateVersion",
                    "lastAccountReconciliationAt",
                    "lastPositionRiskReconciliationAt",
                    "backgroundReconciliationPausedUntil",
                    "lastOrdersRetryAt",
                ] {
                    receiver.define(name, Js::Number(0.0))?;
                }
                receiver.define("ordersSnapshotStale", Js::Bool(false))?;
                receiver.define("hasLoggedBalanceAggregation", Js::Bool(false))?;
                for name in ["grafanaEndpointsEvery", "grafanaEndpointsPeriodic"] {
                    let key = if name == "grafanaEndpointsEvery" {
                        "GRAFANA_ENDPOINTS_EVERY"
                    } else {
                        "GRAFANA_ENDPOINTS_PERIODIC"
                    };
                    receiver.define(name, shared.globals.get(key)?)?;
                }
                Ok(())
            },
        )
        .map_err(|e| e.to_exchange_error())?;
        shared
            .base
            .set(base)
            .map_err(|_| ExchangeError::new("duplicate BaseExchange"))?;
        *shared.exchange.borrow_mut() = receiver.clone();
        let rt = Scope {
            shared: shared.clone(),
            this: receiver.clone(),
        };
        ops::install(&rt).map_err(|e| e.to_exchange_error())?;
        let weak = Rc::downgrade(&shared);
        let create_api = Js::function("createAPI", move |_, args| {
            let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
            api::create(
                &rt,
                safe_cex_exchange::js_runtime::value::arg(&args, 0),
                false,
            )
        });
        let limit = Js::function("rateLimit", |_, args| {
            let client = safe_cex_exchange::js_runtime::value::arg(&args, 0);
            client.call_method(
                "__configureRateLimit",
                vec![Js::object(vec![("maxRPS", Js::Number(3.0))])],
            )?;
            Ok(client)
        });
        let options = receiver.get("options").map_err(|e| e.to_exchange_error())?;
        let xhr = receiver
            .call_method(
                "createXhrWithSecondaryKey",
                vec![
                    options.clone(),
                    create_api,
                    Js::object(vec![("rateLimit", limit)]),
                ],
            )
            .map_err(|e| e.to_exchange_error())?;
        receiver
            .define("xhr", xhr)
            .map_err(|e| e.to_exchange_error())?;
        receiver
            .define(
                "publicXhr",
                api::create(&rt, options.clone(), true).map_err(|e| e.to_exchange_error())?,
            )
            .map_err(|e| e.to_exchange_error())?;
        receiver
            .define(
                "unlimitedXHR",
                api::create(&rt, options, false).map_err(|e| e.to_exchange_error())?,
            )
            .map_err(|e| e.to_exchange_error())?;
        let clients = ["_primaryXhr", "_secondaryXhr", "publicXhr", "unlimitedXHR"]
            .into_iter()
            .map(|key| receiver.get(key))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_exchange_error())?;
        receiver
            .call_method("installRequestWeightGuards", vec![Js::array(clients)])
            .map_err(|e| e.to_exchange_error())?;
        let venue = Rc::new(Self {
            scope: rt,
            capabilities: exchange::capabilities(),
        });
        let erased: Rc<dyn safe_cex_exchange::exchange::Exchange> = venue.clone();
        *shared.sockets.venue.borrow_mut() = Some(Rc::downgrade(&erased));
        let (public, _) = ws_public::new(&venue.scope).map_err(|e| e.to_exchange_error())?;
        receiver
            .define("publicWebsocket", public)
            .map_err(|e| e.to_exchange_error())?;
        let (private, _) = ws_private::new(&venue.scope).map_err(|e| e.to_exchange_error())?;
        receiver
            .define("privateWebsocket", private)
            .map_err(|e| e.to_exchange_error())?;
        // BaseExchange's source property lookups resolve these actual receiver
        // members into their Public/Private slots, including future replacement.
        Ok(venue)
    }

    pub fn receiver(&self) -> Js {
        self.scope.this.clone()
    }

    pub fn base(&self) -> &Rc<BaseExchange> {
        self.scope.base()
    }
}
