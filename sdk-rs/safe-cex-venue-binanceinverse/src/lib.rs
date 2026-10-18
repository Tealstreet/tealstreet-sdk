//! Binance COIN-M: source-corresponding native venue assembly.
pub mod types;

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
        types::install(&shared.globals).map_err(|e| e.to_exchange_error())?;
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
        Ok(Rc::new(Self {
            scope: Scope {
                shared,
                this: receiver,
            },
        }))
    }

    pub fn receiver(&self) -> Js {
        self.scope.this.clone()
    }

    pub fn base(&self) -> &Rc<BaseExchange> {
        self.scope.base()
    }
}
