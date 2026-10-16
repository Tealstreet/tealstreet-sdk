//! Constructor collaborators for the existing GRVT composition port.
use super::{
    methods,
    runtime::{Host, value},
};
use crate::builder_workflow::JsResult;
use crate::{
    composition::{ClientOrderIdRule, CompositionHost, Fields, NonceIds, NonceTimes},
    private_commands::PrivateCommands,
    public_subscriptions::PublicSubscriptions,
};
use safe_cex_exchange::{
    js_runtime::{
        base_exchange::{self, BaseExchangeParts},
        sockets,
        value::{Js, arg},
    },
    runtime::LogSeverity,
};
use std::rc::Rc;

pub(super) fn client_order_id_rule() -> safe_cex_exchange::client_order_id::ClientOrderIdRule {
    use safe_cex_exchange::client_order_id::*;
    ClientOrderIdRule {
        label: "GRVT".into(),
        requirement: "be a canonical decimal integer between 1 and 9223372036854775807".into(),
        is_valid: Rc::new(|id| crate::composition::ClientOrderIdRule.is_valid(id)),
        is_honorable_shape: None,
        mint: None,
    }
}
impl CompositionHost for Host {
    type Timer = u64;
    type PrivateTimer = Js;
    type FillKey = super::private::Key;
    fn base_exchange(&self, name: Js, opts: Js, store: Js) -> JsResult<Js> {
        let x = &self.0.shared.extra;
        let (receiver, base) = value(base_exchange::create(
            &self.0.shared.sockets,
            BaseExchangeParts {
                name: x.name.clone(),
                options: x.options.clone(),
                store: self.0.shared.store.clone(),
                settings: super::exchange::settings(),
                external_public_data: x.external_public_data.clone(),
            },
            name,
            opts,
            store,
            |_| Ok(()),
        ))?;
        let _ = self.0.shared.base.set(base);
        Ok(receiver)
    }
    fn base_socket(&self, parent: Js) -> JsResult<Js> {
        Ok(sockets::base_websocket(&self.0.shared.sockets, parent))
    }
    fn attach_fields(&self, instance: &Js, fields: Fields<Js>) {
        let exchange = instance.has_own("options");
        let keys: Vec<&'static str> = if exchange {
            let mut keys = vec![
                "clientOrderIdRule",
                "xhr",
                "edgeXhr",
                "tradingXhr",
                "publicWebsocket",
                "privateWebsocket",
                "wallet",
                "grvtClient",
                "grvtTradingAccountId",
                "apiExtras",
                "defaultLeverage",
                "_nonceToOrderId",
                "_nonceCreationTimes",
                "_builderAuthState",
                "grafanaEndpointsEvery",
                "grafanaEndpointsPeriodic",
            ];
            keys.extend(crate::composition::EXCHANGE_MEMBERS);
            keys
        } else {
            let mut keys = vec![
                "subscriptions",
                "requestId",
                "topics",
                "tickerSubscriptions",
                "tickerBatchTimers",
                "tickerBatchGeneration",
                "reportedMissingProxyHost",
                "treatAnyMessageAsPong",
                "authenticated",
                "authenticationFailed",
                "sessionRefreshInFlight",
                "sessionRefreshAttempts",
                "privateStreamHealthyAt",
                "lastPositionFetchTime",
                "positionFetchThrottleMs",
                "positionFetchScheduled",
                "hasPendingPositionEvents",
                "ordersThrottleTimeout",
                "processedFillIds",
            ];
            keys.extend(crate::composition::PUBLIC_MEMBERS);
            keys.extend(crate::composition::PRIVATE_MEMBERS);
            keys.sort_unstable();
            keys.dedup();
            keys
        };
        for key in keys {
            let get = fields.clone();
            let set = fields.clone();
            instance.define_accessor(
                key,
                Some(Rc::new(move |_| {
                    let current = get.borrow().get(key).cloned().unwrap_or(Js::Undefined);
                    super::fields::read(current)
                })),
                Some(Rc::new(move |_, v| {
                    let current = set.borrow().get(key).cloned().unwrap_or(Js::Undefined);
                    if !super::fields::write(&current, v.clone())? {
                        set.borrow_mut().insert(key, v);
                    }
                    Ok(())
                })),
            );
        }
    }
    fn rule_value(&self, rule: Rc<ClientOrderIdRule>) -> Js {
        Js::object(vec![
            ("label", Js::str("GRVT")),
            (
                "requirement",
                Js::str("be a canonical decimal integer between 1 and 9223372036854775807"),
            ),
            (
                "isValid",
                Js::function("isValid", move |_, a| {
                    Ok(Js::Bool(rule.is_valid(&String::from_utf16_lossy(
                        &arg(&a, 0).to_js_string()?,
                    ))))
                }),
            ),
        ])
    }
    fn nonce_ids_view(&self, ids: NonceIds) -> Js {
        super::nonce::ids(self, ids)
    }
    fn nonce_times_view(&self, times: NonceTimes) -> Js {
        super::nonce::times(self, times)
    }
    fn public_field(&self, state: Rc<PublicSubscriptions<u64>>, key: &str) -> Js {
        use super::fields;
        match key {
            "requestId" => {
                let get = state.clone();
                fields::number(
                    Rc::new(move || get.request_id.get()),
                    Rc::new(move |v| state.request_id.set(v)),
                )
            }
            "tickerBatchGeneration" => {
                let get = state.clone();
                fields::number(
                    Rc::new(move || get.ticker_batch_generation.get()),
                    Rc::new(move |v| state.ticker_batch_generation.set(v)),
                )
            }
            "subscriptions" => {
                let get = state.clone();
                fields::readonly(fields::map(
                    Rc::new(move || {
                        get.subscriptions
                            .borrow()
                            .iter()
                            .map(|(id, v)| {
                                Ok((
                                    Js::Number(id.value()),
                                    safe_cex_exchange::js_runtime::value::from_typed(v)?,
                                ))
                            })
                            .collect()
                    }),
                    Rc::new(move || state.subscriptions.borrow_mut().clear()),
                ))
            }
            "tickerSubscriptions" => {
                let get = state.clone();
                fields::readonly(fields::map(
                    Rc::new(move || {
                        Ok(get
                            .ticker_subscriptions
                            .borrow()
                            .iter()
                            .map(|(key, v)| (Js::str(key), Js::Number(*v)))
                            .collect())
                    }),
                    Rc::new(move || state.ticker_subscriptions.borrow_mut().clear()),
                ))
            }
            "tickerBatchTimers" => {
                let get = state.clone();
                fields::readonly(fields::set(
                    Rc::new(move || {
                        get.ticker_batch_timers
                            .values()
                            .into_iter()
                            .map(|id| Js::Number(id as f64))
                            .collect()
                    }),
                    Rc::new(move || {
                        state
                            .cancel_ticker_batches(|_| Ok(()))
                            .expect("infallible timer view clear");
                    }),
                ))
            }
            "topics" => Js::object(vec![]),
            _ => unreachable!("composition public field inventory"),
        }
    }
    fn private_field(&self, state: Rc<PrivateCommands<Js, super::private::Key>>, key: &str) -> Js {
        use super::fields;
        macro_rules! boolean {
            ($field:ident) => {{
                let get = state.clone();
                fields::boolean(
                    Rc::new(move || get.$field.get()),
                    Rc::new(move |v| state.$field.set(v)),
                )
            }};
        }
        macro_rules! number {
            ($field:ident) => {{
                let get = state.clone();
                fields::number(
                    Rc::new(move || get.$field.get()),
                    Rc::new(move |v| state.$field.set(v)),
                )
            }};
        }
        match key {
            "requestId" => number!(request_id),
            "sessionRefreshAttempts" => number!(session_refresh_attempts),
            "lastPositionFetchTime" => number!(last_position_fetch_time),
            "positionFetchThrottleMs" => number!(position_fetch_throttle_ms),
            "reportedMissingProxyHost" => boolean!(reported_missing_proxy_host),
            "authenticated" => boolean!(authenticated),
            "authenticationFailed" => boolean!(authentication_failed),
            "sessionRefreshInFlight" => boolean!(session_refresh_in_flight),
            "positionFetchScheduled" => boolean!(position_fetch_scheduled),
            "hasPendingPositionEvents" => boolean!(has_pending_position_events),
            "privateStreamHealthyAt" => {
                let get = state.clone();
                fields::view(
                    Rc::new(move || {
                        Ok(get
                            .private_stream_healthy_at
                            .get()
                            .map_or(Js::Undefined, Js::Number))
                    }),
                    Rc::new(move |v| {
                        state.private_stream_healthy_at.set(if v.is_undefined() {
                            None
                        } else {
                            Some(v.to_number()?)
                        });
                        Ok(())
                    }),
                )
            }
            "ordersThrottleTimeout" => {
                let get = state.clone();
                fields::view(
                    Rc::new(move || {
                        Ok(get
                            .orders_throttle_timeout
                            .borrow()
                            .clone()
                            .unwrap_or(Js::Undefined))
                    }),
                    Rc::new(move |v| {
                        *state.orders_throttle_timeout.borrow_mut() =
                            if v.is_undefined() { None } else { Some(v) };
                        Ok(())
                    }),
                )
            }
            "subscriptions" => {
                let get = state.clone();
                fields::readonly(fields::map(
                    Rc::new(move || {
                        Ok(get
                            .subscriptions
                            .borrow()
                            .iter()
                            .map(|(id, v)| {
                                (
                                    Js::Number(id.value()),
                                    Js::object(vec![
                                        ("request_id", Js::Number(v.request_id)),
                                        ("method", Js::str("subscribe")),
                                        ("stream", Js::str(&v.stream)),
                                        (
                                            "feed",
                                            Js::array(
                                                v.feed
                                                    .borrow()
                                                    .iter()
                                                    .map(|v| Js::str(v))
                                                    .collect(),
                                            ),
                                        ),
                                        ("is_full", Js::Bool(v.is_full)),
                                    ]),
                                )
                            })
                            .collect())
                    }),
                    Rc::new(move || state.subscriptions.borrow_mut().clear()),
                ))
            }
            "processedFillIds" => {
                let get = state.clone();
                fields::readonly(fields::set(
                    Rc::new(move || {
                        get.processed_fill_ids
                            .borrow()
                            .iter()
                            .map(|v| v.0.clone())
                            .collect()
                    }),
                    Rc::new(move || state.processed_fill_ids.borrow_mut().clear()),
                ))
            }
            "treatAnyMessageAsPong" => Js::Bool(true),
            _ => unreachable!("composition private field inventory"),
        }
    }
    fn member(&self, class: &'static str, name: &'static str, instance: &Js) -> Js {
        let weak = Rc::downgrade(&self.0.shared);
        let receiver = instance.clone();
        Js::function(name, move |_, args| {
            let host = Host::from_shared(
                weak.upgrade().ok_or_else(|| {
                    safe_cex_exchange::js_runtime::value::Thrown::error(
                        "Error",
                        "exchange runtime disposed",
                    )
                })?,
                receiver.clone(),
            );
            if class == "exchange" {
                methods::invoke(host, name, args)
            } else {
                super::socket::invoke(host, class, name, args)
            }
        })
    }
    fn prototype_members(&self, instance: &Js, members: &indexmap::IndexMap<&'static str, Js>) {
        for (key, value) in members {
            let _ = instance.define(key, value.clone());
        }
    }
    fn grafana(&self, periodic: bool) -> Js {
        Js::array(
            if periodic {
                crate::protocol::GRAFANA_ENDPOINTS_PERIODIC
            } else {
                crate::protocol::GRAFANA_ENDPOINTS_EVERY
            }
            .iter()
            .map(|s| Js::str(s))
            .collect(),
        )
    }
    fn api(&self, kind: &'static str, opts: Js, instance: Js) -> JsResult<Js> {
        match kind {
            "market" => crate::api_runtime::create_api(Rc::new(self.clone()), opts, instance),
            "edge" => crate::api_runtime::create_edge_api(Rc::new(self.clone()), opts, instance),
            _ => crate::api_runtime::create_trading_api(Rc::new(self.clone()), opts, instance),
        }
    }
    fn rate_limit(&self, instance: Js, config: Js) -> JsResult<Js> {
        value(instance.call_method("__configureRateLimit", vec![config]))?;
        Ok(instance)
    }
    fn sdk_env(&self, testnet: bool) -> Js {
        Js::str(if testnet { "TESTNET" } else { "PRODUCTION" })
    }
    fn sdk_client(&self, config: Js) -> JsResult<Js> {
        value(super::sdk::create(self, config))
    }
    fn socket(&self, private: bool, parent: Js) -> JsResult<Js> {
        if private {
            let model = crate::composition::new_private_socket(self, parent)?;
            let receiver = model.base.clone();
            let owner = super::private::Private::new(self, model);
            self.0.shared.extra.private.borrow_mut().push(owner);
            Ok(receiver)
        } else {
            let model = crate::composition::new_public_socket(self, parent)?;
            let receiver = model.base.clone();
            let owner = super::public::Public::new(self, model);
            self.0.shared.extra.public.borrow_mut().push(owner);
            Ok(receiver)
        }
    }

    fn same_string(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn severity(&self, severity: LogSeverity) -> Js {
        Js::str(severity.as_str())
    }
}
