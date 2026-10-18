use super::{Rt, Shared, host::Host, scope, text};
use safe_cex_exchange::js_runtime::{
    base_exchange, client,
    promise::await_js,
    scope::bound_member,
    value::{Js, Thrown, arg, from_typed, to_json, to_retained_input, to_typed},
};
use std::rc::{Rc, Weak};
pub(super) fn install_globals(g: &Js, weak: Weak<Shared>) {
    let logging = weak.clone();
    g.get("console")
        .expect("builtin console")
        .define(
            "log",
            Js::function("log", move |_, a| {
                let rt = scope(&logging, Js::Undefined)?;
                let parts = a.iter().map(super::text).collect::<Result<Vec<_>, _>>()?;
                rt.shared.runtime.console("log", &parts.join(" "));
                Ok(Js::Undefined)
            }),
        )
        .expect("console object");
    let auth_weak = weak.clone();
    g.define(
        "authenticateRequest",
        Js::function("authenticateRequest", move |_, args| {
            let rt = scope(&auth_weak, arg(&args, 1))?;
            let task = rt.clone();
            let config = arg(&args, 0);
            Ok(rt.spawn(Box::pin(async move {
                crate::auth_initialization::authenticate_request(&Host::new(&task), config).await
            })))
        }),
    )
    .expect("fresh globals");
    for (n, f) in [
        (
            "safeCexAxiosCreate",
            http_create as fn(&Rt, Vec<Js>) -> Result<Js, Thrown>,
        ),
        ("rateLimit", rate_limit),
        ("retry", retry),
        ("installTransportAdapter", transport),
        ("networkErrorOnRead", network_condition),
        ("patternClientOrderIdRule", pattern_rule),
    ] {
        let weak = weak.clone();
        g.define(
            n,
            Js::function(n, move |_, a| f(&scope(&weak, Js::Undefined)?, a)),
        )
        .expect("global object");
    }
}
fn http_create(rt: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let opts = base_exchange::exchange_options(&arg(&a, 1))?;
    let host = (rt.shared.extra.deps.http)(&opts).map_err(Thrown::from)?;
    let client = client::factory::create(
        host.clone(),
        rt.shared.runtime.transport().as_ref(),
        arg(&a, 0),
        &arg(&a, 1),
        rt.base(),
    )?;
    *rt.shared.extra.api_http.borrow_mut() = Some(host);
    Ok(client)
}
fn rate_limit(_: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let xhr = arg(&a, 0);
    xhr.call_method("__configureRateLimit", vec![arg(&a, 1)])?;
    Ok(xhr)
}
fn retry(_: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    arg(&a, 0).call_method("__configureRetry", vec![arg(&a, 1)])?;
    Ok(Js::Undefined)
}
fn transport(rt: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let host = rt
        .shared
        .extra
        .api_http
        .borrow()
        .clone()
        .ok_or_else(|| Thrown::error("Error", "Paradex client factory has not run"))?;
    let xhr = arg(&a, 0);
    client::factory::install_transport_adapter(
        host,
        rt.shared.runtime.transport().as_ref(),
        &xhr,
        &arg(&a, 1),
        to_typed(&arg(&a, 2))?,
        safe_cex_transport::RestAdapterVariant::Default,
    )?;
    Ok(xhr)
}
fn network_condition(_: &Rt, _: Vec<Js>) -> Result<Js, Thrown> {
    Ok(Js::function("networkErrorOnRead", |_, a| {
        let e = arg(&a, 0);
        let config = e.get("config")?;
        if config.is_nullish() {
            return Ok(Js::Bool(false));
        }
        let m = config.get("method")?;
        let m = if m.is_undefined() {
            "get".into()
        } else {
            text(&m)?.to_lowercase()
        };
        if !matches!(m.as_str(), "get" | "head" | "options") {
            return Ok(Js::Bool(false));
        }
        let code = e.get("code")?;
        let response = e.get("response")?;
        let failure = safe_cex_transport::RestFailure {
            code: if code.is_nullish() {
                None
            } else {
                Some(text(&code)?)
            },
            message: text(&e.get("message")?)?,
            transport_error: None,
            response: if response.is_nullish() {
                None
            } else {
                Some(Box::new(safe_cex_transport::RestResponse {
                    status: response.get("status")?.to_number()? as u16,
                    reason: None,
                    headers: Default::default(),
                    body: safe_cex_transport::RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::Null,
                    ),
                }))
            },
            request: Box::new(safe_cex_transport::RestRequest {
                method: m,
                ..Default::default()
            }),
            context: None,
        };
        Ok(Js::Bool(safe_cex_transport::retry::is_network_error(
            &failure,
        )))
    }))
}
fn pattern_rule(_: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let overrides = arg(&a, 2).spread_object()?;
    overrides.define("label", arg(&a, 0))?;
    overrides.define("pattern", arg(&a, 1))?;
    Ok(overrides)
}
pub(super) fn allocate(rt: &Rt, c: &'static str, a: Vec<Js>) -> Result<Js, Thrown> {
    let owner = match c {
        "ParadexExchange" => {
            let parts = rt
                .shared
                .extra
                .parts
                .borrow_mut()
                .take()
                .ok_or_else(|| Thrown::error("Error", "Paradex base already constructed"))?;
            let (owner, base) = base_exchange::create(
                &rt.shared.sockets,
                parts,
                arg(&a, 0),
                arg(&a, 1),
                arg(&a, 2),
                |_| Ok(()),
            )?;
            rt.shared
                .base
                .set(base)
                .map_err(|_| Thrown::error("Error", "Paradex base already constructed"))?;
            owner
        }
        "ParadexPublicWebSocket" | "ParadexPrivateWebSocket" => {
            safe_cex_exchange::js_runtime::sockets::base_websocket(&rt.shared.sockets, arg(&a, 0))
        }
        "ParadexRequestSigner" | "ParadexJWTManager" => Js::object_with_class(c, vec![]),
        _ => return Err(Thrown::type_error(&format!("Unknown Paradex class {c}"))),
    };
    let methods: &[&str] = match c {
        "ParadexExchange" => &[
            "initializeSDK",
            "ensurePrivateHistoryAuth",
            "resetSDKStateAfterInitFailure",
            "getPositionMode",
            "getParaclearBalance",
            "getSocializedLossFactor",
        ],
        "ParadexRequestSigner" => &[
            "getAccountAddress",
            "signOrder",
            "signModifyOrder",
            "signRequest",
            "toChainIdHex",
            "toChainQuantum",
        ],
        "ParadexJWTManager" => &["getToken", "generateNewToken", "clearToken"],
        _ => &[],
    };
    for n in methods {
        owner.define(n, callback(&rt.with_this(owner.clone()), n, vec![]))?;
    }
    Ok(owner)
}
pub(super) fn callback(rt: &Rt, n: &str, captures: Vec<Js>) -> Js {
    let weak = Rc::downgrade(&rt.shared);
    let owner = rt.this.clone();
    let name = n.to_owned();
    Js::function(n, move |_, a| {
        let rt = scope(&weak, owner.clone())?;
        if let Some(value) = sync(&rt, &name, &a, &captures)? {
            return Ok(value);
        }
        let n = name.clone();
        let c = captures.clone();
        let task = rt.clone();
        Ok(rt.spawn(Box::pin(async move { dispatch(task, &n, a, c).await })))
    })
}
fn sync(rt: &Rt, n: &str, a: &[Js], c: &[Js]) -> Result<Option<Js>, Thrown> {
    let h = Host::new(rt);
    let x = arg(a, 0);
    Ok(Some(match n {
        "super" => Js::Undefined,
        "rule-mint" => crate::runtime_assembly::mint_rule_client_id(&h)?,
        "rule-shape" => crate::runtime_assembly::honorable_rule_shape(&h, x)?,
        "api-serialize" => crate::runtime_assembly::serialize_api_params(&h, x)?,
        "retry-delay" => crate::runtime_assembly::api_retry_delay(&h, x)?,
        "api-success" => crate::runtime_assembly::api_success(&h, x)?,
        "api-error" => crate::runtime_assembly::api_error(&h, x)?,
        "initializeSDKPromise" => rt.this.call_method("initializeSDK", vec![])?,
        "createAPI" => crate::runtime_assembly::create_api(&h, x, arg(a, 1))?,
        "rateLimit" => rate_limit(rt, a.to_vec())?,
        "newPublicWebSocket" => super::sockets::construct(rt, false, x)?,
        "newPrivateWebSocket" => super::sockets::construct(rt, true, x)?,
        "generateClientId" => Js::utf16(&crate::order_commands::generate_client_id(
            || {
                rt.global("uuid")?
                    .call(Js::Undefined, vec![])?
                    .to_js_string()
                    .map(|v| v.to_vec())
            },
            |e| Ok::<_, Thrown>(Thrown::error("Error", e)),
        )?),
        "isNativeBatchOrder" => {
            let o: safe_cex_types::PlaceOrderOpts = to_typed(&x)?;
            Js::Bool(crate::batch_predicate::is_native_batch_order(
                &crate::batch_predicate::NativeBatchOrder {
                    r#type: o.r#type,
                    trigger_price: o.trigger_price,
                    stop_loss: o.stop_loss,
                    take_profit: o.take_profit,
                    tp_trigger_type: o.tp_trigger_type,
                    sl_trigger_type: o.sl_trigger_type,
                    close: o.close,
                    time_in_force: o.time_in_force,
                },
            ))
        }
        "mapOrderType" => Js::str(crate::order_maps::map_order_type(to_typed(&x)?)),
        "mapTimeInForce" => {
            let t = if x.is_nullish() {
                None
            } else {
                Some(text(&x)?)
            };
            table_hit(rt, crate::order_maps::map_time_in_force(t.as_deref()))?
        }
        "reverseMapOrderType" => table_hit(
            rt,
            crate::order_maps::reverse_map_order_type(
                &text(&x)?,
                if arg(a, 1).is_undefined() {
                    None
                } else {
                    Some(arg(a, 1).truthy())
                },
            ),
        )?,
        "mapOrderStatus" => table_hit(rt, crate::order_maps::map_order_status(&text(&x)?))?,
        "mapOrder" => {
            let raw = to_retained_input(&x)?;
            if !raw.is_object() {
                return Err(Thrown::type_error("Order row must be an object"));
            }
            from_typed(
                &crate::order_mapper::map_order(&raw, |ms| {
                    (rt.shared.extra.deps.local_wall_time_to_utc)(ms)
                })
                .map_err(|e| Thrown::error("Error", &e))?,
            )?
        }
        "validateTpSlPrices" => {
            let o: safe_cex_types::PlaceOrderOpts = to_typed(&x)?;
            Js::Bool(crate::bracket_validation::validate_tp_sl_prices(
                o.side,
                o.price,
                o.take_profit,
                o.stop_loss,
                |s, l| h.log(h.logger()?, s.encode_utf16().collect(), l),
            )?)
        }
        "listenOrderBook" if rt.this.get("parent")?.is_undefined() => {
            crate::subscription_delegates::listen_order_book(x, arg(a, 1), |symbol, callback| {
                rt.this
                    .get("publicWebsocket")?
                    .call_method("listenOrderBook", vec![symbol, callback])
            })?
        }
        "listenOHLCV" if rt.this.get("parent")?.is_undefined() => {
            crate::subscription_delegates::listen_ohlcv(x, arg(a, 1), |opts, callback| {
                rt.this
                    .get("publicWebsocket")?
                    .call_method("listenOHLCV", vec![opts, callback])
            })?
        }
        "toChainIdHex" => crate::account_rest::to_chain_id_hex(&h, x)?,
        "toChainQuantum" => super::sdk::quantum(rt, &x, &arg(a, 1))?,
        "getAccountAddress" => crate::sdk_state::get_account_address(
            || rt.this.get("sdkAccount")?.get("account"),
            |a| a.get("address"),
        )?,
        "clearToken" => {
            rt.this.set("token", Js::Null)?;
            rt.this.set("tokenExpiry", Js::Number(0.0))?;
            rt.this.set("refreshPromise", Js::Null)?;
            Js::Undefined
        }
        "resetSDKStateAfterInitFailure" => {
            crate::state_operations::reset_sdk_state_after_init_failure::<Js, Js, Js, Js, Thrown>(
                |_| rt.this.set("sdkInitPromise", Js::Null),
                |_| rt.this.set("sdkAccount", Js::Null),
                |_| rt.this.set("requestSigner", Js::Null),
                |_| rt.this.set("jwtManager", Js::Null),
            )?;
            Js::Undefined
        }
        "dispose" if !rt.this.get("parent")?.is_undefined() => {
            return super::sockets::sync(rt, n, a, c);
        }
        "dispose" => {
            let flag = std::cell::Cell::new(false);
            crate::lifecycle::dispose(
                &flag,
                || {
                    rt.this.set("isDisposed", Js::Bool(flag.get()))?;
                    rt.this
                        .get("publicWebsocket")?
                        .call_method("dispose", vec![])?;
                    Ok::<_, Thrown>(())
                },
                || {
                    rt.this
                        .get("privateWebsocket")?
                        .call_method("dispose", vec![])?;
                    Ok(())
                },
            )?;
            Js::Undefined
        }
        _ => return super::sockets::sync(rt, n, a, c),
    }))
}
fn table_hit<T: serde::Serialize>(rt: &Rt, hit: safe_cex_num::TableHit<T>) -> Result<Js, Thrown> {
    match hit {
        safe_cex_num::TableHit::Mapped(x) => from_typed(&x),
        safe_cex_num::TableHit::Missing => Ok(Js::Undefined),
        safe_cex_num::TableHit::Inherited(i) => {
            use safe_cex_num::ProtoMember as P;
            let key = match i {
                P::Constructor => "constructor",
                P::DefineGetter => "__defineGetter__",
                P::DefineSetter => "__defineSetter__",
                P::HasOwnProperty => "hasOwnProperty",
                P::LookupGetter => "__lookupGetter__",
                P::LookupSetter => "__lookupSetter__",
                P::IsPrototypeOf => "isPrototypeOf",
                P::PropertyIsEnumerable => "propertyIsEnumerable",
                P::ToString => "toString",
                P::ValueOf => "valueOf",
                P::Prototype => "__proto__",
                P::ToLocaleString => "toLocaleString",
            };
            let _ = rt;
            Js::object(vec![]).get(key)
        }
    }
}
pub(super) fn select(rt: &Rt, n: &str) -> Result<Js, Thrown> {
    let target = match n {
        "get" => "xhr.get",
        "post" => "xhr.post",
        "put" => "xhr.put",
        "signOrder" => "requestSigner.signOrder",
        "signModifyOrder" => "requestSigner.signModifyOrder",
        "ensureAuth" => "ensurePrivateHistoryAuth",
        "buildOrder" => "buildMainOrderPayload",
        "subscribeBook" => "subscribeToOrderBook",
        "unsubscribeBook" => "unsubscribeFromOrderBook",
        "cleanupStats" => "cleanupMessageHandlerStats",
        "setSlippagePromise" => "parent.setMaxSlippage",

        "resolveIocPrice" => "resolveMarketIocExecutionPrice",
        "validateTpSl" => "validateTpSlPrices",
        "fetch" => return Ok(super::sdk::fetch_function(rt)),
        "signMessage" => "sdkAccount.account.signMessage",
        "loadStarknet" => return Ok(Js::function(n, |_, _| Ok(Js::Undefined))),
        "signTypedData" => return Ok(super::sdk::typed_signer(rt)),
        "jsonResponse" => {
            return Ok(Js::function(n, |_, a| {
                arg(&a, 0).call_method("json", vec![])
            }));
        }
        "textResponse" => {
            return Ok(Js::function(n, |_, a| {
                arg(&a, 0).call_method("text", vec![])
            }));
        }
        "awaitSDK" => return Ok(Js::function(n, |_, a| Ok(arg(&a, 0)))),
        "super"
        | "createAPI"
        | "rateLimit"
        | "newPublicWebSocket"
        | "newPrivateWebSocket"
        | "initializeSDKPromise" => return Ok(callback(rt, n, vec![])),
        other => other,
    };
    if let Some((owner, key)) = target.rsplit_once('.') {
        bound_member(&rt.path(owner)?, key)
    } else {
        bound_member(&rt.this, target)
    }
}
async fn dispatch(rt: Rt, n: &str, a: Vec<Js>, c: Vec<Js>) -> Result<Js, Thrown> {
    if n == "ohlcv-poll" {
        return crate::runtime_assembly::poll_ohlcv(
            &Host::new(&rt),
            rt.this.clone(),
            arg(&c, 0),
            arg(&c, 1),
            arg(&c, 2),
            arg(&c, 3),
            callback(&rt, n, c.clone()),
        )
        .await;
    }
    if let Some(value) = super::sockets::asynchronous(&rt, n).await? {
        return Ok(value);
    }
    let h = Host::new(&rt);
    let x = arg(&a, 0);
    match n {
        "start" => crate::market_data::start(&h).await,
        "tick" => crate::market_data::tick(&h).await,
        "fetchMarkets" => crate::market_data::fetch_markets(&h).await,
        "fetchTickers" => crate::market_data::fetch_tickers(&h).await,
        "fetchPositions" => crate::market_data::fetch_positions(&h).await,
        "fetchOrderBook" => crate::market_data::fetch_order_book(&h, x, arg(&a, 1)).await,
        "fetchOHLCV" => crate::market_data::fetch_ohlcv(&h, x).await,
        "fetchHistoricalFills" => crate::account_rest::fetch_historical_fills(&h, x).await,
        "fetchHistoricalOrders" => crate::account_rest::fetch_historical_orders(&h, x).await,
        "fetchIncomeHistory" => crate::account_rest::fetch_income_history(&h, x).await,
        "fetchIncomeHistoryRange" => {
            let from = x.to_number()?;
            let to = arg(&a, 1).to_number()?;
            let kind: Option<String> = to_typed(&arg(&a, 2))?;
            let records = crate::history_reads::fetch_income_history_range(
                from,
                to,
                kind.as_deref(),
                |opts| {
                    let promise = rt
                        .this
                        .call_method("fetchIncomeHistory", vec![from_typed(&opts)?])?;
                    Ok::<_, Thrown>(async move { to_typed(&await_js(promise).await?) })
                },
            )
            .await?;
            from_typed(&records)
        }
        "fetchHistoricalPositions" => {
            let venue = rt
                .shared
                .sockets
                .venue
                .borrow()
                .as_ref()
                .and_then(std::rc::Weak::upgrade)
                .ok_or_else(|| Thrown::error("Error", "Paradex exchange was dropped"))?;
            let rows = venue
                .fetch_historical_positions(safe_cex_exchange::journal::HistoricalPositionOptions {
                    account: to_typed(&x.get("account")?)?,
                    from: to_typed(&x.get("from")?)?,
                    to: to_typed(&x.get("to")?)?,
                })
                .await
                .map_err(Thrown::from)?;
            from_typed(&rows)
        }

        "setLeverage" => crate::account_rest::set_leverage(&h, x, arg(&a, 1)).await,
        "setMaxSlippage" => crate::account_rest::set_max_slippage(&h, x, arg(&a, 1)).await,
        "buildMainOrderPayload" => {
            crate::order_execution::build_main_order_payload(&h, x, arg(&a, 1)).await
        }
        "resolveMarketIocExecutionPrice" => {
            crate::order_execution::resolve_market_ioc_price(&h, x).await
        }
        "placeNativeBatchChunk" => crate::order_execution::place_native_batch_chunk(&h, x).await,
        "placeTakeProfitStopLoss" => {
            crate::order_execution::place_take_profit_stop_loss(&h, x).await
        }
        "updateOrder" => crate::order_execution::update_order(&h, x).await,
        "placeOrder" => Ok(Js::array(crate::order_commands::place_order(&h, x).await?)),
        "placeOrders" => Ok(Js::array(crate::order_commands::place_orders(&h, x).await?)),
        "placeMainOrder" => crate::order_commands::place_main_order(&h, x).await,
        "cancelOrder" => {
            crate::order_commands::cancel_order(&h, x).await?;
            Ok(Js::Undefined)
        }
        "cancelOrders" => {
            crate::order_commands::cancel_orders(&h, x).await?;
            Ok(Js::Undefined)
        }
        "cancelNativeBatchChunk" => {
            crate::order_commands::cancel_native_batch_chunk(&h, x).await?;
            Ok(Js::Undefined)
        }
        "cancelAllOrders" => {
            crate::order_commands::cancel_all_orders(&h).await?;
            Ok(Js::Undefined)
        }
        "cancelSymbolOrders" => {
            crate::order_commands::cancel_symbol_orders(&h, x).await?;
            Ok(Js::Undefined)
        }
        "fetchBalance" => balance(&rt).await,
        "fetchOrders" => orders(&rt).await,
        "fetchLeverageSettings" => leverage(&rt).await,
        "generateNewToken" => crate::account_rest::generate_new_token(&h).await,
        "api-request" => crate::runtime_assembly::api_request(&h, rt.this.clone(), x).await,
        "initializeSDK" => {
            crate::auth_initialization::initialize_exchange_sdk(&h).await?;
            Ok(Js::Undefined)
        }
        "validateAccount" => crate::auth_initialization::validate_account(&h).await,
        "ensurePrivateHistoryAuth" => {
            crate::auth_initialization::ensure_private_history_auth(&h, &x.to_js_string()?).await?;
            Ok(Js::Undefined)
        }
        "getPositionMode" => from_typed(&crate::account::get_position_mode().await),
        "getAccount" => {
            let account = crate::account::get_account(
                |endpoint| {
                    let value = rt
                        .this
                        .get("xhr")?
                        .call_method("get", vec![Js::str(endpoint)])?;
                    Ok::<_, Thrown>(async move {
                        let data = await_js(value).await?.get("data")?;
                        to_typed(&data)
                    })
                },
                |e| text(&e.value),
                |s, l| h.log(h.logger()?, s.encode_utf16().collect(), l),
            )
            .await?;
            from_typed(&account)
        }
        _ => super::sdk::dispatch(&rt, n, a, c).await,
    }
}

fn coercion(e: safe_cex_num::JsCoercionError) -> Thrown {
    Thrown::error(e.name, e.message)
}
fn describe(rt: &Rt, e: &Thrown) -> Result<Option<String>, Thrown> {
    let v = super::primitives::call(rt, "describeError", vec![e.value.clone()])?;
    if v.is_undefined() {
        Ok(None)
    } else {
        Ok(Some(text(&v)?))
    }
}
async fn data(rt: Rt, endpoint: &str) -> Result<Option<serde_json::Value>, Thrown> {
    let result = await_js(
        rt.this
            .get("xhr")?
            .call_method("get", vec![Js::str(endpoint)])?,
    )
    .await?
    .get("data")?;
    if result.is_undefined() {
        Ok(None)
    } else {
        Ok(Some(to_json(&result)?))
    }
}
fn init(
    rt: &Rt,
) -> Result<Option<safe_cex_exchange::js_runtime::promise::JsFuture<'static>>, Thrown> {
    let value = rt.this.get("sdkInitPromise")?;
    Ok(value.truthy().then(|| {
        Box::pin(async move { await_js(value).await })
            as safe_cex_exchange::js_runtime::promise::JsFuture<'static>
    }))
}
async fn balance(rt: &Rt) -> Result<Js, Thrown> {
    let h = Host::new(rt);
    let initialization = init(rt)?.map(|f| async move {
        f.await?;
        Ok::<_, Thrown>(())
    });
    let value = crate::balance::fetch_balance(
        initialization,
        || rt.this.get("jwtManager").is_ok_and(|v| v.truthy()),
        |e| Ok::<_, Thrown>(data(rt.clone(), e)),
        &mut crate::balance::BalanceEffects {
            log: |s: String, l| h.log(h.logger()?, s.encode_utf16().collect(), l),
            stringify_error: |e: &Thrown| text(&e.value),
            describe_error: |e: &Thrown| describe(rt, e),
            missing_data_error: || Thrown::error("Error", "Balance API returned no data"),
            coercion_error: coercion,
        },
    )
    .await?;
    from_typed(&value)
}
async fn orders(rt: &Rt) -> Result<Js, Thrown> {
    let h = Host::new(rt);
    let initialization = init(rt)?.map(|f| async move {
        f.await?;
        Ok::<_, Thrown>(())
    });
    let value = crate::order_reads::fetch_orders(
        initialization,
        || rt.this.get("jwtManager").is_ok_and(|v| v.truthy()),
        |endpoint| {
            let rt = rt.clone();
            Ok::<_, Thrown>(async move {
                let response = await_js(
                    rt.this
                        .get("xhr")?
                        .call_method("get", vec![Js::str(endpoint)])?,
                )
                .await?
                .get("data")?;
                if response.is_undefined() {
                    Ok(None)
                } else {
                    Ok(Some(to_retained_input(&response)?))
                }
            })
        },
        |row| match crate::order_mapper::map_order_row(row, |ms| {
            (rt.shared.extra.deps.local_wall_time_to_utc)(ms)
        })
        .map_err(|e| Thrown::error("Error", &e))?
        {
            Ok(order) => Ok(Some(order)),
            Err(refusal) => {
                let received = refusal
                    .value
                    .as_ref()
                    .map(safe_cex_num::js_retained_json_stringify)
                    .transpose()
                    .map_err(coercion)?
                    .flatten()
                    .unwrap_or_else(|| "undefined".into());
                h.log(
                    h.logger()?,
                    format!(
                        "Paradex order row refused: Order.{} requires a string; received: {}",
                        refusal.field, received
                    )
                    .encode_utf16()
                    .collect(),
                    "warning",
                )?;
                Ok(None)
            }
        },
        &mut crate::order_reads::OrderReadEffects {
            log: |s: String, l| h.log(h.logger()?, s.encode_utf16().collect(), l),
            stringify_error: |e: &Thrown| text(&e.value),
            describe_error: |e: &Thrown| describe(rt, e),
            coercion_error: coercion,
        },
    )
    .await?;
    from_typed(&value.into_iter().flatten().collect::<Vec<_>>())
}
async fn leverage(rt: &Rt) -> Result<Js, Thrown> {
    let h = Host::new(rt);
    crate::leverage_reads::fetch_leverage_settings(
        |e| {
            let rt = rt.clone();
            Ok::<_, Thrown>(async move {
                let value = data(rt, e).await?;
                value
                    .map(|v| {
                        serde_json::from_value::<crate::types::MarginResponse>(v)
                            .map_err(|e| Thrown::type_error(&e.to_string()))
                    })
                    .transpose()
            })
        },
        |s, l| {
            rt.this
                .get("leverageSettings")?
                .call_method("set", vec![Js::str(s), Js::Number(l)])?;
            Ok(())
        },
        |e| text(&e.value),
        |s: String, l| h.log(h.logger()?, s.encode_utf16().collect(), l),
    )
    .await?;
    Ok(Js::Undefined)
}
