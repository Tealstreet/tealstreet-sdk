use super::values as v;
use super::*;
use crate::{
    classes::{self, BoundMember, ClassHost, ClassKind, ClassReturn, MemberBinding, NativeMember},
    lifecycle as l,
};
use safe_cex_exchange::{
    client_order_id::{ClientOrderIdRuleOverrides, pattern_client_order_id_rule},
    exchange::ExchangeSettings,
    js_runtime::{
        base_exchange::{self, BaseExchangeParts},
        scope::scope_of,
        sockets,
    },
};
use std::rc::{Rc, Weak};

pub(super) struct Factory(pub Weak<rt::Shared>);
impl Factory {
    fn host(&self) -> Result<Host, Thrown> {
        Ok(Host(scope_of(&self.0)?))
    }
}
pub(super) fn settings(h: &Host) -> ExchangeSettings {
    let runtime = h.0.shared.runtime.clone();
    ExchangeSettings {
        tick_interval: Some(2000.0),
        positions_supported: Some(true),
        supports_public_only: true,
        client_order_id_rule: Some(pattern_client_order_id_rule(
            "WEEX".into(),
            "/^[.A-Z:/a-z0-9_-]{1,36}$/".into(),
            Rc::new(|id| {
                (1..=36).contains(&id.len())
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".:/_-".contains(&b))
            }),
            ClientOrderIdRuleOverrides {
                mint: Some(Rc::new(move || {
                    std::format!("ts{}", runtime.unique_id().replace(['-', '_'], ""))
                        .chars()
                        .take(36)
                        .collect()
                })),
                ..Default::default()
            },
        )),
    }
}
fn table<K: serde::Serialize, V: serde::Serialize>(values: &[(K, V)]) -> Result<Js, Thrown> {
    let result = Js::object(std::vec![]);
    for (key, value) in values {
        result.define_key(&from_typed(key)?.to_js_string()?, from_typed(value)?)?;
    }
    Ok(result)
}
pub(super) fn constant(name: &str) -> Result<Js, Thrown> {
    use crate::types::*;
    match name {
        "GRAFANA_ENDPOINTS_EVERY" => from_typed(GRAFANA_ENDPOINTS_EVERY),
        "GRAFANA_ENDPOINTS_PERIODIC" => from_typed(GRAFANA_ENDPOINTS_PERIODIC),
        "ENDPOINTS" => table(ENDPOINTS),
        "INTERVAL" => table(INTERVAL),
        "TIME_IN_FORCE" => table(TIME_IN_FORCE),
        "POSITION_MODE" => table(POSITION_MODE),
        "MARGIN_TYPE" => table(MARGIN_TYPE),
        "REVERSE_POSITION_MODE" => table(REVERSE_POSITION_MODE),
        "REVERSE_MARGIN_TYPE" => table(REVERSE_MARGIN_TYPE),
        "REVERSE_POSITION_SIDE" => table(REVERSE_POSITION_SIDE),
        "REVERSE_ORDER_SIDE" => table(REVERSE_ORDER_SIDE),
        "REVERSE_ORDER_TYPE" => table(REVERSE_ORDER_TYPE),
        "REVERSE_TIME_IN_FORCE" => table(REVERSE_TIME_IN_FORCE),
        "REVERSE_ORDER_TRIGGER_TYPE" => table(REVERSE_ORDER_TRIGGER_TYPE),
        _ => Err(Thrown::type_error(&std::format!(
            "Unknown WEEX constant {name}"
        ))),
    }
}
impl ClassHost for Factory {
    type Value = Js;
    type Error = Thrown;
    fn exchange_runtime(&self) -> l::WeexExchangeConstructionRuntime<Self, Js, Thrown> {
        l::WeexExchangeConstructionRuntime {
            base_construct: |f, name, options, store| {
                let h = f.host()?;
                let (receiver, base) = base_exchange::create(
                    &h.0.shared.sockets,
                    BaseExchangeParts {
                        name: h.0.shared.extra.name.clone(),
                        options: h.0.shared.extra.options.clone(),
                        store: h.0.shared.store.clone(),
                        settings: settings(&h),
                        external_public_data: h.0.shared.extra.external_public_data.clone(),
                    },
                    name,
                    options,
                    store,
                    |_| Ok(()),
                )?;
                h.0.shared
                    .base
                    .set(base)
                    .map_err(|_| Thrown::type_error("WEEX base already constructed"))?;
                *h.0.shared.exchange.borrow_mut() = receiver.clone();
                Ok(receiver)
            },
            client_rule: |_, _| {
                Ok(Js::object(std::vec![
                    ("label", Js::str("WEEX")),
                    ("requirement", Js::str("match /^[.A-Z:/a-z0-9_-]{1,36}$/")),
                ]))
            },
            api_factory: |f| super::api::factory(&f.host()?),
            new_map: |f| v::new_map(&f.host()?, Js::Undefined),
            assign: |_, o, k, v| o.set(k, v),
            constant: |_, name| constant(name),
            define: |_, o, k, v| o.define(k, v),
            method: |_, o, k| safe_cex_exchange::js_runtime::scope::bound_member(o, k),
            call: |_, f, a| f.call(Js::Undefined, a),
            construct: |f, name, parent| match name {
                "WeexPublicWebsocket" => Ok(classes::WeexPublicWebsocket::new(
                    Rc::new(Factory(f.0.clone())),
                    parent,
                )?
                .receiver),
                "WeexPrivateWebsocket" => Ok(classes::WeexPrivateWebsocket::new(
                    Rc::new(Factory(f.0.clone())),
                    parent,
                )?
                .receiver),
                _ => Err(Thrown::type_error("Unknown WEEX socket class")),
            },
            undefined: || Js::Undefined,
            text: Js::str,
        }
    }
    fn public_runtime(
        &self,
    ) -> crate::public_feed::WeexPublicConstructionRuntime<Self, Js, Thrown> {
        crate::public_feed::WeexPublicConstructionRuntime {
            base_construct: |f, parent| super::sockets::base(&f.host()?, ClassKind::Public, parent),
            define: |_, o, k, v| o.define(k, v),
            new_set: |f| v::set(&f.host()?, Js::Undefined),
            object: Js::object,
            array: Js::array,
            number: Js::Number,
            boolean: Js::Bool,
            property: |_, o, k| o.get(k),
            method: |_, o, k| safe_cex_exchange::js_runtime::scope::bound_member(o, k),
            call_event: |_, f, this, event, handler| {
                f.call(this, std::vec![Js::str(event), handler])?;
                Ok(())
            },
        }
    }
    fn super_private(&self, parent: Js) -> Result<Js, Thrown> {
        super::sockets::base(&self.host()?, ClassKind::Private, parent)
    }
    fn define(&self, o: &Js, k: &'static str, v: Js) -> Result<(), Thrown> {
        o.define(k, v)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn boolean(&self, v: bool) -> Js {
        Js::Bool(v)
    }
    fn number(&self, v: f64) -> Js {
        Js::Number(v)
    }
    fn member(
        &self,
        kind: ClassKind,
        name: &'static str,
    ) -> Result<Option<MemberBinding<Js, Thrown>>, Thrown> {
        let Some(member) = classes::members(kind).iter().find(|m| m.name == name) else {
            return Ok(None);
        };
        let weak = self.0.clone();
        Ok(Some(MemberBinding {
            target: member.target,
            body: Rc::new(move |this, args| {
                let h = Host(scope_of(&weak)?.with_this(this));
                if kind == ClassKind::Exchange {
                    dispatch(h, name, args).map(ClassReturn::Value)
                } else {
                    super::sockets::dispatch(h, kind, name, args).map(ClassReturn::Value)
                }
            }),
        }))
    }
    fn binding_error(&self, _: ClassKind, name: &'static str, reason: &'static str) -> Thrown {
        Thrown::type_error(&std::format!("WEEX {name}: {reason}"))
    }
    fn arrow(&self, name: &'static str, body: BoundMember<Js, Thrown>) -> Result<Js, Thrown> {
        let weak = self.0.clone();
        Ok(Js::function(name, move |_, args| match body(args)? {
            ClassReturn::Value(value) => Ok(value),
            ClassReturn::Future(future) => Ok(scope_of(&weak)?.spawn(future)),
        }))
    }
    fn bind_receiver(
        &self,
        receiver: Js,
        body: NativeMember<Js, Thrown>,
    ) -> BoundMember<Js, Thrown> {
        let Js::Object(receiver) = receiver else {
            return Rc::new(|_| Err(Thrown::type_error("WEEX receiver is not an object")));
        };
        let weak = Rc::downgrade(&receiver);
        Rc::new(move |args| {
            body(
                Js::Object(
                    weak.upgrade()
                        .ok_or_else(|| Thrown::type_error("WEEX receiver was released"))?,
                ),
                args,
            )
        })
    }
    fn prototype(
        &self,
        kind: ClassKind,
        name: &'static str,
        body: NativeMember<Js, Thrown>,
    ) -> Result<(), Thrown> {
        let h = self.host()?;
        let mut protos = h.0.shared.extra.prototypes.borrow_mut();
        if !protos.iter().any(|(k, _)| *k == kind) {
            let prototype = Js::object(std::vec![]);
            if let Js::Object(data) = &prototype {
                *data.proto.borrow_mut() =
                    Some(sockets::base_websocket_class(&h.0.shared.sockets).get("prototype")?);
            }
            protos.push((kind, prototype));
        }
        let prototype = protos.iter().find(|(k, _)| *k == kind).unwrap().1.clone();
        if !prototype.has_own(name) {
            let weak = self.0.clone();
            prototype.define(
                name,
                Js::function(name, move |this, args| match body(this, args)? {
                    ClassReturn::Value(value) => Ok(value),
                    ClassReturn::Future(future) => Ok(scope_of(&weak)?.spawn(future)),
                }),
            )?;
        }
        Ok(())
    }
}

pub(super) fn lifecycle_runtime() -> l::WeexLifecycleRuntime<Host, Js, Thrown> {
    l::WeexLifecycleRuntime {
        base_construct: |h, name, options, store| {
            let f = Factory(Rc::downgrade(&h.0.shared));
            (f.exchange_runtime().base_construct)(&f, name, options, store)
        },
        client_rule: |_, receiver| receiver.get("clientOrderIdRule"),
        api_factory: super::api::factory,
        bind: |h, receiver, name| v::method(h, receiver, name),
        new_map: |h| v::new_map(h, Js::Undefined),
        assign: |_, o, k, v| o.set(k, v),
        constant: |_, name| constant(name),
        define: v::define,
        this: Host::this,
        get: v::get,
        optional_get: v::optional,
        method: v::method,
        call: v::invoke,
        call_async: v::invoke_async,
        construct: |h, name, parent| {
            let f = Rc::new(Factory(Rc::downgrade(&h.0.shared)));
            if name == "WeexPublicWebsocket" {
                Ok(classes::WeexPublicWebsocket::new(f, parent)?.receiver)
            } else {
                Ok(classes::WeexPrivateWebsocket::new(f, parent)?.receiver)
            }
        },
        then: |h, future, callback| {
            Ok(Box::pin(async move {
                let value = future.await?;
                l::lifecycle_callback(h, &lifecycle_runtime(), callback, value)
            }))
        },
        all: |_, a, b| {
            Ok(Box::pin(async move {
                let (a, b) = futures::future::try_join(a, b).await?;
                Ok(Js::array(std::vec![a, b]))
            }))
        },
        loop_callback: |h, callback| {
            let weak = h.weak();
            Ok(Js::function("tick", move |_, _| {
                l::lifecycle_callback(&weak()?, &lifecycle_runtime(), callback, Js::Undefined)
            }))
        },
        schedule_loop: |h, callback, interval| {
            h.global("loop")?
                .call(Js::Undefined, std::vec![callback, interval])?;
            Ok(())
        },
        object: Js::object,
        undefined: || Js::Undefined,
        text: Js::str,
        string_units: |s| Js::utf16(&s),
        boolean: Js::Bool,
        number: Js::Number,
        truthy: Js::truthy,
        strict_false: |v| v.strict_equals(&Js::Bool(false)),
        copy: |_, v| v.spread_object(),
        template: |_, v| v::units(v),
        numeric: |_, v| v.to_number(),
        equals_text: v::equals,
        hash_input: |_, value| Ok(String::from_utf16_lossy(&value.to_js_string()?).into_bytes()),
        error_value: |e| e.value.clone(),
        new_error: |_, v| Thrown::error("Error", &v.to_display()),
    }
}

fn symbol_runtime() -> crate::symbol_settings::WeexSymbolSettingsRuntime<Host, Js, Js, Thrown> {
    crate::symbol_settings::WeexSymbolSettingsRuntime {
        cached: |h, symbol| {
            h.root("symbolSettings")?
                .call_method("get", std::vec![Js::utf16(symbol)])
        },
        truthy: Js::truthy,
        fetch: |h, symbol| {
            let pending = h.call("fetchSymbolConfigs", std::vec![Js::utf16(symbol)]);
            Box::pin(async move { await_js(&h.0.exchange_runtime(), pending?).await })
        },
        fetched: |_, configs, symbol| configs.call_method("get", std::vec![Js::utf16(symbol)]),
        account_position_mode: |h| to_typed(&h.root("accountPositionMode")?),
        default_config: |_, config| v::default_config(config),
    }
}
fn dispatch(mut h: Host, name: &'static str, args: Vec<Js>) -> Result<Js, Thrown> {
    let first = arg(&args, 0);
    let second = arg(&args, 1);
    match name {
        "listenOHLCV" => {
            return crate::market_reads::listen_ohlcv(
                &h,
                &super::reads::market_runtime(),
                first,
                second,
            );
        }
        "listenOrderBook" => {
            return crate::market_reads::listen_order_book(
                &h,
                &super::reads::market_runtime(),
                first,
                second,
            );
        }
        "getSymbolConfigFromCache" => {
            return crate::symbol_settings::get_symbol_config_from_cache(
                &h,
                &symbol_runtime(),
                &v::units(&first)?,
            );
        }
        "formatOrderPayload" => {
            return crate::trading::format_order_payload(&h, &super::trading::runtime(), first);
        }
        "inferPositionSide" => {
            return from_typed(&crate::order_routing::infer_position_side(
                to_typed(&first.get("side")?).ok(),
                first.get("reduceOnly")?.truthy(),
            ));
        }
        "isAlgoOrder" => {
            return Ok(Js::Bool(crate::order_routing::is_algo_order(
                to_typed(&first.get("type")?).ok(),
                &v::units(&first.get("id")?)?,
            )));
        }
        "isBatchableRegularOrder" => {
            let defined = |name| first.get(name).map(|v| !v.is_undefined());
            return Ok(Js::Bool(crate::order_routing::is_batchable_regular_order(
                &crate::order_routing::WeexBatchOrderFields {
                    order_type: to_typed(&first.get("type")?).ok(),
                    trigger_price_defined: defined("triggerPrice")?,
                    stop_loss_defined: defined("stopLoss")?,
                    take_profit_defined: defined("takeProfit")?,
                    reduce_only_is_true: first.get("reduceOnly")?.strict_equals(&Js::Bool(true)),
                    close_is_true: first.get("close")?.strict_equals(&Js::Bool(true)),
                    tp_trigger_type_defined: defined("tpTriggerType")?,
                    sl_trigger_type_defined: defined("slTriggerType")?,
                },
            )));
        }
        "parseOrderResult" => {
            return crate::order_result::parse_order_result(
                &mut h,
                &crate::order_result::WeexOrderResultRuntime {
                    optional_property: |_, o, k| v::optional_dummy(o, k),
                    is_false: |v| v.strict_equals(&Js::Bool(false)),
                    truthy: Js::truthy,
                    string: |_, s| Ok(Js::str(s)),
                    emit_error: |h, message| h.emit(message.clone()),
                    new_error: |_, v| Thrown::error("Error", &v.to_display()),
                    uuid: |h| {
                        Ok(Js::str(
                            &h.0.shared.runtime.unique_id().replace(['-', '_'], ""),
                        ))
                    },
                    integer_string: |_, v| v::integer(v).and_then(|v| v::units(&v)),
                },
                &first,
                second,
            )
            .map(|s| Js::utf16(&s));
        }
        _ => {}
    }
    let spawn = h.clone();
    Ok(spawn.spawn(Box::pin(async move {
        use crate::{account_actions as a, account_reads as r, market_reads as m, trading as t};
        let life = lifecycle_runtime();
        let market = super::reads::market_runtime();
        let account = super::reads::account_runtime();
        let trade = super::trading::runtime();
        let action = super::actions::runtime();
        match name {
            "validateAccount" => l::validate_account(&h, &life).await,
            "getAccount" => l::get_account(&h, &life).await,
            "start" => l::start(&h, &life).await,
            "tick" => l::tick(&h, &life).await,
            "fetchMarkets" => m::fetch_markets(&h, &market).await,
            "fetchTickers" => m::fetch_tickers(&h, &market).await,
            "fetchOHLCV" => m::fetch_ohlcv(&h, &market, first).await,
            "fetchOrderBook" => m::fetch_order_book(&h, &market, first, second).await,
            "fetchPublicTrades" => m::fetch_public_trades(&h, &market, first, second).await,
            "fetchUserTradeRows" => m::fetch_user_trade_rows(&h, &market, first).await,
            "fetchHistoricalFills" => m::fetch_historical_fills(&h, &market, first).await,
            "fetchSymbolMaxLeverage" => r::fetch_symbol_max_leverage(&h, &account, first).await,
            "fetchLeverageForSymbol" => r::fetch_leverage_for_symbol(&h, &account, first).await,
            "fetchBalance" => r::fetch_balance(&h, &account).await,
            "fetchBalanceAndPositions" => r::fetch_balance_and_positions(&h, &account).await,
            "fetchOrders" => r::fetch_orders(&h, &account, first).await,
            "fetchNormalOrders" => r::fetch_normal_orders(&h, &account, first).await,
            "fetchAlgoOrders" => r::fetch_algo_orders(&h, &account, first).await,
            "fetchSymbolConfigs" => r::fetch_symbol_configs(&h, &account, first).await,
            "fetchOpenOrderRows" => {
                r::fetch_open_order_rows(&h, &account, v::units(&first)?, second).await
            }
            "getOrFetchSymbolConfig" => {
                let symbol = v::units(&first)?;
                crate::symbol_settings::get_or_fetch_symbol_config(
                    &h,
                    &symbol_runtime(),
                    &symbol,
                    second.truthy(),
                )
                .await
            }
            "fetchIncomeHistory" => {
                crate::income_reads::fetch_income_history(
                    &h,
                    &super::reads::income_runtime(),
                    first,
                )
                .await
            }
            "fetchIncomeHistoryRange" => {
                let result = crate::income_reads::fetch_income_history_range(
                    Rc::new(h.clone()),
                    &super::reads::range_runtime(),
                    h.0.exchange_runtime(),
                    first.to_number()?,
                    second.to_number()?,
                    arg(&args, 2),
                )
                .await
                .map_err(Thrown::from)?;
                from_typed(&result)
            }
            "placeOrder" => t::place_order(&h, &trade, first).await,
            "placeOrders" => t::place_orders(&h, &trade, first).await,
            "placeRegularOrderBatch" => t::place_regular_order_batch(&h, &trade, first).await,
            "updateOrder" => t::update_order(&h, &trade, first).await,
            "fetchPositions" => t::fetch_positions(&h, &trade, first).await,
            _ => {
                match name {
                    "cancelOrders" => {
                        let orders = first
                            .array_values()?
                            .ok_or_else(|| Thrown::type_error("orders is not iterable"))?;
                        a::cancel_orders(&h, &action, &orders).await?;
                    }
                    "cancelSymbolOrders" => {
                        a::cancel_symbol_orders(&h, &action, &v::units(&first)?).await?
                    }
                    "cancelAllOrders" => a::cancel_all_orders(&h, &action).await?,
                    "updateSymbolMetadata" => {
                        a::update_symbol_metadata(&h, &action, &v::units(&first)?).await?
                    }
                    "setLeverage" => {
                        a::set_leverage(
                            &h,
                            &action,
                            &v::units(&first)?,
                            second.to_number()?,
                            if arg(&args, 2).is_undefined() {
                                None
                            } else {
                                Some(to_typed(&arg(&args, 2))?)
                            },
                        )
                        .await?
                    }
                    "changeMarginType" | "changePositionMode" => {
                        let symbol = if second.is_undefined() {
                            None
                        } else {
                            Some(v::units(&second)?)
                        };
                        if name == "changeMarginType" {
                            a::change_margin_type(&h, &action, first.truthy(), symbol.as_deref())
                                .await?;
                        } else {
                            a::change_position_mode(&h, &action, first.truthy(), symbol.as_deref())
                                .await?;
                        }
                    }
                    "nukePositions" => a::nuke_positions(&h, &action).await?,
                    "nuke" => a::nuke(&h, &action).await?,
                    _ => {
                        return Err(Thrown::type_error(&std::format!(
                            "Unknown WEEX exchange member {name}"
                        )));
                    }
                }
                Ok(Js::Undefined)
            }
        }
    })))
}
