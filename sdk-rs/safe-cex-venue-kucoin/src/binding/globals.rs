//! KuCoin module scope: protocol tables, the imported helpers the source
//! reads, and the BaseExchange/BaseWebSocket/createAPI construction globals.
use super::host::{W, shared, text};
use crate::{spot_types, types, ws_helpers};
use safe_cex_exchange::js_runtime::{
    base_exchange, client,
    promise::await_js,
    scope::scope_of,
    sockets,
    value::{Js, Thrown, arg, from_json, from_typed, to_json, to_typed},
};
use safe_cex_types::{ExchangeName, OrderBook, PlaceOrderOpts};
use std::rc::{Rc, Weak};

fn table(rows: &[(&str, &str)]) -> Js {
    Js::object(rows.iter().map(|(k, v)| (*k, Js::str(v))).collect())
}

fn list(rows: &[&str]) -> Js {
    Js::array(rows.iter().map(|s| Js::str(s)).collect())
}

fn enum_table<T: serde::Serialize>(rows: &[(&str, T)]) -> Js {
    Js::object(
        rows.iter()
            .map(|(k, v)| {
                let text = serde_json::to_value(v)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default();
                (*k, Js::str(&text))
            })
            .collect(),
    )
}

fn venue(weak: &W) -> Result<Rc<dyn safe_cex_exchange::exchange::Exchange>, Thrown> {
    shared(weak)?
        .sockets
        .venue
        .borrow()
        .as_ref()
        .and_then(Weak::upgrade)
        .ok_or_else(|| Thrown::error("Error", "KuCoin exchange is not assembled"))
}

fn spawn(
    weak: &W,
    body: safe_cex_exchange::js_runtime::promise::JsFuture<'static>,
) -> Result<Js, Thrown> {
    Ok(scope_of(weak)?.spawn(body))
}

fn number(v: &Js) -> Result<f64, Thrown> {
    v.to_number()
}

/// `^[A-Za-z0-9_-]{1,40}$`, the only pattern either KuCoin class declares.
fn kucoin_client_order_id(id: &str) -> bool {
    (1..=40).contains(&id.chars().count())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

pub(crate) const CLIENT_ORDER_ID_SOURCE: &str = "^[A-Za-z0-9_-]{1,40}$";

pub(crate) fn client_order_id_rule(
    spot: bool,
) -> safe_cex_exchange::client_order_id::ClientOrderIdRule {
    safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
        if spot { "KuCoin Spot" } else { "KuCoin" }.into(),
        format!("/{CLIENT_ORDER_ID_SOURCE}/"),
        Rc::new(kucoin_client_order_id),
        Default::default(),
    )
}

pub(crate) fn regexp(source: &str) -> Result<Js, Thrown> {
    if source != CLIENT_ORDER_ID_SOURCE {
        return Err(Thrown::error(
            "SyntaxError",
            &format!("KuCoin binding has no RegExp /{source}/"),
        ));
    }
    let regexp = Js::object_with_class("RegExp", vec![("source", Js::str(source))]);
    regexp.define(
        "test",
        Js::function("test", |_, a| {
            Ok(Js::Bool(kucoin_client_order_id(&text(&arg(&a, 0))?)))
        }),
    )?;
    Ok(regexp)
}

/// Apply a typed OrderBook helper to a live JS book, writing both sides back.
fn with_book(book: &Js, f: impl FnOnce(&mut OrderBook) -> f64) -> Result<f64, Thrown> {
    let mut typed: OrderBook = to_typed(book)?;
    let out = f(&mut typed);
    book.set("bids", from_typed(&typed.bids)?)?;
    book.set("asks", from_typed(&typed.asks)?)?;
    Ok(out)
}

const DAY_MS: f64 = 86_400_000.0;

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// dayjs `subtract(amount, unit)` on epoch milliseconds (UTC calendar for months/years).
fn dayjs_subtract(ms: f64, amount: f64, unit: &str) -> f64 {
    let step = match unit {
        "ms" | "millisecond" | "milliseconds" => 1.0,
        "s" | "second" | "seconds" => 1000.0,
        "m" | "minute" | "minutes" => 60_000.0,
        "h" | "hour" | "hours" => 3_600_000.0,
        "d" | "day" | "days" => return ms - amount.round() * DAY_MS,
        "w" | "week" | "weeks" => return ms - (amount * 7.0).round() * DAY_MS,
        "M" | "month" | "months" | "y" | "year" | "years" => {
            let months = if unit.starts_with('y') {
                amount * 12.0
            } else {
                amount
            } as i64;
            let days = (ms / DAY_MS).floor() as i64;
            let within = ms - days as f64 * DAY_MS;
            let (y, m, d) = civil_from_days(days);
            let total = y * 12 + (m - 1) - months;
            let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) + 1);
            let last = days_from_civil(
                if nm == 12 { ny + 1 } else { ny },
                if nm == 12 { 1 } else { nm + 1 },
                1,
            ) - days_from_civil(ny, nm, 1);
            return days_from_civil(ny, nm, d.min(last)) as f64 * DAY_MS + within;
        }
        _ => 1.0,
    };
    ms - amount * step
}

fn dayjs(ms: f64) -> Js {
    let value = Js::object_with_class("Dayjs", vec![]);
    let _ = value.define(
        "valueOf",
        Js::function("valueOf", move |_, _| Ok(Js::Number(ms))),
    );
    let _ = value.define(
        "subtract",
        Js::function("subtract", move |_, a| {
            Ok(dayjs(dayjs_subtract(
                ms,
                number(&arg(&a, 0))?,
                &text(&arg(&a, 1))?,
            )))
        }),
    );
    value
}

/// The JS BaseExchange instance plus the base members the KuCoin bodies call.
fn base_exchange(weak: &W, a: Vec<Js>) -> Result<Js, Thrown> {
    let scope = shared(weak)?;
    let parts = scope
        .extra
        .parts
        .borrow_mut()
        .take()
        .ok_or_else(|| Thrown::error("Error", "BaseExchange constructed twice"))?;
    let (this, base) =
        base_exchange::create(&scope.sockets, parts, arg(&a, 0), arg(&a, 1), arg(&a, 2))?;
    let _ = scope.base.set(base.clone());
    scope.extra.supers.borrow_mut().push((
        this.clone(),
        Js::object(vec![("dispose", this.get("dispose")?)]),
    ));
    let def = |k: &str, v: Js| this.define(k, v);
    {
        let weak = weak.clone();
        let base = base.clone();
        def(
            "initExternalPublicData",
            Js::function("initExternalPublicData", move |_, _| {
                let factory = shared_factory(&weak)?;
                let base = base.clone();
                spawn(
                    &weak,
                    Box::pin(async move {
                        let used = base
                            .init_external_public_data(move |config| match factory {
                                Some(factory) => factory(config),
                                None => unreachable!("KucoinExchange::new rejects externalPublicData without a provider"),
                            })
                            .await?;
                        Ok(Js::Bool(used))
                    }),
                )
            }),
        )?;
    }
    {
        let base = base.clone();
        def(
            "getTickInterval",
            Js::function("getTickInterval", move |_, _| {
                Ok(base.get_tick_interval().map_or(Js::Undefined, Js::Number))
            }),
        )?;
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
        )?;
    }
    {
        let weak = weak.clone();
        let base = base.clone();
        def(
            "fetchAndSetPrivateSnapshot",
            Js::function("fetchAndSetPrivateSnapshot", move |_, a| {
                let tries = if arg(&a, 0).is_undefined() {
                    1.0
                } else {
                    number(&arg(&a, 0))?
                };
                let venue = venue(&weak)?;
                let base = base.clone();
                spawn(
                    &weak,
                    Box::pin(async move {
                        base.fetch_and_set_private_snapshot(&*venue, tries as u32)
                            .await?;
                        Ok(Js::Undefined)
                    }),
                )
            }),
        )?;
    }
    {
        let weak = weak.clone();
        let base = base.clone();
        def(
            "fetchAndSetOrders",
            Js::function("fetchAndSetOrders", move |_, a| {
                let symbols: Option<Vec<String>> = to_typed(&arg(&a, 0))?;
                let venue = venue(&weak)?;
                let base = base.clone();
                spawn(
                    &weak,
                    Box::pin(async move {
                        from_typed(&base.fetch_and_set_orders(&*venue, symbols).await?)
                    }),
                )
            }),
        )?;
    }
    {
        let base = base.clone();
        this.define_accessor(
            "externalPublicDataProvider",
            Some(Rc::new(move |_| {
                Ok(match base.external_public_data_provider.borrow().clone() {
                    Some(provider) => external_provider(provider),
                    None => Js::Undefined,
                })
            })),
            None,
        );
    }
    let emitter = this.get("emitter")?;
    {
        let weak = weak.clone();
        let base = base.clone();
        emitter.define(
            "on",
            Js::function("on", move |_, a| {
                let event = text(&arg(&a, 0))?;
                let callback = arg(&a, 1);
                let listener = callback.clone();
                let id = base.on(
                    &event,
                    Rc::new(move |_, data| {
                        listener.call(Js::Undefined, vec![from_json(data)])?;
                        Ok(())
                    }),
                );
                shared(&weak)?
                    .extra
                    .listeners
                    .borrow_mut()
                    .push((event, callback, id));
                Ok(Js::Undefined)
            }),
        )?;
    }
    {
        let weak = weak.clone();
        emitter.define(
            "off",
            Js::function("off", move |_, a| {
                let event = text(&arg(&a, 0))?;
                let callback = arg(&a, 1);
                let shared = shared(&weak)?;
                let mut rows = shared.extra.listeners.borrow_mut();
                if let Some(i) = rows
                    .iter()
                    .position(|(e, f, _)| *e == event && f.same_reference(&callback))
                {
                    let (_, _, id) = rows.remove(i);
                    base.off(&event, Some(&id));
                }
                Ok(Js::Undefined)
            }),
        )?;
    }
    Ok(this)
}

fn shared_factory(
    weak: &W,
) -> Result<Option<crate::kucoin_exchange::ExternalPublicDataFactory>, Thrown> {
    Ok(shared(weak)?.extra.external.clone())
}

fn external_provider(provider: Rc<dyn safe_cex_exchange::external::ExternalPublicData>) -> Js {
    let candle = provider.clone();
    let book = provider;
    let cleanup = |cleanup: safe_cex_exchange::runtime::ListenerCleanup| {
        let cell = std::cell::RefCell::new(Some(cleanup));
        Js::function("unsubscribe", move |_, _| {
            if let Some(f) = cell.borrow_mut().take() {
                f()?;
            }
            Ok(Js::Undefined)
        })
    };
    Js::object(vec![
        (
            "subscribeCandle",
            Js::function("subscribeCandle", move |_, a| {
                let callback = arg(&a, 1);
                let done = candle.listen_ohlcv(
                    to_typed(&arg(&a, 0))?,
                    Rc::new(move |c| {
                        if let Ok(v) = from_typed(&c) {
                            let _ = callback.call(Js::Undefined, vec![v]);
                        }
                    }),
                )?;
                Ok(cleanup(done))
            }),
        ),
        (
            "subscribeOrderbook",
            Js::function("subscribeOrderbook", move |_, a| {
                let callback = arg(&a, 1);
                let done = book.listen_order_book(
                    text(&arg(&a, 0))?,
                    Rc::new(move |b| {
                        if let Ok(v) = from_typed(&b) {
                            let _ = callback.call(Js::Undefined, vec![v]);
                        }
                    }),
                )?;
                Ok(cleanup(done))
            }),
        ),
    ])
}

/// `new BaseWebSocket(parent)` on the shared core. The base `dispose` lives on
/// BaseWebSocket.prototype so subclass prototype members and super calls reach it.
fn base_websocket(weak: &W, parent: Js) -> Result<Js, Thrown> {
    let shared = shared(weak)?;
    let socket = sockets::base_websocket(&shared.sockets, parent);
    socket.delete(&safe_cex_exchange::js_runtime::value::utf16("dispose"));
    let core = sockets::core_of(&shared.sockets, &socket)
        .ok_or_else(|| Thrown::error("Error", "socket core missing"))?;
    socket.define(
        "retryFailedConnect",
        Js::function("retryFailedConnect", move |_, a| {
            core.base()?.retry_failed_connect(&text(&arg(&a, 0))?)?;
            Ok(Js::Undefined)
        }),
    )?;
    Ok(socket)
}

fn base_websocket_prototype(weak: &W) -> Js {
    let weak = weak.clone();
    Js::object(vec![(
        "dispose",
        Js::function("dispose", move |this, _| {
            let shared = shared(&weak)?;
            if let Some(core) = sockets::core_of(&shared.sockets, &this)
                && let Some(base) = core.base.get()
            {
                base.dispose();
            }
            Ok(Js::Undefined)
        }),
    )])
}

fn map_constructor() -> Js {
    Js::constructor("Map", |_, a| {
        let map = Js::new_map();
        if let Js::Map(entries) = &map
            && let Some(rows) = arg(&a, 0).array_values()
        {
            for row in rows {
                let k = row.get("0")?;
                let v = row.get("1")?;
                let mut entries = entries.borrow_mut();
                match entries.iter_mut().find(|(x, _)| x.same_value_zero(&k)) {
                    Some(slot) => slot.1 = v,
                    None => entries.push((k, v)),
                }
            }
        }
        Ok(map)
    })
}

fn exchange_name(text: &str) -> Result<ExchangeName, Thrown> {
    serde_json::from_value(serde_json::Value::String(text.into()))
        .map_err(|e| Thrown::type_error(&e.to_string()))
}

fn json_arg(v: &Js) -> Result<Option<serde_json::Value>, Thrown> {
    Ok(if v.is_undefined() {
        None
    } else {
        Some(to_json(v)?)
    })
}

pub(crate) fn install(
    g: &Js,
    weak: W,
    runtime: &Rc<dyn safe_cex_exchange::websocket::WsRuntime>,
) -> Result<(), Thrown> {
    let def = |k: &str, v: Js| g.define(k, v);
    // kucoin.types.ts / kucoinspot.types.ts
    def("ENDPOINTS", table(types::ENDPOINTS))?;
    def(
        "GRAFANA_ENDPOINTS_EVERY",
        list(types::GRAFANA_ENDPOINTS_EVERY),
    )?;
    def(
        "GRAFANA_ENDPOINTS_PERIODIC",
        list(types::GRAFANA_ENDPOINTS_PERIODIC),
    )?;
    def("INTERVAL", table(types::INTERVAL))?;
    def("WS_INTERVAL", table(types::WS_INTERVAL))?;
    def("ORDER_STATUS", enum_table(types::ORDER_STATUS))?;
    def("ORDER_TYPE", enum_table(types::ORDER_TYPE))?;
    def("TIME_IN_FORCE", enum_table(types::TIME_IN_FORCE))?;
    def(
        "KUCOIN_SPOT_ENDPOINTS",
        table(spot_types::KUCOIN_SPOT_ENDPOINTS),
    )?;
    def(
        "KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY",
        list(spot_types::KUCOIN_SPOT_GRAFANA_ENDPOINTS_EVERY),
    )?;
    def(
        "KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC",
        list(spot_types::KUCOIN_SPOT_GRAFANA_ENDPOINTS_PERIODIC),
    )?;
    def(
        "KUCOIN_SPOT_INTERVAL",
        table(spot_types::KUCOIN_SPOT_INTERVAL),
    )?;
    def(
        "KUCOIN_SPOT_ORDER_STATUS",
        enum_table(spot_types::KUCOIN_SPOT_ORDER_STATUS),
    )?;
    def(
        "KUCOIN_SPOT_ORDER_TYPE",
        enum_table(spot_types::KUCOIN_SPOT_ORDER_TYPE),
    )?;
    def(
        "KUCOIN_SPOT_TIME_IN_FORCE",
        enum_table(spot_types::KUCOIN_SPOT_TIME_IN_FORCE),
    )?;
    // kucoin.ws-public.ts module constants
    def("SNAPSHOT_CHANNEL", Js::str("/contractMarket/snapshot"))?;
    def(
        "ACTIVE_DETAIL_TICKER_CHANNELS",
        list(&["/contractMarket/tickerV2", "/contract/instrument"]),
    )?;
    def(
        "ENABLE_RECONNECTING_WEBSOCKET",
        Js::Bool(runtime.enable_reconnecting()),
    )?;

    // Builtins the shared scope leaves out.
    let clock = runtime.clone();
    def(
        "performance",
        Js::object(vec![(
            "now",
            Js::function("now", move |_, _| Ok(Js::Number(clock.monotonic_now_ms()))),
        )]),
    )?;
    let number_global = g.get("Number")?;
    number_global.define(
        "isFinite",
        Js::function("isFinite", |_, a| {
            Ok(Js::Bool(
                matches!(arg(&a, 0), Js::Number(v) if v.is_finite()),
            ))
        }),
    )?;
    number_global.define("MAX_SAFE_INTEGER", Js::Number(9_007_199_254_740_991.0))?;
    def("Map", map_constructor())?;

    // Imported utils.
    def(
        "adjust",
        Js::function("adjust", |_, a| {
            let step = arg(&a, 1);
            let step = match &step {
                Js::String(_) => safe_cex_num::PrecisionInput::String(text(&step)?),
                other => safe_cex_num::PrecisionInput::Number(number(other)?),
            };
            Ok(Js::Number(safe_cex_num::adjust(number(&arg(&a, 0))?, step)))
        }),
    )?;
    def(
        "multiply",
        Js::function("multiply", |_, a| {
            Ok(Js::Number(safe_cex_num::multiply(
                number(&arg(&a, 0))?,
                number(&arg(&a, 1))?,
            )))
        }),
    )?;
    def(
        "divide",
        Js::function("divide", |_, a| {
            Ok(Js::Number(safe_cex_num::divide(
                number(&arg(&a, 0))?,
                number(&arg(&a, 1))?,
                None,
            )))
        }),
    )?;
    def(
        "subtract",
        Js::function("subtract", |_, a| {
            Ok(Js::Number(safe_cex_num::subtract(
                number(&arg(&a, 0))?,
                number(&arg(&a, 1))?,
            )))
        }),
    )?;
    def(
        "uuid",
        Js::function("uuid", |_, _| {
            Ok(Js::str(
                &safe_cex_crypto::uuid::uuid().map_err(|e| Thrown::error("Error", &e))?,
            ))
        }),
    )?;
    def(
        "omitUndefined",
        Js::function("omitUndefined", |_, a| {
            let out = Js::object(vec![]);
            let input = arg(&a, 0);
            if !input.is_nullish() {
                for (k, v) in input.own_entries()? {
                    if !v.is_undefined() {
                        out.define_key(&k, v)?;
                    }
                }
            }
            Ok(out)
        }),
    )?;
    def(
        "createPositionId",
        Js::function("createPositionId", |_, a| {
            let (symbol, side, mode) = (arg(&a, 0), arg(&a, 1), arg(&a, 2));
            if mode.strict_equals(&Js::str("oneway")) || !side.truthy() {
                return Ok(symbol);
            }
            let mut out = symbol.to_js_string()?.to_vec();
            out.push(b':' as u16);
            out.extend_from_slice(&side.to_js_string()?);
            Ok(Js::String(out.into()))
        }),
    )?;
    def(
        "toBinance",
        Js::function("toBinance", |_, a| {
            Ok(Js::str(&safe_cex_types::symbol_converter::to_binance(
                &text(&arg(&a, 0))?,
                &exchange_name(&text(&arg(&a, 1))?)?,
            )))
        }),
    )?;
    def(
        "flatten",
        Js::function("flatten", |_, a| Ok(super::host::flat(&arg(&a, 0)))),
    )?;
    let series = weak.clone();
    def(
        "mapSeries",
        Js::function("mapSeries", move |_, a| {
            let rows = arg(&a, 0);
            let f = arg(&a, 1);
            spawn(
                &series,
                Box::pin(async move {
                    let mut out = vec![];
                    let values = rows.array_values().unwrap_or_default();
                    for (i, v) in values.into_iter().enumerate() {
                        out.push(
                            await_js(f.call(
                                Js::Undefined,
                                vec![v, Js::Number(i as f64), rows.clone()],
                            )?)
                            .await?,
                        );
                    }
                    Ok(Js::array(out))
                }),
            )
        }),
    )?;
    def(
        "partition",
        Js::function("partition", |_, a| {
            let (mut yes, mut no) = (vec![], vec![]);
            let f = arg(&a, 1);
            for v in arg(&a, 0).array_values().unwrap_or_default() {
                if f.call(Js::Undefined, vec![v.clone()])?.truthy() {
                    yes.push(v);
                } else {
                    no.push(v);
                }
            }
            Ok(Js::array(vec![Js::array(yes), Js::array(no)]))
        }),
    )?;
    def(
        "uniqBy",
        Js::function("uniqBy", |_, a| {
            let key = text(&arg(&a, 1))?;
            let mut seen: Vec<Js> = vec![];
            let mut out = vec![];
            for v in arg(&a, 0).array_values().unwrap_or_default() {
                let k = if v.is_nullish() {
                    Js::Undefined
                } else {
                    v.get(&key)?
                };
                if !seen.iter().any(|s| s.same_value_zero(&k)) {
                    seen.push(k);
                    out.push(v);
                }
            }
            Ok(Js::array(out))
        }),
    )?;
    let looper = weak.clone();
    def(
        "loop",
        Js::function("loop", move |_, a| {
            let f = arg(&a, 0);
            let interval = arg(&a, 1);
            let scope = scope_of(&looper)?;
            let runtime = scope.exchange_runtime();
            spawn(
                &looper,
                Box::pin(async move {
                    let interval = number(&interval)?;
                    if interval == f64::INFINITY {
                        return Ok(Js::Undefined);
                    }
                    let interval = if interval > 0.0 { interval } else { 5000.0 };
                    runtime.sleep(interval as u64).await?;
                    runtime.sleep(0).await?;
                    if let Err(e) = f.call(Js::Undefined, vec![]) {
                        scope.report(&e, "loop");
                    }
                    Ok(Js::Undefined)
                }),
            )
        }),
    )?;
    let clock = runtime.clone();
    def(
        "dayjs",
        Js::function("dayjs", move |_, a| {
            Ok(dayjs(if a.is_empty() {
                clock.now_ms()
            } else {
                number(&arg(&a, 0))?
            }))
        }),
    )?;
    def(
        "jsonParse",
        Js::function("jsonParse", |_, a| {
            let text = text(&arg(&a, 0))?;
            Ok(serde_json::from_str::<serde_json::Value>(&text)
                .map(|v| from_json(&v))
                .unwrap_or(Js::Null))
        }),
    )?;
    def(
        "parseNumber",
        Js::function("parseNumber", |_, a| {
            let value = json_arg(&arg(&a, 0))?;
            Ok(ws_helpers::parse_number(value.as_ref())
                .map_err(|e| Thrown::type_error(&e))?
                .map_or(Js::Undefined, Js::Number))
        }),
    )?;
    for (name, f) in [
        (
            "normalizeTimestamp",
            ws_helpers::normalize_timestamp
                as for<'v> fn(Option<&'v serde_json::Value>, f64) -> Result<f64, String>,
        ),
        ("normalizeMillis", ws_helpers::normalize_millis),
    ] {
        let clock = runtime.clone();
        def(
            name,
            Js::function(name, move |_, a| {
                let value = json_arg(&arg(&a, 0))?;
                Ok(Js::Number(
                    f(value.as_ref(), clock.now_ms()).map_err(|e| Thrown::type_error(&e))?,
                ))
            }),
        )?;
    }
    def(
        "calcOrderBookTotal",
        Js::function("calcOrderBookTotal", |_, a| {
            with_book(&arg(&a, 0), |b| {
                safe_cex_store::orderbook::calc_order_book_total(b);
                0.0
            })?;
            Ok(Js::Undefined)
        }),
    )?;
    def(
        "sortOrderBook",
        Js::function("sortOrderBook", |_, a| {
            with_book(&arg(&a, 0), |b| {
                safe_cex_store::orderbook::sort_order_book(b);
                0.0
            })?;
            Ok(Js::Undefined)
        }),
    )?;
    def(
        "updateOrderBookByPrice",
        Js::function("updateOrderBookByPrice", |_, a| {
            let bids = text(&arg(&a, 1))? == "bids";
            let (price, amount) = (number(&arg(&a, 2))?, number(&arg(&a, 3))?);
            let upsert = arg(&a, 4).is_undefined() || arg(&a, 4).truthy();
            let index = with_book(&arg(&a, 0), |b| {
                safe_cex_store::update_order_book::update_order_book_by_price(
                    b, bids, price, amount, upsert,
                ) as f64
            })?;
            Ok(Js::Number(index))
        }),
    )?;
    let snapshots = weak.clone();
    def(
        "createKucoinSnapshotWebsockets",
        Js::function("createKucoinSnapshotWebsockets", move |_, a| {
            let parent = arg(&a, 0);
            let count = number(&arg(&a, 1))?;
            let class = shared(&snapshots)?.globals.get("KucoinPublicWebsocket")?;
            crate::socket_setup::create_snapshot_websockets(
                &parent,
                count,
                |length, build| {
                    let mut out = vec![];
                    let length = if length.is_finite() && length > 0.0 {
                        length as usize
                    } else {
                        0
                    };
                    for i in 0..length {
                        out.push(build(i)?);
                    }
                    Ok(Js::array(out))
                },
                |parent, role, index, shards| {
                    class.call(
                        Js::Undefined,
                        vec![
                            parent.clone(),
                            Js::object(vec![
                                ("role", Js::str(role)),
                                ("shardIndex", Js::Number(index as f64)),
                                ("shardCount", Js::Number(shards)),
                            ]),
                        ],
                    )
                },
            )
        }),
    )?;

    // Construction-time collaborators.
    let w = weak.clone();
    let base = Js::constructor("BaseExchange", move |_, a| base_exchange(&w, a));
    base.define("prototype", Js::object(vec![]))?;
    def("BaseExchange", base)?;
    let w = weak.clone();
    let socket = Js::constructor("BaseWebSocket", move |_, a| base_websocket(&w, arg(&a, 0)));
    socket.define("prototype", base_websocket_prototype(&weak))?;
    def("BaseWebSocket", socket)?;
    for name in ["createAPI", "createKucoinSpotAPI"] {
        let w = weak.clone();
        def(
            name,
            Js::function(name, move |_, _| {
                Ok(client::create(shared(&w)?.http.clone(), Js::object(vec![])))
            }),
        )?;
    }
    def(
        "rateLimit",
        Js::function("rateLimit", |_, a| {
            let xhr = arg(&a, 0);
            xhr.call_method("__configureRateLimit", vec![arg(&a, 1)])?;
            Ok(xhr)
        }),
    )?;
    def(
        "patternClientOrderIdRule",
        Js::function("patternClientOrderIdRule", |_, a| {
            let pattern = arg(&a, 1);
            let source = text(&pattern.get("source")?)?;
            Ok(Js::object(vec![
                ("label", arg(&a, 0)),
                ("requirement", Js::str(&format!("match /{source}/"))),
                (
                    "isValid",
                    Js::function("isValid", move |_, v| pattern.call_method("test", v)),
                ),
            ]))
        }),
    )?;
    Ok(())
}
