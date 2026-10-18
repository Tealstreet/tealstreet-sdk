//! Concrete MEXC socket construction and private protocol dispatch.
use crate::{
    exchange::MexcExchange,
    ws_private::{MexcPrivateHandlers, ping_private, subscribe_private_feeds},
};
use safe_cex_exchange::{
    OwnedOperation,
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{
        BaseWebSocket, WsDiagnosticReceiveOptions, WsDiagnosticSendOptions, WsProtocol, WsRuntime,
    },
    ws_parent::WsExchangeParent,
};
use safe_cex_transport::{relay::WebSocketConnectionOptions, socket::SocketFrame};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub struct MexcSocketDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub current_time_ms: Rc<dyn Fn() -> f64>,
    pub remote_account_name: Option<String>,
    pub emit_fill: Rc<dyn Fn(crate::ws_private::MexcFillEvent) -> Result<(), ExchangeError>>,
}

pub(crate) struct PrivateProtocol {
    parent: Weak<MexcExchange>,
    current_time_ms: Rc<dyn Fn() -> f64>,
    handlers: MexcPrivateHandlers,
    remote_account_name: Option<String>,
    heartbeat: RefCell<Option<OwnedOperation>>,
    balance_timeout: RefCell<Option<OwnedOperation>>,
}
impl PrivateProtocol {
    pub(crate) fn bind(
        parent: &Rc<MexcExchange>,
        runtime: Rc<dyn WsRuntime>,
        handlers: MexcPrivateHandlers,
        remote_account_name: Option<String>,
        current_time_ms: Rc<dyn Fn() -> f64>,
    ) -> Rc<BaseWebSocket> {
        let protocol = Rc::new(Self {
            parent: Rc::downgrade(parent),
            current_time_ms,
            handlers,
            remote_account_name,
            heartbeat: RefCell::default(),
            balance_timeout: RefCell::default(),
        });
        let erased: Rc<dyn Exchange> = parent.clone();
        let socket =
            BaseWebSocket::new(2, WsExchangeParent::new(&erased), runtime, protocol.clone());
        socket.treat_any_message_as_pong.set(true);
        let weak = Rc::downgrade(&protocol);
        socket.set_before_close(Rc::new(move || {
            if let Some(protocol) = weak.upgrade() {
                protocol.stop_heartbeat();
            }
        }));
        socket
    }
    fn stop_heartbeat(&self) {
        let previous = self.heartbeat.borrow_mut().take();
        if let Some(operation) = previous {
            operation.cancel();
        }
    }
    fn start_heartbeat(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        self.stop_heartbeat();
        ping_private(socket)?;
        let weak = Rc::downgrade(socket);
        let operation =
            socket.start_interval(crate::types::MEXC_WS_HEARTBEAT_INTERVAL_MS, move || {
                if let Some(socket) = weak.upgrade() {
                    if !socket.is_disposed() && socket.is_connected() {
                        ping_private(&socket)?;
                    }
                }
                Ok(())
            })?;
        *self.heartbeat.borrow_mut() = Some(operation);
        Ok(())
    }
    fn send_login(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        let Some(parent) = self.parent.upgrade() else {
            return Err(ExchangeError::new("MEXC private parent was dropped"));
        };
        let options = parent.base().options();
        let api_key = safe_cex_num::js_trim(&options.key).to_owned();
        let request_time = safe_cex_num::js_number_string((self.current_time_ms)());
        let signature = parent.login_signature(
            api_key.clone(),
            request_time.clone(),
            self.remote_account_name.clone(),
        );
        let socket = socket.clone();
        let owner = parent.clone();
        parent.base().start_owned_operation(async move {
            let result = async {
                let Some(signature) = reaction(signature).await? else {return Ok(());};
                let text = serde_json::to_string(&json!({"method":"login", "param":{"apiKey":api_key,"reqTime":request_time,"signature":signature}})).map_err(|e| ExchangeError::new(e.to_string()))?;
                socket.send(&text)?;
                socket.emit_ws_diagnostic_send(&text, &WsDiagnosticSendOptions {format:Some(Rc::new(|message| {
                    let mut value: Value = serde_json::from_str(message).ok()?;
                    value["param"]["apiKey"] = json!("[redacted]");
                    value["param"]["signature"] = json!("[redacted]");
                    Some(format!("[mexc private] {value}"))
                })), redact:None})?;
                Ok::<_, ExchangeError>(())
            }.await;
            if let Err(error) = result {owner.base().runtime.report_exception(&error, &json!({"operation":"MEXC private sendLogin"}));}
        });
        Ok(())
    }
    fn refresh_balance(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        let previous = self.balance_timeout.borrow_mut().take();
        if let Some(operation) = previous {
            operation.cancel();
        }
        let weak = self.parent.clone();
        let weak_socket = Rc::downgrade(socket);
        let operation = socket.start_timeout(1000, move || {
            let Some(parent) = weak.upgrade() else {
                return Ok(());
            };
            let owner = parent.clone();
            let weak_socket = weak_socket.clone();
            parent.base().start_owned_operation(async move {
                let _ = async {
                    let balance = reaction(owner.fetch_balance()).await?;
                    if weak_socket
                        .upgrade()
                        .is_some_and(|socket| !socket.is_disposed())
                    {
                        owner.base().store.update(
                            safe_cex_store::StoreUpdate {
                                balance: Some(balance),
                                ..Default::default()
                            },
                            false,
                        )?;
                    }
                    Ok::<_, ExchangeError>(())
                }
                .await;
            });
            Ok(())
        })?;
        *self.balance_timeout.borrow_mut() = Some(operation);
        Ok(())
    }
}
impl WsProtocol for PrivateProtocol {
    fn dispose(&self) {
        self.stop_heartbeat();
        let previous = self.balance_timeout.borrow_mut().take();
        if let Some(operation) = previous {
            operation.cancel();
        }
    }
    fn ping(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        ping_private(socket)
    }
    fn connect_and_subscribe(
        &self,
        socket: Rc<BaseWebSocket>,
        _: bool,
    ) -> Result<(), ExchangeError> {
        let parent = self
            .parent
            .upgrade()
            .ok_or_else(|| ExchangeError::new("MEXC private parent was dropped"))?;
        if socket.is_disposed()
            || parent.is_public_data_only()
            || parent.base().options().key.is_empty()
        {
            return Ok(());
        }
        let options = parent.base().options();
        let network = if options.testnet.unwrap_or(false) {
            "testnet"
        } else {
            "livenet"
        };
        let url = crate::types::BASE_WS_URL["private"][network]
            .as_str()
            .expect("MEXC private URL")
            .to_owned();
        socket.start_websocket(
            url,
            vec![],
            Some(WebSocketConnectionOptions {
                headers: Some(safe_cex_exchange::websocket::UpgradeHeaders::Static(
                    indexmap::IndexMap::from([("User-Agent".into(), "Tealstreet".into())]),
                )),
                ..Default::default()
            }),
        );
        Ok(())
    }
    fn on_open(&self, socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = (|| {
            if socket.is_disposed() {
                return Ok(());
            }
            self.start_heartbeat(&socket)?;
            self.send_login(&socket)?;
            let parent = self
                .parent
                .upgrade()
                .ok_or_else(|| ExchangeError::new("MEXC private parent was dropped"))?;
            let owner = parent.clone();
            parent.base().start_owned_operation(async move {
                if let Err(error) = owner.fetch_and_set_private_snapshot(10).await {
                    owner
                        .base()
                        .runtime
                        .report_exception(&error, &json!({"operation":"MEXC private snapshot"}));
                }
            });
            socket.emit_ws_reconnected()
        })();
        Box::pin(async move { result })
    }
    fn on_message(
        &self,
        socket: &Rc<BaseWebSocket>,
        frame: &SocketFrame,
    ) -> Result<(), ExchangeError> {
        if socket.is_disposed() {
            return Ok(());
        }
        socket.track_last_received()?;
        let raw = match frame {
            SocketFrame::Text(text) => text.clone(),
            SocketFrame::Binary(bytes) => String::from_utf8_lossy(bytes).into_owned(),
            SocketFrame::Close { .. } => return Ok(()),
        };
        let value = safe_cex_transport::how::big_int_as_string_json_parse(&raw)
            .into_value()
            .map_err(|error| ExchangeError::new(error.to_string()))?;
        if !safe_cex_num::js_truthy(Some(&value)) {
            return Ok(());
        }
        socket.emit_ws_diagnostic_receive(
            &Value::String(raw),
            &WsDiagnosticReceiveOptions {
                allow_channel: None,
                format: Some(Rc::new(|message| {
                    Some(format!(
                        "[mexc private] {}",
                        message.as_str().unwrap_or_default()
                    ))
                })),
                redact: None,
            },
        )?;
        if ["channel", "msg", "method"]
            .into_iter()
            .any(|key| value.get(key).and_then(Value::as_str) == Some("pong"))
        {
            return socket.handle_pong_event(15_000);
        }
        if value.get("channel").and_then(Value::as_str) == Some("rs.login") {
            if value.get("data") == Some(&json!("success"))
                || value.get("success") == Some(&json!(true))
            {
                subscribe_private_feeds(socket)?;
                receive(socket, "[mexc login] success")?;
            } else {
                let diagnostic = safe_cex_num::js_json_stringify(&value.clone().into())
                    .map_err(ExchangeError::from)?
                    .expect("a parsed JSON frame is a defined value");
                receive(socket, &format!("[mexc login] failed {diagnostic}"))?;
                socket.emitter().emit_and_log_error(
                    "MEXC private ws login failed, double check your key and secret",
                    None,
                    None,
                )?;
            }
            return Ok(());
        }
        if value.get("success") == Some(&json!(false)) {
            let message = value
                .get("message")
                .filter(|v| safe_cex_num::js_truthy(Some(v)))
                .or_else(|| {
                    value
                        .get("msg")
                        .filter(|v| safe_cex_num::js_truthy(Some(v)))
                });
            let detail = safe_cex_num::js_string(message.or(Some(&json!("unknown error"))))
                .map_err(crate::exchange_errors::coercion)?;
            return socket.emitter().log(
                &format!("MEXC private WS request failed: {detail}"),
                safe_cex_exchange::runtime::LogSeverity::Warning,
            );
        }
        let channel_value = value
            .get("channel")
            .filter(|v| safe_cex_num::js_truthy(Some(v)));
        let channel = safe_cex_num::js_string(channel_value.or(Some(&json!(""))))
            .map_err(crate::exchange_errors::coercion)?;
        if channel.is_empty() {
            return Ok(());
        }
        if !crate::types::is_mexc_control_channel(&channel) {
            socket.record_message_handler_stats(&channel)?;
        }
        let store = socket.store();
        match channel.as_str() {
            "push.personal.order" => self.handlers.handle_order(store.as_ref(), &value),
            "push.personal.order.deal" => self.handlers.handle_fill(store.as_ref(), &value),
            "push.personal.position" => self.handlers.handle_position(store.as_ref(), &value),
            "push.personal.asset" => self.refresh_balance(socket),
            "push.personal.plan.order" | "push.personal.trigger.order" => {
                self.handlers.handle_plan_order(store.as_ref(), &value)
            }
            "push.personal.stop.order" | "push.personal.stop.planorder" => {
                self.handlers.handle_stop_order(store.as_ref(), &value)
            }
            _ => Ok(()),
        }
    }
}

fn receive(socket: &Rc<BaseWebSocket>, message: &str) -> Result<(), ExchangeError> {
    socket.emit_ws_diagnostic_receive(
        &json!(message),
        &WsDiagnosticReceiveOptions {
            allow_channel: None,
            format: Some(Rc::new(|message| {
                Some(format!(
                    "[mexc private] {}",
                    message.as_str().unwrap_or_default()
                ))
            })),
            redact: None,
        },
    )
}
pub(crate) fn reaction<F: std::future::Future>(
    future: F,
) -> impl std::future::Future<Output = F::Output> {
    let mut future = Box::pin(future);
    let mut yielded = false;
    std::future::poll_fn(move |cx| {
        if !yielded {
            yielded = true;
            cx.waker().wake_by_ref();
            return std::task::Poll::Pending;
        }
        future.as_mut().poll(cx)
    })
}
