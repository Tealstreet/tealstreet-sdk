//! One exchange account on the native host behind a JSON surface, for hosts that
//! cannot name SDK types (the napi binding). Values cross as JSON; failures cross as
//! `{name, message, code?}`. Requires a tokio `LocalSet`, like the native host.
use crate::native_host::{bind_http_events, native_venue_host_signed, native_venue_host_with};
use crate::rust_venues::rust_constructor;
use safe_cex_exchange::{
    client_order_id::ClientOrderLookup,
    exchange::Exchange,
    native::{ProxyTokenSource, native_runtime, native_tealstreet_runtime},
    runtime::{Cleanup, ExchangeError, ListenerCleanup, RemoteSigner},
    websocket::WsRuntime,
};
use safe_cex_store::Store;
use safe_cex_transport::{
    RestAdapter, TransportError, TransportFuture,
    native::reqwest_adapter,
    signing::{RemoteSignerRequest, RemoteSignerResponse},
};
use safe_cex_types::{ExchangeName, ExchangeOptions, PlaceOrderOpts};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
};

pub type JsonFuture<T> = Pin<Box<dyn Future<Output = Result<T, Value>>>>;
/// `sign` answers one remote-signer request (ts: RemoteSigner.sign); `proxy_token`
/// selects the Tealstreet transport (ts: registerTealstreetTransport + accessToken).
pub struct NativeAccountSpec {
    /// Shared by every account on the caller's LocalSet, including hosted ones.
    pub hyperliquid_limiter: Rc<crate::native_host::HyperliquidRateLimiter>,
    pub exchange: String,
    pub options: Value,
    pub account_name: Option<String>,
    pub sign: Option<Rc<dyn Fn(Value) -> JsonFuture<Value>>>,
    pub proxy_token: Option<Rc<dyn Fn() -> JsonFuture<String>>>,
}
pub struct NativeAccount {
    exchange: Rc<dyn Exchange>,
    store: Rc<dyn Store>,
    signer: Option<Rc<HostSigner>>,
    streams: RefCell<BTreeMap<u32, NativeStream>>,
    next_stream: Cell<u32>,
}

struct NativeStream {
    active: Rc<Cell<bool>>,
    cleanup: Option<ListenerCleanup>,
}
impl NativeAccount {
    fn dispose_streams(&self) -> Result<(), ExchangeError> {
        let streams = std::mem::take(&mut *self.streams.borrow_mut());
        // Stop every host callback before invoking any potentially reentrant cleanup.
        for stream in streams.values() {
            stream.active.set(false);
        }
        let mut first_error = None;
        for stream in streams.into_values() {
            if let Some(cleanup) = stream.cleanup
                && let Err(error) = cleanup()
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

type AvailabilityListeners = Rc<RefCell<Vec<(u64, Rc<dyn Fn(bool)>)>>>;
/// ts: the hosted RemoteSigner; the host pushes availability changes.
struct HostSigner {
    sign: Rc<dyn Fn(Value) -> JsonFuture<Value>>,
    available: Cell<bool>,
    listeners: AvailabilityListeners,
    next: Cell<u64>,
}
fn error_text(error: &Value) -> String {
    error["message"]
        .as_str()
        .map_or_else(|| error.to_string(), str::to_owned)
}
impl safe_cex_transport::signing::RemoteSigner for HostSigner {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse> {
        let sign = self.sign.clone();
        Box::pin(async move {
            let wire =
                serde_json::to_value(&request).map_err(|e| TransportError::Host(e.to_string()))?;
            let response = sign(wire)
                .await
                .map_err(|error| TransportError::Host(error_text(&error)))?;
            request.decode_response(&response.to_string())
        })
    }
}
impl RemoteSigner for HostSigner {
    fn is_available(&self) -> bool {
        self.available.get()
    }
    fn on_availability_change(&self, callback: Rc<dyn Fn(bool)>) -> Cleanup {
        let id = self.next.get();
        self.next.set(id + 1);
        self.listeners.borrow_mut().push((id, callback));
        let listeners = Rc::downgrade(&self.listeners);
        Box::new(move || {
            if let Some(listeners) = listeners.upgrade() {
                listeners.borrow_mut().retain(|(row, _)| *row != id);
            }
        })
    }
}

fn exchange_error(error: &ExchangeError) -> Value {
    let mut value = json!({"name": error.name, "message": error.message});
    if let Some(code) = &error.code {
        value["code"] = code.clone();
    }
    value
}
fn host_error(message: impl Into<String>) -> Value {
    json!({"name": "Error", "message": message.into()})
}

/// Build an account through its bound registry Rust constructor.
pub fn open_native_account(spec: NativeAccountSpec) -> Result<Rc<NativeAccount>, Value> {
    let name: ExchangeName = serde_json::from_value(Value::String(spec.exchange.clone()))
        .map_err(|e| host_error(e.to_string()))?;
    let options: ExchangeOptions =
        serde_json::from_value(spec.options).map_err(|e| host_error(e.to_string()))?;
    let runtime: Rc<dyn WsRuntime> = match spec.proxy_token {
        Some(token) => {
            let source: ProxyTokenSource = Rc::new(move || {
                let pending = token();
                Box::pin(async move {
                    pending
                        .await
                        .map_err(|error| TransportError::Host(error_text(&error)))
                })
            });
            native_tealstreet_runtime(source)
        }
        None => native_runtime(),
    };
    let adapters = Rc::new(|| {
        reqwest_adapter()
            .map(|adapter| Arc::new(adapter) as Arc<dyn RestAdapter>)
            .map_err(|e| ExchangeError::new(e.to_string()))
    });
    let signer = spec.sign.map(|sign| {
        Rc::new(HostSigner {
            sign,
            available: Cell::new(true),
            listeners: Rc::default(),
            next: Cell::new(0),
        })
    });
    let host = match &signer {
        Some(signer) => native_venue_host_signed(
            runtime,
            adapters,
            signer.clone() as Rc<dyn RemoteSigner>,
            spec.account_name,
            spec.hyperliquid_limiter,
        ),
        None => native_venue_host_with(runtime, adapters, spec.hyperliquid_limiter),
    };
    let constructor = rust_constructor(&name, host.clone())
        .ok_or_else(|| host_error(format!("no Rust constructor for {}", spec.exchange)))?;
    let store: Rc<dyn Store> = Rc::new(safe_cex_store::store::DefaultStore::new());
    let exchange = constructor
        .construct(name, options, store.clone())
        .map_err(|error| exchange_error(&error))?;
    bind_http_events(&host, &exchange);
    Ok(Rc::new(NativeAccount {
        exchange,
        store,
        signer,
        streams: RefCell::default(),
        next_stream: Cell::new(1),
    }))
}

/// ts: RemoteSigner.onAvailabilityChange, driven from the host's signer session.
pub fn native_account_signer_available(account: &NativeAccount, available: bool) {
    let Some(signer) = &account.signer else {
        return;
    };
    signer.available.set(available);
    let listeners: Vec<_> = signer.listeners.borrow().clone();
    for (_, listener) in listeners {
        listener(available);
    }
}

/// The emitter events a host forwards (ts: the events ExchangeWorker listens to).
const NATIVE_ACCOUNT_EVENTS: &[&str] = &[
    "error",
    "log",
    "warning",
    "fill",
    "update",
    "updatePositions",
    "ws-reconnected",
    "remoteSignerStatus",
    "markets:changed",
    "activeTickerSymbols:changed",
    "grafana-error",
    "grafana-http-request",
    "grafana-http-response",
    "grafana-info",
    "listenOHLCV",
    "listenOHLCVs",
    "listenOrderBook",
];
/// Forwards emitter events and private Store slices until the returned cleanup runs.
pub fn native_account_watch(
    account: &NativeAccount,
    on_event: Rc<dyn Fn(&str, &Value)>,
    on_store: Rc<dyn Fn(Value)>,
) -> Box<dyn Fn()> {
    let base = account.exchange.base();
    let ids: Vec<(&str, String)> = NATIVE_ACCOUNT_EVENTS
        .iter()
        .map(|event| {
            let forward = on_event.clone();
            let id = base.on(
                event,
                Rc::new(move |name: &str, data: &Value| {
                    forward(name, data);
                    Ok(())
                }),
            );
            (*event, id)
        })
        .collect();
    let unsubscribe = account.store.subscribe(Box::new(move |data, _| {
        on_store(json!({
            "latency": data.latency,
            "balance": data.balance,
            "orders": data.orders,
            "positions": data.positions,
            "loaded": data.loaded,
            "updateIds": data.update_ids,
        }));
        Ok(())
    }));
    let exchange = account.exchange.clone();
    Box::new(move || {
        unsubscribe();
        for (event, id) in &ids {
            exchange.base().off(event, Some(id));
        }
    })
}

fn arg<T: DeserializeOwned>(args: &[Value], index: usize) -> Result<T, Value> {
    serde_json::from_value(args.get(index).cloned().unwrap_or(Value::Null))
        .map_err(|e| host_error(format!("Invalid argument {index}: {e}")))
}
fn encode<T: Serialize>(value: T) -> Result<Value, Value> {
    serde_json::to_value(value).map_err(|e| host_error(e.to_string()))
}
fn done<T>(result: Result<T, ExchangeError>) -> Result<T, Value> {
    result.map_err(|error| exchange_error(&error))
}

/// Callback APIs cross the JSON command channel as handles. Each notification
/// retains the venue callback value alongside its registration ID.
fn listen(account: &NativeAccount, method: &'static str, args: &[Value]) -> Result<Value, Value> {
    let id = account.next_stream.get();
    let next = id
        .checked_add(1)
        .ok_or_else(|| host_error("stream handles exhausted"))?;
    let active = Rc::new(Cell::new(true));
    let enabled = active.clone();
    let exchange = Rc::downgrade(&account.exchange);
    let notify = Rc::new(move |data: Value| {
        if enabled.get()
            && let Some(exchange) = exchange.upgrade()
        {
            let base = exchange.base();
            if let Err(error) = base.emit(method, json!({"subscriptionId": id, "data": data})) {
                base.runtime
                    .report_exception(&error, &json!({"method": method}));
            }
        }
    });
    let venue = account.exchange.as_ref();
    let cleanup = match method {
        "listenOHLCV" => venue
            .listen_ohlcv(arg(args, 0)?, Rc::new(move |candle| notify(json!(candle))))
            .map(Some),
        "listenOHLCVs" => {
            venue.listen_ohlcvs(arg(args, 0)?, Rc::new(move |candle| notify(json!(candle))));
            Ok(None)
        }
        "listenOrderBook" => venue
            .listen_order_book(arg(args, 0)?, Rc::new(move |book| notify(json!(book))))
            .map(Some),
        _ => unreachable!("only declared callback methods reach listen"),
    };
    let cleanup = cleanup.map_err(|error| {
        active.set(false);
        exchange_error(&error)
    })?;
    account
        .streams
        .borrow_mut()
        .insert(id, NativeStream { active, cleanup });
    account.next_stream.set(next);
    Ok(json!(id))
}

/// ts: the Exchange methods ExchangeWorker calls, by name; an unknown name rejects.
pub fn native_account_call(
    account: Rc<NativeAccount>,
    method: String,
    args: Vec<Value>,
) -> JsonFuture<Value> {
    Box::pin(async move {
        let venue = account.exchange.as_ref();
        let base = venue.base();
        match method.as_str() {
            "start" => done(venue.start().await).map(|_| Value::Null),
            "dispose" => {
                let streams = account.dispose_streams();
                let disposed = venue.dispose();
                done(streams.and(disposed)).map(|_| Value::Null)
            }
            "listenOHLCV" => listen(&account, "listenOHLCV", &args),
            "listenOHLCVs" => listen(&account, "listenOHLCVs", &args),
            "listenOrderBook" => listen(&account, "listenOrderBook", &args),
            "unlisten" => {
                let id: u32 = arg(&args, 0)?;
                let stream = account.streams.borrow_mut().remove(&id);
                if let Some(stream) = stream {
                    stream.active.set(false);
                    if let Some(cleanup) = stream.cleanup {
                        done(cleanup())?;
                    }
                }
                Ok(Value::Null)
            }
            "disablePublicWebsockets" => {
                base.disable_public_websockets();
                Ok(Value::Null)
            }
            "reconnect" => done(base.reconnect()).map(|_| Value::Null),
            "recoverAfterOffline" => done(venue.recover_after_offline()).and_then(encode),
            "waitForRemoteSigner" => done(base.wait_for_remote_signer().await).and_then(encode),
            "getActiveTickerSymbols" => encode(base.get_active_ticker_symbols()),
            "setActiveTickerSymbols" => {
                let symbols: Vec<String> = arg(&args, 0)?;
                done(base.set_active_ticker_symbols(&symbols)).map(|_| Value::Null)
            }
            "fetchLastLogsFromWebsockets" => encode(base.fetch_last_logs_from_websockets()),
            "validateAccount" => done(venue.validate_account().await).and_then(encode),
            "getAccount" => done(venue.get_account().await).and_then(encode),
            "fetchMarkets" => done(venue.fetch_markets().await).and_then(encode),
            "refreshMarkets" => done(venue.refresh_markets().await).and_then(|changes| {
                encode(json!({"added": changes.added, "removed": changes.removed}))
            }),
            "fetchTickers" => done(venue.fetch_tickers().await).and_then(encode),
            "fetchPositions" => done(venue.fetch_positions(arg(&args, 0)?).await).and_then(encode),
            "fetchOrders" => done(venue.fetch_orders(arg(&args, 0)?).await).and_then(encode),
            "fetchAndSetOrders" => {
                done(venue.fetch_and_set_orders(arg(&args, 0)?).await).and_then(encode)
            }
            "findOrderByClientOrderId" => done(
                venue
                    .find_order_by_client_order_id(arg(&args, 0)?, arg(&args, 1)?)
                    .await,
            )
            .map(|lookup| match lookup {
                ClientOrderLookup::Found(order) => json!({"found": true, "order": order}),
                ClientOrderLookup::NotFound { definitive } => {
                    json!({"found": false, "definitive": definitive})
                }
            }),
            "removeOrders" => {
                // TS accepts Pick<Order, 'id'>[], not an array of bare IDs.
                #[derive(serde::Deserialize)]
                struct OrderId {
                    id: String,
                }
                let orders: Vec<OrderId> = arg(&args, 0)?;
                let ids: Vec<String> = orders.into_iter().map(|order| order.id).collect();
                done(base.remove_orders(&ids)).map(|_| Value::Null)
            }
            "fetchAndSetPrivateSnapshot" => {
                done(venue.fetch_and_set_private_snapshot(1).await).map(|_| Value::Null)
            }
            "placeOrder" => {
                let order: PlaceOrderOpts = arg(&args, 0)?;
                done(base.place_order(venue, order).await).and_then(encode)
            }
            "placeOrders" => {
                let orders: Vec<PlaceOrderOpts> = arg(&args, 0)?;
                done(base.place_orders(venue, orders).await).and_then(encode)
            }
            "updateOrder" => done(venue.update_order(arg(&args, 0)?).await).and_then(encode),
            "cancelOrders" => done(venue.cancel_orders(arg(&args, 0)?).await).map(|_| Value::Null),
            "cancelSymbolOrders" => {
                done(venue.cancel_symbol_orders(arg(&args, 0)?).await).map(|_| Value::Null)
            }
            "cancelAllOrders" => done(venue.cancel_all_orders().await).map(|_| Value::Null),
            "mintClientOrderId" => encode(base.mint_client_order_id(&arg(&args, 0)?)),
            "placeOrderWithClientOrderId" => {
                let order: PlaceOrderOpts = arg(&args, 0)?;
                done(base.place_order_with_client_order_id(venue, order).await).map(|placed| {
                    json!({"orderIds": placed.order_ids, "clientOrderId": placed.client_order_id})
                })
            }
            "fetchOptionChain" => {
                done(venue.fetch_option_chain(arg(&args, 0)?).await).and_then(encode)
            }
            "fetchOptionExpiries" => {
                done(venue.fetch_option_expiries(arg(&args, 0)?).await).and_then(encode)
            }
            "fetchAccountGreeks" => done(venue.fetch_account_greeks().await).and_then(encode),
            "nuke" => done(venue.nuke(arg::<Option<u32>>(&args, 0)?.unwrap_or(0)).await)
                .map(|_| Value::Null),
            "nukePositions" => done(
                venue
                    .nuke_positions(arg::<Option<u32>>(&args, 0)?.unwrap_or(0))
                    .await,
            )
            .map(|_| Value::Null),
            "nukeOrders" => done(base.nuke_orders(venue).await).map(|_| Value::Null),
            "setLeverage" => done(
                venue
                    .set_leverage(arg(&args, 0)?, arg(&args, 1)?, arg(&args, 2)?)
                    .await,
            )
            .map(|_| Value::Null),
            "setAllLeverage" => {
                done(venue.set_all_leverage(arg(&args, 0)?).await).map(|_| Value::Null)
            }
            "changePositionMode" => done(
                venue
                    .change_position_mode(arg(&args, 0)?, arg(&args, 1)?)
                    .await,
            )
            .map(|_| Value::Null),
            "changeMarginType" => done(
                venue
                    .change_margin_type(arg(&args, 0)?, arg(&args, 1)?)
                    .await,
            )
            .map(|_| Value::Null),
            "fetchLeverageForSymbol" => {
                done(venue.fetch_leverage_for_symbol(arg(&args, 0)?).await).and_then(encode)
            }
            "fetchSymbolMaxLeverage" => {
                done(venue.fetch_symbol_max_leverage(arg(&args, 0)?).await).and_then(encode)
            }
            "scheduleCancel" => {
                done(venue.schedule_cancel(arg(&args, 0)?).await).map(|_| Value::Null)
            }
            "fetchOHLCV" => done(venue.fetch_ohlcv(arg(&args, 0)?).await).and_then(encode),
            "fetchPublicTrades" => done(
                venue
                    .fetch_public_trades(arg(&args, 0)?, arg(&args, 1)?)
                    .await,
            )
            .and_then(encode),
            "fetchHistoricalPositions" => {
                #[derive(serde::Deserialize)]
                struct Options {
                    account: String,
                    from: Option<f64>,
                    to: Option<f64>,
                }
                let options: Options = arg(&args, 0)?;
                done(
                    venue
                        .fetch_historical_positions(
                            safe_cex_exchange::journal::HistoricalPositionOptions {
                                account: options.account,
                                from: options.from,
                                to: options.to,
                            },
                        )
                        .await,
                )
                .and_then(encode)
            }
            "fetchHistoricalFills" => {
                done(venue.fetch_historical_fills(arg(&args, 0)?).await).and_then(encode)
            }
            "fetchIncomeHistory" => {
                done(venue.fetch_income_history(arg(&args, 0)?).await).and_then(encode)
            }
            "fetchIncomeHistoryRange" => done(
                venue
                    .fetch_income_history_range(arg(&args, 0)?, arg(&args, 1)?, arg(&args, 2)?)
                    .await,
            )
            .and_then(encode),
            "fetchClosedPnl" => done(venue.fetch_closed_pnl(arg(&args, 0)?).await).and_then(encode),
            "fetchHistoricalOrders" => {
                done(venue.fetch_historical_orders(arg(&args, 0)?).await).and_then(encode)
            }
            "updateSymbolMetadata" => {
                done(venue.update_symbol_metadata(arg(&args, 0)?).await).map(|_| Value::Null)
            }
            other => Err(host_error(format!("Unknown method {other}"))),
        }
    })
}

#[cfg(test)]
mod model_codec_tests {
    use super::encode;
    use safe_cex_exchange::js_runtime::value::{Js, from_json, to_typed};
    use safe_cex_types::{Order, Position};
    use serde_json::Value;
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    fn account_encoder_round_trips_model_presence_through_the_napi_json_surface() {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../safe-cex-types/tests/support/model-field-corpus.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(cases.len(), 126);
        let mut wire = Vec::new();
        for case in &cases {
            let input: Value = serde_json::from_str(case["input"].as_str().unwrap()).unwrap();
            let native = from_json(&input);
            for (field, state) in case["fields"]
                .as_array()
                .unwrap()
                .iter()
                .zip(case["states"].as_array().unwrap())
            {
                if state == 1 {
                    native
                        .define(field.as_str().unwrap(), Js::Undefined)
                        .unwrap();
                }
            }
            let encoded = if case["model"] == "order" {
                encode(to_typed::<Order>(&native).unwrap()).unwrap()
            } else {
                encode(to_typed::<Position>(&native).unwrap()).unwrap()
            };
            // napi native_exchange forwards this exact Value.to_string(). The
            // JS consumer parses that text before exposing SDK model objects.
            wire.push(encoded.to_string());
        }
        let mut node = Command::new("node").args(["-e", "let text='';process.stdin.on('data',x=>text+=x).on('end',()=>process.stdout.write(JSON.stringify(JSON.parse(text).map(x=>JSON.stringify(JSON.parse(x))))));"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        node.stdin
            .take()
            .unwrap()
            .write_all(serde_json::to_string(&wire).unwrap().as_bytes())
            .unwrap();
        let output = node.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(actual.len(), 126);
        for (actual, case) in actual.iter().zip(&cases) {
            assert_eq!(
                actual,
                case["wire"].as_str().unwrap(),
                "account/napi field-presence bytes {case}"
            );
        }
    }
}
