//! The JS BaseExchange instance a venue's assembled class extends: TS base
//! fields plus members bridged to the Rust core BaseExchange.
use super::{
    client::JsRestAdapter,
    promise::Promise,
    sockets::JsSocketContext,
    value::{Getter, Js, Thrown, arg, from_typed, to_json, to_typed},
};
use crate::{
    error_reporting::ExchangeErrorInput,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    external::ExternalPublicData,
    runtime::{ExchangeRuntime, ListenerCleanup, RemoteSigner},
};
use safe_cex_store::Store;
use safe_cex_transport::signing::RemoteSignerRequest;
use safe_cex_transport::{
    RestAdapter, TransportError, TransportProvider,
    secondary::{RateLimitWrapper, create_api_with_secondary_key},
};
use safe_cex_types::{
    Candle, ExchangeName, ExchangeOptions, ExchangeOptionsExternalPublicData, Market, OHLCVOptions,
    OrderBook, PlaceOrderOpts,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::{Rc, Weak},
    sync::Arc,
};

/// Builds the `options.externalPublicData` provider for the exchange whose base it
/// is given (the provider writes that exchange's Store); the host supplies it.
pub type ExternalPublicDataFactory =
    Rc<dyn Fn(&ExchangeOptionsExternalPublicData, Rc<BaseExchange>) -> Rc<dyn ExternalPublicData>>;

/// Typed inputs of the Rust core, distinct from the JS-shaped constructor arguments.
pub struct BaseExchangeParts {
    pub name: ExchangeName,
    pub options: ExchangeOptions,
    pub store: Rc<dyn Store>,
    pub settings: ExchangeSettings,
    /// Required when `options.externalPublicData` is set.
    pub external_public_data: Option<ExternalPublicDataFactory>,
}

/// exchange-server's browser relay for one account, with that account's name.
pub type HostedSigner = (Rc<dyn RemoteSigner>, Option<String>);

/// ts: options.remoteSigner, `{ accountName, sign(request) }` over the hosted signer.
fn js_remote_signer(
    signer: Rc<dyn RemoteSigner>,
    account: Option<String>,
    runtime: Rc<dyn ExchangeRuntime>,
) -> Js {
    let sign = Js::function("sign", move |_, a| {
        let request: RemoteSignerRequest = serde_json::from_value(to_json(&arg(&a, 0))?)
            .map_err(|e| Thrown::type_error(&e.to_string()))?;
        let signer = signer.clone();
        Ok(Js::Promise(Promise::spawn(
            &runtime,
            Box::pin(async move {
                let response = signer
                    .sign(request)
                    .await
                    .map_err(|e| Thrown::error("Error", &e.to_string()))?;
                let value = serde_json::to_value(&response)
                    .map_err(|e| Thrown::type_error(&e.to_string()))?;
                Ok(super::value::from_json(&value))
            }),
        )))
    });
    Js::object(vec![
        (
            "accountName",
            account.map_or(Js::Undefined, |a| Js::str(&a)),
        ),
        ("sign", sign),
    ])
}

fn js_payload_signer(
    signer: Rc<dyn safe_cex_crypto::PayloadSigner>,
    runtime: Rc<dyn ExchangeRuntime>,
) -> Js {
    use safe_cex_crypto::{Encoding, PayloadScheme, PayloadSignerRequest};
    Js::function("payloadSigner", move |_, args| {
        let request = arg(&args, 0);
        let exchange_name = to_typed(&request.get("exchangeName")?)?;
        let key: String = to_typed(&request.get("key")?)?;
        let payload: String = to_typed(&request.get("payload")?)?;
        let scheme: String = to_typed(&request.get("scheme")?)?;
        let encoding: String = to_typed(&request.get("encoding")?)?;
        let scheme = match (scheme.as_str(), encoding.as_str()) {
            ("hmac-sha256", "hex") => PayloadScheme::HmacSha256 {
                encoding: Encoding::Hex,
            },
            ("hmac-sha256", "base64") => PayloadScheme::HmacSha256 {
                encoding: Encoding::Base64,
            },
            ("stark", "felt-pair") => PayloadScheme::Stark,
            _ => {
                return Err(Thrown::type_error(
                    "Unsupported payload signer scheme/encoding",
                ));
            }
        };
        let signer = signer.clone();
        Ok(Js::Promise(Promise::spawn(
            &runtime,
            Box::pin(async move {
                let result = signer
                    .sign(PayloadSignerRequest {
                        exchange_name,
                        key: &key,
                        payload: &payload,
                        scheme,
                    })
                    .await
                    .map_err(|error| Thrown::error("Error", &error))?;
                let output = Js::object(vec![("signature", Js::str(&result.signature))]);
                if let Some(headers) = result.headers {
                    output.define("headers", from_typed(&headers)?)?;
                }
                Ok(output)
            }),
        )))
    })
}

fn assembled_venue(context: &Weak<JsSocketContext>) -> Result<Rc<dyn Exchange>, Thrown> {
    context
        .upgrade()
        .and_then(|c| c.venue.borrow().as_ref().and_then(Weak::upgrade))
        .ok_or_else(|| Thrown::error("Error", "exchange is not assembled"))
}

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

/// Initializes source identity fields before publishing the immutable typed base.
/// The callback runs on the receiver before base methods or external providers
/// exist; it must only initialize fields, without starting work or retaining it.
/// Later mutable receiver writes do not alter these construction-time flags.
pub fn create(
    context: &Rc<JsSocketContext>,
    parts: BaseExchangeParts,
    name: Js,
    options: Js,
    store: Js,
    initialize_fields: impl FnOnce(&Js) -> Result<(), Thrown>,
) -> Result<(Js, Rc<BaseExchange>), Thrown> {
    let runtime: Rc<dyn ExchangeRuntime> = context.runtime.clone();
    if parts.options.external_public_data.is_some() && parts.external_public_data.is_none() {
        return Err(Thrown::error(
            "Error",
            "externalPublicData is configured but no external public data provider was supplied",
        ));
    }
    let external_public_data = parts.external_public_data;
    let mut core = BaseExchange::new(
        parts.name,
        parts.options,
        parts.store,
        runtime.clone(),
        parts.settings,
    );
    let remote = context
        .http
        .borrow()
        .as_ref()
        .map(|http| -> Result<Option<HostedSigner>, Thrown> {
            http.configure_exchange(core.name.clone())?;
            Ok(http.hosted_signer())
        })
        .transpose()?
        .flatten();
    if let Some((signer, account)) = remote {
        core.signers.remote_signer = Some(signer.clone());
        options.set(
            "remoteSigner",
            js_remote_signer(signer, account, runtime.clone()),
        )?;
    }
    let payload = {
        let host = context.http.borrow().clone();
        host.and_then(|host| host.payload_signer())
    };
    if let Some(signer) = payload {
        core.signers.payload_signer = Some(signer.clone());
        options.set("payloadSigner", js_payload_signer(signer, runtime.clone()))?;
    }
    let this = Js::object_with_class(
        "BaseExchange",
        vec![
            ("name", name.clone()),
            ("_exchangeName", name),
            ("options", options),
            ("store", store),
            ("isSpot", Js::Bool(false)),
            ("isDemo", Js::Bool(false)),
        ],
    );
    initialize_fields(&this)?;
    core.is_spot = this.get("isSpot")?.truthy();
    core.is_demo = this.get("isDemo")?.truthy();
    let base = Rc::new(core);
    let def = |k: &str, v: Js| {
        let _ = this.define(k, v);
    };
    {
        let getter = base.clone();
        let setter = base.clone();
        this.define_accessor(
            "isDisposed",
            Some(Rc::new(move |_| Ok(Js::Bool(getter.is_disposed())))),
            Some(Rc::new(move |_, value| {
                setter.set_disposed_flag(value.truthy());
                Ok(())
            })),
        )?;
    }
    {
        // Dispatches through the venue's privacy hook, as the source getter does.
        let context = Rc::downgrade(context);
        let getter: Getter = Rc::new(move |_| {
            let venue = context
                .upgrade()
                .and_then(|c| c.venue.borrow().as_ref().and_then(Weak::upgrade))
                .ok_or_else(|| Thrown::error("Error", "exchange is not assembled"))?;
            Ok(Js::Bool(venue.is_public_data_only()))
        });
        this.define_accessor("isPublicDataOnly", Some(getter), None)?;
    }
    {
        let base = base.clone();
        // Listener ids by JS function identity, so `off(event, fn)` finds its row.
        let registered: Rc<std::cell::RefCell<Vec<(String, Js, String)>>> = Rc::default();
        let (on_base, off_base) = (base.clone(), base.clone());
        let (on_rows, off_rows) = (registered.clone(), registered);
        def(
            "emitter",
            Js::object(vec![
                (
                    "emit",
                    Js::function("emit", move |_, a| {
                        let event = text(&arg(&a, 0))?;
                        base.emit_args(&event, a.get(1..).unwrap_or_default())?;
                        Ok(Js::Bool(true))
                    }),
                ),
                (
                    "on",
                    Js::function("on", move |this, a| {
                        let event = text(&arg(&a, 0))?;
                        let listener = arg(&a, 1);
                        let callee = listener.clone();
                        let id = on_base.on_args(
                            &event,
                            Rc::new(move |_, data| {
                                callee.call(Js::Undefined, data.to_vec())?;
                                Ok(())
                            }),
                        );
                        on_rows.borrow_mut().push((event, listener, id));
                        Ok(this)
                    }),
                ),
                (
                    "off",
                    Js::function("off", move |this, a| {
                        let event = text(&arg(&a, 0))?;
                        let listener = arg(&a, 1);
                        let snapshot = off_rows.borrow().clone();
                        let functions: HashMap<_, _> = snapshot
                            .iter()
                            .filter(|(name, _, _)| *name == event)
                            .map(|(_, function, id)| (id.clone(), function.clone()))
                            .collect();
                        let retained: HashSet<_> = off_base
                            .retain_event_listeners(&event, |id| {
                                if !listener.truthy() {
                                    return Ok(false);
                                }
                                let Some(function) = functions.get(id) else {
                                    return Ok(true);
                                };
                                Ok::<_, Thrown>(
                                    !function.strict_equals(&listener)
                                        && !function.get("_")?.strict_equals(&listener),
                                )
                            })?
                            .into_iter()
                            .collect();
                        let displaced =
                            {
                                let mut rows = off_rows.borrow_mut();
                                let (displaced, mut others): (Vec<_>, Vec<_>) =
                                    std::mem::take(&mut *rows)
                                        .into_iter()
                                        .partition(|(name, _, _)| *name == event);
                                others.extend(snapshot.into_iter().filter(|(name, _, id)| {
                                    *name == event && retained.contains(id)
                                }));
                                *rows = others;
                                displaced
                            };
                        drop(displaced);
                        Ok(this)
                    }),
                ),
            ]),
        );
        // ts: base.ts:454 — `this.on/once/off` are the emitter's members bound to it.
        if let Ok(emitter) = this.get("emitter") {
            for name in ["on", "off"] {
                if let Ok(member) = super::scope::bound_member(&emitter, name) {
                    def(name, member);
                }
            }
        }
    }
    {
        let base = base.clone();
        def(
            "log",
            Js::function("log", move |_, a| {
                let severity = arg(&a, 1);
                let severity = if severity.is_undefined() {
                    "info".to_owned()
                } else {
                    text(&severity)?
                };
                base.log_value(&arg(&a, 0), &severity)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "emitAndLogError",
            Js::function("emitAndLogError", move |_, a| {
                let error = arg(&a, 0);
                let input = ExchangeErrorInput::Raw(error);
                let msg = arg(&a, 1);
                let msg = if msg.is_undefined() {
                    None
                } else {
                    Some(text(&msg)?)
                };
                let extra = arg(&a, 2);
                let extra = if extra.is_undefined() {
                    None
                } else {
                    Some(crate::js_runtime::value::to_retained_input(&extra)?)
                };
                base.emit_and_log_error(input, msg.as_deref(), extra.as_ref())?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let receiver = this.clone();
        let ws_runtime = context.runtime.clone();
        let runtime = runtime.clone();
        def(
            "createXhrWithSecondaryKey",
            Js::function("createXhrWithSecondaryKey", move |_, a| {
                let (xhr, primary, secondary) = create_xhr_with_secondary_key(
                    &runtime,
                    ws_runtime.transport().as_ref(),
                    &receiver,
                    &a,
                )?;
                receiver.set("_primaryXhr", primary)?;
                receiver.set("_secondaryXhr", secondary)?;
                Ok(xhr)
            }),
        );
    }
    {
        let context = Rc::downgrade(context);
        let runtime = runtime.clone();
        def(
            "fetchAndSetPrivateSnapshot",
            Js::function("fetchAndSetPrivateSnapshot", move |_, a| {
                let context = context.clone();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        let venue = assembled_venue(&context)?;
                        if venue.is_public_data_only() || venue.base().is_disposed() {
                            return Ok(Js::Undefined);
                        }
                        let tries = arg(&a, 0);
                        // TS exits before coercion; `count < tries` rounds fractions up.
                        let tries = if tries.is_undefined() {
                            1.0
                        } else {
                            tries.to_number()?
                        };
                        let tries = if tries > 0.0 {
                            tries.ceil().min(f64::from(u32::MAX)) as u32
                        } else {
                            0
                        };
                        venue.fetch_and_set_private_snapshot(tries).await?;
                        Ok(Js::Undefined)
                    }),
                )))
            }),
        );
    }
    {
        let context = Rc::downgrade(context);
        let runtime = runtime.clone();
        def(
            "fetchAndSetOrders",
            Js::function("fetchAndSetOrders", move |_, a| {
                let context = context.clone();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        let venue = assembled_venue(&context)?;
                        let symbols = arg(&a, 0);
                        let symbols: Option<Vec<String>> = if symbols.truthy() {
                            Some(to_typed(&symbols)?)
                        } else {
                            None
                        };
                        let orders = venue.fetch_and_set_orders(symbols).await?;
                        from_typed(&orders)
                    }),
                )))
            }),
        );
    }
    {
        let base = base.clone();
        let runtime = runtime.clone();
        // One JS object per live provider, so repeated reads keep the field's identity.
        let held: Rc<RefCell<Option<(Rc<dyn ExternalPublicData>, Js)>>> = Rc::default();
        this.define_accessor(
            "externalPublicDataProvider",
            Some(Rc::new(move |_| {
                let Some(provider) = base.external_public_data_provider.borrow().clone() else {
                    return Ok(Js::Undefined);
                };
                if let Some((current, object)) = &*held.borrow()
                    && Rc::ptr_eq(current, &provider)
                {
                    return Ok(object.clone());
                }
                let object = external_provider_object(provider.clone(), runtime.clone());
                *held.borrow_mut() = Some((provider, object.clone()));
                Ok(object)
            })),
            None,
        )?;
    }
    {
        let base = base.clone();
        def(
            "dispose",
            Js::function("dispose", move |_, _| {
                base.dispose()?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "assertSingleNativeClientOrderId",
            Js::function("assertSingleNativeClientOrderId", move |_, a| {
                let id = arg(&a, 0);
                let id = if id.is_undefined() {
                    None
                } else {
                    Some(text(&id)?)
                };
                base.assert_single_native_client_order_id(
                    id.as_deref(),
                    arg(&a, 1).to_number()? as usize,
                )?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let base = base.clone();
        let context = Rc::downgrade(context);
        let runtime = runtime.clone();
        def(
            "getOrFetchPositionsForSymbol",
            Js::function("getOrFetchPositionsForSymbol", move |_, a| {
                let context = context.clone();
                let base = base.clone();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        let symbol = text(&arg(&a, 0))?;
                        let venue = assembled_venue(&context)?;
                        let positions = base
                            .get_or_fetch_positions_for_symbol(venue, &symbol)
                            .await?;
                        from_typed(&positions)
                    }),
                )))
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "getTickInterval",
            Js::function("getTickInterval", move |_, _| {
                Ok(base.get_tick_interval().map_or(Js::Undefined, Js::Number))
            }),
        );
    }
    {
        let base = base.clone();
        let context = Rc::downgrade(context);
        this.define_accessor(
            "needsAccountMarketOverlay",
            Some(Rc::new(move |_| {
                let venue = assembled_venue(&context)?;
                Ok(Js::Bool(base.needs_account_market_overlay(&*venue)))
            })),
            None,
        )?;
    }
    {
        let base = base.clone();
        let runtime = runtime.clone();
        def(
            "loadAccountMarketOverlay",
            Js::function("loadAccountMarketOverlay", move |_, a| {
                let load = arg(&a, 0);
                let base = base.clone();
                let reporter = runtime.clone();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        let overlay = Box::pin(async move {
                            let value = super::promise::await_js(
                                &reporter,
                                load.call(Js::Undefined, vec![])?,
                            )
                            .await?;
                            Ok(market_overlay(value, reporter))
                        });
                        base.load_account_market_overlay(overlay).await?;
                        Ok(Js::Undefined)
                    }),
                )))
            }),
        );
    }
    {
        let base = base.clone();
        let runtime = runtime.clone();
        def(
            "initExternalPublicData",
            Js::function("initExternalPublicData", move |_, _| {
                let base = base.clone();
                let owner = base.clone();
                let factory = external_public_data.clone();
                Ok(Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move {
                        let used = base
                            .init_external_public_data(move |config| match factory {
                                Some(factory) => factory(config, owner),
                                None => unreachable!(
                                    "create() refuses externalPublicData without a provider"
                                ),
                            })
                            .await?;
                        Ok(Js::Bool(used))
                    }),
                )))
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "resolveCallerClientOrderId",
            Js::function("resolveCallerClientOrderId", move |_, a| {
                let opts: PlaceOrderOpts = to_typed(&arg(&a, 0))?;
                Ok(base
                    .resolve_caller_client_order_id(&opts)?
                    .map_or(Js::Undefined, |id| Js::str(&id)))
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "assertCallerClientOrderIds",
            Js::function("assertCallerClientOrderIds", move |_, a| {
                let orders: Vec<PlaceOrderOpts> = to_typed(&arg(&a, 0))?;
                base.assert_caller_client_order_ids(&orders)?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let base = base.clone();
        def(
            "deriveAlgoOrdersFromNormalOrdersOpts",
            Js::function("deriveAlgoOrdersFromNormalOrdersOpts", move |_, a| {
                let opts: Vec<PlaceOrderOpts> = to_typed(&arg(&a, 0))?;
                from_typed(&base.derive_algo_orders_from_normal_orders_opts(&opts))
            }),
        );
    }
    // Source BaseExchange.getAllSockets reads these properties on every call.
    // Defer core creation until the venue is assembled and retain weak owners.
    if let Js::Object(receiver) = &this {
        use crate::socket_lifecycle::SocketSlot;
        for (name, slot) in [
            ("privateWebsocket", SocketSlot::Private),
            ("publicWebsocket", SocketSlot::Public),
            ("publicWebsocketSecondary", SocketSlot::PublicSecondary),
            ("publicInverseWebsocket", SocketSlot::PublicInverse),
            ("futuresWebsocket", SocketSlot::Futures),
            ("businessWebsocket", SocketSlot::Business),
            ("secondsTickersWebsocket", SocketSlot::SecondsTickers),
            ("thirdTickersWebsocket", SocketSlot::ThirdTickers),
        ] {
            let receiver = Rc::downgrade(receiver);
            let context = Rc::downgrade(context);
            base.set_socket_lookup(
                slot,
                Rc::new(move || {
                    let context = context.upgrade()?;
                    let receiver = Js::Object(receiver.upgrade()?);
                    let read = || -> Result<Option<Rc<crate::websocket::BaseWebSocket>>, Thrown> {
                        let socket = receiver.get(name)?;
                        super::sockets::core_of(&context, &socket)
                            .map(|core| core.base())
                            .transpose()
                    };
                    read().unwrap_or_else(|error| {
                        context.runtime.report_exception(
                            &error.to_exchange_error(),
                            &serde_json::json!({"operation":"getAllSockets", "slot":name}).into(),
                        );
                        None
                    })
                }),
            );
        }
    }
    {
        let context = Rc::downgrade(context);
        def(
            "callMethodInWorker",
            Js::function("callMethodInWorker", move |_, args| {
                worker_rpc(&context, args)
            }),
        );
    }
    Ok((this, base))
}

/// Keep worker RPC ownership in the core: its exact reply listener and timeout
/// are released by WorkerReplyGuard, and the venue receives the completion hook.
fn worker_rpc(context: &Weak<JsSocketContext>, args: Vec<Js>) -> Result<Js, Thrown> {
    let context = context
        .upgrade()
        .ok_or_else(|| Thrown::error("Error", "exchange is not assembled"))?;
    let runtime: Rc<dyn ExchangeRuntime> = context.runtime.clone();
    Ok(Js::Promise(Promise::spawn(
        &runtime,
        Box::pin(async move {
            let venue = assembled_venue(&Rc::downgrade(&context))?;
            let method = to_typed(&arg(&args, 0))?;
            let args = args
                .iter()
                .skip(1)
                .map(to_json)
                .collect::<Result<Vec<_>, _>>()?;
            let result = match venue
                .base()
                .call_method_in_worker(venue.as_ref(), method, args)
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    // Keep the native metadata and expose the source-reconstructed
                    // stack/code properties to JS callers as well.
                    let (stack, code) = (error.stack.clone(), error.code.clone());
                    let thrown = Thrown::from(error);
                    if let Some(stack) = stack {
                        thrown.value.define("stack", Js::str(&stack))?;
                    }
                    if let Some(code) = code {
                        thrown.value.define("code", code)?;
                    }
                    return Err(thrown);
                }
            };
            Ok(super::value::from_json(&result))
        }),
    )))
}

/// Decode structural options without serializing executable host capabilities.
/// The source object retains these handles; core receives them from its host.
pub fn exchange_options(options: &Js) -> Result<ExchangeOptions, Thrown> {
    let structural = options.spread_object()?;
    structural.delete(&super::value::utf16("remoteSigner"))?;
    structural.delete(&super::value::utf16("payloadSigner"))?;
    to_typed(&structural)
}

type ClientRows = RefCell<Vec<(Arc<dyn RestAdapter>, Js)>>;

/// ts: createApiWithSecondaryKey through the typed `create_api_with_secondary_key`, which
/// decides the split, the secondary_* overrides and the secondary proxy precedence.
/// Returns `[xhr, primaryXhr, secondaryXhr]`; without a secondary key all three are one client.
fn create_xhr_with_secondary_key(
    runtime: &Rc<dyn ExchangeRuntime>,
    transport: &dyn TransportProvider,
    receiver: &Js,
    a: &[Js],
) -> Result<(Js, Js, Js), Thrown> {
    let opts = arg(a, 0);
    let create_api = arg(a, 1);
    let other = arg(a, 2);
    let other_member = |key: &str| {
        if other.is_nullish() {
            Ok(Js::Undefined)
        } else {
            other.get(key)
        }
    };
    let proxy_url = other_member("proxyUrl")?
        .as_str()
        .map(|url| String::from_utf16_lossy(url));
    let rate_limit = other_member("rateLimit")?;
    let secondary_rate_limit = other_member("secondaryRateLimit")?;
    let typed: ExchangeOptions = exchange_options(&opts)?;

    let rows: ClientRows = RefCell::default();
    let failure: RefCell<Option<Thrown>> = RefCell::default();
    let created = std::cell::Cell::new(0_u32);
    let record = |xhr: Js| -> Arc<dyn RestAdapter> {
        // The typed helper takes `Arc` adapters; a JS client stays on its own thread.
        #[expect(clippy::arc_with_non_send_sync)]
        let adapter: Arc<dyn RestAdapter> =
            Arc::new(JsRestAdapter::new(runtime.clone(), xhr.clone()));
        rows.borrow_mut().push((adapter.clone(), xhr));
        adapter
    };
    let client_of = |adapter: &Arc<dyn RestAdapter>| {
        rows.borrow()
            .iter()
            .find(|(row, _)| Arc::ptr_eq(row, adapter))
            .map(|(_, xhr)| xhr.clone())
    };
    let fail = |thrown: Thrown| {
        let message = thrown.message();
        failure.borrow_mut().get_or_insert(thrown);
        TransportError::Host(message)
    };
    let create = |options: &ExchangeOptions| {
        let call = created.get();
        created.set(call + 1);
        let options = if call == 0 {
            opts.clone()
        } else {
            secondary_options(&opts, options.override_proxy_url.as_deref()).map_err(fail)?
        };
        let xhr = create_api
            .call(Js::Undefined, vec![options, receiver.clone()])
            .map_err(fail)?;
        Ok(record(xhr))
    };
    let limiter = |limit: Js| {
        move |adapter: Arc<dyn RestAdapter>| -> Arc<dyn RestAdapter> {
            let Some(xhr) = client_of(&adapter) else {
                return adapter;
            };
            match limit.call(Js::Undefined, vec![xhr]) {
                Ok(limited) => record(limited),
                Err(thrown) => {
                    fail(thrown);
                    adapter
                }
            }
        }
    };
    let primary_limit = rate_limit.truthy().then(|| limiter(rate_limit.clone()));
    let secondary_limit = secondary_rate_limit
        .truthy()
        .then(|| limiter(secondary_rate_limit.clone()));
    let clients = create_api_with_secondary_key(
        &typed,
        transport,
        proxy_url.as_deref(),
        create,
        primary_limit.as_ref().map(|f| f as &RateLimitWrapper<'_>),
        secondary_limit.as_ref().map(|f| f as &RateLimitWrapper<'_>),
    );
    if let Some(thrown) = failure.borrow_mut().take() {
        return Err(thrown);
    }
    let clients = clients.map_err(|error| Thrown::error("Error", &error.to_string()))?;
    let primary = client_of(&clients.primary)
        .ok_or_else(|| Thrown::error("Error", "createApi returned no primary client"))?;
    if clients.shares_primary_adapter() {
        return Ok((primary.clone(), primary.clone(), primary));
    }
    // The secondary client is the last one built: created after the primary, then rate limited.
    let secondary = rows
        .borrow()
        .last()
        .map(|(_, xhr)| xhr.clone())
        .ok_or_else(|| Thrown::error("Error", "createApi returned no secondary client"))?;
    Ok((reads_on(&primary, &secondary), primary, secondary))
}

/// ts: `{ ...options, ...secondary_* overrides, overrideProxyUrl }` over the JS options object.
fn secondary_options(opts: &Js, proxy: Option<&str>) -> Result<Js, Thrown> {
    let prefix = super::value::utf16("secondary_");
    let out = opts.spread_object()?;
    for (key, value) in opts.own_entries()? {
        if let Some(stripped) = key.strip_prefix(prefix.as_ref()) {
            out.define_key(stripped, value)?;
        }
    }
    out.define("overrideProxyUrl", proxy.map_or(Js::Undefined, Js::str))?;
    Ok(out)
}

/// ts: the get-trapping Proxy over the primary client. Every other member is read through
/// the prototype link to the primary; `get(url, config)` goes to the secondary client.
fn reads_on(primary: &Js, secondary: &Js) -> Js {
    let secondary = secondary.clone();
    let routed = Js::object_with_class(
        "Axios",
        vec![(
            "get",
            Js::function("get", move |_, a| {
                secondary.call_method("get", vec![arg(&a, 0), arg(&a, 1)])
            }),
        )],
    );
    if let Js::Object(object) = &routed {
        *object.proto.borrow_mut() = Some(primary.clone());
    }
    routed
}

/// The `externalPublicDataProvider` members venue bodies call, bound to the Rust provider.
/// A candle or book the JS callback throws on is reported, as a provider listener would be.
fn external_provider_object(
    provider: Rc<dyn ExternalPublicData>,
    runtime: Rc<dyn ExchangeRuntime>,
) -> Js {
    fn deliver<T: serde::Serialize>(
        callback: &Js,
        value: &T,
        runtime: &Rc<dyn ExchangeRuntime>,
        source: &str,
    ) {
        let delivered = from_typed(value).and_then(|v| callback.call(Js::Undefined, vec![v]));
        if let Err(error) = delivered {
            runtime.report_exception(
                &error.to_exchange_error(),
                &serde_json::json!({ "source": source }).into(),
            );
        }
    }
    fn unsubscribe(cleanup: ListenerCleanup) -> Js {
        let slot = RefCell::new(Some(cleanup));
        Js::function("unsubscribe", move |_, _| {
            let cleanup = slot.borrow_mut().take();
            if let Some(cleanup) = cleanup {
                cleanup()?;
            }
            Ok(Js::Undefined)
        })
    }
    let (candles, books) = (provider.clone(), provider);
    let (candle_runtime, book_runtime) = (runtime.clone(), runtime);
    Js::object_with_class(
        "ExternalPublicDataProvider",
        vec![
            (
                "subscribeCandle",
                Js::function("subscribeCandle", move |_, a| {
                    let options: OHLCVOptions = to_typed(&arg(&a, 0))?;
                    let (callback, runtime) = (arg(&a, 1), candle_runtime.clone());
                    let cleanup = candles.listen_ohlcv(
                        options,
                        Rc::new(move |candle: Candle| {
                            deliver(&callback, &candle, &runtime, "subscribeCandle")
                        }),
                    )?;
                    Ok(unsubscribe(cleanup))
                }),
            ),
            (
                "subscribeOrderbook",
                Js::function("subscribeOrderbook", move |_, a| {
                    let symbol = text(&arg(&a, 0))?;
                    let (callback, runtime) = (arg(&a, 1), book_runtime.clone());
                    let cleanup = books.listen_order_book(
                        symbol,
                        Rc::new(move |book: OrderBook| {
                            deliver(&callback, &book, &runtime, "subscribeOrderbook")
                        }),
                    )?;
                    Ok(unsubscribe(cleanup))
                }),
            ),
        ],
    )
}

/// A JS `(markets) => markets` overlay as the core's typed overlay; null is none.
/// A throwing or malformed overlay is reported and leaves the markets unchanged.
fn market_overlay(
    value: Js,
    reporter: Rc<dyn ExchangeRuntime>,
) -> Option<crate::snapshots::AccountMarketOverlay> {
    if value.is_nullish() {
        return None;
    }
    Some(Rc::new(move |markets: Vec<Market>| {
        let mapped = from_typed(&markets)
            .and_then(|m| value.call(Js::Undefined, vec![m]))
            .and_then(|m| to_typed::<Vec<Market>>(&m));
        match mapped {
            Ok(mapped) => mapped,
            Err(error) => {
                reporter.report_exception(
                    &error.to_exchange_error(),
                    &serde_json::json!({"source": "accountMarketOverlay"}).into(),
                );
                markets
            }
        }
    }))
}
