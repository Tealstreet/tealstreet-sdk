//! ClassHost plus the dispatch from each manifest target to its ported Rust body,
//! adapted from the receiver's JS-shaped arguments.
use super::{
    host::h,
    host_typed::call_future,
    promise::{Promise, await_js},
    rt::Rt,
    sockets::{SocketHost, host_of},
    value::{Js, Thrown, from_json, from_typed, to_json, to_typed},
};
use crate::{
    classes::{BodyFuture, BoundMember, ClassHost, Kind, NativeMember, Return},
    rest_account::{Handle, ReadResult},
};
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_num::{JsJsonValue, numeric_ingress::JsNumberInput};
use std::{error::Error, future::Future, rc::Rc};

fn ex(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

fn value(v: Js) -> ReadResult<Rt, Return<Rt>> {
    Ok(Return::Value(h(v)))
}

fn future(body: impl Future<Output = Result<Js, Thrown>> + 'static) -> ReadResult<Rt, Return<Rt>> {
    let body: BodyFuture<Rt> = Box::pin(async move { body.await.map(h) });
    Ok(Return::Future(body))
}

/// A Js value as numeric-ingress input (`None` for undefined response data).
pub(crate) fn number_input(v: &Js) -> Result<Option<JsNumberInput>, Thrown> {
    if v.is_undefined() {
        return Ok(None);
    }
    safe_cex_exchange::js_runtime::value::to_retained_input(v)
        .map(|input| Some(JsNumberInput::from(input)))
}

pub(crate) fn json_value(v: &JsJsonValue) -> Js {
    match v {
        JsJsonValue::Undefined => Js::Undefined,
        JsJsonValue::Null => Js::Null,
        JsJsonValue::Bool(b) => Js::Bool(*b),
        JsJsonValue::Number(n) => Js::Number(*n),
        JsJsonValue::String(s) => Js::str(s),
        JsJsonValue::BigInt(n) => Js::BigInt(Rc::new(n.clone())),
        JsJsonValue::Array(items) => Js::array(items.iter().map(json_value).collect()),
        JsJsonValue::Object(fields) => {
            let out = Js::object(vec![]);
            for (k, v) in fields {
                let _ = out.define(k, json_value(v));
            }
            out
        }
    }
}

fn js_json(v: &Js) -> Result<JsJsonValue, Thrown> {
    Ok(match v {
        Js::Undefined => JsJsonValue::Undefined,
        other => JsJsonValue::from(to_json(other)?),
    })
}

/// `const { data } = await client.verb(path, config?)` as numeric-ingress data.
fn get_data(
    rt: &Rt,
    client: &str,
    verb: &str,
    args: Vec<Js>,
) -> Result<impl Future<Output = Result<Option<JsNumberInput>, ExchangeError>> + use<>, ExchangeError>
{
    let promise = rt
        .this
        .get(client)
        .and_then(|c| c.call_method(verb, args))
        .map_err(ex)?;
    Ok(async move {
        let response = await_js(promise).await.map_err(ex)?;
        number_input(&response.get("data").map_err(ex)?).map_err(ex)
    })
}

fn boxed(e: ExchangeError) -> Box<dyn Error> {
    Box::new(e)
}

fn log_with(
    rt: &Rt,
    owner: Js,
    message: String,
    severity: LogSeverity,
) -> Result<(), ExchangeError> {
    let severity = serde_json::to_value(severity)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "info".into());
    let _ = rt;
    owner
        .call_method("log", vec![Js::str(&message), Js::str(&severity)])
        .map_err(ex)?;
    Ok(())
}

fn options_text(rt: &Rt, key: &str) -> Result<Option<String>, ExchangeError> {
    let v = rt
        .this
        .get("parent")
        .and_then(|p| p.get("options"))
        .and_then(|o| o.get(key))
        .map_err(ex)?;
    Ok(if v.is_nullish() {
        None
    } else {
        Some(v.to_display())
    })
}

fn send_text(this: &Js) -> impl FnMut(String) -> Result<(), ExchangeError> + 'static {
    let this = this.clone();
    move |text| {
        this.call_method("send", vec![Js::str(&text)]).map_err(ex)?;
        Ok(())
    }
}

fn socket(rt: &Rt) -> Result<Rc<SocketHost>, Thrown> {
    host_of(rt)
}

fn lengths(handlers: &Js) -> Vec<Result<Option<usize>, ExchangeError>> {
    handlers
        .own_keys()
        .into_iter()
        .map(|k| {
            let list = handlers.get_key(&k).map_err(ex)?;
            if list.is_nullish() {
                return Ok(None);
            }
            Ok(list.get("length").map_err(ex)?.as_f64().map(|n| n as usize))
        })
        .collect()
}

/// The listener-dispose arrow a `listen*` member returns.
fn disposer(dispose: impl Fn() -> Result<(), ExchangeError> + 'static) -> Js {
    Js::function("dispose", move |_, _| {
        dispose().map_err(Thrown::from)?;
        Ok(Js::Undefined)
    })
}

fn public_socket(rt: &Rt) -> Result<Js, ExchangeError> {
    rt.this.get("publicWebsocket").map_err(ex)
}

fn create_public(rt: &Rt) -> Result<Js, ExchangeError> {
    let class = match rt.shared.extra.venue {
        crate::construction::Venue::Futures => "KrakenPublicWebsocket",
        crate::construction::Venue::Spot => "KrakenSpotPublicWebsocket",
    };
    rt.global(class)
        .and_then(|c| c.call(Js::Undefined, vec![rt.this.clone()]))
        .map_err(ex)
}

/// `if (!this.publicWebsocket) { create; connect }` then `this.publicWebsocket.member(...)`.
fn delegate_public(rt: &Rt, member: &str, args: Vec<Js>) -> Result<Js, ExchangeError> {
    if public_socket(rt)?.is_nullish() {
        let socket = create_public(rt)?;
        rt.this.set("publicWebsocket", socket.clone()).map_err(ex)?;
        socket
            .call_method("connectAndSubscribe", vec![])
            .map_err(ex)?;
    }
    public_socket(rt)?.call_method(member, args).map_err(ex)
}

pub(crate) fn dispatch(
    rt: Rt,
    target: &'static str,
    args: Vec<Handle<Rt>>,
) -> ReadResult<Rt, Return<Rt>> {
    use crate::*;
    let name = target.rsplit("::").next().unwrap_or(target);
    let file = target
        .split("/src/")
        .nth(1)
        .and_then(|p| p.split(".rs").next())
        .unwrap_or_default();
    let a0 = args.first().cloned().unwrap_or_else(|| h(Js::Undefined));
    let a1 = args.get(1).cloned().unwrap_or_else(|| h(Js::Undefined));
    let a2 = args.get(2).cloned().unwrap_or_else(|| h(Js::Undefined));
    match (file, name) {
        // ---- REST read bodies over the ReadHost family ----
        ("rest_account", "futures_markets") => {
            future(async move { Ok((*rest_account::futures_markets(&rt).await?).clone()) })
        }
        ("rest_account", "spot_markets") => {
            future(async move { Ok((*rest_account::spot_markets(&rt).await?).clone()) })
        }
        ("rest_account", "spot_tickers") => {
            future(async move { Ok((*rest_account::spot_tickers(&rt).await?).clone()) })
        }
        ("rest_account", "spot_orders") => {
            future(async move { Ok((*rest_account::spot_orders(&rt).await?).clone()) })
        }
        ("rest_account", "futures_positions") => {
            future(async move { Ok((*rest_account::futures_positions(&rt).await?).clone()) })
        }
        ("rest_account", "futures_balance") => {
            future(async move { Ok((*rest_account::futures_balance(&rt).await?).clone()) })
        }
        ("rest_account", "spot_refresh_balance") => future(async move {
            rest_account::spot_refresh_balance(&rt).await?;
            Ok(Js::Undefined)
        }),
        ("rest_account", "futures_fills") => future(async move {
            let opts = if a0.is_undefined() {
                h(Js::object(vec![]))
            } else {
                a0
            };
            Ok((*rest_account::futures_fills(&rt, &opts).await?).clone())
        }),
        // ---- trading ----
        ("trading", "futures_place") => {
            future(async move { Ok((*trading::futures_place(&rt, a0).await?).clone()) })
        }
        ("trading", "spot_place") => {
            future(async move { Ok((*trading::spot_place(&rt, a0).await?).clone()) })
        }
        ("trading", "futures_update") => {
            future(async move { Ok((*trading::futures_update(&rt, &a0).await?).clone()) })
        }
        ("trading", "spot_update") => {
            future(async move { Ok((*trading::spot_update(&rt, &a0).await?).clone()) })
        }
        ("trading", "futures_cancel") => future(async move {
            trading::futures_cancel(&rt, &a0).await?;
            Ok(Js::Undefined)
        }),
        ("trading", "futures_set_leverage") => future(async move {
            trading::futures_set_leverage(&rt, a0, &a1).await?;
            Ok(Js::Undefined)
        }),
        // ts: changeMarginType(isolated, symbol).
        ("trading", "futures_margin") => future(async move {
            trading::futures_margin(&rt, &a0, a1).await?;
            Ok(Js::Undefined)
        }),
        ("spot_cancel", "cancel_orders") => future(async move {
            spot_cancel::cancel_orders(&rt, a0)
                .await
                .map_err(Thrown::from)?;
            Ok(Js::Undefined)
        }),
        // ---- typed read bodies ----
        ("tickers", "fetch_tickers") => future(async move {
            let store = rt.shared.store.clone();
            let tickers = tickers::fetch_tickers(
                store.as_ref(),
                |endpoint| get_data(&rt, "xhr", "get", vec![Js::str(endpoint)]),
                |error| {
                    rt.shared.runtime.console("trace", &error.message);
                    Ok(())
                },
            )
            .await
            .map_err(Thrown::from)?;
            from_typed(&tickers)
        }),
        ("orders", "fetch_orders") => future(async move {
            let store = rt.shared.store.clone();
            let clock = rt.clone();
            let orders = orders::fetch_orders(
                store.as_ref(),
                |endpoint| get_data(&rt, "xhr", "get", vec![Js::str(endpoint)]),
                || Ok(clock.now()),
                |n| n,
                |error| {
                    rt.shared.runtime.console("trace", &error.message);
                    Ok(())
                },
            )
            .await
            .map_err(Thrown::from)?;
            from_typed(&orders)
        }),
        ("futures_candles", "fetch_ohlcv") => future(async move {
            let options = number_input(&a0)?
                .ok_or_else(|| Thrown::type_error("OHLCV options are required"))?;
            let testnet = rt
                .this
                .get("options")
                .and_then(|o| o.get("testnet"))
                .is_ok_and(|v| v.truthy());
            let candles = futures_candles::fetch_ohlcv(
                options,
                testnet,
                || Ok(rt.now()),
                |url, from, to| {
                    let params = Js::object(vec![]);
                    if let Some(from) = from {
                        params.set("from", Js::Number(from)).map_err(ex)?;
                    }
                    if let Some(to) = to {
                        params.set("to", Js::Number(to)).map_err(ex)?;
                    }
                    get_data(
                        &rt,
                        "xhr",
                        "get",
                        vec![Js::str(&url), Js::object(vec![("params", params)])],
                    )
                },
                |prefix, data, error| {
                    let detail = data
                        .map(|d| d.to_string())
                        .or_else(|| error.map(|e| e.message.clone()))
                        .unwrap_or_default();
                    rt.shared
                        .runtime
                        .console("error", &format!("{prefix} {detail}"));
                    Ok(())
                },
                |message| {
                    rt.this
                        .call_method("emitAndLogError", vec![Js::str(message)])
                        .map_err(ex)?;
                    Ok(())
                },
            )
            .await
            .map_err(Thrown::from)?;
            from_typed(&candles)
        }),
        ("spot_reads", "fetch_ohlcv") => future(async move {
            let options = number_input(&a0)?
                .ok_or_else(|| Thrown::type_error("OHLCV options are required"))?;
            let store = rt.shared.store.clone();
            let candles = spot_reads::fetch_ohlcv(
                options,
                store.as_ref(),
                |endpoint, pair, interval, since| {
                    let params = Js::object(vec![
                        (
                            "pair",
                            safe_cex_exchange::js_runtime::value::from_retained_input(pair),
                        ),
                        (
                            "interval",
                            match interval {
                                safe_cex_num::TableHit::Mapped(n) => Js::Number(n),
                                _ => Js::Undefined,
                            },
                        ),
                    ]);
                    if let Some(since) = since {
                        params.set("since", Js::Number(since)).map_err(ex)?;
                    }
                    get_data(
                        &rt,
                        "xhr",
                        "get",
                        vec![Js::str(endpoint), Js::object(vec![("params", params)])],
                    )
                },
                |prefix, message| {
                    rt.shared
                        .runtime
                        .console("error", &format!("{prefix} {message}"));
                    Ok(())
                },
            )
            .await
            .map_err(Thrown::from)?;
            from_typed(&candles)
        }),
        ("spot_balance", "fetch_balance") => future(async move {
            let store = rt.shared.store.clone();
            let balance = spot_balance::fetch_balance(
                store.as_ref(),
                |endpoint| {
                    get_data(
                        &rt,
                        "xhr",
                        "post",
                        vec![Js::str(endpoint), Js::object(vec![])],
                    )
                },
                |message| {
                    rt.this
                        .call_method("emitAndLogError", vec![Js::str(message)])
                        .map_err(ex)?;
                    Ok(())
                },
            )
            .await
            .map_err(Thrown::from)?;
            from_typed(&balance)
        }),
        ("reads", "fetch_account_log_page") => future(async move {
            let params = number_input(&a0)?
                .ok_or_else(|| Thrown::type_error("account-log params are required"))?;
            let store = rt.shared.store.clone();
            let mut identity = |n: f64| n;
            let records = reads::fetch_account_log_page(
                params,
                store.as_ref(),
                |endpoint, params| {
                    let request = get_data(
                        &rt,
                        "historyXhr",
                        "get",
                        vec![
                            Js::str(endpoint),
                            Js::object(vec![("params", json_value(&params))]),
                        ],
                    );
                    async move { request.map_err(boxed)?.await.map_err(boxed) }
                },
                &mut identity,
            )
            .await
            .map_err(|e| Thrown::error("Error", &e.to_string()))?;
            from_typed(&records)
        }),
        ("reads", "fetch_historical_positions") => future(async move {
            let options = number_input(&a0)?
                .ok_or_else(|| Thrown::type_error("historical position options are required"))?;
            let folder =
                rt.shared.extra.journal.clone().ok_or_else(|| {
                    Thrown::error("Error", "Kraken journal folder is not configured")
                })?;
            let venue = rt
                .shared
                .sockets
                .venue
                .borrow()
                .as_ref()
                .and_then(|v| v.upgrade())
                .ok_or_else(|| Thrown::error("Error", "Kraken exchange is not assembled"))?;
            let records =
                reads::fetch_historical_positions(venue.as_ref(), folder.as_ref(), options)
                    .await
                    .map_err(Thrown::from)?;
            Ok(Js::array(records.iter().map(from_json).collect()))
        }),
        ("reads", "fetch_leverage_settings") => {
            let reader = rt.clone();
            let clear_rt = rt.clone();
            let set_rt = rt.clone();
            let console_rt = rt.clone();
            let body = reads::fetch_leverage_settings(
                move |endpoint| get_data(&reader, "xhr", "get", vec![Js::str(endpoint)]),
                move || {
                    clear_rt
                        .this
                        .get("leverageSettings")
                        .and_then(|m| m.call_method("clear", vec![]))
                        .map_err(ex)?;
                    Ok(())
                },
                move |symbol, leverage| {
                    set_rt
                        .this
                        .get("leverageSettings")
                        .and_then(|m| {
                            m.call_method("set", vec![from_json(&symbol), from_json(&leverage)])
                        })
                        .map_err(ex)?;
                    Ok(())
                },
                move |prefix, message| {
                    console_rt
                        .shared
                        .runtime
                        .console("error", &format!("{prefix} {message}"));
                    Ok(())
                },
            );
            future(async move {
                body.await.map_err(Thrown::from)?;
                Ok(Js::Undefined)
            })
        }
        ("reads", "fetch_spot_positions") => future(async move {
            let positions = reads::fetch_spot_positions(rt.shared.store.as_ref());
            from_typed(&positions)
        }),
        ("reads", "fetch_ws_token") => {
            let reader = rt.clone();
            let body = reads::fetch_ws_token(move |endpoint, body| {
                let request = get_data(
                    &reader,
                    "xhr",
                    "post",
                    vec![Js::str(endpoint), from_json(&body)],
                );
                async move { request.map_err(boxed)?.await.map_err(boxed) }
            });
            future(async move {
                let token = body
                    .await
                    .map_err(|e| Thrown::error("Error", &e.to_string()))?;
                Ok(from_json(&token))
            })
        }
        ("validation", "validate_futures_account") | ("validation", "validate_spot_account") => {
            let spot = name == "validate_spot_account";
            future(async move {
                let read = |r: validation::ValidationRead| {
                    let mut call = vec![Js::str(r.endpoint)];
                    if let Some(body) = &r.body {
                        call.push(from_json(body));
                    }
                    get_data(&rt, "xhr", &r.method.to_lowercase(), call)
                };
                let balance = || {
                    let promise = rt.this.call_method("fetchBalance", vec![]).map_err(ex)?;
                    Ok(async move {
                        let value = await_js(promise).await.map_err(ex)?;
                        to_typed::<safe_cex_types::Balance>(&value).map_err(ex)
                    })
                };
                let result = if spot {
                    validation::validate_spot_account(read, balance).await
                } else {
                    validation::validate_futures_account(read, balance).await
                }
                .map_err(Thrown::from)?;
                from_typed(&result)
            })
        }
        ("private_accounts", "income_range") => {
            future(
                async move { Ok((*private_accounts::income_range(&rt, a0, a1, a2).await?).clone()) },
            )
        }
        ("private_accounts", "income_history") => future(async move {
            let opts = if a0.is_undefined() {
                h(Js::object(vec![]))
            } else {
                a0
            };
            Ok((*private_accounts::income_history(&rt, opts).await?).clone())
        }),
        // ---- exchange lifecycle ----
        ("startup", "start_futures") => future(async move {
            startup::start_futures(Rc::new(rt))
                .await
                .map_err(Thrown::from)?;
            Ok(Js::Undefined)
        }),
        ("startup", "start_spot") => future(async move {
            startup::start_spot(Rc::new(rt))
                .await
                .map_err(Thrown::from)?;
            Ok(Js::Undefined)
        }),
        ("startup", "fetch_and_set_orders") => future(async move {
            let orders = startup::fetch_and_set_orders(
                || {
                    call_future(&rt, "fetchOrders", vec![])
                        .map(|f| async move { f.await.map(Rc::new) })
                },
                |orders: &Rc<Js>| {
                    rt.this
                        .get("store")
                        .and_then(|s| {
                            s.call_method("addOrUpdateOrdersFull", vec![(**orders).clone()])
                        })
                        .map_err(ex)?;
                    Ok(())
                },
                || rt.shared.store.loaded().clone(),
                |flags| {
                    rt.this
                        .get("store")
                        .and_then(|s| {
                            s.call_method(
                                "update",
                                vec![Js::object(vec![("loaded", from_typed(&flags)?)])],
                            )
                        })
                        .map_err(ex)?;
                    Ok(())
                },
            )
            .await
            .map_err(Thrown::from)?;
            Ok((*orders).clone())
        }),
        ("periodic_sync", "futures_tick") | ("periodic_sync", "spot_tick") => {
            let count = if a0.is_undefined() {
                None
            } else {
                Some(a0.to_number()?)
            };
            let spot = name == "spot_tick";
            future(async move {
                let host = Rc::new(rt);
                if spot {
                    periodic_sync::spot_tick(host, count).await
                } else {
                    periodic_sync::futures_tick(host, count).await
                }
                .map_err(Thrown::from)?;
                Ok(Js::Undefined)
            })
        }
        ("lifecycle", "futures_dispose") => {
            lifecycle::futures_dispose(&rt).map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("ohlcv_polling", "listen_ohlcv") => {
            let callback = (*a1).clone();
            let cleanup = ohlcv_polling::listen_ohlcv(
                Rc::new(rt),
                a0,
                Rc::new(move |candle: Handle<Rt>| {
                    callback.call(Js::Undefined, vec![(*candle).clone()])?;
                    Ok(())
                }),
            )?;
            value(Js::function("dispose", move |_, _| {
                cleanup()?;
                Ok(Js::Undefined)
            }))
        }
        // ---- exchange stream delegators ----
        ("stream_adapters", "futures_listen_order_book")
        | ("stream_adapters", "spot_listen_order_book") => value(
            delegate_public(&rt, "listenOrderBook", vec![(*a0).clone(), (*a1).clone()])
                .map_err(Thrown::from)?,
        ),
        ("stream_adapters", "spot_listen_ohlcv") => value(
            delegate_public(&rt, "listenOHLCV", vec![(*a0).clone(), (*a1).clone()])
                .map_err(Thrown::from)?,
        ),
        ("stream_adapters", "futures_listen_ohlcv") => value(disposer(|| Ok(()))),
        ("stream_adapters", "futures_handle_ohlc") => value(Js::Undefined),
        // ---- socket bodies ----
        // ts: `this.parent.getActiveTickerSymbols()` — the BaseExchange member, on the Rust core.
        ("ws_helpers", "get_ticker_product_ids") => {
            let base = rt.base().clone();
            let ids = ws_helpers::get_ticker_product_ids(rt.shared.store.as_ref(), || {
                Ok(base.get_active_ticker_symbols())
            })
            .map_err(Thrown::from)?;
            value(Js::array(ids.iter().map(|s| Js::str(s)).collect()))
        }
        ("ws_helpers", "map_timeframe") => {
            let key = a0.to_js_string()?;
            value(match ws_helpers::map_timeframe(&key) {
                safe_cex_num::TableHit::Mapped(n) => Js::Number(n),
                safe_cex_num::TableHit::Inherited(_) => {
                    Js::function("inherited", |_, _| Ok(Js::Undefined))
                }
                safe_cex_num::TableHit::Missing => Js::Number(1.0),
            })
        }
        ("feed_stats", "futures_clear_feed_stats_if_idle")
        | ("feed_stats", "spot_clear_feed_stats_if_idle") => {
            let feed = (*a0).clone();
            let this = rt.this.clone();
            feed_stats::futures_clear_feed_stats_if_idle(&feed, lengths(&a1), |feed| {
                this.call_method("cleanupMessageHandlerStats", vec![feed.clone()])
                    .map_err(ex)?;
                Ok(())
            })
            .map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("lifecycle", "futures_public_connect")
        | ("lifecycle", "futures_private_connect")
        | ("lifecycle", "spot_public_connect")
        | ("lifecycle", "spot_private_connect") => {
            let host = socket(&rt)?;
            match name {
                "futures_public_connect" => lifecycle::futures_public_connect(host),
                "futures_private_connect" => lifecycle::futures_private_connect(host),
                "spot_public_connect" => lifecycle::spot_public_connect(host),
                _ => lifecycle::spot_private_connect(host),
            }
            .map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("public_subscriptions", "futures_on_open") => {
            public_subscriptions::futures_on_open(socket(&rt)?.as_ref()).map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("public_subscriptions", "spot_on_open") => {
            public_subscriptions::spot_on_open(socket(&rt)?.as_ref()).map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("message_dispatch", _) => {
            let host = socket(&rt)?;
            // ts: `onMessage = ({ data }: MessageEvent) => ...`.
            let data = Rc::new(if a0.is_nullish() {
                Js::Undefined
            } else {
                a0.get("data")?
            });
            match name {
                "futures_public" => message_dispatch::futures_public(host.as_ref(), data),
                "futures_private" => message_dispatch::futures_private(host.as_ref(), data),
                "spot_public" => message_dispatch::spot_public(host.as_ref(), data),
                _ => message_dispatch::spot_private(host.as_ref(), data),
            }
            .map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("stream_updates", "futures_handle_ticker") => {
            stream_updates::futures_handle_ticker(socket(&rt)?.as_ref(), &a0)
                .map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("stream_updates", "spot_handle_ticker") => {
            stream_updates::spot_handle_ticker(socket(&rt)?.as_ref(), &a0).map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("stream_updates", "spot_handle_ohlc") => {
            stream_updates::spot_handle_ohlc(socket(&rt)?.as_ref(), &a0).map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("orderbooks", _) => {
            let host = socket(&rt)?;
            match name {
                "futures_snapshot" => orderbooks::futures_snapshot(host.as_ref(), &a0),
                "futures_delta" => orderbooks::futures_delta(host.as_ref(), &a0),
                "futures_emit" => orderbooks::futures_emit(host.as_ref(), &a0),
                "spot_snapshot" => orderbooks::spot_snapshot(host.as_ref(), a0, &a1),
                "spot_delta" => orderbooks::spot_delta(host.as_ref(), a0, &a1),
                _ => orderbooks::spot_emit(host.as_ref(), &a0),
            }
            .map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("book_listeners", _) => {
            let host = socket(&rt)?;
            let listener = match name {
                "futures_listen_book" => book_listeners::futures_listen_book(host, a0, a1),
                "spot_listen_book" => book_listeners::spot_listen_book(host, a0, a1),
                _ => book_listeners::spot_listen_ohlcv(host, a0, a1),
            }
            .map_err(Thrown::from)?;
            value(disposer(move || listener.dispose()))
        }
        ("spot_dispatch", "handle_book") => {
            spot_dispatch::handle_book(socket(&rt)?.as_ref(), &a0).map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("spot_dispatch", "handle_balances") => {
            spot_dispatch::handle_balances(socket(&rt)?.as_ref(), &a0).map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("private_accounts", "futures_fills")
        | ("private_accounts", "futures_open_orders")
        | ("private_accounts", "futures_open_positions")
        | ("private_accounts", "futures_balances")
        | ("private_accounts", "spot_executions") => {
            match name {
                "futures_fills" => private_accounts::futures_fills(&rt, a0),
                "futures_open_orders" => private_accounts::futures_open_orders(&rt, a0),
                "futures_open_positions" => private_accounts::futures_open_positions(&rt, a0),
                "futures_balances" => private_accounts::futures_balances(&rt, a0),
                _ => private_accounts::spot_executions(&rt, a0),
            }?;
            value(Js::Undefined)
        }
        ("private_accounts", "spot_parse_order") => {
            value((*private_accounts::spot_parse_order(&rt, a0)?).clone())
        }
        ("private_control", "perform_auth") => {
            let host = socket(&rt)?;
            future(async move {
                private_control::perform_auth(host.as_ref())
                    .await
                    .map_err(Thrown::from)?;
                Ok(Js::Undefined)
            })
        }
        ("private_control", "subscribe_private_feeds") => {
            private_control::subscribe_private_feeds(socket(&rt)?.as_ref())
                .map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("private_control", "futures_throttled_fetch_orders") => {
            private_control::futures_throttled_fetch_orders(socket(&rt)?.as_ref())
                .map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("private_control", "spot_throttled_fetch_orders") => {
            private_control::spot_throttled_fetch_orders(socket(&rt)?.as_ref())
                .map_err(Thrown::from)?;
            value(Js::Undefined)
        }
        ("private_control", "futures_fetch_throttled_orders") => {
            let host = socket(&rt)?;
            future(async move {
                private_control::futures_fetch_throttled_orders(host.as_ref())
                    .await
                    .map_err(Thrown::from)?;
                Ok(Js::Undefined)
            })
        }
        ("private_auth", "authenticate") => {
            let host = socket(&rt)?;
            let key = options_text(&rt, "key").map_err(Thrown::from)?;
            let secret = options_text(&rt, "secret").map_err(Thrown::from)?;
            let remote = rt.base().signers.remote_signer.is_some();
            let send = send_text(&rt.this);
            let parent = rt.this.get("parent")?;
            future(async move {
                let log_rt = rt.clone();
                private_auth::authenticate(
                    &host.futures_auth,
                    key.as_deref(),
                    secret.as_deref(),
                    remote,
                    send,
                    |message, severity| log_with(&log_rt, parent.clone(), message, severity),
                )
                .await
                .map_err(Thrown::from)?;
                let waiting = host.futures_auth.borrow().waiting_challenge;
                rt.this.set("waitingChallenge", Js::Bool(waiting))?;
                Ok(Js::Undefined)
            })
        }
        ("private_auth", "futures_on_open") => {
            let parent = rt.this.get("parent")?;
            future(async move {
                let this = rt.this.clone();
                let disposed = rt.this.clone();
                private_auth::futures_on_open(
                    move || disposed.get("isDisposed").is_ok_and(|v| v.truthy()),
                    move || {
                        let promise = this.call_method("authenticate", vec![]).map_err(ex)?;
                        Ok(async move {
                            await_js(promise).await.map_err(ex)?;
                            Ok(())
                        })
                    },
                    |message, severity| log_with(&rt, parent.clone(), message, severity),
                )
                .await
                .map_err(Thrown::from)?;
                Ok(Js::Undefined)
            })
        }
        ("private_auth", "set_token") => {
            let host = socket(&rt)?;
            let token = js_json(&a0)?;
            private_auth::set_token(&host.spot_auth, token);
            rt.this.set("wsToken", (*a0).clone())?;
            value(Js::Undefined)
        }
        ("private_auth", "spot_on_open") => {
            let host = socket(&rt)?;
            let parent = rt.this.get("parent")?;
            let send = send_text(&rt.this);
            future(async move {
                let disposed = rt.this.clone();
                let fetch_parent = parent.clone();
                private_auth::spot_on_open(
                    &host.spot_auth,
                    move || disposed.get("isDisposed").is_ok_and(|v| v.truthy()),
                    move || {
                        let promise = fetch_parent
                            .call_method("fetchWsToken", vec![])
                            .map_err(ex)?;
                        Ok(async move {
                            let token = await_js(promise).await.map_err(ex)?;
                            js_json(&token).map_err(ex)
                        })
                    },
                    send,
                    |message, severity| log_with(&rt, parent.clone(), message, severity),
                )
                .await
                .map_err(Thrown::from)?;
                let (authenticated, token) = {
                    let state = host.spot_auth.borrow();
                    (state.authenticated, json_value(&state.token))
                };
                rt.this.set("authenticated", Js::Bool(authenticated))?;
                rt.this.set("wsToken", token)?;
                Ok(Js::Undefined)
            })
        }
        ("lib", "sign_kraken_ws_auth") => future(async move {
            let request: safe_cex_types::ws_auth::WsAuthRequest = to_typed(&a0)?;
            let secret = rt.this.get("options").and_then(|o| o.get("secret"))?;
            let secret = if secret.is_nullish() {
                String::new()
            } else {
                secret.to_display()
            };
            from_typed(&safe_cex_crypto::sign_kraken_ws_auth(&request, &secret))
        }),
        _ => Err(Thrown::error(
            "Error",
            &format!("Kraken binding has no adapter for {target}"),
        )),
    }
}

fn prototype_of(rt: &Rt, kind: Kind) -> Js {
    let mut protos = rt.shared.extra.prototypes.borrow_mut();
    if let Some((_, p)) = protos.iter().find(|(k, _)| *k == kind) {
        return p.clone();
    }
    let proto = Js::object(vec![]);
    protos.push((kind, proto.clone()));
    proto
}

impl ClassHost for Rt {
    fn super_socket(&self, kind: Kind, parent: Handle<Self>) -> ReadResult<Self, Handle<Self>> {
        let socket = super::sockets_js::base_websocket(&self.shared.sockets, (*parent).clone());
        if let Js::Object(o) = &socket {
            *o.proto.borrow_mut() = Some(prototype_of(self, kind));
        }
        let host = SocketHost::new(self.with_this(socket.clone()));
        self.shared.extra.sockets.borrow_mut().push(host);
        Ok(h(socket))
    }
    fn define(&self, receiver: &Js, name: &[u16], value: Handle<Self>) -> ReadResult<Self, ()> {
        receiver.define_key(name, (*value).clone())
    }
    fn undefined_value(&self) -> Handle<Self> {
        h(Js::Undefined)
    }
    fn fresh_map(&self) -> ReadResult<Self, Handle<Self>> {
        Ok(h(Js::new_map()))
    }
    fn imported(&self, name: &'static str) -> ReadResult<Self, Handle<Self>> {
        let list = match name {
            "GRAFANA_ENDPOINTS_EVERY" => crate::spot_types::GRAFANA_ENDPOINTS_EVERY,
            "GRAFANA_ENDPOINTS_PERIODIC" => crate::spot_types::GRAFANA_ENDPOINTS_PERIODIC,
            other => {
                return Err(Thrown::error(
                    "ReferenceError",
                    &format!("{other} is not defined"),
                ));
            }
        };
        Ok(h(Js::array(list.iter().map(|s| Js::str(s)).collect())))
    }
    fn member(
        &self,
        _kind: Kind,
        _name: &'static str,
        target: &'static str,
    ) -> ReadResult<Self, NativeMember<Self>> {
        let rt = self.clone();
        Ok(Rc::new(
            move |receiver: Handle<Rt>, args: Vec<Handle<Rt>>| {
                dispatch(rt.with_this((*receiver).clone()), target, args)
            },
        ))
    }
    fn arrow(&self, body: BoundMember<Self>) -> ReadResult<Self, Handle<Self>> {
        let runtime = self.exchange_runtime();
        Ok(h(Js::function("", move |_, a| {
            let args: Vec<Handle<Rt>> = a.into_iter().map(h).collect();
            Ok(match body(args) {
                Ok(Return::Value(v)) => (*v).clone(),
                Ok(Return::Future(f)) => Js::Promise(Promise::spawn(
                    &runtime,
                    Box::pin(async move { f.await.map(|v| (*v).clone()) }),
                )),
                // An async arrow rejects; a sync one throws. Bodies that fail before
                // their first await surface as the arrow's synchronous throw.
                Err(e) => return Err(e),
            })
        })))
    }
    fn prototype(
        &self,
        kind: Kind,
        name: &'static str,
        body: NativeMember<Self>,
    ) -> ReadResult<Self, ()> {
        let proto = prototype_of(self, kind);
        let runtime = self.exchange_runtime();
        proto.define(
            name,
            Js::function(name, move |this, a| {
                let args: Vec<Handle<Rt>> = a.into_iter().map(h).collect();
                Ok(match body(h(this), args)? {
                    Return::Value(v) => (*v).clone(),
                    Return::Future(f) => Js::Promise(Promise::spawn(
                        &runtime,
                        Box::pin(async move { f.await.map(|v| (*v).clone()) }),
                    )),
                })
            }),
        )
    }
}
