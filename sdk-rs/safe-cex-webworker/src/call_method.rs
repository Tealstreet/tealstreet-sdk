use crate::handler::BaseHandler;
use crate::payload::StoreWorkerOp;
use crate::payload::StoreWorkerPayload;
use crate::{
    data_post::{WorkerDataPayload, WorkerDataPost},
    error_post::WorkerErrorPost,
};
use safe_cex_exchange::throttle::ThrottleManager;
use safe_cex_exchange::throttle::symbol_metadata_throttler;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::value::{Js, from_json, from_typed},
    runtime::{ExchangeError, ExchangeFuture},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{future::Future, pin::Pin, rc::Rc};

pub struct CallMethodHandler {
    base: Rc<BaseHandler>,
    exchange: Rc<dyn Exchange>,
    post: WorkerDataPost,
    symbol_metadata_throttler: Rc<ThrottleManager<String>>,
}
struct ReplyGuard {
    post: WorkerDataPost,
    request_id: String,
    sent: bool,
    runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>,
}
impl ReplyGuard {
    fn send(&mut self, success: bool, result: Option<Js>) -> Result<(), ExchangeError> {
        let data = Js::object(vec![
            ("requestId", Js::str(&self.request_id)),
            ("success", Js::Bool(success)),
        ]);
        if let Some(result) = result {
            data.define("result", result)
                .map_err(|e| e.to_exchange_error())?;
        }
        self.sent = true;
        self.post.send(WorkerDataPayload {
            op: StoreWorkerOp::CallMethodDone,
            data,
        })
    }
}
impl Drop for ReplyGuard {
    fn drop(&mut self) {
        // Drop settles exactly once, including when the host discards an unpolled task.
        if !self.sent
            && let Err(error) = self.send(
                false,
                Some(Js::object(vec![
                    ("name", Js::str("Error")),
                    ("message", Js::str("Worker request cancelled")),
                ])),
            )
        {
            self.runtime
                .report_exception(&error, &json!({"operation":"worker cancelled reply"}));
        }
    }
}
fn arg<T: DeserializeOwned>(args: &[Value], index: usize) -> Result<T, ExchangeError> {
    serde_json::from_value(args.get(index).cloned().unwrap_or(Value::Null))
        .map_err(|e| ExchangeError::new(format!("Invalid worker argument {index}: {e}")))
}
fn encode<T: Serialize>(result: T) -> Result<Option<Js>, ExchangeError> {
    from_typed(&result)
        .map(Some)
        .map_err(|e| e.to_exchange_error())
}
impl CallMethodHandler {
    pub fn new(
        exchange: Rc<dyn Exchange>,
        post: WorkerDataPost,
        error_post: WorkerErrorPost,
    ) -> Rc<Self> {
        let throttler = symbol_metadata_throttler(exchange.clone());
        let base = BaseHandler::new(
            exchange.clone(),
            "CallMethodHandler".into(),
            error_post,
            Rc::new(|_| {}),
        );
        Rc::new(Self {
            base,
            exchange,
            post,
            symbol_metadata_throttler: throttler,
        })
    }
    pub fn handle_call_method(
        self: &Rc<Self>,
        request_id: String,
        method: String,
        args: Vec<Value>,
    ) -> Pin<Box<dyn Future<Output = ()>>> {
        let mut reply = ReplyGuard {
            post: self.post.clone(),
            request_id,
            sent: false,
            runtime: self.exchange.base().runtime.clone(),
        };
        let handler = self.clone();
        Box::pin(async move {
            let known = matches!(
                method.as_str(),
                "setLeverage"
                    | "scheduleCancel"
                    | "updateSymbolMetadata"
                    | "changeMarginType"
                    | "changePositionMode"
                    | "fetchLastLogsFromWebsockets"
                    | "fetchAndSetPrivateSnapshot"
                    | "reconnect"
                    // Source recovery is a real command, delegated to the shared exchange hook.
                    | "recoverAfterOffline"
                    | "fetchOHLCV"
                    | "fetchHistoricalFills"
                    | "fetchOrders"
                    | "fetchAndSetOrders"
                    | "removeOrders"
                    | "fetchHistoricalOrders"
                    | "fetchClosedPnl"
                    | "fetchIncomeHistory"
                    | "fetchIncomeHistoryRange"
            );
            let outcome = if !known {
                handler
                    .exchange
                    .base()
                    .runtime
                    .console("error", &format!("Unknown method {method}"));
                reply.send(false, None)
            } else {
                handler
                    .execute(&method, &args)
                    .await
                    .and_then(|result| reply.send(true, Some(result.unwrap_or(Js::Undefined))))
            };
            match outcome {
                Ok(()) => {}
                Err(error) => {
                    handler
                        .exchange
                        .base()
                        .runtime
                        .console("error", &format!("Error calling method: {error}"));
                    let serialized = Js::object(vec![
                        ("message", Js::str(&error.message)),
                        ("name", Js::str(&error.name)),
                        (
                            "stack",
                            error.stack.as_deref().map_or(Js::Undefined, Js::str),
                        ),
                    ]);
                    if let Some(code) = &error.code {
                        serialized
                            .define("code", from_json(code))
                            .expect("fresh error data");
                    }
                    if let Err(error) = reply.send(false, Some(serialized)) {
                        handler
                            .exchange
                            .base()
                            .runtime
                            .report_exception(&error, &json!({"operation":"worker error reply"}));
                    }
                }
            }
        })
    }
    fn execute<'a>(&'a self, method: &'a str, args: &'a [Value]) -> ExchangeFuture<'a, Option<Js>> {
        Box::pin(async move {
            let venue = self.exchange.as_ref();
            match method {
                "setLeverage" => {
                    venue
                        .set_leverage(arg(args, 0)?, arg(args, 1)?, arg(args, 2)?)
                        .await?;
                    Ok(None)
                }
                "scheduleCancel" => {
                    let time = args
                        .first()
                        .map(|value| serde_json::from_value(value.clone()))
                        .transpose()
                        .map_err(|e| ExchangeError::new(e.to_string()))?;
                    venue.schedule_cancel(time).await?;
                    Ok(None)
                }
                "updateSymbolMetadata" => {
                    self.symbol_metadata_throttler.call(arg(args, 0)?);
                    Ok(Some(Js::Null))
                }
                "changeMarginType" => {
                    venue
                        .change_margin_type(arg(args, 0)?, arg(args, 1)?)
                        .await?;
                    Ok(None)
                }
                "changePositionMode" => {
                    venue
                        .change_position_mode(arg(args, 0)?, arg(args, 1)?)
                        .await?;
                    Ok(None)
                }
                "fetchLastLogsFromWebsockets" => {
                    encode(venue.base().fetch_last_logs_from_websockets())
                }
                "fetchAndSetPrivateSnapshot" => {
                    venue.fetch_and_set_private_snapshot(1).await?;
                    Ok(None)
                }
                "reconnect" => {
                    venue.base().reconnect()?;
                    Ok(Some(Js::Null))
                }
                "recoverAfterOffline" => Ok(Some(Js::Bool(venue.recover_after_offline()?))),
                "fetchOHLCV" => encode(venue.fetch_ohlcv(arg(args, 0)?).await?),
                "fetchHistoricalFills" => {
                    encode(venue.fetch_historical_fills(arg(args, 0)?).await?)
                }
                "fetchOrders" => encode(venue.fetch_orders(arg(args, 0)?).await?),
                "fetchAndSetOrders" => encode(venue.fetch_and_set_orders(arg(args, 0)?).await?),
                "removeOrders" => {
                    let orders: Vec<Value> = arg(args, 0)?;
                    let ids: Result<Vec<_>, _> = orders
                        .iter()
                        .map(|order| {
                            order
                                .get("id")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                                .ok_or_else(|| ExchangeError::new("Invalid worker order id"))
                        })
                        .collect();
                    venue.base().remove_orders(&ids?)?;
                    Ok(Some(Js::Null))
                }
                "fetchHistoricalOrders" => {
                    encode(venue.fetch_historical_orders(arg(args, 0)?).await?)
                }
                "fetchClosedPnl" => encode(venue.fetch_closed_pnl(arg(args, 0)?).await?),
                "fetchIncomeHistory" => encode(venue.fetch_income_history(arg(args, 0)?).await?),
                "fetchIncomeHistoryRange" => encode(
                    venue
                        .fetch_income_history_range(arg(args, 0)?, arg(args, 1)?, arg(args, 2)?)
                        .await?,
                ),
                _ => unreachable!("handle_call_method checked the dispatch set"),
            }
        })
    }
    pub fn handle(self: &Rc<Self>, payload: StoreWorkerPayload) {
        if payload.op != StoreWorkerOp::CallMethod {
            return;
        }
        let data = payload.data.unwrap_or(Value::Null);
        let request_id = data["requestId"].as_str().unwrap_or("").to_owned();
        let method = data["method"].as_str().unwrap_or("").to_owned();
        let args = data["args"].as_array().cloned().unwrap_or_default();
        let task = self.handle_call_method(request_id, method, args);
        self.exchange.base().runtime.spawn_local(task);
    }
    pub fn dispose(&self) {
        self.symbol_metadata_throttler.clear();
        self.base.dispose();
    }
}
